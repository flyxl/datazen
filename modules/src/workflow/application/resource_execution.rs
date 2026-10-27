//! Workflow SQL steps use a short, independent resource through the execution gateway.
use crate::shared::context::RequestContext;
use crate::table_editing::ports::ArtifactChunkReader;
use crate::workflow::domain::command::WorkflowCommandStep;
use crate::workflow::domain::error::WorkflowError;
use datazen_driver_api::{CommandCatalog, ConnectionConfig, DatabaseDriver};
use std::sync::Arc;

/// 一次派发所需的**端口**集合。
///
/// 它曾经直接持有 `datazen::execution::application::RuntimeConnectionUseCases`——
/// 那让「Workflow 结果组装」这个纯共享逻辑被钉在连接运行时上（R1）。现在它只
/// 持有两个消费者定义端口的 trait 对象，具体实例由宿主装配。
pub struct PreparedExecution {
    pub use_cases: Arc<dyn ConnectionUseCases>,
    pub artifacts: Arc<dyn ArtifactChunkReader>,
    pub context: RequestContext,
    pub target: ExecutionTarget,
}
#[async_trait::async_trait]
pub trait ResourceExecutionHost: Send + Sync {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError>;
    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError>;
    async fn safe_mode(&self) -> bool;
    async fn prepare(
        &self,
        connection_id: &str,
        revision: u64,
        command: &str,
        target: ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError>;
}

use crate::connection::domain::NamespaceTarget;
use crate::execution::application::requests::{CommandCall, ExecuteAtTargetRequest};
use crate::execution::application::ConnectionUseCases;
use crate::execution::application::IdempotentOperation;
use crate::execution::domain::target::{ExecutionTarget, ObjectTarget};
use crate::execution::domain::ExecutionState;
use crate::shared::id::{ConnectionId, Counter};
use datazen_driver_api::{check_command_access, validate_command_input, CommandResult};
use std::collections::BTreeMap;

struct CancelOnDrop {
    use_cases: std::sync::Arc<dyn ConnectionUseCases>,
    context: crate::shared::context::RequestContext,
    execution: Option<crate::shared::id::ExecutionId>,
}
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let Some(execution) = self.execution.take() else {
            return;
        };
        let use_cases = self.use_cases.clone();
        let context = self.context.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(error) = use_cases.cancel_execution(&context, execution).await {
                    tracing::warn!(code = ?error.code, "workflow cancellation could not be confirmed");
                }
            });
        }
    }
}

/// No SQL submission has started, so an expired deadline is safe to recover from.
async fn before_dispatch<T>(
    deadline: tokio::time::Instant,
    future: impl std::future::Future<Output = T>,
) -> Result<T, WorkflowError> {
    tokio::time::timeout_at(deadline, future)
        .await
        .map_err(|_| {
            WorkflowError::Step("SQL step timed out before dispatch; no SQL submitted".into())
        })
}

pub async fn execute(
    state: &dyn ResourceExecutionHost,
    step: &WorkflowCommandStep,
    connection_id: &str,
    permission_mode: Option<crate::mcp::domain::permission::McpPermissionMode>,
    workflow_deadline: Option<tokio::time::Instant>,
    require_workflow: bool,
) -> Result<CommandResult, WorkflowError> {
    execute_bound(
        state,
        step,
        connection_id,
        permission_mode,
        workflow_deadline,
        require_workflow,
        SessionBinding::Independent,
    )
    .await
}

/// How one step binds to a runtime session.
///
/// A borrowed session belongs to someone else (for example the user's editor
/// tab). The executor must never close it: closing a session is the owner's
/// decision, and a tool call that quietly rolled back the editor's transaction
/// would be worse than refusing to run.
enum SessionBinding {
    Independent,
    Borrowed(crate::execution::domain::SessionHandle),
}

impl SessionBinding {
    /// The operation this binding submits under, plus the handle it owns.
    ///
    /// Extracted so "a borrowed session must never degrade into an independent
    /// execution" is a property a test can pin, rather than a shape that has to
    /// be re-read from the middle of a long function.
    fn submission(
        &self,
    ) -> (
        IdempotentOperation,
        Option<crate::execution::domain::SessionHandle>,
    ) {
        match self {
            Self::Independent => (IdempotentOperation::ExecuteAtTarget, None),
            Self::Borrowed(handle) => (IdempotentOperation::ExecuteInSession, Some(handle.clone())),
        }
    }
}

