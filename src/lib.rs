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

mod client;
mod config;
mod event;
mod message;
mod server;

pub use config::Config;
pub use event::Event;
pub use message::Message;

pub use client::Client;
pub use server::Server;
