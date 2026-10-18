//! One owner-bound cleanup worker per MCP client, independent of transport lifetime.
use super::server::DataZenMcpServer;
use datazen_application::{
    dto::requests::{CloseMode, CloseSessionRequest},
    sessions::ConnectionUseCases,
};
use datazen_platform_api::dto::session::ResourceRelease;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

impl DataZenMcpServer {
    pub(crate) fn start_session_cleanup(&self) {
        if self
            .sessions
            .worker
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let sessions = self.sessions.clone();
        let state = self.app_state.clone();
        let client = self.client_instance_id.clone();
        tokio::spawn(async move {
            let mut backoff = Duration::from_millis(100);
            let mut next_retry = Instant::now();
            loop {
                sessions.purge_expired(Instant::now());
                let pending = sessions.pending_cleanup();
                if !pending.is_empty() && Instant::now() >= next_retry {
                    if let Ok(adapter) = state.platform_require() {
                        if let Ok(runtime) = adapter.runtime() {
                            // Reconstruct only the backend-owned client identity. Never accept an
                            // owner or handle from the caller, or scan by connectionId.
                            let context = datazen_platform_api::context::RequestContext::new(
                                adapter.identity().organization_id().clone(),
                                adapter.identity().principal_id().clone(),
                                None,
                                datazen_platform_api::id::ClientInstanceId::new(client.clone()),
                                crate::platform::identity::new_request_id(),
                                None,
                            );
                            for binding in pending {
                                if binding.client_instance_id != client {
                                    continue;
                                }
                                // Runtime rolls back registered handles on their original resource
                                // and physically closes it; unknown effects remain unknown.
                                let result = runtime
                                    .close_session(
                                        &context,
                                        CloseSessionRequest::new(
                                            binding.handle,
                                            CloseMode::RollbackAndClose,
                                        ),
                                    )
                                    .await;
                                if matches!(result, Ok(receipt) if receipt.resource_release == ResourceRelease::Confirmed)
                                {
                                    sessions.confirm_cleanup(&binding.reference);
                                }
                                // Failure/Pending/Quarantined retains the cleanup owner. No SQL,
                                // arguments, credentials or driver error text is logged here.
                            }
                        }
                    }
                }
                let pending = !sessions.pending_cleanup().is_empty();
                if pending && Instant::now() >= next_retry {
                    next_retry = Instant::now() + backoff;
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                }
                let expiry = sessions.next_expiry();
                if !pending && expiry.is_none() {
                    sessions.worker.store(false, Ordering::Release);
                    // Pair the stop with admission's CAS: an open racing the stop either
                    // starts another worker or is picked up here, never two workers.
                    if (sessions.next_expiry().is_some() || !sessions.pending_cleanup().is_empty())
                        && sessions
                            .worker
                            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                    {
                        continue;
                    }
                    return;
                }
                let delay = if pending {
                    next_retry.saturating_duration_since(Instant::now())
                } else {
                    Duration::from_secs(3600)
                };
                let delay = expiry
                    .map(|at| at.saturating_duration_since(Instant::now()).min(delay))
                    .unwrap_or(delay);
                if !pending {
                    backoff = Duration::from_millis(100);
                }
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {},
                    _ = sessions.wake.notified() => {},
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mcp::{
            session::SessionBinding,
            types::{OpenSessionInput, UseSessionInput},
        },
        testing::{app_state::TestAppState, mock_driver::MockDriverOptions},
    };
    use rmcp::handler::server::wrapper::Parameters;
    use std::sync::Arc;

    async fn open(server: &DataZenMcpServer) -> SessionBinding {
        let reply = server
            .open_session(Parameters(OpenSessionInput {
                connection_id: "mcp-cleanup".into(),
                database: "app".into(),
                schema: Some("public".into()),
                ttl_ms: Some(600_000),
            }))
            .await
            .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        server
            .sessions
            .lookup(
                reply["clientSessionRef"].as_str().unwrap(),
                server.client_instance_id(),
                Instant::now(),
            )
            .unwrap()
    }
    async fn cleaned(server: &DataZenMcpServer) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !server.sessions.pending_cleanup().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("cleanup confirmed");
    }
    async fn assert_released(server: &DataZenMcpServer, binding: &SessionBinding) {
        let runtime = server
            .app_state
            .platform_require()
            .unwrap()
            .runtime()
            .unwrap();
        assert!(
            !runtime.is_registered(&datazen_runtime::connection::DbSessionId::new(
                binding.handle.db_session_id.as_str()
            ))
        );
        let snapshot = runtime.observability_snapshot().await.unwrap();
        assert_eq!(snapshot.budget_in_use, 0);
        assert_eq!(snapshot.in_use_total, 0);
    }
    fn use_sql(binding: &SessionBinding, sql: &str) -> Parameters<UseSessionInput> {
        Parameters(UseSessionInput {
            client_session_ref: binding.reference.clone(),
            command: "execute".into(),
            input: serde_json::json!({"sql": sql}),
            timeout_ms: None,
        })
    }

    #[tokio::test]
    async fn ttl_revokes_at_one_millisecond_and_releases_runtime_without_another_call() {
        let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
        test.save_connection("mcp-cleanup").await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_allowed_connections(&["mcp-cleanup".into()]);
        use datazen_application::dto::requests::OpenSessionRequest;
        use datazen_platform_api::{
            context::OwnerRef,
            dto::idempotency::IdempotentOperation,
            id::ConnectionId,
            target::{ExecutionTarget, NamespaceTarget},
        };
        let adapter = server.app_state.platform_require().unwrap();
        let runtime = adapter.runtime().unwrap();
        let context = server.request_context(adapter.identity());
        let token = runtime
            .issue_submission_token(&context, IdempotentOperation::OpenSession, None)
            .await
            .unwrap();
        let receipt = runtime
            .open_session(
                &context,
                OpenSessionRequest::new(
                    ExecutionTarget::new(
                        ConnectionId::new("mcp-cleanup"),
                        NamespaceTarget::database("app"),
                    ),
                    OwnerRef::ClientSession {
                        client_instance_id: context.client_instance_id.clone(),
                        purpose: super::super::server::MCP_SESSION_PURPOSE.into(),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .unwrap();
        let now = Instant::now();
        let binding = server.sessions.open(
            server.client_instance_id(),
            "mcp-cleanup",
            receipt.session.handle,
            receipt.session.context_revision,
            Duration::from_millis(1),
            now,
        );
        // Inject exactly the deadline; lookup must revoke and retain the owner,
        // even if a caller discards purge_expired's return value.
        assert!(server
            .sessions
            .lookup(
                &binding.reference,
                server.client_instance_id(),
                now + Duration::from_millis(1)
            )
            .is_err());
        server
            .sessions
            .purge_expired(now + Duration::from_millis(1));
        assert_eq!(server.sessions.pending_cleanup(), vec![binding.clone()]);
        server.start_session_cleanup();
        cleaned(&server).await;
        assert_released(&server, &binding).await;
    }

    #[tokio::test]
    async fn timer_reclaims_a_one_ms_session_without_any_followup_tool_call() {
        let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
        test.save_connection("mcp-cleanup").await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_allowed_connections(&["mcp-cleanup".into()]);
        let runtime = server
            .app_state
            .platform_require()
            .unwrap()
            .runtime()
            .unwrap();
        let reply = server
            .open_session(Parameters(OpenSessionInput {
                connection_id: "mcp-cleanup".into(),
                database: "app".into(),
                schema: Some("public".into()),
                ttl_ms: Some(1),
            }))
            .await
            .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !runtime.registered_session_ids().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(server
            .sessions
            .lookup(
                reply["clientSessionRef"].as_str().unwrap(),
                server.client_instance_id(),
                Instant::now()
            )
            .is_err());
        assert_eq!(
            runtime
                .observability_snapshot()
                .await
                .unwrap()
                .budget_in_use,
            0
        );
    }

    #[tokio::test]
    async fn disconnect_rolls_back_registered_transaction_and_does_not_close_other_client() {
        let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
        test.save_connection("mcp-cleanup").await;
        let mock = test.mock.clone();
        let state = Arc::new(test.state);
        let server =
            DataZenMcpServer::new(state.clone()).with_allowed_connections(&["mcp-cleanup".into()]);
        let other = DataZenMcpServer::new(state).with_allowed_connections(&["mcp-cleanup".into()]);
        let binding = open(&server).await;
        let foreign = open(&other).await;
        server
            .use_session(use_sql(&binding, "BEGIN"))
            .await
            .unwrap();
        assert_eq!(mock.open_transaction_count(), 1);
        let runtime = server
            .app_state
            .platform_require()
            .unwrap()
            .runtime()
            .unwrap();
        let context =
            server.request_context(server.app_state.platform_require().unwrap().identity());
        assert!(runtime
            .close_session(
                &context,
                CloseSessionRequest::new(binding.handle.clone(), CloseMode::RequireNoTransaction)
            )
            .await
            .is_err());
        server.release_sessions().await;
        assert!(server
            .use_session(use_sql(&binding, "SELECT 1"))
            .await
            .unwrap_err()
            .to_string()
            .contains("SessionLost"));
        cleaned(&server).await;
        assert_eq!(mock.open_transaction_count(), 0);
        assert_eq!(mock.commit_calls(), 0);
        assert!(
            !runtime.is_registered(&datazen_runtime::connection::DbSessionId::new(
                binding.handle.db_session_id.as_str()
            ))
        );
        assert!(
            runtime.is_registered(&datazen_runtime::connection::DbSessionId::new(
                foreign.handle.db_session_id.as_str()
            ))
        );
        assert!(other
            .sessions
            .lookup(
                &foreign.reference,
                other.client_instance_id(),
                Instant::now()
            )
            .is_ok());
        other.release_sessions().await;
        cleaned(&other).await;
        assert_released(&server, &binding).await;
    }

    #[tokio::test]
    async fn unknown_release_keeps_cleanup_owner_and_repeated_disconnect_has_one_worker() {
        let test = TestAppState::with_resource_provider(MockDriverOptions {
            disconnect_error: Some("synthetic close failure".into()),
            ..Default::default()
        })
        .await;
        test.save_connection("mcp-cleanup").await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_allowed_connections(&["mcp-cleanup".into()]);
        let binding = open(&server).await;
        server.release_sessions().await;
        for _ in 0..10 {
            server.release_sessions().await;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(server.sessions.pending_cleanup(), vec![binding.clone()]);
        assert!(server.sessions.worker.load(Ordering::Acquire));
        assert!(server
            .sessions
            .lookup(
                &binding.reference,
                server.client_instance_id(),
                Instant::now()
            )
            .is_err());
        // Runtime has revoked execution access even though physical release cannot
        // be confirmed. MCP must retain the cleanup owner instead of claiming success.
        let runtime = server
            .app_state
            .platform_require()
            .unwrap()
            .runtime()
            .unwrap();
        assert!(
            !runtime.is_registered(&datazen_runtime::connection::DbSessionId::new(
                binding.handle.db_session_id.as_str()
            ))
        );
        let adapter = server.app_state.platform_require().unwrap();
        let receipt = runtime
            .close_session(
                &server.request_context(adapter.identity()),
                CloseSessionRequest::new(binding.handle, CloseMode::RollbackAndClose),
            )
            .await
            .unwrap();
        assert_eq!(receipt.resource_release, ResourceRelease::Quarantined);
        assert_eq!(
            receipt.effect_outcome,
            datazen_platform_api::dto::execution::EffectOutcome::Unknown
        );
        // Physical capacity is held while release is undecidable: MCP must not
        // invent a free budget slot or erase the retained cleanup record.
        let snapshot = runtime.observability_snapshot().await.unwrap();
        assert!(snapshot.budget_in_use > 0);
    }
}
