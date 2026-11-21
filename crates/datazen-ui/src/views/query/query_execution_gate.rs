//! Pure port of the `useQueryExecutionGate` hook's LOGIC (not the React shell).
//!
//! React owns an async pipeline: resolve the target SQL → freeze a snapshot →
//! unclosed-transaction check → bind-parameter validation → risk assessment →
//! hard blocks → confirmation → submit. The Rune port turns every step into a
//! pure decision returned to an injected host; no command, promise, thread or
//! timer runs here. The one timer in the hook (50ms bind-param focus) becomes
//! a deadline recorded in state, which the host wakes at.
//!
//! ## Removal stage
//!
//! No production host wires this gate yet. The query-panel host will own the
//! state, execute [`GateAction`]s and feed confirm results back as [`GateEvent`]s;
//! until then every reusable item carries a per-item `#[allow(dead_code)]` with
//! this removal stage.

use std::collections::BTreeMap;

mod confirm;
mod risk;
mod sql;

use crate::views::query::query_editor_section::ExecutionStrategy;
use confirm::confirm_spec;
use risk::{assess, RiskMode};
use sql::{
    missing_params, statement_ranges, substitute_params, suspicious_pg_double_quote,
    transaction_analysis,
};

const PRODUCTION_GROUP: &str = "preset:production";

/// React `ExecuteKind`.
#[allow(dead_code)] // VM mirror; consumed by decisions and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecuteKind {
    Full,
    Selection,
}

/// Which store command the host runs after the gate passes (React's
/// `storeExecuteQuery` vs `storeExecuteSelection` branch in `runExecute`).
#[allow(dead_code)] // host-facing decision; consumed by the reducer and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionMode {
    Query,
    Selection,
}

/// React `ExecutionSnapshot`, frozen before any async step.
#[allow(dead_code)] // VM surface; supplied by the host and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutionSnapshot {
    pub(crate) panel_id: String,
    pub(crate) db_session_id: String,
    pub(crate) sql: String,
    pub(crate) read_only: bool,
    pub(crate) safe_mode: bool,
    pub(crate) is_production: bool,
    pub(crate) confirm_dangerous: bool,
}

/// React `PendingExecute` for the unclosed-transaction dialog.
#[allow(dead_code)] // VM surface; consumed by the reducer and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingExecute {
    pub(crate) kind: ExecuteKind,
    pub(crate) sql: Option<String>,
}

/// React `ResolveExecutionTargetResult` fields the gate consumes.
#[allow(dead_code)] // VM surface; consumed by the reducer and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AskTarget {
    pub(crate) current_statement: Option<String>,
    pub(crate) statement_count: usize,
    pub(crate) entire_script: String,
}

/// Inputs to one `requestExecute` call, all host-owned.
#[allow(dead_code)] // VM surface; supplied by the host and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GateInput {
    pub(crate) kind: ExecuteKind,
    pub(crate) selection_sql: Option<String>,
    pub(crate) cursor_offset: Option<usize>,
    pub(crate) strategy: ExecutionStrategy,
    pub(crate) values: BTreeMap<String, String>,
    pub(crate) confirm_writes: bool,
    pub(crate) database_type: String,
    pub(crate) snapshot: ExecutionSnapshot,
}

impl GateInput {
    /// A full execution of `SELECT 1` with the test fixture's defaults.
    #[allow(dead_code)] // fixture helper; consumed by the tests.
    pub(crate) fn full(sql: &str) -> Self {
        Self {
            kind: ExecuteKind::Full,
            selection_sql: None,
            cursor_offset: None,
            strategy: ExecutionStrategy::CurrentStatement,
            values: BTreeMap::new(),
            confirm_writes: false,
            database_type: "sqlite".to_owned(),
            snapshot: ExecutionSnapshot {
                panel_id: "p1".to_owned(),
                db_session_id: "sess-1".to_owned(),
                sql: sql.to_owned(),
                read_only: false,
                safe_mode: true,
                is_production: false,
                confirm_dangerous: true,
            },
        }
    }
}

