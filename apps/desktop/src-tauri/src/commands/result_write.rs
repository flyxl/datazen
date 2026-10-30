//! Result edits use immutable provenance and the original live runtime resource.
use super::{AppState, CommandError};
use crate::platform::into_command_error;
use datazen::application::sessions::ConnectionUseCases;
use datazen::execution::application::requests::{CommandCall, ExecuteInSessionRequest};
use datazen::execution::application::{RuntimeArtifactChunk, RuntimeConnectionUseCases};
use datazen::services::result_edit::{read_only_error_code, GridReadOnlyReason, WriteTarget};
use datazen::{
    execution::application::idempotency::IdempotentOperation,
    execution::{ExecutionState, RuntimeResultBinding, WritableMapping},
    shared::context::RequestContext,
    shared::id::Counter,
};
use datazen_driver_api::{ColumnInfo, TableSchema, Value};
use serde::Deserialize;
use serde_json::json;
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultCellWrite {
    pub binding: RuntimeResultBinding,
    pub statement_index: u32,
    pub row_index: usize,
    pub column: String,
    pub value: Value,
}

async fn chunks(
    runtime: &RuntimeConnectionUseCases,
    context: &RequestContext,
    execution: datazen::shared::id::ExecutionId,
) -> Result<Vec<RuntimeArtifactChunk>, CommandError> {
    let view = runtime
        .get_execution(context, execution)
        .await
        .map_err(into_command_error)?;
    let mut chunks = Vec::new();
    for artifact in view.artifact_ids {
        let first = runtime
            .read_artifact_chunk(context, artifact.clone(), Counter::ZERO)
            .await
            .map_err(into_command_error)?;
        for index in 0..first.published_chunk_count.get() {
            chunks.push(if index == 0 {
                first.clone()
            } else {
                runtime
                    .read_artifact_chunk(context, artifact.clone(), Counter::new(index))
                    .await
                    .map_err(into_command_error)?
            });
        }
    }
    Ok(chunks)
}

