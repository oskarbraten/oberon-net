// use tokio::time::Duration;

#[derive(Debug, Clone)]
pub enum Event {
    Connected,
    /// Received data on the specified connection.
    Received {
        data: Vec<u8>,
        // /// The estimated RTT so far if it has been calculated.
        // rtt: Option<Duration>,
        // /// The time in milliseconds the other side of the connection waited before sending the RTT-acknowledgement.
        // rtt_offset: Option<Duration>,
    },
    Disconnected,
}
