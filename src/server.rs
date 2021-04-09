use anyhow::Result;
use slab::Slab;
use std::sync::Arc;
use std::{convert::TryInto, future::Future, net::SocketAddr};
use tokio::{
    io::split,
    net::{TcpListener, UdpSocket},
    sync::mpsc::{
        unbounded_channel as tokio_channel, UnboundedReceiver as TokioReceiver,
        UnboundedSender as TokioSender,
    },
    sync::RwLock,
};

#[cfg(feature = "tls")]
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};

use crate::{utils::read_frame, Delivery};
use crate::{Config, Connection, Event};

pub type ConnectionId = u32;

pub struct Server;

impl Server {
    pub fn listen(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        config: Config,
        #[cfg(feature = "tls")] server_config: ServerConfig,
    ) -> (
        ServerSender,
        ServerReceiver,
        impl Future<Output = Result<(), anyhow::Error>>,
    ) {
        let (outbound_sender, outbound_receiver) =
            tokio_channel::<(ConnectionId, Vec<u8>, Delivery)>();
        let (inbound_sender, inbound_receiver) =
            async_channel::bounded::<(ConnectionId, Event)>(config.event_capacity);

        let task = Self::task(
            tcp_address,
            udp_address,
            config,
            inbound_sender,
            outbound_receiver,
            #[cfg(feature = "tls")]
            server_config,
        );

        (
            ServerSender {
                sender: outbound_sender,
            },
            ServerReceiver {
                receiver: inbound_receiver,
            },
            task,
        )
    }

    async fn task(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        _config: Config,
        inbound_sender: async_channel::Sender<(ConnectionId, Event)>,
        mut outbound_receiver: TokioReceiver<(ConnectionId, Vec<u8>, Delivery)>,
        #[cfg(feature = "tls")] server_config: ServerConfig,
    ) -> Result<()> {
        let socket = UdpSocket::bind(udp_address).await?;
        let mut socket_buffer: [u8; 1200] = [0; 1200];

        #[cfg(feature = "tls")]
        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let listener = TcpListener::bind(tcp_address).await?;

        let connections = Arc::new(RwLock::new(Slab::new()));

        loop {
            tokio::select! {
                result = listener.accept() => {
                    if let Ok((stream, address)) = result {
                        log::debug!("Accepting connection: {}", address);

                        let _ = stream.set_nodelay(true);

                        #[cfg(feature = "tls")]
                        let (read_stream, write_stream) = {
                            let acceptor = acceptor.clone();
                            let stream = acceptor.accept(stream).await?;
                            split(stream)
                        };

                        #[cfg(not(feature = "tls"))]
                        let (read_stream, write_stream) = split(stream);

                        let id = {
                            let mut connections = connections.write().await;

                            let entry = connections.vacant_entry();

                            let id = entry.key() as u32;
                            let connection = Connection::accept(id, write_stream).await.unwrap();

                            entry.insert(connection);

                            id
                        };

                        let connections = connections.clone();
                        let inbound_sender = inbound_sender.clone();
                        tokio::spawn(async move {
                            let mut read_stream = read_stream;
                            loop {
                                match read_frame(&mut read_stream, 500000).await {
                                    Ok(data) => {
                                        inbound_sender.send((id, Event::Received {
                                            data
                                        })).await.unwrap();
                                    },
                                    Err(err) => {
                                        log::debug!("Read frame error: {:#?}", err);
                                        log::debug!("Disconnecting client {}.", id);
                                        {
                                            let mut connections = connections.write().await;
                                            connections.remove(id as usize);
                                        }
                                        inbound_sender.send((id, Event::Disconnected)).await.unwrap();
                                        break;
                                    }
                                }
                            }
                        });
                    }
                },
                result = socket.recv_from(&mut socket_buffer) => {
                    if let Ok((bytes_read, address)) = result {
                        // Must receive more than tag (u64) bytes + id (u32) + mode (u8)
                        if bytes_read >= 14 {
                            let id = socket_buffer[8..12].try_into().map(|bytes| u32::from_be_bytes(bytes));
                            let connections = connections.read().await;
                            let result = id.ok().and_then(|id| connections.get(id as usize).map(|c| (id, c)));
                            if let Some((id, connection)) = result {

                                let tag = &socket_buffer[0..8];
                                let mode = socket_buffer[12];
                                let data = &socket_buffer[13..bytes_read];

                                let mut udp_address = connection.udp_address.lock().await;
                                if mode == 0 && udp_address.map(|addr| addr == address).unwrap_or(false) && connection.verify(data, tag) {
                                    // Verified sender, create event:
                                    inbound_sender.send((id, Event::Received {
                                        data: data.to_vec()
                                    })).await.unwrap();
                                } else if mode == 1 && udp_address.is_none() && data == b"ACK" && connection.verify(data, tag) {
                                    // Handshake - Received UDP, respond with ACK (3):
                                    *udp_address = Some(address);
                                    connection.write(b"ACK").await.unwrap(); // TODO: handle possible error?
                                }
                            }
                        }
                    }
                },
                result = outbound_receiver.recv() => {
                    if let Some((id, mut data, delivery)) = result {
                        let connections = connections.read().await;
                        if let Some(connection) = connections.get(id as usize) {
                            match delivery {
                                Delivery::Reliable => {
                                    match connection.write(&data).await {
                                        Ok(()) => {},
                                        Err(err) => log::debug!("Error writing message (TCP): {}", err)
                                    }
                                },
                                Delivery::Unreliable => {
                                    let udp_address = connection.udp_address.lock().await;
                                    if let Some(address) = *udp_address {
                                        let tag = connection.sign(&data);

                                        let mut bytes = tag.to_vec();
                                        bytes.append(&mut data);

                                        match socket.send_to(&bytes, address).await {
                                            Ok(_) => {},
                                            Err(err) => log::debug!("Error writing message (UDP): {}", err)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerSender {
    sender: TokioSender<(ConnectionId, Vec<u8>, Delivery)>,
}

impl ServerSender {
    pub fn send(&self, id: ConnectionId, data: Vec<u8>, delivery: Delivery) -> Result<()> {
        self.sender
            .send((id, data, delivery))
            .map_err(|err| err.into())
    }

    pub fn reliable(&self, id: ConnectionId, data: Vec<u8>) -> Result<()> {
        self.send(id, data, Delivery::Reliable)
    }

    pub fn unreliable(&self, id: ConnectionId, data: Vec<u8>) -> Result<()> {
        self.send(id, data, Delivery::Unreliable)
    }
}

#[derive(Debug, Clone)]
pub struct ServerReceiver {
    receiver: async_channel::Receiver<(ConnectionId, Event)>,
}

impl ServerReceiver {
    pub async fn recv(&self) -> Result<(ConnectionId, Event)> {
        self.receiver.recv().await.map_err(|err| err.into())
    }

    pub fn try_recv(&self) -> Result<(ConnectionId, Event)> {
        self.receiver.try_recv().map_err(|err| err.into())
    }
}
