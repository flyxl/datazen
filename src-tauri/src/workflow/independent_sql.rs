//! Workflow SQL steps use a short, independent resource through the execution gateway.
use super::{error::WorkflowError, WorkflowCommandStep};
use crate::commands::AppState;
use datazen_application::{
    dto::requests::{CommandCall, ExecuteAtTargetRequest},
    sessions::ConnectionUseCases,
};
use datazen_driver_api::{check_command_access, validate_command_input, CommandResult};
use datazen_platform_api::{
    dto::{execution::ExecutionState, idempotency::IdempotentOperation},
    id::{ConnectionId, Counter},
    target::{ExecutionTarget, NamespaceTarget},
};
use std::collections::BTreeMap;

struct CancelOnDrop {
    runtime: std::sync::Arc<datazen_runtime::application::RuntimeConnectionUseCases>,
    context: datazen_platform_api::context::RequestContext,
    execution: Option<datazen_platform_api::id::ExecutionId>,
}
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let Some(execution) = self.execution.take() else {
            return;
        };
        let runtime = self.runtime.clone();
        let context = self.context.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(error) = runtime.cancel_execution(&context, execution).await {
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
        .map_err(|_| WorkflowError::Step("SQL step timed out before dispatch; no SQL submitted".into()))
}

pub(super) async fn execute(
    state: &AppState,
    step: &WorkflowCommandStep,
    connection_id: &str,
    permission_mode: Option<crate::mcp::permission::McpPermissionMode>,
) -> Result<CommandResult, WorkflowError> {
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(step.timeout_secs.unwrap_or(30));
    let (config, metadata) = before_dispatch(
        deadline, state.store.platform_profile(connection_id),
    ).await?
        .ok_or_else(|| WorkflowError::ConnectionFailed {
            connection_id: connection_id.into(),
            message: "profile unavailable".into(),
        })?;
    if !metadata.enabled {
        return Err(WorkflowError::Validation("connection disabled".into()));
    }
    let driver = before_dispatch(
        deadline,
        state.driver_registry.get_sql_driver_by_name(config.database_type.as_str()),
    ).await?
        .ok_or_else(|| WorkflowError::UnsupportedCommand {
            command: step.command.clone(),
            connection_id: connection_id.into(),
        })?;
    // Query steps and SQL Command steps converge here after resolving workflow
    // inheritance and validating explicit namespace fields. Require a named
    // database before either path can acquire an independent SQL resource.
    if super::workflows::executor::database_is_required(
        step.input.get("database").and_then(serde_json::Value::as_str),
        config.database.as_deref(),
        driver.has_multi_database(),
    ) {
        return Err(WorkflowError::MissingDatabase {
            step_id: step.id.clone(),
            connection_id: connection_id.into(),
        });
    }
    let definition = driver
        .command_definitions()
        .into_iter()
        .find(|definition| definition.id == step.command)
        .ok_or_else(|| WorkflowError::UnsupportedCommand {
            command: step.command.clone(),
            connection_id: connection_id.into(),
        })?;
    if !definition.metadata.workflow {
        return Err(WorkflowError::CommandNotInWorkflow {
            command: step.command.clone(),
        });
    }
    validate_command_input(&definition, &step.input).map_err(WorkflowError::Validation)?;
    check_command_access(
        &definition,
        crate::commands::access_level_for_mode(permission_mode),
    )
    .map_err(WorkflowError::Validation)?;
    let sql = step
        .input
        .get("sql")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| WorkflowError::Validation("SQL input unavailable".into()))?;
    crate::sql_guard::check_sql(
        sql,
        config.read_only,
        before_dispatch(deadline, state.store.get_settings()).await?.safe_mode,
    )
    .map_err(WorkflowError::Validation)?;
    if let Some(mode) = permission_mode {
        crate::mcp::permission::check_sql_allowed(sql, mode).map_err(WorkflowError::Validation)?;
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
    let adapter = state
        .platform_require()
        .map_err(|error| WorkflowError::Validation(error.to_string()))?;
    let call = adapter.begin_call();
    let runtime = adapter
        .runtime()
        .map_err(|error| WorkflowError::Validation(error.to_string()))?;
    let token = before_dispatch(
        deadline,
        runtime.issue_submission_token(call.context(), IdempotentOperation::ExecuteAtTarget, None),
    ).await?
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
    let receipt = tokio::time::timeout_at(
        deadline,
        runtime.execute_at_target(
            call.context(),
            ExecuteAtTargetRequest::new(
                ExecutionTarget::new(ConnectionId::new(connection_id), namespace),
                Counter::new(metadata.config_revision),
                CommandCall::new(&step.command, step.input.clone()),
                token.idempotency_key,
            ),
        ),
    ).await
        .map_err(|_| submission_unconfirmed("SubmissionReceiptTimeout".into()))?
        .map_err(|error| submission_unconfirmed(format!("Submission:{:?}", error.code)))?;
    let mut cancellation = CancelOnDrop {
        runtime: runtime.clone(),
        context: call.context().clone(),
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
                runtime.get_execution(call.context(), receipt.execution_id.clone()),
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
            Ok(Err(error)) => return Err(unconfirmed(
                &last_state, &last_effects, format!("{:?}", error.code),
            )),
            Err(_) if timed_out => return Err(unconfirmed(
                &last_state, &last_effects, "TimeoutAwaitingTerminal".into(),
            )),
            Err(_) => {
                timed_out = true;
                // A cancellation receipt only acknowledges the request. Keep
                // polling until the execution itself confirms a terminal state.
                let cancelled = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    runtime.cancel_execution(call.context(), receipt.execution_id.clone()),
                ).await;
                match cancelled {
                    Ok(Ok(_)) => {},
                    Ok(Err(error)) => return Err(unconfirmed(
                        &last_state, &last_effects, format!("Cancel:{:?}", error.code),
                    )),
                    Err(_) => return Err(unconfirmed(
                        &last_state, &last_effects, "CancelAcknowledgementTimeout".into(),
                    )),
                }
                poll_deadline = tokio::time::Instant::now()
                    + std::time::Duration::from_secs(30);
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
            code: if timed_out { format!("StepTimeout:{:?}", view.error_code) } else { format!("{:?}", view.error_code) },
        });
    }
    // Reading a result does not execute SQL again. Preserve the known effect
    // when storage or transport fails after the statement has completed.
    let result_read_error =
        |error: datazen_application::error::ApiError| WorkflowError::SqlResultRead {
            execution_id: receipt.execution_id.as_str().to_owned(),
            effects: format!("{:?}", view.effect_outcome),
            code: format!("{:?}", error.code),
        };
    // Terminal is known, so result transport timeout can safely use Skip/Fallback.
    // One deadline covers all chunks rather than extending on every read.
    let read_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let read_timeout = || WorkflowError::SqlResultRead {
        execution_id: receipt.execution_id.as_str().to_owned(),
        effects: format!("{:?}", view.effect_outcome),
        code: "ArtifactReadTimeout".into(),
    };
    let statements = tokio::time::timeout_at(read_deadline, async {
        let mut statements = BTreeMap::<u64, serde_json::Value>::new();
        for artifact in view.artifact_ids {
            let first = runtime
                .read_artifact_chunk(call.context(), artifact.clone(), Counter::ZERO)
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
                    runtime
                        .read_artifact_chunk(call.context(), artifact.clone(), Counter::new(index))
                        .await
                        .map_err(&result_read_error)?
                };
                let Some(output) = chunk.output else { continue };
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
                if output["kind"] == "rows" {
                    statement["sql"] = output["sql"].clone();
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
        Ok::<_, WorkflowError>(statements)
    }).await
        .map_err(|_| read_timeout())??;
    Ok(CommandResult::new(serde_json::json!({
        "results": statements.into_values().collect::<Vec<_>>(),
        "totalTimeMs": started.elapsed().as_millis() as u64,
    })))
}
