# Zelda

[![Crates.io][crates_icon]][crates_link] ![MIT][license]

[crates_icon]: https://img.shields.io/crates/v/zelda.svg
[crates_link]: https://crates.io/crates/zelda/
[license]: https://img.shields.io/crates/l/zelda.svg?maxAge=2592000

Zelda is a lightweight application-level network protocol for use in real-time applications. It is built on UDP and provides lightweight virtual connections and estimation of Round-trip time. The API is inspired by [laminar](https://github.com/amethyst/laminar).

Zelda does not provide any reliability and is instead developed to be used alongside other reliable protocols (for example TCP) or with a reliability layer on top.

## Features

* [x] Virtual connections
* [x] Thread-safe send/receive
* [x] RTT estimation
* [ ] Immediate ack support (RTT estimation)

## Quick-start

```rust
use std::net::SocketAddr;
use crossbeam::channel::{Sender, Receiver};
use zelda::{Socket, Config, Packet, Event};

let socket_address: SocketAddr = "127.0.0.1:38000".parse().unwrap();

let socket1 = Socket::bind(socket_address, Config::default())?;
let socket2 = Socket::bind_any(Config::default())?;

println!("Address of socket 2: {}", socket2.local_address());

let packet_sender: Sender<Packet> = socket2.packet_sender();
packet_sender.send(Packet::new(socket_address, "Hello, Client!".as_bytes().to_vec()));

let event_receiver: Receiver<Event> = socket1.event_receiver();

while let Ok(event) = event_receiver.recv() {
    Event::Connected(addr) => {
        // A connection was established with addr.
    },
    Event::Received { address, payload, rtt, rtt_offset } => {
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
            Ok(Event::Received { address, payload, rtt, rtt_offset }) => {
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
            Ok(Event::Received { address, payload, rtt, rtt_offset }) => {
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

```bash
tc qdisc change dev <interface-name> root netem delay 100ms 20ms distribution normal
```

Check the [netem documentation](https://wiki.linuxfoundation.org/networking/netem#packet_loss) for more options.