/// Run one step on a session that the caller owns. The executor borrows the
/// handle only; it never closes or rolls back that session.
pub async fn execute_with_existing_session(
    state: &dyn ResourceExecutionHost,
    step: &WorkflowCommandStep,
    connection_id: &str,
    permission_mode: Option<crate::mcp::domain::permission::McpPermissionMode>,
    workflow_deadline: Option<tokio::time::Instant>,
    require_workflow: bool,
    handle: crate::execution::domain::SessionHandle,
) -> Result<CommandResult, WorkflowError> {
    execute_bound(
        state,
        step,
        connection_id,
        permission_mode,
        workflow_deadline,
        require_workflow,
        SessionBinding::Borrowed(handle),
    )
    .await
}

async fn execute_bound(
    state: &dyn ResourceExecutionHost,
    step: &WorkflowCommandStep,
    connection_id: &str,
    permission_mode: Option<crate::mcp::domain::permission::McpPermissionMode>,
    workflow_deadline: Option<tokio::time::Instant>,
    require_workflow: bool,
    binding: SessionBinding,
) -> Result<CommandResult, WorkflowError> {
    let step_deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(step.timeout_secs.unwrap_or(30));
    let global_limit = workflow_deadline.is_some_and(|deadline| deadline <= step_deadline);
    let deadline = workflow_deadline.map_or(step_deadline, |deadline| deadline.min(step_deadline));
    let (config, revision, enabled) =
        before_dispatch(deadline, state.profile(connection_id)).await??;
    if !enabled {
        return Err(WorkflowError::Validation("connection disabled".into()));
    }
    let driver = before_dispatch(deadline, state.driver(&config)).await??;
    // Command description only; execution keeps its own lifecycle below.
    let catalog = datazen_driver_api::LegacyCommandCatalog::new(driver.as_ref());
    // Query steps and SQL Command steps converge here after resolving workflow
    // inheritance and validating explicit namespace fields. Require a named
    // database before either path can acquire an independent SQL resource.
    if matches!(step.command.as_str(), "query" | "execute" | "explain")
        && super::command_runtime::database_is_required(
            step.input
                .get("database")
                .and_then(serde_json::Value::as_str),
            config.database.as_deref(),
            driver.has_multi_database(),
        )
    {
        return Err(WorkflowError::MissingDatabase {
            step_id: step.id.clone(),
            connection_id: connection_id.into(),
        });
    }
    let definition = catalog
        .command_definitions()
        .into_iter()
        .chain(datazen_driver_api::schema_catalog_command_definitions())
        .chain(crate::schema::application::metadata::command_definitions())
        .find(|definition| definition.id == step.command)
        .ok_or_else(|| WorkflowError::UnsupportedCommand {
            command: step.command.clone(),
            connection_id: connection_id.into(),
        })?;
    if require_workflow && !definition.metadata.workflow {
        return Err(WorkflowError::CommandNotInWorkflow {
            command: step.command.clone(),
        });
    }
    validate_command_input(&definition, &step.input).map_err(WorkflowError::Validation)?;
    check_command_access(
        &definition,
        crate::mcp::application::command_policy::access_level_for_mode(permission_mode),
    )
    .map_err(WorkflowError::Validation)?;
    if matches!(step.command.as_str(), "query" | "execute" | "explain") {
        let sql = step
            .input
            .get("sql")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| WorkflowError::Validation("SQL input unavailable".into()))?;
        crate::execution::domain::sql_guard::check_sql(
            sql,
            config.read_only,
            before_dispatch(deadline, state.safe_mode()).await?,
        )
        .map_err(WorkflowError::Validation)?;
        if let Some(mode) = permission_mode {
            crate::mcp::domain::permission::check_sql_allowed(sql, mode)
                .map_err(WorkflowError::Validation)?;
        }
    }
    let namespace = NamespaceTarget::new(
        step.input
            .get("database")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .or(config.database),
        None,
        step.input
            .get("schema")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .or(config.schema),
        vec![],
    );
    let mut target = ExecutionTarget::new(ConnectionId::new(connection_id), namespace);
    if step.command == "get_table_schema" {
        let table = step
            .input
            .get("table")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| WorkflowError::Validation("Table identity unavailable".into()))?;
        target = target.with_object(ObjectTarget::new("table", table));
    }

    let prepared = before_dispatch(
        deadline,
        state.prepare(connection_id, revision, &step.command, target),
    )
    .await??;
    let use_cases = prepared.use_cases;
    let artifacts = prepared.artifacts;
    let context = prepared.context;
    let target = prepared.target;
    let (submission_operation, bound_handle) = binding.submission();
    let token = before_dispatch(
        deadline,
        use_cases.issue_submission_token(&context, submission_operation, bound_handle.as_ref()),
    )
    .await?
    .map_err(|error| WorkflowError::Validation(error.to_string()))?;
    // Check explicitly before polling submission: timeout_at may poll an already
    // ready future even when its deadline has elapsed.
    if tokio::time::Instant::now() >= deadline {
        return Err(WorkflowError::Step(
            "SQL step timed out before dispatch; no SQL submitted".into(),
        ));
    }
    let submission_unconfirmed = |code: String| WorkflowError::SqlUnconfirmed {
        // Submission may have been accepted even if its receipt is lost.
        execution_id: "unavailable".into(),
        state: "Unknown".into(),
        effects: "Unknown".into(),
        code,
    };
    let mut command_input = step.input.clone();
    if step.command == "explain" {
        // Namespace fields have already selected and validated `target` above.
        // Explain executes in that retained resource context; its command API
        // intentionally rejects a second, legacy namespace override.
        if let Some(input) = command_input.as_object_mut() {
            input.remove("database");
            input.remove("schema");
        }
    }
    let submission = async {
        if let Some(handle) = bound_handle.clone() {
            use_cases
                .execute_in_session(
                    &context,
                    crate::execution::application::requests::ExecuteInSessionRequest::new(
                        handle,
                        CommandCall::new(&step.command, command_input),
                        token.idempotency_key,
                    ),
                )
                .await
        } else {
            use_cases
                .execute_at_target(
                    &context,
                    ExecuteAtTargetRequest::new(
                        target,
                        Counter::new(revision),
                        CommandCall::new(&step.command, command_input),
                        token.idempotency_key,
                    ),
                )
                .await
        }
    };
    let receipt = tokio::time::timeout_at(deadline, submission)
        .await
        .map_err(|_| submission_unconfirmed("SubmissionReceiptTimeout".into()))?
        .map_err(|error| submission_unconfirmed(format!("Submission:{:?}", error.code)))?;
    let mut cancellation = CancelOnDrop {
        use_cases: use_cases.clone(),
        context: context.clone(),
        execution: Some(receipt.execution_id.clone()),
    };
    let started = std::time::Instant::now();
    let mut last_state = "Accepted".to_owned();
    let mut last_effects = "Unknown".to_owned();
    let unconfirmed = |state: &str, effects: &str, code: String| WorkflowError::SqlUnconfirmed {
        execution_id: receipt.execution_id.as_str().to_owned(),
        state: state.to_owned(),
        effects: effects.to_owned(),
        code,
    };
    let mut timed_out = false;
    let mut poll_deadline = deadline;
    let view = loop {
        // timeout_at polls its inner future first. A ready registry lookup
        // can therefore win even after expiry on every iteration. Enforce the
        // deadline explicitly before polling, including the terminal grace period.
        let polled = if tokio::time::Instant::now() >= poll_deadline {
            Err(())
        } else {
            tokio::time::timeout_at(
                poll_deadline,
                use_cases.get_execution(&context, receipt.execution_id.clone()),
            )
            .await
            .map_err(|_| ())
        };
        match polled {
            Ok(Ok(view)) => {
                last_state = format!("{:?}", view.state);
                last_effects = format!("{:?}", view.effect_outcome);
                if view.state.is_terminal() {
                    break view;
                }
            }
            Ok(Err(error)) => {
                return Err(unconfirmed(
                    &last_state,
                    &last_effects,
                    format!("{:?}", error.code),
                ))
            }
            Err(_) if timed_out => {
                return Err(unconfirmed(
                    &last_state,
                    &last_effects,
                    "TimeoutAwaitingTerminal".into(),
                ))
            }
            Err(_) => {
                timed_out = true;
                // A cancellation receipt only acknowledges the request. Keep
                // polling until the execution itself confirms a terminal state.
                let cancelled = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    use_cases.cancel_execution(&context, receipt.execution_id.clone()),
                )
                .await;
                match cancelled {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => {
                        return Err(unconfirmed(
                            &last_state,
                            &last_effects,
                            format!("Cancel:{:?}", error.code),
                        ))
                    }
                    Err(_) => {
                        return Err(unconfirmed(
                            &last_state,
                            &last_effects,
                            "CancelAcknowledgementTimeout".into(),
                        ))
                    }
                }
                poll_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    cancellation.execution = None;
    if timed_out || view.state != ExecutionState::Succeeded {
        return Err(WorkflowError::SqlExecution {
            execution_id: receipt.execution_id.as_str().to_owned(),
            state: format!("{:?}", view.state),
            effects: format!("{:?}", view.effect_outcome),
            code: if timed_out {
                format!(
                    "{}:{:?}",
                    if global_limit {
                        "GlobalTimeout"
                    } else {
                        "StepTimeout"
                    },
                    view.error_code
                )
            } else {
                format!("{:?}", view.error_code)
            },
        });
    }
    // Reading a result does not execute SQL again. Preserve the known effect
    // when storage or transport fails after the statement has completed.
    let result_read_error = |error: crate::shared::error::ApiError| WorkflowError::SqlResultRead {
        execution_id: receipt.execution_id.as_str().to_owned(),
        effects: format!("{:?}", view.effect_outcome),
        code: format!("{:?}", error.code),
    };
    // Terminal is known, so result transport timeout can safely use Skip/Fallback.
    // One deadline covers all chunks rather than extending on every read.
    let read_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let read_deadline =
        workflow_deadline.map_or(read_deadline, |deadline| deadline.min(read_deadline));
    let read_timeout = || WorkflowError::SqlResultRead {
        execution_id: receipt.execution_id.as_str().to_owned(),
        effects: format!("{:?}", view.effect_outcome),
        code: "ArtifactReadTimeout".into(),
    };
    let statements = tokio::time::timeout_at(read_deadline, async {
        let mut statements = BTreeMap::<u64, serde_json::Value>::new();
        let mut command_output = None;
        for artifact in view.artifact_ids {
            let first = artifacts
                .read_chunk(&context, &artifact, Counter::ZERO)
                .await
                .map_err(&result_read_error)?;
            let count = first.published_chunk_count.get();
            for index in 0..count {
                if tokio::time::Instant::now() >= read_deadline {
                    return Err(read_timeout());
                }
                let chunk = if index == 0 {
                    first.clone()
                } else {
                    artifacts
                        .read_chunk(&context, &artifact, Counter::new(index))
                        .await
                        .map_err(&result_read_error)?
                };
                let Some(output) = chunk.output else { continue };
                if output["kind"] == "commandResult" {
                    command_output = Some(output["data"].clone());
                    continue;
                }
                let Some(statement_index) = output
                    .get("statementIndex")
                    .and_then(serde_json::Value::as_u64)
                else {
                    continue;
                };
                let statement = statements.entry(statement_index).or_insert_with(|| {
                    serde_json::json!({
                        "sql":"", "columns":[], "rows":[], "executionTimeMs":0, "truncated":false,
                    })
                });
                if output["kind"] == "rows" || output["kind"] == "statementSummary" {
                    if output["sql"].is_string() {
                        statement["sql"] = output["sql"].clone();
                    }
                }
                if output["kind"] == "rows" {
                    if let (Some(rows), Some(incoming)) =
                        (statement["rows"].as_array_mut(), output["rows"].as_array())
                    {
                        rows.extend(incoming.iter().cloned());
                    }
                } else if output["kind"] == "statementMetadata" {
                    statement["columns"] = output["columns"].clone();
                    statement["truncated"] = output["truncated"].clone();
                }
                if !output["rowsAffected"].is_null() {
                    statement["rowsAffected"] = output["rowsAffected"].clone();
                }
            }
        }
        Ok::<_, WorkflowError>((statements, command_output))
    })
    .await
    .map_err(|_| read_timeout())??;
    if let Some(output) = statements.1 {
        return Ok(CommandResult::new(output));
    }
    Ok(CommandResult::new(serde_json::json!({
        "results": statements.0.into_values().collect::<Vec<_>>(),
        "totalTimeMs": started.elapsed().as_millis() as u64,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::domain::SessionHandle;
    use crate::shared::id::{DbSessionId, RuntimeEpoch};

    fn handle(id: &str) -> SessionHandle {
        SessionHandle::new(DbSessionId::new(id), RuntimeEpoch::new("1"))
    }

    #[test]
    fn independent_binding_submits_at_target_without_a_handle() {
        let (operation, bound) = SessionBinding::Independent.submission();
        assert_eq!(operation, IdempotentOperation::ExecuteAtTarget);
        assert!(
            bound.is_none(),
            "independent execution must not carry a handle"
        );
    }

    #[test]
    fn borrowed_binding_always_submits_in_session() {
        // The property that matters: a granted (borrowed) session must never
        // silently degrade into an independent execution. Downgrading this to
        // ExecuteAtTarget would still compile and would quietly stop sharing the
        // editor's session, so it is pinned by an assertion instead.
        let (operation, bound) = SessionBinding::Borrowed(handle("dbs-1")).submission();
        assert_eq!(operation, IdempotentOperation::ExecuteInSession);
        assert_eq!(
            bound.map(|handle| handle.db_session_id.as_str().to_owned()),
            Some("dbs-1".to_string())
        );
    }
}
