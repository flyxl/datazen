mod filter;
mod types;

pub use filter::{
    display_table_name, filter_table_rows, row_diff_counts, table_has_row_diffs,
    table_is_unchanged, table_key,
};
pub use types::{RowDiffCounts, TableDiffFilter, TableListRow, TableSyncStatus, DATA_SYNC_FILTERS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableItemBadge {
    RowDiffs(RowDiffCounts),
    Incompatible,
    Identical,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterButtonView {
    pub filter: TableDiffFilter,
    pub test_id: String,
    pub pressed: bool,
    pub variant: &'static str,
    pub label_key: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableListItemView {
    pub key: String,
    pub display_name: String,
    pub active: bool,
    pub badge: TableItemBadge,
    pub test_id: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableListView {
    pub search_placeholder_key: &'static str,
    pub empty_message_key: &'static str,
    pub filters: Vec<FilterButtonView>,
    pub items: Vec<TableListItemView>,
    pub empty: bool,
}

pub const SEARCH_PLACEHOLDER_KEY: &str = "common.searchTables";
pub const NO_TABLES_MATCH_KEY: &str = "sync.noTablesMatch";
pub const ROW_DIFFS_KEY: &str = "sync.rowDiffs";
pub const MAPPING_INCOMPATIBLE_KEY: &str = "sync.mappingIncompatible";
pub const IDENTICAL_KEY: &str = "sync.identical";
pub const TABLE_ITEM_TEST_ID: &str = "data-sync-table-item";

pub fn filter_label_key(filter: TableDiffFilter) -> String {
    format!("sync.filter.{}", filter.as_str())
}

pub fn filter_test_id(filter: TableDiffFilter) -> String {
    format!("data-sync-filter-{}", filter.as_str())
}

pub fn build_table_list_view(
    rows: &[TableListRow],
    filter: TableDiffFilter,
    search: &str,
    selected_table_key: Option<&str>,
) -> TableListView {
    let filters = DATA_SYNC_FILTERS
        .iter()
        .map(|value| FilterButtonView {
            filter: *value,
            test_id: filter_test_id(*value),
            pressed: *value == filter,
            variant: if *value == filter {
                "secondary"
            } else {
                "ghost"
            },
            label_key: filter_label_key(*value),
        })
        .collect();

    let items: Vec<TableListItemView> = filter::filter_table_rows(rows, filter, search)
        .into_iter()
        .map(|index| {
            let row = &rows[index];
            let key = filter::table_key(row);
            let badge =
                if row.status == TableSyncStatus::Matched && filter::table_has_row_diffs(row) {
                    TableItemBadge::RowDiffs(filter::row_diff_counts(row))
                } else if row.status == TableSyncStatus::Incompatible {
                    TableItemBadge::Incompatible
                } else {
                    TableItemBadge::Identical
                };
            TableListItemView {
                display_name: filter::display_table_name(row),
                active: selected_table_key == Some(key.as_str()),
                key,
                badge,
                test_id: TABLE_ITEM_TEST_ID,
            }
        })
        .collect();

    TableListView {
        search_placeholder_key: SEARCH_PLACEHOLDER_KEY,
        empty_message_key: NO_TABLES_MATCH_KEY,
        filters,
        empty: items.is_empty(),
        items,
    }
}

pub const TABLE_LIST_PANEL_SURFACE: [&str; 7] = [
    "TableListPanel",
    "buildTableListView",
    "filterTableRows",
    "rowDiffCounts",
    "tableHasRowDiffs",
    "tableIsUnchanged",
    "TABLE_LIST_PANEL_GAP",
];

pub const TABLE_LIST_PANEL_GAP: [&str; 6] = [
    "React DOM: Input, Button, button and data-testid markers",
    "useI18n().t translations and their keys",
    "cn class merging and Tailwind class literals",
    "onFilterChange / onSearchChange / onSelectTable callbacks",
    "Search icon from lucide-react",
    "scroll container and focus handling",
];

#[cfg(test)]
mod tests {
    use crate::DataSyncOperation;

    use super::*;

    fn rows() -> Vec<TableListRow> {
        vec![
            TableListRow::matched("orders", vec![DataSyncOperation::Insert]),
            TableListRow::matched("users", vec![DataSyncOperation::Update]),
            TableListRow {
                status: TableSyncStatus::Incompatible,
                ..TableListRow::matched("bad", Vec::new())
            },
        ]
    }

    #[test]
    fn filter_buttons_pin_keys_testids_and_active_variant() {
        let view = build_table_list_view(&rows(), TableDiffFilter::Update, "", None);
        assert_eq!(view.filters.len(), 6);
        assert_eq!(view.filters[0].filter, TableDiffFilter::All);
        assert_eq!(view.filters[0].test_id, "data-sync-filter-all");
        assert_eq!(view.filters[0].label_key, "sync.filter.all");
        let active = view
            .filters
            .iter()
            .find(|item| item.pressed)
            .expect("one pressed");
        assert_eq!(active.filter, TableDiffFilter::Update);
        assert_eq!(active.variant, "secondary");
        assert_eq!(active.test_id, "data-sync-filter-update");
        assert!(view.filters.iter().any(|item| item.variant == "ghost"));
        assert_eq!(view.search_placeholder_key, "common.searchTables");
    }

    #[test]
    fn items_badges_and_active_selection_are_observable() {
        let view = build_table_list_view(&rows(), TableDiffFilter::All, "", Some("users"));
        assert_eq!(view.items.len(), 3);
        assert_eq!(view.items[0].key, "orders");
        assert_eq!(view.items[0].test_id, "data-sync-table-item");
        assert_eq!(
            view.items[0].badge,
            TableItemBadge::RowDiffs(RowDiffCounts {
                inserts: 1,
                updates: 0,
                deletes: 0,
                unchanged: 0,
            })
        );
        assert!(view.items[1].active);
        assert!(!view.items[0].active);
        assert_eq!(view.items[2].badge, TableItemBadge::Incompatible);
        assert!(!view.empty);
    }

    #[test]
    fn diff_free_matched_rows_show_the_identical_badge() {
        let view = build_table_list_view(
            &[TableListRow::matched("clean", Vec::new())],
            TableDiffFilter::All,
            "",
            None,
        );
        assert_eq!(view.items[0].badge, TableItemBadge::Identical);
    }

    #[test]
    fn empty_state_is_derived_from_the_filtered_list() {
        let view = build_table_list_view(&rows(), TableDiffFilter::Delete, "", None);
        assert!(view.empty);
        assert!(view.items.is_empty());
        assert_eq!(view.empty_message_key, "sync.noTablesMatch");
        let searched = build_table_list_view(&rows(), TableDiffFilter::All, "nope", None);
        assert!(searched.empty);
    }

    #[test]
    fn surface_and_gap_are_pinned_and_root_reachable() {
        assert_eq!(TABLE_LIST_PANEL_SURFACE.len(), 7);
        assert_eq!(TABLE_LIST_PANEL_GAP.len(), 6);
        assert_eq!(crate::TABLE_LIST_PANEL_SURFACE.len(), 7);
        assert_eq!(crate::TABLE_LIST_PANEL_GAP.len(), 6);
        assert_eq!(ROW_DIFFS_KEY, "sync.rowDiffs");
        assert_eq!(IDENTICAL_KEY, "sync.identical");
    }
}
