use std::time::Duration;
use std::net::SocketAddr;
use super::datagram::Payload;

pub enum Event {
    Connected(SocketAddr),
    /// Received a payload on the specified connection.
    Received {
        address: SocketAddr,
        payload: Payload,
        /// The estimated RTT so far if it has been calculated.
        rtt: Option<Duration>,
        /// The time in milliseconds the other side of the connection waited before sending the RTT-acknowledgement.
        rtt_offset: Option<Duration>
    },
    Disconnected(SocketAddr)
}