use futures::channel::mpsc::UnboundedSender;

use thiserror::Error;

use crate::ConnectionId;

#[derive(Debug, Error)]
pub enum DisconnectError {
    #[error("The disconnector is full.")]
    Full,
    #[error("The disconnector is disconnected.")]
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct Disconnector {
    sender: UnboundedSender<ConnectionId>,
}

impl Disconnector {
    pub fn new(sender: UnboundedSender<ConnectionId>) -> Self {
        Self { sender }
    }
    pub fn disconnect(&self, id: ConnectionId) -> Result<(), DisconnectError> {
        self.sender.unbounded_send(id).map_err(|err| {
            if err.is_full() {
                DisconnectError::Full
            } else {
                DisconnectError::Disconnected
            }
        })
    }
}
