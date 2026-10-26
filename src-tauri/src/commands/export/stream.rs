//! Host-only lossless adapter from one Runtime query to the existing formatter.
use super::*;
use datazen_application::{dto::requests::*, sessions::ConnectionUseCases};
use datazen_modules::export::ExportError as FeatureError;
use datazen_platform_api::{
    dto::{execution::ExecutionState, idempotency::IdempotentOperation, session::ResourceRelease},
    id::{ConfigRevision, ConnectionId, Counter, ExecutionId},
    target::{ExecutionTarget, NamespaceTarget},
    OwnerRef, RequestContext,
};
use datazen_runtime::application::RuntimeConnectionUseCases;

fn unavailable() -> FeatureError {
    FeatureError::Validation("runtimeExportUnavailable".into())
}
struct Cleanup {
    runtime: Arc<RuntimeConnectionUseCases>,
    ctx: RequestContext,
    handle: Option<datazen_platform_api::dto::session::SessionHandle>,
    execution: Option<ExecutionId>,
    watcher: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            watcher.abort();
        }
        let Some(handle) = self.handle.take() else {
            return;
        };
        let runtime = self.runtime.clone();
        let ctx = self.ctx.clone();
        let execution = self.execution.take();
        if let Ok(task) = tokio::runtime::Handle::try_current() {
            task.spawn(async move {
                if let Some(id) = execution {
                    let _ = runtime.cancel_execution(&ctx, id).await;
                }
                // Close retains uncertain resources/budget in the Runtime owner.
                let _ = runtime
                    .close_session(
                        &ctx,
                        CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                    )
                    .await;
            });
        }
    }
}
/// The export plan generates one SELECT per table. Unexpected statements,
/// truncated results, or missing metadata fail the entire staging artifact.
#[derive(Default)]
struct Delivery {
    started: bool,
    ended: bool,
}
impl Delivery {
    async fn deliver(
        &mut self,
        value: serde_json::Value,
        sql: &str,
        callback: &datazen_driver_api::QueryStreamCallback,
    ) -> Result<(), FeatureError> {
        // Formatting/file writes never occupy a Runtime reactor worker. Await
        // each write before receiving another batch: no detached producer queue.
        let mut delivery = std::mem::take(self);
        let callback = callback.clone();
        let sql = sql.to_owned();
        let (next, result) = tokio::task::spawn_blocking(move || {
            let result = delivery.accept(value, &sql, &callback);
            (delivery, result)
        })
        .await
        .map_err(|_| unavailable())?;
        *self = next;
        result
    }
    fn accept(
        &mut self,
        value: serde_json::Value,
        sql: &str,
        callback: &datazen_driver_api::QueryStreamCallback,
    ) -> Result<(), FeatureError> {
        use datazen_driver_api::QueryStreamEvent;
        if value["statementIndex"].as_u64() != Some(0) {
            return Err(unavailable());
        }
        match value["kind"].as_str() {
            Some("statementColumns") | Some("statementMetadata") => {
                if value["truncated"].as_bool() == Some(true) {
                    return Err(unavailable());
                }
                if !self.started {
                    let columns = serde_json::from_value(value["columns"].clone())
                        .map_err(|_| unavailable())?;
                    callback(QueryStreamEvent::StatementStart {
                        index: 0,
                        sql: sql.into(),
                        columns,
                    });
                    self.started = true;
                }
                if value["kind"] == "statementMetadata" && !self.ended {
                    callback(QueryStreamEvent::StatementEnd {
                        index: 0,
                        rows_affected: value["rowsAffected"].as_u64(),
                        execution_time_ms: 0,
                        truncated: false,
                    });
                    self.ended = true;
                }
            }
            Some("rows") if self.started && !self.ended => {
                let rows =
                    serde_json::from_value(value["rows"].clone()).map_err(|_| unavailable())?;
                callback(QueryStreamEvent::Rows { index: 0, rows });
            }
            Some("statementSummary") if !self.ended => {}
            _ => return Err(unavailable()),
        }
        Ok(())
    }
}
impl DesktopExportSource {
    pub(super) async fn stream_runtime_query(
        &self,
        id: &str,
        sql: &str,
        namespace: datazen_driver_api::SqlTarget<'_>,
        callback: datazen_driver_api::QueryStreamCallback,
    ) -> Result<(), FeatureError> {
        if self.cancel.is_cancelled() {
            return Err(unavailable());
        }
        let (connection, config, revision) = self
            .runtime
            .profile_for_session(id)
            .await
            .map_err(|_| unavailable())?;
        // Only these ResourceProvider implementations have the audited bounded,
        // awaitable SQL row-stream/drain path. Other drivers are not claimed.
        if !matches!(
            config.database_type.as_str(),
            "postgres" | "postgresql" | "mysql"
        ) {
            return Err(unavailable());
        }
        let adapter = self.platform.require().map_err(|_| unavailable())?;
        let runtime = adapter.runtime().map_err(|_| unavailable())?;
        let ctx = adapter.begin_call().context().clone();
        let target = ExecutionTarget::new(
            ConnectionId::new(connection),
            NamespaceTarget::new(
                namespace
                    .database()
                    .map(str::to_owned)
                    .or(config.database.clone()),
                None,
                namespace
                    .schema()
                    .map(str::to_owned)
                    .or(config.schema.clone()),
                vec![],
            ),
        );
        let canonical = adapter
            .resolve_execution_target(&ctx, Counter::new(revision), "query", &target)
            .await
            .map_err(|_| unavailable())?;
        let ns = canonical.namespace;
        let target = ExecutionTarget::new(
            canonical.connection_id,
            NamespaceTarget::new(
                ns.database.map(|v| v.as_str().to_owned()),
                ns.catalog.map(|v| v.as_str().to_owned()),
                ns.schema.map(|v| v.as_str().to_owned()),
                ns.path.into_iter().map(|v| v.as_str().to_owned()).collect(),
            ),
        );
        let token = runtime
            .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
            .await
            .map_err(|_| unavailable())?;
        let opened = runtime
            .open_session(
                &ctx,
                OpenSessionRequest::new(
                    target.clone(),
                    OwnerRef::ClientSession {
                        client_instance_id: ctx.client_instance_id.clone(),
                        purpose: "tableExport".into(),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .map_err(|_| unavailable())?;
        let handle = opened.session.handle;
        let mut cleanup = Cleanup {
            runtime: runtime.clone(),
            ctx: ctx.clone(),
            handle: Some(handle.clone()),
            execution: None,
            watcher: None,
        };
        let result = async {
            let mut binding = runtime.bind_export_output(&ctx, &handle, &target, ConfigRevision::new(revision)).await.map_err(|_| unavailable())?;
            let token = runtime.issue_submission_token(&ctx, IdempotentOperation::ExecuteInSession, Some(&handle)).await.map_err(|_| unavailable())?;
            let receipt = runtime.execute_in_session(&ctx, ExecuteInSessionRequest::new(handle.clone(), CommandCall::new("query", serde_json::json!({"sql":sql})), token.idempotency_key)).await.map_err(|_| unavailable())?;
            cleanup.execution = Some(receipt.execution_id.clone());
            let revoke = binding.revoke_handle();
            let cancelled = self.cancel.clone();
            let watch_runtime = runtime.clone();
            let watch_ctx = ctx.clone();
            let execution = receipt.execution_id.clone();
            cleanup.watcher = Some(tokio::spawn(async move {
                while !cancelled.is_cancelled() { tokio::time::sleep(std::time::Duration::from_millis(20)).await; }
                revoke(); // wake a blocked producer before requesting physical cancellation
                let _ = watch_runtime.cancel_execution(&watch_ctx, execution).await;
            }));
            let mut delivery = Delivery::default();
            loop {
                if self.cancel.is_cancelled() { return Err(unavailable()); }
                tokio::select! {
                    value = binding.recv() => {
                        binding.validate(&runtime).await.map_err(|_| unavailable())?;
                        if self.cancel.is_cancelled() { return Err(unavailable()); }
                        delivery.deliver(value.ok_or_else(unavailable)?, sql, &callback).await?;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                        let view = runtime.get_execution(&ctx, receipt.execution_id.clone()).await.map_err(|_| unavailable())?;
                        if view.state.is_terminal() {
                            if view.state != ExecutionState::Succeeded || view.effect_outcome == datazen_platform_api::dto::execution::EffectOutcome::Unknown { return Err(unavailable()); }
                            // Terminal is set only after every producer send completed.
                            // Drain the one queued batch before publishing Done.
                            while let Ok(value) = binding.try_recv() {
                                binding.validate(&runtime).await.map_err(|_| unavailable())?;
                                if self.cancel.is_cancelled() { return Err(unavailable()); }
                                delivery.deliver(value, sql, &callback).await?;
                            }
                            if !delivery.started || !delivery.ended { return Err(unavailable()); }
                            return Ok(());
                        }
                    }
                }
            }
        }.await;
        if let Some(watcher) = cleanup.watcher.take() {
            watcher.abort();
        }
        if result.is_err() {
            if let Some(id) = cleanup.execution.clone() {
                let _ = runtime.cancel_execution(&ctx, id).await;
            }
        }
        let closed = runtime
            .close_session(
                &ctx,
                CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
            )
            .await;
        cleanup.handle = None;
        if !matches!(closed, Ok(ref receipt) if receipt.resource_release == ResourceRelease::Confirmed)
        {
            return Err(unavailable());
        }
        result?;
        if self.cancel.is_cancelled() {
            return Err(unavailable());
        }
        callback(datazen_driver_api::QueryStreamEvent::Done { total_time_ms: 0 });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::QueryStreamEvent;
    use std::sync::Mutex;

    #[tokio::test]
    async fn delivery_keeps_column_row_end_order_and_ignores_completion_metadata_repeat() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let observed = events.clone();
        let callback: datazen_driver_api::QueryStreamCallback =
            Arc::new(move |event| observed.lock().unwrap().push(event));
        let mut delivery = Delivery::default();
        for value in [
            serde_json::json!({"kind":"statementColumns","statementIndex":0,"columns":[{"name":"value","dataType":"int","nullable":false}]}),
            serde_json::json!({"kind":"rows","statementIndex":0,"rows":[[null]]}),
            serde_json::json!({"kind":"statementMetadata","statementIndex":0,"columns":[],"truncated":false}),
            serde_json::json!({"kind":"statementMetadata","statementIndex":0,"columns":[],"truncated":false}),
        ] {
            delivery
                .deliver(value, "SELECT value", &callback)
                .await
                .unwrap();
        }
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], QueryStreamEvent::StatementStart { .. }));
        assert!(matches!(events[1], QueryStreamEvent::Rows { .. }));
        assert!(matches!(events[2], QueryStreamEvent::StatementEnd { .. }));
    }

    #[tokio::test]
    async fn malformed_truncated_and_extra_statement_fail_before_success() {
        let callback: datazen_driver_api::QueryStreamCallback = Arc::new(|_| {});
        for value in [
            serde_json::json!({"kind":"rows","statementIndex":0,"rows":[]}),
            serde_json::json!({"kind":"statementMetadata","statementIndex":0,"columns":[],"truncated":true}),
            serde_json::json!({"kind":"statementColumns","statementIndex":1,"columns":[]}),
            serde_json::json!({"kind":"executionError","message":"fixed test error"}),
        ] {
            assert!(Delivery::default()
                .deliver(value, "SELECT 1", &callback)
                .await
                .is_err());
        }
    }

    #[tokio::test]
    async fn formatter_panic_fails_the_adapter_instead_of_returning_done() {
        let callback: datazen_driver_api::QueryStreamCallback =
            Arc::new(|_| panic!("controlled formatter fault"));
        assert!(Delivery::default()
            .deliver(
                serde_json::json!({"kind":"statementColumns","statementIndex":0,"columns":[]}),
                "SELECT 1",
                &callback
            )
            .await
            .is_err());
    }
}
