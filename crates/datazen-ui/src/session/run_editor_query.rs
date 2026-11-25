//! Port of `packages/frontend/src/lib/session/runEditorQuery.ts`.
//!
//! React resolves the pane controller, executes one `query` command, streams
//! `QueryStreamEvent`s while an `ExecutionProjection` recovers, publishes
//! authoritative artifact references on any terminal view, refreshes the
//! controller, and either raises a diagnostic or emits `done`. This module is
//! the pure immediate-mode machine for that flow: it emits [`QueryEffect`]s and
//! [`QueryEvent`]s; the host performs the work and feeds answers back through
//! [`QueryRun::step`]. No promise, timeout, thread or SQL re-submission.
//!
//! # Reuse / stand-ins
//!
//! DTOs come from T083/T122 (`CommandCall`, `ExecutionReceipt`, `ExecutionView`,
//! `PublishedResultChunk`, `StatementResultSource`, `RuntimeResultBinding`,
//! `ResultCompleteness`, `SessionError`, `OwnerRef`, `ExecutionTarget`) and the
//! terminal check uses T122's `is_terminal`. No DTO is duplicated. Local VMs
//! (`StatementSnapshot`, `ColumnMeta`, `ArtifactRefs`, `QueryEvent`) mirror the
//! React `StatementResult`/`QueryStreamEvent` shapes this flow constructs;
//! removal stage: when the query-result view model lands in a shared module,
//! replace these with it. The target/owner are built locally with the same
//! normalized namespace and pane-key owner as T208; removal stage: call T208's
//! `query_panel_execution_target` / `editor_owner_for_pane` directly. The
//! elapsed `done` time is supplied by the host (no wall-clock in a pure port);
//! removal stage: pass it in when the view drives the machine.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the query view wires this machine".

use serde_json::Value;

use crate::session::editor_session_controller::{
    CommandCall, ExecutionReceipt, ExecutionState, ExecutionTarget, NamespaceTarget, OwnerRef,
    SessionError, SessionHandle,
};
use crate::session::execution_projection::{
    is_terminal, EffectOutcome, ExecutionErrorCode, ExecutionView, PublishedResultChunk,
    ResultCompleteness, RuntimeResultBinding, StatementResultSource, WritableMapping,
};

/// A lost artifact read is retried 20 times, then surfaced.
const MAX_DISCONNECTED_READS: u32 = 20;

/// The two settings the query envelope reads.
#[allow(dead_code)] // consumer: the query view; remove when it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuerySettings {
    pub limit_select_results: bool,
    pub query_result_limit: i64,
}

#[allow(dead_code)] // consumer: the query view; remove when it lands.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryRequest {
    pub key: String,
    pub connection_id: String,
    pub sql: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub params: Vec<(String, Value)>,
    pub settings: QuerySettings,
}

/// A shaped column (React's `ColumnInfo` guard).
#[allow(dead_code)] // local VM; remove with the shared query-result VM.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

/// React `StatementResult` as assembled by this flow.
#[allow(dead_code)] // local VM; remove with the shared query-result VM.
#[derive(Clone, Debug, PartialEq)]
pub struct StatementSnapshot {
    pub sql: String,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Value>>,
    pub rows_affected: Option<i64>,
    pub truncated: bool,
    pub source: Option<StatementResultSource>,
    pub runtime_binding: Option<RuntimeResultBinding>,
}

/// The authoritative artifact references published on any terminal view.
#[allow(dead_code)] // local VM; remove with the shared query-result VM.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactRefs {
    pub execution_id: String,
    pub artifact_ids: Vec<String>,
    pub result_completeness: ResultCompleteness,
}

/// React `QueryStreamEvent` variants this flow emits.
#[allow(dead_code)] // local VM; remove with the shared query-result VM.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryEvent {
    ExecutionStarted { execution_id: String },
    StatementSnapshot { index: u64, result: StatementSnapshot },
    PublishedArtifacts { artifacts: ArtifactRefs },
    Done { total_time_ms: i64 },
}

