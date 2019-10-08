use std::net::SocketAddr;
use super::datagram::Payload;

#[derive(Debug, Clone)]
pub struct Packet {
    pub address: SocketAddr,
    pub reliable: bool,
    pub payload: Payload
}

impl Packet {
    pub fn new(address: SocketAddr, payload: Payload) -> Self {
        Self {
            address,
            reliable: false,
            payload
        }
    }

    pub fn reliable(address: SocketAddr, payload: Payload) -> Self {
        Self {
            address,
            reliable: true,
            payload
        }
    }
}