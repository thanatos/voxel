use std::time::{Duration, Instant};

pub struct Timer {
    started: Instant,
}

impl Timer {
    pub fn start() -> Timer {
        Timer {
            started: Instant::now(),
        }
    }

    pub fn mark(&self) -> Duration {
        let now = Instant::now();
        now - self.started
    }
}
