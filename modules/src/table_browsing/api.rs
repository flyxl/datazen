//! Host-facing use-case surface for table browsing (host-boundary F4b).
//!
//! 固定 `api` 模块的写法（边界方案 §3）：入参是请求上下文 + 已构造好的
//! 端口与驱动，不依赖宿主工具包 / Axum / `AppState`。桌面宿主在这里只保留
//! 传输胶水（profile / driver 查找、平台装配）与执行端口的实现；Web 宿主
//! 以后复用同一批用例函数而不必再抄一份编排。
//!
//! ## 端口：[`TargetCommandRunner`]
//!
//! 消费者定义端口（R1，同 `table_editing::ports` 的纪律）：本模块不命名
//! runtime 门面。目标解析（桌面 = 平台适配器的 `resolve_execution_target`）、
//! 提交、轮询与产物收集由**持有 runtime 门面的那一侧**实现；算法（SQL 拼装、
//! 批布局、计数回退、结果合并）留在这里。
//!
//! ## 错误：[`TableReadError`]
//!
//! 用例层校验/构建失败走 `Validation` / `Internal`，与桌面原内联
//! `CommandError` 构造一一对应；端口失败（runtime / 目标解析）走 `Runtime`
//! **原样透传**——宿主映射表里 `ResourceBusy` 等可处置变体不容折叠（折叠
//! 会改变 IPC 错误类别）。桌面实现 `From<TableReadError> for CommandError`
//! 是唯一一份桌面映射表，Web 宿主以后自持 HTTP 版。
//!
//! 本刀收表读取的两条用例：分页读（[`read_table_page`]）与 DB-15 预览
//! （[`preview_table_read`]）。写链路（commit/preview changes）在
//! `table_editing`；查询历史在 `history`；均不在本模块。

use datazen_driver_api::keyset::CursorValue;
use datazen_driver_api::{
    CountStrategy, DatabaseDriver, LegacySqlDialect, SqlDialect, TableDataResult, TableSchema,
    Value,
};
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::execution::target::{ExecutionTarget, NamespaceTarget, ObjectTarget};
use crate::execution::SessionHandle;
use crate::mcp::application::command_policy::inject_sql_target_fields;
use crate::shared::context::RequestContext;
use crate::shared::error::ApiError;
use crate::shared::id::{ConnectionId, Counter};
use crate::table_editing::change_set::{validate_table_context, RowChangeTableContext};
use crate::table_editing::TableEditingError;

use super::browsing;
use super::executor::filter_sql::{validate_filters, FilterRejection};
use super::executor::table_read_sql::{build_paged_select_sql, build_unbounded_select_sql};
use super::executor::{
    keyset_refusal, paging_info, resolve_paging, FilterCondition, PagingPlan, QueryExecutor,
    SortCondition,
};
use super::paging::{keyset_decision, resolve_order_spec_multi};

/// A table read request as it arrives over IPC (camelCase on the wire).
///
/// The context is the grid's [`RowChangeTableContext`]: the read and the write
/// address the same table identity, so the vocabulary is shared rather than
/// duplicated field-by-field.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableReadRequest {
    pub manual_handle: Option<SessionHandle>,
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

/// The request of a DB-15 preview: the query shape of a page read, plus the
/// one switch that only a preview makes sense of.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableReadPreviewRequest {
    pub manual_handle: Option<SessionHandle>,
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