/// Work the host performs to drive a query run.
#[allow(dead_code)] // emitted work; remove with the query view.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryEffect {
    GetController {
        key: String,
        target: ExecutionTarget,
        owner: OwnerRef,
    },
    Execute { call: CommandCall },
    CreateProjection {
        receipt: ExecutionReceipt,
        handle: Option<SessionHandle>,
    },
    Recover,
    Refresh,
    Dispose,
}

/// Host answers fed back into [`QueryRun::step`].
#[allow(dead_code)] // result feed; remove with the query view.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryInput {
    ControllerReady,
    Executed(Result<ExecutionReceipt, SessionError>),
    Projected,
    Recovered {
        view: Option<ExecutionView>,
        chunks: Vec<PublishedResultChunk>,
        writable_binding: Option<RuntimeResultBinding>,
    },
    RecoverFailed,
    Refreshed {
        total_time_ms: i64,
    },
}

#[allow(dead_code)] // failure mirror; remove with the query view.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryFailure {
    Session(SessionError),
    DisconnectExhausted,
    Execution { message: String },
}

#[allow(dead_code)] // result mirror; remove with the query view.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryOutcome {
    Completed,
    Failed(QueryFailure),
}

#[allow(dead_code)] // step result; remove with the query view.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryStep {
    pub effects: Vec<QueryEffect>,
    pub events: Vec<QueryEvent>,
    pub outcome: Option<QueryOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryPhase {
    AwaitController,
    AwaitExecute,
    AwaitProjection,
    AwaitRecover,
    AwaitRefresh,
    Done,
}

/// The query flow as an explicit machine.
#[allow(dead_code)] // consumer: the query view; remove when it lands.
pub struct QueryRun {
    request: QueryRequest,
    phase: QueryPhase,
    disconnected_reads: u32,
    view: Option<ExecutionView>,
    chunks: Vec<PublishedResultChunk>,
    signatures: Vec<(u64, String)>,
}

/// Start a query run: the first effect resolves the pane controller.
#[allow(dead_code)] // consumer: the query view; remove when it lands.
pub fn start(request: QueryRequest) -> (QueryRun, QueryStep) {
    let target = ExecutionTarget {
        connection_id: request.connection_id.clone(),
        namespace: NamespaceTarget {
            database: request.database.clone(),
            catalog: None,
            schema: request.schema.clone(),
            path: Vec::new(),
        },
        object: None,
    };
    let owner = OwnerRef::Editor {
        client_instance_id: String::new(),
        editor_session_id: request.key.clone(),
    };
    let run = QueryRun {
        request,
        phase: QueryPhase::AwaitController,
        disconnected_reads: 0,
        view: None,
        chunks: Vec::new(),
        signatures: Vec::new(),
    };
    let step = QueryStep {
        effects: vec![QueryEffect::GetController {
            key: run.request.key.clone(),
            target,
            owner,
        }],
        events: Vec::new(),
        outcome: None,
    };
    (run, step)
}

