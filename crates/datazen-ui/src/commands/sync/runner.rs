//! The UI-side orchestration the React file keeps: the selection's
//! client-side folding, the comparison plan's staging, the
//! receipt-driven polling and the job→execution projections — as
//! effect-driven state machines (the `schema_diff::runner` /
//! `session_cancel` pattern: no fake async; the host runs the
//! transport and delivers the receipts back).

use serde_json::{Value, json};

use super::wire::{DataSyncSelectedRowVm, SyncOptionsVm};

/// The poll's delay (`delay(400)`).
pub const SYNC_JOB_POLL_INTERVAL_MS: f64 = 400.0;

/// The paged read's fallback page size
/// (`limit ?? activeComparisonPlan.pageSize ?? 100`).
pub const COMPARISON_PAGE_FALLBACK_SIZE: u64 = 100;

/// `valueToken(value)`: the row key's identity string — the JSON
/// array spelling (the `JSON.stringify` of `Value[]`).
///
/// Known delta: React's `catch { return String(value) }` arm covers
/// cyclic structures a DOM value array can hold; a `serde_json`
/// `Value` is always acyclic, so the fallback is dead code on this
/// side of the wire (the delta the settings pass-throughs carry too).
pub fn value_token(value: &[Value]) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
}

/// The selection row's identity: `sourceTable\0targetTable\0operation
/// \0valueToken(key)` (the React template's `\u0000` separators).
fn row_identity(source_table: &str, target_table: &str, operation: &str, key: &[Value]) -> String {
    format!(
        "{source_table}\0{target_table}\0{operation}\0{}",
        value_token(key)
    )
}

fn row_of(source_table: &str, target_table: &str, operation: &str, key: &[Value]) -> Value {
    json!({
        "sourceTable": source_table,
        "targetTable": target_table,
        "operation": operation,
        "key": key,
    })
}

/// Whether one operation survives the options
/// (`INSERT → options.insert`, `UPDATE → options.update`,
/// `DELETE → options.delete`; anything else is dropped).
fn operation_enabled(operation: &str, options: &SyncOptionsVm) -> bool {
    match operation {
        "INSERT" => options.insert,
        "UPDATE" => options.update,
        "DELETE" => options.delete,
        _ => false,
    }
}

/// `selectionFromTables`: the reviewed selection's client-side
/// folding. The `selectedRows` arm (a caller that knows its rows)
/// filters by the enabled operations; the tables arm folds the plan
/// against the requested set (the `TableResult.rows` the comparison
/// grid selected, `UNCHANGED` excluded). The scopes ride along for
/// server validation — UI state removes disabled operations before
/// it reaches here; a forged or stale caller must still be rejected
/// by the server instead of silently selecting less data.
pub fn selection_from_tables(
    plan: &Value,
    tables: &[Value],
    options: &SyncOptionsVm,
    selected_rows: Option<&[DataSyncSelectedRowVm]>,
    table_selections: Option<&Value>,
) -> Value {
    // Preserve scope input for server validation.
    let active_scopes: &[Value] = match table_selections {
        Some(Value::Array(scopes)) => scopes,
        _ => &[],
    };
    let revision = plan
        .get("selectionRevision")
        .cloned()
        .unwrap_or(Value::Null);
    let build = |rows: Vec<Value>| {
        let mut selection = json!({ "revision": revision, "rows": rows });
        if !active_scopes.is_empty() {
            selection["scopes"] = json!(active_scopes);
        }
        selection
    };

    if let Some(selected_rows) = selected_rows {
        let rows = selected_rows
            .iter()
            .filter(|row| operation_enabled(&row.operation, options))
            .map(|row| {
                row_of(
                    &row.source_table,
                    &row.target_table,
                    &row.operation,
                    &row.key,
                )
            })
            .collect();
        return build(rows);
    }

    // The requested set: the grid's selected, changed rows.
    let mut requested: Vec<String> = Vec::new();
    for table in tables {
        let source_table = table.get("sourceTable").and_then(Value::as_str).unwrap_or("");
        let target_table = table.get("targetTable").and_then(Value::as_str).unwrap_or("");
        if let Some(Value::Array(rows)) = table.get("rows") {
            for row in rows {
                let selected = row.get("selected").and_then(Value::as_bool).unwrap_or(false);
                let operation = row.get("operation").and_then(Value::as_str).unwrap_or("");
                if !selected || operation == "UNCHANGED" {
                    continue;
                }
                let key = row.get("key").and_then(|k| k.as_array()).cloned();
                if let Some(key) = key {
                    requested.push(row_identity(
                        source_table,
                        target_table,
                        operation,
                        &key,
                    ));
                }
            }
        }
    }

    // The plan's rows, filtered by selection/operation/option and the
    // requested set.
    let mut rows: Vec<Value> = Vec::new();
    if let Some(Value::Array(plan_tables)) = plan.get("tables") {
        for table in plan_tables {
            let source_table = table.get("sourceTable").and_then(Value::as_str).unwrap_or("");
            let target_table = table.get("targetTable").and_then(Value::as_str).unwrap_or("");
            let Some(Value::Array(table_rows)) = table.get("rows") else {
                continue;
            };
            for row in table_rows {
                let selected = row.get("selected").and_then(Value::as_bool).unwrap_or(false);
                let operation = row.get("operation").and_then(Value::as_str).unwrap_or("");
                if !selected || operation == "UNCHANGED" {
                    continue;
                }
                if !operation_enabled(operation, options) {
                    continue;
                }
                let Some(key) = row.get("key").and_then(|k| k.as_array()).cloned() else {
                    continue;
                };
                if !requested.contains(&row_identity(
                    source_table,
                    target_table,
                    operation,
                    &key,
                )) {
                    continue;
                }
                rows.push(row_of(source_table, target_table, operation, &key));
            }
        }
    }
    build(rows)
}

