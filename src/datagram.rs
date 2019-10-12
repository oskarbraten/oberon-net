use serde::{Serialize, Deserialize};

pub type Payload = Vec<u8>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Datagram {
    pub version: (u8, u8, u8),
    pub rtt_seq: u16,
    pub rtt_ack: u16,
    pub payload: Payload
}

impl Datagram {
    pub fn new(version: (u8, u8, u8), payload: Payload, rtt_seq: u16, rtt_ack: u16) -> Self {
        Self {
            version,
            rtt_seq,
            rtt_ack,
            payload,
        }
    }
}