async fn execute(
    runtime: &RuntimeConnectionUseCases,
    context: &RequestContext,
    binding: &RuntimeResultBinding,
    expected_context: &datazen::execution::SessionContext,
    command: &str,
    input: serde_json::Value,
) -> Result<Vec<RuntimeArtifactChunk>, CommandError> {
    let original = runtime
        .get_execution(context, binding.execution_id.clone())
        .await
        .map_err(into_command_error)?;
    if original.runtime_binding.as_ref() != Some(binding) {
        return Err(CommandError::Validation(
            "Result resource is no longer writable".into(),
        ));
    }
    let session = runtime
        .get_session(context, binding.handle.clone())
        .await
        .map_err(into_command_error)?;
    // The same target check the command makes, from the same one representation:
    // a write must not be aimed at a session that now points somewhere else.
    if WriteTarget::of(&session.observed_context) != WriteTarget::of(expected_context) {
        return Err(CommandError::Validation(
            "Result session context changed; execute the query again before editing".into(),
        ));
    }
    let token = runtime
        .issue_submission_token(
            context,
            IdempotentOperation::ExecuteInSession,
            Some(&binding.handle),
        )
        .await
        .map_err(into_command_error)?;
    let receipt = runtime
        .execute_in_session(
            context,
            ExecuteInSessionRequest {
                handle: binding.handle.clone(),
                expected_context_revision: Some(session.context_revision),
                call: CommandCall {
                    command: command.into(),
                    input,
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .map_err(into_command_error)?;
    loop {
        let view = runtime
            .get_execution(context, receipt.execution_id.clone())
            .await
            .map_err(into_command_error)?;
        if view.state.is_terminal() {
            if view.state != ExecutionState::Succeeded {
                return Err(CommandError::Validation(format!("Result edit failed: {:?}; effect: {:?}. Inspect the current transaction before retrying.", view.error_code, view.effect_outcome)));
            }
            return chunks(runtime, context, receipt.execution_id).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

pub async fn commit_query_result_cell(
    state: State<'_, AppState>,
    request: ResultCellWrite,
) -> Result<(), CommandError> {
    commit_query_result_cell_impl(&state, request).await
}

pub(crate) async fn commit_query_result_cell_impl(
    state: &AppState,
    request: ResultCellWrite,
) -> Result<(), CommandError> {
    let adapter = state.platform_require().map_err(into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let runtime = adapter.runtime().map_err(into_command_error)?;
    let view = runtime
        .get_execution(&context, request.binding.execution_id.clone())
        .await
        .map_err(into_command_error)?;
    if view.state != ExecutionState::Succeeded
        || view.runtime_binding.as_ref() != Some(&request.binding)
    {
        return Err(CommandError::Validation(
            "Result requires its original live session".into(),
        ));
    }
    // The connection's own state, read once and up front — before a single
    // statement of this write is built:
    //
    // * the session names the connection the write would go to;
    // * a **read-only connection refuses every write**, and it says so in the
    //   grid's own reason code. The narrow mapping verdict says nothing about
    //   that flag, and the origin verdict is not the only channel that can
    //   open a cell, so the host has to ask for it itself: the refusal
    //   happens before anything of the write leaves the process;
    // * the connection's configuration must still be the one this session was
    //   opened with.
    let session = runtime
        .get_session(&context, request.binding.handle.clone())
        .await
        .map_err(into_command_error)?;
    let (config, profile) = state
        .store
        .platform_profile(session.connection_id.as_str())
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if config.read_only {
        // **Which code a refusal carries is a rule, and the rule lives in the
        // shared layer.** The shell asks `read_only_error_code` for it rather
        // than writing the code out: `SessionReadOnly` is already one of the
        // editability reasons, so a read-only connection is refused with the
        // same code the origin verdict would have used — the two ends of the
        // same decision agree, and the vocabulary stays in one place.
        let code = read_only_error_code(GridReadOnlyReason::SessionReadOnly);
        return Err(CommandError::Validation(format!(
            "{code}: Connection is read-only; result cells cannot be written back"
        )));
    }
    if !profile.enabled || Counter::new(profile.config_revision) != session.config_revision {
        return Err(CommandError::Validation(
            "Result connection configuration changed".into(),
        ));
    }
    let stored = chunks(&runtime, &context, request.binding.execution_id.clone()).await?;
    let metadata = stored
        .iter()
        .find(|chunk| {
            chunk.source.statement_index == request.statement_index
                && chunk.source.writable_mapping == WritableMapping::Verified
                && chunk
                    .output
                    .as_ref()
                    .and_then(|output| output.get("kind"))
                    .and_then(|kind| kind.as_str())
                    == Some("statementMetadata")
        })
        .ok_or_else(|| CommandError::Validation("Result has no verified writable source".into()))?;
    let source = &metadata.source;
    let relation = source
        .relation
        .as_ref()
        .ok_or_else(|| CommandError::Validation("Result relation unavailable".into()))?;
    // Never silently redirect a retained result to another target. What decides
    // that is **where the write goes and as whom** — the namespace and the
    // effective identity — never the session's *mode*: a statement may open a
    // transaction, change autocommit or move the search path while it runs, and
    // the observed context is captured *after* the statement, so comparing raw
    // contexts would make a verified result permanently unwritable for those
    // drivers. Both sides go through `WriteTarget`, one representation, one
    // place that decides "the same target" (`result_edit::WriteTarget`).
    if session.connection_id != relation.connection_id
        || WriteTarget::of(&session.observed_context) != WriteTarget::of(&source.context)
    {
        return Err(CommandError::Validation(
            "Result session context changed; execute the query again before editing".into(),
        ));
    }
    let columns: Vec<ColumnInfo> = serde_json::from_value(
        metadata
            .output
            .as_ref()
            .and_then(|output| output.get("columns"))
            .cloned()
            .ok_or_else(|| CommandError::Validation("Result columns unavailable".into()))?,
    )
    .map_err(|_| CommandError::Validation("Invalid result columns".into()))?;
    let mut rows = Vec::new();
    for chunk in &stored {
        if chunk.source.statement_index != request.statement_index {
            continue;
        }
        if let Some(output) = &chunk.output {
            if output.get("kind").and_then(|kind| kind.as_str()) == Some("rows") {
                if let Some(values) = output.get("rows").and_then(|rows| rows.as_array()) {
                    rows.extend(values.iter());
                }
            }
        }
    }
    let row: Vec<Option<Value>> = serde_json::from_value(
        (*rows
            .get(request.row_index)
            .ok_or_else(|| CommandError::Validation("Result row unavailable".into()))?)
        .clone(),
    )
    .map_err(|_| CommandError::Validation("Invalid result row".into()))?;
    let table = relation
        .object
        .as_ref()
        .ok_or_else(|| CommandError::Validation("Result relation unavailable".into()))?
        .name
        .clone();
    let schema_chunks = execute(&runtime, &context, &request.binding, &source.context, "get_table_schema", json!({"table":table,"database":relation.namespace.database,"schema":relation.namespace.schema})).await?;
    let data = schema_chunks
        .iter()
        .filter_map(|chunk| chunk.output.as_ref())
        .find(|output| output.get("kind").and_then(|kind| kind.as_str()) == Some("commandResult"))
        .and_then(|output| output.get("data"))
        .ok_or_else(|| CommandError::Validation("Result schema unavailable".into()))?;
    let schema: TableSchema = serde_json::from_value(
        data.get("schema")
            .cloned()
            .ok_or_else(|| CommandError::Validation("Result schema unavailable".into()))?,
    )
    .map_err(|_| CommandError::Validation("Invalid result schema".into()))?;
    let resolved = data
        .get("resolvedTarget")
        .ok_or_else(|| CommandError::Validation("Driver cannot confirm result identity".into()))?;
    if super::verify_result_mapping(relation.connection_id.clone(), &columns, &schema, resolved)
        .as_ref()
        != Some(relation)
    {
        return Err(CommandError::Validation(
            "Result relation changed; execute the query again".into(),
        ));
    }
    let keys = schema.effective_primary_keys();
    if keys.contains(&request.column) {
        return Err(CommandError::Validation(
            "Primary key edits require the table ChangeSet editor".into(),
        ));
    }
    if !columns.iter().any(|column| column.name == request.column) {
        return Err(CommandError::Validation("Result column unavailable".into()));
    }
    let pk = keys
        .iter()
        .map(|key| {
            let index = columns
                .iter()
                .position(|column| &column.name == key)
                .ok_or_else(|| CommandError::Validation("Result primary key unavailable".into()))?;
            let value = row.get(index).cloned().flatten().ok_or_else(|| {
                CommandError::Validation("Null result primary key cannot be edited".into())
            })?;
            Ok((key.as_str(), Some(value)))
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("SQL driver unavailable".into()))?;
    let mut statement = driver
        .build_update_statement(
            &table,
            &[(request.column.as_str(), Some(request.value.clone()))],
            &pk,
        )
        .map_err(CommandError::Driver)?;
    statement.sql = driver
        .qualify_sql_target(
            &statement.sql,
            relation.namespace.database.as_deref(),
            relation.namespace.schema.as_deref(),
        )
        .ok_or_else(|| CommandError::Validation("Driver cannot qualify result writes".into()))?;
    let output = execute(
        &runtime,
        &context,
        &request.binding,
        &source.context,
        "query",
        json!({"sql":statement.sql,"boundParameters":statement.parameters}),
    )
    .await?;
    let affected = output
        .iter()
        .filter_map(|chunk| chunk.output.as_ref())
        .filter_map(|output| output.get("rowsAffected").and_then(|value| value.as_u64()))
        .next();
    if affected == Some(0) {
        // MySQL reports changed rows: a concurrent writer may already have set
        // the requested value. Confirm it on this resource before calling it unchanged.
        // 装配（引用/占位符/谓词）在 modules `result_edit::confirm_unchanged_read_sql`
        // 定稿（host-boundary 残余收口）；限定与执行留宿主运行时侧。
        let (sql, parameters) = datazen::table_editing::result_edit::confirm_unchanged_read_sql(
            driver.as_ref(),
            &table,
            &request.column,
            &pk,
        )?;
        let sql = driver
            .qualify_sql_target(
                &sql,
                relation.namespace.database.as_deref(),
                relation.namespace.schema.as_deref(),
            )
            .ok_or_else(|| CommandError::Validation("Driver cannot qualify result reads".into()))?;
        let check = execute(
            &runtime,
            &context,
            &request.binding,
            &source.context,
            "query",
            json!({"sql":sql,"boundParameters":parameters}),
        )
        .await?;
        let expected = serde_json::to_value(&request.value)
            .map_err(|_| CommandError::Validation("Invalid edit value".into()))?;
        let values: Vec<&serde_json::Value> = check
            .iter()
            .filter_map(|chunk| chunk.output.as_ref())
            .filter(|output| output.get("kind").and_then(|kind| kind.as_str()) == Some("rows"))
            .filter_map(|output| output.get("rows").and_then(|rows| rows.as_array()))
            .flatten()
            .collect();
        if values.len() == 1 && values[0].as_array().and_then(|row| row.first()) == Some(&expected)
        {
            return Ok(());
        }
        return Err(CommandError::Validation(
            "Result row changed or was deleted; inspect the target before retrying".into(),
        ));
    }
    if affected != Some(1) {
        return Err(CommandError::Validation(format!("Result edit reported {:?} affected rows. Inspect the target and current transaction before retrying.", affected)));
    }
    Ok(())
}
