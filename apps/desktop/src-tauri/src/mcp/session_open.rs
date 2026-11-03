//! Acquisition ownership survives cancellation of an MCP tool request.
//!
//! M2 主体归位后本文件是薄壳：完成语义（守卫、盖章注册、断连拒收）在
//! `datazen::mcp::session_tools::complete_session_open`；此处只做传输错误
//! 映射，并保留旧方法名供传输生命周期与既有测试调用。
use super::server::DataZenMcpServer;
use datazen::execution::OpenSessionReceipt;
use datazen::mcp::session_tools::{SessionToolError, StatefulSessionHost};
use datazen::shared::error::ApiError;
use rmcp::ErrorData as McpError;
use std::{future::Future, sync::Arc, time::Duration};

impl DataZenMcpServer {
    pub(crate) async fn complete_session_open(
        &self,
        connection_id: String,
        ttl: Duration,
        acquisition: impl Future<Output = Result<OpenSessionReceipt, ApiError>> + Send + 'static,
    ) -> Result<String, McpError> {
        let host: Arc<dyn StatefulSessionHost> = Arc::new(self.clone());
        datazen::mcp::session_tools::complete_session_open(
            host,
            self.sessions.clone(),
            self.client_instance_id.clone(),
            connection_id,
            ttl,
            acquisition,
        )
        .await
        .map_err(|error| match error {
            SessionToolError::Plain(message) => McpError::invalid_params(message, None),
            SessionToolError::WithHelp(message) | SessionToolError::Internal(message) => {
                DataZenMcpServer::map_err(message)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mcp::types::{CloseSessionInput, OpenSessionInput},
        testing::{app_state_test_support::TestAppState, mock_driver::MockDriverOptions},
    };
    use datazen::application::sessions::ConnectionUseCases;
    use datazen::execution::application::requests::OpenSessionRequest;
    use datazen::{
        execution::application::idempotency::IdempotentOperation,
        execution::target::{ExecutionTarget, NamespaceTarget},
        shared::context::OwnerRef,
        shared::id::ConnectionId,
    };
    use rmcp::handler::server::wrapper::Parameters;
    use std::time::Instant;

    async fn acquisition(
        server: &DataZenMcpServer,
    ) -> impl Future<Output = Result<OpenSessionReceipt, ApiError>> + Send + 'static {
        let adapter = server.app_state.platform_require().unwrap();
        let runtime = adapter.runtime().unwrap();
        let context = server.request_context(adapter.identity());
        let token = runtime
            .issue_submission_token(&context, IdempotentOperation::OpenSession, None)
            .await
            .unwrap();
        let request = OpenSessionRequest::new(
            ExecutionTarget::new(
                ConnectionId::new("mcp-cancel-open"),
                NamespaceTarget::database("app"),
            ),
            OwnerRef::ClientSession {
                client_instance_id: context.client_instance_id.clone(),
                purpose: super::super::server::MCP_SESSION_PURPOSE.into(),
            },
            token.idempotency_key,
        );
        async move { runtime.open_session(&context, request).await }
    }
    async fn server() -> (TestAppState, DataZenMcpServer) {
        let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
        test.save_connection("mcp-cancel-open").await;
        let server = DataZenMcpServer::new(Arc::new(test.state.clone()))
            .with_allowed_connections(&["mcp-cancel-open".into()]);
        (test, server)
    }
    async fn wait_empty(server: &DataZenMcpServer) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let snapshot = server
                    .app_state
                    .platform_require()
                    .unwrap()
                    .runtime()
                    .unwrap()
                    .observability_snapshot()
                    .await
                    .unwrap();
                if server
                    .app_state
                    .platform_require()
                    .unwrap()
                    .runtime()
                    .unwrap()
                    .registered_session_ids()
                    .is_empty()
                    && server.sessions.is_empty()
                    && server.sessions.pending_cleanup().is_empty()
                    && snapshot.in_use_total == 0
                    && snapshot.budget_in_use == 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("late resource and budget released");
    }
    #[tokio::test]
    async fn cancelled_request_retains_late_acquisition_for_cleanup_without_disconnect() {
        let (_test, server) = server().await;
        let acquisition = acquisition(&server).await;
        let gate = Arc::new(tokio::sync::Notify::new());
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (acquired, opened) = tokio::sync::oneshot::channel();
        let task = tokio::spawn({
            let server = server.clone();
            let gate = gate.clone();
            async move {
                server
                    .complete_session_open(
                        "mcp-cancel-open".into(),
                        Duration::from_secs(3600),
                        async move {
                            let _ = entered.send(());
                            gate.notified().await;
                            let receipt = acquisition.await;
                            let _ = acquired.send(receipt.is_ok());
                            receipt
                        },
                    )
                    .await
            }
        });
        ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        gate.notify_one();
        assert!(opened.await.unwrap(), "late Runtime acquisition completed");
        wait_empty(&server).await;
    }
    #[tokio::test]
    async fn disconnect_during_open_rejects_late_reply_and_cleans_original_owner() {
        let (_test, server) = server().await;
        let acquisition = acquisition(&server).await;
        let gate = Arc::new(tokio::sync::Notify::new());
        let (entered, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn({
            let server = server.clone();
            let gate = gate.clone();
            async move {
                server
                    .complete_session_open(
                        "mcp-cancel-open".into(),
                        Duration::from_secs(3600),
                        async move {
                            let _ = entered.send(());
                            gate.notified().await;
                            acquisition.await
                        },
                    )
                    .await
            }
        });
        ready.await.unwrap();
        server.release_sessions().await;
        gate.notify_one();
        assert!(task
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("SessionLost"));
        wait_empty(&server).await;
    }
    #[tokio::test]
    async fn delivered_open_stays_usable_until_explicit_idempotent_close() {
        let (_test, server) = server().await;
        let reply = server
            .open_session(Parameters(OpenSessionInput {
                connection_id: "mcp-cancel-open".into(),
                database: "app".into(),
                schema: None,
                ttl_ms: None,
            }))
            .await
            .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        let reference = reply["clientSessionRef"].as_str().unwrap().to_owned();
        assert!(server
            .sessions
            .lookup(&reference, server.client_instance_id(), Instant::now())
            .is_ok());
        for expected in [true, false] {
            let reply = server
                .close_session(Parameters(CloseSessionInput {
                    client_session_ref: reference.clone(),
                    mode: None,
                }))
                .await
                .unwrap();
            let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
            assert_eq!(reply["closed"], expected);
        }
        wait_empty(&server).await;
    }
}
