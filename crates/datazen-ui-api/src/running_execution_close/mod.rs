mod decision;
mod gate;
mod pane_keys;
mod settle;

pub use decision::{
    guard_decision, guard_result, running_close_request, running_panes_of, stoppable_panes,
    GuardDecision,
};
pub use gate::{
    ask_now_resolution, gate_change_effects, host_error_answer, no_gate_answer, settled_once,
    AskResolution, GateChangeEffects,
};
pub use pane_keys::{
    is_pane_key_of_panel, pane_id_of_pane_key, pane_key, pane_keys_of_panel, panel_id_of_pane_key,
    DEFAULT_PANE_ID, PANE_SEPARATOR,
};
pub use settle::{
    deadline_from, effective_retry_ms, initial_settle_state, settle_running_panes_step,
    SettleOutcome, SettleState,
};

use crate::QueryExecState;

pub const RUNNING_CLOSE_TIMEOUT_MS: i64 = 15_000;
pub const RUNNING_CLOSE_POLL_MS: i64 = 50;
pub const RUNNING_CLOSE_RETRY_MS: i64 = 250;

#[derive(Clone, Debug, PartialEq)]
pub struct RunningPane {
    pub pane_id: String,
    pub exec: QueryExecState,
    pub sql: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningExecutionRequest {
    pub panel_id: String,
    pub title: String,
    pub pane_id: String,
    pub sql: String,
    pub running_panes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunningCloseDecision {
    Proceed,
    Declined,
    Unconfirmed { pending: Vec<String> },
}

pub const RUNNING_EXECUTION_CLOSE_SURFACE: [&str; 8] = [
    "setRunningExecutionGate",
    "askRunningExecutionGate",
    "runningPanesOf",
    "stoppablePanes",
    "confirmStoppableRunningPanes",
    "createRunningCloseGuard",
    "settleRunningPanes",
    "RUNNING_EXECUTION_CLOSE_GAP",
];

pub const RUNNING_EXECUTION_CLOSE_GAP: [&str; 6] = [
    "Promise serialization of concurrent gate prompts",
    "host confirmation dialog and its Promise<boolean>",
    "setTimeout / Date.now polling driven by the host runtime",
    "requestCancel IPC and its acceptance receipt timing",
    "the store's live queryExec map writes",
    "markBlocked host callback for unconfirmed panes",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timings_and_surfaces_are_pinned() {
        assert_eq!(RUNNING_CLOSE_TIMEOUT_MS, 15_000);
        assert_eq!(RUNNING_CLOSE_POLL_MS, 50);
        assert_eq!(RUNNING_CLOSE_RETRY_MS, 250);
        assert_eq!(RUNNING_EXECUTION_CLOSE_SURFACE.len(), 8);
        assert_eq!(RUNNING_EXECUTION_CLOSE_GAP.len(), 6);
        assert_eq!(crate::RUNNING_EXECUTION_CLOSE_SURFACE.len(), 8);
        assert_eq!(crate::RUNNING_EXECUTION_CLOSE_GAP.len(), 6);
    }
}
