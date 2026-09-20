//! Cancellation registry for AI streaming requests.
//!
//! Maps `request_id` → `CancellationToken` so that an IPC `ai_cancel` call
//! can interrupt an in-flight stream.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Thread-safe registry that tracks active streaming requests by their
/// `request_id` and allows external cancellation via IPC.
#[derive(Clone, Default)]
pub struct CancellationRegistry {
    inner: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl CancellationRegistry {
    /// Register a new request and return its cancellation token.
    ///
    /// The caller should hold the token and check `token.is_cancelled()`
    /// (or `token.cancelled().await`) at appropriate points in the stream.
    pub async fn register(&self, request_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.inner
            .lock()
            .await
            .insert(request_id.to_string(), token.clone());
        token
    }

    /// Cancel a registered request. Returns `true` if the request was found
    /// and cancelled, `false` if it was not registered (already finished or
    /// never started).
    pub async fn cancel(&self, request_id: &str) -> bool {
        if let Some(token) = self.inner.lock().await.remove(request_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Remove a request from the registry without cancelling it.
    /// Called when a request completes normally.
    pub async fn unregister(&self, request_id: &str) {
        self.inner.lock().await.remove(request_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn register_returns_distinct_tokens() {
        let reg = CancellationRegistry::default();
        let t1 = reg.register("req-1").await;
        let t2 = reg.register("req-2").await;
        assert!(!t1.is_cancelled());
        assert!(!t2.is_cancelled());
        // Cancelling one should not affect the other
        reg.cancel("req-1").await;
        assert!(t1.is_cancelled());
        assert!(!t2.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_returns_true_only_for_registered() {
        let reg = CancellationRegistry::default();
        assert!(!reg.cancel("unknown").await);
        reg.register("r1").await;
        assert!(reg.cancel("r1").await);
        // Double-cancel returns false (already removed)
        assert!(!reg.cancel("r1").await);
    }

    #[tokio::test]
    async fn unregister_removes_without_cancelling() {
        let reg = CancellationRegistry::default();
        let token = reg.register("r1").await;
        reg.unregister("r1").await;
        assert!(!token.is_cancelled());
        assert!(!reg.cancel("r1").await);
    }

    #[tokio::test]
    async fn cancelled_token_is_detected() {
        let reg = CancellationRegistry::default();
        let token = reg.register("r1").await;

        // Spawn a task that waits for cancellation
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = token.cancelled() => true,
                _ = tokio::time::sleep(Duration::from_secs(10)) => false,
            }
        });

        // Give the task time to start
        tokio::time::sleep(Duration::from_millis(10)).await;
        reg.cancel("r1").await;
        let result = handle.await.unwrap();
        assert!(result);
    }
}
