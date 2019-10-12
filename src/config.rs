use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// The amount of time without communication that can be passed before a client is considered disconnected.
    pub timeout: Duration,
    /// Number of incoming events the socket can hold before it blocks reading from the UDP-socket.
    /// If the capacity is reached the underlying receive buffer may also reach its capacity resulting in packets being dropped.
    pub event_capacity: usize,
    /// Factor used for smoothing RTT, formula: ((1.0 - rtt_alpha) * previous_estimate) + (rtt_alpha * sample_rtt)
    pub rtt_alpha: f32,
    /// Capacity of the queue that records RTT, lower capacity can mean less measurements.
    /// Set this based on the send-rate of the client and server, and the expected maximum RTT.
    pub rtt_queue_capacity: usize
}

impl Config {
    pub fn default() -> Self {
        Self {
            timeout: Duration::from_millis(1000),
            rtt_alpha: 0.125,
            rtt_queue_capacity: 90,
            event_capacity: 65536
        }
    }
}