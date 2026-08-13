use super::super::error::CommandError;
use super::super::AppState;

/// Retired product path: overwrite copy (DROP + INSERT) is not Data Synchronization.
pub(crate) async fn sync_table_impl(
    _state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<u64, CommandError> {
    tracing::info!(%source_connection_id, %target_connection_id, %table_name, "sync_table refused");
    Err(crate::data_sync::refuse_overwrite_copy().into())
}

/// Retired product path: overwrite copy (DROP + INSERT) is not Data Synchronization.
pub(crate) async fn sync_tables_impl(
    _state: &AppState,
    task_id: String,
    _source_connection_id: String,
    _target_connection_id: String,
    _source_config_id: String,
    _target_config_id: String,
    tables: Vec<String>,
    _skip_tables: Vec<String>,
    strategy: String,
    _resume_table: Option<String>,
    resume_offset: u64,
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%task_id, table_count = tables.len(), %strategy, resume_offset, "sync_tables refused");
    Err(crate::data_sync::refuse_overwrite_copy().into())
}
