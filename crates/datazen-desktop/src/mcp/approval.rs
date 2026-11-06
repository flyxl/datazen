//! Desktop presentation for MCP per-call approvals.
//!
//! The MCP server pauses a non-read-only/high-risk call here. The broker emits
//! `mcp:approval-requested` to the main window and waits for
//! `mcp_respond_approval` / `mcp_cancel_approval`. A missing UI handle, an emit
//! failure, or a five-minute expiry all resolve to a non-allow decision, so the
//! tool body never runs without an explicit Allow once.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use datazen::mcp::approval::{McpApprovalDecision, McpApprovalPort, McpApprovalRequest};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

pub const MCP_APPROVAL_EVENT: &str = "mcp:approval-requested";
pub const MCP_AUDIT_ERROR_EVENT: &str = "mcp:audit-error";
pub const MCP_APPROVAL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

type PendingMap = HashMap<String, oneshot::Sender<McpApprovalDecision>>;

trait ApprovalPresenter: Send + Sync {
    fn present(&self, request: &McpApprovalRequest) -> Result<(), String>;
}

struct TauriApprovalPresenter(AppHandle);

impl ApprovalPresenter for TauriApprovalPresenter {
    fn present(&self, request: &McpApprovalRequest) -> Result<(), String> {
        self.0
            .emit(MCP_APPROVAL_EVENT, request)
            .map_err(|error| error.to_string())
    }
}

pub struct McpApprovalBroker {
    pending: Arc<Mutex<PendingMap>>,
    presenter: OnceLock<Arc<dyn ApprovalPresenter>>,
    timeout: Duration,
}

impl McpApprovalBroker {
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            presenter: OnceLock::new(),
            timeout,
        }
    }

    pub fn production() -> Self {
        Self::new(MCP_APPROVAL_TIMEOUT)
    }

    pub fn attach_app_handle(&self, handle: AppHandle) {
        let _ = self.presenter.set(Arc::new(TauriApprovalPresenter(handle)));
    }

    #[cfg(test)]
    fn install_test_presenter(&self) {
        struct Noop;
        impl ApprovalPresenter for Noop {
            fn present(&self, _request: &McpApprovalRequest) -> Result<(), String> {
                Ok(())
            }
        }
        let _ = self.presenter.set(Arc::new(Noop));
    }

    /// Resolve one pending request. Returns false for unknown/answered ids.
    pub fn respond(&self, request_id: &str, decision: McpApprovalDecision) -> bool {
        let sender = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(request_id));
        sender.is_some_and(|sender| sender.send(decision).is_ok())
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        self.respond(request_id, McpApprovalDecision::Cancel)
    }

    pub fn cancel_all(&self) -> usize {
        let senders: Vec<_> = self
            .pending
            .lock()
            .map(|mut pending| pending.drain().map(|(_, sender)| sender).collect())
            .unwrap_or_default();
        let mut cancelled = 0;
        for sender in senders {
            if sender.send(McpApprovalDecision::Cancel).is_ok() {
                cancelled += 1;
            }
        }
        cancelled
    }

    #[cfg(test)]
    pub fn pending_count(&self) -> usize {
        self.pending.lock().map(|p| p.len()).unwrap_or(0)
    }
}

struct PendingGuard {
    pending: Arc<Mutex<PendingMap>>,
    request_id: String,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&self.request_id);
        }
    }
}

#[async_trait]
impl McpApprovalPort for McpApprovalBroker {
    async fn decide(&self, request: &McpApprovalRequest) -> McpApprovalDecision {
        let Some(presenter) = self.presenter.get().cloned() else {
            return McpApprovalDecision::Deny;
        };
        let (sender, receiver) = oneshot::channel();
        let guard = PendingGuard {
            pending: self.pending.clone(),
            request_id: request.request_id.clone(),
        };
        {
            let Ok(mut pending) = self.pending.lock() else {
                return McpApprovalDecision::Deny;
            };
            pending.insert(request.request_id.clone(), sender);
        }
        if presenter.present(request).is_err() {
            return McpApprovalDecision::Deny;
        }
        let decision = tokio::select! {
            decision = receiver => decision.unwrap_or(McpApprovalDecision::Cancel),
            _ = tokio::time::sleep(self.timeout) => McpApprovalDecision::Timeout,
        };
        drop(guard);
        decision
    }
}

static BROKER: OnceLock<Arc<McpApprovalBroker>> = OnceLock::new();

pub fn approval_broker() -> Arc<McpApprovalBroker> {
    BROKER
        .get_or_init(|| Arc::new(McpApprovalBroker::production()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::mcp::approval::approval_preview;

    fn request(id: &str) -> McpApprovalRequest {
        approval_preview(
            id,
            "query",
            Some("c1"),
            None,
            None,
            Some("DELETE FROM t"),
            None,
            "now",
            "later",
            100,
        )
    }

    #[tokio::test]
    async fn deny_without_handle_never_allows() {
        let broker = McpApprovalBroker::new(Duration::from_millis(20));
        assert_eq!(
            broker.decide(&request("no-handle")).await,
            McpApprovalDecision::Deny
        );
        assert_eq!(broker.pending_count(), 0);
    }

    #[tokio::test]
    async fn timeout_denies_and_clears_pending() {
        let broker = Arc::new(McpApprovalBroker::new(Duration::from_millis(30)));
        broker.install_test_presenter();
        let broker_for_task = broker.clone();
        let task = tokio::spawn(async move { broker_for_task.decide(&request("timeout")).await });
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(broker.pending_count(), 1);
        assert_eq!(task.await.unwrap(), McpApprovalDecision::Timeout);
        assert_eq!(broker.pending_count(), 0);
    }

    #[tokio::test]
    async fn cancel_resolves_pending_as_cancel() {
        let broker = Arc::new(McpApprovalBroker::new(Duration::from_secs(5)));
        broker.install_test_presenter();
        let broker_for_task = broker.clone();
        let task = tokio::spawn(async move { broker_for_task.decide(&request("cancel")).await });
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(broker.cancel("cancel"));
        assert_eq!(task.await.unwrap(), McpApprovalDecision::Cancel);
    }

    #[tokio::test]
    async fn allow_once_resolves_pending_as_allow() {
        let broker = Arc::new(McpApprovalBroker::new(Duration::from_secs(5)));
        broker.install_test_presenter();
        let broker_for_task = broker.clone();
        let task = tokio::spawn(async move { broker_for_task.decide(&request("allow")).await });
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(broker.respond("allow", McpApprovalDecision::AllowOnce));
        assert_eq!(task.await.unwrap(), McpApprovalDecision::AllowOnce);
    }
}
