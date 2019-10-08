use serde::{Serialize, Deserialize};

pub type Payload = Vec<u8>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Datagram {
    Unreliable(Payload),
    Reliable(usize, Payload),
    Ack(usize)
}