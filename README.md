# Zelda

Zelda is a lightweight application-level network protocol for use in real-time applications. It is built on UDP and provides lightweight virtual connections and estimation of Round-trip time. The API is inspired by [laminar](https://github.com/amethyst/laminar).

Zelda does not provide any reliability and is instead developed to be used alongside other reliable protocols (for example TCP).

## Features
 * [x] Virtual connections
 * [x] RTT estimation
 * [x] Thread-safe send/receive


## Quick-start
Zelda provides a struct called Socket which can be bound to an address:
```rust
use std::net::SocketAddr;
use crossbeam::channel::{Sender, Receiver};
use zelda::{Socket, Config, Packet, Event};

let socket_1_address: SocketAddr = "127.0.0.1:38000".parse().unwrap();

let socket_1 = Socket::bind(, Config::default())?;
let socket_2 = Socket::bind_any(Config::default())?;

println!("Address of socket 2: {}", socket_2.local_address());

let packet_sender: Sender<Packet> = socket_2.packet_sender();
packet_sender.send(Packet::new(socket_1_address, "Hello, Client!".as_bytes().to_vec()));

let event_receiver: Receiver<Event> = socket_1.event_receiver();

while let Ok(event) = event_receiver.recv() {
    Event::Connected(addr) => {
        // A connection was established with addr.
    },
    Event::Received(addr, payload, estrtt) => {
        // Received payload on addr with estimated rtt.
    },
    Event::Disconnected(addr) => {
        // Client with addr disconnected.
        break;
    }
}
```

Another full example:
```rust
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
```

## Simulating network conditions 
Zelda does not include a link conditioner, instead you should use a separate program such as [netem](https://wiki.linuxfoundation.org/networking/netem) to simulate link conditions.

### Using netem on linux
Setting 100ms latency with a 20ms normally distributed variation:
```
tc qdisc change dev <interface-name> root netem delay 100ms 20ms distribution normal
```
Check the documentation for more: [netem documentation](https://wiki.linuxfoundation.org/networking/netem#packet_loss)