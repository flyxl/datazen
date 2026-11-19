//! Port of `packages/frontend/src/windows/workflow/workflowStepResultUtils.tsx`
//! (the workflow run's step-result helpers) plus the view models those helpers
//! operate on, and the chartability judgement the execution-result panel
//! renders from.
//!
//! The React file exports three helpers, all ported here:
//!
//! - **`StepStatusIcon`** — the status → mark ladder: the glyph and the
//!   Tailwind colour class the icon paints in
//!   ([`WorkflowStepStatus::icon_glyph`] and
//!   [`WorkflowStepStatus::icon_ink`], with [`StatusInk::color`] resolving
//!   the class onto the kit's semantic palette: `text-green-500` → the
//!   success green, `text-yellow-500` → the warning yellow, `text-red-400`
//!   → the danger red).
//! - **`extractStepColumnNames`** — [`extract_step_column_names`]: the
//!   declared `columns` array wins, else the first row object's keys, else
//!   nothing.
//! - **`stepToStatementResult`** — [`step_to_statement_result`]: the two
//!   null guards (no result rows / nothing extractable as columns), the
//!   [`ColumnInfo`] assembly (`dataType || 'text'`, `nullable: true`), the
//!   object/array row conversion and the sql/timing projections, returned
//!   as the [`StatementResult`] mirror.
//!
//! [`StatementPreview`] / [`step_to_statement_preview`] stay as the
//! display-safe projection the execution-result panel consumes (the row
//! count, the first-20 table, the >1000 sampling warning); the preview is
//! derived from the formal conversion, so the panel and the statement
//! contract share one path.
//!
//! The chartability judgement (`isChartableResult` from
//! `lib/chart/fieldInference.ts`) stays the chartability-relevant slice
//! ([`is_chartable_preview`]); the rest of that file (the full field
//! inference) is T079's port, not this one's.
//!
//! The view models mirror the contract types in
//! `packages/backend-client/src/types/workflow.ts` (`StepExecutionResult`,
//! `WorkflowExecutionResult`, `StepStatus`) and
//! `packages/backend-client/src/types/driverCommands.ts` (`ColumnInfo`,
//! `StatementResult`, `Value`). The API crate (`datazen-ui-api`) only
//! carries the lite run report (`WorkflowStepResult { step, ok, summary }`),
//! so the full shapes live here until the host contract formalises them —
//! WorkflowPage (T147) is the first consumer that needs typed steps — and
//! `WorkflowExecutionResult::from_step_results` is the honest projection
//! that bridges the two today.

use rune::Color;
use serde_json::Value;

use crate::ui::kit::kit;

/// React's default column typing (`c.dataType || 'text'`), the type every
/// column `stepToStatementResult` assembles carries when the step's result
/// declared none — or declared as an empty string, which React's falsy `||`
/// defaults exactly like an absent one.
const DEFAULT_COLUMN_TYPE: &str = "text";

/// Mirror of `StepStatus` (`'success' | 'failed' | 'skipped' | 'timed_out'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; the panel reads it, tests pin all four
pub(crate) enum WorkflowStepStatus {
    Success,
    Failed,
    Skipped,
    TimedOut,
}

impl WorkflowStepStatus {
    /// Parse the contract's status string; anything it does not name is the
    /// failure bucket, exactly like React's icon ladders (unknown → `✗`
    /// red).
    ///
    /// The panel matches on the enum (the lite run report carries booleans,
    /// and `from_step_results` maps them); the string parse is the ladder's
    /// entry for the day a host contract hands statuses as strings —
    /// WorkflowPage (T147) is the first consumer — the same reusable-VM
    /// stance as the enum's own allow, pinned by the tests.
    #[allow(dead_code)]
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "success" => Self::Success,
            "skipped" => Self::Skipped,
            "timed_out" => Self::TimedOut,
            _ => Self::Failed,
        }
    }

    /// React's `StepStatusIcon` glyph for the status — the literal text mark
    /// the component renders (`✓` / `⏭` / `⏱` / `✗`).
    ///
    /// The glyph strings are the ladder's honest data. A consumer whose font
    /// pool cannot shape `⏭`/`⏱` draws the lucide skip-forward / clock icons
    /// in the ink's place instead — the precedent `execution_result_panel`
    /// set for its own inline ladder (which comes from that panel's own
    /// React source, whose colour ladder does not special-case `timed_out`;
    /// this utils ladder does, painting it the warning yellow).
    #[allow(dead_code)] // WorkflowPage (T147) is the first consumer; tests pin it
    pub(crate) fn icon_glyph(self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::Skipped => "⏭",
            Self::TimedOut => "⏱",
            Self::Failed => "✗",
        }
    }

    /// React's `StepStatusIcon` colour ladder: green for success, yellow for
    /// skipped and timed-out, red for the failure bucket every unnamed
    /// status parses into.
    #[allow(dead_code)] // WorkflowPage (T147) is the first consumer; tests pin it
    pub(crate) fn icon_ink(self) -> StatusInk {
        match self {
            Self::Success => StatusInk::Success,
            Self::Skipped | Self::TimedOut => StatusInk::Warning,
            Self::Failed => StatusInk::Danger,
        }
    }
}

