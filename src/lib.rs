use std::thread;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::net::{UdpSocket, SocketAddr};
use std::collections::HashMap;

use slab::Slab;
use chashmap::CHashMap;
use crossbeam::channel;

mod config;
use config::Config;

mod event;
pub use event::Event;

mod datagram;
use datagram::Datagram;

mod packet;
pub use packet::Packet;

#[derive(Debug, Clone)]
pub struct Connection {
    pub last_received: Instant,
    pub received_reliables: HashMap<usize, Instant>
}

#[derive(Debug, Clone)]
pub struct Socket {
    sender: channel::Sender<Packet>,
    receiver: channel::Receiver<Event>
}

impl Socket {
    pub fn bind_any(config: Config) -> Self {
        Self::bind("0.0.0.0:0".parse().unwrap(), config)
    }

    pub fn bind(address: SocketAddr, config: Config) -> Self {
        let connections: Arc<CHashMap<SocketAddr, Connection>> = Arc::new(CHashMap::new());

        let sent_reliables: Arc<Mutex<Slab<(Instant, Vec<u8>)>>> = Arc::new(Mutex::new(Slab::new()));
        
        let (outbound_sender, outbound_receiver) = channel::unbounded::<Packet>();
        let (inbound_sender, inbound_receiver) = channel::unbounded::<Event>();

        let socket = UdpSocket::bind(address).expect("Unable to bind UDP-socket.");

        {
            let socket = socket.try_clone().expect("Unable to clone UDP-socket.");
            let inbound_sender = inbound_sender.clone();
            let connections = connections.clone();
            let sent_reliables = sent_reliables.clone();
            thread::spawn(move || {
                loop {

                    // Receive datagrams:
                    let mut buffer = [0; 1450];
                    match socket.recv_from(&mut buffer) {
                        Ok((bytes_read, address)) => {
                            match bincode::deserialize::<Datagram>(&buffer[..bytes_read]) {
                                Ok(datagram) => {
                                    if let Some(mut connection) = connections.get_mut(&address) {
                                        connection.last_received = Instant::now();
                                    } else {
                                        connections.insert(address.clone(), Connection {
                                            last_received: Instant::now(),
                                            received_reliables: HashMap::new()
                                        });
                                        inbound_sender.try_send(Event::Connected(address)).expect("Unable to dispatch event to channel.");
                                    }

                                    match datagram {
                                        Datagram::Unreliable(payload) => {
                                            inbound_sender.try_send(Event::Received(address, payload)).expect("Unable to dispatch event to channel.");
                                        },
                                        Datagram::Reliable(id, payload) => {
                                            match connections.get_mut(&address) {
                                                Some(mut connection) => {
                                                    // Check if this packet has already been received.
                                                    if let Some(when) = connection.received_reliables.get(&id) {
                                                        if when.elapsed() >= Duration::from_millis(1000) {
                                                            // The old entry has timed out, we can assume that this payload is new.
                                                            connection.received_reliables.insert(id, Instant::now());
                                                            inbound_sender.try_send(Event::Received(address, payload)).expect("Unable to dispatch event to channel.");
                                                        } else {
                                                            // The old entry is still active, avoid emitting a duplicate payload.
                                                        }
                                                    } else {
                                                        // No entry for this Datagram, it is fresh.
                                                        connection.received_reliables.insert(id, Instant::now());
                                                        inbound_sender.try_send(Event::Received(address, payload)).expect("Unable to dispatch event to channel.");
                                                    }

                                                    // Send acknowledgement:
                                                    let buffer = bincode::serialize(&Datagram::Ack(id)).expect("Unable to serialize datagram.");
                                                    match socket.send_to(&buffer[0..], address) {
                                                        Ok(_) => {},
                                                        Err(msg) => println!("Error sending packet: {}", msg)
                                                    }
                                                },
                                                None => {
                                                    // The Connection is no longer present in the list. Do nothing?
                                                }
                                            }
                                        },
                                        Datagram::Ack(id) => {
                                            let mut sent_reliables = sent_reliables.lock().expect("Unable to lock send reliables slab.");
                                            if sent_reliables.contains(id) {
                                                sent_reliables.remove(id);
                                            }
                                        }
                                    }
                                },
                                Err(msg) => println!("Error parsing payload: {}", msg)
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
            let sent_reliables = sent_reliables.clone();
            let socket = socket.try_clone().expect("Unable to clone UDP-socket.");
            thread::spawn(move || {
                loop {
                    match outbound_receiver.recv() {
                        Ok(Packet { address, reliable: false, payload }) => {
                            let buffer = bincode::serialize(&Datagram::Unreliable(payload)).expect("Unable to serialize datagram.");
                            match socket.send_to(&buffer[0..], address) {
                                Ok(_) => {},
                                Err(msg) => println!("Error sending packet: {}", msg)
                            }
                        },
                        Ok(Packet { address, reliable: true, payload }) => {
                            let mut sent_reliables = sent_reliables.lock().expect("Unable to lock send reliables slab.");
                            let entry = sent_reliables.vacant_entry();
                            let buffer = bincode::serialize(&Datagram::Reliable(entry.key(), payload)).expect("Unable to serialize datagram.");

                            match socket.send_to(&buffer[0..], address) {
                                Ok(_) => {},
                                Err(msg) => println!("Error sending packet: {}", msg)
                            }

                            entry.insert((Instant::now(), buffer));
                        },
                        Err(_) => {
                            break; // Is empty and disconnected, terminate thread.
                        }
                    }
                }
            });

            // Timeout checker thread:
            {
                let connections = connections.clone();
                let inbound_sender = inbound_sender.clone();

                thread::spawn(move || {
                    loop {
                        {
                            connections.retain(|address, connection: &Connection| {
                                if connection.last_received.elapsed() >= config.timeout {
                                    inbound_sender.try_send(Event::Disconnected(address.clone())).expect("Unable to dispatch event to channel.");
                                    false
                                } else {
                                    true
                                }
                            });
                        }

                        thread::sleep(Duration::from_millis(100));
                    }
                });
            }
        }
        
        Self {
            sender: outbound_sender,
            receiver: inbound_receiver
        }
    }

    pub fn event_receiver(&self) -> channel::Receiver<Event> {
        self.receiver.clone()
    }

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

        let server = Socket::bind(server_address, Config::default());
        let client = Socket::bind(client_address, Config::default());

        client.packet_sender().send(Packet::reliable(server_address, "Hello, World!".as_bytes().to_vec()));
        
        let j1 = std::thread::spawn(move || {
            loop {
                match server.event_receiver().recv() {
                    Ok(Event::Connected(addr)) => {
                        println!("Client connected to server!");
                        assert_eq!(addr, client_address);
                    },
                    Ok(Event::Received(addr, payload)) => {
                        println!("Server received a packet from the client!");
                        assert_eq!(addr, client_address);
                        assert_eq!("Hello, World!".as_bytes().to_vec(), payload);
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
            loop {
                match client.event_receiver().recv() {
                    Ok(Event::Connected(addr)) => {
                        println!("Server connected to client!");
                        assert_eq!(addr, server_address);
                    },
                    Ok(Event::Received(addr, payload)) => {
                        println!("Client received a packet from the server!");
                        assert_eq!(addr, server_address);
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