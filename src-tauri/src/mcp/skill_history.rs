//! Persists Skill execution history for auditing and replay.

use super::skills::SkillExecutionResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::RwLock;

const MAX_HISTORY_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub variables: serde_json::Value,
    pub result: SkillExecutionResult,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListItem {
    pub id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub success: bool,
    pub total_time_ms: u64,
    pub created_at: String,
}

pub struct SkillHistoryManager {
    history_dir: PathBuf,
    cache: RwLock<Vec<HistoryEntry>>,
}

impl SkillHistoryManager {
    pub fn new(history_dir: PathBuf) -> Self {
        Self {
            history_dir,
            cache: RwLock::new(Vec::new()),
        }
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
        skill_id: &str,
        skill_name: &str,
        variables: &serde_json::Value,
        result: &SkillExecutionResult,
    ) -> Result<String, String> {
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
            skill_id: skill_id.into(),
            skill_name: skill_name.into(),
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

    pub async fn list(&self, skill_id: Option<&str>) -> Vec<HistoryListItem> {
        let cache = self.cache.read().await;
        cache
            .iter()
            .filter(|e| skill_id.map_or(true, |sid| e.skill_id == sid))
            .map(|e| HistoryListItem {
                id: e.id.clone(),
                skill_id: e.skill_id.clone(),
                skill_name: e.skill_name.clone(),
                success: e.result.success,
                total_time_ms: e.result.total_time_ms,
                created_at: e.created_at.clone(),
            })
            .collect()
    }

    pub async fn get(&self, history_id: &str) -> Option<HistoryEntry> {
        self.cache
            .read()
            .await
            .iter()
            .find(|e| e.id == history_id)
            .cloned()
    }

    pub async fn clear(&self, skill_id: Option<&str>) -> Result<usize, String> {
        let mut cache = self.cache.write().await;
        let (to_remove, to_keep): (Vec<_>, Vec<_>) = cache
            .drain(..)
            .partition(|e| skill_id.map_or(true, |sid| e.skill_id == sid));

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
    use crate::mcp::skills::{StepExecutionResult, StepStatus};

    fn make_test_result(success: bool) -> SkillExecutionResult {
        SkillExecutionResult {
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
        let mgr = SkillHistoryManager::new(dir.path().to_path_buf());
        mgr.load().await.unwrap();

        let result = make_test_result(true);
        let id = mgr
            .record("skill-1", "Skill 1", &serde_json::json!({"uid": "U001"}), &result)
            .await
            .unwrap();

        let list = mgr.list(None).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].skill_id, "skill-1");
        assert!(list[0].success);

        let entry = mgr.get(&id).await.unwrap();
        assert_eq!(entry.skill_name, "Skill 1");
        assert_eq!(entry.variables, serde_json::json!({"uid": "U001"}));
    }

    #[tokio::test]
    async fn test_list_filter_by_skill_id() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SkillHistoryManager::new(dir.path().to_path_buf());
        mgr.load().await.unwrap();

        let r = make_test_result(true);
        mgr.record("skill-a", "A", &serde_json::json!({}), &r).await.unwrap();
        mgr.record("skill-b", "B", &serde_json::json!({}), &r).await.unwrap();
        mgr.record("skill-a", "A", &serde_json::json!({}), &r).await.unwrap();

        assert_eq!(mgr.list(None).await.len(), 3);
        assert_eq!(mgr.list(Some("skill-a")).await.len(), 2);
        assert_eq!(mgr.list(Some("skill-b")).await.len(), 1);
    }

    #[tokio::test]
    async fn test_clear() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SkillHistoryManager::new(dir.path().to_path_buf());
        mgr.load().await.unwrap();

        let r = make_test_result(true);
        mgr.record("s1", "S1", &serde_json::json!({}), &r).await.unwrap();
        mgr.record("s2", "S2", &serde_json::json!({}), &r).await.unwrap();

        let removed = mgr.clear(Some("s1")).await.unwrap();
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
            let mgr = SkillHistoryManager::new(dir.path().to_path_buf());
            mgr.load().await.unwrap();
            mgr.record("s1", "S1", &serde_json::json!({"x": 1}), &r)
                .await
                .unwrap();
        }

        let mgr2 = SkillHistoryManager::new(dir.path().to_path_buf());
        mgr2.load().await.unwrap();
        assert_eq!(mgr2.list(None).await.len(), 1);
    }
}
