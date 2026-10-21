//! Table browsing uses independent target executions; it never borrows a tree/editor handle.
use super::{AppState, CommandError, RowChangeTableContext};
use crate::{
    db::{CountStrategy, TableDataResult, TableSchema, Value},
    platform::into_command_error,
    services::{table_browsing, FilterCondition, OrderBy, QueryExecutor, SortCondition},
};
use datazen_application::{
    dto::requests::{CommandCall, ExecuteAtTargetRequest},
    sessions::ConnectionUseCases,
};
use datazen_platform_api::{
    context::RequestContext,
    dto::{execution::ExecutionState, idempotency::IdempotentOperation},
    id::{ConnectionId, Counter},
    target::{ExecutionTarget, NamespaceTarget, ObjectTarget},
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
    /// The row-count strategy the caller asks for. `None` leaves the choice
    /// to the driver.
    pub count_strategy: Option<CountStrategy>,
    /// The columns to order by when the caller did not ask for an order. An
    /// empty list means "do not order at all".
    pub default_order_columns: Option<Vec<String>>,
}

/// What a read does about its row count.
///
/// "Reuse the total you already have" and "this database cannot count" are
/// two different facts and they answer differently: the first means the
/// response carries no count at all, the second means it carries a count
/// nobody can provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountPlan {
    Reuse,
    Unsupported,
    Exact,
    Estimated,
}