/// `selectionFromStatements`: the statement list folded back into a
/// selection. Each statement resolves against the plan's table with
/// the matching `targetTable`: an operation+key match wins; a table
/// WITHOUT row payloads (a paged comparison) trusts the statement
/// (the server-owned plan remains the authority for validating this
/// key and operation); a table WITH rows but no match falls through
/// to the next table.
pub fn selection_from_statements(plan: &Value, statements: &[Value]) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for statement in statements {
        let table_name = statement.get("table").and_then(Value::as_str).unwrap_or("");
        let operation = statement.get("operation").and_then(Value::as_str).unwrap_or("");
        let statement_key = statement.get("rowKey").and_then(|k| k.as_array()).cloned();
        let statement_key_token = statement_key.as_deref().map(value_token);
        let Some(Value::Array(plan_tables)) = plan.get("tables") else {
            continue;
        };
        for table in plan_tables {
            let target_table = table.get("targetTable").and_then(Value::as_str).unwrap_or("");
            if target_table != table_name {
                continue;
            }
            let source_table = table.get("sourceTable").and_then(Value::as_str).unwrap_or("");
            let table_rows = table.get("rows").and_then(|r| r.as_array()).cloned();
            let match_row = table_rows.as_deref().and_then(|rows| {
                rows.iter().find(|row| {
                    let row_op = row.get("operation").and_then(Value::as_str).unwrap_or("");
                    if row_op != operation {
                        return false;
                    }
                    // `valueToken(row.key) === valueToken(statement.rowKey)`
                    let row_key = row.get("key").and_then(|k| k.as_array()).cloned();
                    match (row_key, &statement_key_token) {
                        (Some(row_key), Some(statement_token)) => {
                            value_token(&row_key) == *statement_token
                        }
                        _ => false,
                    }
                })
            });
            if let Some(match_row) = match_row {
                rows.push(row_of(
                    source_table,
                    target_table,
                    match_row.get("operation").and_then(Value::as_str).unwrap_or(""),
                    match_row.get("key").and_then(|k| k.as_array()).cloned().unwrap_or_default().as_slice(),
                ));
                break;
            }
            // Paged comparisons deliberately omit row payloads.
            if table_rows.is_none() {
                if let Some(statement_key) = statement_key.clone() {
                    rows.push(row_of(source_table, target_table, operation, &statement_key));
                }
                break;
            }
        }
    }
    json!({
        "revision": plan.get("selectionRevision").cloned().unwrap_or(Value::Null),
        "rows": rows,
    })
}

