use crate::{row_key_string, DataSyncOperation, SyncOptions};

pub const SELECTED_ROW_TOKEN_SEPARATOR: char = '\0';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataSyncSelectionScopeMode {
    All,
    Defaults,
}

impl DataSyncSelectionScopeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Defaults => "defaults",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataSyncTableSelection {
    pub source_table: String,
    pub target_table: String,
    pub selection_mode: DataSyncSelectionScopeMode,
    pub operations: Vec<DataSyncOperation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataSyncSelectedRow {
    pub source_table: String,
    pub target_table: String,
    pub operation: DataSyncOperation,
    pub key: Vec<serde_json::Value>,
}

pub fn selected_row_token(row: &DataSyncSelectedRow) -> String {
    [
        row.source_table.as_str(),
        row.target_table.as_str(),
        row.operation.as_str(),
        &row_key_string(&row.key),
    ]
    .join(&SELECTED_ROW_TOKEN_SEPARATOR.to_string())
}

pub fn default_row_selected(operation: DataSyncOperation, options: &SyncOptions) -> bool {
    match operation {
        DataSyncOperation::Insert => options.insert,
        DataSyncOperation::Update => options.update,
        DataSyncOperation::Delete | DataSyncOperation::Unchanged => false,
    }
}

pub fn scope_selects_operation(
    scope: &DataSyncTableSelection,
    operation: DataSyncOperation,
    options: &SyncOptions,
) -> bool {
    operation != DataSyncOperation::Unchanged
        && scope.operations.contains(&operation)
        && (scope.selection_mode == DataSyncSelectionScopeMode::All
            || default_row_selected(operation, options))
}

pub fn scope_for_operation(
    scopes: &[DataSyncTableSelection],
    source_table: &str,
    target_table: &str,
    operation: DataSyncOperation,
) -> Option<DataSyncTableSelection> {
    scopes
        .iter()
        .find(|scope| {
            scope.source_table == source_table
                && scope.target_table == target_table
                && scope.operations.contains(&operation)
        })
        .cloned()
}

pub const SELECTION_SCOPE_SURFACE: [&str; 6] = [
    "selectedRowToken",
    "defaultRowSelected",
    "scopeSelectsOperation",
    "scopeForOperation",
    "DataSyncTableSelection",
    "SELECTION_SCOPE_GAP",
];

pub const SELECTION_SCOPE_GAP: [&str; 3] = [
    "the backend-client wire types for DataSyncSelection",
    "the store that applies scopes to a comparison page",
    "excludedRows are carried but not consulted by these three pure helpers",
];

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scope(
        mode: DataSyncSelectionScopeMode,
        operations: Vec<DataSyncOperation>,
    ) -> DataSyncTableSelection {
        DataSyncTableSelection {
            source_table: "orders".to_owned(),
            target_table: "orders".to_owned(),
            selection_mode: mode,
            operations,
        }
    }

    fn options() -> SyncOptions {
        SyncOptions {
            insert: true,
            update: false,
            delete: false,
        }
    }

    #[test]
    fn token_joins_identity_parts_with_null_separators() {
        let row = DataSyncSelectedRow {
            source_table: "src".to_owned(),
            target_table: "tgt".to_owned(),
            operation: DataSyncOperation::Insert,
            key: vec![json!(1), json!("a")],
        };
        assert_eq!(
            selected_row_token(&row),
            "src\u{0}tgt\u{0}INSERT\u{0}[1,\"a\"]"
        );
    }

    #[test]
    fn defaults_follow_the_sync_options() {
        assert!(default_row_selected(DataSyncOperation::Insert, &options()));
        assert!(!default_row_selected(DataSyncOperation::Update, &options()));
        assert!(!default_row_selected(DataSyncOperation::Delete, &options()));
        assert!(!default_row_selected(
            DataSyncOperation::Unchanged,
            &options()
        ));
        let all = SyncOptions {
            insert: true,
            update: true,
            delete: true,
        };
        assert!(default_row_selected(DataSyncOperation::Update, &all));
        assert!(!default_row_selected(DataSyncOperation::Delete, &all));
    }

    #[test]
    fn all_mode_covers_every_listed_operation_but_never_unchanged() {
        let all = scope(
            DataSyncSelectionScopeMode::All,
            vec![DataSyncOperation::Insert, DataSyncOperation::Delete],
        );
        assert!(scope_selects_operation(
            &all,
            DataSyncOperation::Insert,
            &options()
        ));
        assert!(scope_selects_operation(
            &all,
            DataSyncOperation::Delete,
            &options()
        ));
        assert!(!scope_selects_operation(
            &all,
            DataSyncOperation::Update,
            &options()
        ));
        assert!(!scope_selects_operation(
            &all,
            DataSyncOperation::Unchanged,
            &options()
        ));
        assert_eq!(DataSyncSelectionScopeMode::All.as_str(), "all");
        assert_eq!(DataSyncSelectionScopeMode::Defaults.as_str(), "defaults");
    }

    #[test]
    fn defaults_mode_honors_the_option_flags() {
        let defaults = scope(
            DataSyncSelectionScopeMode::Defaults,
            vec![DataSyncOperation::Insert, DataSyncOperation::Update],
        );
        assert!(scope_selects_operation(
            &defaults,
            DataSyncOperation::Insert,
            &options()
        ));
        assert!(!scope_selects_operation(
            &defaults,
            DataSyncOperation::Update,
            &options()
        ));
        let all_on = SyncOptions {
            insert: true,
            update: true,
            delete: false,
        };
        assert!(scope_selects_operation(
            &defaults,
            DataSyncOperation::Update,
            &all_on
        ));
    }

    #[test]
    fn scope_lookup_matches_table_pair_and_operation_and_returns_the_first() {
        let scopes = [
            scope(
                DataSyncSelectionScopeMode::All,
                vec![DataSyncOperation::Insert],
            ),
            scope(
                DataSyncSelectionScopeMode::Defaults,
                vec![DataSyncOperation::Insert, DataSyncOperation::Update],
            ),
        ];
        let found = scope_for_operation(&scopes, "orders", "orders", DataSyncOperation::Insert)
            .expect("a scope exists");
        assert_eq!(found.selection_mode, DataSyncSelectionScopeMode::All);
        assert!(
            scope_for_operation(&scopes, "other", "orders", DataSyncOperation::Insert).is_none()
        );
        assert!(
            scope_for_operation(&scopes, "orders", "orders", DataSyncOperation::Delete).is_none()
        );
        assert!(
            scope_for_operation(&scopes, "orders", "orders", DataSyncOperation::Unchanged)
                .is_none()
        );
    }

    #[test]
    fn surface_and_gap_are_pinned_and_root_reachable() {
        assert_eq!(SELECTION_SCOPE_SURFACE.len(), 6);
        assert_eq!(SELECTION_SCOPE_GAP.len(), 3);
        assert_eq!(crate::SELECTION_SCOPE_SURFACE.len(), 6);
        assert_eq!(crate::SELECTION_SCOPE_GAP.len(), 3);
    }
}
