use anyhow::Result;
use std::{future::Future, net::SocketAddr};
use tokio::{
    io::split,
    net::{TcpStream, UdpSocket},
    sync::mpsc::{
        unbounded_channel as tokio_channel, UnboundedReceiver as TokioReceiver,
        UnboundedSender as TokioSender,
    },
};

#[cfg(feature = "tls")]
use tokio_rustls::{rustls::ClientConfig, webpki::DNSName, TlsConnector};

#[cfg(feature = "tls")]
use std::sync::Arc;

use crate::{utils::read_frame, Delivery};
use crate::{Config, Connection, Event};

pub struct Client;

impl Client {
    pub fn connect(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        config: Config,
        #[cfg(feature = "tls")] client_config: ClientConfig,
        #[cfg(feature = "tls")] domain: DNSName,
    ) -> (
        ClientSender,
        ClientReceiver,
        impl Future<Output = Result<(), anyhow::Error>>,
    ) {
        let (outbound_sender, outbound_receiver) = tokio_channel::<(Vec<u8>, Delivery)>();
        let (inbound_sender, inbound_receiver) =
            async_channel::bounded::<Event>(config.event_capacity);

        let task = Self::task(
            tcp_address,
            udp_address,
            config,
            inbound_sender,
            outbound_receiver,
            #[cfg(feature = "tls")]
            client_config,
            #[cfg(feature = "tls")]
            domain,
        );

        (
            ClientSender {
                sender: outbound_sender,
            },
            ClientReceiver {
                receiver: inbound_receiver,
            },
            task,
        )
    }

    async fn task(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        _config: Config,
        inbound_sender: async_channel::Sender<Event>,
        mut outbound_receiver: TokioReceiver<(Vec<u8>, Delivery)>,
        #[cfg(feature = "tls")] client_config: ClientConfig,
        #[cfg(feature = "tls")] domain: DNSName,
    ) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(udp_address).await?;

        let mut socket_buffer: [u8; 1200] = [0; 1200];

        let stream = TcpStream::connect(tcp_address).await?;
        stream.set_nodelay(true).unwrap();

        #[cfg(not(feature = "tls"))]
        let (mut read_stream, write_stream) = split(stream);

        #[cfg(feature = "tls")]
        let (mut read_stream, write_stream) = {
            let connector = TlsConnector::from(Arc::new(client_config));
            let stream = connector.connect(domain.as_ref(), stream).await?;
            split(stream)
        };

        let (id, connection) = Connection::connect(&socket, &mut read_stream, write_stream).await?;

        inbound_sender.send(Event::Connected).await?;

        loop {
            tokio::select! {
                result = read_frame(&mut read_stream, 500000) => {
                    match result {
                        Ok(data) => {
                            inbound_sender.send(Event::Received {
                                data
                            }).await?;
                        },
                        Err(err) => {
                            log::debug!("Client read frame error: {:#?}", err);
                            inbound_sender.send(Event::Disconnected).await?;
                            break Err(err.into());
                        }
                    }
                },
                result = socket.recv(&mut socket_buffer) => {
                    if let Ok(bytes_read) = result {
                        // Must receive more than tag (u64) bytes
                        if bytes_read > 8 {
                            let tag = &socket_buffer[0..8];
                            let data = &socket_buffer[8..bytes_read];

                            if connection.verify(data, tag) {
                                inbound_sender.send(Event::Received {
                                    data: data.to_vec()
                                }).await?;
                            }
                        }
                    }
                },
                result = outbound_receiver.recv() => {
                    if let Some((mut data, delivery)) = result {
                        match delivery {
                            Delivery::Reliable => match connection.write(&data).await {
                                Ok(()) => {},
                                Err(err) => log::debug!("Error writing message (TCP): {}", err)
                            },
                            Delivery::Unreliable => {
                                let mut bytes = connection.sign(&data).to_vec(); // Add tag.
                                bytes.extend(&id.to_be_bytes()); // Add id.
                                bytes.push(0); // Add mode (0 = message)
                                bytes.append(&mut data); // Add data.

                                match socket.send(&bytes).await {
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

#[derive(Debug, Clone)]
pub struct ClientSender {
    sender: TokioSender<(Vec<u8>, Delivery)>,
}

impl ClientSender {
    pub fn send(&self, data: Vec<u8>, delivery: Delivery) -> Result<()> {
        self.sender.send((data, delivery)).map_err(|err| err.into())
    }

    pub fn reliable(&self, data: Vec<u8>) -> Result<()> {
        self.send(data, Delivery::Reliable)
    }

    pub fn unreliable(&self, data: Vec<u8>) -> Result<()> {
        self.send(data, Delivery::Unreliable)
    }
}

#[derive(Debug, Clone)]
pub struct ClientReceiver {
    receiver: async_channel::Receiver<Event>,
}

impl ClientReceiver {
    pub async fn recv(&self) -> Result<Event> {
        self.receiver.recv().await.map_err(|err| err.into())
    }

    pub fn try_recv(&self) -> Result<Event> {
        self.receiver.try_recv().map_err(|err| err.into())
    }
}
