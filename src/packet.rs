use std::net::SocketAddr;
use super::datagram::Payload;

#[derive(Debug, Clone)]
pub struct Packet {
    pub address: SocketAddr,
    pub payload: Payload
}

impl Packet {
    pub fn new(address: SocketAddr, payload: Payload) -> Self {
        Self {
            address,
            payload
        }
    }
}