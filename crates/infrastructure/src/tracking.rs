use core::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Instant;

pub struct ThroughputTracker {
    processed_count: AtomicU64,
    last_count: AtomicU64,
    last_timestamp_ms: AtomicU64,
}

impl ThroughputTracker {
    pub fn new() -> Self {
        Self {
            processed_count: AtomicU64::new(0),
            last_count: AtomicU64::new(0),
            last_timestamp_ms: AtomicU64::new(current_time_millis()),
        }
    }

    pub fn update_and_get_rate(&self) -> Option<f64> {
        let current_count = self.processed_count.load(Ordering::Relaxed);
        let prev_count = self.last_count.swap(current_count, Ordering::Relaxed);

        let now = current_time_millis();
        let last = self.last_timestamp_ms.swap(now, Ordering::Relaxed);

        let elapsed_sec = (now - last) as f64 / 1000.0;
        if elapsed_sec > 0.0 {
            return Some((current_count - prev_count) as f64 / elapsed_sec);
        }

        None
    }

    pub fn record_processed_message(&self) {
        self.processed_count.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn current_time_millis() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let duration = start.elapsed();
    #[allow(clippy::arithmetic_side_effects, reason = "unrealistic overflow")]
    {
        duration.as_secs() * 1000 + u64::from(duration.subsec_millis())
    }
}
