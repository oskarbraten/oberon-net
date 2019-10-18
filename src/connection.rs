use std::time::{Instant, Duration};
use super::collections::SequenceRingBuffer;

#[derive(Debug, Clone)]
pub struct Rtt {
    pub current: Option<Duration>,
    pub timers: SequenceRingBuffer<u16, Instant>,
    pub remote_seq: u16,
    pub remote_timer: Instant
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub last_interaction: Instant,
    pub rtt: Rtt
}

impl Connection {
    pub fn new(max_timers: u16) -> Self {
        Self {
            last_interaction: Instant::now(),
            rtt: Rtt {
                current: None,
                timers: SequenceRingBuffer::new(max_timers),
                remote_seq: 0,
                remote_timer: Instant::now()
            }
        }
    }
}