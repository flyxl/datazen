use std::collections::BTreeMap;

use crate::{types::CountStrategy, FilterCondition, SortCondition, TableState};

pub const TABLE_DATA_STORE_GAP: [&str; 8] = [
    "databaseCommands.getTableData / previewPendingChanges / commitPendingChanges IPC",
    "tableManualHandle session binding",
    "injected reload callback for page/filter/sort transitions",
    "persisted sort and column-layout storage",
    "settings store defaultPageSize / defaultTableSort / rowCountMode",
    "DB_REGISTRY driver metadata and driverCapabilities",
    "t() error translations",
    "panel slices wired from selection/pending-edit/enum/draft/seed slices",
];

pub const TABLE_DATA_STORE_SURFACE: [&str; 9] = [
    "loadTableData",
    "reloadPanel",
    "invalidateCachedData",
    "orderRequest",
    "applyFilters",
    "applyRawWhere",
    "addSort",
    "commitFetchedPage",
    "commitPendingChanges",
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableTargetData {
    pub db_session_id: String,
    pub table: String,
    pub connection_id: Option<String>,
    pub driver_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadTableDataParams {
    pub panel_id: String,
    pub target: TableTargetData,
    pub skip_count: Option<bool>,
    pub count_strategy: Option<CountStrategy>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrderRequest {
    pub default_order_columns: Option<Vec<String>>,
    pub sorts: Vec<SortCondition>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableDataStoreSnapshot {
    pub by_panel: BTreeMap<String, TableState>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FetchedPage {
    pub columns: Vec<crate::types::ColumnSchema>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total_rows: Option<u64>,
    pub total_rows_kind: Option<CountStrategy>,
    pub page: u64,
    pub page_size: u64,
    pub drift_suspected: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReloadRequest {
    pub skip_count: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnyColumnExpansion {
    pub conditions: Vec<FilterCondition>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PendingStatus {
    #[default]
    Idle,
    Previewing,
    Committing,
}

impl PendingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Previewing => "previewing",
            Self::Committing => "committing",
        }
    }
}
