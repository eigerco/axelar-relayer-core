use core::fmt::Display;
use core::time::Duration;

use tokio_retry::strategy::ExponentialBackoff;

use super::{Abortable, RetryError};
use crate::{MAX_ATTEMPTS, RATE_LIMIT_WAIT_SECS};

pub struct RetryFn<Fn, T, Err>
where
    Fn: AsyncFn() -> Result<T, Err>,
    Err: Display,
{
    backoff: ExponentialBackoff,
    function: Fn,
    max_attempts: usize,
    rate_limit_wait: Duration,
}

impl<Fn, T, Err> RetryFn<Fn, T, Err>
where
    Fn: AsyncFn() -> Result<T, Err>,
    Err: Display + Abortable,
{
    pub(crate) const fn new(backoff: ExponentialBackoff, function: Fn) -> Self {
        Self {
            backoff,
            function,
            // TODO: config
            max_attempts: MAX_ATTEMPTS,
            rate_limit_wait: Duration::from_secs(RATE_LIMIT_WAIT_SECS),
        }
    }

    // you can use a macro to reduce copy-paste
    // but this will reduce readability
    #[tracing::instrument(skip_all)]
    pub async fn retry(self) -> Result<T, RetryError<Err>> {
        for (retry_attempt, duration) in self.backoff.enumerate().take(self.max_attempts) {
            tokio::time::sleep(duration).await;
            match (self.function)().await {
                Ok(res) => return Ok(res),
                Err(err) if err.abortable() => {
                    tracing::error!(%err, "aborted");
                    return Err(RetryError::Aborted(err));
                }
                Err(err) if err.rate_limit() => {
                    tracing::error!(%err, "rate limit reached, extend wait");
                    tokio::time::sleep(self.rate_limit_wait).await;
                }
                Err(err) => {
                    tracing::error!(%err, retry_attempt, "retryable lambda err");
                }
            }
        }

        return Err(RetryError::MaxAttempts);
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicI32, Ordering};

    use tokio_retry::strategy::ExponentialBackoff;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestError {
        abort: bool,
    }
    impl core::fmt::Display for TestError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "TestError abort={}", self.abort)
        }
    }

    impl Abortable for TestError {
        fn abortable(&self) -> bool {
            self.abort
        }

        fn rate_limit(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn retry_succeeds_first_try() {
        let call_count = AtomicI32::new(0);
        let func = || {
            let counter = &call_count; // Reference the outer variable
            async move {
                counter.fetch_add(1, Ordering::Relaxed);
                Ok::<_, TestError>("ok")
            }
        };
        let backoff = ExponentialBackoff::from_millis(1);
        let retry_fn = RetryFn::new(backoff, func);
        let result = retry_fn.retry().await;
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(call_count.load(Ordering::Relaxed), 1_i32);
    }

    #[tokio::test]
    async fn retry_until_success() {
        // The function will be called 3 times.
        // First 2 times it will return an error
        // and the last time it will return Ok
        let call_count = AtomicI32::new(0);
        let func = || {
            let counter = &call_count;
            async move {
                counter.fetch_add(1, Ordering::Relaxed);
                if counter.load(Ordering::Relaxed) < 3_i32 {
                    Err(TestError { abort: false })
                } else {
                    Ok::<_, TestError>("ok")
                }
            }
        };
        let backoff = ExponentialBackoff::from_millis(1);
        let retry_fn = RetryFn::new(backoff, func);
        let result = retry_fn.retry().await;
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(call_count.load(Ordering::Relaxed), 3_i32);
    }

    #[tokio::test]
    async fn retry_aborts_on_abortable_error() {
        let func = || async { Err::<(), _>(TestError { abort: true }) };
        let backoff = ExponentialBackoff::from_millis(1);
        let retry_fn = RetryFn::new(backoff, func);
        let result = retry_fn.retry().await;
        assert!(matches!(result, Err(RetryError::Aborted(ref e)) if e.abort));
    }

    #[tokio::test]
    async fn retry_returns_max_attempts() {
        let call_count = AtomicI32::new(0);
        let func = || {
            let counter = &call_count;
            async move {
                counter.fetch_add(1, Ordering::Relaxed);
                Err::<(), _>(TestError { abort: false })
            }
        };
        let backoff = ExponentialBackoff::from_millis(1);
        let mut retry_fn = RetryFn::new(backoff, func);
        retry_fn.max_attempts = 3;
        let result = retry_fn.retry().await;
        assert!(matches!(result, Err(RetryError::MaxAttempts)));

        // Check that the function was called 3 times
        assert_eq!(call_count.load(Ordering::Relaxed), 3_i32);
    }
}
