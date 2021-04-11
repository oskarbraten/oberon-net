pub use futures::channel::mpsc::{
    unbounded as channel, TryRecvError, TrySendError, UnboundedReceiver as InnerReceiver,
    UnboundedSender as InnerSender,
};

use crate::{ConnectionId, Delivery};

pub type ServerSender = Sender<(ConnectionId, Vec<u8>, Delivery)>;
pub type ClientSender = Sender<(Vec<u8>, Delivery)>;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SendError {
    #[error("The sender is full.")]
    Full,
    #[error("The sender is disconnected.")]
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct Sender<T> {
    sender: InnerSender<T>,
}

impl<T> Sender<T> {
    pub fn new(sender: InnerSender<T>) -> Self {
        Self { sender }
    }
}

/// # Sender used for Client
impl ClientSender {
    pub fn send(&self, data: Vec<u8>, delivery: Delivery) -> Result<(), SendError> {
        self.sender.unbounded_send((data, delivery)).map_err(|err| {
            if err.is_full() {
                SendError::Full
            } else {
                SendError::Disconnected
            }
        })
    }

    /// Send data to the server with reliable delivery.
    pub fn reliable(&self, data: Vec<u8>) -> Result<(), SendError> {
        self.send(data, Delivery::Reliable)
    }

    /// Send data to the server with unreliable delivery.
    pub fn unreliable(&self, data: Vec<u8>) -> Result<(), SendError> {
        self.send(data, Delivery::Unreliable)
    }
}

/// # Sender used for Server
impl ServerSender {
    pub fn send(
        &self,
        id: ConnectionId,
        data: Vec<u8>,
        delivery: Delivery,
    ) -> Result<(), SendError> {
        self.sender
            .unbounded_send((id, data, delivery))
            .map_err(|err| {
                if err.is_full() {
                    SendError::Full
                } else {
                    SendError::Disconnected
                }
            })
    }

    /// Send data to a client with reliable delivery.
    pub fn reliable(&self, id: ConnectionId, data: Vec<u8>) -> Result<(), SendError> {
        self.send(id, data, Delivery::Reliable)
    }

    /// Send data to a client with unreliable delivery.
    pub fn unreliable(&self, id: ConnectionId, data: Vec<u8>) -> Result<(), SendError> {
        self.send(id, data, Delivery::Unreliable)
    }
}
