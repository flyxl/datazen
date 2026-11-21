mod runtime;
mod transitions;

use std::collections::BTreeMap;

use serde_json::Value;

pub use runtime::{
    extract_error, notify_schema_actions, post_success_view_mode, run_start_state,
    should_apply_event, ExecutionError, SchemaAction,
};
pub use transitions::{
    is_cancellation_error, patch_exec, query_error_transition, reduce_query_execution_state,
    transition_exec, QueryExecutionTransition,
};

use crate::{ChartConfig, StatementResult};

pub type ExecTable = BTreeMap<String, QueryExecState>;
pub type BindParams = BTreeMap<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryCancelState {
    Idle,
    Requested,
    Failed,
}

impl QueryCancelState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Requested => "requested",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryTerminalState {
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

impl QueryTerminalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryExecState {
    pub sql: String,
    pub results: Vec<StatementResult>,
    pub active_result_idx: usize,
    pub error: Option<String>,
    pub running: bool,
    pub execution_time_ms: Option<u64>,
    pub cancel_state: QueryCancelState,
    pub cancel_error: Option<String>,
    pub terminal_state: Option<QueryTerminalState>,
    pub chart_config: Option<ChartConfig>,
    pub result_view_mode: Option<String>,
    pub execution_id: Option<String>,
    pub published_artifacts: Option<Value>,
    pub stream_run_id: Option<u64>,
    pub result_detail_row_index: Option<u64>,
}

impl QueryExecState {
    pub fn empty() -> Self {
        Self {
            sql: String::new(),
            results: Vec::new(),
            active_result_idx: 0,
            error: None,
            running: false,
            execution_time_ms: None,
            cancel_state: QueryCancelState::Idle,
            cancel_error: None,
            terminal_state: None,
            chart_config: None,
            result_view_mode: None,
            execution_id: None,
            published_artifacts: None,
            stream_run_id: None,
            result_detail_row_index: None,
        }
    }
}

pub const QUERY_ERROR_QUOTA_CODE: &str = "SessionQuotaExceeded";
pub const QUOTA_MESSAGE_KEY: &str = "query.session.editorQuotaExceeded";
pub const GENERIC_FAILURE_KEY: &str = "common.executionFailed";
pub const REFRESH_CONNECTION_EVENT: &str = "datazen:refresh-connection";

pub const QUERY_EXEC_ACTIONS_SURFACE: [&str; 8] = [
    "runStreamingQuery",
    "runBoundQuery",
    "patchExec",
    "reduceQueryExecutionState",
    "queryErrorTransition",
    "extractError",
    "notifySchemaChangedIfNeeded",
    "QUERY_EXEC_ACTIONS_GAP",
];

pub const QUERY_EXEC_ACTIONS_GAP: [&str; 7] = [
    "runEditorQuery IPC and its QueryStreamEvent channel",
    "async setExec/getExec store callbacks and streamRunId staleness",
    "crossWindowBus emit for datazen:refresh-connection",
    "schema cache and table-data cache invalidation hosts",
    "resolvePostQueryViewMode over the settings store",
    "the i18n t() callback for error messages",
    "error classification beyond the quota code",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_and_wire_literals_are_pinned() {
        let empty = QueryExecState::empty();
        assert!(!empty.running);
        assert_eq!(empty.cancel_state, QueryCancelState::Idle);
        assert_eq!(empty.terminal_state, None);
        assert_eq!(empty.active_result_idx, 0);
        assert_eq!(QueryCancelState::Requested.as_str(), "requested");
        assert_eq!(QueryTerminalState::Unknown.as_str(), "unknown");
        assert_eq!(QUERY_ERROR_QUOTA_CODE, "SessionQuotaExceeded");
        assert_eq!(REFRESH_CONNECTION_EVENT, "datazen:refresh-connection");
        assert_eq!(QUERY_EXEC_ACTIONS_SURFACE.len(), 8);
        assert_eq!(QUERY_EXEC_ACTIONS_GAP.len(), 7);
        assert_eq!(crate::QUERY_EXEC_ACTIONS_GAP.len(), 7);
    }
}
