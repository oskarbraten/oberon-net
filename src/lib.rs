use std::thread;
use std::sync::Arc;
use std::time::Instant;
use std::io::Result;
use std::net::{UdpSocket, SocketAddr};

const VERSION_MAJOR: &'static str = env!("CARGO_PKG_VERSION_MAJOR");
const VERSION_MINOR: &'static str = env!("CARGO_PKG_VERSION_MINOR");
const VERSION_PATCH: &'static str = env!("CARGO_PKG_VERSION_PATCH");

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref VERSION: (u8, u8, u8) = (VERSION_MAJOR.parse().unwrap(), VERSION_MINOR.parse().unwrap(), VERSION_PATCH.parse().unwrap());
}

use chashmap::CHashMap;
use crossbeam::channel;

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
    local_address: SocketAddr,
    sender: channel::Sender<Packet>,
    receiver: channel::Receiver<Event>
}

impl Socket {
    /// Bind to any available port on the system.
    pub fn bind_any(config: Config) -> Result<Self> {
        Self::bind("0.0.0.0:0".parse().unwrap(), config)
    }

    /// Bind to the specified address.
    pub fn bind(address: SocketAddr, config: Config) -> Result<Self> {
        let connections: Arc<CHashMap<SocketAddr, Connection>> = Arc::new(CHashMap::new());

        let (outbound_sender, outbound_receiver) = channel::unbounded::<Packet>();
        let (inbound_sender, inbound_receiver) = channel::bounded::<Event>(config.event_capacity);

        let socket = UdpSocket::bind(address)?;
        let local_address = socket.local_addr()?;

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
                                Ok(Datagram { payload, rtt_seq, rtt_ack }) => {
                                    connections.alter(address.clone(), |conn| {
                                        let mut connection = match conn {
                                            Some(mut connection) => {
                                                connection.last_interaction = Instant::now();

                                                connection
                                            },
                                            None => {
                                                let connection = Connection::new();
                                                inbound_sender.send(Event::Connected(address)).expect("Unable to dispatch event to channel.");

                                                connection
                                            }
                                        };

                                        if let Some(instant) = connection.rtt_timers.remove(&rtt_ack) {
                                            let rtt_sample = instant.elapsed();

                                            match connection.rtt {
                                                Some(rtt) => {
                                                    connection.rtt = Some((rtt.mul_f32(1.0 - config.rtt_alpha)) + rtt_sample.mul_f32(config.rtt_alpha));
                                                },
                                                None => {
                                                    connection.rtt = Some(rtt_sample);
                                                }
                                            }
                                        }

                                        connection.rtt_seq_remote = rtt_seq;
                                        inbound_sender.send(Event::Received(address, payload, connection.rtt)).expect("Unable to dispatch event to channel.");
                                        
                                        Some(connection)
                                    });
                                },
                                Err(_) => {
                                    // println!("Error parsing payload: {}", msg);
                                    continue;
                                }
                            }
                        },
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
                                        let connection = Connection::new();
                                        inbound_sender.send(Event::Connected(address)).expect("Unable to dispatch event to channel.");

                                        connection
                                    }
                                };

                                connection.rtt_seq_local = connection.rtt_seq_local.wrapping_add(1);
                                connection.rtt_timers.insert(connection.rtt_seq_local, Instant::now());

                                // Trim queue:
                                while connection.rtt_timers.len() > config.rtt_queue_capacity {
                                    connection.rtt_timers.pop_front();
                                }

                                let mut buffer: Vec<u8> = vec![(*VERSION).0, (*VERSION).1, (*VERSION).2];
                                bincode::serialize_into(&mut buffer, &Datagram::new(payload, connection.rtt_seq_local, connection.rtt_seq_remote)).expect("Unable to serialize datagram.");
                                match socket.send_to(&buffer[0..], address) {
                                    Ok(_) => {},
                                    Err(msg) => println!("Error sending packet: {}", msg)
                                }
                                
                                Some(connection)
                            });
                        },
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
            thread::spawn(move || {
                loop {
                    {
                        connections.retain(|address, connection: &Connection| {
                            if connection.last_interaction.elapsed() >= config.timeout {
                                inbound_sender.try_send(Event::Disconnected(address.clone())).expect("Unable to dispatch event to channel.");
                                false
                            } else {
                                true
                            }
                        });
                    }

                    thread::sleep(config.timeout);
                }
            });
        }
        
        Ok(Self {
            local_address,
            sender: outbound_sender,
            receiver: inbound_receiver
        })
    }

    /// Gets the local address of the socket.
    pub fn local_address(&self) -> SocketAddr {
        self.local_address
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
    use super::{Socket, Event, SocketAddr, Config, Packet};
    
    #[test]
    fn sending_and_receiving() {

        let server_address: SocketAddr = "127.0.0.1:38000".parse().unwrap();
        let client_address: SocketAddr = "127.0.0.1:38001".parse().unwrap();

        let server = Socket::bind(server_address, Config::default()).unwrap();
        let client = Socket::bind(client_address, Config::default()).unwrap();

        let j1 = std::thread::spawn(move || {
            
            for _ in 0..20 {
                server.packet_sender().send(Packet::new(client_address, "Hello, Client!".as_bytes().to_vec())).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            loop {
                match server.event_receiver().recv() {
                    Ok(Event::Connected(addr)) => {
                        println!("Client connected to server!");
                        assert_eq!(addr, client_address);
                    },
                    Ok(Event::Received(addr, payload, rtt)) => {
                        println!("Server received content: {}, estimated RTT: {} ms, has estimate: {}", std::str::from_utf8(&payload).unwrap(), rtt.unwrap_or_default().as_millis(), rtt.is_some());
                        assert_eq!(addr, client_address);
                        assert_eq!("Hello, Server!".as_bytes().to_vec(), payload);
                    },
                    Ok(Event::Disconnected(addr)) => {
                        println!("Client disconnnected from server!");
                        assert_eq!(addr, client_address);
                        break;
                    },
                    Err(err) => {
                        panic!("Error: {}", err);
                    }
                }
            }
        });
        
        let j2 = std::thread::spawn(move || {
            for _ in 0..20 {
                client.packet_sender().send(Packet::new(server_address, "Hello, Server!".as_bytes().to_vec())).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            loop {
                match client.event_receiver().recv() {
                    Ok(Event::Connected(addr)) => {
                        println!("Server connected to client!");
                        assert_eq!(addr, server_address);
                    },
                    Ok(Event::Received(addr, payload, rtt)) => {
                        println!("Client received content: {}, estimated RTT: {} ms, has estimate: {}", std::str::from_utf8(&payload).unwrap(), rtt.unwrap_or_default().as_millis(), rtt.is_some());
                        assert_eq!(addr, server_address);
                        assert_eq!("Hello, Client!".as_bytes().to_vec(), payload);
                    },
                    Ok(Event::Disconnected(addr)) => {
                        println!("Server disconnnected from client!");
                        assert_eq!(addr, server_address);
                        break;
                    },
                    Err(err) => {
                        panic!("Error: {}", err);
                    }
                }
            }
        });

        j1.join().unwrap();
        j2.join().unwrap();
    }
}