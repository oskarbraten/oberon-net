use hibitset::BitSet;
use slab::Slab;
use std::{convert::TryInto, future::Future, sync::Arc};
use tokio::{
    io::split,
    net::{TcpListener, ToSocketAddrs, UdpSocket},
    sync::RwLock,
};

#[cfg(feature = "rustls")]
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};

use thiserror::Error;
#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Unable to create server.")]
    Io(#[from] std::io::Error),
}

use crate::{
    receiver, sender, Config, Connection, ConnectionId, Delivery, Event, Receiver, Sender,
};

use futures::StreamExt;

pub struct Server;

impl<'a> Server {
    /// Start a server listening on the specified address.
    /// Returns a [`Sender`], [`Receiver`] and a [`Future`] which must be awaited in an async executor (see the examples in the [repository](https://github.com/oskarbraten/zelda/)).
    /// The server can run in a separate thread and messages/events can be sent/received in a synchronous context.
    pub fn listen<
        A: ToSocketAddrs,
        #[cfg(feature = "token")] F: 'static,
        #[cfg(feature = "token")] U: 'static,
    >(
        address: A,
        config: Config,
        #[cfg(feature = "rustls")] server_config: ServerConfig,
        #[cfg(feature = "token")] validation_fn: F,
    ) -> (
        Sender<(ConnectionId, Vec<u8>, Delivery)>,
        Receiver<(ConnectionId, Event)>,
        impl Future<Output = Result<(), ServerError>>,
    )
    where
        U: Send + Sync + Clone,
        F: (Fn(Vec<u8>) -> Option<U>) + Send + Sync + Clone,
    {
        let (outbound_sender, outbound_receiver) =
            sender::channel::<(ConnectionId, Vec<u8>, Delivery)>();
        let (inbound_sender, inbound_receiver) =
            receiver::channel::<(ConnectionId, Event)>(config.event_capacity);

        let task = Self::task(
            address,
            config,
            inbound_sender,
            outbound_receiver,
            #[cfg(feature = "rustls")]
            server_config,
            #[cfg(feature = "token")]
            validation_fn,
        );

        (
            Sender::new(outbound_sender),
            Receiver::new(inbound_receiver),
            task,
        )
    }

    async fn task<
        A: ToSocketAddrs,
        #[cfg(feature = "token")] F: 'static,
        #[cfg(feature = "token")] U: 'static,
    >(
        address: A,
        config: Config,
        mut inbound_sender: receiver::InnerSender<(ConnectionId, Event)>,
        mut outbound_receiver: sender::InnerReceiver<(ConnectionId, Vec<u8>, Delivery)>,
        #[cfg(feature = "rustls")] server_config: ServerConfig,
        #[cfg(feature = "token")] validation_fn: F,
    ) -> Result<(), ServerError>
    where
        U: Send + Sync + Clone,
        F: (Fn(Vec<u8>) -> Option<U>) + Send + Sync + Clone,
    {
        // #[cfg(feature = "token")]
        // let validation_fn = Arc::new(validation_fn);

        let socket = UdpSocket::bind(&address).await?;

        #[cfg(feature = "rustls")]
        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let listener = TcpListener::bind(&address).await?;

        // #[cfg(feature = "token")]
        // let connections: Arc<RwLock<Slab<Connection<_, U>>>> = Arc::new(RwLock::new(Slab::new()));

        // #[cfg(not(feature = "token"))]
        // let connections: Arc<RwLock<Slab<Connection<_, ()>>>> = Arc::new(RwLock::new(Slab::new()));

        let connections = Arc::new(RwLock::new(Slab::new()));
        let established_connections = Arc::new(RwLock::new(BitSet::new()));

        let mut recv_buffer = [0u8; std::u16::MAX as usize];
        loop {
            tokio::select! {
                result = listener.accept() => {
                    if let Ok((stream, address)) = result {
                        log::debug!("Accepting a new connection: {}", address);

                        let _ = stream.set_nodelay(true);

                        #[cfg(feature = "rustls")]
                        let (read_stream, write_stream) = {
                            let acceptor = acceptor.clone();
                            let stream = acceptor.accept(stream).await?;
                            split(stream)
                        };

                        #[cfg(not(feature = "rustls"))]
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
                        let established_connections = established_connections.clone();
                        let mut inbound_sender = inbound_sender.clone();
                        // #[cfg(feature = "token")]
                        let validation_fn = validation_fn.clone();

                        tokio::spawn(async move {
                            let mut read_stream = read_stream;
                            loop {
                                match Connection::read(&mut read_stream, config.max_reliable_size).await {
                                    Ok(data) => {
                                        let is_connected = established_connections.read().await.contains(id);
                                        if is_connected {
                                            inbound_sender.try_send((id, Event::Received(data))).unwrap();
                                        } else if &data[0..3] == b"ACK" {

                                            #[cfg(feature = "token")]
                                            let token_data = {
                                                let token = data[3..].to_vec();
                                                validation_fn(token)
                                            };
                                            #[cfg(feature = "token")]
                                            if token_data.is_none() {
                                                // Token validation failed, remove and drop connection.
                                                let mut connections = connections.write().await;
                                                connections.remove(id as usize);
                                                break;
                                            } else {
                                                let mut connections = connections.write().await;
                                                let connection = connections.get_mut(id as usize).unwrap();

                                                connection.data = Some(token_data);
                                            }

                                            established_connections.write().await.add(id);
                                            inbound_sender.try_send((id, Event::Connected)).unwrap();
                                        }
                                    },
                                    Err(err) => {
                                        log::debug!("Error reading frame (TCP): {:#?}", err);
                                        {
                                            let mut connections = connections.write().await;
                                            connections.remove(id as usize);
                                            established_connections.write().await.remove(id);
                                        }
                                        inbound_sender.try_send((id, Event::Disconnected)).unwrap();
                                        break;
                                    }
                                }
                            }
                        });
                    }
                },
                result = socket.recv_from(&mut recv_buffer) => {
                    if let Ok((bytes_read, remote_address)) = result {
                        // Must receive more than tag (u64) bytes + id (u32)
                        if bytes_read >= 14 {
                            let id = recv_buffer[8..12].try_into().map(|bytes| u32::from_be_bytes(bytes));
                            let connections = connections.read().await;
                            let result = id.ok().and_then(|id| connections.get(id as usize).map(|c| (id, c)));
                            if let Some((id, connection)) = result {

                                let tag = &recv_buffer[0..8];
                                let data = &recv_buffer[12..bytes_read];

                                let is_connected = established_connections.read().await.contains(id);
                                let mut connection_address = connection.address.lock().await;
                                if is_connected && connection_address.map(|addr| addr == remote_address).unwrap_or(false) && connection.verify(data, tag) {
                                    // Verified sender, create event:
                                    inbound_sender.try_send((id, Event::Received(data.to_vec()))).unwrap();
                                } else if !is_connected && connection_address.is_none() && data == b"ACK" && connection.verify(data, tag) {
                                    // Handshake - Received UDP, respond with ACK (3):
                                    *connection_address = Some(remote_address);
                                    connection.write(b"ACK").await.unwrap(); // TODO: handle possible error?
                                }
                            }
                        }
                    }
                },
                result = outbound_receiver.next() => {
                    if let Some((id, mut data, delivery)) = result {
                        let is_connected = established_connections.read().await.contains(id);
                        if is_connected {
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
                                        let connection_address = connection.address.lock().await;
                                        if let Some(connection_address) = *connection_address {
                                            let tag = connection.sign(&data);

                                            let mut bytes = tag.to_vec();
                                            bytes.append(&mut data);

                                            match socket.send_to(&bytes, connection_address).await {
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
}
