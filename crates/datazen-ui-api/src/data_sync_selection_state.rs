use std::collections::BTreeMap;

use crate::{DataSyncSelectedRow, DataSyncTableSelection};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataSyncPageCursor {
    pub current: Option<String>,
    pub next: Option<String>,
    pub previous: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionScopeKey {
    pub source_id: String,
    pub target_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: String,
    pub target_schema: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionState {
    pub selected_rows: Vec<DataSyncSelectedRow>,
    pub table_selections: Vec<DataSyncTableSelection>,
    pub page_loading: bool,
    pub page_index: BTreeMap<String, i64>,
    pub page_cursors: BTreeMap<String, DataSyncPageCursor>,
}

pub trait SelectionRefs {
    fn set_selected_rows(&mut self, rows: &[DataSyncSelectedRow]);

    fn set_table_selections(&mut self, scopes: &[DataSyncTableSelection]);

    fn clear_loaded_page(&mut self);
}

pub struct DataSyncSelectionState {
    pub state: SelectionState,
    pub scope: SelectionScopeKey,
}

impl DataSyncSelectionState {
    pub fn new(scope: SelectionScopeKey) -> Self {
        Self {
            state: SelectionState::default(),
            scope,
        }
    }

    pub fn sync_refs(&self, refs: &mut dyn SelectionRefs) {
        refs.set_selected_rows(&self.state.selected_rows);
        refs.set_table_selections(&self.state.table_selections);
    }

    pub fn on_scope_change(
        &mut self,
        scope: SelectionScopeKey,
        refs: &mut dyn SelectionRefs,
    ) -> bool {
        if scope == self.scope {
            return false;
        }
        self.scope = scope;
        self.state.selected_rows.clear();
        self.state.table_selections.clear();
        self.state.page_index.clear();
        self.state.page_cursors.clear();
        refs.clear_loaded_page();
        self.sync_refs(refs);
        true
    }

    pub fn set_selected_rows(
        &mut self,
        rows: Vec<DataSyncSelectedRow>,
        refs: &mut dyn SelectionRefs,
    ) {
        self.state.selected_rows = rows;
        refs.set_selected_rows(&self.state.selected_rows);
    }

    pub fn set_table_selections(
        &mut self,
        scopes: Vec<DataSyncTableSelection>,
        refs: &mut dyn SelectionRefs,
    ) {
        self.state.table_selections = scopes;
        refs.set_table_selections(&self.state.table_selections);
    }

    pub fn set_page_loading(&mut self, loading: bool) {
        self.state.page_loading = loading;
    }

    pub fn set_page_index(&mut self, index: BTreeMap<String, i64>) {
        self.state.page_index = index;
    }

    pub fn set_page_cursors(&mut self, cursors: BTreeMap<String, DataSyncPageCursor>) {
        self.state.page_cursors = cursors;
    }
}

pub const DATA_SYNC_SELECTION_STATE_SURFACE: [&str; 8] = [
    "DataSyncPageCursor",
    "SelectionScopeKey",
    "SelectionState",
    "SelectionRefs",
    "syncRefs",
    "onScopeChange",
    "setSelectedRows",
    "DATA_SYNC_SELECTION_STATE_GAP",
];

pub const DATA_SYNC_SELECTION_STATE_GAP: [&str; 4] = [
    "React useState/useEffect and the six-dependency invalidation",
    "DataSyncSharedRefs mutable ref slots are injected as SelectionRefs",
    "SetStateAction functional updaters are immediate-mode in this port",
    "no DOM or IPC calls exist in this module",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DataSyncOperation, DataSyncSelectedRow, DataSyncSelectionScopeMode, DataSyncTableSelection,
    };

    #[derive(Default)]
    struct Refs {
        rows: Vec<Vec<DataSyncSelectedRow>>,
        scopes: Vec<Vec<DataSyncTableSelection>>,
        cleared: usize,
    }

    impl SelectionRefs for Refs {
        fn set_selected_rows(&mut self, rows: &[DataSyncSelectedRow]) {
            self.rows.push(rows.to_vec());
        }

        fn set_table_selections(&mut self, scopes: &[DataSyncTableSelection]) {
            self.scopes.push(scopes.to_vec());
        }

        fn clear_loaded_page(&mut self) {
            self.cleared += 1;
        }
    }

    fn scope() -> SelectionScopeKey {
        SelectionScopeKey {
            source_id: "c1".to_owned(),
            target_id: "c2".to_owned(),
            source_database: "app".to_owned(),
            target_database: "dw".to_owned(),
            source_schema: "public".to_owned(),
            target_schema: "analytics".to_owned(),
        }
    }

    fn row(token: &str) -> DataSyncSelectedRow {
        DataSyncSelectedRow {
            source_table: "t".to_owned(),
            target_table: "t2".to_owned(),
            operation: DataSyncOperation::Update,
            key: vec![serde_json::json!(token)],
        }
    }

    fn table_scope() -> DataSyncTableSelection {
        DataSyncTableSelection {
            source_table: "t".to_owned(),
            target_table: "t2".to_owned(),
            selection_mode: DataSyncSelectionScopeMode::All,
            operations: vec![DataSyncOperation::Update],
        }
    }

    #[test]
    fn a_new_state_is_empty_and_mirrors_the_refs_once() {
        let mut refs = Refs::default();
        let state = DataSyncSelectionState::new(scope());
        assert!(state.state.selected_rows.is_empty());
        assert!(state.state.table_selections.is_empty());
        assert!(!state.state.page_loading);
        assert!(state.state.page_index.is_empty());
        assert!(state.state.page_cursors.is_empty());
        state.sync_refs(&mut refs);
        assert_eq!(refs.rows.len(), 1);
        assert_eq!(refs.scopes.len(), 1);
        assert_eq!(refs.cleared, 0);
    }

    #[test]
    fn an_unchanged_scope_keeps_everything() {
        let mut refs = Refs::default();
        let mut state = DataSyncSelectionState::new(scope());
        state.set_selected_rows(vec![row("1")], &mut refs);
        state.set_page_loading(true);
        state.set_page_index(BTreeMap::from([("t".to_owned(), 2)]));
        assert!(!state.on_scope_change(scope(), &mut refs));
        assert_eq!(state.state.selected_rows.len(), 1);
        assert!(state.state.page_loading);
        assert_eq!(state.state.page_index["t"], 2);
        assert_eq!(refs.cleared, 0);
    }

    #[test]
    fn each_endpoint_dimension_invalidates_the_selection_only() {
        let fields = [
            "source_id",
            "target_id",
            "source_database",
            "target_database",
            "source_schema",
            "target_schema",
        ];
        for field in fields {
            let mut refs = Refs::default();
            let mut state = DataSyncSelectionState::new(scope());
            state.set_selected_rows(vec![row("1")], &mut refs);
            state.set_table_selections(vec![table_scope()], &mut refs);
            state.set_page_loading(true);
            state.set_page_index(BTreeMap::from([("t".to_owned(), 3)]));
            state.set_page_cursors(BTreeMap::from([(
                "t".to_owned(),
                DataSyncPageCursor {
                    current: Some("c".to_owned()),
                    next: None,
                    previous: vec!["p".to_owned()],
                },
            )]));
            let mut changed = scope();
            match field {
                "source_id" => changed.source_id = "other".to_owned(),
                "target_id" => changed.target_id = "other".to_owned(),
                "source_database" => changed.source_database = "other".to_owned(),
                "target_database" => changed.target_database = "other".to_owned(),
                "source_schema" => changed.source_schema = "other".to_owned(),
                "target_schema" => changed.target_schema = "other".to_owned(),
                _ => {}
            }
            assert!(state.on_scope_change(changed, &mut refs));
            assert!(state.state.selected_rows.is_empty());
            assert!(state.state.table_selections.is_empty());
            assert!(state.state.page_index.is_empty());
            assert!(state.state.page_cursors.is_empty());
            assert!(
                state.state.page_loading,
                "pageLoading is not in the dependency list"
            );
            assert_eq!(refs.cleared, 1);
            assert_eq!(refs.rows.len(), 2);
        }
    }

    #[test]
    fn setters_are_observable_and_refs_stay_in_sync() {
        let mut refs = Refs::default();
        let mut state = DataSyncSelectionState::new(scope());
        state.set_selected_rows(vec![row("a")], &mut refs);
        state.set_table_selections(vec![table_scope()], &mut refs);
        assert_eq!(state.state.selected_rows.len(), 1);
        assert_eq!(state.state.table_selections.len(), 1);
        assert_eq!(refs.rows.last().map(Vec::len), Some(1));
        assert_eq!(refs.scopes.last().map(Vec::len), Some(1));
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(DATA_SYNC_SELECTION_STATE_SURFACE.len(), 8);
        assert!(DATA_SYNC_SELECTION_STATE_SURFACE.contains(&"onScopeChange"));
        assert_eq!(DATA_SYNC_SELECTION_STATE_GAP.len(), 4);
        assert_eq!(crate::DATA_SYNC_SELECTION_STATE_SURFACE.len(), 8);
        assert_eq!(crate::DATA_SYNC_SELECTION_STATE_GAP.len(), 4);
        let _: Option<crate::DataSyncPageCursor> = Some(DataSyncPageCursor::default());
    }
}
