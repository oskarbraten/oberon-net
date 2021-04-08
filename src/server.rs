use anyhow::Result;
use slab::Slab;
use std::{convert::TryInto, future::Future, net::SocketAddr, thread};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::mpsc::{
        unbounded_channel as tokio_channel, UnboundedReceiver as TokioReceiver,
        UnboundedSender as TokioSender,
    },
};

use crate::utils::{read_frame, sign, verify};
use crate::{Config, Connection, Event, Message};

pub type ConnectionId = u32;

pub struct Server;

impl Server {
    pub fn listen_with_runtime(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        config: Config,
        worker_threads: usize,
    ) -> (ServerSender, ServerReceiver) {
        let (sender, receiver, task) = Self::listen(tcp_address, udp_address, config);

        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(worker_threads)
                .enable_all()
                .build()
                .unwrap();

            runtime.block_on(task).unwrap();
        });

        (sender, receiver)
    }

    pub fn listen(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        config: Config,
    ) -> (
        ServerSender,
        ServerReceiver,
        impl Future<Output = Result<(), anyhow::Error>>,
    ) {
        let (outbound_sender, outbound_receiver) = tokio_channel::<(ConnectionId, Message)>();
        let (inbound_sender, inbound_receiver) =
            async_channel::bounded::<(ConnectionId, Event)>(config.event_capacity);

        let task = Self::task(
            tcp_address,
            udp_address,
            config,
            inbound_sender,
            outbound_receiver,
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
        inbound_sender: async_channel::Sender<(u32, Event)>,
        mut outbound_receiver: TokioReceiver<(u32, Message)>,
    ) -> Result<()> {
        let socket = UdpSocket::bind(udp_address).await?;
        let mut socket_buffer: [u8; 1200] = [0; 1200];

        let listener = TcpListener::bind(tcp_address).await?;
        let mut connections: Slab<Connection> = Slab::new();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    if let Ok((stream, address)) = result {
                        println!("Accepting connection: {}", address);

                        let _ = stream.set_nodelay(true);
                        let (read_stream, write_stream) = stream.into_split();

                        let entry = connections.vacant_entry();

                        let id = entry.key() as u32;
                        let connection = Connection::accept(id, write_stream).await.unwrap();

                        entry.insert(connection);

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
                                        println!("Read frame error: {:#?}", err);
                                        // TODO: disconnect? event?
                                        break;
                                    }
                                }
                            }
                        });
                    }
                },
                result = socket.recv_from(&mut socket_buffer) => {
                    if let Ok((bytes_read, address)) = result {
                        // Must receive more than id (u32) + tag (u64) bytes
                        if bytes_read > 12 {
                            let id = socket_buffer[0..4].try_into().map(|bytes| u32::from_be_bytes(bytes));
                            let result = id.ok().and_then(|id| connections.get_mut(id as usize).map(|c| (id, c)));
                            if let Some((id, connection)) = result {
                                let tag = &socket_buffer[4..12];
                                let data = &socket_buffer[12..bytes_read];
                                if let Some(udp_address) = connection.udp_address {
                                    if address == udp_address {
                                        if verify(&connection.key, data, tag) {
                                            // Verified sender, create event:
                                            inbound_sender.send((id, Event::Received {
                                                data: data.to_vec()
                                            })).await.unwrap();
                                        }
                                    } else {
                                        // Ignore message because address does not match specified connection-id.
                                    }
                                } else {
                                    // Handshake - Received UDP, respond with CONNECTED (3):
                                    if verify(&connection.key, data, tag) && data == b"ACK" {
                                        connection.udp_address = Some(address);
                                        connection.write(b"ACK").await.unwrap(); // TODO: handle possible error?
                                        inbound_sender.send((id, Event::Connected)).await.unwrap();
                                    }
                                }
                            }
                        }
                    }
                },
                result = outbound_receiver.recv() => {
                    if let Some((id, mut message)) = result {
                        if let Some(connection) = connections.get_mut(id as usize) {
                            if message.reliable {
                                match connection.write(&message.data).await {
                                    Ok(()) => {},
                                    Err(err) => println!("Error writing message (TCP): {}", err)
                                }
                            } else if let Some(address) = connection.udp_address {
                                let tag = sign(&connection.key, &message.data);

                                let mut data = tag.to_vec();
                                data.append(&mut message.data);

                                match socket.send_to(&data, address).await {
                                    Ok(_) => {},
                                    Err(err) => println!("Error writing message (UDP): {}", err)
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
    sender: TokioSender<(ConnectionId, Message)>,
}

impl ServerSender {
    pub fn send(&self, id: ConnectionId, message: Message) -> Result<()> {
        self.sender.send((id, message)).map_err(|err| err.into())
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
