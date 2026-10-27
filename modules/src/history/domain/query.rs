//! 查询历史条目（原 `src-tauri/src/store/models.rs::QueryHistoryEntry`）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Record of a executed SQL statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryEntry {
    pub id: String,
    /// Owning persisted connection id (config).
    pub connection_id: String,
    /// Session-active logical database when the statement ran ("" = unknown/legacy rows).
    pub database: String,
    /// Schema namespace when known (PG search_path is not session-tracked yet → usually None).
    #[serde(default)]
    pub schema: Option<String>,
    pub sql: String,
    pub executed_at: DateTime<Utc>,
    pub execution_time_ms: u64,
    pub rows_affected: Option<u64>,
    pub success: bool,
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IPC 形状钉住：camelCase 键名是前端契约，迁移不得改变序列化形态。
    #[test]
    fn query_history_entry_keeps_camel_case_wire_shape() {
        let entry = QueryHistoryEntry {
            id: "h1".into(),
            connection_id: "c1".into(),
            database: "db".into(),
            schema: None,
            sql: "SELECT 1".into(),
            executed_at: DateTime::<Utc>::UNIX_EPOCH,
            execution_time_ms: 12,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["executionTimeMs"], 12);
        assert_eq!(json["connectionId"], "c1");
        assert_eq!(json["executedAt"], "1970-01-01T00:00:00Z");
        let round: QueryHistoryEntry = serde_json::from_value(json).unwrap();
        assert_eq!(round.id, entry.id);
        assert_eq!(round.execution_time_ms, entry.execution_time_ms);
    }
}
