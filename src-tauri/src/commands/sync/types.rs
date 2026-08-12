pub(super) const DATA_COMPARE_SAMPLE_LIMIT: usize = 1000;
pub(super) const DATA_COMPARE_MISMATCH_LIMIT: usize = 50;

/// Progress event emitted during sync.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SyncProgressEvent {
    pub(super) task_id: String,
    pub(super) phase: String,
    pub(super) table_index: usize,
    pub(super) total_tables: usize,
    pub(super) current_table: String,
    pub(super) source_row_count: u64,
    pub(super) synced_rows: u64,
    pub(super) completed_tables: Vec<String>,
    pub(super) error: Option<String>,
}

pub(super) const BATCH_SIZE: usize = 500;