/// Live context used by React's `isSnapshotStale` after a confirm returns.
#[allow(dead_code)] // VM surface; supplied by the host and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveContext {
    pub(crate) sql: String,
    pub(crate) read_only: bool,
    pub(crate) safe_mode: bool,
    pub(crate) confirm_dangerous: bool,
}

/// The decisions the host executes. No decision performs an action itself.
#[allow(dead_code)] // host port; removed with the query-panel wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GateAction {
    Execute {
        mode: ExecutionMode,
        sql: String,
        snapshot: ExecutionSnapshot,
    },
    Message {
        key: &'static str,
        token: Option<String>,
    },
    OpenUnclosedTransaction {
        pending: PendingExecute,
    },
    OpenStrategyAsk {
        target: AskTarget,
    },
    ConfirmDangerous {
        spec: confirm::ConfirmSpec,
    },
    PostExecution {
        had_error: bool,
    },
}

/// React `connConfig?.group === PRESET_GROUPS.production`.
#[allow(dead_code)] // pure helper; consumed by snapshot builders and tests.
pub(crate) fn is_production_group(group: Option<&str>) -> bool {
    group == Some(PRODUCTION_GROUP)
}

/// The gate's observable state (mirrors the hook's refs/flags).
#[allow(dead_code)] // VM surface; owned by the host.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct QueryExecutionGateState {
    pub(crate) pending_execute: Option<PendingExecute>,
    pub(crate) tx_unclosed_open: bool,
    pub(crate) ask_modal: Option<AskTarget>,
    /// React's 50ms `paramFocusTimerRef` deadline, on the host clock.
    pub(crate) param_focus_deadline: Option<f64>,
    /// The decision awaiting the host's confirm result.
    pub(crate) confirm_pending: Option<PendingConfirm>,
    /// The input behind an open ask modal, so its choices can re-enter the gate.
    pub(crate) ask_context: Option<GateInput>,
}

#[allow(dead_code)] // VM surface; removed with the query-panel wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingConfirm {
    pub(crate) target: ResolvedTarget,
    pub(crate) input: GateInput,
}

#[allow(dead_code)] // VM surface; removed with the query-panel wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedTarget {
    pub(crate) kind: ExecuteKind,
    pub(crate) sql: String,
}

/// Host events fed back into the reducer.
#[allow(dead_code)] // host port; removed with the query-panel wiring.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GateEvent {
    RequestExecute { input: GateInput, now: f64 },
    ConfirmUnclosed { input: GateInput },
    CancelUnclosed,
    AskExecuteCurrent,
    AskExecuteEntire,
    AskCancel,
    ConfirmResult { confirmed: bool, live: LiveContext },
}

/// React `isSnapshotStale`.
#[allow(dead_code)] // pure helper; consumed by confirm handling and tests.
pub(crate) fn is_stale(snapshot: &ExecutionSnapshot, live: &LiveContext) -> bool {
    snapshot.sql != live.sql
        || snapshot.read_only != live.read_only
        || snapshot.safe_mode != live.safe_mode
        || snapshot.confirm_dangerous != live.confirm_dangerous
}

/// React's statement resolution for the three strategies the gate handles
/// without an editor selection. Selection always wins.
#[allow(dead_code)] // pure helper; consumed by the pipeline and tests.
pub(crate) fn resolve_target(input: &GateInput) -> Result<ResolvedTarget, AskTarget> {
    let doc = input.snapshot.sql.as_str();
    if let Some(selection) = input.selection_sql.as_deref() {
        return Ok(ResolvedTarget {
            kind: ExecuteKind::Selection,
            sql: selection.to_owned(),
        });
    }
    let ranges = statement_ranges(doc);
    if input.cursor_offset.is_none() || ranges.len() <= 1 {
        return Ok(ResolvedTarget {
            kind: ExecuteKind::Full,
            sql: doc.to_owned(),
        });
    }
    let cursor = input.cursor_offset.unwrap_or(0);
    let current = ranges
        .iter()
        .copied()
        .find(|(from, to)| cursor >= *from && cursor <= *to)
        .or_else(|| ranges.first().copied())
        .unwrap_or((0, doc.len()));
    let current_sql = doc.get(current.0..current.1).unwrap_or("").trim().to_owned();
    match input.strategy {
        ExecutionStrategy::Ask => Err(AskTarget {
            current_statement: if current_sql.is_empty() { None } else { Some(current_sql) },
            statement_count: ranges.len(),
            entire_script: doc.to_owned(),
        }),
        ExecutionStrategy::EntireScript => Ok(ResolvedTarget {
            kind: ExecuteKind::Full,
            sql: doc.to_owned(),
        }),
        ExecutionStrategy::CurrentStatement | ExecutionStrategy::LargestStatement => {
            Ok(ResolvedTarget {
                kind: ExecuteKind::Selection,
                sql: current_sql,
            })
        }
    }
}

