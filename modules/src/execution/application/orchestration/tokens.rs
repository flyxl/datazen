use super::*;
use crate::execution::{IdempotentOperation, SubmissionToken};
#[derive(Clone)]
pub(super) struct IssuedToken {
    ctx: RequestContext,
    operation: IdempotentOperation,
    epoch: Counter,
    handle: Option<SessionHandle>,
    pub(super) expires_at_nanos: u64,
}
fn operation(value: IdempotentOperation) -> SubmissionOperation {
    match value {
        IdempotentOperation::CreateProfile => SubmissionOperation::OpenSession,
        IdempotentOperation::OpenSession => SubmissionOperation::OpenSession,
        IdempotentOperation::ExecuteInSession => SubmissionOperation::ExecuteInSession,
        IdempotentOperation::CancelExecution => SubmissionOperation::CancelExecution,
        IdempotentOperation::ExecuteAtTarget => SubmissionOperation::OpenSession,
        IdempotentOperation::SetSessionContext => SubmissionOperation::SetContext,
        IdempotentOperation::StartJob => SubmissionOperation::OpenSession,
    }
}
impl RuntimeConnectionUseCases {
    pub fn validate_cancel_submission(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
        execution_id: &ExecutionId,
    ) -> Result<(), ApiError> {
        let handle = self.inner.sink.handle(ctx, execution_id)?;
        self.validate_token(
            ctx,
            key,
            IdempotentOperation::CancelExecution,
            Some(&handle),
        )
    }
    pub fn validate_profile_submission(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
    ) -> Result<(), ApiError> {
        self.validate_token(ctx, key, IdempotentOperation::CreateProfile, None)
    }
    pub(super) async fn issue(
        &self,
        ctx: &RequestContext,
        op: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, ApiError> {
        self.authorize(ctx, AuthorizationAction::Connect).await?;
        let epoch = match handle {
            Some(h) => {
                self.session_view(ctx, h).await?;
                convert::handle(h)?.runtime_epoch
            }
            None => Counter::new(0),
        };
        if matches!(
            op,
            IdempotentOperation::ExecuteInSession
                | IdempotentOperation::SetSessionContext
                | IdempotentOperation::CancelExecution
        ) && handle.is_none()
        {
            return Err(ApiError::invalid_argument(
                "session handle required for operation",
            ));
        }
        let key = self
            .inner
            .keyring
            .issue(
                operation(op),
                if matches!(
                    op,
                    IdempotentOperation::CreateProfile | IdempotentOperation::StartJob
                ) {
                    None
                } else {
                    Some(epoch)
                },
                self.inner.clock.now_nanos(),
                DEFAULT_TTL_NANOS,
            )
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "submission token unavailable",
                )
            })?;
        let now = self.inner.clock.now_nanos();
        {
            let mut tokens = lock(&self.inner.tokens);
            tokens.retain(|_, token| token.expires_at_nanos > now);
            if tokens.len() >= 4096 {
                return Err(ApiError::new(
                    ApiErrorCode::ResourceBusy,
                    "submission token capacity reached",
                ));
            }
            tokens.insert(
                token_digest(&key),
                IssuedToken {
                    ctx: ctx.clone(),
                    operation: op,
                    epoch,
                    handle: handle.cloned(),
                    expires_at_nanos: now.saturating_add(DEFAULT_TTL_NANOS),
                },
            );
        }
        self.reclaim_receipts().await;
        let expires = std::time::SystemTime::now()
            .checked_add(std::time::Duration::from_secs(86400))
            .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|v| v.as_millis() as i64)
            .unwrap_or(0);
        let clock = crate::execution::runtime::directory::ClockBase::new(
            crate::execution::runtime::directory::MonoInstant::ZERO,
            expires,
        );
        Ok(SubmissionToken {
            idempotency_key: IdempotencyKey::new(key),
            expires_at: clock.project(crate::execution::runtime::directory::MonoInstant::ZERO),
        })
    }
    pub(super) async fn coordinate_admission(
        &self,
        key: &str,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, ApiError> {
        let mutex = {
            let mut keys = lock(&self.inner.target_admissions);
            keys.retain(|_, value| value.strong_count() > 0);
            if let Some(mutex) = keys.get(key).and_then(std::sync::Weak::upgrade) {
                mutex
            } else {
                if keys.len() >= 4096 {
                    return Err(ApiError::new(
                        ApiErrorCode::ResourceBusy,
                        "admission capacity reached",
                    ));
                }
                let mutex = Arc::new(tokio::sync::Mutex::new(()));
                keys.insert(key.into(), Arc::downgrade(&mutex));
                mutex
            }
        };
        Ok(mutex.lock_owned().await)
    }
    pub(super) async fn reclaim_receipts(&self) {
        let now = self.inner.clock.now_nanos();
        let active_tokens: std::collections::HashSet<String> = lock(&self.inner.tokens)
            .iter()
            .filter(|(_, token)| token.expires_at_nanos > now)
            .map(|(key, _)| key.clone())
            .collect();
        let active_admissions: std::collections::HashSet<String> = {
            let mut keys = lock(&self.inner.target_admissions);
            keys.retain(|_, mutex| mutex.strong_count() > 0);
            keys.keys().cloned().collect()
        };
        self.inner.receipts.lock().await.retain(|key, _| {
            active_tokens.contains(&token_digest(key)) || active_admissions.contains(key)
        });
        let active_executions = lock(&self.inner.sink.0)
            .values()
            .filter(|e| !e.view.state.is_terminal())
            .map(|e| e.view.execution_id.clone())
            .collect();
        let live_sessions = lock(&self.inner.sessions)
            .values()
            .filter(|e| e.closed.is_none())
            .map(|e| e.handle.db_session_id.as_str().to_owned())
            .collect();
        self.inner
            .gateway
            .reclaim_external_receipts(
                |key| active_tokens.contains(&token_digest(key)) || active_admissions.contains(key),
                &active_executions,
                &live_sessions,
            )
            .await;
    }
    pub(super) fn validate_token(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
        op: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> Result<(), ApiError> {
        let token = lock(&self.inner.tokens)
            .get(&token_digest(key.as_str()))
            .cloned()
            .ok_or_else(|| {
                ApiError::new(
                    ApiErrorCode::IdempotencyExpired,
                    "submission token is not active in this runtime",
                )
            })?;
        if token.ctx.organization_id != ctx.organization_id
            || token.ctx.principal_id != ctx.principal_id
            || token.ctx.client_instance_id != ctx.client_instance_id
            || token.operation != op
            || token.handle.as_ref() != handle
        {
            return Err(ApiError::permission_denied(
                "submission token scope mismatch",
            ));
        }
        self.inner
            .keyring
            .verify(
                key.as_str(),
                self.inner.clock.now_nanos(),
                if op.binds_runtime_epoch() {
                    Some(token.epoch)
                } else {
                    None
                },
            )
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::IdempotencyExpired,
                    "submission token expired or invalid",
                )
            })?;
        Ok(())
    }
}
