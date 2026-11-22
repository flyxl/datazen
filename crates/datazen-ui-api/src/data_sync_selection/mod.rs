mod page;
mod scopes;
mod totals;

use std::collections::BTreeMap;

use serde_json::Value;

pub use page::{
    loaded_page_token, next_page_cursors, next_page_index, page_selection_decision, resolve_cursor,
    table_page_key, CursorOutcome,
};
pub use scopes::{
    clear_table_operation_scope, is_table_operation_scoped, reconcile_selected_rows,
    set_table_operation_scope, update_exclusions,
};
pub use totals::{has_selected_deletes, row_counts, total_selected_rows, RowCounts};

use crate::{
    ComparisonRow, ComparisonTableStatus, DataSyncSelectionScopeMode, SelectedComparisonRow,
};

#[derive(Clone, Debug, PartialEq)]
pub struct TableSelectionScope {
    pub source_table: String,
    pub target_table: String,
    pub selection_mode: DataSyncSelectionScopeMode,
    pub operations: Vec<crate::DataSyncOperation>,
    pub excluded_rows: Vec<crate::data_sync_options::ScopeExclusion>,
}

pub fn scope_for_table_operation(
    scopes: &[TableSelectionScope],
    source: &str,
    target: &str,
    operation: crate::DataSyncOperation,
) -> Option<TableSelectionScope> {
    scopes
        .iter()
        .find(|scope| {
            scope.source_table == source
                && scope.target_table == target
                && scope.operations.contains(&operation)
        })
        .cloned()
}

impl TableSelectionScope {
    pub fn selects_operation(
        &self,
        operation: crate::DataSyncOperation,
        options: &crate::SyncOptions,
    ) -> bool {
        operation != crate::DataSyncOperation::Unchanged
            && self.operations.contains(&operation)
            && (self.selection_mode == DataSyncSelectionScopeMode::All
                || crate::default_row_selected(operation, options))
    }
}

pub const PAGE_DIRECTION_INITIAL: &str = "initial";
pub const PAGE_DIRECTION_PREVIOUS: &str = "previous";
pub const PAGE_DIRECTION_NEXT: &str = "next";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageDirection {
    Initial,
    Previous,
    Next,
}

impl PageDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => PAGE_DIRECTION_INITIAL,
            Self::Previous => PAGE_DIRECTION_PREVIOUS,
            Self::Next => PAGE_DIRECTION_NEXT,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageCursor {
    pub current: Option<String>,
    pub next: Option<String>,
    pub previous: Vec<String>,
}

pub type PageCursorTable = BTreeMap<String, PageCursor>;

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionTable {
    pub source_table: String,
    pub target_table: String,
    pub status: ComparisonTableStatus,
    pub rows: Option<Vec<ComparisonRow>>,
    pub first_cursor: Option<String>,
    pub page_size: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionStateView {
    pub selected_rows: Vec<SelectedComparisonRow>,
    pub scopes: Vec<TableSelectionScope>,
    pub page_cursors: PageCursorTable,
    pub page_index: BTreeMap<String, i64>,
    pub loaded_pages: Vec<String>,
}

pub struct RowChange {
    pub operation: crate::DataSyncOperation,
    pub key: Vec<Value>,
    pub selected: bool,
}

pub const DATA_SYNC_SELECTION_SURFACE: [&str; 10] = [
    "loadTablePage",
    "totalSelectedRows",
    "hasSelectedDeletes",
    "updateTableRows",
    "setTableOperationScope",
    "clearTableOperationScope",
    "isTableOperationScoped",
    "resolveCursor",
    "pageSelectionDecision",
    "DATA_SYNC_SELECTION_GAP",
];

pub const DATA_SYNC_SELECTION_GAP: [&str; 7] = [
    "React hook state and useCallback/useMemo wiring",
    "syncCommands getDataSyncComparisonPage IPC",
    "async page loading, cancellation flags and error dialogs",
    "the shared refs for selected rows, scopes and loaded pages",
    "wizard setMappingResults / setSyncState callbacks",
    "paged comparison contractVersion detection",
    "the table snapshot used for row-level reconciliation",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_literals_and_surfaces_are_pinned() {
        assert_eq!(PageDirection::Initial.as_str(), "initial");
        assert_eq!(PageDirection::Previous.as_str(), "previous");
        assert_eq!(PageDirection::Next.as_str(), "next");
        assert_eq!(DATA_SYNC_SELECTION_SURFACE.len(), 10);
        assert_eq!(DATA_SYNC_SELECTION_GAP.len(), 7);
        assert_eq!(crate::DATA_SYNC_SELECTION_GAP.len(), 7);
    }
}
