//! The persisted form of a Data Sync task（断点/续传状态）。
//!
//! W8 刀3e-2 词汇归位：定义本体从桌面 `store/models.rs` 上移（FavoriteQuery
//! / QueryHistoryEntry 同款先例），桌面文件形（`sync_tasks.json`）与服务端
//! 表形（v15 `sync_tasks`，组织键投影）共用这一条词汇，序列化形状
//! （serde camelCase + 各字段缺省）钉在模块单测——IPC wire 逐字节不变。
//!
//! 与 [`super::model::SyncTask`]（比较/执行管线的域内任务定义：端点 +
//! 选项 + 映射）是**两个概念**：本类型是「任务列表 + 断点续传状态」的
//! 持久化投影——连接 id、库/schema 选择、表清单、已完成表、当前表与
//! 行偏移、源侧行数快照、策略/状态/错误信息。运行时会话 id 与 OFFSET
//! 检查点是进程局部状态，[`StoredSyncTask::normalize_legacy_state`] 在
//! 每次落盘/入账前把它们清除——遗留检查点永不安全续传。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Persisted state for a data-sync task (checkpoint / resume).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredSyncTask {
    pub id: String,
    /// Runtime db session id from legacy task files only.
    ///
    /// A `dbSessionId` is process-local and must never be persisted or used
    /// to resume a task after restart. The field remains in the Rust model so
    /// old JSON can be read, but it is cleared during store normalization and
    /// omitted from every serialized task.
    #[serde(default, skip_serializing)]
    pub source_db_session_id: String,
    /// Runtime db session id from legacy task files only; see the source field.
    #[serde(default, skip_serializing)]
    pub target_db_session_id: String,
    /// Persisted owning connection id (config) for display / resume lookup.
    pub source_connection_id: String,
    /// Persisted owning connection id (config) for display / resume lookup.
    pub target_connection_id: String,
    /// Logical source catalog/database selected for the task, when known.
    #[serde(default)]
    pub source_database: Option<String>,
    /// Logical target catalog/database selected for the task, when known.
    #[serde(default)]
    pub target_database: Option<String>,
    /// Source schema selected for the task, when known.
    #[serde(default)]
    pub source_schema: Option<String>,
    /// Target schema selected for the task, when known.
    #[serde(default)]
    pub target_schema: Option<String>,
    /// All tables selected for sync.
    pub tables: Vec<String>,
    /// Tables that have been fully synced.
    pub completed_tables: Vec<String>,
    /// Table that was being synced when interrupted (if any).
    pub current_table: Option<String>,
    /// Row offset within the current table (rows already inserted).
    #[serde(default)]
    pub current_table_offset: u64,
    /// Source row count snapshot at task creation, keyed by table name.
    #[serde(default)]
    pub source_row_counts: HashMap<String, u64>,
    /// "full" | "continue"
    #[serde(default = "default_sync_task_strategy")]
    pub strategy: String,
    /// "running" | "paused" | "completed" | "failed"
    #[serde(default = "default_sync_task_status")]
    pub status: String,
    #[serde(default)]
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Legacy checkpoints are never safe to resume from an offset. The value
    /// is deliberately explicit so clients cannot mistake a task for a safe
    /// resumable job.
    #[serde(default = "default_sync_task_resume_state")]
    pub resume_state: String,
}

fn default_sync_task_strategy() -> String {
    "unknown".into()
}

fn default_sync_task_status() -> String {
    "interrupted".into()
}

fn default_sync_task_resume_state() -> String {
    "unknown".into()
}

