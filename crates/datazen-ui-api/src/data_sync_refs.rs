#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use crate::{DataSyncJobKind, DataSyncSelectedRow, DataSyncSyncState, DataSyncTableSelection};

pub const DATA_SYNC_REFS_SURFACE: [&str; 3] = [
    "useDataSyncRefs",
    "DataSyncSharedRefs",
    "DATA_SYNC_REFS_GAP",
];

pub const DATA_SYNC_REFS_GAP: [&str; 3] = [
    "React useRef identity and cross-hook sharing",
    "async promise timing behind each mutable ref",
    "loadedPageRef Set identity across page reloads",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSyncTableMapping {
    pub source_table: String,
    pub target_table: String,
    pub enabled: bool,
    pub source_filter: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataSyncSharedRefs {
    pub job_id: Option<String>,
    pub job_kind: Option<DataSyncJobKind>,
    pub cancel_requested_job: Option<String>,
    pub cancelling_status_job: Option<String>,
    pub compare_generation: u64,
    pub write_in_flight: bool,
    pub sync_state: DataSyncSyncState,
    pub selected_rows: Vec<DataSyncSelectedRow>,
    pub table_selections: Vec<DataSyncTableSelection>,
    pub loaded_pages: BTreeSet<String>,
    pub pending_profile_mappings: Option<Vec<DataSyncTableMapping>>,
}

impl Default for DataSyncSharedRefs {
    fn default() -> Self {
        Self {
            job_id: None,
            job_kind: None,
            cancel_requested_job: None,
            cancelling_status_job: None,
            compare_generation: 0,
            write_in_flight: false,
            sync_state: DataSyncSyncState::Idle,
            selected_rows: Vec::new(),
            table_selections: Vec::new(),
            loaded_pages: BTreeSet::new(),
            pending_profile_mappings: None,
        }
    }
}

pub fn create_data_sync_refs() -> DataSyncSharedRefs {
    DataSyncSharedRefs::default()
}
