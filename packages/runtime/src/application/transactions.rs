//! Compatibility transaction controls use the same actor/resource handle ledger
//! as SQL execution. No driver TransactionHandle escapes into a transport map.
use super::*;
use datazen_application::{
    dto::requests::{CommandCall, ExecuteInSessionRequest},
    sessions::ConnectionUseCases,
};
use datazen_platform_api::dto::idempotency::IdempotentOperation;

#[derive(Clone, Copy)]
pub enum SessionTransactionOperation {
    Begin,
    Commit,
    Rollback,
}
impl RuntimeConnectionUseCases {
    fn fence_transaction(&self, handle: &SessionHandle) {
        let sessions = lock(&self.inner.sessions);
        if sessions
            .get(&handle.db_session_id)
            .is_some_and(|entry| entry.handle == *handle && entry.closed.is_none())
        {
            lock(&self.inner.transaction_unknown).insert(handle.clone());
        }
    }

    /// The spawned operation owns its serialization guard after caller cancellation.
    /// Otherwise a dropped IPC could release the lock while BEGIN is still running.
    pub async fn session_transaction(
        &self,
        ctx: &RequestContext,
        session_id: &str,
        operation: SessionTransactionOperation,
    ) -> Result<(), ApiError> {
        self.authorize(ctx, AuthorizationAction::Write).await?;
        let handle = self.get_session_by_id(ctx, session_id).await?.handle;
        let gate = {
            let mut gates = lock(&self.inner.transaction_gates);
            gates.retain(|_, gate| gate.strong_count() > 0);
            if let Some(gate) = gates.get(&handle).and_then(std::sync::Weak::upgrade) {
                gate
            } else {
                let gate = Arc::new(tokio::sync::Mutex::new(()));
                gates.insert(handle.clone(), Arc::downgrade(&gate));
                gate
            }
        };
        let service = self.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let _guard = gate.lock().await;
            service
                .run_session_transaction(&ctx, handle, operation)
                .await
        })
        .await
        .map_err(|_| {
            ApiError::new(
                ApiErrorCode::OutcomeUnknown,
                "transaction control stopped; verify original session",
            )
        })?
    }
    async fn run_session_transaction(
        &self,
        ctx: &RequestContext,
        handle: SessionHandle,
        operation: SessionTransactionOperation,
    ) -> Result<(), ApiError> {
        use datazen_platform_api::dto::session::TransactionState;
        let before = self.get_session(ctx, handle.clone()).await?;
        if lock(&self.inner.transaction_unknown).contains(&handle)
            && !matches!(operation, SessionTransactionOperation::Rollback)
        {
            return Err(ApiError::new(
                ApiErrorCode::OutcomeUnknown,
                "transaction outcome requires verification",
            ));
        }
        match (operation, before.observed_context.transaction_state) {
            (_, TransactionState::Unsupported) => {
                return Err(ApiError::new(
                    ApiErrorCode::CapabilityUnsupported,
                    "session transactions unsupported",
                ))
            }
            (SessionTransactionOperation::Begin, TransactionState::Active) => return Ok(()),
            (
                SessionTransactionOperation::Begin,
                TransactionState::Aborted | TransactionState::Unknown,
            )
            | (SessionTransactionOperation::Commit, TransactionState::Unknown) => {
                return Err(ApiError::new(
                    ApiErrorCode::OutcomeUnknown,
                    "transaction state requires rollback or verification",
                ))
            }
            (
                SessionTransactionOperation::Commit | SessionTransactionOperation::Rollback,
                TransactionState::None,
            ) => return Err(ApiError::invalid_argument("No open transaction")),
            _ => {}
        }
        let command = match operation {
            SessionTransactionOperation::Begin => "platform.beginTransaction",
            SessionTransactionOperation::Commit => "platform.commitTransaction",
            SessionTransactionOperation::Rollback => "platform.rollbackTransaction",
        };
        let token = self
            .issue_submission_token(ctx, IdempotentOperation::ExecuteInSession, Some(&handle))
            .await?;
        // Fence before enqueue: a stopped waiter/worker cannot turn a pending
        // control into permission to replay it. Clear only on confirmed success.
        self.fence_transaction(&handle);
        let receipt = self
            .execute_in_session(
                ctx,
                ExecuteInSessionRequest::new(
                    handle.clone(),
                    CommandCall::new(command, serde_json::json!({"defersCommit":true})),
                    token.idempotency_key,
                ),
            )
            .await?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        let observed = tokio::time::timeout_at(deadline, async {
            loop {
                let view = self
                    .get_execution(ctx, receipt.execution_id.clone())
                    .await?;
                if view.state.is_terminal() {
                    return Ok::<_, ApiError>(view);
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await;
        let result = match observed {
            Ok(Ok(result)) => result,
            _ => {
                self.fence_transaction(&handle);
                return Err(ApiError::new(
                    ApiErrorCode::OutcomeUnknown,
                    "transaction receipt unconfirmed; never replay control",
                ));
            }
        };
        let effect_confirmed = match operation {
            SessionTransactionOperation::Begin => matches!(
                result.effect_outcome,
                EffectOutcome::NotStarted | EffectOutcome::Completed
            ),
            SessionTransactionOperation::Commit => {
                result.effect_outcome == EffectOutcome::Completed
            }
            SessionTransactionOperation::Rollback => {
                result.effect_outcome == EffectOutcome::RolledBack
            }
        };
        if result.state != ExecutionState::Succeeded || !effect_confirmed {
            self.fence_transaction(&handle);
            return Err(ApiError::new(
                ApiErrorCode::OutcomeUnknown,
                "transaction control outcome requires verification",
            ));
        }
        let after = self.get_session(ctx, handle.clone()).await?;
        let confirmed = match operation {
            SessionTransactionOperation::Begin => {
                after.observed_context.transaction_state == TransactionState::Active
            }
            _ => after.observed_context.transaction_state == TransactionState::None,
        };
        if !confirmed {
            self.fence_transaction(&handle);
            return Err(ApiError::new(
                ApiErrorCode::OutcomeUnknown,
                "transaction state not confirmed",
            ));
        }
        lock(&self.inner.transaction_unknown).remove(&handle);
        Ok(())
    }
    /// Expired sessions have no transaction to control; ownership errors remain errors.
    pub async fn session_transaction_status(
        &self,
        ctx: &RequestContext,
        session_id: &str,
    ) -> Result<bool, ApiError> {
        let view = match self.get_session_by_id(ctx, session_id).await {
            Ok(view) => view,
            Err(error)
                if matches!(
                    error.code,
                    ApiErrorCode::SessionNotFound
                        | ApiErrorCode::SessionLost
                        | ApiErrorCode::NotFound
                ) =>
            {
                return Ok(false)
            }
            Err(error) => return Err(error),
        };
        if lock(&self.inner.transaction_unknown).contains(&view.handle)
            || view.observed_context.transaction_state
                == datazen_platform_api::dto::session::TransactionState::Unknown
        {
            return Err(ApiError::new(
                ApiErrorCode::OutcomeUnknown,
                "transaction status unconfirmed",
            ));
        }
        Ok(matches!(
            view.observed_context.transaction_state,
            datazen_platform_api::dto::session::TransactionState::Active
                | datazen_platform_api::dto::session::TransactionState::Aborted
        ))
    }
}
