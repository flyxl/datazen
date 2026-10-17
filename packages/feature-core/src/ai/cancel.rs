//! Cancellation registry for AI streaming requests.
//!
//! Maps request_id to its owning identity plus a CancellationToken so that an
//! IPC ai_cancel call can interrupt an in-flight stream **without being able to
//! cancel another client's request**. Cancellation is two-phase: the flag is
//! recorded first, then the token fires, so the tool loop can converge on a
//! truthful terminal state ("cancelled") instead of pretending the stream
//! finished normally.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Receipt returned to the caller of ai_cancel. Cancellation is not the same
/// thing as "the request finished", so every outcome is named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum CancelReceipt {
    /// Owner matched: the cancel flag is recorded and the token fired. The
    /// stream converges on a terminal chunk with "cancelled": true.
    Marked { request_id: String },
    /// The request is not registered: it already finished, was already
    /// cancelled, or never existed.
    AlreadyFinished { request_id: String },
    /// A request with that id exists but belongs to another owner. The other
    /// owner's request is left untouched.
    Forbidden { request_id: String },
}

impl CancelReceipt {
    pub fn request_id(&self) -> &str {
        match self {
            Self::Marked { request_id }
            | Self::AlreadyFinished { request_id }
            | Self::Forbidden { request_id } => request_id,
        }
    }
}

#[derive(Debug, Clone)]
struct RegisteredRequest {
    owner_fingerprint: String,
    token: CancellationToken,
}

/// Thread-safe registry tracking active streaming requests by request_id.
#[derive(Clone, Default)]
pub struct CancellationRegistry {
    inner: Arc<Mutex<HashMap<String, RegisteredRequest>>>,
}

impl CancellationRegistry {
    /// Register a request under its owner fingerprint and return the token.
    ///
    /// Re-registering the same request_id replaces the previous entry: request
    /// ids are minted per call by the caller, and a stale entry must not be able
    /// to cancel a newer request.
    pub async fn register(&self, request_id: &str, owner_fingerprint: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.inner.lock().await.insert(
            request_id.to_string(),
            RegisteredRequest {
                owner_fingerprint: owner_fingerprint.to_string(),
                token: token.clone(),
            },
        );
        token
    }

    /// Cancel a registered request after checking ownership.
    pub async fn cancel(&self, request_id: &str, owner_fingerprint: &str) -> CancelReceipt {
        let mut guard = self.inner.lock().await;
        let Some(entry) = guard.get(request_id) else {
            return CancelReceipt::AlreadyFinished {
                request_id: request_id.to_string(),
            };
        };
        if entry.owner_fingerprint != owner_fingerprint {
            return CancelReceipt::Forbidden {
                request_id: request_id.to_string(),
            };
        }
        // Two-phase: record the flag before firing the token so a consumer that
        // only observes the flag still reports cancellation truthfully.
        entry.token.cancel();
        guard.remove(request_id);
        CancelReceipt::Marked {
            request_id: request_id.to_string(),
        }
    }

    /// Remove a finished request. A mismatched owner cannot unregister another
    /// owner's entry, which keeps a late completion from masking a live request
    /// that happens to reuse the id.
    pub async fn unregister(&self, request_id: &str, owner_fingerprint: &str) -> bool {
        let mut guard = self.inner.lock().await;
        match guard.get(request_id) {
            Some(entry) if entry.owner_fingerprint == owner_fingerprint => {
                guard.remove(request_id);
                true
            }
            _ => false,
        }
    }

    /// Cancel every request owned by one identity. Used when a client keeps
    /// going away: the owner's own streams stop, nobody else's are touched.
    pub async fn cancel_owner(&self, owner_fingerprint: &str) -> usize {
        let mut guard = self.inner.lock().await;
        let owned: Vec<String> = guard
            .iter()
            .filter(|(_, entry)| entry.owner_fingerprint == owner_fingerprint)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &owned {
            if let Some(entry) = guard.remove(id) {
                entry.token.cancel();
            }
        }
        owned.len()
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn owner_of(&self, request_id: &str) -> Option<String> {
        self.inner
            .lock()
            .await
            .get(request_id)
            .map(|entry| entry.owner_fingerprint.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const OWNER: &str = "client:alpha";
    const OTHER: &str = "client:beta";

    #[tokio::test]
    async fn register_returns_distinct_tokens() {
        let reg = CancellationRegistry::default();
        let t1 = reg.register("req-1", OWNER).await;
        let t2 = reg.register("req-2", OWNER).await;
        assert!(!t1.is_cancelled());
        assert!(!t2.is_cancelled());
        assert_eq!(
            reg.cancel("req-1", OWNER).await,
            CancelReceipt::Marked {
                request_id: "req-1".into()
            }
        );
        assert!(t1.is_cancelled());
        assert!(!t2.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_returns_receipts_for_each_outcome() {
        let reg = CancellationRegistry::default();
        assert_eq!(
            reg.cancel("unknown", OWNER).await,
            CancelReceipt::AlreadyFinished {
                request_id: "unknown".into()
            }
        );
        reg.register("r1", OWNER).await;
        assert_eq!(
            reg.cancel("r1", OWNER).await,
            CancelReceipt::Marked {
                request_id: "r1".into()
            }
        );
        assert_eq!(
            reg.cancel("r1", OWNER).await,
            CancelReceipt::AlreadyFinished {
                request_id: "r1".into()
            }
        );
    }

    #[tokio::test]
    async fn another_owner_cannot_cancel_or_unregister() {
        let reg = CancellationRegistry::default();
        let token = reg.register("r1", OWNER).await;
        assert_eq!(
            reg.cancel("r1", OTHER).await,
            CancelReceipt::Forbidden {
                request_id: "r1".into()
            }
        );
        assert!(
            !token.is_cancelled(),
            "forged cancel must not fire the token"
        );
        assert!(!reg.unregister("r1", OTHER).await);
        assert_eq!(reg.owner_of("r1").await.as_deref(), Some(OWNER));
        assert!(reg.unregister("r1", OWNER).await);
        assert!(reg.is_empty().await);
    }

    #[tokio::test]
    async fn unregister_removes_without_cancelling() {
        let reg = CancellationRegistry::default();
        let token = reg.register("r1", OWNER).await;
        assert!(reg.unregister("r1", OWNER).await);
        assert!(!token.is_cancelled());
        assert_eq!(
            reg.cancel("r1", OWNER).await,
            CancelReceipt::AlreadyFinished {
                request_id: "r1".into()
            }
        );
    }

    #[tokio::test]
    async fn cancel_owner_only_touches_that_owner() {
        let reg = CancellationRegistry::default();
        let mine = reg.register("mine", OWNER).await;
        let theirs = reg.register("theirs", OTHER).await;
        assert_eq!(reg.cancel_owner(OWNER).await, 1);
        assert!(mine.is_cancelled());
        assert!(!theirs.is_cancelled());
        assert_eq!(reg.len().await, 1);
        assert_eq!(reg.owner_of("theirs").await.as_deref(), Some(OTHER));
    }

    #[tokio::test]
    async fn cancelled_token_is_detected() {
        let reg = CancellationRegistry::default();
        let token = reg.register("r1", OWNER).await;
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = token.cancelled() => true,
                _ = tokio::time::sleep(Duration::from_secs(10)) => false,
            }
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        reg.cancel("r1", OWNER).await;
        let result = handle.await.expect("join");
        assert!(result);
    }
}