fn risk_mode(snapshot: &ExecutionSnapshot) -> RiskMode {
    RiskMode {
        read_only: snapshot.read_only,
        safe_mode: snapshot.safe_mode,
        is_production: snapshot.is_production,
        confirm_dangerous: snapshot.confirm_dangerous,
    }
}

fn message(key: &'static str) -> GateAction {
    GateAction::Message { key, token: None }
}

/// The gate's business pipeline after a target has been resolved. Shared by
/// the first request and the deferred unclosed-transaction confirmation.
fn decide_target(target: &ResolvedTarget, input: &GateInput) -> (Vec<GateAction>, Option<PendingConfirm>) {
    let snapshot = &input.snapshot;
    let decision = assess(&target.sql, &risk_mode(snapshot));

    if let Some(reason) = decision.hard_block_reason {
        let key = if reason == "readOnly" {
            "query.editor.executionConfirm.blockedReadOnly"
        } else {
            "query.editor.executionConfirm.blockedSafeMode"
        };
        return (vec![message(key)], None);
    }

    let mut reasons = decision.confirm_reasons.clone();
    if input.confirm_writes && decision.classification != risk::RiskClassification::Read && reasons.is_empty() {
        reasons.push("chat-write");
    }
    if decision.needs_confirm || !reasons.is_empty() {
        let spec = confirm_spec(&reasons, &decision.findings, &target.sql);
        return (
            vec![GateAction::ConfirmDangerous { spec }],
            Some(PendingConfirm {
                target: target.clone(),
                input: input.clone(),
            }),
        );
    }

    let substituted = substitute_params(&target.sql, &input.values);
    let mode = if target.kind == ExecuteKind::Selection || substituted != target.sql {
        ExecutionMode::Selection
    } else {
        ExecutionMode::Query
    };
    (
        vec![GateAction::Execute {
            mode,
            sql: substituted,
            snapshot: snapshot.clone(),
        }],
        None,
    )
}

/// `requestExecute` end to end, before any host confirm result.
#[allow(dead_code)] // pure gate; consumed by the reducer and tests.
pub(crate) fn decide(input: &GateInput, now: f64) -> (Vec<GateAction>, Option<PendingConfirm>) {
    if input.database_type.eq_ignore_ascii_case("postgresql")
        && suspicious_pg_double_quote(&input.snapshot.sql)
    {
        return (
            vec![message("query.postgresDoubleQuoteHint")],
            None,
        );
    }
    let target = match resolve_target(input) {
        Ok(target) => target,
        Err(ask) => return (vec![GateAction::OpenStrategyAsk { target: ask }], None),
    };
    let (unclosed, _, _, _) = transaction_analysis(&target.sql);
    if unclosed {
        return (
            vec![GateAction::OpenUnclosedTransaction {
                pending: PendingExecute {
                    kind: target.kind,
                    sql: if target.kind == ExecuteKind::Selection {
                        Some(target.sql.clone())
                    } else {
                        None
                    },
                },
            }],
            None,
        );
    }
    let missing = missing_params(&target.sql, &input.values);
    if let Some(first) = missing.first() {
        return (
            vec![GateAction::Message {
                key: "query.editor.param.missingValue",
                token: Some(first.label.clone()),
            }],
            None,
        );
    }
    // A missing-param focus deadline is recorded, never fired by this module.
    let _ = now;
    decide_target(&target, input)
}

