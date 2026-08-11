//! Persists workflow execution history for auditing and replay.

use crate::store::{HistoryDb, HistoryEntry, HistoryListItem};
use crate::workflow::workflows::WorkflowExecutionResult;
use std::sync::Arc;

pub struct WorkflowHistoryManager {
    db: Arc<HistoryDb>,
}

impl WorkflowHistoryManager {
    pub fn new(db: Arc<HistoryDb>) -> Self {
        Self { db }
    }

    pub async fn record(
        &self,
        workflow_id: &str,
        workflow_name: &str,
        variables: &serde_json::Value,
        result: &WorkflowExecutionResult,
    ) -> Result<String, String> {
        let now = chrono::Local::now();
        let id = format!(
            "{}_{}",
            now.format("%Y%m%dT%H%M%S"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        self.db
            .record_workflow(
                &id,
                workflow_id,
                workflow_name,
                variables,
                result,
                &now.to_rfc3339(),
            )
            .map_err(|e| e.to_string())?;

        Ok(id)
    }

    pub async fn list(&self, workflow_id: Option<&str>) -> Vec<HistoryListItem> {
        self.db
            .list_workflow_history(workflow_id)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to list workflow history from SQLite");
                Vec::new()
            })
    }

    pub async fn get(&self, history_id: &str) -> Option<HistoryEntry> {
        self.db.get_workflow_history(history_id).unwrap_or_else(|e| {
            tracing::warn!(error = %e, history_id, "Failed to get workflow history from SQLite");
            None
        })
    }

    pub async fn clear(&self, workflow_id: Option<&str>) -> Result<usize, String> {
        self.db
            .clear_workflow_history(workflow_id)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::HistoryDb;
    use crate::workflow::workflows::{StepExecutionResult, StepStatus};

    fn make_test_result(success: bool) -> WorkflowExecutionResult {
        WorkflowExecutionResult {
            success,
            final_output: "test output".into(),
            steps: vec![StepExecutionResult {
                step_id: "s1".into(),
                step_type: "query".into(),
                status: StepStatus::Success,
                result: Some(serde_json::json!({"rows": []})),
                execution_time_ms: 10,
                error: None,
                connection_name: None,
                sql_executed: Some("SELECT 1".into()),
            }],
            total_time_ms: 42,
            error: None,
        }
    }

    fn open_mgr(dir: &std::path::Path) -> WorkflowHistoryManager {
        WorkflowHistoryManager::new(HistoryDb::open(dir).expect("history db"))
    }

    #[tokio::test]
    async fn test_record_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = open_mgr(dir.path());

        let result = make_test_result(true);
        let id = mgr
            .record("workflow-1", "Workflow 1", &serde_json::json!({"uid": "U001"}), &result)
            .await
            .unwrap();

        let list = mgr.list(None).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].workflow_id, "workflow-1");
        assert!(list[0].success);

        let entry = mgr.get(&id).await.unwrap();
        assert_eq!(entry.workflow_name, "Workflow 1");
        assert_eq!(entry.variables, serde_json::json!({"uid": "U001"}));
    }

    #[tokio::test]
    async fn test_list_filter_by_workflow_id() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = open_mgr(dir.path());

        let r = make_test_result(true);
        mgr.record("workflow-a", "A", &serde_json::json!({}), &r).await.unwrap();
        mgr.record("workflow-b", "B", &serde_json::json!({}), &r).await.unwrap();
        mgr.record("workflow-a", "A", &serde_json::json!({}), &r).await.unwrap();

        assert_eq!(mgr.list(None).await.len(), 3);
        assert_eq!(mgr.list(Some("workflow-a")).await.len(), 2);
        assert_eq!(mgr.list(Some("workflow-b")).await.len(), 1);
    }

    #[tokio::test]
    async fn test_clear() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = open_mgr(dir.path());

        let r = make_test_result(true);
        mgr.record("w1", "W1", &serde_json::json!({}), &r).await.unwrap();
        mgr.record("w2", "W2", &serde_json::json!({}), &r).await.unwrap();

        let removed = mgr.clear(Some("w1")).await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(mgr.list(None).await.len(), 1);

        let removed = mgr.clear(None).await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(mgr.list(None).await.len(), 0);
    }

    #[tokio::test]
    async fn test_persistence_across_load() {
        let dir = tempfile::tempdir().unwrap();
        let r = make_test_result(true);

        {
            let mgr = open_mgr(dir.path());
            mgr.record("w1", "W1", &serde_json::json!({"x": 1}), &r)
                .await
                .unwrap();
        }

        let mgr2 = open_mgr(dir.path());
        assert_eq!(mgr2.list(None).await.len(), 1);
    }

    #[tokio::test]
    async fn test_get_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = open_mgr(dir.path());
        assert!(mgr.get("missing-id").await.is_none());
    }

    #[tokio::test]
    async fn test_history_truncates_beyond_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = open_mgr(dir.path());
        let r = make_test_result(true);

        for i in 0..105 {
            mgr.record(&format!("w{i}"), "W", &serde_json::json!({}), &r)
                .await
                .unwrap();
        }
        assert!(mgr.list(None).await.len() <= crate::store::MAX_WORKFLOW_HISTORY);
    }
}
