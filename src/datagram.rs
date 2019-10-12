use serde::{Serialize, Deserialize};

pub type Payload = Vec<u8>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Datagram {
    pub payload: Payload,
    pub rtt_seq: u32,
    pub rtt_ack: u32
}

impl Datagram {
    pub fn new(payload: Payload, rtt_seq: u32, rtt_ack: u32) -> Self {
        Self {
            payload,
            rtt_seq,
            rtt_ack
        }
    }
}