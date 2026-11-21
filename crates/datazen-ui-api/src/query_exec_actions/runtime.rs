use crate::StatementResult;

use super::{QueryExecState, GENERIC_FAILURE_KEY, QUOTA_MESSAGE_KEY};

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionError {
    Quota,
    Message(String),
    Generic,
}

pub fn extract_error<F>(error: &ExecutionError, translate: F) -> String
where
    F: Fn(&str) -> String,
{
    match error {
        ExecutionError::Quota => translate(QUOTA_MESSAGE_KEY),
        ExecutionError::Message(message) => message.clone(),
        ExecutionError::Generic => translate(GENERIC_FAILURE_KEY),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaAction {
    InvalidateSchemaCache,
    InvalidateTableData,
    EmitRefreshConnection,
}

pub fn notify_schema_actions(
    may_mutate_schema: bool,
    contains_data_modifying: bool,
    contains_schema_changing_ddl: bool,
) -> Vec<SchemaAction> {
    let mut actions = Vec::new();
    if may_mutate_schema {
        actions.push(SchemaAction::InvalidateSchemaCache);
    }
    if contains_data_modifying {
        actions.push(SchemaAction::InvalidateTableData);
    }
    if contains_schema_changing_ddl {
        actions.push(SchemaAction::EmitRefreshConnection);
    }
    actions
}

pub fn run_start_state(exec: &QueryExecState, run_id: u64) -> QueryExecState {
    let pinned: Vec<StatementResult> = exec
        .results
        .iter()
        .filter(|result| result.pinned == Some(true))
        .cloned()
        .collect();
    let base_offset = pinned.len();
    QueryExecState {
        results: pinned,
        active_result_idx: base_offset,
        stream_run_id: Some(run_id),
        execution_time_ms: None,
        execution_id: None,
        ..exec.clone()
    }
}

pub fn should_apply_event(exec: &QueryExecState, run_id: u64) -> bool {
    exec.stream_run_id == Some(run_id)
}

pub fn post_success_view_mode(is_chartable: bool, auto_chart: bool) -> &'static str {
    if auto_chart && is_chartable {
        "chart"
    } else {
        "table"
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn translate(key: &str) -> String {
        format!("T:{key}")
    }

    fn exec() -> QueryExecState {
        let mut pinned = StatementResult {
            source: None,
            runtime_binding: None,
            provenance: None,
            sql: "p".to_owned(),
            columns: Vec::new(),
            rows: Vec::new(),
            rows_affected: None,
            execution_time_ms: 0,
            truncated: None,
            pinned: Some(true),
        };
        let mut plain = pinned.clone();
        plain.pinned = None;
        plain.sql = "u".to_owned();
        pinned.rows.push(vec![Some(json!(1))]);
        QueryExecState {
            results: vec![pinned, plain],
            ..QueryExecState::empty()
        }
    }

    #[test]
    fn quota_errors_get_an_actionable_message_and_others_keep_theirs() {
        assert_eq!(
            extract_error(&ExecutionError::Quota, translate),
            "T:query.session.editorQuotaExceeded"
        );
        assert_eq!(
            extract_error(&ExecutionError::Message("boom".to_owned()), translate),
            "boom"
        );
        assert_eq!(
            extract_error(&ExecutionError::Generic, translate),
            "T:common.executionFailed"
        );
    }

    #[test]
    fn schema_notifications_follow_the_three_guards_in_order() {
        assert!(notify_schema_actions(false, false, false).is_empty());
        assert_eq!(
            notify_schema_actions(true, true, true),
            [
                SchemaAction::InvalidateSchemaCache,
                SchemaAction::InvalidateTableData,
                SchemaAction::EmitRefreshConnection,
            ]
        );
        assert_eq!(
            notify_schema_actions(false, false, true),
            [SchemaAction::EmitRefreshConnection]
        );
    }

    #[test]
    fn run_start_keeps_only_pinned_results_and_stamps_the_run() {
        let started = run_start_state(&exec(), 7);
        assert_eq!(started.results.len(), 1);
        assert_eq!(started.results[0].sql, "p");
        assert_eq!(started.active_result_idx, 1);
        assert_eq!(started.stream_run_id, Some(7));
        assert_eq!(started.execution_time_ms, None);
        assert_eq!(started.execution_id, None);
    }

    #[test]
    fn stale_stream_events_are_ignored() {
        let started = run_start_state(&exec(), 7);
        assert!(should_apply_event(&started, 7));
        assert!(!should_apply_event(&started, 8));
        assert!(!should_apply_event(&QueryExecState::empty(), 7));
    }

    #[test]
    fn view_mode_requires_both_auto_setting_and_chartable_result() {
        assert_eq!(post_success_view_mode(true, true), "chart");
        assert_eq!(post_success_view_mode(true, false), "table");
        assert_eq!(post_success_view_mode(false, true), "table");
    }
}
