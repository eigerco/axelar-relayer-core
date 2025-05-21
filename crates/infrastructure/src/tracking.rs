#![allow(dead_code, reason = "not implemented for every feature yet")]
use core::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub(crate) struct ThroughputTracker {
    processed_count: AtomicU64,
    last_count: AtomicU64,
    last_timestamp_ms: AtomicU64,
}

impl ThroughputTracker {
    pub(crate) fn new() -> Self {
        Self {
            processed_count: AtomicU64::new(0),
            last_count: AtomicU64::new(0),
            last_timestamp_ms: AtomicU64::new(current_time_millis()),
        }
    }

    /// Rate of operations i.e. messages/sec
    #[allow(clippy::float_arithmetic, reason = "need floating-point operations")]
    #[allow(clippy::cast_precision_loss, reason = "necessary in this context")]
    #[allow(
        clippy::as_conversions,
        reason = "Safer alternatives would be more complex here"
    )]
    #[allow(
        clippy::arithmetic_side_effects,
        reason = "Subtraction is needed for rate calculation"
    )]
    pub(crate) fn update_and_get_rate(&self) -> Option<f64> {
        let current_count = self.processed_count.load(Ordering::Acquire);
        let prev_count = self.last_count.swap(current_count, Ordering::AcqRel);

        let now = current_time_millis();
        let last = self.last_timestamp_ms.swap(now, Ordering::AcqRel);

        let elapsed_sec = (now - last) as f64 / 1000.0_f64;
        if elapsed_sec > 0.0_f64 {
            return Some((current_count - prev_count) as f64 / elapsed_sec);
        }

        None
    }

    pub(crate) fn record_processed_message(&self) {
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
