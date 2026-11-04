//! Restore through the same fixed Session / ExecutionGateway as ordinary SQL.
//!
//! D6 item 5 切片 b 自 modules 归位（唯一定义在此）。
use super::restore_job::{
    RestoreEvidence, RestoreGateway, RestoreInput, RestorePlan, RestoreRejection,
};
use crate::execution::application::requests::*;
use crate::execution::application::use_cases::ConnectionUseCases;
use crate::execution::application::{ExecutionView, IdempotentOperation};
use crate::execution::domain::{ContextConfidence, EffectOutcome, ExecutionState, SessionHandle};
use crate::jobs::domain::job::JobProgress;
use crate::jobs::{CancelToken, StageTerminal};
use crate::shared::context::{OwnerRef, RequestContext};
use crate::shared::id::{Counter, ExecutionId};
use datazen_driver_api::{restore_sql, DatabaseDriver};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Host authorization is rechecked before session admission and before writes.
#[async_trait::async_trait]
pub trait RestoreAuthorization: Send + Sync {
    async fn validate(
        &self,
        ctx: &RequestContext,
        plan: &RestorePlan,
    ) -> Result<(), RestoreRejection>;
}

/// Shared production orchestration over a retained Runtime Session.
pub struct RuntimeRestoreGateway {
    pub runtime: Arc<dyn ConnectionUseCases>,
    pub authorization: Arc<dyn RestoreAuthorization>,
    pub driver: Arc<dyn DatabaseDriver>,
    pub profile: String,
}