/// React's `StepStatusIcon` ink ladder, mapped onto the kit's semantic
/// palette: `text-green-500` → the success green, `text-yellow-500` → the
/// warning yellow, `text-red-400` → the danger red. The ink resolves through
/// the kit's palette — the same tokens `execution_result_panel` paints its
/// own ✓/✗ with — so no raw `oklch` literals enter the port (those live only
/// in `ui/palette.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // WorkflowPage (T147) is the first consumer; tests pin it
pub(crate) enum StatusInk {
    Success,
    Warning,
    Danger,
}

impl StatusInk {
    /// The ink resolved through the kit's semantic palette for one
    /// appearance.
    #[allow(dead_code)] // WorkflowPage (T147) is the first consumer; tests pin it
    pub(crate) fn color(self, dark: bool) -> Color {
        let palette = &kit(dark).c;
        match self {
            Self::Success => palette.success.fg,
            Self::Warning => palette.warning.fg,
            Self::Danger => palette.danger.fg,
        }
    }
}

/// Mirror of `StepExecutionResult`. `step_type` is `Option`: the React type
/// declares it required, but the lite run report the page feeds today types
/// no steps, and the panel omits the `[{type}]` chip rather than displaying a
/// type nobody reported (the chip returns when the typed contract lands).
#[derive(Clone, Debug)]
pub(crate) struct StepExecutionResult {
    pub(crate) step_id: String,
    pub(crate) step_type: Option<String>,
    pub(crate) status: WorkflowStepStatus,
    /// `result?: Record<string, unknown>` — carried as the raw JSON the
    /// host's engine would send; `None` is React's `undefined`.
    pub(crate) result: Option<Value>,
    pub(crate) execution_time_ms: u64,
    pub(crate) error: Option<String>,
    pub(crate) connection_name: Option<String>,
    pub(crate) sql_executed: Option<String>,
}

/// Mirror of `WorkflowExecutionResult`.
#[derive(Clone, Debug)]
pub(crate) struct WorkflowExecutionResult {
    pub(crate) success: bool,
    pub(crate) final_output: String,
    pub(crate) steps: Vec<StepExecutionResult>,
    pub(crate) total_time_ms: u64,
    pub(crate) error: Option<String>,
}

/// One preview column: React's `{ name, dataType? }` from
/// `extractStepColumnNames`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StepColumn {
    pub(crate) name: String,
    pub(crate) data_type: Option<String>,
}

/// Mirror of `ColumnInfo` (`driverCommands.ts`) the way
/// `stepToStatementResult` assembles it: the React default typing
/// (`dataType || 'text'`) and the pinned `nullable: true`. The driver's own
/// statement results carry real declarations and nullability instead; this
/// assembly never lies about either, it just fills React's defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnInfo {
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) nullable: bool,
}

/// Mirror of `StatementResult` (`driverCommands.ts`) the way
/// `stepToStatementResult` returns it: the executed SQL, the assembled
/// columns and the converted rows. The contract's optional legs (`source`,
/// `runtimeBinding`, `provenance`, `rowsAffected`, `truncated`, `pinned`)
/// belong to the driver's own results — the step conversion never sets them,
/// so the mirror does not carry them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StatementResult {
    pub(crate) sql: String,
    pub(crate) columns: Vec<ColumnInfo>,
    pub(crate) rows: Vec<Vec<Option<Value>>>,
    pub(crate) execution_time_ms: u64,
}

/// The display-safe projection of [`StatementResult`] the panel renders:
/// the columns re-typed the way `stepToStatementResult` assembles them, the
/// full row list (the >1000 sampling warning and the `... N more` row read
/// it), and the row count the host reported (`rows_count`, when it did).
#[derive(Clone, Debug)]
pub(crate) struct StatementPreview {
    pub(crate) columns: Vec<StepColumn>,
    pub(crate) rows: Vec<Vec<Option<Value>>>,
    pub(crate) rows_count: usize,
}

