//! # Overview
//! Usage examples in the [repository](https://github.com/oskarbraten/zelda/).
//!
//! To get started, you can create a server by using the listen function on the [`Server`]-struct.
//! Afterwards, you can connect to server using the connect function on the [`Client`]-struct.
//! Both functions return a tuple of a [`Sender`], [`Receiver`], and a Future.
//!
//! The Future represents the computation that drives the transport protocol.
//! It attempts to perfom a handshake which establishes a connection between the client and the server.
//! When the Future is running (for example with a Tokio runtime), you can start sending messages on the [`Sender`] and receiving events on the [`Receiver`].

#[derive(Debug, Clone, Copy)]
pub enum Delivery {
    /// The message is guaranteed to reach the recipient (server or client).
    /// It is also encrypted if the `rustls` feature is enabled.
    Reliable,
    /// The message is not guaranteed to reach the recipient (server or client), nor is it guaranteed to arrive in order or once.
    Unreliable,
}
#[derive(Debug, Clone)]
pub enum Event {
    Connected,
    Received(Vec<u8>),
    Disconnected,
}

mod connection;
use connection::Connection;
pub type ConnectionId = u32;

mod client;
mod config;
mod receiver;
mod sender;
mod server;

pub use config::Config;

pub use receiver::{Receiver, RecvError};
pub use sender::{SendError, Sender};

pub use client::{Client, ClientReceiver, ClientSender};
pub use server::{Server, ServerReceiver, ServerSender};