/// `durableFailure(job, phase)`: the terminal, durable answer of a
/// failed prepare/apply poll — the four sentences verbatim.
pub fn durable_failure(job: &Value, phase: &str) -> String {
    let state = job.get("state").and_then(Value::as_str).unwrap_or("");
    if state == "cancelled" {
        return format!("Data Sync {phase} job was cancelled.");
    }
    let error = job.get("error").and_then(Value::as_str).unwrap_or("");
    if error == "notDispatchedAfterRestart" {
        return "Data Sync job was not executed after restart; start a fresh comparison."
            .to_owned();
    }
    let effect = job.get("effectOutcome").and_then(Value::as_str).unwrap_or("");
    let unknown_progress = job
        .get("progress")
        .and_then(|p| p.get("unknown"))
        .map(|n| n.as_f64().unwrap_or(0.0) > 0.0)
        .unwrap_or(false);
    if effect == "unknown" || unknown_progress {
        return "Data Sync write outcome needs verification; compare current data before continuing."
            .to_owned();
    }
    format!("Data Sync {phase} job failed; compare again before continuing.")
}

/// Whether a job view is terminal (`succeeded` / `failed` /
/// `cancelled`) — the poll loop's exit.
pub fn job_terminal(job: &Value) -> bool {
    matches!(
        job.get("state").and_then(Value::as_str),
        Some("succeeded") | Some("failed") | Some("cancelled")
    )
}