impl WorkflowExecutionResult {
    /// The honest bridge from the port's lite run report (name / ok /
    /// summary per step) to this VM: every step keeps its text as the id, a
    /// failure carries its summary as the step's error line, and nothing the
    /// lite report never said is invented — no step types, no timing, no
    /// payloads, no workflow-level output or error. The typed contract
    /// replaces this projection when the host formalises the full step
    /// shapes (WorkflowPage, T147, is the first consumer that needs them).
    pub(crate) fn from_step_results(results: &[datazen_ui_api::WorkflowStepResult]) -> Self {
        let steps = results
            .iter()
            .map(|r| StepExecutionResult {
                step_id: r.step.clone(),
                step_type: None,
                status: if r.ok {
                    WorkflowStepStatus::Success
                } else {
                    WorkflowStepStatus::Failed
                },
                result: None,
                execution_time_ms: 0,
                error: (!r.ok).then(|| r.summary.clone()),
                connection_name: None,
                sql_executed: None,
            })
            .collect();
        Self {
            success: results.iter().all(|r| r.ok),
            final_output: String::new(),
            steps,
            total_time_ms: 0,
            error: None,
        }
    }
}

/// React's `extractStepColumnNames`: the `columns` array wins when it has
/// entries, otherwise the first row object's keys; nothing extractable is
/// `[]`. React's `undefined` result is the caller's `Option<&Value>` (the
/// same stance the `StepExecutionResult::result` field takes).
///
/// A `columns` entry without a string `name` is dropped rather than kept as
/// a nameless column: React would hand `{ name: undefined }` on to the row
/// mapping (`obj[undefined]` → null cells), the port refuses to invent a
/// column nobody can read — and if that empties the set, the row-key
/// fallback takes over exactly like React's `cols.length > 0` check.
pub(crate) fn extract_step_column_names(result: &Value) -> Vec<StepColumn> {
    if let Some(cols) = result.get("columns").and_then(Value::as_array) {
        let named: Vec<StepColumn> = cols
            .iter()
            .filter_map(|col| {
                let name = col.get("name")?.as_str()?.to_owned();
                let data_type = col
                    .get("dataType")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Some(StepColumn { name, data_type })
            })
            .collect();
        if !named.is_empty() {
            return named;
        }
    }
    if let Some(rows) = result.get("rows").and_then(Value::as_array) {
        if let Some(first) = rows.first().and_then(Value::as_object) {
            return first
                .keys()
                .map(|name| StepColumn {
                    name: name.clone(),
                    data_type: None,
                })
                .collect();
        }
    }
    Vec::new()
}

/// React's `stepToStatementResult` — the formal port.
///
/// The two null guards come first: no `result`, no `rows` array, or nothing
/// extractable as columns converts to nothing. Then the statement result
/// assembles: the `ColumnInfo` columns with React's defaults (`dataType ||
/// 'text'`, `nullable: true`), the rows converted by [`statement_rows`], the
/// SQL from `step.sqlExecuted ?? ''`, and the timing from
/// `result.execution_time_ms ?? step.executionTimeMs` (a JSON number the
/// `u64` field cannot hold honestly — negative or fractional — falls back
/// to the step's own clock rather than lying about the type).
pub(crate) fn step_to_statement_result(step: &StepExecutionResult) -> Option<StatementResult> {
    // React: `const r = step.result; if (!r?.rows) return null`.
    let result = step.result.as_ref()?;
    let raw_rows = result.get("rows")?.as_array()?;
    let cols = extract_step_column_names(result);
    if cols.is_empty() {
        return None;
    }
    // React: `cols.map(c => ({ name, dataType: c.dataType || 'text',
    // nullable: true }))` — the `||` is falsy semantics, so an empty-string
    // dataType defaults the same as an absent one.
    let columns: Vec<ColumnInfo> = cols
        .iter()
        .map(|column| ColumnInfo {
            name: column.name.clone(),
            data_type: column
                .data_type
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(DEFAULT_COLUMN_TYPE)
                .to_owned(),
            nullable: true,
        })
        .collect();
    Some(StatementResult {
        // React: `step.sqlExecuted ?? ''`.
        sql: step.sql_executed.clone().unwrap_or_default(),
        columns,
        rows: statement_rows(raw_rows, &cols)?,
        // React: `(r.execution_time_ms as number) ?? step.executionTimeMs`.
        execution_time_ms: result
            .get("execution_time_ms")
            .and_then(Value::as_u64)
            .unwrap_or(step.execution_time_ms),
    })
}

