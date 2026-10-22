//! Sample rows shared by every history_db test module.
//!
//! Kept in one place so a change to [`QueryHistoryEntry`]'s shape fails in one
//! file rather than in four, and so "a query row" means the same thing to the
//! filtering, deduplication, retention, and paging suites.

use super::*;
use chrono::Utc;
use uuid::Uuid;

use datazen_feature_core::workflow::model::{
    StepExecutionResult, StepStatus, WorkflowExecutionResult,
};

/// A query row `days_ago` in the past, on the default connection.
pub(super) fn sample_query(sql: &str, days_ago: i64) -> QueryHistoryEntry {
    QueryHistoryEntry {
        id: Uuid::new_v4().to_string(),
        connection_id: "cfg1".into(),
        database: "app".into(),
        schema: None,
        sql: sql.into(),
        executed_at: Utc::now() - chrono::Duration::days(days_ago),
        execution_time_ms: 10,
        rows_affected: Some(1),
        success: true,
        error_message: None,
    }
}

/// A query row pinned to a named connection, so scope-sensitive assertions
/// (dedup by connection, filtering by connection) have something to separate.
pub(super) fn sample_query_for_config(sql: &str, connection_id: &str) -> QueryHistoryEntry {
    QueryHistoryEntry {
        id: Uuid::new_v4().to_string(),
        connection_id: connection_id.into(),
        database: "app".into(),
        schema: None,
        sql: sql.into(),
        executed_at: Utc::now(),
        execution_time_ms: 10,
        rows_affected: Some(1),
        success: true,
        error_message: None,
    }
}

pub(super) fn make_test_result(success: bool) -> WorkflowExecutionResult {
    WorkflowExecutionResult {
        success,
        final_output: "test".into(),
        steps: vec![StepExecutionResult {
            step_id: "s1".into(),
            step_type: "query".into(),
            status: StepStatus::Success,
            result: Some(serde_json::json!({})),
            execution_time_ms: 10,
            error: None,
            connection_name: None,
            sql_executed: Some("SELECT 1".into()),
        }],
        total_time_ms: 42,
        error: None,
    }
}
