//! # Zelda
//!
//! ## Example:
//!
//! ```rust
//! use std::net::SocketAddr;
//! use crossbeam::channel::{Sender, Receiver};
//! use zelda::{Socket, Config, Packet, Event};
//!
//! fn main() -> Result<(), std::io::Error> {
//!
//!     let socket_address: SocketAddr = "127.0.0.1:38000".parse().unwrap();
//!     
//!     let socket1 = Socket::bind(socket_address, Config::default())?;
//!     let socket2 = Socket::bind_any(Config::default())?;
//!     
//!     println!("Address of socket 2: {}", socket2.local_address());
//!     
//!     let packet_sender: Sender<Packet> = socket2.packet_sender();
//!     packet_sender.send(Packet::new(socket_address, "Hello, Client!".as_bytes().to_vec()));
//!     
//!     let event_receiver: Receiver<Event> = socket1.event_receiver();
//!     
//!     while let Ok(event) = event_receiver.recv() {
//!         match event {
//!             Event::Connected(addr) => {
//!                 // A connection was established with addr.
//!             },
//!             Event::Received { address, payload, rtt, rtt_offset } => {
//!                 // Received payload on addr with estimated rtt.
//!                 println!("Received payload: {}", std::str::from_utf8(&payload).unwrap());
//!             },
//!             Event::Disconnected(addr) => {
//!                 // Client with addr disconnected.
//!                 break;
//!             }
//!         }
//!     }
//!
//!     Ok(())
//!
//! }
//! ```
//!
//! More examples in the [repository](https://github.com/oskarbraten/zelda).

use std::io::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const VERSION_MAJOR: &'static str = env!("CARGO_PKG_VERSION_MAJOR");
const VERSION_MINOR: &'static str = env!("CARGO_PKG_VERSION_MINOR");
const VERSION_PATCH: &'static str = env!("CARGO_PKG_VERSION_PATCH");

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref VERSION: (u8, u8, u8) = (
        VERSION_MAJOR.parse().unwrap(),
        VERSION_MINOR.parse().unwrap(),
        VERSION_PATCH.parse().unwrap()
    );
}

use chashmap::CHashMap;
use crossbeam::channel;

mod collections;

mod config;
pub use config::Config;

mod event;
pub use event::Event;

mod packet;
pub use packet::Packet;

mod datagram;
use datagram::Datagram;

mod connection;
use connection::Connection;

#[derive(Debug, Clone)]
pub struct Socket {
    socket: Arc<UdpSocket>,
    sender: channel::Sender<Packet>,
    receiver: channel::Receiver<Event>,
}

impl Socket {
    /// Bind to any available port on the system and connect to the specified address.
    /// Connect in this case means connecting the underlying UdpSocket such that datagrams from different addresses are filtered.
    pub fn connect(address: SocketAddr, config: Config) -> Result<Self> {
        let any = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0);
        let socket = UdpSocket::bind(any)?;
        socket.connect(address)?;

