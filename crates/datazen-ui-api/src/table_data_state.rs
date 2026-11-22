use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{types::ColumnSchema, FilterCondition, GridSelection, PendingRowChange, SortCondition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultOrderOverride {
    Inherit,
    PrimaryKeyAsc,
    PrimaryKeyDesc,
    None,
}

impl DefaultOrderOverride {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::PrimaryKeyAsc => "primaryKeyAsc",
            Self::PrimaryKeyDesc => "primaryKeyDesc",
            Self::None => "none",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "primaryKeyAsc" => Self::PrimaryKeyAsc,
            "primaryKeyDesc" => Self::PrimaryKeyDesc,
            "none" => Self::None,
            _ => Self::Inherit,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterLogic {
    And,
    Or,
}

impl FilterLogic {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
        }
    }

    pub fn from_str(value: &str) -> Self {
        if value == "or" {
            Self::Or
        } else {
            Self::And
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Structured,
    AnyColumn,
    RawSql,
}

impl FilterMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Structured => "structured",
            Self::AnyColumn => "anyColumn",
            Self::RawSql => "rawSql",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "anyColumn" => Self::AnyColumn,
            "rawSql" => Self::RawSql,
            _ => Self::Structured,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellEditLocation {
    pub row: u64,
    pub col: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppliedFilter<'a> {
    Structured {
        conditions: &'a [FilterCondition],
        logic: FilterLogic,
    },
    Raw {
        predicate: &'a str,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableState {
    pub context: Option<Value>,
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<BTreeMap<String, Value>>,
    pub total_rows: u64,
    pub total_rows_kind: crate::types::CountStrategy,
    pub default_order_override: DefaultOrderOverride,
    pub page: u64,
    pub page_size: u64,
    pub keyset: Value,
    pub drift_recount_page: Option<u64>,
    pub filters: Vec<FilterCondition>,
    pub filter_logic: FilterLogic,
    pub draft_filters: Vec<FilterCondition>,
    pub draft_filter_logic: FilterLogic,
    pub filter_panel_open: bool,
    pub filter_mode: FilterMode,
    pub any_column_draft: String,
    pub raw_where_draft: String,
    pub raw_where: Option<String>,
    pub sorts: Vec<SortCondition>,
    pub column_order: Option<Vec<String>>,
    pub frozen_column_count: u64,
    pub pending_changes: BTreeMap<String, PendingRowChange>,
    pub draft_inserts: BTreeMap<String, Value>,
    pub draft_insert_order: Vec<String>,
    pub highlighted_new_row_key: Option<String>,
    pub row_identity_anchors: BTreeMap<u64, String>,
    pub preview_plan: Option<Value>,
    pub pending_status: Value,
    pub selected_rows: BTreeSet<u64>,
    pub last_selected_index: Option<u64>,
    pub grid_selection: GridSelection,
    pub editing_cell: Option<CellEditLocation>,
    pub enum_candidates: BTreeMap<String, Vec<String>>,
    pub detail_row_index: Option<u64>,
    pub loading: bool,
    pub request_revision: u64,
    pub loading_revision: Option<u64>,
    pub error: Option<String>,
    pub visible_columns: Option<Vec<String>>,
}

impl TableState {
    pub fn empty() -> Self {
        Self {
            context: None,
            columns: Vec::new(),
            rows: Vec::new(),
            total_rows: 0,
            total_rows_kind: crate::types::CountStrategy::Unsupported,
            default_order_override: DefaultOrderOverride::Inherit,
            page: 0,
            page_size: 0,
            keyset: Value::Null,
            drift_recount_page: None,
            filters: Vec::new(),
            filter_logic: FilterLogic::And,
            draft_filters: Vec::new(),
            draft_filter_logic: FilterLogic::And,
            filter_panel_open: false,
            filter_mode: FilterMode::Structured,
            any_column_draft: String::new(),
            raw_where_draft: String::new(),
            raw_where: None,
            sorts: Vec::new(),
            column_order: None,
            frozen_column_count: 0,
            pending_changes: BTreeMap::new(),
            draft_inserts: BTreeMap::new(),
            draft_insert_order: Vec::new(),
            highlighted_new_row_key: None,
            row_identity_anchors: BTreeMap::new(),
            preview_plan: None,
            pending_status: Value::Null,
            selected_rows: BTreeSet::new(),
            last_selected_index: None,
            grid_selection: GridSelection::default(),
            editing_cell: None,
            enum_candidates: BTreeMap::new(),
            detail_row_index: None,
            loading: false,
            request_revision: 0,
            loading_revision: None,
            error: None,
            visible_columns: None,
        }
    }

    pub fn effective_column_order(&self, displayed: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(order) = &self.column_order {
            for name in order {
                if displayed.iter().any(|column| column == name) && !result.contains(name) {
                    result.push(name.clone());
                }
            }
        }
        for name in displayed {
            if !result.contains(name) {
                result.push(name.clone());
            }
        }
        result
    }

    pub fn limited_frozen_columns(&self, displayed_len: usize) -> u64 {
        self.frozen_column_count.min(displayed_len as u64)
    }

    pub fn applied_filter(&self) -> AppliedFilter<'_> {
        if self.filter_mode == FilterMode::RawSql {
            if let Some(predicate) = self.raw_where.as_deref() {
                return AppliedFilter::Raw { predicate };
            }
        }
        AppliedFilter::Structured {
            conditions: &self.filters,
            logic: self.filter_logic,
        }
    }

    pub fn has_pending_edits(&self) -> bool {
        !self.pending_changes.is_empty() || !self.draft_inserts.is_empty()
    }

    pub fn ordered_draft_inserts(&self) -> Vec<String> {
        self.draft_insert_order
            .iter()
            .filter(|id| self.draft_inserts.contains_key(*id))
            .cloned()
            .collect()
    }

    pub fn revision_is_current(&self) -> bool {
        self.loading_revision.is_none() || self.loading_revision == Some(self.request_revision)
    }
}

pub const TABLE_DATA_STATE_SURFACE: [&str; 9] = [
    "TableState",
    "DefaultOrderOverride",
    "FilterLogic",
    "FilterMode",
    "CellEditLocation",
    "effectiveColumnOrder",
    "appliedFilter",
    "orderedDraftInserts",
    "TABLE_DATA_STATE_GAP",
];

pub const TABLE_DATA_STATE_GAP: [&str; 5] = [
    "TableChangeContext, PendingInsert, PendingStatus, RowChangePlan and KeysetPagingState stay opaque until their ports land",
    "the store actions that own writes to TableState are outside this type-only file",
    "no DOM, IPC or locale calls exist in this module",
    "rows are Record<string, unknown> mirrored as a JSON value map",
    "TableState::empty is a Rust construction convenience, not a React export",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_literals_round_trip_with_first_arm_fallbacks() {
        for (value, literal) in [
            (DefaultOrderOverride::Inherit, "inherit"),
            (DefaultOrderOverride::PrimaryKeyAsc, "primaryKeyAsc"),
            (DefaultOrderOverride::PrimaryKeyDesc, "primaryKeyDesc"),
            (DefaultOrderOverride::None, "none"),
        ] {
            assert_eq!(value.as_str(), literal);
            assert_eq!(DefaultOrderOverride::from_str(literal), value);
        }
        assert_eq!(
            DefaultOrderOverride::from_str("nope"),
            DefaultOrderOverride::Inherit
        );
        assert_eq!(FilterLogic::And.as_str(), "and");
        assert_eq!(FilterLogic::from_str("or"), FilterLogic::Or);
        assert_eq!(FilterLogic::from_str("xor"), FilterLogic::And);
        assert_eq!(FilterMode::AnyColumn.as_str(), "anyColumn");
        assert_eq!(FilterMode::RawSql.as_str(), "rawSql");
        assert_eq!(FilterMode::from_str("rawSql"), FilterMode::RawSql);
        assert_eq!(FilterMode::from_str("nope"), FilterMode::Structured);
    }

    #[test]
    fn empty_state_keeps_the_documented_neutral_defaults() {
        let state = TableState::empty();
        assert_eq!(state.total_rows, 0);
        assert_eq!(
            state.total_rows_kind,
            crate::types::CountStrategy::Unsupported
        );
        assert_eq!(state.default_order_override, DefaultOrderOverride::Inherit);
        assert_eq!(state.page, 0);
        assert!(state.column_order.is_none());
        assert_eq!(state.frozen_column_count, 0);
        assert_eq!(state.filter_logic, FilterLogic::And);
        assert_eq!(state.filter_mode, FilterMode::Structured);
        assert_eq!(state.raw_where, None);
        assert!(state.pending_changes.is_empty());
        assert!(state.grid_selection == GridSelection::default());
        assert!(state.revision_is_current());
    }

    #[test]
    fn column_order_is_a_permutation_hint_over_displayed_columns() {
        let mut state = TableState::empty();
        let displayed = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(state.effective_column_order(&displayed), displayed);
        state.column_order = Some(vec!["c".to_owned(), "ghost".to_owned(), "a".to_owned()]);
        assert_eq!(state.effective_column_order(&displayed), ["c", "a", "b"]);
        state.column_order = Some(vec!["b".to_owned(), "b".to_owned()]);
        assert_eq!(state.effective_column_order(&displayed), ["b", "a", "c"]);
    }

    #[test]
    fn frozen_columns_are_limited_by_the_displayed_width() {
        let mut state = TableState::empty();
        state.frozen_column_count = 5;
        assert_eq!(state.limited_frozen_columns(2), 2);
        assert_eq!(state.limited_frozen_columns(9), 5);
    }

    #[test]
    fn applied_filter_prefers_a_live_raw_predicate_only_in_raw_mode() {
        let mut state = TableState::empty();
        state.raw_where = Some("x = 1".to_owned());
        assert!(matches!(
            state.applied_filter(),
            AppliedFilter::Structured { .. }
        ));
        state.filter_mode = FilterMode::RawSql;
        match state.applied_filter() {
            AppliedFilter::Raw { predicate } => assert_eq!(predicate, "x = 1"),
            AppliedFilter::Structured { .. } => panic!("expected the raw channel"),
        }
        state.raw_where = None;
        assert!(matches!(
            state.applied_filter(),
            AppliedFilter::Structured { .. }
        ));
    }

    #[test]
    fn draft_insert_order_ignores_stale_ids_and_pending_tracks_both_maps() {
        let mut state = TableState::empty();
        state
            .draft_inserts
            .insert("d1".to_owned(), serde_json::json!({}));
        state.draft_insert_order = vec!["gone".to_owned(), "d1".to_owned()];
        assert_eq!(state.ordered_draft_inserts(), ["d1"]);
        assert!(state.has_pending_edits());
        state.draft_inserts.clear();
        state.draft_insert_order.clear();
        assert!(!state.has_pending_edits());
    }

    #[test]
    fn loading_revision_tracks_the_latest_request_only() {
        let mut state = TableState::empty();
        state.request_revision = 3;
        assert!(state.revision_is_current());
        state.loading_revision = Some(2);
        assert!(!state.revision_is_current());
        state.loading_revision = Some(3);
        assert!(state.revision_is_current());
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(TABLE_DATA_STATE_SURFACE.len(), 9);
        assert!(TABLE_DATA_STATE_SURFACE.contains(&"TableState"));
        assert_eq!(TABLE_DATA_STATE_GAP.len(), 5);
        assert_eq!(crate::TABLE_DATA_STATE_SURFACE.len(), 9);
        assert_eq!(crate::TABLE_DATA_STATE_GAP.len(), 5);
        let _: Option<crate::TableState> = Some(TableState::empty());
    }
}
