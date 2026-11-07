//! Shared API retry policy for provider calls.
//!
//! Only transient failures are retried: rate limits, timeouts, and provider
//! availability. Authentication, validation, context-length, and other
//! deterministic failures return immediately. Streaming calls additionally
//! must not retry once any chunk has been emitted (the caller tracks that).

use crate::ai::domain::types::AiError;
use std::future::Future;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Hard cap for configured retries (the safety gate exposes 0–10).
pub const MAX_API_RETRIES: u32 = 10;
const DEFAULT_BASE_DELAY: Duration = Duration::from_millis(250);
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RetryPolicy {
    pub fn new(max_retries: u32) -> Self {
        Self::with_delays(max_retries, DEFAULT_BASE_DELAY, DEFAULT_MAX_DELAY)
    }

    pub fn with_delays(max_retries: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_retries: max_retries.min(MAX_API_RETRIES),
            base_delay,
            max_delay: max_delay.max(base_delay),
        }
    }

    pub fn from_gate(gate: &crate::ai::domain::types::AiSafetyGateConfig) -> Self {
        Self::new(gate.api_retries)
    }

    /// True only for errors that a fresh provider call can plausibly fix.
    pub fn is_retryable(error: &AiError) -> bool {
        matches!(
            error,
            AiError::RateLimited { .. } | AiError::Timeout(_) | AiError::ProviderNotAvailable(_)
        )
    }

    /// Whether another attempt is allowed after `retries_done` retries.
    pub fn should_retry(&self, retries_done: u32, error: &AiError) -> bool {
        retries_done < self.max_retries && Self::is_retryable(error)
    }

    /// Exponential backoff: base × 2^retry, capped at `max_delay`.
    pub fn delay_for(&self, retry_index: u32) -> Duration {
        let factor = 1u32 << retry_index.min(10);
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }

    /// Wait for the backoff window, returning false if cancellation lands
    /// before or during the wait.
    pub async fn wait_before_retry(
        &self,
        retry_index: u32,
        cancel: Option<&CancellationToken>,
    ) -> bool {
        let delay = self.delay_for(retry_index);
        match cancel {
            Some(token) => {
                if token.is_cancelled() {
                    return false;
                }
                tokio::select! {
                    _ = tokio::time::sleep(delay) => !token.is_cancelled(),
                    _ = token.cancelled() => false,
                }
            }
            None => {
                tokio::time::sleep(delay).await;
                true
            }
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(2)
    }
}

/// Retry a fallible async operation according to `policy`.
///
/// Cancellation is checked before every attempt and during backoff; a
/// cancelled request returns [`AiError::Cancelled`] without another attempt.
pub async fn retry_async<F, Fut, T>(
    policy: &RetryPolicy,
    cancel: Option<&CancellationToken>,
    mut operation: F,
) -> Result<T, AiError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AiError>>,
{
    let mut retries_done = 0;
    loop {
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(AiError::Cancelled("request cancelled".into()));
        }
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !policy.should_retry(retries_done, &error) {
                    return Err(error);
                }
                if !policy.wait_before_retry(retries_done, cancel).await {
                    return Err(AiError::Cancelled(
                        "request cancelled during retry backoff".into(),
                    ));
                }
                retries_done += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn quick_policy(max_retries: u32) -> RetryPolicy {
        RetryPolicy::with_delays(
            max_retries,
            Duration::from_millis(10),
            Duration::from_millis(25),
        )
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        let policy = quick_policy(4);
        assert_eq!(policy.delay_for(0), Duration::from_millis(10));
        assert_eq!(policy.delay_for(1), Duration::from_millis(20));
        assert_eq!(policy.delay_for(2), Duration::from_millis(25));
        assert_eq!(policy.delay_for(9), Duration::from_millis(25));
    }

    #[test]
    fn retries_are_clamped_to_ten() {
        assert_eq!(RetryPolicy::new(50).max_retries, MAX_API_RETRIES);
        assert_eq!(RetryPolicy::new(0).max_retries, 0);
    }

    #[test]
    fn only_transient_errors_are_retryable() {
        assert!(RetryPolicy::is_retryable(&AiError::RateLimited {
            retry_after_secs: 0
        }));
        assert!(RetryPolicy::is_retryable(&AiError::Timeout(30)));
        assert!(RetryPolicy::is_retryable(&AiError::ProviderNotAvailable(
            "down".into()
        )));
        assert!(!RetryPolicy::is_retryable(&AiError::InvalidApiKey));
        assert!(!RetryPolicy::is_retryable(&AiError::RequestFailed(
            "bad request".into()
        )));
    }

    #[tokio::test]
    async fn retries_until_success_and_honors_the_limit() {
        let policy = quick_policy(2);
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let result = retry_async(&policy, None, || {
            let call = seen.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if call < 3 {
                    Err(AiError::RateLimited {
                        retry_after_secs: 0,
                    })
                } else {
                    Ok("ok")
                }
            }
        })
        .await;
        assert_eq!(result.expect("third attempt succeeds"), "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn zero_retries_never_repeats() {
        let policy = quick_policy(0);
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let result = retry_async(&policy, None, || {
            seen.fetch_add(1, Ordering::SeqCst);
            async {
                Err::<(), _>(AiError::RateLimited {
                    retry_after_secs: 0,
                })
            }
        })
        .await;
        assert!(matches!(result, Err(AiError::RateLimited { .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn non_retryable_error_is_not_repeated() {
        let policy = quick_policy(5);
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let result = retry_async(&policy, None, || {
            seen.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(AiError::InvalidApiKey) }
        })
        .await;
        assert!(matches!(result, Err(AiError::InvalidApiKey)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancel_during_backoff_stops_without_retrying() {
        let policy = quick_policy(5);
        let token = CancellationToken::new();
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let cancel = token.clone();
        let result = retry_async(&policy, Some(&token), || {
            seen.fetch_add(1, Ordering::SeqCst);
            cancel.cancel();
            async {
                Err::<(), _>(AiError::RateLimited {
                    retry_after_secs: 0,
                })
            }
        })
        .await;
        assert!(matches!(result, Err(AiError::Cancelled(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pre_cancelled_request_never_calls_provider() {
        let token = CancellationToken::new();
        token.cancel();
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let result = retry_async(&quick_policy(3), Some(&token), || {
            seen.fetch_add(1, Ordering::SeqCst);
            async { Ok::<(), AiError>(()) }
        })
        .await;
        assert!(matches!(result, Err(AiError::Cancelled(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
