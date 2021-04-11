use std::future::Future;
use tokio::{
    io::split,
    net::{TcpStream, ToSocketAddrs, UdpSocket},
};

#[cfg(feature = "rustls")]
use tokio_rustls::{rustls::ClientConfig, webpki::DNSName, TlsConnector};

#[cfg(feature = "rustls")]
use std::sync::Arc;

use crate::{
    connection::ConnectionError, receiver, sender, Config, Connection, Delivery, Event, Receiver,
    Sender,
};

use futures::StreamExt;

use thiserror::Error;
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Unable to create client.")]
    Io(#[from] std::io::Error),
    #[error("Unable to establish connection.")]
    Connection(#[from] ConnectionError),
    #[error("Unable to dispatch event.")]
    Event(#[from] receiver::TrySendError<Event>),
}

pub struct Client;

impl Client {
    /// Connect to a server.
    /// Returns a [`Sender`], [`Receiver`] and a [`Future`] which must be awaited in an async executor (see the examples in the [repository](https://github.com/oskarbraten/zelda/)).
    /// The client can run in a separate thread and messages/events can be sent/received in a synchronous context.
    pub fn connect<A: ToSocketAddrs>(
        address: A,
        config: Config,
        #[cfg(feature = "rustls")] domain: DNSName,
        #[cfg(feature = "rustls")] client_config: ClientConfig,
        #[cfg(feature = "token")] token: Vec<u8>,
    ) -> (
        Sender<(Vec<u8>, Delivery)>,
        Receiver<Event>,
        impl Future<Output = Result<(), ClientError>>,
    ) {
        let (outbound_sender, outbound_receiver) = sender::channel::<(Vec<u8>, Delivery)>();
        let (inbound_sender, inbound_receiver) = receiver::channel::<Event>(config.event_capacity);

        let task = Self::task(
            address,
            config,
            inbound_sender,
            outbound_receiver,
            #[cfg(feature = "rustls")]
            domain,
            #[cfg(feature = "rustls")]
            client_config,
            #[cfg(feature = "token")]
            token,
        );

        (
            Sender::new(outbound_sender),
            Receiver::new(inbound_receiver),
            task,
        )
    }

    async fn task<A: ToSocketAddrs>(
        address: A,
        config: Config,
        mut inbound_sender: receiver::InnerSender<Event>,
        mut outbound_receiver: sender::InnerReceiver<(Vec<u8>, Delivery)>,
        #[cfg(feature = "rustls")] domain: DNSName,
        #[cfg(feature = "rustls")] client_config: ClientConfig,
        #[cfg(feature = "token")] token: Vec<u8>,
    ) -> Result<(), ClientError> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(&address).await?;

        let stream = TcpStream::connect(&address).await?;
        stream.set_nodelay(true).unwrap();

        #[cfg(not(feature = "rustls"))]
        let (mut read_stream, write_stream) = split(stream);

        #[cfg(feature = "rustls")]
        let (mut read_stream, write_stream) = {
            let connector = TlsConnector::from(Arc::new(client_config));
            let stream = connector.connect(domain.as_ref(), stream).await?;
            split(stream)
        };

        let (id, connection) = Connection::connect(
            &socket,
            &mut read_stream,
            write_stream,
            #[cfg(feature = "token")]
            token,
        )
        .await?;
        inbound_sender.try_send(Event::Connected)?;

        let mut recv_buffer = [0u8; std::u16::MAX as usize];
        loop {
            tokio::select! {
                result = Connection::read(&mut read_stream, config.max_reliable_size) => {
                    match result {
                        Ok(data) => {
                            inbound_sender.try_send(Event::Received(data))?;
                        },
                        Err(err) => {
                            log::debug!("Error reading frame (TCP): {:#?}", err);
                            inbound_sender.try_send(Event::Disconnected)?;
                            return Err(err.into());
                        }
                    }
                },
                result = socket.recv(&mut recv_buffer) => {
                    if let Ok(bytes_read) = result {
                        // Must receive more than tag (u64) bytes
                        if bytes_read > 8 {
                            let tag = &recv_buffer[0..8];
                            let data = &recv_buffer[8..bytes_read];

                            if connection.verify(data, tag) {
                                inbound_sender.try_send(Event::Received(data.to_vec()))?;
                            }
                        }
                    }
                },
                result = outbound_receiver.next() => {
                    if let Some((mut data, delivery)) = result {
                        match delivery {
                            Delivery::Reliable => match connection.write(&data).await {
                                Ok(()) => {},
                                Err(err) => log::debug!("Error writing message (TCP): {}", err)
                            },
                            Delivery::Unreliable => {
                                let mut bytes = connection.sign(&data).to_vec(); // Add tag.
                                bytes.extend(&id.to_be_bytes()); // Add id.
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
