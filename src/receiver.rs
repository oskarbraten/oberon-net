pub use futures::channel::mpsc::{
    channel, Receiver as InnerReceiver, Sender as InnerSender, TryRecvError, TrySendError,
};
use futures::StreamExt;

use thiserror::Error;
#[derive(Debug, Error)]
pub enum RecvError {
    #[error("Receiver error: {0}")]
    Recv(#[from] TryRecvError),
    #[error("Receiver is empty and closed.")]
    Closed,
}

#[derive(Debug)]
pub struct Receiver<T> {
    receiver: InnerReceiver<T>,
}

impl<T> Receiver<T> {
    pub fn new(receiver: InnerReceiver<T>) -> Self {
        Self { receiver }
    }

    /// Asynchronously receive an event.
    pub async fn recv(&mut self) -> Option<T> {
        self.receiver.next().await
    }

    /// Attempts to receive an event. This function is non-blocking.
    pub fn try_recv(&mut self) -> Result<T, RecvError> {
        match self.receiver.try_next() {
            Ok(Some(t)) => Ok(t),
            Ok(None) => Err(RecvError::Closed),
            Err(err) => Err(err.into()),
        }
    }
}
