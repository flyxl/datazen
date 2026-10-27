//! Table browsing uses independent target executions; it never borrows a tree/editor handle.
use super::{AppState, CommandError, RowChangeTableContext};
use crate::platform::into_command_error;
use crate::services::{table_browsing, FilterCondition, QueryExecutor, SortCondition};
use datazen_driver_api::keyset::CursorValue;
use datazen_driver_api::{
    CountStrategy, DatabaseDriver, SqlDialect, TableDataResult, TableSchema, Value,
};
use datazen_feature_core::services::query_executor::table_read_sql::build_unbounded_select_sql;
use datazen_feature_core::services::query_executor::{
    keyset_refusal, paging_info, resolve_paging, PagingPlan,
};
use datazen_feature_core::services::table_paging::{keyset_decision, resolve_order_spec_multi};
use datazen_modules::application::sessions::ConnectionUseCases;
use datazen_modules::execution::application::requests::{CommandCall, ExecuteAtTargetRequest};
use datazen_modules::execution::application::RuntimeConnectionUseCases;
use datazen_modules::{
    execution::application::idempotency::IdempotentOperation,
    execution::target::{ExecutionTarget, NamespaceTarget, ObjectTarget},
    execution::ExecutionState,
    shared::context::RequestContext,
    shared::id::{ConnectionId, Counter},
};
use serde::Deserialize;
use serde_json::{json, Value as Json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableReadRequest {
    pub manual_handle: Option<datazen_modules::execution::SessionHandle>,
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
    /// The boundary of the page: the key values of the last row of the
    /// previous page. `None` — the default — is the first page of a read.
    ///
    /// The field is optional so that a caller that does not know about it is
    /// not broken by it: an older front end sends no boundary and gets the
    /// offset read it always got.
    pub after_key: Option<Vec<CursorValue>>,
    /// "This read needs keyset paging; do not fall back to an offset."
    ///
    /// The default is `false`, and `false` means the read **degrades**: when
    /// the page cannot be read by seek, the host reads it by offset and says
    /// so in the answer. A caller that cannot accept an offset read asks for
    /// it explicitly and gets an error instead of a different page.
    pub require_keyset: Option<bool>,
    /// How many boundaries the caller holds in its chain.
    ///
    /// The host refuses a chain that is deeper than it accepts: a boundary
    /// that far from the first page describes a moment in the past that the
    /// data underneath has almost certainly left.
    pub cursor_depth: Option<u32>,
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

/// The query-shape half of a read request (DB-15).
///
/// Everything the statement assembly needs, independent of where the request
/// came from: a real page read and a preview differ only in whether the
/// assembled statements are executed, never in how they are built.
struct ReadQueryShape<'a> {
    page: u32,
    page_size: u32,
    filters: Option<&'a [FilterCondition]>,
    sorts: Option<&'a [SortCondition]>,
    filter_logic: Option<&'a str>,
    count_strategy: Option<CountStrategy>,
    default_order_columns: Option<&'a [String]>,
    after_key: Option<&'a [CursorValue]>,
    require_keyset: bool,
    cursor_depth: u32,
    /// The caller already holds a total and wants no count statement.
    skip_count: bool,
}

/// The statements a read (or a preview) would run, before anything runs.
struct AssembledStatements {
    /// The statement that reads the rows of the page.
    select: String,
    /// The statement that counts the rows, when one runs with the read —
    /// the exact `COUNT(*)` for an exact plan, the driver's estimate for an
    /// estimated plan, `None` when no count runs at all.
    count_statement: Option<String>,
    /// The exact `COUNT(*)` of the filtered table, whatever the plan is.
    ///
    /// A read that asked for an estimate still falls back to this statement
    /// when the estimate comes back unusable, and a preview shows it — the
    /// fallback and the plan must ask the same question.
    exact_count_statement: String,
    /// How the count is obtained.
    count_plan: CountPlan,
    /// The paging plan the select was built with.
    plan: PagingPlan,
}

/// Assemble the statements of a table read without executing anything.
///
/// This is the single place the read's SQL exists (DB-15): the preview shows
/// what this builds, the filtered export streams what this builds, and a
/// page read executes what this builds — one builder, no drift. The byte
/// identity of the offset statement is the contract the tests pin.
fn assemble_statements(
    driver: &dyn DatabaseDriver,
    dialect: &dyn SqlDialect,
    schema: &TableSchema,
    database: &str,
    schema_name: Option<&str>,
    table: &str,
    shape: ReadQueryShape<'_>,
) -> Result<AssembledStatements, CommandError> {
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
        driver,
        &schema.columns,
        &primary_keys,
        shape.default_order_columns,
    );
    // The order of this read, and whether it is a total order. The decision
    // is made once, here: the statement, the boundary and the answer to the
    // caller all read it from the same place.
    //
    // The caller's explicit sort list is ordered (DB-11): its positions are
    // the ORDER BY positions, and a one-entry list resolves to the exact
    // single-column order this path always built. This is the IPC edge that
    // used to truncate `sorts` to its first entry — every column after the
    // first silently disappeared here, which is why the grid could not sort
    // by more than one column even with a plural contract on the wire.
    let explicit_sorts: Vec<(&str, bool)> = shape
        .sorts
        .unwrap_or_default()
        .iter()
        .map(|sort| (sort.column.as_str(), sort.descending))
        .collect();
    let order = resolve_order_spec_multi(&explicit_sorts, &default_order, &primary_keys);
    let decision = keyset_decision(order, &schema.columns);
    let plan = resolve_paging(
        driver,
        &decision,
        shape.after_key,
        shape.page,
        dialect,
        u64::from(shape.page_size),
    );

    // A caller that needs keyset paging and cannot have it is told so; a
    // caller that does not care reads the page by offset and is told that
    // too, in the answer.
    if let Some(message) = keyset_refusal(&plan, shape.require_keyset, shape.cursor_depth) {
        return Err(CommandError::Validation(message));
    }

    let select =
        datazen_feature_core::services::query_executor::table_read_sql::build_paged_select_sql(
            &schema.table_name,
            &schema.columns,
            shape.filters.map(<[FilterCondition]>::to_vec),
            &plan.order,
            plan.mode,
            plan.predicate.as_deref(),
            dialect,
            u64::from(shape.page_size),
            u64::from(shape.page).saturating_mul(u64::from(shape.page_size)),
            shape.filter_logic,
        )?
        .sql;

    let count_plan = if shape.skip_count {
        CountPlan::Reuse
    } else {
        match table_browsing::resolve_count_strategy(
            driver,
            shape.count_strategy,
            false,
            table_browsing::has_effective_filters(shape.filters),
            database,
            schema_name,
            table,
        ) {
            CountStrategy::Unsupported => CountPlan::Unsupported,
            CountStrategy::Exact => CountPlan::Exact,
            CountStrategy::Estimated => CountPlan::Estimated,
        }
    };
    // The exact `COUNT(*)` is always built (DB-15): the preview shows it, and
    // a read whose estimate came back unusable falls back to it — the fallback
    // and the plan must ask the same question, so the statement exists in
    // exactly one place.
    let exact_count_statement = QueryExecutor::build_count_sql(
        &schema.table_name,
        &schema.columns,
        &shape.filters.map(<[FilterCondition]>::to_vec),
        dialect,
        shape.filter_logic,
    )?;
    let count_statement = match count_plan {
        CountPlan::Exact => Some(exact_count_statement.clone()),
        CountPlan::Estimated => Some(
            driver
                .count_estimate_sql(database, schema_name, table)
                .ok_or_else(|| {
                    CommandError::Internal(
                        "the driver declared an estimated row count but produced no statement"
                            .into(),
                    )
                })?,
        ),
        CountPlan::Reuse | CountPlan::Unsupported => None,
    };

    Ok(AssembledStatements {
        select,
        count_statement,
        exact_count_statement,
        count_plan,
        plan,
    })
}

