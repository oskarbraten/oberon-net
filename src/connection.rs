use std::time::{Instant, Duration};
use linked_hash_map::LinkedHashMap;

#[derive(Debug, Clone)]
pub struct Connection {
    pub last_interaction: Instant,
    pub rtt: Option<Duration>,
    pub rtt_timers: LinkedHashMap<u16, Instant>,
    pub rtt_seq_local: u16,
    pub rtt_seq_remote: u16
}

impl Connection {
    pub fn new() -> Self {
        Self {
            last_interaction: Instant::now(),
            rtt: None,
            rtt_timers: LinkedHashMap::new(),
            rtt_seq_local: 0,
            rtt_seq_remote: 0
        }
    }
}