//! Port of `packages/frontend/src/lib/session/runEditorExplain.ts`.
//!
//! The React function is an async orchestration: resolve the pane controller,
//! execute one `explain` command, wrap the receipt in an `ExecutionProjection`,
//! recover until a terminal view, refresh, then read `planText` from a
//! `commandResult` chunk (and dispose in `finally`). This module keeps that
//! flow as a pure immediate-mode machine: it emits [`ExplainEffect`]s and the
//! host feeds answers back through [`ExplainRun::step`]. No promise, timeout,
//! thread or re-submission of SQL — a lost reply only re-emits `Recover`.
//!
//! # Reuse / stand-ins
//!
//! DTOs come from T083/T122 (`CommandCall`, `ExecutionReceipt`, `ExecutionView`,
//! `PublishedResultChunk`, `SessionError`, `OwnerRef`, `ExecutionTarget`,
//! `NamespaceTarget`) and the terminal check uses T122's `is_terminal`. No DTO
//! is duplicated. Two small local pieces remain, each with a removal stage:
//!
//! * `editor_owner_for_pane` (T208) and `queryPanelExecutionTarget` (T208) are
//!   not on this branch yet, so `start` builds the target/owner locally with
//!   the same normalized namespace and pane-key owner. Remove when T208 lands
//!   and call those helpers instead.
//! * the lowercase string views of `ExecutionState` / `ExecutionErrorCode` /
//!   `EffectOutcome` are local because the DTO enums have no `as_str`. Remove
//!   when those enums expose it.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the explain view wires this machine".

use crate::session::editor_session_controller::{
    CommandCall, ExecutionReceipt, ExecutionState, ExecutionTarget, NamespaceTarget, OwnerRef,
    SessionError, SessionHandle,
};
use crate::session::execution_projection::{
    is_terminal, EffectOutcome, ExecutionErrorCode, ExecutionView, PublishedResultChunk,
};

/// A lost artifact read is retried up to 20 times, then surfaced. React's
/// `++failedReads > 20` means the 21st failure throws.
const MAX_FAILED_READS: u32 = 20;

/// Inputs to one explain run (React's positional arguments). `params` keeps
/// insertion order so the emitted input envelope is deterministic.
#[allow(dead_code)] // consumer: the explain view; remove when it lands.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplainRequest {
    pub key: String,
    pub connection_id: String,
    pub sql: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub params: Vec<(String, serde_json::Value)>,
}

/// Work the host performs to drive an explain run.
#[allow(dead_code)] // emitted work; remove with the explain view.
#[derive(Clone, Debug, PartialEq)]
pub enum ExplainEffect {
    /// `registry.get(key, target, ownerForPane(key))`.
    GetController {
        key: String,
        target: ExecutionTarget,
        owner: OwnerRef,
    },
    /// `controller.execute` with the explain envelope.
    Execute { call: CommandCall },
    /// Wrap the accepted receipt in an `ExecutionProjection`.
    CreateProjection {
        receipt: ExecutionReceipt,
        handle: Option<SessionHandle>,
    },
    /// `projection.recover()` — never re-submits SQL.
    Recover,
    /// `controller.refresh().catch(() => undefined)` after a terminal view.
    Refresh,
    /// `projection.dispose()` in `finally`.
    Dispose,
}

/// Host answers fed back into [`ExplainRun::step`].
#[allow(dead_code)] // result feed; remove with the explain view.
#[derive(Clone, Debug, PartialEq)]
pub enum ExplainInput {
    ControllerReady,
    Executed(Result<ExecutionReceipt, SessionError>),
    Projected,
    Recovered {
        view: Option<ExecutionView>,
        chunks: Vec<PublishedResultChunk>,
    },
    RecoverFailed,
    Refreshed,
}

/// React's thrown failure, kept as a typed value (no panics).
#[allow(dead_code)] // failure mirror; remove with the explain view.
#[derive(Clone, Debug, PartialEq)]
pub enum ExplainFailure {
    Session(SessionError),
    RecoverExhausted,
    Execution { message: String },
    ResultUnavailable,
}

/// The successful React return is an `ExplainResult` with `planText`.
#[allow(dead_code)] // result mirror; remove with the explain view.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplainPlan {
    pub plan_text: String,
}

#[allow(dead_code)] // result mirror; remove with the explain view.
#[derive(Clone, Debug, PartialEq)]
pub enum ExplainOutcome {
    Plan(ExplainPlan),
    Failed(ExplainFailure),
}