async fn execute(
    adapter: &crate::platform::PlatformAdapter,
    runtime: &RuntimeConnectionUseCases,
    context: &RequestContext,
    target: &ExecutionTarget,
    revision: Counter,
    manual_handle: Option<&datazen_modules::execution::SessionHandle>,
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
                datazen_modules::execution::application::requests::ExecuteInSessionRequest {
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

/// Fetch the typed schema a read (or a preview) assembles against.
///
/// One round trip; the statement assembly is local. Shared by the page read
/// and the DB-15 preview so the two cannot build against different schemas.
#[allow(clippy::too_many_arguments)]
async fn fetch_table_schema(
    adapter: &crate::platform::PlatformAdapter,
    runtime: &RuntimeConnectionUseCases,
    call_context: &RequestContext,
    target: &ExecutionTarget,
    revision: Counter,
    manual_handle: Option<&datazen_modules::execution::SessionHandle>,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<TableSchema, CommandError> {
    let outputs = execute(
        adapter,
        runtime,
        call_context,
        target,
        revision,
        manual_handle,
        "get_table_schema",
        json!({"table": table, "database": database, "schema": schema}),
    )
    .await?;
    let definition = outputs
        .iter()
        .find(|output| output["kind"] == "commandResult")
        .and_then(|output| output.get("data"))
        .and_then(|data| data.get("schema"))
        .ok_or_else(|| CommandError::Validation("Typed table schema result unavailable".into()))?;
    serde_json::from_value(definition.clone())
        .map_err(|_| CommandError::Validation("Invalid table schema result".into()))
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
    let schema = fetch_table_schema(
        adapter,
        &runtime,
        &context,
        &target,
        revision,
        request.manual_handle.as_ref(),
        &request.context.table,
        database,
        request.context.schema.as_deref(),
    )
    .await?;
    // SQL construction depends on the dialect contract only; execution below
    // still goes through the driver's explicit target path.
    let dialect = datazen_driver_api::LegacySqlDialect::new(driver.as_ref());
    // One assembly for everything (DB-15): the page statement, the count
    // statement and the count plan are built in the one place a preview can
    // reach, so what runs is what a preview would have shown.
    let assembled = assemble_statements(
        driver.as_ref(),
        &dialect,
        &schema,
        database,
        request.context.schema.as_deref(),
        &request.context.table,
        ReadQueryShape {
            page: request.page,
            page_size: request.page_size,
            filters: request.filters.as_deref(),
            sorts: request.sorts.as_deref(),
            filter_logic: request.filter_logic.as_deref(),
            count_strategy: request.count_strategy,
            default_order_columns: request.default_order_columns.as_deref(),
            after_key: request.after_key.as_deref(),
            require_keyset: request.require_keyset.unwrap_or(false),
            cursor_depth: request.cursor_depth.unwrap_or(0),
            skip_count: request.skip_count.unwrap_or(false),
        },
    )?;
    let select = assembled.select;
    let mut count_plan = assembled.count_plan;
    // The count travels in the same statement batch as the rows, so a read
    // still costs one round trip.
    let sql = match count_plan {
        CountPlan::Reuse | CountPlan::Unsupported => select.clone(),
        // The statement is assembled already; the batch is the only way the
        // count and the rows share one round trip.
        CountPlan::Exact => {
            let count = assembled.count_statement.ok_or_else(|| {
                CommandError::Internal("Exact count statement unavailable".into())
            })?;
            format!("{count};\n{select}")
        }
        CountPlan::Estimated => {
            let estimate = assembled.count_statement.ok_or_else(|| {
                CommandError::Internal(
                    "the driver declared an estimated row count but produced no statement".into(),
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
    // An unusable estimate is the absence of a fact, not a fact.
    //
    // PostgreSQL answers `-1` for a relation the planner never looked at, and
    // that is exactly the state of a table created and filled moments ago —
    // the common case for "open the table I just made". Reporting "the count
    // is unavailable" there is worse than paying one `COUNT(*)`: the rows are
    // already in hand, the table is small by construction, and the same read
    // already counts exactly whenever the driver cannot estimate at all.
    if count_plan == CountPlan::Estimated && total_rows.is_none() {
        let counted = execute(
            adapter,
            &runtime,
            &context,
            &target,
            revision,
            request.manual_handle.as_ref(),
            "query",
            json!({"sql": assembled.exact_count_statement.clone()}),
        )
        .await;
        if let Ok(outputs) = counted {
            for output in outputs {
                if output["kind"] != "rows" {
                    continue;
                }
                let result_rows: Vec<Vec<Option<Value>>> =
                    serde_json::from_value(output["rows"].clone()).unwrap_or_default();
                let cell = result_rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(Option::as_ref);
                if let Some(count) = cell.and_then(table_browsing::target_count_cell) {
                    total_rows = Some(count);
                    count_plan = CountPlan::Exact;
                    break;
                }
            }
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
    let paging = paging_info(
        &assembled.plan,
        &schema.columns,
        &rows,
        request.page,
        request.page_size,
        total_rows,
    );
    Ok(TableDataResult {
        columns: schema.columns,
        rows,
        total_rows,
        total_rows_kind,
        page: request.page,
        page_size: request.page_size,
        paging: Some(paging),
    })
}

/// The request of a DB-15 preview: the query shape of a page read, plus the
/// one switch that only a preview makes sense of.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableReadPreviewRequest {
    pub manual_handle: Option<datazen_modules::execution::SessionHandle>,
    pub context: RowChangeTableContext,
    pub page: u32,
    pub page_size: u32,
    pub filters: Option<Vec<FilterCondition>>,
    pub sorts: Option<Vec<SortCondition>>,
    pub filter_logic: Option<String>,
    pub count_strategy: Option<CountStrategy>,
    pub default_order_columns: Option<Vec<String>>,
    pub after_key: Option<Vec<CursorValue>>,
    pub require_keyset: Option<bool>,
    pub cursor_depth: Option<u32>,
    /// Read the whole filtered table instead of one page: the paging frame
    /// (`LIMIT`/`OFFSET` and the seek predicate) is dropped.
    ///
    /// The filtered full-table export streams this statement — the export and
    /// the grid read the same builder, so the exported rows are the rows the
    /// filters really select (DB-15).
    pub unbounded: Option<bool>,
}

/// The answer of a DB-15 preview: the statements a read would send.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableReadPreviewSql {
    /// The statement that reads the rows.
    ///
    /// Paged by default; the whole filtered table when the request asked for
    /// `unbounded`.
    pub data_sql: String,
    /// The statement that counts the rows, when one runs with the read.
    ///
    /// `None` when no count runs (the strategy is unsupported, or the caller
    /// of a real read would reuse a total it already holds — a preview never
    /// says "reuse": it shows the count statement the database would answer).
    pub count_sql: Option<String>,
    /// The exact `COUNT(*)` of the filtered table, always present.
    ///
    /// A preview that asked for an estimate still shows this: the estimate is
    /// what would run, the count is what the estimate falls back to.
    pub exact_count_sql: String,
    /// How the count would be read: `exact`, `estimated` or `unsupported`.
    pub count_kind: CountStrategy,
}

/// Show the statements a table read would send, without sending them (DB-15).
///
/// The preview assembles through the same code a real read executes
/// through — the schema round trip, the filter validation, the order and the
/// paging decision are all the real ones — so what the user is shown is the
/// statement the product actually sends, not a description of it.
pub async fn preview(
    state: &AppState,
    request: TableReadPreviewRequest,
) -> Result<TableReadPreviewSql, CommandError> {
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
    // The same refusal a real read gives: a preview of a statement this
    // database would refuse is a lie, not a preview.
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
    let schema = fetch_table_schema(
        adapter,
        &runtime,
        &context,
        &target,
        revision,
        request.manual_handle.as_ref(),
        &request.context.table,
        database,
        request.context.schema.as_deref(),
    )
    .await?;
    let dialect = datazen_driver_api::LegacySqlDialect::new(driver.as_ref());
    let shape = ReadQueryShape {
        page: request.page,
        page_size: request.page_size,
        filters: request.filters.as_deref(),
        sorts: request.sorts.as_deref(),
        filter_logic: request.filter_logic.as_deref(),
        count_strategy: request.count_strategy,
        default_order_columns: request.default_order_columns.as_deref(),
        after_key: request.after_key.as_deref(),
        require_keyset: request.require_keyset.unwrap_or(false),
        cursor_depth: request.cursor_depth.unwrap_or(0),
        // A preview never "reuses" a total: its job is to show the statements,
        // and the reuse decision is a caller's optimisation, not a statement.
        skip_count: false,
    };
    let assembled = assemble_statements(
        driver.as_ref(),
        &dialect,
        &schema,
        database,
        request.context.schema.as_deref(),
        &request.context.table,
        shape,
    )?;
    let data_sql = if request.unbounded.unwrap_or(false) {
        build_unbounded_select_sql(
            &schema.table_name,
            &schema.columns,
            request.filters,
            &assembled.plan.order,
            &dialect,
            request.filter_logic.as_deref(),
        )?
    } else {
        assembled.select
    };
    let count_kind = match assembled.count_plan {
        CountPlan::Reuse | CountPlan::Unsupported => CountStrategy::Unsupported,
        CountPlan::Exact => CountStrategy::Exact,
        CountPlan::Estimated => CountStrategy::Estimated,
    };
    Ok(TableReadPreviewSql {
        data_sql,
        count_sql: assembled.count_statement,
        exact_count_sql: assembled.exact_count_statement,
        count_kind,
    })
}