        Self::bind_with(socket, config)
    }

    /// Bind to any available port on the system.
    pub fn bind_any(config: Config) -> Result<Self> {
        Self::bind(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0),
            config,
        )
    }

    /// Bind to the specified address.
    pub fn bind(address: SocketAddr, config: Config) -> Result<Self> {
        let socket = UdpSocket::bind(address)?;

        Self::bind_with(socket, config)
    }

    pub fn bind_with(socket: UdpSocket, config: Config) -> Result<Self> {
        let connections: Arc<CHashMap<SocketAddr, Connection>> = Arc::new(CHashMap::new());

        let (outbound_sender, outbound_receiver) = channel::unbounded::<Packet>();
        let (inbound_sender, inbound_receiver) = channel::bounded::<Event>(config.event_capacity);

        // Reader thread:
        {
            let socket = socket.try_clone()?;
            let inbound_sender = inbound_sender.clone();
            let connections = connections.clone();
            thread::spawn(move || {
                loop {
                    // Receive datagrams:
                    let mut buffer = [0; 1450];
                    match socket.recv_from(&mut buffer) {
                        Ok((bytes_read, address)) => {
                            let remote_version = (buffer[0], buffer[1], buffer[2]);
                            if remote_version != *VERSION {
                                continue; // Protocol version does not match, just discard it.
                            }

                            match bincode::deserialize::<Datagram>(&buffer[3..bytes_read]) {
                                Ok(Datagram {
                                    payload,
                                    rtt_seq,
                                    rtt_ack,
                                    rtt_offset,
                                }) => {
                                    connections.alter(address.clone(), |conn| {
                                        let mut connection = match conn {
                                            Some(mut connection) => {
                                                connection.last_interaction = Instant::now();

                                                connection
                                            }
                                            None => {
                                                let connection =
                                                    Connection::new(config.rtt_buffer_size);
                                                inbound_sender
                                                    .send(Event::Connected(address))
                                                    .expect("Unable to dispatch event to channel.");

                                                connection
                                            }
                                        };

                                        if let Some(instant) =
                                            connection.rtt.timers.remove(&rtt_ack)
                                        {
                                            let rtt_sample = instant.elapsed();
                                            let rtt_corrected = rtt_sample
                                                .checked_sub(Duration::from_millis(
                                                    rtt_offset as u64,
                                                ))
                                                .unwrap_or(Duration::from_millis(0));

                                            match connection.rtt.current {
                                                Some(rtt) => {
                                                    connection.rtt.current = Some(
                                                        (rtt.mul_f32(1.0 - config.rtt_alpha))
                                                            + rtt_corrected
                                                                .mul_f32(config.rtt_alpha),
                                                    );
                                                }
                                                None => {
                                                    connection.rtt.current = Some(rtt_corrected);
                                                }
                                            }
                                        }

                                        connection.rtt.remote_seq = rtt_seq;
                                        connection.rtt.remote_timer = Instant::now();

                                        inbound_sender
                                            .send(Event::Received {
                                                address,
                                                payload,
                                                rtt: connection.rtt.current,
                                                rtt_offset: connection.rtt.current.and(Some(
                                                    Duration::from_millis(rtt_offset as u64),
                                                )),
                                            })
                                            .expect("Unable to dispatch event to channel.");
                                        Some(connection)
                                    });
                                }
                                Err(_) => {
                                    // println!("Error parsing payload: {}", msg);
                                    continue;
                                }
                            }
                        }
                        Err(msg) => {
                            panic!("Encountered IO error: {}", msg);
                        }
                    }
                }
            });
        }

        // Sender thread:
        {
            let socket = socket.try_clone()?;
            let inbound_sender = inbound_sender.clone();
            let connections = connections.clone();
            thread::spawn(move || {
                loop {
                    match outbound_receiver.recv() {
                        Ok(Packet { address, payload }) => {
                            connections.alter(address.clone(), |conn| {
                                let mut connection = match conn {
                                    Some(connection) => connection,
                                    None => {
                                        let connection = Connection::new(config.rtt_buffer_size);
                                        inbound_sender
                                            .send(Event::Connected(address))
                                            .expect("Unable to dispatch event to channel.");

                                        connection
                                    }
                                };

                                let seq = connection.rtt.timers.insert(Instant::now());
                                let rtt_offset = connection
                                    .rtt
                                    .remote_timer
                                    .elapsed()
                                    .as_millis()
                                    .min(std::u16::MAX as u128)
                                    as u16;

                                let mut buffer: Vec<u8> =
                                    vec![(*VERSION).0, (*VERSION).1, (*VERSION).2];
                                bincode::serialize_into(
                                    &mut buffer,
                                    &Datagram::new(
                                        payload,
                                        seq,
                                        connection.rtt.remote_seq,
                                        rtt_offset,
                                    ),
                                )
                                .expect("Unable to serialize datagram.");
                                match socket.send_to(&buffer[0..], address) {
                                    Ok(_) => {}
                                    Err(msg) => println!("Error sending packet: {}", msg),
                                }
                                Some(connection)
                            });
                        }
                        Err(_) => {
                            break; // Is empty and disconnected, terminate thread.
                        }
                    }
                }
            });
        }

        // Connection timeout checker thread:
        {
            let connections = connections.clone();
            let inbound_sender = inbound_sender.clone();
            thread::spawn(move || loop {
                {
                    connections.retain(|address, connection: &Connection| {
                        if connection.last_interaction.elapsed() >= config.timeout {
                            inbound_sender
                                .try_send(Event::Disconnected(address.clone()))
                                .expect("Unable to dispatch event to channel.");
                            false
                        } else {
                            true
                        }
                    });
                }

                thread::sleep(config.timeout_check_interval);
            });
        }
        Ok(Self {
            socket: Arc::new(socket),
            sender: outbound_sender,
            receiver: inbound_receiver,
        })
    }

    /// Gets an Arc to the underlying UdpSocket.
    pub fn socket(&self) -> Arc<UdpSocket> {
        self.socket.clone()
    }

    /// Gets the local address of the socket.
    pub fn local_address(&self) -> Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Get a clone of the event receiver.
    /// It is thread-safe, and is used to listen for events on the socket.
    pub fn event_receiver(&self) -> channel::Receiver<Event> {
        self.receiver.clone()
    }

    /// Get a clone of the packet sender.
    /// It is thread-safe, and is used to send packets.
    pub fn packet_sender(&self) -> channel::Sender<Packet> {
        self.sender.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{collections::SequenceRingBuffer, Config, Event, Packet, Socket, SocketAddr};
    #[test]
    fn sequence_ring_buffer() {
        for size in 1..std::u8::MAX {
            let mut buffer: SequenceRingBuffer<u8, ()> = SequenceRingBuffer::new(size);
            let max = (std::u8::MAX / size) * size;

            for i in 0..(2 * (std::u8::MAX as u16)) {
                let seq = buffer.insert(());
                assert_eq!(seq, ((i + 1) % max as u16) as u8);
                if seq == 255 {
                    println!("Seq: {}, I: {}", seq, ((i + 1) % max as u16) as u8);
                }
            }
        }
    }

    #[test]
    fn sending_and_receiving() {
        let server_address: SocketAddr = "127.0.0.1:38000".parse().unwrap();
        let client_address: SocketAddr = "127.0.0.1:38001".parse().unwrap();

        let server = Socket::bind(server_address, Config::default()).unwrap();
        let client = Socket::bind(client_address, Config::default()).unwrap();

        let j1 = std::thread::spawn(move || {
            for _ in 0..10 {
                server
                    .packet_sender()
                    .send(Packet::new(
                        client_address,
                        "Hello, Client!".as_bytes().to_vec(),
                    ))
                    .unwrap();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            loop {
                match server.event_receiver().recv() {
                    Ok(Event::Connected(addr)) => {
                        println!("Client connected to server!");
                        assert_eq!(addr, client_address);
                    }
                    Ok(Event::Received {
                        address,
                        payload,
                        rtt,
                        rtt_offset,
                    }) => {
                        println!("Server received content: {}, estimated RTT: {} ms, offset: {} ms, has estimate: {}", std::str::from_utf8(&payload).unwrap(), rtt.unwrap_or_default().as_millis(), rtt_offset.unwrap_or_default().as_millis(), rtt.is_some());
                        assert_eq!(address, client_address);
                        assert_eq!("Hello, Server!".as_bytes().to_vec(), payload);
                    }
                    Ok(Event::Disconnected(addr)) => {
                        println!("Client disconnected from server!");
                        assert_eq!(addr, client_address);
                        break;
                    }
                    Err(err) => {
                        panic!("Error: {}", err);
                    }
                }
            }
        });

        let j2 = std::thread::spawn(move || loop {
            match client.event_receiver().recv() {
                Ok(Event::Connected(addr)) => {
                    println!("Server connected to client!");
                    assert_eq!(addr, server_address);
                }
                Ok(Event::Received {
                    address,
                    payload,
                    rtt,
                    rtt_offset,
                }) => {
                    println!("Client received content: {}, estimated RTT: {} ms, offset: {} ms, has estimate: {}", std::str::from_utf8(&payload).unwrap(), rtt.unwrap_or_default().as_millis(), rtt_offset.unwrap_or_default().as_millis(), rtt.is_some());
                    assert_eq!(address, server_address);
                    assert_eq!("Hello, Client!".as_bytes().to_vec(), payload);

                    client
                        .packet_sender()
                        .send(Packet::new(
                            server_address,
                            "Hello, Server!".as_bytes().to_vec(),
                        ))
                        .unwrap();
                }
                Ok(Event::Disconnected(addr)) => {
                    println!("Server disconnnected from client!");
                    assert_eq!(addr, server_address);
                    break;
                }
                Err(err) => {
                    panic!("Error: {}", err);
                }
            }
        });

        j1.join().unwrap();
        j2.join().unwrap();
    }

    #[test]
    fn connecting() {
        let address1: SocketAddr = "127.0.0.1:38000".parse().unwrap();
        let address2: SocketAddr = "127.0.0.1:38001".parse().unwrap();

        let client1 = Socket::bind(address1, Config::default()).unwrap();
        let client2 = Socket::bind(address2, Config::default()).unwrap();

        let client3 = Socket::connect(address1, Config::default()).unwrap();
        let address3 = client3.local_address().unwrap();

        client1
            .packet_sender()
            .send(Packet::new(
                address3,
                "Hello, Client 3!".as_bytes().to_vec(),
            ))
            .unwrap();

        // Packets from other clients, such as client 2, are filtered:
        client2
            .packet_sender()
            .send(Packet::new(
                address3,
                "Hello, Client 3!".as_bytes().to_vec(),
            ))
            .unwrap();

        loop {
            match client3.event_receiver().recv() {
                Ok(Event::Connected(addr)) => {
                    assert_eq!(addr, address1);
                }
                Ok(Event::Received {
                    address,
                    payload,
                    rtt: _,
                    rtt_offset: _,
                }) => {
                    assert_eq!(address, address1);
                    assert_eq!("Hello, Client 3!".as_bytes().to_vec(), payload);
                }
                Ok(Event::Disconnected(addr)) => {
                    assert_eq!(addr, address1);
                    break;
                }
                Err(err) => {
                    panic!("Error: {}", err);
                }
            }
        }
    }
}
