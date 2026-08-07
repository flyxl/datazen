//! Persists workflow execution history for auditing and replay.

use crate::workflow::workflows::WorkflowExecutionResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, RwLock};

const MAX_HISTORY_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    #[serde(alias = "skillId")]
    pub workflow_id: String,
    #[serde(alias = "skillName")]
    pub workflow_name: String,
    pub variables: serde_json::Value,
    pub result: WorkflowExecutionResult,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListItem {
    pub id: String,
    #[serde(alias = "skillId")]
    pub workflow_id: String,
    #[serde(alias = "skillName")]
    pub workflow_name: String,
    pub success: bool,
    pub total_time_ms: u64,
    pub created_at: String,
}

pub struct WorkflowHistoryManager {
    history_dir: PathBuf,
    cache: RwLock<Vec<HistoryEntry>>,
    loaded: AtomicBool,
    load_lock: Mutex<()>,
}

impl WorkflowHistoryManager {
    pub fn new(history_dir: PathBuf) -> Self {
        // Migrate legacy `skill_history` directory if present.
        if !history_dir.exists() {
            if let Some(parent) = history_dir.parent() {
                let legacy = parent.join("skill_history");
                if legacy.is_dir() {
                    if let Err(e) = std::fs::rename(&legacy, &history_dir) {
                        tracing::warn!(
                            from = %legacy.display(),
                            to = %history_dir.display(),
                            error = %e,
                            "Failed to rename skill_history → workflow_history"
                        );
                    } else {
                        tracing::info!(
                            to = %history_dir.display(),
                            "Migrated skill_history → workflow_history"
                        );
                    }
                }
            }
        }

        Self {
            history_dir,
            cache: RwLock::new(Vec::new()),
            loaded: AtomicBool::new(false),
            load_lock: Mutex::new(()),
        }
    }

    pub async fn ensure_loaded(&self) -> Result<(), String> {
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let _guard = self.load_lock.lock().await;
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        self.load().await?;
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }

    pub async fn load(&self) -> Result<(), String> {
        if !self.history_dir.exists() {
            std::fs::create_dir_all(&self.history_dir).map_err(|e| e.to_string())?;
            return Ok(());
        }

        let mut entries = Vec::new();
        let dir_entries = std::fs::read_dir(&self.history_dir).map_err(|e| e.to_string())?;

        for entry in dir_entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<HistoryEntry>(&content) {
                        Ok(he) => entries.push(he),
                        Err(e) => {
                            tracing::warn!("Failed to parse history {:?}: {}", path, e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to read history {:?}: {}", path, e);
                    }
                }
            }
        }

        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        *self.cache.write().await = entries;
        Ok(())
    }

    pub async fn record(
        &self,
        workflow_id: &str,
        workflow_name: &str,
        variables: &serde_json::Value,
        result: &WorkflowExecutionResult,
    ) -> Result<String, String> {
        self.ensure_loaded().await?;

        if !self.history_dir.exists() {
            std::fs::create_dir_all(&self.history_dir).map_err(|e| e.to_string())?;
        }

        let now = chrono::Local::now();
        let id = format!(
            "{}_{}",
            now.format("%Y%m%dT%H%M%S"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        let entry = HistoryEntry {
            id: id.clone(),
            workflow_id: workflow_id.into(),
            workflow_name: workflow_name.into(),
            variables: variables.clone(),
            result: result.clone(),
            created_at: now.to_rfc3339(),
        };

        let path = self.history_dir.join(format!("{id}.json"));
        let content = serde_json::to_string_pretty(&entry).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())?;

        let mut cache = self.cache.write().await;
        cache.insert(0, entry);

        // Auto-cleanup: remove oldest entries beyond limit
        while cache.len() > MAX_HISTORY_ENTRIES {
            if let Some(old) = cache.pop() {
                let old_path = self.history_dir.join(format!("{}.json", old.id));
                let _ = std::fs::remove_file(old_path);
            }
        }

        Ok(id)
    }

    pub async fn list(&self, workflow_id: Option<&str>) -> Vec<HistoryListItem> {
        if let Err(e) = self.ensure_loaded().await {
            tracing::warn!("Failed to load workflow history before list: {e}");
            return Vec::new();
        }
        let cache = self.cache.read().await;
        cache
            .iter()
            .filter(|e| workflow_id.map_or(true, |wid| e.workflow_id == wid))
            .map(|e| HistoryListItem {
                id: e.id.clone(),
                workflow_id: e.workflow_id.clone(),
                workflow_name: e.workflow_name.clone(),
                success: e.result.success,
                total_time_ms: e.result.total_time_ms,
                created_at: e.created_at.clone(),
            })
            .collect()
    }

    pub async fn get(&self, history_id: &str) -> Option<HistoryEntry> {
        if let Err(e) = self.ensure_loaded().await {
            tracing::warn!("Failed to load workflow history before get: {e}");
            return None;
        }
        self.cache
            .read()
            .await
            .iter()
            .find(|e| e.id == history_id)
            .cloned()
    }

    pub async fn clear(&self, workflow_id: Option<&str>) -> Result<usize, String> {
        self.ensure_loaded().await?;
        let mut cache = self.cache.write().await;
        let (to_remove, to_keep): (Vec<_>, Vec<_>) = cache
            .drain(..)
            .partition(|e| workflow_id.map_or(true, |wid| e.workflow_id == wid));

        let count = to_remove.len();
        for entry in &to_remove {
            let path = self.history_dir.join(format!("{}.json", entry.id));
            let _ = std::fs::remove_file(path);
        }

        *cache = to_keep;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[tokio::test]
    async fn test_record_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = WorkflowHistoryManager::new(dir.path().to_path_buf());
        mgr.load().await.unwrap();

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
        let mgr = WorkflowHistoryManager::new(dir.path().to_path_buf());
        mgr.load().await.unwrap();

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
        let mgr = WorkflowHistoryManager::new(dir.path().to_path_buf());
        mgr.load().await.unwrap();

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
            let mgr = WorkflowHistoryManager::new(dir.path().to_path_buf());
            mgr.load().await.unwrap();
            mgr.record("w1", "W1", &serde_json::json!({"x": 1}), &r)
                .await
                .unwrap();
        }

        let mgr2 = WorkflowHistoryManager::new(dir.path().to_path_buf());
        mgr2.load().await.unwrap();
        assert_eq!(mgr2.list(None).await.len(), 1);
    }
}
