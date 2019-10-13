use std::time::{Instant, Duration};
use linked_hash_map::LinkedHashMap;

#[derive(Debug, Clone)]
pub struct Connection {
    pub last_interaction: Instant,
    pub rtt: Option<Duration>,
    pub rtt_local_seq: u16,
    pub rtt_local_timers: LinkedHashMap<u16, Instant>,
    pub rtt_remote_seq: u16,
    pub rtt_remote_timer: Instant
}

impl Connection {
    pub fn new() -> Self {
        Self {
            last_interaction: Instant::now(),
            rtt: None,
            rtt_local_seq: 0,
            rtt_local_timers: LinkedHashMap::new(),
            rtt_remote_seq: 0,
            rtt_remote_timer: Instant::now()
        }
    }
}