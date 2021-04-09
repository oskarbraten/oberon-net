//! # Zelda
//!
//! Checkout the examples in the [repository](https://github.com/oskarbraten/zelda/).

mod client;
mod config;
mod connection;
mod event;
mod server;
mod utils;
use connection::Connection;

pub use config::Config;
pub use event::Event;

#[derive(Debug, Clone, Copy)]
pub enum Delivery {
    Reliable,
    Unreliable,
}

pub use client::Client;
pub use server::Server;
