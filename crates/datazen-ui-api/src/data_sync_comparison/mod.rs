mod flow;
mod mapping;
mod validate;

use serde_json::Value;

pub use flow::{
    cancel_actions, clear_job_refs, first_diff_key, is_compared_sync_state, is_current_generation,
    is_paged_comparison, next_generation, selected_rows_from_merged, should_keep_cancelled_result,
    CancelActions, ClearJobRefsInput, DataSyncJobKind, SelectedComparisonRow,
};
pub use mapping::{
    apply_options_to_rows, mark_disabled_tables, merge_compare_into_mappings, tables_for_compare,
    toggle_disabled_status, update_source_filter, MappedRow,
};
pub use validate::{validate_endpoints, EndpointInput, ValidationFailure};

use crate::DataSyncOperation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataSyncSyncState {
    Idle,
    Inspecting,
    Comparing,
    Compared,
    Executing,
    Unknown,
    Done,
}

impl DataSyncSyncState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Inspecting => "inspecting",
            Self::Comparing => "comparing",
            Self::Compared => "compared",
            Self::Executing => "executing",
            Self::Unknown => "unknown",
            Self::Done => "done",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonTableStatus {
    Matched,
    Disabled,
    Incompatible,
    Unmapped,
}

impl ComparisonTableStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "MATCHED",
            Self::Disabled => "DISABLED",
            Self::Incompatible => "INCOMPATIBLE",
            Self::Unmapped => "UNMAPPED_SOURCE",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonRow {
    pub operation: DataSyncOperation,
    pub key: Vec<Value>,
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComparisonTableRow {
    pub source_table: String,
    pub target_table: String,
    pub status: ComparisonTableStatus,
    pub source_filter: Option<String>,
    pub rows: Option<Vec<ComparisonRow>>,
}

impl ComparisonTableRow {
    pub fn matched(name: &str, rows: Vec<ComparisonRow>) -> Self {
        Self {
            source_table: name.to_owned(),
            target_table: name.to_owned(),
            status: ComparisonTableStatus::Matched,
            source_filter: None,
            rows: Some(rows),
        }
    }
}

pub const DATA_SYNC_SELECT_BOTH_KEY: &str = "sync.selectBoth";
pub const DATA_SYNC_SAME_DB_KEY: &str = "sync.cannotSameDb";
pub const DATA_SYNC_DB_REQUIRED_KEY: &str = "sync.selectDbRequired";
pub const DATA_SYNC_TRANSFER_HINT_KEY: &str = "sync.useTransferHint";
pub const SYNC_CANCELLING_EXECUTION_KEY: &str = "sync.cancellingExecution";
pub const SYNC_COMPARE_CANCELLED_KEY: &str = "sync.compareCancelled";

pub const DATA_SYNC_COMPARISON_SURFACE: [&str; 10] = [
    "validateEndpoints",
    "handleInspect",
    "handleCompare",
    "handleCancel",
    "toggleDisabledTable",
    "updateSourceFilter",
    "mergeCompareIntoMappings",
    "summarizeCompare",
    "buildCompareReportText",
    "DATA_SYNC_COMPARISON_GAP",
];

pub const DATA_SYNC_COMPARISON_GAP: [&str; 8] = [
    "React hook state and useCallback/useMemo wiring",
    "syncCommands IPC for inspect, compare, cancel and job refresh",
    "compareGenerationRef staleness fence across async completions",
    "crypto.randomUUID job ids and jobKind/jobId refs",
    "navigator.clipboard.writeText for the compare report",
    "cross-window refresh and endpoint session refresh",
    "i18n t() for status and error messages",
    "paged comparison contractVersion detection",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_surfaces_and_gaps_are_pinned() {
        assert_eq!(DATA_SYNC_COMPARISON_SURFACE.len(), 10);
        assert_eq!(DATA_SYNC_COMPARISON_GAP.len(), 8);
        assert_eq!(crate::DATA_SYNC_COMPARISON_SURFACE.len(), 10);
        assert_eq!(crate::DATA_SYNC_COMPARISON_GAP.len(), 8);
        assert_eq!(DataSyncSyncState::Compared.as_str(), "compared");
        assert_eq!(ComparisonTableStatus::Disabled.as_str(), "DISABLED");
    }
}
