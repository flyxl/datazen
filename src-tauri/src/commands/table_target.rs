//! Table browsing uses independent target executions; it never borrows a tree/editor handle.
use super::{AppState, CommandError, RowChangeTableContext};
use crate::{
    db::{TableDataResult, TableSchema, Value},
    platform::into_command_error,
    services::{FilterCondition, OrderBy, QueryExecutor, SortCondition},
};
use datazen_application::{
    dto::requests::{CommandCall, ExecuteAtTargetRequest},
    sessions::ConnectionUseCases,
};
use datazen_platform_api::{
    context::RequestContext,
    dto::{execution::ExecutionState, idempotency::IdempotentOperation},
    id::{ConnectionId, Counter},
    target::{ExecutionTarget, NamespaceTarget},
};
use datazen_runtime::application::RuntimeConnectionUseCases;
use serde::Deserialize;
use serde_json::{json, Value as Json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableReadRequest {
    pub manual_handle: Option<datazen_platform_api::dto::session::SessionHandle>,
    pub context: RowChangeTableContext,
    pub page: u32,
    pub page_size: u32,
    pub filters: Option<Vec<FilterCondition>>,
    pub sorts: Option<Vec<SortCondition>>,
    pub skip_count: Option<bool>,
    pub filter_logic: Option<String>,
}

async fn execute(
    runtime: &RuntimeConnectionUseCases,
    context: &RequestContext,
    target: &ExecutionTarget,
    revision: Counter,
    manual_handle: Option<&datazen_platform_api::dto::session::SessionHandle>,
    command: &str,
    input: Json,
) -> Result<Vec<Json>, CommandError> {
    let receipt = if let Some(handle) = manual_handle {
        let session = runtime
            .get_session(context, handle.clone())
            .await
            .map_err(into_command_error)?;
        if session.connection_id != target.connection_id
            || session.config_revision != revision
            || session.observed_context.namespace != target.namespace
        {
            return Err(CommandError::Validation(
                "Manual table session target changed".into(),
            ));
        }
        let token = runtime
            .issue_submission_token(context, IdempotentOperation::ExecuteInSession, Some(handle))
            .await
            .map_err(into_command_error)?;
        runtime
            .execute_in_session(
                context,
                datazen_application::dto::requests::ExecuteInSessionRequest {
                    handle: handle.clone(),
                    expected_context_revision: Some(session.context_revision),
                    call: CommandCall {
                        command: command.into(),
                        input,
                    },
                    idempotency_key: token.idempotency_key,
                },
            )
            .await
            .map_err(into_command_error)?
    } else {
        let token = runtime
            .issue_submission_token(context, IdempotentOperation::ExecuteAtTarget, None)
            .await
            .map_err(into_command_error)?;
        runtime
            .execute_at_target(
                context,
                ExecuteAtTargetRequest {
                    target: target.clone(),
                    expected_config_revision: revision,
                    call: CommandCall {
                        command: command.into(),
                        input,
                    },
                    idempotency_key: token.idempotency_key,
                },
            )
            .await
            .map_err(into_command_error)?
    };
    let view = loop {
        let view = runtime
            .get_execution(context, receipt.execution_id.clone())
            .await
            .map_err(into_command_error)?;
        if view.state.is_terminal() {
            break view;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    if view.state != ExecutionState::Succeeded {
        return Err(CommandError::Validation(format!(
            "Table execution failed: {:?}; effect outcome: {:?}",
            view.error_code, view.effect_outcome
        )));
    }
    let mut outputs = Vec::new();
    for artifact in view.artifact_ids {
        let first = runtime
            .read_artifact_chunk(context, artifact.clone(), Counter::ZERO)
            .await
            .map_err(into_command_error)?;
        for index in 0..first.published_chunk_count.get() {
            let chunk = if index == 0 {
                first.clone()
            } else {
                runtime
                    .read_artifact_chunk(context, artifact.clone(), Counter::new(index))
                    .await
                    .map_err(into_command_error)?
            };
            if let Some(output) = chunk.output {
                outputs.push(output);
            }
        }
    }
    Ok(outputs)
}

pub async fn read(
    state: &AppState,
    request: TableReadRequest,
) -> Result<TableDataResult, CommandError> {
    super::validate_table_context(&request.context)?;
    if request.page_size == 0 || request.page_size > 10000 {
        return Err(CommandError::Validation("Invalid table page size".into()));
    }
    let (config, metadata) = state
        .store
        .platform_profile(&request.context.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != request.context.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("SQL driver unavailable".into()))?;
    let adapter = state.platform_require().map_err(into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let runtime = adapter.runtime().map_err(into_command_error)?;
    let target = ExecutionTarget::new(
        ConnectionId::new(&request.context.connection_id),
        NamespaceTarget::new(
            request.context.database.clone(),
            None,
            request.context.schema.clone(),
            Vec::new(),
        ),
    );
    let revision = Counter::new(metadata.config_revision);
    let database = request
        .context
        .database
        .as_deref()
        .ok_or_else(|| CommandError::Validation("Table database required".into()))?;
    let outputs = execute(
        &runtime,
        &context,
        &target,
        revision,
        None,
        "get_table_schema",
        json!({
            "table": request.context.table, "database": database, "schema": request.context.schema,
        }),
    )
    .await?;
    let definition = outputs
        .iter()
        .find(|output| output["kind"] == "commandResult")
        .and_then(|output| output.get("data"))
        .and_then(|data| data.get("schema"))
        .ok_or_else(|| CommandError::Validation("Typed table schema result unavailable".into()))?;
    let schema: TableSchema = serde_json::from_value(definition.clone())
        .map_err(|_| CommandError::Validation("Invalid table schema result".into()))?;
    let quote = |name: &str| driver.quote_ident(name);
    let literal = |value: &Value| driver.try_format_sql_literal(&Some(value.clone()));
    let select = QueryExecutor::build_select_sql(
        &schema.table_name,
        &schema.columns,
        request.filters.clone(),
        request
            .sorts
            .and_then(|sorts| sorts.into_iter().next())
            .map(|sort| OrderBy {
                column: sort.column,
                descending: sort.descending,
            }),
        &quote,
        &literal,
        &driver.pagination_syntax(
            u64::from(request.page_size),
            u64::from(request.page).saturating_mul(u64::from(request.page_size)),
        ),
        request.filter_logic.as_deref(),
    )?;
    let skip_count = request.skip_count.unwrap_or(false) || driver.skip_count_query();
    let sql = if skip_count {
        select
    } else {
        let count = QueryExecutor::build_count_sql(
            &schema.table_name,
            &schema.columns,
            &request.filters,
            &quote,
            &literal,
            request.filter_logic.as_deref(),
        )?;
        format!("{count};\n{select}")
    };
    let outputs = execute(
        &runtime,
        &context,
        &target,
        revision,
        request.manual_handle.as_ref(),
        "query",
        json!({"sql": sql}),
    )
    .await?;
    let mut rows = Vec::new();
    let mut total_rows = None;
    for output in outputs {
        if output["kind"] != "rows" {
            continue;
        }
        let index = output["statementIndex"].as_u64().ok_or_else(|| {
            CommandError::Validation("Result statement identity unavailable".into())
        })?;
        let result_rows: Vec<Vec<Option<Value>>> =
            serde_json::from_value(output["rows"].clone())
                .map_err(|_| CommandError::Validation("Invalid table result rows".into()))?;
        if !skip_count && index == 0 {
            total_rows = result_rows
                .first()
                .and_then(|row| row.first())
                .and_then(|value| value.as_ref())
                .and_then(|value| {
                    if let Value::Integer(count) = value {
                        Some(*count)
                    } else {
                        None
                    }
                });
        } else if index == u64::from(!skip_count) {
            rows.extend(result_rows);
        }
    }
    Ok(TableDataResult {
        columns: schema.columns,
        rows,
        total_rows,
        page: request.page,
        page_size: request.page_size,
    })
}