/// The error surface of the table-read use cases.
///
/// 变体与桌面原内联 `CommandError` 构造一一对应（`Validation` / `Internal`
/// 同文案直转）；`Runtime` 不折不动——`ResourceBusy` 在宿主映射表里是独立
/// 可处置类别，折叠成 `Internal` 会改变 IPC 错误外观。
#[derive(Debug, thiserror::Error)]
pub enum TableReadError {
    /// 执行端口失败（目标解析、提交、轮询、产物收集），原样交给宿主映射。
    #[error(transparent)]
    Runtime(#[from] ApiError),
    /// 用例层校验或构建失败。
    #[error("{0}")]
    Validation(String),
    /// 用例层内部故障（如驱动声明了估计计数却不给语句）。
    #[error("{0}")]
    Internal(String),
}

impl From<FilterRejection> for TableReadError {
    fn from(rejection: FilterRejection) -> Self {
        Self::Validation(rejection.contract_message())
    }
}

/// Where and under which identity a table command runs.
///
/// One read resolves these once and reuses them for every statement it sends
/// (schema round trip, page batch, fallback count), so a page and its count
/// cannot drift onto different targets.
#[derive(Clone)]
pub struct TargetBinding {
    /// The explicit target the commands run at.
    pub target: ExecutionTarget,
    /// The connection config revision the caller read; a mismatch fails closed.
    pub revision: Counter,
    /// An editor session handle to run in, when the caller holds one. `None`
    /// runs at the target through the host's resolution path.
    pub manual_handle: Option<SessionHandle>,
}

/// Run one command at a table target and collect its outputs (F4b port).
///
/// 消费者定义端口（R1）：实现方持有 runtime 门面与目标解析（桌面 = 平台
/// 适配器），完成提交令牌、执行、终态轮询与产物分块收集。返回的
/// `Vec<Json>` 是产物输出块的原样序列（`rows` / `commandResult` 两种形状
/// 都真实存在，由 [`statement_row_sets`] 统一解读）。
///
/// 端口**不重试、不缓存、不做业务判断**；失败一律 `ApiError`。
#[async_trait::async_trait]
pub trait TargetCommandRunner: Send + Sync {
    /// Execute `command` with `input` under `binding`, returning every output
    /// chunk the execution published.
    async fn run(
        &self,
        context: &RequestContext,
        binding: &TargetBinding,
        command: &str,
        input: Json,
    ) -> Result<Vec<Json>, ApiError>;
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
) -> Result<AssembledStatements, TableReadError> {
    // The primary key has one source: the schema's own normalised list. The
    // column flags are a second, possibly stale, copy of the same fact, and
    // a driver that fills one and not the other used to lose its key here
    // and order by the first column instead.
    let primary_keys = schema.effective_primary_keys();
    browsing::report_pk_source_disagreement(&schema.table_name, &schema.columns, &primary_keys);
    let default_order = browsing::resolve_default_order(
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
        return Err(TableReadError::Validation(message));
    }

    let select = build_paged_select_sql(
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
        match browsing::resolve_count_strategy(
            driver,
            shape.count_strategy,
            false,
            browsing::has_effective_filters(shape.filters),
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
                    TableReadError::Internal(
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

/// Fetch the typed schema a read (or a preview) assembles against.
///
/// One round trip; the statement assembly is local. Shared by the page read
/// and the DB-15 preview so the two cannot build against different schemas.
async fn fetch_table_schema(
    context: &RequestContext,
    runner: &dyn TargetCommandRunner,
    binding: &TargetBinding,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<TableSchema, TableReadError> {
    let outputs = runner
        .run(
            context,
            binding,
            "get_table_schema",
            json!({"table": table, "database": database, "schema": schema}),
        )
        .await?;
    let definition = outputs
        .iter()
        .find(|output| output["kind"] == "commandResult")
        .and_then(|output| output.get("data"))
        .and_then(|data| data.get("schema"))
        .ok_or_else(|| {
            TableReadError::Validation("Typed table schema result unavailable".into())
        })?;
    serde_json::from_value(definition.clone())
        .map_err(|_| TableReadError::Validation("Invalid table schema result".into()))
}

/// The row sets of every statement a read executed, keyed by statement index.
///
/// Two shapes reach this code and both are real. A resource that streams rows
/// publishes them as `rows` chunks, one per block, each tagged with the
/// statement it came from; a resource that answers the command in one piece
/// publishes a single `commandResult` whose `data.results` holds one entry per
/// statement in batch order. Reading only the streaming shape is not a
/// narrower version of this helper — it is an empty page for every driver that
/// answers in one piece.
fn statement_row_sets(
    outputs: Vec<Json>,
) -> Result<std::collections::BTreeMap<u64, Vec<Vec<Option<Value>>>>, TableReadError> {
    let mut statements: std::collections::BTreeMap<u64, Vec<Vec<Option<Value>>>> =
        std::collections::BTreeMap::new();
    for output in outputs {
        match output["kind"].as_str() {
            Some("rows") => {
                let index = output["statementIndex"].as_u64().ok_or_else(|| {
                    TableReadError::Validation("Result statement identity unavailable".into())
                })?;
                let chunk: Vec<Vec<Option<Value>>> = serde_json::from_value(output["rows"].clone())
                    .map_err(|_| TableReadError::Validation("Invalid table result rows".into()))?;
                // Drivers publish a final empty result chunk after each
                // statement; appending it keeps the row data and adds nothing.
                statements.entry(index).or_default().extend(chunk);
            }
            Some("commandResult") => {
                let results = output["data"]["results"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for (index, result) in results.into_iter().enumerate() {
                    let rows: Vec<Vec<Option<Value>>> =
                        serde_json::from_value(result["rows"].clone()).map_err(|_| {
                            TableReadError::Validation("Invalid table result rows".into())
                        })?;
                    statements.entry(index as u64).or_default().extend(rows);
                }
            }
            _ => continue,
        }
    }
    Ok(statements)
}

/// Validate the grid's table context for a read-shaped request.
///
/// The host runs the same check before its profile lookup (its copy keeps the
/// error order stable for this transport); this copy is the one that holds
/// when a second transport calls the use case directly.
fn validate_read_context(context: &RowChangeTableContext) -> Result<(), TableReadError> {
    validate_table_context(context).map_err(|error| match error {
        TableEditingError::Validation(message) => TableReadError::Validation(message),
        other => TableReadError::Internal(other.to_string()),
    })
}

/// Bind the target a table command runs at: the table object inside its
/// namespace, under the config revision the caller read.
fn target_binding(
    context: &RowChangeTableContext,
    manual_handle: Option<SessionHandle>,
    revision: Counter,
) -> TargetBinding {
    TargetBinding {
        target: ExecutionTarget::new(
            ConnectionId::new(&context.connection_id),
            NamespaceTarget::new(
                context.database.clone(),
                None,
                context.schema.clone(),
                Vec::new(),
            ),
        )
        .with_object(ObjectTarget::new("table", context.table.clone())),
        revision,
        manual_handle,
    }
}

/// Read one page of a table through an explicit target execution.
///
/// The count and the rows travel in one statement batch; a failed estimate
/// degrades to a rows-only batch and then to the exact `COUNT(*)`, never to a
/// missing page (DB-15). The host has already validated the profile, the
/// driver type and the filters; every check here re-runs because it must also
/// hold when a second transport calls this function.
pub async fn read_table_page(
    driver: &dyn DatabaseDriver,
    request: TableReadRequest,
    revision: Counter,
    context: RequestContext,
    runner: &dyn TargetCommandRunner,
) -> Result<TableDataResult, TableReadError> {
    validate_read_context(&request.context)?;
    if request.page_size == 0 || request.page_size > 10000 {
        return Err(TableReadError::Validation("Invalid table page size".into()));
    }
    // The filter list is checked before anything is built or fetched: a filter
    // this database cannot serve, or one that is missing a value, has to fail
    // before the statement is assembled and before the connection is used.
    validate_filters(
        driver.supported_filter_operators(),
        request.filters.as_deref().unwrap_or_default(),
    )?;
    let binding = target_binding(&request.context, request.manual_handle.clone(), revision);
    let database = request
        .context
        .database
        .as_deref()
        .ok_or_else(|| TableReadError::Validation("Table database required".into()))?;
    let schema = fetch_table_schema(
        &context,
        runner,
        &binding,
        &request.context.table,
        database,
        request.context.schema.as_deref(),
    )
    .await?;
    // SQL construction depends on the dialect contract only; execution below
    // still goes through the driver's explicit target path.
    let dialect = LegacySqlDialect::new(driver);
    // One assembly for everything (DB-15): the page statement, the count
    // statement and the count plan are built in the one place a preview can
    // reach, so what runs is what a preview would have shown.
    let assembled = assemble_statements(
        driver,
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
                TableReadError::Internal("Exact count statement unavailable".into())
            })?;
            format!("{count};\n{select}")
        }
        CountPlan::Estimated => {
            let estimate = assembled.count_statement.ok_or_else(|| {
                TableReadError::Internal(
                    "the driver declared an estimated row count but produced no statement".into(),
                )
            })?;
            format!("{estimate};\n{select}")
        }
    };
    let mut estimate_failed = false;
    // F7: the read's `FROM` carries the bare table name the driver reported, so
    // the schema has to travel with the statement. An at-target `query` bypasses
    // the `driver_command` envelope that injects the targeting fields, and every
    // SQL driver qualifies unqualified relations from exactly those fields
    // (`qualify_sql_target`) instead of switching the session — without them a
    // read of a non-default schema resolves against the session default and
    // fails with "Invalid object name".
    let read_call = |sql: &str| {
        let mut input = json!({ "sql": sql });
        inject_sql_target_fields(
            &mut input,
            Some(database),
            request.context.schema.as_deref(),
        );
        input
    };
    let outputs = match runner
        .run(&context, &binding, "query", read_call(&sql))
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
                "table read: the row count estimate failed; reading the rows on their own"
            );
            estimate_failed = true;
            runner
                .run(&context, &binding, "query", read_call(&select))
                .await?
        }
        Err(error) => return Err(error.into()),
    };
    // A batch that carries a count puts it first and the rows second; a
    // batch that carries only the rows has the rows at index 0.
    let count_at = match count_plan {
        CountPlan::Exact | CountPlan::Estimated if !estimate_failed => Some(0u64),
        _ => None,
    };
    let rows_at = u64::from(count_at.is_some());
    let statements = statement_row_sets(outputs)?;
    let mut rows = Vec::new();
    let mut total_rows = None;
    for (index, result_rows) in &statements {
        if count_at == Some(*index) {
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
                CountPlan::Estimated => cell.and_then(browsing::estimated_count_cell),
                // The exact count keeps the reading this path has always had.
                _ => cell.and_then(browsing::target_count_cell),
            };
            if let Some(count) = count {
                total_rows = Some(count);
            }
        } else if *index == rows_at {
            rows.extend(result_rows.iter().cloned());
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
        let counted = runner
            .run(
                &context,
                &binding,
                "query",
                read_call(assembled.exact_count_statement.as_str()),
            )
            .await;
        if let Ok(outputs) = counted {
            for (index, result_rows) in statement_row_sets(outputs).unwrap_or_default() {
                if index != 0 {
                    continue;
                }
                let cell = result_rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(Option::as_ref);
                if let Some(count) = cell.and_then(browsing::target_count_cell) {
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

/// Show the statements a table read would send, without sending them (DB-15).
///
/// The preview assembles through the same code a real read executes
/// through — the schema round trip, the filter validation, the order and the
/// paging decision are all the real ones — so what the user is shown is the
/// statement the product actually sends, not a description of it.
pub async fn preview_table_read(
    driver: &dyn DatabaseDriver,
    request: TableReadPreviewRequest,
    revision: Counter,
    context: RequestContext,
    runner: &dyn TargetCommandRunner,
) -> Result<TableReadPreviewSql, TableReadError> {
    validate_read_context(&request.context)?;
    if request.page_size == 0 || request.page_size > 10000 {
        return Err(TableReadError::Validation("Invalid table page size".into()));
    }
    // The same refusal a real read gives: a preview of a statement this
    // database would refuse is a lie, not a preview.
    validate_filters(
        driver.supported_filter_operators(),
        request.filters.as_deref().unwrap_or_default(),
    )?;
    let binding = target_binding(&request.context, request.manual_handle.clone(), revision);
    let database = request
        .context
        .database
        .as_deref()
        .ok_or_else(|| TableReadError::Validation("Table database required".into()))?;
    let schema = fetch_table_schema(
        &context,
        runner,
        &binding,
        &request.context.table,
        database,
        request.context.schema.as_deref(),
    )
    .await?;
    let dialect = LegacySqlDialect::new(driver);
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
        driver,
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

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::{ColumnSchema, TableOptions};
    use std::sync::Arc;

    fn context() -> RequestContext {
        use crate::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn table_context() -> RowChangeTableContext {
        RowChangeTableContext {
            connection_id: "conn-1".into(),
            db_session_id: "session-1".into(),
            driver_type: "postgres".into(),
            database: Some("app".into()),
            schema: None,
            table: "users".into(),
        }
    }

    fn column(name: &str, data_type: &str, primary: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: data_type.into(),
            nullable: !primary,
            default_value: None,
            comment: None,
            is_primary_key: primary,
            is_auto_increment: false,
        }
    }

    fn schema() -> TableSchema {
        TableSchema {
            table_name: "users".into(),
            columns: vec![column("id", "integer", true), column("name", "text", false)],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: TableOptions::default(),
        }
    }

    fn shape<'a>() -> ReadQueryShape<'a> {
        ReadQueryShape {
            page: 0,
            page_size: 10,
            filters: None,
            sorts: None,
            filter_logic: None,
            count_strategy: None,
            default_order_columns: None,
            after_key: None,
            require_keyset: false,
            cursor_depth: 0,
            skip_count: false,
        }
    }

    fn driver() -> Arc<MockDriver> {
        MockDriver::new("postgres", MockDriverOptions::default())
    }

    /// The port must be object-safe: the api takes it as `&dyn`.
    #[test]
    fn the_port_is_object_safe() {
        fn assert_object_safe<T: Send + Sync + ?Sized>() {}
        assert_object_safe::<dyn TargetCommandRunner>();
    }

    #[test]
    fn exact_plan_carries_the_count_built_for_it() {
        let driver = driver();
        let dialect = LegacySqlDialect::new(driver.as_ref());
        let schema = schema();
        let assembled = assemble_statements(
            driver.as_ref(),
            &dialect,
            &schema,
            "app",
            None,
            "users",
            shape(),
        )
        .expect("assembly");
        // The exact plan runs the same COUNT(*) the fallback would run —
        // one statement, one place (DB-15).
        assert_eq!(assembled.count_plan, CountPlan::Exact);
        assert_eq!(
            assembled.count_statement.as_deref(),
            Some(assembled.exact_count_statement.as_str())
        );
        assert!(assembled.select.to_uppercase().contains("SELECT"));
    }

    #[test]
    fn an_estimate_plan_runs_the_drivers_estimate_statement() {
        let driver = MockDriver::new(
            "postgres",
            MockDriverOptions {
                count_estimate_rows: Some(5),
                count_strategy: Some(CountStrategy::Estimated),
                ..MockDriverOptions::default()
            },
        );
        let dialect = LegacySqlDialect::new(driver.as_ref());
        let schema = schema();
        let assembled = assemble_statements(
            driver.as_ref(),
            &dialect,
            &schema,
            "app",
            None,
            "users",
            shape(),
        )
        .expect("assembly");
        assert_eq!(assembled.count_plan, CountPlan::Estimated);
        let estimate = assembled.count_statement.expect("estimate statement");
        assert_ne!(estimate, assembled.exact_count_statement);
        // The exact COUNT(*) still exists for the fallback and the preview.
        assert!(!assembled.exact_count_statement.is_empty());
    }

    #[test]
    fn skip_count_reuses_and_runs_no_count_statement() {
        let driver = driver();
        let dialect = LegacySqlDialect::new(driver.as_ref());
        let schema = schema();
        let shape = ReadQueryShape {
            skip_count: true,
            ..shape()
        };
        let assembled = assemble_statements(
            driver.as_ref(),
            &dialect,
            &schema,
            "app",
            None,
            "users",
            shape,
        )
        .expect("assembly");
        assert_eq!(assembled.count_plan, CountPlan::Reuse);
        assert!(assembled.count_statement.is_none());
        // The exact count still exists for the preview to show.
        assert!(!assembled.exact_count_statement.is_empty());
    }

    #[test]
    fn the_two_result_shapes_merge_into_one_row_map() {
        // The streaming shape: `rows` chunks tagged by statement index.
        let streamed = vec![
            json!({"kind": "rows", "statementIndex": 0, "rows": [[{"v": 1}]]}),
            json!({"kind": "rows", "statementIndex": 0, "rows": []}),
            json!({"kind": "rows", "statementIndex": 1, "rows": [[{"v": 2}]]}),
        ];
        let sets = statement_row_sets(streamed).expect("rows");
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[&0].len(), 1);
        assert_eq!(sets[&1].len(), 1);

        // The one-piece shape: a single `commandResult` with per-statement
        // results in batch order.
        let batched = vec![json!({
            "kind": "commandResult",
            "data": {"results": [{"rows": [[{"v": 1}]]}, {"rows": [[{"v": 2}]]}]}
        })];
        let sets = statement_row_sets(batched).expect("rows");
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[&1].len(), 1);
    }

    #[test]
    fn a_missing_statement_identity_is_a_validation_fault() {
        let bad = vec![json!({"kind": "rows", "rows": []})];
        let error = statement_row_sets(bad).expect_err("no statementIndex");
        assert!(matches!(error, TableReadError::Validation(_)));
        assert_eq!(error.to_string(), "Result statement identity unavailable");
    }

    /// A runner that answers from a script, so the use cases can be driven
    /// end to end without a runtime.
    struct ScriptedRunner {
        outputs: Vec<Json>,
    }

    #[async_trait::async_trait]
    impl TargetCommandRunner for ScriptedRunner {
        async fn run(
            &self,
            _context: &RequestContext,
            _binding: &TargetBinding,
            _command: &str,
            _input: Json,
        ) -> Result<Vec<Json>, ApiError> {
            Ok(self.outputs.clone())
        }
    }

    fn schema_output() -> Json {
        json!({
            "kind": "commandResult",
            "data": {"schema": serde_json::to_value(schema()).unwrap()}
        })
    }

    /// A batch that carries a count puts the count first and the rows second,
    /// exactly as the assembled batch would come back.
    fn batched_query_output() -> Json {
        json!({
            "kind": "commandResult",
            "data": {"results": [
                {"rows": [[Some(Value::Integer(3))]]},
                {"rows": [[Some(Value::Integer(1)), Some(Value::String("alice".into()))]]}
            ]}
        })
    }

    fn read_request() -> TableReadRequest {
        TableReadRequest {
            manual_handle: None,
            context: table_context(),
            page: 0,
            page_size: 10,
            filters: None,
            sorts: None,
            skip_count: None,
            filter_logic: None,
            count_strategy: None,
            default_order_columns: None,
            after_key: None,
            require_keyset: None,
            cursor_depth: None,
        }
    }

    #[tokio::test]
    async fn a_read_assembles_against_the_fetched_schema_and_returns_rows() {
        let runner = ScriptedRunner {
            outputs: vec![schema_output(), batched_query_output()],
        };
        let driver = driver();
        let result = read_table_page(
            driver.as_ref(),
            read_request(),
            Counter::ZERO,
            context(),
            &runner,
        )
        .await
        .expect("read");
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.total_rows, Some(3));
        assert_eq!(result.total_rows_kind, Some(CountStrategy::Exact));
        assert_eq!(result.page, 0);
        assert_eq!(result.page_size, 10);
    }

    #[tokio::test]
    async fn a_read_without_a_database_is_rejected_by_the_context_check() {
        let runner = ScriptedRunner { outputs: vec![] };
        let driver = driver();
        let mut request = read_request();
        request.context.database = None;
        let error = read_table_page(driver.as_ref(), request, Counter::ZERO, context(), &runner)
            .await
            .expect_err("no database");
        // The context check is the stricter one: it demands a non-blank
        // database, so a missing database never reaches the read's own
        // defensive `Table database required` arm (kept verbatim from the
        // host for the day a transport bypasses the context check).
        assert!(matches!(error, TableReadError::Validation(_)));
        assert!(error.to_string().contains("incomplete"));
    }

    #[tokio::test]
    async fn a_port_failure_travels_unchanged_through_the_runtime_variant() {
        struct FailingRunner;
        #[async_trait::async_trait]
        impl TargetCommandRunner for FailingRunner {
            async fn run(
                &self,
                _context: &RequestContext,
                _binding: &TargetBinding,
                _command: &str,
                _input: Json,
            ) -> Result<Vec<Json>, ApiError> {
                Err(ApiError::new(
                    crate::shared::error::ApiErrorCode::ResourceBusy,
                    "connection quota is full",
                ))
            }
        }
        let runner = FailingRunner;
        let driver = driver();
        let error = read_table_page(
            driver.as_ref(),
            read_request(),
            Counter::ZERO,
            context(),
            &runner,
        )
        .await
        .expect_err("port failure");
        // The busy fact must reach the host mapping table undistorted.
        match error {
            TableReadError::Runtime(api_error) => {
                assert_eq!(
                    api_error.code,
                    crate::shared::error::ApiErrorCode::ResourceBusy
                );
                assert_eq!(api_error.message, "connection quota is full");
            }
            other => panic!("expected Runtime, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_preview_shows_the_statements_without_sending_them() {
        let runner = ScriptedRunner {
            outputs: vec![schema_output()],
        };
        let driver = driver();
        let request = TableReadPreviewRequest {
            manual_handle: None,
            context: table_context(),
            page: 0,
            page_size: 10,
            filters: None,
            sorts: None,
            filter_logic: None,
            count_strategy: None,
            default_order_columns: None,
            after_key: None,
            require_keyset: None,
            cursor_depth: None,
            unbounded: None,
        };
        let preview =
            preview_table_read(driver.as_ref(), request, Counter::ZERO, context(), &runner)
                .await
                .expect("preview");
        assert!(preview.data_sql.to_uppercase().contains("SELECT"));
        // The preview always shows the exact count and what would run.
        assert!(preview.count_sql.is_some());
        assert!(!preview.exact_count_sql.is_empty());
        assert_eq!(preview.count_kind, CountStrategy::Exact);
    }

    #[tokio::test]
    async fn an_incomplete_context_is_rejected() {
        let runner = ScriptedRunner { outputs: vec![] };
        let driver = driver();
        let mut request = read_request();
        request.context.table = "  ".into();
        let error = read_table_page(driver.as_ref(), request, Counter::ZERO, context(), &runner)
            .await
            .expect_err("incomplete context");
        assert!(matches!(error, TableReadError::Validation(_)));
        assert!(error.to_string().contains("incomplete"));
    }

    /// 去掉注释行后的源码（与 target_resolver 的闸门同款）。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The api surface must not name a host type: no host toolkit, no
    /// AppState, no desktop adapter, no runtime facade. The port is the only
    /// execution seam. The forbidden words are concatenated so the test's
    /// own literals do not trip it.
    #[test]
    fn the_api_signature_carries_no_host_type() {
        let source = code_only(include_str!("api.rs"));
        for forbidden in [
            ["tau", "ri"].concat(),
            ["App", "State"].concat(),
            ["Platform", "Adapter"].concat(),
            ["Runtime", "ConnectionUseCases"].concat(),
        ] {
            assert!(
                !source.contains(forbidden.as_str()),
                "api.rs must not reference `{forbidden}` in code"
            );
        }
    }
}