#[allow(dead_code)] // step result; remove with the explain view.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplainStep {
    pub effects: Vec<ExplainEffect>,
    pub outcome: Option<ExplainOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExplainPhase {
    AwaitController,
    AwaitExecute,
    AwaitProjection,
    AwaitRecover,
    AwaitRefresh,
    Done,
}

/// The explain flow as an explicit machine.
#[allow(dead_code)] // consumer: the explain view; remove when it lands.
pub struct ExplainRun {
    request: ExplainRequest,
    phase: ExplainPhase,
    failed_reads: u32,
    view: Option<ExecutionView>,
    chunks: Vec<PublishedResultChunk>,
}

/// Start an explain run: the first effect resolves the pane controller.
#[allow(dead_code)] // consumer: the explain view; remove when it lands.
pub fn start(request: ExplainRequest) -> (ExplainRun, ExplainStep) {
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
    // Local stand-in for T208's `editorOwnerForPane(key)`; the client id is not
    // part of React's mocked owner here. Remove when T208 lands.
    let owner = OwnerRef::Editor {
        client_instance_id: String::new(),
        editor_session_id: request.key.clone(),
    };
    let run = ExplainRun {
        request,
        phase: ExplainPhase::AwaitController,
        failed_reads: 0,
        view: None,
        chunks: Vec::new(),
    };
    let step = ExplainStep {
        effects: vec![ExplainEffect::GetController {
            key: run.request.key.clone(),
            target,
            owner,
        }],
        outcome: None,
    };
    (run, step)
}

#[allow(dead_code)] // consumer: the explain view; remove when it lands.
impl ExplainRun {
    pub fn phase(&self) -> &'static str {
        match self.phase {
            ExplainPhase::AwaitController => "awaitController",
            ExplainPhase::AwaitExecute => "awaitExecute",
            ExplainPhase::AwaitProjection => "awaitProjection",
            ExplainPhase::AwaitRecover => "awaitRecover",
            ExplainPhase::AwaitRefresh => "awaitRefresh",
            ExplainPhase::Done => "done",
        }
    }

    /// Advance with one host answer. Out-of-phase inputs cannot move the run.
    pub fn step(&mut self, input: ExplainInput) -> ExplainStep {
        match (self.phase, input) {
            (ExplainPhase::AwaitController, ExplainInput::ControllerReady) => {
                self.phase = ExplainPhase::AwaitExecute;
                ExplainStep {
                    effects: vec![ExplainEffect::Execute {
                        call: explain_command_call(
                            &self.request.sql,
                            &self.request.params,
                            self.request.database.as_deref(),
                            self.request.schema.as_deref(),
                        ),
                    }],
                    outcome: None,
                }
            }
            (ExplainPhase::AwaitExecute, ExplainInput::Executed(Ok(receipt))) => {
                self.phase = ExplainPhase::AwaitProjection;
                ExplainStep {
                    effects: vec![ExplainEffect::CreateProjection {
                        receipt,
                        handle: None,
                    }],
                    outcome: None,
                }
            }
            (ExplainPhase::AwaitExecute, ExplainInput::Executed(Err(error))) => {
                // The execute await happens before the projection is created
                // and outside the `try/finally dispose` in React, so no
                // projection exists yet — do not emit Dispose here.
                self.failed(ExplainFailure::Session(error), false)
            }
            (ExplainPhase::AwaitProjection, ExplainInput::Projected) => {
                self.phase = ExplainPhase::AwaitRecover;
                self.recover()
            }
            (ExplainPhase::AwaitRecover, ExplainInput::Recovered { view, chunks }) => {
                self.view = view;
                self.chunks = chunks;
                self.failed_reads = 0;
                if self
                    .view
                    .as_ref()
                    .is_some_and(|view| is_terminal(view.state))
                {
                    self.phase = ExplainPhase::AwaitRefresh;
                    ExplainStep {
                        effects: vec![ExplainEffect::Refresh],
                        outcome: None,
                    }
                } else {
                    self.recover()
                }
            }
            (ExplainPhase::AwaitRecover, ExplainInput::RecoverFailed) => {
                self.failed_reads += 1;
                if self.failed_reads > MAX_FAILED_READS {
                    self.failed(ExplainFailure::RecoverExhausted, true)
                } else {
                    // 250ms backoff is the host's concern; never re-submit SQL.
                    self.recover()
                }
            }
            (ExplainPhase::AwaitRefresh, ExplainInput::Refreshed) => self.settle(),
            _ => ExplainStep {
                effects: Vec::new(),
                outcome: None,
            },
        }
    }

    fn recover(&self) -> ExplainStep {
        ExplainStep {
            effects: vec![ExplainEffect::Recover],
            outcome: None,
        }
    }

    /// After the post-terminal refresh, preserve the backend's verdict; only a
    /// succeeded run scans chunks for the plan.
    fn settle(&mut self) -> ExplainStep {
        let Some(view) = self.view.clone() else {
            return self.failed(ExplainFailure::ResultUnavailable, true);
        };
        if view.state != ExecutionState::Succeeded {
            return self.failed(
                ExplainFailure::Execution {
                    message: explain_failure_message(&view),
                },
                true,
            );
        }
        match explain_plan_text(&self.chunks) {
            Some(plan_text) => self.finish(ExplainOutcome::Plan(ExplainPlan { plan_text })),
            None => self.failed(ExplainFailure::ResultUnavailable, true),
        }
    }

    fn finish(&mut self, outcome: ExplainOutcome) -> ExplainStep {
        self.phase = ExplainPhase::Done;
        ExplainStep {
            effects: vec![ExplainEffect::Dispose],
            outcome: Some(outcome),
        }
    }

    fn failed(&mut self, error: ExplainFailure, dispose: bool) -> ExplainStep {
        self.phase = ExplainPhase::Done;
        ExplainStep {
            effects: if dispose {
                vec![ExplainEffect::Dispose]
            } else {
                Vec::new()
            },
            outcome: Some(ExplainOutcome::Failed(error)),
        }
    }
}

