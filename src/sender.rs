use anyhow::Result;
use tokio::sync::mpsc::UnboundedSender;

use crate::{ConnectionId, Delivery};

#[derive(Debug, Clone)]
pub struct Sender<T> {
    sender: UnboundedSender<T>,
}

impl<T> Sender<T> {
    pub fn new(sender: UnboundedSender<T>) -> Self {
        Self { sender }
    }
}

/// # Sender used for Client
impl Sender<(Vec<u8>, Delivery)> {
    pub fn send(&self, data: Vec<u8>, delivery: Delivery) -> Result<()> {
        self.sender.send((data, delivery)).map_err(|err| err.into())
    }

    /// Send data to the server with reliable delivery.
    pub fn reliable(&self, data: Vec<u8>) -> Result<()> {
        self.send(data, Delivery::Reliable)
    }

    /// Send data to the server with unreliable delivery.
    pub fn unreliable(&self, data: Vec<u8>) -> Result<()> {
        self.send(data, Delivery::Unreliable)
    }
}

/// # Sender used for Server
impl Sender<(ConnectionId, Vec<u8>, Delivery)> {
    pub fn send(&self, id: ConnectionId, data: Vec<u8>, delivery: Delivery) -> Result<()> {
        self.sender
            .send((id, data, delivery))
            .map_err(|err| err.into())
    }

    /// Send data to a client with reliable delivery.
    pub fn reliable(&self, id: ConnectionId, data: Vec<u8>) -> Result<()> {
        self.send(id, data, Delivery::Reliable)
    }

    /// Send data to a client with unreliable delivery.
    pub fn unreliable(&self, id: ConnectionId, data: Vec<u8>) -> Result<()> {
        self.send(id, data, Delivery::Unreliable)
    }
}