/// `executionFromJob(details)`: the apply job's details folded into
/// the `DataSyncExecutionResponse` the UI renders — the committed
/// counter (or the job's progress), the effect-outcome spelling, and
/// the error sentence.
pub fn execution_from_job(details: &Value) -> Value {
    let job = details.get("job").cloned().unwrap_or(Value::Null);
    let state = job.get("state").and_then(Value::as_str).unwrap_or("");
    let error_code = job.get("error").and_then(Value::as_str).unwrap_or("");
    let effect = job.get("effectOutcome").and_then(Value::as_str).unwrap_or("");

    // domainResults.find(entry => entry.stageId === 'apply')
    let apply_result = details
        .get("domainResults")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry.get("stageId").and_then(Value::as_str) == Some("apply"))
        });
    let counter = |code: &str| -> f64 {
        apply_result
            .and_then(|result| result.get("counters").and_then(Value::as_array))
            .and_then(|counters| {
                counters
                    .iter()
                    .find(|entry| entry.get("code").and_then(Value::as_str) == Some(code))
            })
            .and_then(|entry| entry.get("value"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    let committed_progress = job
        .get("progress")
        .and_then(|p| p.get("committed"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let committed = if counter("committed") != 0.0 {
        counter("committed")
    } else {
        committed_progress
    };
    // `Number(...)` — integers on the wire.
    let committed = if committed.fract() == 0.0 {
        json!(committed as i64)
    } else {
        json!(committed)
    };

    let outcome = if effect == "partiallyApplied" {
        "partially_applied"
    } else if effect == "rolledBack" {
        "rolled_back"
    } else if effect == "notStarted" {
        "not_started"
    } else if effect == "unknown" || counter("unknown") > 0.0 {
        "unknown"
    } else {
        "committed"
    };

    let error: Option<String> = if state == "succeeded" {
        None
    } else if error_code == "notDispatchedAfterRestart" {
        Some("Data Sync job was not executed after restart.".to_owned())
    } else if outcome == "unknown" {
        Some("Commit outcome is unknown; compare current data before continuing.".to_owned())
    } else {
        Some("Data Sync apply job failed; compare again before continuing.".to_owned())
    };

    json!({
        "applied": committed,
        "affectedRows": committed,
        "rolledBack": outcome == "rolled_back",
        "outcome": outcome,
        "error": error,
        "rollbackReason": if outcome == "rolled_back" { error } else { None },
        "skipped": 0,
        "conflicts": [],
    })
}

/// The module-level staging (`activeComparisonPlan` /
/// `activeExecutionOptions`): the comparison chain's remembered plan
/// and the options the last execute/generate carried.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncPlanStaging {
    /// `activeComparisonPlan` — `null` until a comparison succeeds
    /// (and `compareDataSync` clears it on entry).
    pub plan: Option<Value>,
    /// `activeExecutionOptions` — `DEFAULT_SYNC_OPTIONS` until an
    /// execute/generate replaces it.
    pub options: SyncOptionsVm,
}

impl Default for SyncPlanStaging {
    fn default() -> Self {
        Self {
            plan: None,
            options: SyncOptionsVm::default_options(),
        }
    }
}

impl SyncPlanStaging {
    pub fn new() -> Self {
        Self::default()
    }

    /// `compareDataSync`'s entry: `activeComparisonPlan = null`.
    pub fn clear_plan(&mut self) {
        self.plan = None;
    }

    /// The chain's success arm: `activeComparisonPlan = response`.
    pub fn stage_plan(&mut self, preview: Value) {
        self.plan = Some(preview);
    }

    /// The staged plan's id (`activeComparisonPlan.planId`), or the
    /// missing-plan refusal the three gated faces answer.
    pub fn plan_id(&self) -> Result<String, &'static str> {
        match &self.plan {
            Some(plan) => Ok(plan
                .get("planId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()),
            None => Err("data sync comparison plan is missing; compare again"),
        }
    }

    /// The staged plan's page size (the paged read's
    /// `activeComparisonPlan.pageSize ?? 100`).
    pub fn page_size(&self) -> u64 {
        self.plan
            .as_ref()
            .and_then(|plan| plan.get("pageSize"))
            .and_then(Value::as_u64)
            .unwrap_or(COMPARISON_PAGE_FALLBACK_SIZE)
    }

    /// `activeExecutionOptions = options` (executeDataSyncJob /
    /// generateDataSyncSql's staging arm).
    pub fn stage_options(&mut self, options: SyncOptionsVm) {
        self.options = options;
    }
}

/// The compare chain (`compareDataSync`): submit → poll → read the
/// reviewed projection, effect-driven — the host runs the transport
/// and delivers each receipt back.
#[derive(Clone, Debug, PartialEq)]
pub enum SyncCompareStep {
    /// `startDataSyncPrepareJob` fired; awaiting the accepted receipt.
    AwaitingPrepareAccept,
    /// `getDataSyncJob` until a terminal state.
    Polling { job_id: String },
    /// Terminal `succeeded`; awaiting `getDataSyncJobPreview`.
    AwaitingPreview { job_id: String },
    /// A terminal non-succeeded state: the durable failure sentence.
    Failed { message: String },
    /// The preview landed and is staged.
    Done,
}

/// The compare chain's machine over the staging.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncCompareChain {
    pub staging: SyncPlanStaging,
    pub step: Option<SyncCompareStep>,
}

impl SyncCompareChain {
    pub fn new() -> Self {
        Self::default()
    }

    /// `compareDataSync`'s entry: clear the plan and answer the
    /// prepare request to fire (`requestedJobId` is the caller's
    /// `jobId ?? data-sync-{uuid}` — the host mints the uuid).
    pub fn begin_compare(
        &mut self,
        requested_job_id: &str,
        prepare_request: (crate::commands::sync::wire::SyncMethod, Value),
    ) -> (crate::commands::sync::wire::SyncMethod, Value) {
        self.staging.clear_plan();
        self.step = Some(SyncCompareStep::AwaitingPrepareAccept);
        let _ = requested_job_id;
        prepare_request
    }

    /// The prepare receipt: `accepted.jobId` moves the chain to the
    /// poll loop.
    pub fn note_prepare_accepted(&mut self, accepted: &Value) {
        let job_id = accepted
            .get("jobId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        self.step = Some(SyncCompareStep::Polling { job_id });
    }

    /// One poll receipt (`waitForDataSyncJob`'s loop body): answers
    /// `Some(poll_request)` while the job runs (the host re-fires it
    /// after the 400ms delay), or moves the chain on a terminal
    /// state. `poll_request` builds the next
    /// `getDataSyncJob({ jobId })`.
    pub fn note_job(
        &mut self,
        job: &Value,
        phase: &str,
    ) -> Option<(crate::commands::sync::wire::SyncMethod, Value)> {
        let step = self.step.clone();
        match step {
            Some(SyncCompareStep::Polling { job_id }) => {
                if !job_terminal(job) {
                    return Some(super::wire::get_data_sync_job_request(&job_id));
                }
                let state = job.get("state").and_then(Value::as_str).unwrap_or("");
                if state == "succeeded" {
                    self.step = Some(SyncCompareStep::AwaitingPreview { job_id });
                    None
                } else {
                    self.step = Some(SyncCompareStep::Failed {
                        message: durable_failure(job, phase),
                    });
                    None
                }
            }
            _ => None,
        }
    }

    /// The preview receipt: stage the plan and finish.
    pub fn note_preview(&mut self, preview: Value) {
        self.staging.stage_plan(preview);
        self.step = Some(SyncCompareStep::Done);
    }

    /// The failure the chain ended on (the thrown `durableFailure`).
    pub fn failure(&self) -> Option<&str> {
        match &self.step {
            Some(SyncCompareStep::Failed { message }) => Some(message),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::sync::wire::{
        SyncMethod, SyncOptionsVm, start_data_sync_prepare_job_request,
    };

    fn options() -> SyncOptionsVm {
        SyncOptionsVm::default_options()
    }

    fn plan() -> Value {
        json!({
            "planId": "plan-1",
            "selectionRevision": "rev-2",
            "pageSize": 40,
            "tables": [
                {
                    "sourceTable": "s.users",
                    "targetTable": "t.users",
                    "rows": [
                        { "operation": "INSERT", "selected": true,  "key": [1] },
                        { "operation": "UPDATE", "selected": true,  "key": [2] },
                        { "operation": "DELETE", "selected": true,  "key": [3] },
                        { "operation": "INSERT", "selected": false, "key": [4] },
                        { "operation": "UNCHANGED", "selected": true, "key": [5] },
                    ],
                },
                {
                    "sourceTable": "s.orders",
                    "targetTable": "t.orders",
                    "rows": null,
                },
            ],
        })
    }

    fn table_result(selected: &[i64]) -> Value {
        let rows: Vec<Value> = [1i64, 2, 3, 4, 5]
            .iter()
            .map(|key| {
                let operation = match key {
                    1 | 4 => "INSERT",
                    2 => "UPDATE",
                    3 => "DELETE",
                    _ => "UNCHANGED",
                };
                json!({ "operation": operation, "selected": selected.contains(key), "key": [key] })
            })
            .collect();
        json!({
            "sourceTable": "s.users",
            "targetTable": "t.users",
            "rows": rows,
        })
    }

    #[test]
    fn value_tokens_are_the_json_array_spelling() {
        assert_eq!(value_token(&[json!(1), json!("a")]), r#"[1,"a"]"#);
        assert_eq!(value_token(&[]), "[]");
    }

    #[test]
    fn the_selected_rows_arm_filters_by_enabled_operations() {
        let rows = vec![
            DataSyncSelectedRowVm {
                source_table: "s.users".into(),
                target_table: "t.users".into(),
                operation: "INSERT".into(),
                key: vec![json!(1)],
            },
            DataSyncSelectedRowVm {
                source_table: "s.users".into(),
                target_table: "t.users".into(),
                operation: "DELETE".into(),
                key: vec![json!(3)],
            },
        ];
        // DEFAULT options: delete is off.
        let selection = selection_from_tables(&plan(), &[], &options(), Some(&rows), None);
        assert_eq!(selection["revision"], "rev-2");
        assert_eq!(selection["rows"].as_array().unwrap().len(), 1);
        assert_eq!(selection["rows"][0]["operation"], "INSERT");
        // No scopes: the key stays absent.
        assert!(selection.get("scopes").is_none());
    }

    #[test]
    fn the_scopes_ride_along_when_supplied() {
        let rows: Vec<DataSyncSelectedRowVm> = vec![];
        let selection = selection_from_tables(
            &plan(),
            &[],
            &options(),
            Some(&rows),
            Some(&json!([{ "table": "s.users" }])),
        );
        assert_eq!(selection["scopes"][0]["table"], "s.users");
        // Empty scopes (an empty array) stay absent — the
        // `activeScopes.length > 0` spread.
        let selection = selection_from_tables(
            &plan(),
            &[],
            &options(),
            Some(&rows),
            Some(&json!([])),
        );
        assert!(selection.get("scopes").is_none());
    }

    #[test]
    fn the_tables_arm_folds_the_requested_set() {
        // The grid selected rows 1 and 3 (insert + delete).
        let selection = selection_from_tables(
            &plan(),
            &[table_result(&[1, 3])],
            &options(),
            None,
            None,
        );
        let rows = selection["rows"].as_array().unwrap();
        // Delete is disabled by the default options: only the insert.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["operation"], "INSERT");
        assert_eq!(rows[0]["sourceTable"], "s.users");
        assert_eq!(rows[0]["targetTable"], "t.users");
        assert_eq!(rows[0]["key"][0], 1);

        // Enabling delete keeps both.
        let mut opts = options();
        opts.delete = true;
        let selection = selection_from_tables(
            &plan(),
            &[table_result(&[1, 3])],
            &opts,
            None,
            None,
        );
        assert_eq!(selection["rows"].as_array().unwrap().len(), 2);

        // A row the grid did not request (row 2, selected in the plan
        // but not in the table result) is not folded.
        let selection = selection_from_tables(
            &plan(),
            &[table_result(&[1])],
            &opts,
            None,
            None,
        );
        let folded: Vec<&str> = selection["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["operation"].as_str().unwrap())
            .collect();
        assert_eq!(folded, vec!["INSERT"]);
    }

    #[test]
    fn the_statements_arm_matches_the_plan_rows() {
        let statements = vec![
            json!({ "table": "t.users", "operation": "UPDATE", "rowKey": [2] }),
        ];
        let selection = selection_from_statements(&plan(), &statements);
        assert_eq!(selection["revision"], "rev-2");
        assert_eq!(selection["rows"].as_array().unwrap().len(), 1);
        assert_eq!(selection["rows"][0]["operation"], "UPDATE");
        assert_eq!(selection["rows"][0]["key"][0], 2);

        // A paged table (rows null) trusts the statement.
        let statements = vec![
            json!({ "table": "t.orders", "operation": "INSERT", "rowKey": [42] }),
        ];
        let selection = selection_from_statements(&plan(), &statements);
        assert_eq!(selection["rows"][0]["sourceTable"], "s.orders");
        assert_eq!(selection["rows"][0]["operation"], "INSERT");
        assert_eq!(selection["rows"][0]["key"][0], 42);

        // A statement with no matching table folds nothing.
        let statements = vec![
            json!({ "table": "t.none", "operation": "INSERT", "rowKey": [1] }),
        ];
        let selection = selection_from_statements(&plan(), &statements);
        assert_eq!(selection["rows"].as_array().unwrap().len(), 0);

        // A statement against a table WITH rows but no matching row
        // folds nothing (the fall-through arm — the plan's rows stay
        // the authority; only a table WITHOUT payloads trusts the
        // statement).
        let statements = vec![
            json!({ "table": "t.users", "operation": "DELETE", "rowKey": [999] }),
        ];
        let selection = selection_from_statements(&plan(), &statements);
        assert_eq!(selection["rows"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn durable_failures_spell_the_four_sentences() {
        let cancelled = json!({ "state": "cancelled" });
        assert_eq!(
            durable_failure(&cancelled, "prepare"),
            "Data Sync prepare job was cancelled."
        );
        let restarted = json!({ "state": "failed", "error": "notDispatchedAfterRestart" });
        assert_eq!(
            durable_failure(&restarted, "apply"),
            "Data Sync job was not executed after restart; start a fresh comparison."
        );
        let unknown_effect = json!({ "state": "failed", "effectOutcome": "unknown" });
        assert_eq!(
            durable_failure(&unknown_effect, "apply"),
            "Data Sync write outcome needs verification; compare current data before continuing."
        );
        let unknown_progress = json!({ "state": "failed", "progress": { "unknown": 3 } });
        assert_eq!(
            durable_failure(&unknown_progress, "prepare"),
            "Data Sync write outcome needs verification; compare current data before continuing."
        );
        let plain = json!({ "state": "failed" });
        assert_eq!(
            durable_failure(&plain, "apply"),
            "Data Sync apply job failed; compare again before continuing."
        );
    }

    #[test]
    fn the_execution_projection_follows_the_counters() {
        // Committed counter wins over the job progress.
        let details = json!({
            "job": { "state": "succeeded", "progress": { "committed": 5 } },
            "domainResults": [
                { "stageId": "apply", "counters": [ { "code": "committed", "value": 12 } ] },
            ],
        });
        let execution = execution_from_job(&details);
        assert_eq!(execution["applied"], 12);
        assert_eq!(execution["affectedRows"], 12);
        assert_eq!(execution["outcome"], "committed");
        assert_eq!(execution["rolledBack"], false);
        assert_eq!(execution["skipped"], 0);
        assert_eq!(execution["conflicts"], json!([]));
        assert!(execution["error"].is_null(), "a succeeded job has no error");

        // The progress fallback when the counter is absent.
        let details = json!({
            "job": { "state": "succeeded", "progress": { "committed": 7 } },
            "domainResults": [],
        });
        assert_eq!(execution_from_job(&details)["applied"], 7);

        // rolledBack carries the error as the rollback reason.
        let details = json!({
            "job": { "state": "failed", "effectOutcome": "rolledBack" },
            "domainResults": [],
        });
        let execution = execution_from_job(&details);
        assert_eq!(execution["outcome"], "rolled_back");
        assert_eq!(execution["rolledBack"], true);
        assert_eq!(
            execution["rollbackReason"],
            "Data Sync apply job failed; compare again before continuing."
        );

        // The unknown counter forces the unknown outcome.
        let details = json!({
            "job": { "state": "failed", "effectOutcome": "none" },
            "domainResults": [
                { "stageId": "apply", "counters": [ { "code": "unknown", "value": 2 } ] },
            ],
        });
        let execution = execution_from_job(&details);
        assert_eq!(execution["outcome"], "unknown");
        assert_eq!(
            execution["error"],
            "Commit outcome is unknown; compare current data before continuing."
        );

        // The restart refusal keeps its sentence.
        let details = json!({
            "job": { "state": "failed", "error": "notDispatchedAfterRestart" },
            "domainResults": [],
        });
        assert_eq!(
            execution_from_job(&details)["error"],
            "Data Sync job was not executed after restart."
        );
    }

    #[test]
    fn the_staging_gates_on_the_missing_plan() {
        let mut staging = SyncPlanStaging::new();
        assert_eq!(
            staging.plan_id(),
            Err("data sync comparison plan is missing; compare again")
        );
        assert_eq!(staging.page_size(), COMPARISON_PAGE_FALLBACK_SIZE);
        staging.stage_plan(plan());
        assert_eq!(staging.plan_id(), Ok("plan-1".to_owned()));
        assert_eq!(staging.page_size(), 40);
        staging.clear_plan();
        assert!(staging.plan_id().is_err());
    }

    #[test]
    fn the_compare_chain_polls_until_terminal() {
        let mut chain = SyncCompareChain::new();
        let (method, _) = chain.begin_compare(
            "data-sync-uuid-1",
            start_data_sync_prepare_job_request(
                "s1", "t1", None, "data-sync-uuid-1", None, None, None, None, None, None,
            ),
        );
        assert_eq!(method, SyncMethod::StartDataSyncPrepareJob);

        chain.note_prepare_accepted(&json!({ "jobId": "job-1" }));
        // A running job answers the next poll.
        let poll = chain.note_job(&json!({ "state": "running" }), "prepare");
        assert!(poll.is_some());
        let (method, payload) = poll.unwrap();
        assert_eq!(method, SyncMethod::GetDataSyncJob);
        assert_eq!(payload["jobId"], "job-1");
        // A succeeded job moves to the preview read.
        assert!(chain
            .note_job(&json!({ "state": "succeeded" }), "prepare")
            .is_none());
        chain.note_preview(plan());
        assert_eq!(chain.staging.plan_id(), Ok("plan-1".to_owned()));
        assert_eq!(chain.step, Some(SyncCompareStep::Done));
        assert!(chain.failure().is_none());
    }

    #[test]
    fn a_failed_prepare_ends_the_chain_with_the_durable_failure() {
        let mut chain = SyncCompareChain::new();
        chain.begin_compare(
            "j",
            start_data_sync_prepare_job_request("s1", "t1", None, "j", None, None, None, None, None, None),
        );
        chain.note_prepare_accepted(&json!({ "jobId": "j" }));
        assert!(chain
            .note_job(&json!({ "state": "cancelled" }), "prepare")
            .is_none());
        assert_eq!(
            chain.failure(),
            Some("Data Sync prepare job was cancelled.")
        );
        // The staged plan stays absent (the chain failed before the
        // preview).
        assert!(chain.staging.plan_id().is_err());
    }
}