/// React's `explain` envelope: bind params only when non-empty, database/schema
/// only when truthy. Keys are emitted in React's literal order.
#[allow(dead_code)] // envelope helper; remove with the explain view.
pub fn explain_command_call(
    sql: &str,
    params: &[(String, serde_json::Value)],
    database: Option<&str>,
    schema: Option<&str>,
) -> CommandCall {
    let mut fields = vec![format!("\"sql\":{}", json_string(sql))];
    if !params.is_empty() {
        let body: Vec<String> = params
            .iter()
            .map(|(key, value)| format!("{}:{}", json_string(key), value))
            .collect();
        fields.push(format!("\"params\":{{{}}}", body.join(",")));
    }
    if let Some(database) = database.filter(|value| !value.is_empty()) {
        fields.push(format!("\"database\":{}", json_string(database)));
    }
    if let Some(schema) = schema.filter(|value| !value.is_empty()) {
        fields.push(format!("\"schema\":{}", json_string(schema)));
    }
    CommandCall {
        command: "explain".to_owned(),
        input: format!("{{{}}}", fields.join(",")),
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

/// Scan chunks for a `commandResult` output carrying a string `planText`.
#[allow(dead_code)] // plan helper; remove with the explain view.
pub fn explain_plan_text(chunks: &[PublishedResultChunk]) -> Option<String> {
    for chunk in chunks {
        let Some(output) = chunk.payload.get("output") else {
            continue;
        };
        if output.get("kind").and_then(|kind| kind.as_str()) != Some("commandResult") {
            continue;
        }
        if let Some(plan) = output
            .get("data")
            .and_then(|data| data.get("planText"))
            .and_then(|plan| plan.as_str())
        {
            return Some(plan.to_owned());
        }
    }
    None
}

/// React's `${state}: ${errorCode}; effect: ${effectOutcome}`.
#[allow(dead_code)] // error helper; remove with the explain view.
pub fn explain_failure_message(view: &ExecutionView) -> String {
    format!(
        "{}: {}; effect: {}",
        execution_state_str(view.state),
        error_code_str(view.error_code),
        effect_outcome_str(view.effect_outcome)
    )
}

// Local lowercase string views: the DTO enums have no `as_str`.
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

fn error_code_str(code: Option<ExecutionErrorCode>) -> &'static str {
    match code {
        Some(ExecutionErrorCode::SqlError) => "sqlError",
        Some(ExecutionErrorCode::ProtocolError) => "protocolError",
        Some(ExecutionErrorCode::Cancelled) => "cancelled",
        Some(ExecutionErrorCode::Timeout) => "timeout",
        Some(ExecutionErrorCode::ResourceLost) => "resourceLost",
        Some(ExecutionErrorCode::PipelineAborted) => "pipelineAborted",
        Some(ExecutionErrorCode::HostRejected) => "hostRejected",
        None => "undefined",
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
