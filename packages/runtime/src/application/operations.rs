use super::*;
use async_trait::async_trait;
use datazen_application::{
    dto::{profile::ProfileView, requests::*},
    sessions::ConnectionUseCases,
};
use datazen_platform_api::{
    dto::{
        event::EventSequence,
        idempotency::{IdempotentOperation, SubmissionToken},
    },
    ports::{event::EventSubscription, profile::ProfileScope, session_directory::CloseDisposition},
};
impl RuntimeConnectionUseCases {
    async fn enqueue(
        &self,
        ctx: &RequestContext,
        request: ExecuteInSessionRequest,
        short: bool,
    ) -> Result<ExecutionReceipt, ApiError> {
        let action = if request.call.command == "query"
            || request.call.command.starts_with("list_")
            || request.call.command.starts_with("get_")
        {
            AuthorizationAction::Query
        } else {
            AuthorizationAction::Write
        };
        self.authorize(ctx, action.clone()).await?;
        let view = self.session_view(ctx, &request.handle).await?;
        let entry = self.session_entry(ctx, &request.handle)?;
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
        let accepted = self
            .inner
            .gateway
            .accept(&Self::principal(ctx, &request.handle), gateway_request)
            .await
            .map_err(gateway_error)?;
        let receipt: ExecutionReceipt = convert::json_convert(accepted.receipt.clone())?;
        if !accepted.is_replay() {
            let profile = self.profile(ctx, &view.connection_id).await?;
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
                        capability_snapshot: CapabilitySnapshot::new(profile.driver_id, "", 0),
                        executed_at: Timestamp::new(utc_now()),
                    }),
                    artifact_ids: vec![ArtifactId::new(format!(
                        "artifact-{}",
                        receipt.execution_id.as_str()
                    ))],
                    result_completeness: ResultCompleteness::Pending,
                    truncation_reason: None,
                    error_code: None,
                    runtime_binding: None,
                },
            );
            let service = self.clone();
            let ctx = ctx.clone();
            let handle = request.handle;
            let id = receipt.execution_id.clone();
            tokio::spawn(async move {
                // The async policy port is checked again immediately before physical dispatch.
                let result = match service.authorize(&ctx, action).await {
                    Ok(()) => service
                        .inner
                        .gateway
                        .dispatch(&Self::principal(&ctx, &handle), &id)
                        .await
                        .map_err(gateway_error),
                    Err(error) => Err(error),
                };
                service.inner.sink.finish(&id, result.is_ok());
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
        }
        Ok(receipt)
    }
}
fn utc_now() -> String {
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
            .map(|p| p.to_view())
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
        self.authorize(ctx, AuthorizationAction::Connect).await?;
        convert::owner(ctx, &request.initial_target.connection_id, &request.owner)?;
        self.validate_token(
            ctx,
            &request.idempotency_key,
            IdempotentOperation::OpenSession,
            None,
        )?;
        let fingerprint = serde_json::to_value(&request)
            .map_err(|_| ApiError::invalid_argument("invalid session request"))?;
        let mut receipts = self.inner.receipts.lock().await;
        if let Some((prior, value)) = receipts.get(request.idempotency_key.as_str()) {
            if prior != &fingerprint {
                return Err(ApiError::new(
                    ApiErrorCode::IdempotencyConflict,
                    "session request changed",
                ));
            }
            let receipt: OpenSessionReceipt = convert::json_convert(value)?;
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
        receipts.insert(
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
        self.enqueue(ctx, request, false).await
    }
    async fn execute_at_target(
        &self,
        ctx: &RequestContext,
        request: ExecuteAtTargetRequest,
    ) -> Result<ExecutionReceipt, ApiError> {
        request.validate()?;
        self.validate_token(
            ctx,
            &request.idempotency_key,
            IdempotentOperation::ExecuteAtTarget,
            None,
        )?;
        self.authorize(ctx, AuthorizationAction::Query).await?;
        let profile = self.profile(ctx, &request.target.connection_id).await?;
        if profile.config_revision.counter() != request.expected_config_revision {
            return Err(ApiError::new(
                ApiErrorCode::ConfigRevisionMismatch,
                "connection configuration changed",
            ));
        }
        let fingerprint = serde_json::to_value(&request)
            .map_err(|_| ApiError::invalid_argument("invalid target request"))?;
        let mut receipts = self.inner.receipts.lock().await;
        if let Some((prior, value)) = receipts.get(request.idempotency_key.as_str()) {
            if prior != &fingerprint {
                return Err(ApiError::new(
                    ApiErrorCode::IdempotencyConflict,
                    "target request changed",
                ));
            }
            return convert::json_convert(value);
        }
        let opened = self
            .create_session(
                ctx,
                request.target,
                OwnerRef::ClientSession {
                    client_instance_id: ctx.client_instance_id.clone(),
                    purpose: "explicitTargetExecution".into(),
                },
            )
            .await?;
        let handle = opened.session.handle;
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
        receipts.insert(
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
    async fn set_session_context(
        &self,
        ctx: &RequestContext,
        request: SetSessionContextRequest,
    ) -> Result<ContextChangeReceipt, ApiError> {
        request.validate()?;
        self.authorize(ctx, AuthorizationAction::Connect).await?;
        self.validate_token(
            ctx,
            &request.idempotency_key,
            IdempotentOperation::SetSessionContext,
            Some(&request.handle),
        )?;
        let fingerprint = serde_json::to_value(&request)
            .map_err(|_| ApiError::invalid_argument("invalid context request"))?;
        let mut receipts = self.inner.receipts.lock().await;
        if let Some((prior, value)) = receipts.get(request.idempotency_key.as_str()) {
            if prior != &fingerprint {
                return Err(ApiError::new(
                    ApiErrorCode::IdempotencyConflict,
                    "context request changed",
                ));
            }
            return convert::json_convert(value);
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
        lock(&self.inner.sessions).insert(
            replaced.session.db_session_id.clone(),
            SessionEntry {
                ctx: ctx.clone(),
                owner: entry.owner,
                initial: desired,
                handle: replaced.session.clone(),
                attached: true,
                closed: None,
            },
        );
        self.inner.sink.invalidate(&request.handle);
        let receipt = ContextChangeReceipt {
            session: self.session_view(ctx, &replaced.session).await?,
            replaced_session_id: Some(replaced.replaced_session_id),
            attachment_token: Some(replaced.attachment_token),
        };
        receipts.insert(
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
        self.authorize(ctx, AuthorizationAction::Connect).await?;
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
        if let Some(physical) =
            lock(&self.inner.physical).get(&convert::handle(&request.handle)?.runtime_epoch.get())
        {
            receipt.resource_release = match physical.release {
                Some(crate::registry::backend::CloseResourceOutcome::Closed) => {
                    ResourceRelease::Confirmed
                }
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
        }
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
