use super::*;
use datazen_platform_api::dto::idempotency::{IdempotentOperation, SubmissionToken};
#[derive(Clone)]
pub(super) struct IssuedToken {
    ctx: RequestContext,
    operation: IdempotentOperation,
    epoch: Counter,
    handle: Option<SessionHandle>,
    expires_at_nanos: u64,
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
        let mut tokens = lock(&self.inner.tokens);
        tokens.retain(|_, token| token.expires_at_nanos > now);
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
        drop(tokens);
        let expires = std::time::SystemTime::now()
            .checked_add(std::time::Duration::from_secs(86400))
            .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|v| v.as_millis() as i64)
            .unwrap_or(0);
        let clock = crate::directory::ClockBase::new(crate::directory::MonoInstant::ZERO, expires);
        Ok(SubmissionToken {
            idempotency_key: IdempotencyKey::new(key),
            expires_at: clock.project(crate::directory::MonoInstant::ZERO),
        })
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