#[allow(dead_code)] // consumer: the query view; remove when it lands.
impl QueryRun {
    pub fn phase(&self) -> &'static str {
        match self.phase {
            QueryPhase::AwaitController => "awaitController",
            QueryPhase::AwaitExecute => "awaitExecute",
            QueryPhase::AwaitProjection => "awaitProjection",
            QueryPhase::AwaitRecover => "awaitRecover",
            QueryPhase::AwaitRefresh => "awaitRefresh",
            QueryPhase::Done => "done",
        }
    }

    pub fn step(&mut self, input: QueryInput) -> QueryStep {
        match (self.phase, input) {
            (QueryPhase::AwaitController, QueryInput::ControllerReady) => {
                self.phase = QueryPhase::AwaitExecute;
                QueryStep {
                    effects: vec![QueryEffect::Execute {
                        call: query_command_call(&self.request),
                    }],
                    events: Vec::new(),
                    outcome: None,
                }
            }
            (QueryPhase::AwaitExecute, QueryInput::Executed(Ok(receipt))) => {
                self.phase = QueryPhase::AwaitProjection;
                QueryStep {
                    effects: vec![QueryEffect::CreateProjection {
                        receipt: receipt.clone(),
                        handle: None,
                    }],
                    events: vec![QueryEvent::ExecutionStarted {
                        execution_id: receipt.execution_id,
                    }],
                    outcome: None,
                }
            }
            (QueryPhase::AwaitExecute, QueryInput::Executed(Err(error))) => {
                // No projection exists yet: do not dispose (React parity).
                self.failed(QueryFailure::Session(error), false)
            }
            (QueryPhase::AwaitProjection, QueryInput::Projected) => {
                self.phase = QueryPhase::AwaitRecover;
                self.recover_step()
            }
            (
                QueryPhase::AwaitRecover,
                QueryInput::Recovered {
                    view,
                    chunks,
                    writable_binding,
                },
            ) => {
                self.view = view;
                self.chunks = chunks;
                self.disconnected_reads = 0;
                let mut step = self.publish_snapshots(&writable_binding);
                if self
                    .view
                    .as_ref()
                    .is_some_and(|view| is_terminal(view.state))
                {
                    if let Some(view) = self.view.clone() {
                        step.events.push(QueryEvent::PublishedArtifacts {
                            artifacts: ArtifactRefs {
                                execution_id: view.execution_id,
                                artifact_ids: view.artifact_ids,
                                result_completeness: view.result_completeness,
                            },
                        });
                    }
                    self.phase = QueryPhase::AwaitRefresh;
                    step.effects.push(QueryEffect::Refresh);
                } else {
                    step.effects.push(QueryEffect::Recover);
                }
                step
            }
            (QueryPhase::AwaitRecover, QueryInput::RecoverFailed) => {
                self.disconnected_reads += 1;
                if self.disconnected_reads > MAX_DISCONNECTED_READS {
                    self.failed(QueryFailure::DisconnectExhausted, true)
                } else {
                    self.recover_step()
                }
            }
            (
                QueryPhase::AwaitRefresh,
                QueryInput::Refreshed { total_time_ms },
            ) => self.settle(total_time_ms),
            _ => QueryStep {
                effects: Vec::new(),
                events: Vec::new(),
                outcome: None,
            },
        }
    }

    fn recover_step(&self) -> QueryStep {
        QueryStep {
            effects: vec![QueryEffect::Recover],
            events: Vec::new(),
            outcome: None,
        }
    }

    /// Build `statementSnapshot` events for changed results, in index order.
    fn publish_snapshots(&mut self, writable_binding: &Option<RuntimeResultBinding>) -> QueryStep {
        let provenance = self
            .view
            .as_ref()
            .and_then(|view| view.provenance.clone());
        let snapshots = statement_snapshots(&self.chunks, provenance.is_some(), writable_binding);
        let mut events = Vec::new();
        for (index, result) in snapshots {
            // A structural Debug snapshot is React's `JSON.stringify(result)`:
            // stable for the same published values, and no serde derive on the
            // local VM is needed.
            let signature = format!("{result:?}");
            if self
                .signatures
                .iter()
                .any(|(saved_index, saved)| *saved_index == index && saved == &signature)
            {
                continue;
            }
            self.signatures.push((index, signature));
            events.push(QueryEvent::StatementSnapshot { index, result });
        }
        QueryStep {
            effects: Vec::new(),
            events,
            outcome: None,
        }
    }

    fn settle(&mut self, total_time_ms: i64) -> QueryStep {
        let Some(view) = self.view.clone() else {
            return self.failed(
                QueryFailure::Execution {
                    message: "Execution outcome unavailable".to_owned(),
                },
                true,
            );
        };
        if view.state != ExecutionState::Succeeded {
            let message = execution_error_message(&view, &self.chunks);
            return self.failed(QueryFailure::Execution { message }, true);
        }
        self.phase = QueryPhase::Done;
        QueryStep {
            effects: vec![QueryEffect::Dispose],
            events: vec![QueryEvent::Done { total_time_ms }],
            outcome: Some(QueryOutcome::Completed),
        }
    }

    fn failed(&mut self, error: QueryFailure, dispose: bool) -> QueryStep {
        self.phase = QueryPhase::Done;
        QueryStep {
            effects: if dispose {
                vec![QueryEffect::Dispose]
            } else {
                Vec::new()
            },
            events: Vec::new(),
            outcome: Some(QueryOutcome::Failed(error)),
        }
    }
}

