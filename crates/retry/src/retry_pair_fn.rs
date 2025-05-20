use core::fmt::Display;
use core::marker::PhantomData;
use core::time::Duration;

use super::{Abortable, RetryError};
use crate::backoff_pair_iterator::{AlternationStep, BackoffPairIterator};
use crate::{MAX_ATTEMPTS, RATE_LIMIT_WAIT_SECS};

pub struct RetryPairFn<Fn1, Fn2, T, Err> {
    backoff: BackoffPairIterator,
    first_function: Fn1,
    second_function: Fn2,
    max_attempts: usize,
    rate_limit_wait: Duration,
    _phantom: PhantomData<(T, Err)>,
}

impl<Fn1, Fn2, T, Err> RetryPairFn<Fn1, Fn2, T, Err>
where
    Fn1: AsyncFn() -> Result<T, Err>,
    Fn2: AsyncFn() -> Result<T, Err>,
    Err: Display + Abortable,
{
    pub(crate) const fn new(
        backoff: BackoffPairIterator,
        first_function: Fn1,
        second_function: Fn2,
    ) -> Self {
        Self {
            backoff,
            first_function,
            second_function,
            // TODO: put into config
            max_attempts: MAX_ATTEMPTS,
            rate_limit_wait: Duration::from_secs(RATE_LIMIT_WAIT_SECS),
            _phantom: PhantomData,
        }
    }

    // you can use a macro to reduce copy-paste
    // but this will reduce readability
    #[tracing::instrument(skip_all, name = "retry if fails (alternating)")]
    pub async fn retry(self) -> Result<T, RetryError<Err>> {
        let mut primary_fn_aborted = false;
        let mut primary_fn_rate_limitted = false;
        let mut secondary_fn_aborted = false;
        let mut secondary_fn_rate_limitted = false;
        let mut last_abort_err: Option<Err> = None;
        for (retry_count, iteration) in self.backoff.enumerate().take(self.max_attempts) {
            match iteration.alteration_step {
                AlternationStep::First => {
                    if primary_fn_aborted {
                        if secondary_fn_aborted {
                            return Err(RetryError::Aborted(last_abort_err.expect("abort error")));
                        }
                        continue;
                    }
                    if primary_fn_rate_limitted {
                        tokio::time::sleep(self.rate_limit_wait).await;
                        primary_fn_rate_limitted = false;
                    } else {
                        tokio::time::sleep(iteration.duration).await;
                    }
                    match (self.first_function)().await {
                        Ok(res) => return Ok(res),
                        Err(err) if err.abortable() => {
                            tracing::error!(%err, retry_count, "primary lambda aborted");
                            primary_fn_aborted = true;
                            last_abort_err = Some(err);
                            if secondary_fn_aborted {
                                tracing::error!("both lambdas aborted");
                                return Err(RetryError::Aborted(
                                    last_abort_err.expect("abort error"),
                                ));
                            }
                        }
                        Err(err) if err.rate_limit() => {
                            primary_fn_rate_limitted = true;
                            tracing::error!(%err, retry_count, "retry attempt with primary lambda faced rate_limit");
                        }
                        Err(err) => {
                            tracing::error!(%err, retry_count, "retry attempt with primary lambda failed");
                        }
                    }
                }
                AlternationStep::Second => {
                    if secondary_fn_aborted {
                        if primary_fn_aborted {
                            return Err(RetryError::Aborted(last_abort_err.expect("abort error")));
                        }
                        continue;
                    }
                    if secondary_fn_rate_limitted {
                        tokio::time::sleep(self.rate_limit_wait).await;
                        secondary_fn_rate_limitted = false;
                    } else {
                        tokio::time::sleep(iteration.duration).await;
                    }
                    match (self.second_function)().await {
                        Ok(res) => return Ok(res),
                        Err(err) if err.abortable() => {
                            tracing::error!(%err, retry_count, "secondary lambda aborted");
                            secondary_fn_aborted = true;
                            last_abort_err = Some(err);
                            if primary_fn_aborted {
                                tracing::error!("both lambdas aborted");
                                return Err(RetryError::Aborted(
                                    last_abort_err.expect("abort error"),
                                ));
                            }
                        }
                        Err(err) if err.rate_limit() => {
                            secondary_fn_rate_limitted = true;
                            tracing::error!(%err, retry_count, "retry attempt with primary lambda faced rate_limit");
                        }
                        Err(err) => {
                            tracing::error!(%err, retry_count, "retry attempt with secondary lambda failed");
                        }
                    }
                }
            }
        }
        return Err(RetryError::MaxAttempts);
    }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicI32, Ordering};
    use core::time::Duration;
    use std::sync::Arc;

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestError {
        abort: bool,
        msg: &'static str,
    }
    impl core::fmt::Display for TestError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "TestError abort={} msg={}", self.abort, self.msg)
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

    fn test_backoff() -> BackoffPairIterator {
        BackoffPairIterator::new(
            Duration::from_millis(1),
            core::num::NonZeroU64::new(2).unwrap(),
            Duration::from_millis(10),
        )
    }

    macro_rules! function_always_succeeding {
        ($val:expr) => {{
            let call_count = Arc::new(AtomicI32::new(0_i32));
            let call_count_clone = call_count.clone();
            let func = move || {
                let counter = call_count_clone.clone();

                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    Ok::<_, TestError>($val)
                }
            };
            (func, call_count)
        }};
    }

    macro_rules! function_always_failing {
        () => {{
            let call_count = Arc::new(AtomicI32::new(0_i32));
            let call_count_clone = call_count.clone();
            let func = move || {
                let counter = call_count_clone.clone();

                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    Err::<&'static str, _>(TestError {
                        abort: false,
                        msg: "fail",
                    })
                }
            };
            (func, call_count)
        }};
    }

    macro_rules! function_always_aborting {
        () => {{
            let call_count = Arc::new(AtomicI32::new(0_i32));
            let call_count_clone = call_count.clone();
            let func = move || {
                let counter = call_count_clone.clone();

                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    Err::<&'static str, _>(TestError {
                        abort: true,
                        msg: "fails with abort",
                    })
                }
            };
            (func, call_count)
        }};
    }

    macro_rules! function_failing_until_n_attempt {
        ($threshold:expr, $val:expr) => {{
            let call_count = Arc::new(AtomicI32::new(0_i32));
            let call_count_clone = call_count.clone();
            let func = move || {
                let counter = call_count_clone.clone();

                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    if counter.load(Ordering::Relaxed) < $threshold {
                        Err(TestError {
                            abort: false,
                            msg: "fail",
                        })
                    } else {
                        Ok::<_, TestError>($val)
                    }
                }
            };
            (func, call_count)
        }};
    }

    macro_rules! function_aborts_on_n_attempt {
        ($threshold:expr) => {{
            let call_count = Arc::new(AtomicI32::new(0_i32));
            let call_count_clone = call_count.clone();
            let func = move || {
                let counter = call_count_clone.clone();

                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    if counter.load(Ordering::Relaxed) < $threshold {
                        Err(TestError {
                            abort: false,
                            msg: "fail",
                        })
                    } else {
                        Err(TestError {
                            abort: true,
                            msg: "fail",
                        })
                    }
                }
            };
            (func, call_count)
        }};
    }

    #[tokio::test]
    async fn retry_pair_succeeds_on_first_fn() {
        let (func1, _func1_called_count) = function_always_succeeding!("ok1");
        let (func2, _func2_called_count) = function_always_failing!();
        let retry = RetryPairFn::new(test_backoff(), func1, func2);
        assert_eq!(retry.retry().await.unwrap(), "ok1");
    }

    #[tokio::test]
    async fn retry_pair_succeeds_on_second_fn() {
        let (func1, _func1_called_count) = function_always_failing!();
        let (func2, _func2_called_count) = function_always_succeeding!("ok2");
        let retry = RetryPairFn::new(test_backoff(), func1, func2);
        assert_eq!(retry.retry().await.unwrap(), "ok2");
    }

    #[tokio::test]
    async fn retry_pair_alternates_until_success() {
        let (func1, func1_called_count) = function_failing_until_n_attempt!(3_i32, "ok1");
        let (func2, _func2_called_count) = function_always_failing!();
        let retry = RetryPairFn::new(test_backoff(), func1, func2);
        let result = retry.retry().await.unwrap();
        assert_eq!(result, "ok1");
        assert!(func1_called_count.load(Ordering::Relaxed) >= 3_i32);
    }

    #[tokio::test]
    async fn retry_pair_aborts_when_both_abort() {
        let (func1, _func1_called_count) = function_always_aborting!();
        let (func2, _func2_called_count) = function_always_aborting!();
        let retry = RetryPairFn::new(test_backoff(), func1, func2);
        assert!(matches!(retry.retry().await, Err(RetryError::Aborted(e)) if e.abort));
    }

    #[tokio::test]
    async fn retry_pair_returns_max_attempts() {
        let (func, func_called_count) = function_always_failing!();
        let backoff = test_backoff();
        let retry = RetryPairFn {
            max_attempts: 4,
            ..RetryPairFn::new(backoff, func.clone(), func)
        };
        assert!(matches!(retry.retry().await, Err(RetryError::MaxAttempts)));
        assert_eq!(func_called_count.load(Ordering::Relaxed), 4_i32);
    }

    #[tokio::test]
    async fn retry_pair_primary_aborts_secondary_retries_until_max_attempts() {
        let (func1, _func1_called_count) = function_always_aborting!();
        let (func2, _func2_called_count) = function_always_failing!();
        let retry = RetryPairFn {
            max_attempts: 4,
            ..RetryPairFn::new(test_backoff(), func1, func2)
        };
        assert!(matches!(retry.retry().await, Err(RetryError::MaxAttempts)));
    }

    #[tokio::test]
    async fn retry_pair_secondary_aborts_primary_retries_until_max_attempts() {
        let (func1, _func1_called_count) = function_always_failing!();
        let (func2, _func2_called_count) = function_always_aborting!();
        let retry = RetryPairFn {
            max_attempts: 4,
            ..RetryPairFn::new(test_backoff(), func1, func2)
        };
        assert!(matches!(retry.retry().await, Err(RetryError::MaxAttempts)));
    }

    #[tokio::test]
    async fn retry_pair_primary_aborts_after_some_failures() {
        let (func1, func1_called_count) = function_aborts_on_n_attempt!(3_i32);
        let (func2, _func2_called_count) = function_always_failing!();
        let retry = RetryPairFn {
            max_attempts: 6,
            ..RetryPairFn::new(test_backoff(), func1, func2)
        };
        assert!(matches!(retry.retry().await, Err(RetryError::MaxAttempts)));
        assert_eq!(func1_called_count.load(Ordering::Relaxed), 3_i32);
    }

    #[tokio::test]
    async fn retry_pair_secondary_aborts_after_some_failures() {
        let (func1, _func1_called_count) = function_always_failing!();
        let (func2, func2_called_count) = function_aborts_on_n_attempt!(2_i32);
        let retry = RetryPairFn {
            max_attempts: 5,
            ..RetryPairFn::new(test_backoff(), func1, func2)
        };
        assert!(matches!(retry.retry().await, Err(RetryError::MaxAttempts)));
        assert_eq!(func2_called_count.load(Ordering::Relaxed), 2_i32);
    }

    #[tokio::test]
    async fn retry_pair_one_aborts_other_always_fails_then_both_abort() {
        let (func1, _func1_called_count) = function_always_aborting!();
        let (func2, func2_called_count) = function_aborts_on_n_attempt!(2_i32);
        let retry = RetryPairFn::new(test_backoff(), func1, func2);
        assert!(matches!(retry.retry().await, Err(RetryError::Aborted(e)) if e.abort));
        assert_eq!(func2_called_count.load(Ordering::Relaxed), 2_i32);
    }
}
