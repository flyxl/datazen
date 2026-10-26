use super::*;
use async_trait::async_trait;
use datazen_platform_api::{
    dto::{
        event::EventSequence,
        idempotency::{IdempotentOperation, SubmissionToken},
    },
    ports::{event::EventSubscription, profile::ProfileScope, session_directory::CloseDisposition},
};
use datazen_platform_api::{
    dto::{profile::ProfileView, requests::*},
    use_cases::connection::ConnectionUseCases,
};
impl RuntimeConnectionUseCases {
    async fn enqueue(
        &self,
        ctx: &RequestContext,
        request: ExecuteInSessionRequest,
        short: bool,
    ) -> Result<ExecutionReceipt, ApiError> {
        let action = if matches!(
            request.call.command.as_str(),
            "query"
                | "explain"
                | "query_stream"
                | "get_table_schema"
                | "list_tables"
                | "list_objects"
                | "get_object_ddl"
                | "get_schemas"
                | "list_databases"
                | "list_catalog"
                | "read_relation_columns"
                | "read_relation_schema"
                | "refresh_schema_metadata"
                | "backup_dump"
        ) {
            AuthorizationAction::Query
        } else {
            AuthorizationAction::Write
        };
        self.authorize(ctx, action.clone()).await?;
        let view = self.session_view(ctx, &request.handle).await?;
        self.authorize_target(
            ctx,
            action.clone(),
            &view.initial_target,
            &request.call.command,
            &request.call.input,
        )
        .await?;
        let entry = self.session_entry(ctx, &request.handle)?;
        if request.call.command == "backup_dump"
            && !lock(&self.inner.sink.2).contains_key(&request.handle)
        {
            return Err(ApiError::new(
                ApiErrorCode::PermissionDenied,
                "backup output must be bound before admission",
            ));
        }
        if matches!(&entry.owner, OwnerRef::ClientSession { purpose, .. } if purpose == "tableExport")
        {
            let binding = lock(&self.inner.sink.3)
                .get(&request.handle)
                .cloned()
                .ok_or_else(|| {
                    ApiError::new(
                        ApiErrorCode::PermissionDenied,
                        "export output must be bound",
                    )
                })?;
            let mut submission = lock(&binding.submission);
            let key = request.idempotency_key.as_str();
            if request.call.command != "query"
                || submission.as_ref().is_some_and(|prior| prior != key)
            {
                return Err(ApiError::new(
                    ApiErrorCode::PermissionDenied,
                    "export permits one query only",
                ));
            }
            *submission = Some(key.to_owned());
        }
        // Resolve provenance before accepting the command. If the actor has
        // already retired this physical resource, fail before creating an
        // accepted gateway receipt that cannot be admitted to the durable sink.
        let profile = self.profile(ctx, &view.connection_id).await?;
        let capability_provenance = self
            .inner
            .registry
            .capability_snapshot(&convert::handle(&request.handle)?)
            .await
            .map_err(convert::runtime)?;
        let capability_snapshot = datazen_platform_api::dto::execution::CapabilitySnapshot {
            driver_id: capability_provenance.driver_id,
            driver_version: capability_provenance.driver_version,
            protocol_version: capability_provenance.protocol_version,
            capability_revision: datazen_platform_api::id::CapabilityRevision::new(
                capability_provenance.capability_revision,
            ),
            confirmed: capability_provenance
                .confirmed
                .into_iter()
                .map(|(key, value)| (key, serde_json::Value::String(value)))
                .collect(),
        };
        let dispatch_target = view.initial_target.clone();
        let dispatch_command = request.call.command.clone();
        let dispatch_input = request.call.input.clone();
        let gateway_request = ExecutionRequest::new(
            convert::handle(&request.handle)?,
            request
                .expected_context_revision
                .unwrap_or(view.context_revision),
            rt::CommandCall {
                command: request.call.command,
                input: request.call.input,
            },
            request.idempotency_key.as_str(),
            Self::source(ctx, &entry),
        );
        let fingerprint = gateway_request.fingerprint();
        let mut activity = self.begin_idle_activity(&request.handle).await?;
        let accepted = self
            .inner
            .gateway
            .accept(&Self::principal(ctx, &request.handle), gateway_request)
            .await
            .map_err(gateway_error)?;
        let receipt: ExecutionReceipt = convert::json_convert(accepted.receipt.clone())?;
        if !accepted.is_replay() {
            self.inner.sink.admit(
                ctx.clone(),
                view.handle.clone(),
                receipt.stream_id.clone(),
                ExecutionView {
                    execution_id: receipt.execution_id.clone(),
                    state: ExecutionState::Queued,
                    effect_outcome: EffectOutcome::Unknown,
                    provenance: Some(ResultProvenance {
                        organization_id: ctx.organization_id.clone(),
                        principal_id: ctx.principal_id.clone(),
                        connection_id: view.connection_id.clone(),
                        config_revision: profile.config_revision,
                        context_before: view.observed_context.clone(),
                        context_after: view.observed_context,
                        requested_target: view.initial_target,
                        capability_snapshot,
                        executed_at: Timestamp::new(utc_now()),
                    }),
                    artifact_ids: vec![],
                    result_completeness: ResultCompleteness::Pending,
                    truncation_reason: None,
                    error_code: None,
                    runtime_binding: None,
                },
                dispatch_command.clone(),
            );
            let service = self.clone();
            let ctx = ctx.clone();
            let handle = request.handle;
            let id = receipt.execution_id.clone();
            tokio::spawn(async move {
                // The async policy port is checked again immediately before physical dispatch.
                // Pending authorization owns a bounded hold, not business TTL.
                let authorization = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    service.authorize_target(
                        &ctx,
                        action,
                        &dispatch_target,
                        &dispatch_command,
                        &dispatch_input,
                    ),
                )
                .await
                .unwrap_or_else(|_| {
                    Err(ApiError::new(
                        ApiErrorCode::PermissionDenied,
                        "dispatch authorization timed out",
                    ))
                });
                let result = match authorization {
                    Ok(()) => {
                        activity.business_started();
                        service
                            .inner
                            .gateway
                            .dispatch(&Self::principal(&ctx, &handle), &id)
                            .await
                            .map_err(gateway_error)
                    }
                    Err(error) => Err(error),
                };
                activity.finish().await;
                let resource_lost = result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.code == ApiErrorCode::SessionLost);
                service
                    .inner
                    .sink
                    .finish(&id, result.is_ok(), resource_lost);
                if service
                    .inner
                    .sink
                    .view(&ctx, &id)
                    .is_ok_and(|v| v.effect_outcome != EffectOutcome::Unknown)
                {
                    service
                        .inner
                        .gateway
                        .resolve_unknown_outcome(handle.db_session_id.as_str(), &fingerprint)
                        .await;
                }
                if let Ok(view) = service.session_view(&ctx, &handle).await {
                    service.inner.sink.session_event(view);
                }

                if short {
                    let _ = service
                        .close_session(
                            &ctx,
                            CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                        )
                        .await;
                }
            });
        } else {
            activity.finish().await;
        }
        Ok(receipt)
    }
    async fn execute_target_owned(
        &self,
        ctx: &RequestContext,
        request: ExecuteAtTargetRequest,
        abandoned: Arc<std::sync::atomic::AtomicBool>,
        cleanup_handle: Arc<Mutex<Option<SessionHandle>>>,
    ) -> Result<ExecutionReceipt, ApiError> {
        request.validate()?;
        self.validate_token(
            ctx,
            &request.idempotency_key,
            IdempotentOperation::ExecuteAtTarget,
            None,
        )?;
        self.authorize_target(
            ctx,
            AuthorizationAction::Query,
            &request.target,
            &request.call.command,
            &request.call.input,
        )
        .await?;
        let profile = self.profile(ctx, &request.target.connection_id).await?;
        if profile.config_revision.counter() != request.expected_config_revision {
            return Err(ApiError::new(
                ApiErrorCode::ConfigRevisionMismatch,
                "connection configuration changed",
            ));
        }
        let fingerprint = serde_json::to_value(&request)
            .map_err(|_| ApiError::invalid_argument("invalid target request"))?;
        let _admission = self
            .coordinate_admission(request.idempotency_key.as_str())
            .await?;
        if let Some((prior, value)) = self
            .inner
            .receipts
            .lock()
            .await
            .get(request.idempotency_key.as_str())
            .cloned()
        {
            if prior != fingerprint {
                return Err(ApiError::new(
                    ApiErrorCode::IdempotencyConflict,
                    "target request changed",
                ));
            }
            return convert::json_convert(&value);
        }
        let opened = self
            .create_session_for_command(
                ctx,
                request.target,
                OwnerRef::ClientSession {
                    client_instance_id: ctx.client_instance_id.clone(),
                    purpose: "explicitTargetExecution".into(),
                },
                &request.call.command,
            )
            .await?;
        let handle = opened.session.handle;
        *lock(&cleanup_handle) = Some(handle.clone());
        if abandoned.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "target admission abandoned",
            ));
        }
        let result = self
            .enqueue(
                ctx,
                ExecuteInSessionRequest::new(
                    handle.clone(),
                    request.call,
                    request.idempotency_key.clone(),
                ),
                true,
            )
            .await;
        let receipt = match result {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = self
                    .close_session(
                        ctx,
                        CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                    )
                    .await;
                return Err(error);
            }
        };
        self.inner.receipts.lock().await.insert(
            request.idempotency_key.as_str().to_owned(),
            (
                fingerprint,
                serde_json::to_value(&receipt).map_err(|_| {
                    ApiError::new(ApiErrorCode::OutcomeUnknown, "target receipt unavailable")
                })?,
            ),
        );
        Ok(receipt)
    }
}
pub(super) fn utc_now() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    crate::directory::ClockBase::new(crate::directory::MonoInstant::ZERO, millis)
        .project(crate::directory::MonoInstant::ZERO)
        .as_str()
        .to_owned()
}
fn gateway_error(error: GatewayError) -> ApiError {
    match error {
        GatewayError::Runtime(error) => convert::runtime(error),
        GatewayError::IdempotencyConflict { .. } => ApiError::new(
            ApiErrorCode::IdempotencyConflict,
            "submission changed since acceptance",
        ),
        GatewayError::PermissionDenied { .. } => {
            ApiError::permission_denied("session permission denied")
        }
        _ => ApiError::new(
            ApiErrorCode::OutcomeUnknown,
            "execution acceptance could not be verified",
        ),
    }
}
#[async_trait]
impl ConnectionUseCases for RuntimeConnectionUseCases {
    async fn list_connections(&self, ctx: &RequestContext) -> Result<Vec<ProfileView>, ApiError> {
        self.authorize(ctx, AuthorizationAction::Connect).await?;
        Ok(self
            .inner
            .profiles
            .list(ctx, ProfileScope::from_context(ctx))
            .await
            .map_err(convert::port)?
            .into_iter()
            .filter(|p| p.organization_id == ctx.organization_id && p.enabled)
            .map(|p| ProfileView::from_record(&p))
            .collect())
    }
    async fn issue_submission_token(
        &self,
        ctx: &RequestContext,
        op: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, ApiError> {
        self.issue(ctx, op, handle).await
    }
    async fn open_session(
        &self,
        ctx: &RequestContext,
        request: OpenSessionRequest,
    ) -> Result<OpenSessionReceipt, ApiError> {
        request.validate()?;
        self.authorize_target(
            ctx,
            AuthorizationAction::Connect,
            &request.initial_target,
            "open_session",
            &serde_json::Value::Null,
        )
        .await?;
        convert::owner(ctx, &request.initial_target.connection_id, &request.owner)?;
        self.validate_token(
            ctx,
            &request.idempotency_key,
            IdempotentOperation::OpenSession,
            None,
        )?;
        let fingerprint = serde_json::to_value(&request)
            .map_err(|_| ApiError::invalid_argument("invalid session request"))?;
        let _admission = self
            .coordinate_admission(request.idempotency_key.as_str())
            .await?;
        if let Some((prior, value)) = self
            .inner
            .receipts
            .lock()
            .await
            .get(request.idempotency_key.as_str())
            .cloned()
        {
            if prior != fingerprint {
                return Err(ApiError::new(
                    ApiErrorCode::IdempotencyConflict,
                    "session request changed",
                ));
            }
            let receipt: OpenSessionReceipt = convert::json_convert(&value)?;
            let entry = self.session_entry(ctx, &receipt.session.handle)?;
            if entry.closed.is_some() {
                return Err(ApiError::new(
                    ApiErrorCode::SessionLost,
                    "session has been released",
                ));
            }
            return Ok(receipt);
        }
        let receipt = self
            .create_session(ctx, request.initial_target, request.owner)
            .await?;
        self.inner.receipts.lock().await.insert(
            request.idempotency_key.as_str().to_owned(),
            (
                fingerprint,
                serde_json::to_value(&receipt).map_err(|_| {
                    ApiError::new(ApiErrorCode::OutcomeUnknown, "session receipt unavailable")
                })?,
            ),
        );
        Ok(receipt)
    }
    async fn get_session(
        &self,
        ctx: &RequestContext,
        handle: SessionHandle,
    ) -> Result<SessionView, ApiError> {
        self.authorize(ctx, AuthorizationAction::Connect).await?;
        self.session_view(ctx, &handle).await
    }
    async fn attach_session(
        &self,
        ctx: &RequestContext,
        request: AttachmentRequest,
    ) -> Result<SessionView, ApiError> {
        request.validate()?;
        self.authorize(ctx, AuthorizationAction::Connect).await?;
        self.session_view(ctx, &request.handle).await?;
        self.inner
            .directory
            .attach_from_client(
                &request.handle,
                ctx.principal_id.clone(),
                ctx.client_instance_id.clone(),
                request.attachment_token,
            )
            .map_err(|_| {
                ApiError::new(ApiErrorCode::ContextConflict, "attachment token rejected")
            })?;
        if let Some(entry) = lock(&self.inner.sessions).get_mut(&request.handle.db_session_id) {
            entry.attached = true;
        }
        // Attachment proves ownership, not business activity. Preserve the deadline.
        self.session_view(ctx, &request.handle).await
    }
    async fn detach_session(
        &self,
        ctx: &RequestContext,
        request: AttachmentRequest,
    ) -> Result<SessionView, ApiError> {
        self.attach_session(ctx, request.clone()).await?;
        self.inner
            .directory
            .detach(&request.handle)
            .map_err(convert::port)?;
        if let Some(entry) = lock(&self.inner.sessions).get_mut(&request.handle.db_session_id) {
            entry.attached = false;
        }
        // 摘掉就是「没人用了」：把空闲期限收到摘挂载宽限。到期由回收器驱逐会话并释放
        // 物理资源——这正是「没有任何客户端的会话不该继续占额度」的落点。只收紧不放开。
        self.arm_detached_grace(&request.handle).await?;
        self.session_view(ctx, &request.handle).await
    }
    async fn execute_in_session(
        &self,
        ctx: &RequestContext,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, ApiError> {
        request.validate()?;
        self.validate_token(
            ctx,
            &request.idempotency_key,
            IdempotentOperation::ExecuteInSession,
            Some(&request.handle),
        )?;
        let _admission = self
            .coordinate_admission(request.idempotency_key.as_str())
            .await?;
        self.enqueue(ctx, request, false).await
    }
    async fn execute_at_target(
        &self,
        ctx: &RequestContext,
        request: ExecuteAtTargetRequest,
    ) -> Result<ExecutionReceipt, ApiError> {
        let service = self.clone();
        let ctx = ctx.clone();
        let abandoned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleanup_handle = Arc::new(Mutex::new(None));
        let (reply, receive) = tokio::sync::oneshot::channel();
        let (ack, acknowledged) = tokio::sync::oneshot::channel();
        let cancel_flag = abandoned.clone();
        tokio::spawn(async move {
            let result = service
                .execute_target_owned(&ctx, request, abandoned, cleanup_handle.clone())
                .await;
            let delivered = reply.send(result).is_ok();
            if !delivered || acknowledged.await.is_err() {
                let abandoned_handle = lock(&cleanup_handle).clone();
                if let Some(handle) = abandoned_handle {
                    // Closing queues behind protocol completion on the original actor;
                    // it cannot rebind a handle to a replacement resource.
                    let executions: Vec<_> = lock(&service.inner.sink.0)
                        .values()
                        .filter(|entry| entry.handle == handle && !entry.view.state.is_terminal())
                        .map(|entry| entry.view.execution_id.clone())
                        .collect();
                    for id in executions {
                        if let Ok(runtime_handle) = convert::handle(&handle) {
                            let _ = service
                                .inner
                                .registry
                                .cancel_execution(&runtime_handle, &id)
                                .await;
                        }
                    }
                    let _ = service
                        .close_session(
                            &ctx,
                            CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                        )
                        .await;
                }
            }
        });
        struct AdmissionDrop(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for AdmissionDrop {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let guard = AdmissionDrop(cancel_flag);
        let result = receive.await.map_err(|_| {
            ApiError::new(ApiErrorCode::OutcomeUnknown, "target admission task lost")
        })?;
        let _ = ack.send(());
        drop(guard);
        result
    }

    async fn set_session_context(
        &self,
        ctx: &RequestContext,
        request: SetSessionContextRequest,
    ) -> Result<ContextChangeReceipt, ApiError> {
        request.validate()?;
        if ctx.is_delegated() {
            return Err(ApiError::permission_denied(
                "delegated session targets are immutable",
            ));
        }
        self.authorize(ctx, AuthorizationAction::Connect).await?;
        self.validate_token(
            ctx,
            &request.idempotency_key,
            IdempotentOperation::SetSessionContext,
            Some(&request.handle),
        )?;
        let fingerprint = serde_json::to_value(&request)
            .map_err(|_| ApiError::invalid_argument("invalid context request"))?;
        let _admission = self
            .coordinate_admission(request.idempotency_key.as_str())
            .await?;
        if let Some((prior, value)) = self
            .inner
            .receipts
            .lock()
            .await
            .get(request.idempotency_key.as_str())
            .cloned()
        {
            if prior != fingerprint {
                return Err(ApiError::new(
                    ApiErrorCode::IdempotencyConflict,
                    "context request changed",
                ));
            }
            return convert::json_convert(&value);
        }
        let entry = self.session_entry(ctx, &request.handle)?;
        self.session_view(ctx, &request.handle).await?;
        let desired = ExecutionTarget::new(entry.initial.connection_id.clone(), request.desired);
        let replaced = self
            .inner
            .replacer
            .replace(crate::registry::ContextChangeRequest {
                handle: convert::handle(&request.handle)?,
                expected_context_revision: request.expected_context_revision.get(),
                desired: convert::target(&desired)?,
                candidate_db_session_id: DbSessionIdGenerator::new().generate(),
                principal_id: ctx.principal_id.clone(),
                client_instance_id: ctx.client_instance_id.clone(),
            })
            .await
            .map_err(convert::runtime)?;
        if replaced.session.db_session_id != request.handle.db_session_id {
            let epoch = convert::handle(&request.handle)?.runtime_epoch.get();
            let physical = lock(&self.inner.physical).get(&epoch).cloned();
            let release = match physical.as_ref().and_then(|p| p.release) {
                Some(
                    crate::registry::backend::CloseResourceOutcome::Closed
                    | crate::registry::backend::CloseResourceOutcome::ReturnedToPool,
                ) => ResourceRelease::Confirmed,
                Some(crate::registry::backend::CloseResourceOutcome::Undecidable { .. }) => {
                    ResourceRelease::Quarantined
                }
                None => ResourceRelease::Pending,
            };
            if let Some(old) = lock(&self.inner.sessions).get_mut(&request.handle.db_session_id) {
                old.closed = Some(CloseReceipt {
                    db_session_id: request.handle.db_session_id.clone(),
                    state: SessionState::Closed,
                    effect_outcome: physical
                        .map(|p| convert::json_convert(p.effect))
                        .transpose()?
                        .unwrap_or(EffectOutcome::Unknown),
                    resource_release: release,
                });
                old.closed_at = Some(Instant::now());
            }
            if release == ResourceRelease::Confirmed {
                lock(&self.inner.physical).remove(&epoch);
            }
            self.reclaim_session_tombstones();
        }
        lock(&self.inner.sessions).insert(
            replaced.session.db_session_id.clone(),
            SessionEntry {
                ctx: ctx.clone(),
                owner: entry.owner,
                initial: desired,
                handle: replaced.session.clone(),
                attached: true,
                closed: None,
                closed_at: None,
            },
        );
        self.inner.sink.invalidate(&request.handle);
        let receipt = ContextChangeReceipt {
            session: self.session_view(ctx, &replaced.session).await?,
            replaced_session_id: Some(replaced.replaced_session_id),
            attachment_token: Some(replaced.attachment_token),
        };
        self.inner.receipts.lock().await.insert(
            request.idempotency_key.as_str().to_owned(),
            (
                fingerprint,
                serde_json::to_value(&receipt).map_err(|_| {
                    ApiError::new(ApiErrorCode::OutcomeUnknown, "context receipt unavailable")
                })?,
            ),
        );
        Ok(receipt)
    }
    async fn close_session(
        &self,
        ctx: &RequestContext,
        request: CloseSessionRequest,
    ) -> Result<CloseReceipt, ApiError> {
        request.validate()?;
        // Owner checks still apply after policy revocation so cleanup remains possible.
        let entry = self.session_entry(ctx, &request.handle)?;
        if let Some(receipt) = entry.closed {
            return Ok(receipt);
        }
        let result = self
            .inner
            .registry
            .close_registered(
                &convert::handle(&request.handle)?,
                match request.mode {
                    CloseMode::RequireNoTransaction => rt::CloseMode::RequireNoTransaction,
                    CloseMode::RollbackAndClose => rt::CloseMode::RollbackAndClose,
                },
            )
            .await;
        let mut receipt = match result {
            Ok(()) => CloseReceipt {
                db_session_id: request.handle.db_session_id.clone(),
                state: SessionState::Closed,
                effect_outcome: EffectOutcome::Unknown,
                resource_release: ResourceRelease::Confirmed,
            },
            Err(rt::RuntimeError::SessionLost(_)) => CloseReceipt {
                db_session_id: request.handle.db_session_id.clone(),
                state: SessionState::Lost,
                effect_outcome: EffectOutcome::Unknown,
                resource_release: ResourceRelease::Quarantined,
            },
            Err(error) => return Err(convert::runtime(error)),
        };
        // 关闭路径先作废活连接借用。
        //
        // 位置在**回执组装之后、目录释放之前**是刻意的：回执里 `Closed`/`Quarantined`
        // 的判读需要先读到物理台账；而借用作废必须早于 `directory.release`，否则
        // 「目录已释放、引用仍可用」这个窗口里，宿主能拿着一份已经作废的引用继续打驱动。
        let interrupted = lock(&self.inner.live)
            .clone()
            .revoke(&request.handle.db_session_id);
        if interrupted > 0 {
            tracing::warn!(
                db_session_id = request.handle.db_session_id.as_str(),
                borrows = interrupted,
                "会话关闭打断了仍在外的活连接借用：引用已作废"
            );
        }
        if let Some(physical) =
            lock(&self.inner.physical).get(&convert::handle(&request.handle)?.runtime_epoch.get())
        {
            receipt.resource_release = match physical.release {
                Some(
                    crate::registry::backend::CloseResourceOutcome::Closed
                    | crate::registry::backend::CloseResourceOutcome::ReturnedToPool,
                ) => ResourceRelease::Confirmed,
                Some(crate::registry::backend::CloseResourceOutcome::Undecidable { .. }) => {
                    ResourceRelease::Quarantined
                }
                None => ResourceRelease::Pending,
            };
            receipt.effect_outcome = convert::json_convert(physical.effect)?;
        }
        let _ = self
            .inner
            .directory
            .release(&request.handle, CloseDisposition::Closed)
            .await;
        if let Some(entry) = lock(&self.inner.sessions).get_mut(&request.handle.db_session_id) {
            entry.closed = Some(receipt.clone());
            entry.closed_at = Some(Instant::now());
        }
        if receipt.resource_release == ResourceRelease::Confirmed {
            lock(&self.inner.physical)
                .remove(&convert::handle(&request.handle)?.runtime_epoch.get());
        }
        self.reclaim_session_tombstones();
        lock(&self.inner.transaction_unknown).remove(&request.handle);
        lock(&self.inner.transaction_gates).remove(&request.handle);
        self.inner.sink.invalidate(&request.handle);
        Ok(receipt)
    }
    async fn get_execution(
        &self,
        ctx: &RequestContext,
        id: ExecutionId,
    ) -> Result<ExecutionView, ApiError> {
        self.authorize(ctx, AuthorizationAction::Query).await?;
        self.inner.sink.view(ctx, &id)
    }
    async fn cancel_execution(
        &self,
        ctx: &RequestContext,
        id: ExecutionId,
    ) -> Result<CancelReceipt, ApiError> {
        self.authorize(ctx, AuthorizationAction::Query).await?;
        let handle = self.inner.sink.handle(ctx, &id)?;
        let view = self.inner.sink.view(ctx, &id)?;
        if view.state.is_terminal() {
            return Ok(CancelReceipt {
                execution_id: id,
                disposition: datazen_platform_api::dto::session::CancelDisposition::AlreadyFinished,
                state: view.state,
            });
        }
        self.session_view(ctx, &handle).await?;
        let receipt = self
            .inner
            .registry
            .cancel_registered(&convert::handle(&handle)?, &id)
            .await
            .map_err(convert::runtime)?;
        convert::json_convert(receipt)
    }
    async fn subscribe_events(
        &self,
        ctx: &RequestContext,
        stream: StreamId,
        after: Option<EventSequence>,
    ) -> Result<EventSubscription, ApiError> {
        self.authorize(ctx, AuthorizationAction::Query).await?;
        self.inner.sink.subscribe(ctx, &stream, after, None)
    }
}

impl RuntimeConnectionUseCases {
    /// Close every live session opened on one persistent profile.
    ///
    /// `save_connection` bumps the profile's `config_revision`; sessions that
    /// survive the save keep their leases and physical connections until a tab
    /// closes them. This retires them right after the revision moved, so the
    /// quota returns to zero and a reconnect starts from the new profile.
    ///
    /// Owner checks still come from each session's **stored** request context:
    /// this is cleanup, not an ownership bypass. Sessions that are already
    /// closed are skipped, and one failing session does not stop the rest — the
    /// save must not fail because cleanup did. Counting is deliberately
    /// best-effort; the caller treats the number as diagnostic only.
    pub async fn close_sessions_for_connection(
        &self,
        ctx: &RequestContext,
        connection_id: &ConnectionId,
    ) -> usize {
        let targets: Vec<(SessionHandle, RequestContext)> = {
            let sessions = lock(&self.inner.sessions);
            sessions
                .values()
                .filter(|entry| {
                    entry.closed.is_none()
                        && entry.initial.connection_id == *connection_id
                        && entry.ctx.organization_id == ctx.organization_id
                        && entry.ctx.principal_id == ctx.principal_id
                    // The revision is profile-wide, so every client
                    // instance (other window, MCP) is invalidated too.
                    // Ownership is still the stored context, not this
                    // caller's identity, and close re-checks it.
                })
                .map(|entry| (entry.handle.clone(), entry.ctx.clone()))
                .collect()
        };

        let mut closed = 0usize;
        for (handle, owner_ctx) in targets {
            let db_session_id = handle.db_session_id.clone();
            // Read the registry view, not `get_session`: the profile revision
            // already moved, so the application-level view would report
            // `connection configuration changed` for every target. The actor's
            // `View` is answered while an execution is flying, so this check
            // never waits for the query. Deferring matters twice: save stays
            // fast, and the in-flight request keeps its old revision (its next
            // request gets `connection configuration changed`) instead of being
            // cancelled by a close it never asked for.
            let active = match convert::handle(&handle) {
                Ok(registry_handle) => self
                    .inner
                    .registry
                    .session_view(&registry_handle)
                    .await
                    .map(|view| view.active_execution_id.is_some())
                    .unwrap_or(false),
                // A handle this application opened cannot fail conversion; if
                // it somehow does, fall through to the close path, which will
                // report the same fact through its own error.
                Err(_) => false,
            };
            if active {
                tracing::info!(
                    db_session_id = db_session_id.as_str(),
                    "session retirement deferred: execution still in flight"
                );
                continue;
            }
            let request = CloseSessionRequest::new(handle, CloseMode::RollbackAndClose);
            match self.close_session(&owner_ctx, request).await {
                Ok(_) => closed += 1,
                Err(error)
                    if matches!(
                        error.code,
                        ApiErrorCode::SessionNotFound | ApiErrorCode::SessionLost
                    ) =>
                {
                    // Another path won the close race; the resource is already
                    // accounted as gone, so this is not a cleanup failure.
                    closed += 1;
                }
                Err(error) => {
                    tracing::warn!(
                        %connection_id,
                        db_session_id = db_session_id.as_str(),
                        code = error.code.as_str(),
                        "session retirement after configuration change failed"
                    );
                }
            }
        }
        closed
    }
}
