use crate::{DriverCapabilities, StatementResult};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueryPhase {
    #[default]
    Idle,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    OutcomeUnknown,
}

impl QueryPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryPhase::Idle => "idle",
            QueryPhase::Running => "running",
            QueryPhase::CancelRequested => "cancel_requested",
            QueryPhase::Succeeded => "succeeded",
            QueryPhase::Failed => "failed",
            QueryPhase::Cancelled => "cancelled",
            QueryPhase::OutcomeUnknown => "outcome_unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CancelCapability {
    Supported,
    Unsupported,
    #[default]
    Unknown,
}

impl CancelCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            CancelCapability::Supported => "supported",
            CancelCapability::Unsupported => "unsupported",
            CancelCapability::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CancelActionState {
    Available,
    Requested,
    #[default]
    Unavailable,
    Failed,
}

impl CancelActionState {
    pub fn as_str(self) -> &'static str {
        match self {
            CancelActionState::Available => "available",
            CancelActionState::Requested => "requested",
            CancelActionState::Unavailable => "unavailable",
            CancelActionState::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueryCancelState {
    #[default]
    Idle,
    Requested,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryTerminalState {
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryExecState {
    pub sql: String,
    pub results: Vec<StatementResult>,
    pub active_result_idx: usize,
    pub error: Option<String>,
    pub running: bool,
    pub execution_time_ms: Option<f64>,
    pub cancel_state: QueryCancelState,
    pub cancel_error: Option<String>,
    pub terminal_state: Option<QueryTerminalState>,
    pub execution_id: Option<String>,
    pub published_artifacts: Option<serde_json::Value>,
    pub result_detail_row_index: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryExecutionTransition {
    Start,
    CancelRequested,
    CancelFailed { error: String },
    Succeeded,
    Failed { error: String },
    Cancelled,
    OutcomeUnknown { error: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryExecutionViewModel {
    pub phase: QueryPhase,
    pub cancel_capability: CancelCapability,
    pub cancel_state: CancelActionState,
    pub elapsed_ms: Option<f64>,
    pub row_count: Option<usize>,
    pub affected_rows: Option<u64>,
    pub error: Option<String>,
    pub execution_id: Option<String>,
}

pub fn reduce_query_execution_state(
    exec: &QueryExecState,
    transition: &QueryExecutionTransition,
) -> QueryExecState {
    let mut next = exec.clone();
    match transition {
        QueryExecutionTransition::Start => {
            next.running = true;
            next.published_artifacts = None;
            next.error = None;
            next.cancel_state = QueryCancelState::Idle;
            next.cancel_error = None;
            next.terminal_state = None;
            next.execution_id = None;
        }
        QueryExecutionTransition::CancelRequested => {
            if exec.running {
                next.cancel_state = QueryCancelState::Requested;
                next.cancel_error = None;
            }
        }
        QueryExecutionTransition::CancelFailed { error } => {
            if exec.running {
                next.cancel_state = QueryCancelState::Failed;
                next.cancel_error = Some(error.clone());
            }
        }
        QueryExecutionTransition::Succeeded => {
            next.running = false;
            next.error = None;
            next.cancel_state = QueryCancelState::Idle;
            next.cancel_error = None;
            next.terminal_state = Some(QueryTerminalState::Succeeded);
            next.execution_id = None;
        }
        QueryExecutionTransition::Failed { error } => {
            next.running = false;
            next.error = Some(error.clone());
            next.cancel_state = QueryCancelState::Idle;
            next.cancel_error = None;
            next.terminal_state = Some(QueryTerminalState::Failed);
            next.execution_id = None;
        }
        QueryExecutionTransition::Cancelled => {
            next.running = false;
            next.error = None;
            next.cancel_state = QueryCancelState::Idle;
            next.cancel_error = None;
            next.terminal_state = Some(QueryTerminalState::Cancelled);
            next.execution_id = None;
        }
        QueryExecutionTransition::OutcomeUnknown { error } => {
            next.running = false;
            next.error = error.clone().or_else(|| exec.error.clone());
            next.cancel_state = QueryCancelState::Idle;
            next.terminal_state = Some(QueryTerminalState::Unknown);
            next.execution_id = None;
        }
    }
    next
}

pub fn get_cancel_capability(capabilities: Option<&DriverCapabilities>) -> CancelCapability {
    let Some(capabilities) = capabilities else {
        return CancelCapability::Unknown;
    };
    if !capabilities.supports_cancel_query {
        return CancelCapability::Unsupported;
    }
    if capabilities.supports_query_execution_cancel {
        CancelCapability::Supported
    } else {
        CancelCapability::Unknown
    }
}

fn contains_user_request(message: &str) -> bool {
    let mut start = 0usize;
    while let Some(offset) = message[start..].find("user") {
        let after = start + offset + 4;
        if message[after..].starts_with("request") {
            return true;
        }
        if after < message.len() {
            let mut chars = message[after..].chars();
            chars.next();
            if chars.as_str().starts_with("request") {
                return true;
            }
        }
        start = after;
    }
    false
}

pub fn is_cancellation_error(message: Option<&str>) -> bool {
    let Some(message) = message else {
        return false;
    };
    let lower = message.to_lowercase();
    lower.contains("cancel") || lower.contains("abort") || lower.contains("interrupt") || contains_user_request(&lower)
}

fn terminal_phase(terminal: Option<QueryTerminalState>, error: Option<&str>) -> Option<QueryPhase> {
    match terminal {
        Some(QueryTerminalState::Succeeded) => Some(QueryPhase::Succeeded),
        Some(QueryTerminalState::Cancelled) => Some(QueryPhase::Cancelled),
        Some(QueryTerminalState::Failed) => Some(QueryPhase::Failed),
        Some(QueryTerminalState::Unknown) => Some(QueryPhase::OutcomeUnknown),
        None if error.is_some() => Some(QueryPhase::Failed),
        None => None,
    }
}

fn compute_cancel_action_state(
    capability: CancelCapability,
    cancel_state: QueryCancelState,
    execution_id: Option<&str>,
    phase: QueryPhase,
) -> CancelActionState {
    if capability != CancelCapability::Supported
        || execution_id.is_none()
        || !(phase == QueryPhase::Running || phase == QueryPhase::CancelRequested)
    {
        return CancelActionState::Unavailable;
    }
    if phase == QueryPhase::CancelRequested || cancel_state == QueryCancelState::Requested {
        return CancelActionState::Requested;
    }
    if cancel_state == QueryCancelState::Failed {
        return CancelActionState::Failed;
    }
    CancelActionState::Available
}

pub fn get_cancel_action_state(view_model: &QueryExecutionViewModel) -> CancelActionState {
    compute_cancel_action_state(
        view_model.cancel_capability,
        match view_model.cancel_state {
            CancelActionState::Requested => QueryCancelState::Requested,
            CancelActionState::Failed => QueryCancelState::Failed,
            _ => QueryCancelState::Idle,
        },
        view_model.execution_id.as_deref(),
        view_model.phase,
    )
}

pub fn to_query_execution_view_model(
    exec: &QueryExecState,
    capabilities: Option<&DriverCapabilities>,
) -> QueryExecutionViewModel {
    let cancel_capability = get_cancel_capability(capabilities);
    let error = exec.cancel_error.clone().or_else(|| exec.error.clone());
    let mut phase = terminal_phase(exec.terminal_state, exec.error.as_deref());
    if exec.running {
        phase = Some(if exec.cancel_state == QueryCancelState::Requested {
            QueryPhase::CancelRequested
        } else {
            QueryPhase::Running
        });
    } else if phase.is_none() {
        phase = Some(QueryPhase::Idle);
    }
    let phase = phase.unwrap_or(QueryPhase::Idle);
    let cancel_state = compute_cancel_action_state(
        cancel_capability,
        exec.cancel_state,
        exec.execution_id.as_deref(),
        phase,
    );
    let active = exec.results.get(exec.active_result_idx);
    QueryExecutionViewModel {
        phase,
        cancel_capability,
        cancel_state,
        elapsed_ms: exec.execution_time_ms,
        row_count: active.map(|result| result.rows.len()),
        affected_rows: active.and_then(|result| result.rows_affected),
        error,
        execution_id: exec.execution_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn exec_state() -> QueryExecState {
        QueryExecState {
            sql: "SELECT 1".to_owned(),
            running: true,
            execution_id: Some("e1".to_owned()),
            execution_time_ms: Some(12.5),
            ..Default::default()
        }
    }

    fn capabilities() -> DriverCapabilities {
        DriverCapabilities {
            supports_cancel_query: true,
            supports_query_execution_cancel: true,
            supports_explain: false,
            supports_streaming_results: false,
            supports_offset: true,
            has_schema_level: true,
        }
    }

    fn result(rows: usize, affected: Option<u64>) -> StatementResult {
        StatementResult {
            source: None,
            runtime_binding: None,
            provenance: None,
            sql: String::new(),
            columns: Vec::new(),
            rows: vec![vec![None]; rows],
            rows_affected: affected,
            execution_time_ms: 0,
            truncated: None,
            pinned: None,
        }
    }

    #[test]
    fn start_resets_the_run_fields() {
        let mut exec = exec_state();
        exec.cancel_state = QueryCancelState::Requested;
        exec.cancel_error = Some("old".to_owned());
        exec.error = Some("old".to_owned());
        exec.terminal_state = Some(QueryTerminalState::Failed);
        exec.published_artifacts = Some(json!({"a": 1}));
        let next = reduce_query_execution_state(&exec, &QueryExecutionTransition::Start);
        assert!(next.running);
        assert_eq!(next.error, None);
        assert_eq!(next.cancel_error, None);
        assert_eq!(next.cancel_state, QueryCancelState::Idle);
        assert_eq!(next.terminal_state, None);
        assert_eq!(next.execution_id, None);
        assert_eq!(next.published_artifacts, None);
    }

    #[test]
    fn cancel_transitions_only_apply_while_running() {
        let running = exec_state();
        let requested = reduce_query_execution_state(&running, &QueryExecutionTransition::CancelRequested);
        assert_eq!(requested.cancel_state, QueryCancelState::Requested);
        let failed = reduce_query_execution_state(
            &running,
            &QueryExecutionTransition::CancelFailed { error: "nope".to_owned() },
        );
        assert_eq!(failed.cancel_state, QueryCancelState::Failed);
        assert_eq!(failed.cancel_error.as_deref(), Some("nope"));

        let settled = QueryExecState { running: false, ..running };
        let ignored = reduce_query_execution_state(&settled, &QueryExecutionTransition::CancelRequested);
        assert_eq!(ignored, settled);
        let ignored = reduce_query_execution_state(
            &settled,
            &QueryExecutionTransition::CancelFailed { error: "nope".to_owned() },
        );
        assert_eq!(ignored, settled);
    }

    #[test]
    fn terminal_transitions_settle_and_clear_the_handle() {
        let running = exec_state();
        for (transition, terminal, error) in [
            (QueryExecutionTransition::Succeeded, QueryTerminalState::Succeeded, None),
            (QueryExecutionTransition::Cancelled, QueryTerminalState::Cancelled, None),
            (
                QueryExecutionTransition::Failed { error: "boom".to_owned() },
                QueryTerminalState::Failed,
                Some("boom"),
            ),
        ] {
            let next = reduce_query_execution_state(&running, &transition);
            assert!(!next.running);
            assert_eq!(next.terminal_state, Some(terminal));
            assert_eq!(next.error.as_deref(), error);
            assert_eq!(next.execution_id, None);
            assert_eq!(next.cancel_state, QueryCancelState::Idle);
        }
    }

    #[test]
    fn outcome_unknown_keeps_the_existing_error_and_cancel_error() {
        let mut exec = exec_state();
        exec.error = Some("original".to_owned());
        exec.cancel_error = Some("cancel note".to_owned());
        let next = reduce_query_execution_state(&exec, &QueryExecutionTransition::OutcomeUnknown { error: None });
        assert_eq!(next.error.as_deref(), Some("original"));
        assert_eq!(next.cancel_error.as_deref(), Some("cancel note"), "cancelError is untouched here");
        assert_eq!(next.terminal_state, Some(QueryTerminalState::Unknown));
        let next = reduce_query_execution_state(
            &exec,
            &QueryExecutionTransition::OutcomeUnknown { error: Some("new".to_owned()) },
        );
        assert_eq!(next.error.as_deref(), Some("new"));
    }

    #[test]
    fn cancel_capability_requires_both_flags() {
        assert_eq!(get_cancel_capability(None), CancelCapability::Unknown);
        let mut caps = capabilities();
        caps.supports_cancel_query = false;
        assert_eq!(get_cancel_capability(Some(&caps)), CancelCapability::Unsupported);
        caps.supports_cancel_query = true;
        caps.supports_query_execution_cancel = false;
        assert_eq!(get_cancel_capability(Some(&caps)), CancelCapability::Unknown);
        caps.supports_query_execution_cancel = true;
        assert_eq!(get_cancel_capability(Some(&caps)), CancelCapability::Supported);
    }

    #[test]
    fn cancellation_errors_recognize_the_documented_spellings() {
        assert!(!is_cancellation_error(None));
        assert!(!is_cancellation_error(Some("")));
        for message in [
            "CANCELLED",
            "cancel",
            "User requested stop",
            "user-request",
            "userrequest",
            "aborted",
            "abort",
            "interrupted",
            "interrupt",
        ] {
            assert!(is_cancellation_error(Some(message)), "{message}");
        }
        assert!(!is_cancellation_error(Some("user   request")), "only one separator char is allowed");
        assert!(!is_cancellation_error(Some("syntax error")));
    }

    #[test]
    fn running_view_model_prefers_the_cancel_request_phase() {
        let exec = exec_state();
        let view = to_query_execution_view_model(&exec, Some(&capabilities()));
        assert_eq!(view.phase, QueryPhase::Running);
        assert_eq!(view.cancel_state, CancelActionState::Available);
        assert_eq!(view.execution_id.as_deref(), Some("e1"));
        assert_eq!(view.elapsed_ms, Some(12.5));

        let requested = QueryExecState { cancel_state: QueryCancelState::Requested, ..exec };
        let view = to_query_execution_view_model(&requested, Some(&capabilities()));
        assert_eq!(view.phase, QueryPhase::CancelRequested);
        assert_eq!(view.cancel_state, CancelActionState::Requested);
    }

    #[test]
    fn terminal_view_model_uses_terminal_phase_and_result_counters() {
        let mut exec = exec_state();
        exec.running = false;
        exec.terminal_state = Some(QueryTerminalState::Succeeded);
        exec.execution_id = None;
        exec.results = vec![result(2, Some(9)), result(5, None)];
        exec.active_result_idx = 1;
        let view = to_query_execution_view_model(&exec, Some(&capabilities()));
        assert_eq!(view.phase, QueryPhase::Succeeded);
        assert_eq!(view.row_count, Some(5));
        assert_eq!(view.affected_rows, None);
        assert_eq!(view.cancel_state, CancelActionState::Unavailable);

        exec.active_result_idx = 0;
        let view = to_query_execution_view_model(&exec, Some(&capabilities()));
        assert_eq!(view.row_count, Some(2));
        assert_eq!(view.affected_rows, Some(9));
    }

    #[test]
    fn error_without_terminal_state_is_a_failed_phase() {
        let exec = QueryExecState {
            running: false,
            error: Some("boom".to_owned()),
            ..Default::default()
        };
        let view = to_query_execution_view_model(&exec, None);
        assert_eq!(view.phase, QueryPhase::Failed);
        assert_eq!(view.error.as_deref(), Some("boom"));
        assert_eq!(view.cancel_capability, CancelCapability::Unknown);
        assert_eq!(view.row_count, None);
    }

    #[test]
    fn cancel_error_is_the_reported_error_when_no_terminal_error_exists() {
        let exec = QueryExecState {
            cancel_error: Some("cancel failed".to_owned()),
            error: Some("earlier".to_owned()),
            ..Default::default()
        };
        let view = to_query_execution_view_model(&exec, None);
        assert_eq!(view.phase, QueryPhase::Failed);
        assert_eq!(view.error.as_deref(), Some("cancel failed"));
    }

    #[test]
    fn cancel_action_state_reflects_capability_and_phase() {
        let base = QueryExecutionViewModel {
            phase: QueryPhase::Running,
            cancel_capability: CancelCapability::Supported,
            cancel_state: CancelActionState::Available,
            elapsed_ms: None,
            row_count: None,
            affected_rows: None,
            error: None,
            execution_id: Some("e".to_owned()),
        };
        assert_eq!(get_cancel_action_state(&base), CancelActionState::Available);
        assert_eq!(
            get_cancel_action_state(&QueryExecutionViewModel { cancel_state: CancelActionState::Failed, ..base.clone() }),
            CancelActionState::Failed
        );
        assert_eq!(
            get_cancel_action_state(&QueryExecutionViewModel { execution_id: None, ..base.clone() }),
            CancelActionState::Unavailable
        );
        assert_eq!(
            get_cancel_action_state(&QueryExecutionViewModel { phase: QueryPhase::Succeeded, ..base.clone() }),
            CancelActionState::Unavailable
        );
        assert_eq!(
            get_cancel_action_state(&QueryExecutionViewModel { cancel_capability: CancelCapability::Unknown, ..base }),
            CancelActionState::Unavailable
        );
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::reduce_query_execution_state;
        let _ = crate::get_cancel_capability;
        let _ = crate::is_cancellation_error;
        let _ = crate::get_cancel_action_state;
        let _ = crate::to_query_execution_view_model;
        assert_eq!(crate::QueryPhase::CancelRequested.as_str(), "cancel_requested");
        assert_eq!(crate::CancelCapability::Supported.as_str(), "supported");
        assert_eq!(crate::CancelActionState::Requested.as_str(), "requested");
        let _: Option<crate::QueryExecState> = None;
        let _: Option<crate::QueryExecutionViewModel> = None;
        let _: Option<crate::QueryExecutionTransition> = None;
        let _: Option<crate::QueryCancelState> = None;
        let _: Option<crate::QueryTerminalState> = None;
    }
}
