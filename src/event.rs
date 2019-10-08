use std::net::SocketAddr;
use super::datagram::Payload;

pub enum Event {
    Connected(SocketAddr),
    Received(SocketAddr, Payload),
    Disconnected(SocketAddr)
}