/// React's row conversion. The **first** row picks the mode, exactly as the
/// source branches:
///
/// - an object first row maps **every** row per column (`obj[c.name] ?? null`
///   — a missing key is a null cell; a non-object row has none of the
///   columns, so it maps to a null row, the `undefined` React's property
///   access yields there);
/// - anything else (an array first row, or no rows at all) passes the rows
///   through positionally at their own length — a short row is simply short,
///   the way React hands `rawRows` to the grid unchanged.
///
/// A pass-through row that is neither an array nor an object cannot become
/// cells this table can render (React's unchecked `rawRows as (Value |
/// null)[][]` cast would hand the raw scalar to the grid as a "row"); the
/// honest port refuses it with `None`.
fn statement_rows(raw_rows: &[Value], cols: &[StepColumn]) -> Option<Vec<Vec<Option<Value>>>> {
    match raw_rows.first() {
        // Object mode: every row is read per column.
        Some(Value::Object(_)) => Some(
            raw_rows
                .iter()
                .map(|raw| match raw {
                    Value::Object(object) => cols
                        .iter()
                        .map(|column| object.get(&column.name).cloned())
                        .collect(),
                    // React's `obj[c.name]` on a non-object is `undefined`
                    // → null cells.
                    _ => cols.iter().map(|_| None).collect(),
                })
                .collect(),
        ),
        // Pass-through mode: rows keep their positional shape and length.
        _ => raw_rows
            .iter()
            .map(|raw| match raw {
                Value::Array(array) => {
                    Some(array.iter().map(|value| Some(value.clone())).collect())
                }
                _ => None,
            })
            .collect(),
    }
}

/// The display projection of [`step_to_statement_result`] the
/// execution-result panel renders: the typed columns, the full row list and
/// the row count — React's panel reads `rows_count` off the raw result the
/// same way (`(step.result?.rows_count as number) ?? rows.length ?? 0`), so
/// the count is read here, not carried by the statement result.
pub(crate) fn step_to_statement_preview(step: &StepExecutionResult) -> Option<StatementPreview> {
    let statement = step_to_statement_result(step)?;
    let rows_count = step
        .result
        .as_ref()
        .and_then(|result| result.get("rows_count"))
        .and_then(Value::as_u64)
        .map_or_else(|| statement.rows.len(), |count| count as usize);
    Some(StatementPreview {
        columns: statement
            .columns
            .iter()
            .map(|column| StepColumn {
                name: column.name.clone(),
                data_type: Some(column.data_type.clone()),
            })
            .collect(),
        rows: statement.rows,
        rows_count,
    })
}

/// React's `isChartableResult` (the chartability-relevant slice of
/// `lib/chart/fieldInference.ts`): true when any column infers numeric — the
/// declared type's numeric keywords, or (after excluding the boolean and
/// datetime type families, which React resolves first) a sample column whose
/// non-null values are all numbers or numeric-like strings. The rest of that
/// file (the full `inferFieldType` / `inferAllFields` ladder — date-like
/// strings, distinct counts) is T079's port, not this one's; this slice
/// stays because the panel's table/chart toggle is this module's consumer.
pub(crate) fn is_chartable_preview(preview: &StatementPreview) -> bool {
    if preview.rows.is_empty() || preview.columns.is_empty() {
        return false;
    }
    preview
        .columns
        .iter()
        .enumerate()
        .any(|(index, column)| column_infers_numeric(column, preview, index))
}

pub(crate) fn column_infers_numeric(column: &StepColumn, preview: &StatementPreview, index: usize) -> bool {
    let declared = column
        .data_type
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    if declared.contains("bool") {
        return false; // React: 'boolean'
    }
    if [
        "int", "serial", "double", "numeric", "decimal", "real", "float", "money", "bigint",
        "smallint",
    ]
    .iter()
    .any(|word| declared.contains(word))
    {
        return true; // React: 'numeric' by declaration
    }
    if ["timestamp", "date", "time"]
        .iter()
        .any(|word| declared.contains(word))
    {
        return false; // React: 'datetime' by declaration
    }
    let non_null: Vec<&Value> = preview
        .rows
        .iter()
        .take(50)
        .filter_map(|row| row.get(index).and_then(Option::as_ref))
        // React's `filter(v => v != null)`: a JSON null cell is no sample
        // either — a nullable numeric column stays numeric.
        .filter(|value| !value.is_null())
        .collect();
    if non_null.is_empty() {
        return false; // React: 'unknown'
    }
    non_null.iter().all(|value| numeric_like(value))
}

/// React's `isNumericLike`: a JSON number, or a non-empty string that parses
/// to a number that is not `NaN` (React's `Number('NaN')` is `NaN`, so a
/// NaN sentinel string is not numeric-like; Rust's parser would otherwise
/// accept it). The other JS-number-string edges — hex literals, the exact
/// `Infinity` spellings — stay the port's honesty: decimal and scientific
/// notation are the shapes a result set actually carries.
fn numeric_like(value: &Value) -> bool {
    match value {
        Value::Number(_) => true,
        Value::String(text) => {
            let trimmed = text.trim();
            !trimmed.is_empty() && trimmed.parse::<f64>().is_ok_and(|n| !n.is_nan())
        }
        _ => false,
    }
}

/// React's `String(value)` projection for preview cells and AI results:
/// arrays join their elements with commas, objects become
/// `[object Object]`; `null`/`undefined` are the caller's null branch.
pub(crate) fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}
