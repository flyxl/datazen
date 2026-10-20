//! In-process table-export capability. No transport tokens, file paths, or new
//! event protocol: one dedicated session, one query, one bounded output slot.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

pub(super) struct Binding {
    ctx: RequestContext,
    target: ExecutionTarget,
    revision: ConfigRevision,
    sender: mpsc::Sender<serde_json::Value>,
    active: AtomicBool,
    revoked: tokio::sync::Notify,
    pub(super) submission: Mutex<Option<String>>,
}
impl Binding {
    pub(super) fn revoke(&self) {
        self.active.store(false, Ordering::SeqCst);
        self.revoked.notify_waiters();
    }
    fn accepts(&self, entry: &results::ExecutionEntry) -> bool {
        self.active.load(Ordering::SeqCst)
            && entry.command == "query"
            && !entry.view.state.is_terminal()
            && entry.visible_to(&self.ctx)
            && entry.view.provenance.as_ref().is_some_and(|p| {
                p.requested_target == self.target && p.config_revision == self.revision
            })
    }
}
/// Dropping closes the consumer; blocked sends fail, allowing the driver to
/// cancel/drain. The execution retains the revoked binding so late output can
/// never fall back to an artifact writer.
pub struct ExportOutputBinding {
    sink: RuntimeResultSink,
    handle: SessionHandle,
    binding: Arc<Binding>,
    receiver: mpsc::Receiver<serde_json::Value>,
}
impl ExportOutputBinding {
    /// Trusted cancellation watcher may revoke while the formatter is writing.
    pub fn revoke_handle(&self) -> impl Fn() + Send + Sync + 'static {
        let binding = self.binding.clone();
        move || binding.revoke()
    }
    pub async fn validate(&self, runtime: &RuntimeConnectionUseCases) -> Result<(), ApiError> {
        let view = runtime
            .session_view(&self.binding.ctx, &self.handle)
            .await?;
        let profile = runtime
            .profile(&self.binding.ctx, &self.binding.target.connection_id)
            .await?;
        if !self.binding.active.load(Ordering::SeqCst)
            || view.initial_target != self.binding.target
            || profile.config_revision != self.binding.revision
        {
            return Err(ApiError::new(
                ApiErrorCode::PermissionDenied,
                "export context changed",
            ));
        }
        runtime
            .authorize_target(
                &self.binding.ctx,
                AuthorizationAction::Query,
                &self.binding.target,
                "query",
                &serde_json::Value::Null,
            )
            .await
    }
    pub fn try_recv(
        &mut self,
    ) -> Result<serde_json::Value, tokio::sync::mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }
    pub async fn recv(&mut self) -> Option<serde_json::Value> {
        self.receiver.recv().await
    }
}
impl Drop for ExportOutputBinding {
    fn drop(&mut self) {
        self.binding.revoke();
        self.receiver.close();
        let mut bindings = lock(&self.sink.3);
        if bindings
            .get(&self.handle)
            .is_some_and(|b| Arc::ptr_eq(b, &self.binding))
        {
            bindings.remove(&self.handle);
        }
    }
}
impl RuntimeConnectionUseCases {
    /// Trusted host adapter only. The profile revision is frozen by the caller
    /// after canonical target resolution and checked again at binding/dispatch.
    pub async fn bind_export_output(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
        target: &ExecutionTarget,
        revision: ConfigRevision,
    ) -> Result<ExportOutputBinding, ApiError> {
        self.authorize_target(
            ctx,
            AuthorizationAction::Query,
            target,
            "query",
            &serde_json::Value::Null,
        )
        .await?;
        let entry = self.session_entry(ctx, handle)?;
        let profile = self.profile(ctx, &target.connection_id).await?;
        if entry.closed.is_some()
            || entry.initial != *target
            || entry.ctx.authentication_session_id != ctx.authentication_session_id
            || entry.ctx.delegation_id != ctx.delegation_id
            || profile.config_revision != revision
            || !matches!(&entry.owner, OwnerRef::ClientSession { purpose, .. } if purpose == "tableExport")
        {
            return Err(ApiError::new(
                ApiErrorCode::PermissionDenied,
                "dedicated export session required",
            ));
        }
        let (sender, receiver) = mpsc::channel(1);
        let binding = Arc::new(Binding {
            ctx: ctx.clone(),
            target: target.clone(),
            revision,
            sender,
            active: AtomicBool::new(true),
            revoked: tokio::sync::Notify::new(),
            submission: Mutex::new(None),
        });
        let mut bindings = lock(&self.inner.sink.3);
        if bindings.contains_key(handle) {
            return Err(ApiError::new(
                ApiErrorCode::ContextConflict,
                "export already bound",
            ));
        }
        bindings.insert(handle.clone(), binding.clone());
        Ok(ExportOutputBinding {
            sink: self.inner.sink.clone(),
            handle: handle.clone(),
            binding,
            receiver,
        })
    }
}
impl RuntimeResultSink {
    /// Only a live bound execution may bypass the interactive result row cap.
    /// This authority is not inferred from client input or a session purpose.
    pub fn has_export_output(&self, id: &ExecutionId) -> bool {
        lock(&self.0)
            .get(id)
            .is_some_and(|e| e.export.as_ref().is_some_and(|b| b.accepts(e)))
    }
    /// Returns an unconsumed value for normal artifact publication. No execution
    /// or binding-map lock survives the await on the single output slot.
    pub async fn publish_export_output(
        &self,
        id: &ExecutionId,
        output: serde_json::Value,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        let binding = {
            let entries = lock(&self.0);
            let entry = entries
                .get(id)
                .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "execution unavailable"))?;
            match &entry.export {
                None => return Ok(Some(output)),
                Some(binding) if binding.accepts(entry) => binding.clone(),
                Some(_) => {
                    return Err(ApiError::new(
                        ApiErrorCode::PermissionDenied,
                        "export output revoked",
                    ))
                }
            }
        };
        // Serialize into a counting writer, never allocate another batch-sized
        // buffer. Includes columns/SQL/metadata as well as row cells.
        struct Limit(usize);
        impl std::io::Write for Limit {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.saturating_add(bytes.len());
                if self.0 > 2 * 1024 * 1024 {
                    return Err(std::io::Error::other("export batch limit"));
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer(&mut Limit(0), &output)
            .map_err(|_| ApiError::invalid_argument("export batch exceeds bounds"))?;
        if output
            .get("rows")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|r| r.len() > 256)
        {
            return Err(ApiError::invalid_argument(
                "export row batch exceeds bounds",
            ));
        }
        let revoked = binding.revoked.notified();
        tokio::pin!(revoked);
        revoked.as_mut().enable();
        if !binding.active.load(Ordering::SeqCst) {
            return Err(ApiError::new(
                ApiErrorCode::SessionLost,
                "export output revoked",
            ));
        }
        tokio::select! {
            biased;
            _ = &mut revoked => return Err(ApiError::new(ApiErrorCode::SessionLost, "export output revoked")),
            result = binding.sender.send(output) => result.map_err(|_| ApiError::new(ApiErrorCode::SessionLost, "export consumer closed"))?,
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::dto::session::{ContextConfidence, SessionContext, TransactionState};
    use datazen_platform_api::target::NamespaceTarget;
    use serde_json::json;

    fn fixture() -> (RuntimeResultSink, ExportOutputBinding, ExecutionId) {
        let sink = RuntimeResultSink::default();
        let ctx = RequestContext::new(
            OrganizationId::new("o"),
            PrincipalId::new("u"),
            None,
            ClientInstanceId::new("c"),
            RequestId::new("r"),
            None,
        );
        let handle = SessionHandle::new(DbSessionId::new("s"), RuntimeEpoch::new("e"));
        let target = ExecutionTarget::new(ConnectionId::new("p"), NamespaceTarget::database("db"));
        let (sender, receiver) = mpsc::channel(1);
        let binding = Arc::new(Binding {
            ctx: ctx.clone(),
            target: target.clone(),
            revision: ConfigRevision::new(1),
            sender,
            active: AtomicBool::new(true),
            revoked: tokio::sync::Notify::new(),
            submission: Mutex::new(None),
        });
        lock(&sink.3).insert(handle.clone(), binding.clone());
        let id = ExecutionId::new("execution");
        let context = SessionContext {
            namespace: target.namespace.clone(),
            search_path: None,
            effective_identity: None,
            transaction_state: TransactionState::None,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        };
        sink.admit(
            ctx.clone(),
            handle.clone(),
            StreamId::new("stream"),
            ExecutionView {
                execution_id: id.clone(),
                state: ExecutionState::Running,
                effect_outcome: EffectOutcome::Unknown,
                provenance: Some(ResultProvenance {
                    organization_id: ctx.organization_id,
                    principal_id: ctx.principal_id,
                    connection_id: target.connection_id.clone(),
                    config_revision: ConfigRevision::new(1),
                    context_before: context.clone(),
                    context_after: context,
                    requested_target: target,
                    capability_snapshot: CapabilitySnapshot::new("postgres", "test", 1),
                    executed_at: Timestamp::new("2026-01-01T00:00:00Z"),
                }),
                artifact_ids: vec![],
                result_completeness: ResultCompleteness::Complete,
                truncation_reason: None,
                error_code: None,
                runtime_binding: None,
            },
            "query".into(),
        );
        (
            sink.clone(),
            ExportOutputBinding {
                sink,
                handle,
                binding,
                receiver,
            },
            id,
        )
    }

    #[tokio::test]
    async fn one_slot_backpressure_does_not_lock_execution_ledger() {
        let (sink, mut binding, id) = fixture();
        let first = json!({"kind":"rows", "rows":[[1]]});
        assert!(sink
            .publish_export_output(&id, first.clone())
            .await
            .unwrap()
            .is_none());
        let second = sink.publish_export_output(&id, json!({"kind":"rows", "rows":[[2]]}));
        tokio::pin!(second);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut second)
                .await
                .is_err()
        );
        assert!(
            sink.0.try_lock().is_ok(),
            "blocked output must not hold global ledger"
        );
        assert_eq!(binding.recv().await.unwrap(), first);
        assert!(second.await.unwrap().is_none());
        assert_eq!(binding.recv().await.unwrap()["rows"][0][0], 2);
    }

    #[tokio::test]
    async fn dropped_consumer_rejects_pending_and_late_output_without_artifacts() {
        let (sink, binding, id) = fixture();
        sink.publish_export_output(&id, json!({"kind":"rows"}))
            .await
            .unwrap();
        let blocked = sink.publish_export_output(&id, json!({"kind":"rows"}));
        tokio::pin!(blocked);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut blocked)
                .await
                .is_err()
        );
        drop(binding);
        assert!(blocked.await.is_err());
        assert!(sink
            .publish_export_output(&id, json!({"kind":"rows"}))
            .await
            .is_err());
        assert!(sink.publish_output(&id, json!({"kind":"rows"})).is_err());
        let entries = lock(&sink.0);
        assert!(entries[&id].view.artifact_ids.is_empty());
        assert!(entries[&id].writer.is_none());
    }

    #[tokio::test]
    async fn owner_target_revision_and_invalidation_fail_closed() {
        for mutation in 0..4 {
            let (sink, binding, id) = fixture();
            {
                let mut entries = lock(&sink.0);
                let entry = entries.get_mut(&id).unwrap();
                match mutation {
                    0 => entry.ctx.principal_id = PrincipalId::new("other"),
                    1 => entry.ctx.client_instance_id = ClientInstanceId::new("other"),
                    2 => {
                        entry
                            .view
                            .provenance
                            .as_mut()
                            .unwrap()
                            .requested_target
                            .namespace = NamespaceTarget::database("other")
                    }
                    _ => {
                        entry.view.provenance.as_mut().unwrap().config_revision =
                            ConfigRevision::new(2)
                    }
                }
            }
            assert!(!sink.has_export_output(&id));
            assert!(sink
                .publish_export_output(&id, json!({"kind":"rows"}))
                .await
                .is_err());
            drop(binding);
        }
        let (sink, binding, id) = fixture();
        sink.publish_export_output(&id, json!({"kind":"rows"}))
            .await
            .unwrap();
        let blocked = sink.publish_export_output(&id, json!({"kind":"rows"}));
        tokio::pin!(blocked);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut blocked)
                .await
                .is_err()
        );
        sink.invalidate(&binding.handle);
        assert!(blocked.await.is_err());
        sink.finish(&id, false, true);
        let entries = lock(&sink.0);
        assert_eq!(entries[&id].view.effect_outcome, EffectOutcome::Unknown);
        assert!(entries[&id].view.artifact_ids.is_empty());
    }

    #[tokio::test]
    async fn cancellation_revoker_wakes_blocked_producer_without_dropping_owner() {
        let (sink, binding, id) = fixture();
        sink.publish_export_output(&id, json!({"kind":"rows"}))
            .await
            .unwrap();
        let blocked = sink.publish_export_output(&id, json!({"kind":"rows"}));
        tokio::pin!(blocked);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut blocked)
                .await
                .is_err()
        );
        let revoke = binding.revoke_handle();
        revoke();
        assert!(blocked.await.is_err());
        assert!(!sink.has_export_output(&id));
        assert_eq!(
            lock(&sink.0)[&id].view.effect_outcome,
            EffectOutcome::Unknown
        );
        assert!(
            lock(&sink.3).contains_key(&binding.handle),
            "revocation must not pretend the session was cleaned up"
        );
    }

    #[tokio::test]
    async fn oversized_batch_is_rejected_before_queueing() {
        let (sink, mut binding, id) = fixture();
        assert!(sink
            .publish_export_output(&id, json!({"rows":vec![json!([1]);257]}))
            .await
            .is_err());
        assert!(sink
            .publish_export_output(&id, json!({"text":"x".repeat(2*1024*1024)}))
            .await
            .is_err());
        assert!(binding.try_recv().is_err());
    }
}
