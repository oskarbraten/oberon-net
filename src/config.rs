#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Maximum accepted size of an incoming reliable message. The default is 1MB, meaning that the
    pub max_reliable_size: u32,
    /// Number of incoming events the socket can hold before it blocks incoming events.
    /// If the capacity is reached the underlying receive buffers may also reach its capacity resulting in packets being dropped.
    pub event_capacity: usize,
}

impl Config {
    pub fn default() -> Self {
        Self {
            max_reliable_size: 1000000,
            event_capacity: 65536,
        }
    }

    pub fn new(max_reliable_size: u32, event_capacity: usize) -> Self {
        Self {
            max_reliable_size,
            event_capacity,
        }
    }
}