async fn execute(
    adapter: &crate::platform::PlatformAdapter,
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
        let canonical = adapter
            .resolve_execution_target(context, revision, command, target)
            .await
            .map_err(into_command_error)?;
        let target = crate::platform::into_execution_target(canonical);
        let token = runtime
            .issue_submission_token(context, IdempotentOperation::ExecuteAtTarget, None)
            .await
            .map_err(into_command_error)?;
        runtime
            .execute_at_target(
                context,
                ExecuteAtTargetRequest {
                    target,
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
    // The filter list is checked before anything is built or fetched: a filter
    // this database cannot serve, or one that is missing a value, has to fail
    // before the statement is assembled and before the connection is used.
    // The shared layer owns the judgement; the host only maps the refusal.
    datazen_feature_core::services::validate_filters(
        driver.supported_filter_operators(),
        request.filters.as_deref().unwrap_or_default(),
    )?;
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
    )
    .with_object(ObjectTarget::new("table", request.context.table.clone()));
    let revision = Counter::new(metadata.config_revision);
    let database = request
        .context
        .database
        .as_deref()
        .ok_or_else(|| CommandError::Validation("Table database required".into()))?;
    let outputs = execute(
        adapter,
        &runtime,
        &context,
        &target,
        revision,
        request.manual_handle.as_ref(),
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
    // SQL construction depends on the dialect contract only; execution below
    // still goes through the driver's explicit target path.
    let dialect = crate::db::LegacySqlDialect::new(driver.as_ref());
    // The primary key has one source: the schema's own normalised list. The
    // column flags are a second, possibly stale, copy of the same fact, and
    // a driver that fills one and not the other used to lose its key here
    // and order by the first column instead.
    let primary_keys = schema.effective_primary_keys();
    table_browsing::report_pk_source_disagreement(
        &schema.table_name,
        &schema.columns,
        &primary_keys,
    );
    let default_order = table_browsing::resolve_default_order(
        driver.as_ref(),
        &schema.columns,
        &primary_keys,
        request.default_order_columns.as_deref(),
    );
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
        &default_order,
        &dialect,
        u64::from(request.page_size),
        u64::from(request.page).saturating_mul(u64::from(request.page_size)),
        request.filter_logic.as_deref(),
    )?;
    let caller_skip_count = request.skip_count.unwrap_or(false);
    let count_plan = if caller_skip_count {
        CountPlan::Reuse
    } else {
        match table_browsing::resolve_count_strategy(
            driver.as_ref(),
            request.count_strategy,
            false,
            table_browsing::has_effective_filters(request.filters.as_deref()),
            database,
            request.context.schema.as_deref(),
            &request.context.table,
        ) {
            CountStrategy::Unsupported => CountPlan::Unsupported,
            CountStrategy::Exact => CountPlan::Exact,
            CountStrategy::Estimated => CountPlan::Estimated,
        }
    };
    // The count travels in the same statement batch as the rows, so a read
    // still costs one round trip.
    let sql = match count_plan {
        CountPlan::Reuse | CountPlan::Unsupported => select.clone(),
        CountPlan::Exact => {
            let count = QueryExecutor::build_count_sql(
                &schema.table_name,
                &schema.columns,
                &request.filters,
                &dialect,
                request.filter_logic.as_deref(),
            )?;
            format!("{count};\n{select}")
        }
        CountPlan::Estimated => {
            let estimate = driver
                .count_estimate_sql(
                    database,
                    request.context.schema.as_deref(),
                    &request.context.table,
                )
                .ok_or_else(|| {
                    CommandError::Internal(
                        "the driver declared an estimated row count but produced no statement"
                            .into(),
                    )
                })?;
            format!("{estimate};\n{select}")
        }
    };
    let mut estimate_failed = false;
    let outputs = match execute(
        adapter,
        &runtime,
        &context,
        &target,
        revision,
        request.manual_handle.as_ref(),
        "query",
        json!({"sql": sql}),
    )
    .await
    {
        Ok(outputs) => outputs,
        // The estimate shares the batch with the rows, so a failed estimate
        // takes the batch down with it. The rows are then asked for on their
        // own: a missing row count must not cost the user the rows.
        Err(error) if count_plan == CountPlan::Estimated => {
            tracing::warn!(
                %database,
                table = %request.context.table,
                %error,
                "table_target: the row count estimate failed; reading the rows on their own"
            );
            estimate_failed = true;
            execute(
                adapter,
                &runtime,
                &context,
                &target,
                revision,
                request.manual_handle.as_ref(),
                "query",
                json!({"sql": select}),
            )
            .await?
        }
        Err(error) => return Err(error),
    };
    // A batch that carries a count puts it first and the rows second; a
    // batch that carries only the rows has the rows at index 0.
    let count_at = match count_plan {
        CountPlan::Exact | CountPlan::Estimated if !estimate_failed => Some(0u64),
        _ => None,
    };
    let rows_at = u64::from(count_at.is_some());
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
        if count_at == Some(index) {
            // Drivers publish a final empty result chunk after each statement.
            // Do not let that metadata-only chunk erase the count row.
            let cell = result_rows
                .first()
                .and_then(|row| row.first())
                .and_then(Option::as_ref);
            // The shared layer owns both readings, so the two host entry
            // points cannot drift into two different shapes of the same DTO.
            let count = match count_plan {
                // An estimate has its own gate: a negative number is not an
                // estimate of anything — it is what PostgreSQL says about a
                // relation the planner never looked at. It must not travel
                // as a row count.
                CountPlan::Estimated => cell.and_then(table_browsing::estimated_count_cell),
                // The exact count keeps the reading this path has always had.
                _ => cell.and_then(table_browsing::target_count_cell),
            };
            if let Some(count) = count {
                total_rows = Some(count);
            }
        } else if index == rows_at {
            rows.extend(result_rows);
        }
    }
    let total_rows_kind = match count_plan {
        // The caller reuses the total it already has: this response says
        // nothing about the count.
        CountPlan::Reuse => None,
        CountPlan::Unsupported => Some(CountStrategy::Unsupported),
        // The count was taken; if it did not come back as a number, the answer
        // is "the rows are there, the number is not".
        CountPlan::Exact => Some(CountStrategy::Exact),
        CountPlan::Estimated => match (estimate_failed, total_rows) {
            (false, Some(_)) => Some(CountStrategy::Estimated),
            _ => Some(CountStrategy::Unsupported),
        },
    };
    Ok(TableDataResult {
        columns: schema.columns,
        rows,
        total_rows,
        total_rows_kind,
        page: request.page,
        page_size: request.page_size,
    })
}