/// React's `query` envelope, in literal key order.
#[allow(dead_code)] // envelope helper; remove with the query view.
pub fn query_command_call(request: &QueryRequest) -> CommandCall {
    let mut fields = vec![
        format!("\"sql\":{}", json_string(&request.sql)),
        "\"recordHistory\":true".to_owned(),
    ];
    if !request.params.is_empty() {
        let body: Vec<String> = request
            .params
            .iter()
            .map(|(key, value)| format!("{}:{}", json_string(key), value))
            .collect();
        fields.push(format!("\"params\":{{{}}}", body.join(",")));
    }
    if request.settings.limit_select_results && request.settings.query_result_limit > 0 {
        fields.push(format!(
            "\"limit\":{}",
            request.settings.query_result_limit
        ));
    }
    if let Some(database) = request.database.as_deref().filter(|v| !v.is_empty()) {
        fields.push(format!("\"database\":{}", json_string(database)));
    }
    if let Some(schema) = request.schema.as_deref().filter(|v| !v.is_empty()) {
        fields.push(format!("\"schema\":{}", json_string(schema)));
    }
    CommandCall {
        command: "query".to_owned(),
        input: format!("{{{}}}", fields.join(",")),
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

fn record(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value.as_object()
}

fn column_meta(value: &Value) -> Option<ColumnMeta> {
    let value = record(value)?;
    Some(ColumnMeta {
        name: value.get("name")?.as_str()?.to_owned(),
        data_type: value.get("dataType")?.as_str()?.to_owned(),
        nullable: value.get("nullable")?.as_bool()?,
    })
}

/// Group chunk payloads into statement snapshots (React's inner loop).
#[allow(dead_code)] // projection helper; remove with the query view.
pub fn statement_snapshots(
    chunks: &[PublishedResultChunk],
    _has_provenance: bool,
    writable_binding: &Option<RuntimeResultBinding>,
) -> Vec<(u64, StatementSnapshot)> {
    let mut statements: Vec<(u64, StatementSnapshot)> = Vec::new();
    for chunk in chunks {
        let envelope = record(&chunk.payload);
        let Some(envelope) = envelope else { continue };
        let Some(output) = envelope.get("output").and_then(record) else {
            continue;
        };
        let Some(index) = output.get("statementIndex").and_then(|v| v.as_u64()) else {
            continue;
        };
        let position = statements
            .iter()
            .position(|(saved_index, _)| *saved_index == index);
        if position.is_none() {
            statements.push((
                index,
                StatementSnapshot {
                    sql: String::new(),
                    columns: Vec::new(),
                    rows: Vec::new(),
                    rows_affected: None,
                    truncated: false,
                    source: None,
                    runtime_binding: None,
                },
            ));
        }
        let statement = statements
            .iter_mut()
            .find(|(saved_index, _)| *saved_index == index)
            .map(|(_, statement)| statement);
        let Some(statement) = statement else { continue };
        let kind = output.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if chunk.source.is_some()
            && (kind == "statementMetadata" || statement.source.is_none())
        {
            statement.source = chunk.source.clone();
        }
        if kind == "rows" {
            if let Some(sql) = output.get("sql").and_then(|v| v.as_str()) {
                statement.sql = sql.to_owned();
            }
            let early = output
                .get("columns")
                .filter(|v| v.is_array())
                .or_else(|| envelope.get("columns").filter(|v| v.is_array()));
            if statement.columns.is_empty() {
                if let Some(columns) = early.and_then(|v| v.as_array()) {
                    statement.columns = columns.iter().filter_map(column_meta).collect();
                }
            }
            if let Some(rows) = output.get("rows").and_then(|v| v.as_array()) {
                if rows.iter().all(|row| row.is_array()) {
                    for row in rows {
                        if let Some(values) = row.as_array() {
                            statement.rows.push(values.clone());
                        }
                    }
                }
            }
        } else if kind == "statementSummary" {
            if let Some(sql) = output.get("sql").and_then(|v| v.as_str()) {
                statement.sql = sql.to_owned();
            }
        } else if kind == "statementMetadata" || kind == "statementColumns" {
            if let Some(columns) = output.get("columns").and_then(|v| v.as_array()) {
                statement.columns = columns.iter().filter_map(column_meta).collect();
            }
            statement.truncated = output.get("truncated").and_then(|v| v.as_bool()) == Some(true);
        }
        if let Some(rows_affected) = output.get("rowsAffected").and_then(|v| v.as_i64()) {
            statement.rows_affected = Some(rows_affected);
        }
        statement.runtime_binding = match statement.source.as_ref() {
            Some(source) if source.writable_mapping == WritableMapping::Verified => {
                writable_binding.clone()
            }
            _ => None,
        };
    }
    statements.sort_by(|(a, _), (b, _)| a.cmp(b));
    statements
}

/// React's failure diagnostic: an `executionError` chunk message, else the
/// view's error code, else the fallback sentence.
#[allow(dead_code)] // error helper; remove with the query view.
pub fn execution_error_message(view: &ExecutionView, chunks: &[PublishedResultChunk]) -> String {
    let diagnostic = chunks.iter().find_map(|chunk| {
        let output = record(&chunk.payload)?.get("output")?;
        let output = record(output)?;
        if output.get("kind")?.as_str()? != "executionError" {
            return None;
        }
        output.get("message")?.as_str().map(str::to_owned)
    });
    // React: `diagnostic?.message ?? view.errorCode ??
    // 'Execution outcome unavailable'`. An absent code is not "undefined".
    let code = diagnostic.unwrap_or_else(|| match view.error_code {
        Some(code) => error_code_str(code).to_owned(),
        None => "Execution outcome unavailable".to_owned(),
    });
    format!(
        "{}: {}; effect: {}",
        execution_state_str(view.state),
        code,
        effect_outcome_str(view.effect_outcome)
    )
}

// Local string views: the DTO enums have no `as_str`.
// Remove when those enums expose it.
fn execution_state_str(state: ExecutionState) -> &'static str {
    match state {
        ExecutionState::Queued => "queued",
        ExecutionState::Running => "running",
        ExecutionState::CancelRequested => "cancelRequested",
        ExecutionState::Succeeded => "succeeded",
        ExecutionState::Failed => "failed",
        ExecutionState::Cancelled => "cancelled",
    }
}

fn error_code_str(code: ExecutionErrorCode) -> &'static str {
    match code {
        ExecutionErrorCode::SqlError => "sqlError",
        ExecutionErrorCode::ProtocolError => "protocolError",
        ExecutionErrorCode::Cancelled => "cancelled",
        ExecutionErrorCode::Timeout => "timeout",
        ExecutionErrorCode::ResourceLost => "resourceLost",
        ExecutionErrorCode::PipelineAborted => "pipelineAborted",
        ExecutionErrorCode::HostRejected => "hostRejected",
    }
}

fn effect_outcome_str(outcome: EffectOutcome) -> &'static str {
    match outcome {
        EffectOutcome::NotStarted => "notStarted",
        EffectOutcome::Completed => "completed",
        EffectOutcome::RolledBack => "rolledBack",
        EffectOutcome::PartiallyApplied => "partiallyApplied",
        EffectOutcome::Unknown => "unknown",
    }
}
