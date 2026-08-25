use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ai::AiProviderConfig;
use crate::db::ConnectionConfig;

use super::settings::AppSettings;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuery {
    pub id: String,
    /// Owning persisted connection id (config).
    pub connection_id: String,
    pub title: String,
    pub sql: String,
    pub created_at: DateTime<Utc>,
}

/// Persisted state for a data-sync task (checkpoint / resume).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTask {
    pub id: String,
    /// Runtime db session id captured when the task was created/resumed.
    pub source_db_session_id: String,
    /// Runtime db session id captured when the task was created/resumed.
    pub target_db_session_id: String,
    /// Persisted owning connection id (config) for display / resume lookup.
    pub source_connection_id: String,
    /// Persisted owning connection id (config) for display / resume lookup.
    pub target_connection_id: String,
    /// All tables selected for sync.
    pub tables: Vec<String>,
    /// Tables that have been fully synced.
    pub completed_tables: Vec<String>,
    /// Table that was being synced when interrupted (if any).
    pub current_table: Option<String>,
    /// Row offset within the current table (rows already inserted).
    pub current_table_offset: u64,
    /// Source row count snapshot at task creation, keyed by table name.
    pub source_row_counts: std::collections::HashMap<String, u64>,
    /// "full" | "continue"
    pub strategy: String,
    /// "running" | "paused" | "completed" | "failed"
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Default)]
pub(crate) struct StoreCache {
    pub(super) connections: Vec<ConnectionConfig>,
    pub(super) groups: Vec<String>,
    pub(super) settings: AppSettings,
    /// Lazy: loaded on first sync / AI access.
    pub(super) sync_tasks: Vec<SyncTask>,
    pub(super) sync_tasks_loaded: bool,
    pub(super) ai_config: Option<AiProviderConfig>,
    pub(super) ai_config_loaded: bool,
}
