use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// The amount of time without communication that can be passed before a client is considered disconnected.
    pub timeout: Duration,
    /// How often to check for timeouts of connections. Under the hood, this is done in a separate thread and will lock individual connections while checking.
    pub timeout_check_interval: Duration,
    /// Number of incoming events the socket can hold before it blocks reading from the UDP-socket.
    /// If the capacity is reached the underlying receive buffer may also reach its capacity resulting in packets being dropped.
    pub event_capacity: usize,
    /// Factor used for smoothing RTT, formula: ((1.0 - rtt_alpha) * previous_estimate) + (rtt_alpha * sample_rtt)
    pub rtt_alpha: f32,
    /// Capacity of the queue that records RTT, lower capacity can mean less measurements.
    /// Set this based on the send-rate of the client and server, and the expected maximum RTT.
    /// 
    /// Example: Given a symmetric send-rate of 60Hz and an expected maximum RTT of 1 second,
    /// we can expect there to be a maximum of 60 outstanding acknowledgements at any given time.
    /// Therefore we need the capacity to hold at least 60 acks in order to get accurate readings.
    pub rtt_queue_capacity: usize
}

impl Config {
    pub fn default() -> Self {
        Self {
            timeout: Duration::from_millis(1000),
            timeout_check_interval: Duration::from_millis(100),
            event_capacity: 65536,
            rtt_alpha: 0.125,
            rtt_queue_capacity: 90
        }
    }

    pub fn new(timeout: Duration, timeout_check_interval: Duration, event_capacity: usize, rtt_alpha: f32, rtt_queue_capacity: usize) -> Self {
        Self {
            timeout,
            timeout_check_interval,
            event_capacity,
            rtt_alpha,
            rtt_queue_capacity
        }
    }
}