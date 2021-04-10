use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Receiver<T> {
    receiver: async_channel::Receiver<T>,
}

impl<T> Receiver<T> {
    pub fn new(receiver: async_channel::Receiver<T>) -> Self {
        Self { receiver }
    }

    /// Asynchronously receive an event.
    pub async fn recv(&self) -> Result<T> {
        self.receiver.recv().await.map_err(|err| err.into())
    }

    /// Attempts to receive an event. This function is non-blocking.
    pub fn try_recv(&self) -> Result<T> {
        self.receiver.try_recv().map_err(|err| err.into())
    }
}
