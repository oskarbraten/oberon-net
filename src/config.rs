use std::time::Duration;

pub struct Config {
    /// The amount of time without communication that can be passed before a client is considered disconnected.
    pub timeout: Duration,
    /// Maximum size that the underlying UDP-socket can receive.
    pub max_size: usize,
    /// How often to retransmit unacked reliable Datagrams.
    pub reliable_retransmit_interval: Duration,
}

impl Config {
    pub fn default() -> Self {
        Self {
            timeout: Duration::from_millis(1000),
            max_size: 1450,
            reliable_retransmit_interval: Duration::from_millis(1000/30)
        }
    }
}