impl StoredSyncTask {
    /// Normalize a task loaded from disk or received through the legacy save
    /// IPC. Runtime sessions and OFFSET checkpoints are process-local state;
    /// retaining them would let a later process silently continue at the
    /// wrong row. Returns whether the value changed.
    pub fn normalize_legacy_state(&mut self) -> bool {
        let mut changed = false;

        if !self.source_db_session_id.is_empty() {
            self.source_db_session_id.clear();
            changed = true;
        }
        if !self.target_db_session_id.is_empty() {
            self.target_db_session_id.clear();
            changed = true;
        }

        let had_unsafe_checkpoint = self.current_table_offset > 0
            || self.strategy.eq_ignore_ascii_case("continue")
            || matches!(self.status.as_str(), "running" | "paused");

        if had_unsafe_checkpoint {
            if self.current_table_offset != 0 {
                self.current_table_offset = 0;
                changed = true;
            }
            if self.strategy != "unknown" {
                self.strategy = "unknown".into();
                changed = true;
            }
            if self.status != "interrupted" {
                self.status = "interrupted".into();
                changed = true;
            }
            let message = "This sync task contains a legacy checkpoint that cannot be resumed safely; run the sync again from the beginning.";
            if self.error_message.as_deref() != Some(message) {
                self.error_message = Some(message.into());
                changed = true;
            }
        }

        if self.resume_state != "unknown" {
            self.resume_state = "unknown".into();
            changed = true;
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn task() -> StoredSyncTask {
        StoredSyncTask {
            id: "task-1".into(),
            source_db_session_id: String::new(),
            target_db_session_id: String::new(),
            source_connection_id: "source".into(),
            target_connection_id: "target".into(),
            source_database: Some("app".into()),
            target_database: Some("app".into()),
            source_schema: None,
            target_schema: None,
            tables: vec!["users".into()],
            completed_tables: vec![],
            current_table: None,
            current_table_offset: 0,
            source_row_counts: HashMap::from([("users".to_string(), 10)]),
            strategy: "full".into(),
            status: "completed".into(),
            error_message: None,
            created_at: Utc.timestamp_opt(0, 0).single().expect("epoch"),
            updated_at: Utc.timestamp_opt(0, 0).single().expect("epoch"),
            resume_state: "unknown".into(),
        }
    }

    #[test]
    fn wire_shape_is_camel_case_with_host_defaults() {
        let value = serde_json::to_value(task()).expect("serialize");
        // The wire the host IPC has always sent (camelCase, no session ids).
        assert_eq!(value["sourceConnectionId"], "source");
        assert_eq!(value["sourceRowCounts"]["users"], 10);
        assert_eq!(value["resumeState"], "unknown");
        assert!(value.get("sourceDbSessionId").is_none());
        assert!(value.get("targetDbSessionId").is_none());
        let back: StoredSyncTask = serde_json::from_value(value.clone()).expect("deserialize");
        assert_eq!(back, task());
    }

    #[test]
    fn legacy_json_without_optionals_decodes_with_defaults() {
        let legacy = serde_json::json!({
            "id": "legacy",
            "sourceConnectionId": "s",
            "targetConnectionId": "t",
            "tables": ["a"],
            "completedTables": [],
            "createdAt": "1970-01-01T00:00:00Z",
            "updatedAt": "1970-01-01T00:00:00Z"
        });
        let task: StoredSyncTask = serde_json::from_value(legacy).expect("legacy decodes");
        assert_eq!(task.strategy, "unknown");
        assert_eq!(task.status, "interrupted");
        assert_eq!(task.resume_state, "unknown");
        assert_eq!(task.current_table_offset, 0);
        assert!(task.source_row_counts.is_empty());
    }

    #[test]
    fn normalization_clears_runtime_sessions_and_unsafe_checkpoints() {
        let mut task = task();
        task.source_db_session_id = "session-9".into();
        task.target_db_session_id = "session-8".into();
        task.current_table_offset = 42;
        task.strategy = "continue".into();
        task.status = "running".into();
        assert!(task.normalize_legacy_state());
        assert!(task.source_db_session_id.is_empty());
        assert!(task.target_db_session_id.is_empty());
        assert_eq!(task.current_table_offset, 0);
        assert_eq!(task.strategy, "unknown");
        assert_eq!(task.status, "interrupted");
        assert!(task
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("cannot be resumed safely"));
        // Idempotent: a second pass changes nothing.
        assert!(!task.normalize_legacy_state());
    }
}
