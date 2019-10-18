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
    /// Size of the buffer that records RTT, lower capacity can mean less measurements.
    /// Set this based on the send-rate of the client and server, and the expected maximum RTT.
    /// The buffer is a ring-buffer meaning once the number of entries starts exceeding the size the oldest entry will be dropped.
    /// 
    /// Explanation: Given a send-rate of 60 packets per second and a size of 60, once a second has passed the oldest sequence number will be overwritten.
    /// Therefore if an acknowledgement takes more than a second it will no longer be used to estimate the RTT.
    /// 
    /// A good value for this is the send-rate multiplied by the timeout. For example: _60 packets/second * 1 second = 60 packets_.
    pub rtt_buffer_size: u16
}

impl Config {
    pub fn default() -> Self {
        Self {
            timeout: Duration::from_millis(1000),
            timeout_check_interval: Duration::from_millis(100),
            event_capacity: 65536,
            rtt_alpha: 0.125,
            rtt_buffer_size: 90
        }
    }

    pub fn new(timeout: Duration, timeout_check_interval: Duration, event_capacity: usize, rtt_alpha: f32, rtt_buffer_size: u16) -> Self {
        Self {
            timeout,
            timeout_check_interval,
            event_capacity,
            rtt_alpha,
            rtt_buffer_size
        }
    }
}