//! Crate with retry logic
pub(crate) mod backoff_pair_iterator;
/// Builder for retries
pub mod builder;
pub(crate) mod retry_fn;
pub(crate) mod retry_pair_fn;

use core::num::NonZeroU64;
use core::time::Duration;

pub use builder::Builder;

pub(crate) const MAX_ATTEMPTS: usize = 20;
pub(crate) const RATE_LIMIT_WAIT_SECS: u64 = 10;

/// Retry error
#[derive(Debug, thiserror::Error)]
pub enum RetryError<Err: Abortable> {
    /// Execution aborted due to non-recoverable error
    #[error("aborted {0}")]
    Aborted(Err),
    /// Max retry attemps reached
    // TODO: add last err
    #[error("max retry attempts reached")]
    MaxAttempts,
}

#[derive(Debug, Clone)]
pub(crate) struct BackoffParrams {
    initial_delay: Duration,
    backoff_factor: NonZeroU64,
    max_delay: Duration,
}

/// Is error abortable i.e. non-recoverable
pub trait Abortable {
    /// error abortable i.e. non-recoverable
    fn abortable(&self) -> bool {
        false
    }
    /// rate limit error i.e. we need to wait more
    fn rate_limit(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU64;
    use core::sync::atomic::{AtomicI32, Ordering};
    use core::time::Duration;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestError(bool); // true = abortable
    impl core::fmt::Display for TestError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "TestError abortable={}", self.0)
        }
    }
    impl Abortable for TestError {
        fn abortable(&self) -> bool {
            self.0
        }

        fn rate_limit(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn builder_with_retry_success() {
        let builder = Builder::new(
            Duration::from_millis(1),
            NonZeroU64::new(2).unwrap(),
            Duration::from_millis(10),
        );
        let call_count = AtomicI32::new(0);
        let func = || {
            let counter = &call_count;
            async move {
                counter.fetch_add(1, Ordering::Relaxed);
                if counter.load(Ordering::Relaxed) < 2_i32 {
                    Err(TestError(false))
                } else {
                    Ok::<_, TestError>("ok")
                }
            }
        };
        let retry = builder.with_retry(func);
        let result = retry.retry().await;
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(call_count.load(Ordering::Relaxed), 2_i32);
    }

    #[tokio::test]
    async fn builder_with_retry_pair_success() {
        let builder = Builder::new(
            Duration::from_millis(1),
            NonZeroU64::new(2).unwrap(),
            Duration::from_millis(10),
        );
        let func1 = || async { Err::<&'static str, _>(TestError(false)) };
        let func2 = || async { Ok::<_, TestError>("ok2") };
        let retry = builder.with_retry_pair(func1, func2);
        let result = retry.retry().await;
        assert_eq!(result.unwrap(), "ok2");
    }

    #[tokio::test]
    async fn builder_with_retry_aborts_on_abortable() {
        let builder = Builder::new(
            Duration::from_millis(1),
            NonZeroU64::new(2).unwrap(),
            Duration::from_millis(10),
        );
        let func = || async { Err::<(), _>(TestError(true)) };
        let retry = builder.with_retry(func);
        let result = retry.retry().await;
        assert!(matches!(result, Err(RetryError::Aborted(_))));
    }
}