enum CommandFailure {
    NotSubmitted,
    OutcomeUnknown,
}
impl RuntimeRestoreGateway {
    async fn command(
        &self,
        ctx: &RequestContext,
        session: &SessionHandle,
        command: &str,
        sql: &str,
        cancel: &CancelToken,
        executions: &mut Vec<ExecutionId>,
    ) -> Result<ExecutionView, CommandFailure> {
        let runtime = self.runtime.clone();
        let token = runtime
            .issue_submission_token(ctx, IdempotentOperation::ExecuteInSession, Some(session))
            .await
            .map_err(|error| {
                tracing::warn!(%error, "restore submission token was refused");
                CommandFailure::NotSubmitted
            })?;
        let receipt = runtime
            .execute_in_session(
                ctx,
                ExecuteInSessionRequest::new(
                    session.clone(),
                    CommandCall::new(command, serde_json::json!({"sql":sql})),
                    token.idempotency_key,
                ),
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, %command, "restore submission was refused");
                CommandFailure::NotSubmitted
            })?;
        executions.push(receipt.execution_id.clone());
        let mut cancel_sent = false;
        loop {
            let view = runtime
                .get_execution(ctx, receipt.execution_id.clone())
                .await
                .map_err(|_| CommandFailure::OutcomeUnknown)?;
            if view.state.is_terminal() {
                return Ok(view);
            }
            if cancel.is_cancelled() && !cancel_sent {
                // This targets the accepted execution only. A request never proves rollback.
                let _ = runtime
                    .cancel_execution(ctx, receipt.execution_id.clone())
                    .await;
                cancel_sent = true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    async fn run(
        &self,
        ctx: &RequestContext,
        plan: &RestorePlan,
        input: Arc<dyn RestoreInput>,
        cancel: &CancelToken,
        evidence: &mut RestoreEvidence,
    ) -> Option<SessionHandle> {
        let mut bytes = Vec::new();
        for index in 0..input.chunks() {
            if cancel.is_cancelled() {
                evidence.terminal = StageTerminal::Cancelled;
                return None;
            }
            let Ok(chunk) = input.chunk(Counter::new(index)).await else {
                tracing::warn!(
                    chunk = index,
                    "restore gateway could not read the staged input"
                );
                return None;
            };
            bytes.extend(chunk);
        }
        let Ok(sql) = String::from_utf8(bytes) else {
            tracing::warn!("restore gateway rejected a non-UTF-8 restore script");
            return None;
        };
        // Preflight text rules come from the backup-dialect contract; the
        // driver stays in the gateway for execution and authorization.
        let dialect = datazen_driver_api::LegacyBackupDialect::new(self.driver.as_ref());
        let prepared = restore_sql::prepare(
            &dialect,
            &sql,
            &self.profile,
            plan.single_transaction,
            plan.overwrite,
            &datazen_driver_api::namespace::NamespaceTarget {
                database: plan.target.namespace.database.clone(),
                catalog: plan.target.namespace.catalog.clone(),
                schema: plan.target.namespace.schema.clone(),
                path: plan.target.namespace.path.clone(),
            },
        );
        let statements = match prepared {
            Ok(statements) => statements,
            Err(error) => {
                // The script failed the backup-dialect preflight (unsupported
                // statement, ownership/`CREATE DATABASE` intent, or a split that
                // cannot be replayed). Without this branch every such rejection
                // surfaced only as `restoreInvalidInput` with no cause.
                tracing::warn!(
                    bytes = sql.len(),
                    profile = %self.profile,
                    single_transaction = plan.single_transaction,
                    overwrite = plan.overwrite,
                    %error,
                    "restore gateway rejected the script in preflight"
                );
                return None;
            }
        };
        if self.validate(ctx, plan).await.is_err() {
            evidence.rejection = Some(RestoreRejection::NotAuthorized);
            return None;
        }
        evidence.rejection = Some(RestoreRejection::RuntimeUnavailable);
        let runtime = self.runtime.clone();
        evidence.rejection = Some(RestoreRejection::AdmissionRejected);
        let Ok(token) = runtime
            .issue_submission_token(ctx, IdempotentOperation::OpenSession, None)
            .await
        else {
            return None;
        };
        let Ok(opened) = runtime
            .open_session(
                ctx,
                OpenSessionRequest::new(
                    plan.target.clone(),
                    OwnerRef::ClientSession {
                        client_instance_id: ctx.client_instance_id.clone(),
                        purpose: "restoreApply".into(),
                    },
                    token.idempotency_key,
                ),
            )
            .await
        else {
            return None;
        };
        let session = opened.session.handle;
        evidence.rejection = Some(RestoreRejection::TargetMismatch);
        if opened.session.config_revision != plan.config_revision {
            return Some(session);
        }
        evidence.rejection = Some(RestoreRejection::RuntimeUnavailable);
        // Materialize the retained resource and observe it before any write.
        let probe = self
            .command(
                ctx,
                &session,
                "query",
                "SELECT 1",
                cancel,
                &mut evidence.executions,
            )
            .await;
        if !matches!(probe, Ok(ref view) if view.state == ExecutionState::Succeeded) {
            return Some(session);
        }
        let Ok(view) = runtime.get_session(ctx, session.clone()).await else {
            return Some(session);
        };
        evidence.rejection = Some(RestoreRejection::TargetMismatch);
        let observed = &view.observed_context;
        let ns = &plan.target.namespace;
        let schema_matches = ns.schema.is_none()
            || observed.namespace.schema == ns.schema
            || observed.search_path.as_ref().and_then(|path| path.first()) == ns.schema.as_ref();
        if observed.confidence != ContextConfidence::Confirmed
            || observed.namespace.database != ns.database
            || !schema_matches
            || observed.transaction_state != crate::execution::domain::TransactionState::None
            || (self.profile == restore_sql::IMPLICIT_COMMIT && observed.autocommit != Some(true))
            || view.config_revision != plan.config_revision
            || self.validate(ctx, plan).await.is_err()
        {
            return Some(session);
        }
        if cancel.is_cancelled() {
            evidence.terminal = StageTerminal::Cancelled;
            return Some(session);
        }
        evidence.rejection = None;
        if plan.single_transaction {
            // Only profiles with transactional DDL pass preflight.
            let begin = self
                .command(
                    ctx,
                    &session,
                    "execute",
                    "BEGIN",
                    cancel,
                    &mut evidence.executions,
                )
                .await;
            if !matches!(begin, Ok(ref view) if view.state == ExecutionState::Succeeded) {
                evidence.effects = if matches!(begin, Err(CommandFailure::NotSubmitted)) {
                    evidence.rejection = Some(RestoreRejection::AdmissionRejected);
                    EffectOutcome::NotStarted
                } else {
                    EffectOutcome::Unknown
                };
                return Some(session);
            }
        }
        let total = statements.len() as u64;
        let mut dispatch_index = 0u64;
        for statement in statements {
            dispatch_index += 1;
            // Which statement the pipeline is on, in the order the dump wrote it:
            // a restore that fails on "missing relation" can only be understood by
            // knowing what came before it on the same session.
            tracing::debug!(
                index = dispatch_index,
                total,
                statement = %statement.chars().take(160).collect::<String>(),
                "restore statement dispatch"
            );
            if cancel.is_cancelled() {
                evidence.terminal = StageTerminal::Cancelled;
                break;
            }
            evidence.progress.attempted = evidence.progress.attempted.saturating_increment();
            // Preserve confirmed earlier commits if admission rejects this
            // statement before it receives any execution record.
            let previous_effect = evidence.effects;
            evidence.effects = EffectOutcome::Unknown;
            match self
                .command(
                    ctx,
                    &session,
                    "execute",
                    &statement,
                    cancel,
                    &mut evidence.executions,
                )
                .await
            {
                Ok(view) if view.state == ExecutionState::Succeeded => {
                    if !plan.single_transaction {
                        evidence.progress.committed =
                            evidence.progress.committed.saturating_increment();
                        evidence.effects = EffectOutcome::PartiallyApplied;
                        evidence.boundaries.push(boundary(
                            plan,
                            evidence.progress.attempted.get(),
                            &statement,
                            &view.execution_id,
                        ));
                    }
                }
                Ok(view) => {
                    tracing::warn!(
                        statement = %statement.chars().take(120).collect::<String>(),
                        state = ?view.state,
                        effect = ?view.effect_outcome,
                        "restore statement did not succeed"
                    );
                    evidence.terminal = if view.state == ExecutionState::Cancelled {
                        StageTerminal::Cancelled
                    } else {
                        StageTerminal::Failed
                    };
                    // Do not erase earlier commits when a later statement failed.
                    evidence.effects = if view.effect_outcome == EffectOutcome::Unknown {
                        EffectOutcome::Unknown
                    } else if evidence.progress.committed.get() > 0 {
                        EffectOutcome::PartiallyApplied
                    } else {
                        view.effect_outcome
                    };
                    break;
                }
                Err(failure) => {
                    tracing::warn!(
                        statement = %statement.chars().take(120).collect::<String>(),
                        submission_unknown = matches!(failure, CommandFailure::OutcomeUnknown),
                        "restore statement did not reach a succeeded state"
                    );
                    match failure {
                        CommandFailure::NotSubmitted => {
                            if previous_effect == EffectOutcome::NotStarted {
                                evidence.rejection = Some(RestoreRejection::AdmissionRejected);
                            }
                            evidence.effects = previous_effect;
                            evidence.terminal = StageTerminal::Failed;
                        }
                        CommandFailure::OutcomeUnknown => {
                            evidence.terminal = StageTerminal::Unknown;
                        }
                    }
                    break;
                }
            }
            // Remain Failed until every planned statement actually completed.
            if evidence.progress.attempted.get() == total {
                evidence.terminal = StageTerminal::Succeeded;
            }
        }
        if plan.single_transaction {
            let complete = evidence.terminal == StageTerminal::Succeeded && !cancel.is_cancelled();
            if cancel.is_cancelled() {
                evidence.terminal = StageTerminal::Cancelled;
            }
            // Cleanup rollback must not itself inherit a cancelled token.
            let cleanup_cancel = CancelToken::new();
            let result = self
                .command(
                    ctx,
                    &session,
                    "execute",
                    if complete { "COMMIT" } else { "ROLLBACK" },
                    &cleanup_cancel,
                    &mut evidence.executions,
                )
                .await;
            match result {
                Ok(view) if view.state == ExecutionState::Succeeded => {
                    evidence.effects = if complete {
                        EffectOutcome::Completed
                    } else {
                        EffectOutcome::RolledBack
                    };
                    if complete {
                        evidence.progress.committed = evidence.progress.attempted;
                        evidence
                            .boundaries
                            .push(boundary(plan, 0, &sql, &view.execution_id));
                    }
                }
                _ => {
                    evidence.effects = EffectOutcome::Unknown;
                    evidence.terminal = StageTerminal::Unknown;
                }
            }
        } else if evidence.terminal == StageTerminal::Succeeded {
            evidence.effects = EffectOutcome::Completed;
        }
        Some(session)
    }
}
#[async_trait::async_trait]
impl RestoreGateway for RuntimeRestoreGateway {
    async fn validate(
        &self,
        ctx: &RequestContext,
        plan: &RestorePlan,
    ) -> Result<(), RestoreRejection> {
        if !matches!(
            self.profile.as_str(),
            restore_sql::TRANSACTIONAL | restore_sql::IMPLICIT_COMMIT
        ) {
            return Err(RestoreRejection::Unsupported);
        }
        self.authorization.validate(ctx, plan).await
    }
    async fn execute(
        &self,
        ctx: &RequestContext,
        plan: &RestorePlan,
        input: Arc<dyn RestoreInput>,
        cancel: &CancelToken,
    ) -> RestoreEvidence {
        let mut evidence = RestoreEvidence {
            rejection: Some(RestoreRejection::InvalidInput),
            terminal: StageTerminal::Failed,
            effects: EffectOutcome::NotStarted,
            progress: JobProgress::default(),
            executions: vec![],
            boundaries: vec![],
        };
        if let Some(handle) = self.run(ctx, plan, input, cancel, &mut evidence).await {
            let runtime = self.runtime.clone();
            let close = runtime
                .close_session(
                    ctx,
                    CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
                )
                .await;
            if !matches!(close, Ok(ref receipt) if receipt.resource_release == crate::execution::domain::ResourceRelease::Confirmed)
            {
                evidence.terminal = StageTerminal::Unknown;
                // Cleanup uncertainty does not erase database evidence:
                // NotStarted, confirmed rollback and commits remain facts.
            }
        }
        if cancel.is_cancelled() && evidence.effects == EffectOutcome::NotStarted {
            evidence.terminal = StageTerminal::Cancelled;
        }
        evidence
    }
}

fn boundary(
    plan: &RestorePlan,
    index: u64,
    payload: &str,
    execution: &ExecutionId,
) -> crate::jobs::domain::job::CommitBoundary {
    crate::jobs::domain::job::CommitBoundary {
        stage_id: crate::shared::id::StageId::new("restoreArtifact"),
        stable_target_fingerprint: RestorePlan::target_fingerprint(&plan.target),
        committed_at: crate::shared::id::Timestamp::new(
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ),
        operation_id: Some(format!("restore-statement-{index}")),
        batch_id: None,
        payload_digest: Some(format!("{:x}", Sha256::digest(payload.as_bytes()))),
        evidence: vec![format!("execution:{}", execution.as_str())],
        verified_at: None,
    }
}
