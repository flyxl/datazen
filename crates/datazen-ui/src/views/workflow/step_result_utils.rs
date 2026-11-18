//! The workflow execution-result view models, plus the minimal slice of
//! `packages/frontend/src/windows/workflow/workflowStepResultUtils.tsx` and
//! `lib/chart/fieldInference.ts` that the execution-result panel needs.
//!
//! This is **not** the full T176 port of those helper files: it carries only
//! the column extraction, the step → statement-result conversion and the
//! chartability judgement the panel renders from, each marked *待 T176
//! 正式化* where the formal port will replace it.
//!
//! The three types mirror the contract types in
//! `packages/backend-client/src/types/workflow.ts` (`StepExecutionResult`,
//! `WorkflowExecutionResult`, `StepStatus`) and
//! `packages/backend-client/src/types/driverCommands.ts`
//! (`ColumnInfo`, `StatementResult`, `Value`). The API crate
//! (`datazen-ui-api`) only carries the lite run report
//! (`WorkflowStepResult { step, ok, summary }`), so the full shapes live here
//! until the host contract formalises them; `WorkflowExecutionResult::
//! from_step_results` is the honest projection that bridges the two today.

use serde_json::Value;

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
    /// Parse the contract's string; anything else is the failure bucket,
    /// exactly like React's icon/color ladders (unknown → `✗` red).
    ///
    /// Today only the tests exercise the parse (the lite run report carries
    /// booleans, and the panel matches on the enum); the T176 typed contract
    /// reads status strings through it — the same reusable-VM stance as the
    /// enum's own allow.
    #[allow(dead_code)]
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "success" => Self::Success,
            "skipped" => Self::Skipped,
            "timed_out" => Self::TimedOut,
            _ => Self::Failed,
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

/// The display-safe projection of `StatementResult` the panel renders:
/// columns with the React default typing (`dataType || 'text'`), the full row
/// list (the >1000 sampling warning and the `... N more` row read it), and
/// the row count the host reported (`rows_count`, when it did).
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
    /// payloads, no workflow-level output or error. The typed contract lands
    /// with T176 and replaces this projection.
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

/// Minimal `extractStepColumnNames`: the `columns` array wins when it has
/// entries, otherwise the first row object's keys. *待 T176 正式化*（the
/// full `workflowStepResultUtils` port takes over）.
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

/// Minimal `stepToStatementResult`: needs a `rows` array and extractable
/// columns; object rows are re-ordered per column like React's mapping
/// (`obj[c.name] ?? null`), array rows pass through per cell. The row count
/// is the host's `rows_count` when present, else the row list's length.
/// *待 T176 正式化*.
pub(crate) fn step_to_statement_preview(step: &StepExecutionResult) -> Option<StatementPreview> {
    let result = step.result.as_ref()?;
    let raw_rows = result.get("rows")?.as_array()?;
    let columns = extract_step_column_names(result);
    if columns.is_empty() {
        return None;
    }
    let mut rows: Vec<Vec<Option<Value>>> = Vec::with_capacity(raw_rows.len());
    for raw in raw_rows {
        let cells: Vec<Option<Value>> = match raw {
            // An object row maps per column; a missing key is null.
            Value::Object(object) => columns
                .iter()
                .map(|column| object.get(&column.name).cloned())
                .collect(),
            // An array row is already positional; a short row pads with null
            // rather than panicking on the host's data.
            Value::Array(array) => columns
                .iter()
                .enumerate()
                .map(|(index, _)| array.get(index).cloned())
                .collect(),
            // Anything else is not a row this preview can show.
            _ => return None,
        };
        rows.push(cells);
    }
    let rows_count = result
        .get("rows_count")
        .and_then(Value::as_u64)
        .map_or_else(|| rows.len(), |count| count as usize);
    Some(StatementPreview {
        columns,
        rows,
        rows_count,
    })
}

/// Minimal `isChartableResult`: true when any column infers numeric — the
/// declared type's numeric keywords, or (after excluding the boolean and
/// datetime type families, which React resolves first) a sample column whose
/// non-null values are all numbers or numeric-like strings. This is only the
/// chartability-relevant slice of `inferFieldType`; the full field inference
/// lands with T176. *待 T176 正式化*.
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

fn column_infers_numeric(column: &StepColumn, preview: &StatementPreview, index: usize) -> bool {
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
        .collect();
    if non_null.is_empty() {
        return false; // React: 'unknown'
    }
    non_null.iter().all(|value| numeric_like(value))
}

/// React's `isNumericLike`: a JSON number, or a non-empty string that parses.
fn numeric_like(value: &Value) -> bool {
    match value {
        Value::Number(_) => true,
        Value::String(text) => {
            let trimmed = text.trim();
            !trimmed.is_empty() && trimmed.parse::<f64>().is_ok()
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
