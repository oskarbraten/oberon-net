/// Handshake:
/// 1. Client connects to server --> server sends `<connection-id><key>` to client over established TCP-connection.
/// 2. Client sends signed `<connection-id><cmac-tag>ACK` response over UDP-socket (repeated until received).
/// 3. Server receives ACK from client, verifies it, and sends `CONNECTED` response over TCP-connection.
use slab::Slab;
use std::{convert::TryInto, net::SocketAddr, thread};
use tokio::{io, io::AsyncReadExt, net::{TcpListener, UdpSocket, tcp::{OwnedReadHalf}}, sync::mpsc};

use crossbeam::channel;

use anyhow::Result;

use crate::{Config, Event, Message};

mod connection;
use connection::Connection;

pub type ConnectionId = u32;

#[derive(Debug, Clone)]
pub struct Server {
    sender: mpsc::UnboundedSender<(ConnectionId, Message)>,
    receiver: channel::Receiver<(ConnectionId, Event)>,
}

pub async fn read_frame(read_stream: &mut OwnedReadHalf, max_frame_size: u32) -> io::Result<Vec<u8>> {
    let frame_size = read_stream.read_u32().await?;
    if frame_size > max_frame_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Max frame size exceeded.",
        ));
    }

    let mut buffer = vec![0u8; frame_size as usize];
    read_stream.read_exact(&mut buffer).await?;

    Ok(buffer)
}

impl Server {
    
    pub async fn listen(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        config: Config,
    ) -> Result<Self> {
        let (outbound_sender, mut outbound_receiver) = mpsc::unbounded_channel::<(ConnectionId, Message)>();
        let (inbound_sender, inbound_receiver) =
            channel::bounded::<(ConnectionId, Event)>(config.event_capacity);

        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();

            runtime.block_on(async move {
                let socket = UdpSocket::bind(udp_address).await.unwrap();
                let mut socket_buffer: [u8; 1200] = [0; 1200];
                
                let listener = TcpListener::bind(tcp_address).await.unwrap();
                let mut connections: Slab<Connection> = Slab::new();

                loop {
                    tokio::select! {
                        result = listener.accept() => {
                            if let Ok((stream, _)) = result {
                                let _ = stream.set_nodelay(true);
                                let (read_stream, write_stream) = stream.into_split();

                                let entry = connections.vacant_entry();

                                let id = entry.key() as u32;
                                let connection = Connection::new(id, write_stream).await.unwrap();

                                entry.insert(connection);

                                let inbound_sender = inbound_sender.clone();
                                tokio::spawn(async move {
                                    let mut read_stream = read_stream;
                                    loop {
                                        match read_frame(&mut read_stream, 500000).await {
                                            Ok(data) => {
                                                inbound_sender.send((id, Event::Received {
                                                    data
                                                })).unwrap();
                                            },
                                            Err(err) => {
                                                println!("Read frame error: {:#?}", err);
                                                panic!("Read frame error.");
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
                                                if connection.verify(data, tag) {
                                                    // Verified sender, create event:
                                                    inbound_sender.send((id, Event::Received {
                                                        data: data.to_vec()
                                                    })).unwrap();
                                                }
                                            } else {
                                                // Ignore message because address does not match specified connection-id.
                                            }
                                        } else {
                                            // Received UDP handshake (3):
                                            if connection.verify(data, tag) && data == b"ACK" {
                                                connection.udp_address = Some(address);
                                                // connection.write(b"CONNECTED").await.unwrap(); // TODO: handle possible error?
                                                inbound_sender.send((id, Event::Connected)).unwrap();
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
                                        let tag = connection.sign(&message.data);
                                        
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
            });
        });

        Ok(Self {
            sender: outbound_sender,
            receiver: inbound_receiver,
        })
    }

    /// Get a clone of the event receiver.
    /// It is thread-safe, and is used to listen for events.
    pub fn receiver(&self) -> ServerReceiver {
        ServerReceiver {
            receiver: self.receiver.clone()
        }
    }

    /// Get a clone of the message sender.
    /// It is thread-safe, and is used to send messages.
    pub fn sender(&self) -> ServerSender {
        ServerSender {
            sender: self.sender.clone()
        }
    }
}


#[derive(Debug, Clone)]
pub struct ServerReceiver {
    receiver: channel::Receiver<(ConnectionId, Event)>,
}

// impl ServerReceiver {
//     pub fn recv(&self) -> Option<(ConnectionId, Event)> {
//         self.receiver.try_recv()
//     }
// }

#[derive(Debug, Clone)]
pub struct ServerSender {
    sender: mpsc::UnboundedSender<(ConnectionId, Message)>
}

// impl ServerSender {
//     pub fn send(&self, id: ConnectionId, message: Message) {
//         self.sender.send((id, message))
//     }
// }