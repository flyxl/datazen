use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ai::AiProviderConfig;
use crate::db::{ConnectionConfig, SavedTunnel};

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

/// A saved SQL favorite, backed by one `.sql` file under the favorites root.
///
/// The `id` is the file's stem — a ULID the app mints, never a title-derived
/// name, so renaming a favorite never rewrites a file (see plan §2.6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuery {
    pub id: String,
    /// Owning persisted connection id (config).
    pub connection_id: String,
    pub title: String,
    pub sql: String,
    pub created_at: DateTime<Utc>,
    /// `None` for favorites migrated from SQLite, which had no such column.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// Optional completion keyword from the front-matter (`-- keyword:`).
    /// Read and round-tripped today; binding it to completion is a later track.
    #[serde(default)]
    pub keyword: Option<String>,
    /// Logical database the statement targets, when the file records one.
    #[serde(default)]
    pub database: Option<String>,
    /// Directory of this favorite relative to the favorites root, `/`-separated.
    /// `None` = the root itself.
    #[serde(default)]
    pub folder: Option<String>,
}

/// Persisted state for a data-sync task (checkpoint / resume).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTask {
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
    pub source_row_counts: std::collections::HashMap<String, u64>,
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

impl SyncTask {
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

#[derive(Default)]
pub(crate) struct StoreCache {
    pub(super) connections: Vec<ConnectionConfig>,
    pub(super) platform_profiles:
        std::collections::BTreeMap<String, super::platform_profiles::ProfileMetadata>,
    /// Independently stored tunnel definitions (`tunnels.json`).
    pub(super) tunnels: Vec<SavedTunnel>,
    pub(super) groups: Vec<String>,
    pub(super) settings: AppSettings,
    /// Lazy: loaded on first sync / AI access.
    pub(super) sync_tasks: Vec<SyncTask>,
    pub(super) sync_tasks_loaded: bool,
    /// Lazy: loaded on first Data Sync profile access.
    pub(super) sync_profiles: Vec<crate::data_sync::SyncProfile>,
    pub(super) sync_profiles_loaded: bool,
    /// Lazy: loaded on first Data Transfer profile access.
    pub(super) transfer_profiles: Vec<crate::data_transfer::TransferProfile>,
    pub(super) transfer_profiles_loaded: bool,
    /// Lazy: loaded on first Schema Diff profile access.
    pub(super) schema_diff_profiles: Vec<crate::schema_diff::SchemaDiffProfile>,
    pub(super) schema_diff_profiles_loaded: bool,
    pub(super) ai_config: Option<AiProviderConfig>,
    pub(super) ai_settings_config: Option<crate::ai::AiSettingsConfig>,
    pub(super) ai_config_loaded: bool,
}