/// State transition for one host event.
#[allow(dead_code)] // reducer; removed with the query-panel wiring.
pub(crate) fn reduce(state: &mut QueryExecutionGateState, event: GateEvent) -> Vec<GateAction> {
    match event {
        GateEvent::RequestExecute { input, now } => {
            state.ask_context = Some(input.clone());
            let (actions, pending) = decide(&input, now);
            if let Some(pending) = pending {
                state.confirm_pending = Some(pending);
            }
            if matches!(actions.first(), Some(GateAction::OpenUnclosedTransaction { .. })) {
                state.tx_unclosed_open = true;
                state.pending_execute = actions.first().and_then(|action| match action {
                    GateAction::OpenUnclosedTransaction { pending } => Some(pending.clone()),
                    _ => None,
                });
            }
            if matches!(actions.first(), Some(GateAction::OpenStrategyAsk { .. })) {
                if let Some(GateAction::OpenStrategyAsk { target }) = actions.first() {
                    state.ask_modal = Some(target.clone());
                }
            }
            if matches!(actions.first(), Some(GateAction::Message { key: "query.editor.param.missingValue", .. })) {
                state.param_focus_deadline = Some(now + 0.05);
            }
            actions
        }
        GateEvent::ConfirmUnclosed { input } => {
            state.pending_execute = None;
            state.tx_unclosed_open = false;
            let target = ResolvedTarget {
                kind: input.kind,
                sql: input.selection_sql.clone().unwrap_or_else(|| input.snapshot.sql.clone()),
            };
            decide_target(&target, &input).0
        }
        GateEvent::CancelUnclosed => {
            state.pending_execute = None;
            state.tx_unclosed_open = false;
            Vec::new()
        }
        GateEvent::AskExecuteCurrent => {
            let mut input = state.ask_context.clone();
            let current = state.ask_modal.as_ref().and_then(|target| target.current_statement.clone());
            state.ask_modal = None;
            match (input.as_mut(), current) {
                (Some(input), Some(sql)) => {
                    input.kind = ExecuteKind::Selection;
                    input.selection_sql = Some(sql);
                    decide(input, 0.0).0
                }
                _ => Vec::new(),
            }
        }
        GateEvent::AskExecuteEntire => {
            let mut input = state.ask_context.clone();
            let entire = state.ask_modal.as_ref().map(|target| target.entire_script.clone());
            state.ask_modal = None;
            match (input.as_mut(), entire) {
                (Some(input), Some(sql)) => {
                    input.kind = ExecuteKind::Full;
                    input.selection_sql = Some(sql);
                    decide(input, 0.0).0
                }
                _ => Vec::new(),
            }
        }
        GateEvent::AskCancel => {
            state.ask_modal = None;
            state.ask_context = None;
            Vec::new()
        }
        GateEvent::ConfirmResult { confirmed, live } => {
            let Some(pending) = state.confirm_pending.take() else {
                return Vec::new();
            };
            if !confirmed {
                return Vec::new();
            }
            if is_stale(&pending.input.snapshot, &live) {
                return vec![message("query.editor.executionConfirm.staleMessage")];
            }
            // React calls runExecute directly after a fresh confirm; the risk
            // gate is not evaluated a second time on the confirmed path.
            let substituted = substitute_params(&pending.target.sql, &pending.input.values);
            let mode = if pending.target.kind == ExecuteKind::Selection
                || substituted != pending.target.sql
            {
                ExecutionMode::Selection
            } else {
                ExecutionMode::Query
            };
            vec![GateAction::Execute {
                mode,
                sql: substituted,
                snapshot: pending.input.snapshot.clone(),
            }]
        }
    }
}

/// React's post-submit branch: an execution error opens the aborted-transaction
/// dialog, otherwise the transaction status is refreshed and completion fires.
#[allow(dead_code)] // pure helper; consumed by the host and tests.
pub(crate) fn post_execution(error: Option<&str>) -> GateAction {
    GateAction::PostExecution {
        had_error: error.is_some_and(|message| !message.is_empty()),
    }
}
