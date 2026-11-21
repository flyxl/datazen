use crate::{
    apply_options_to_rows, operation_allowed, ComparisonTableRow, DataSyncOperation,
    DataSyncSyncState, SelectedComparisonRow, SyncOptions,
};

pub const DEFAULT_CONFLICT_POLICY: &str = "abort";
pub const SYNC_STEP_PREVIEW: &str = "preview";
pub const SYNC_STEP_RESULT: &str = "result";
pub const SYNC_STEP_SETUP: &str = "setup";

#[derive(Clone, Debug, PartialEq)]
pub struct ScopeExclusion {
    pub operation: DataSyncOperation,
    pub key: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableScopeWithExclusions {
    pub source_table: String,
    pub target_table: String,
    pub operations: Vec<DataSyncOperation>,
    pub excluded_rows: Vec<ScopeExclusion>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptionsChangeInput {
    pub previous_policy: Option<String>,
    pub next_policy: Option<String>,
    pub sync_state: DataSyncSyncState,
    pub step: String,
    pub options: SyncOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptionsChangeEffects {
    pub comparison_dropped: bool,
    pub mappings: Vec<ComparisonTableRow>,
    pub selected_rows: Vec<SelectedComparisonRow>,
    pub scopes: Vec<TableScopeWithExclusions>,
    pub sync_state: DataSyncSyncState,
    pub step: String,
}

pub fn invalidates_comparison(input: &OptionsChangeInput) -> bool {
    let previous = input
        .previous_policy
        .as_deref()
        .unwrap_or(DEFAULT_CONFLICT_POLICY);
    let next = input
        .next_policy
        .as_deref()
        .unwrap_or(DEFAULT_CONFLICT_POLICY);
    previous != next
        && (input.sync_state == DataSyncSyncState::Compared
            || input.step == SYNC_STEP_PREVIEW
            || input.step == SYNC_STEP_RESULT)
}

pub fn clear_mapping_rows(mappings: &[ComparisonTableRow]) -> Vec<ComparisonTableRow> {
    mappings
        .iter()
        .map(|row| ComparisonTableRow {
            rows: None,
            ..row.clone()
        })
        .collect()
}

pub fn selected_rows_after_options(
    rows: &[SelectedComparisonRow],
    options: &SyncOptions,
) -> Vec<SelectedComparisonRow> {
    rows.iter()
        .filter(|row| match row.2 {
            DataSyncOperation::Insert => options.insert,
            DataSyncOperation::Update => options.update,
            DataSyncOperation::Delete => options.delete,
            DataSyncOperation::Unchanged => false,
        })
        .cloned()
        .collect()
}

pub fn scopes_after_options(
    scopes: &[TableScopeWithExclusions],
    options: &SyncOptions,
) -> Vec<TableScopeWithExclusions> {
    scopes
        .iter()
        .map(|scope| TableScopeWithExclusions {
            operations: scope
                .operations
                .iter()
                .copied()
                .filter(|operation| operation_allowed(*operation, options))
                .collect(),
            excluded_rows: scope
                .excluded_rows
                .iter()
                .filter(|row| operation_allowed(row.operation, options))
                .cloned()
                .collect(),
            ..scope.clone()
        })
        .filter(|scope| !scope.operations.is_empty())
        .collect()
}

pub fn apply_options_change(
    input: &OptionsChangeInput,
    mappings: &[ComparisonTableRow],
    selected_rows: &[SelectedComparisonRow],
    scopes: &[TableScopeWithExclusions],
) -> OptionsChangeEffects {
    if invalidates_comparison(input) {
        return OptionsChangeEffects {
            comparison_dropped: true,
            mappings: clear_mapping_rows(mappings),
            selected_rows: Vec::new(),
            scopes: Vec::new(),
            sync_state: DataSyncSyncState::Idle,
            step: SYNC_STEP_SETUP.to_owned(),
        };
    }
    let remapped = mappings
        .iter()
        .map(|row| match &row.rows {
            Some(rows) => ComparisonTableRow {
                rows: Some(apply_options_to_rows(rows, &input.options)),
                ..row.clone()
            },
            None => row.clone(),
        })
        .collect();
    OptionsChangeEffects {
        comparison_dropped: false,
        mappings: remapped,
        selected_rows: selected_rows_after_options(selected_rows, &input.options),
        scopes: scopes_after_options(scopes, &input.options),
        sync_state: input.sync_state,
        step: input.step.clone(),
    }
}

pub fn confirm_delete(options: &SyncOptions) -> SyncOptions {
    SyncOptions {
        delete: true,
        ..*options
    }
}

pub const DATA_SYNC_OPTIONS_SURFACE: [&str; 8] = [
    "handleOptionsChange",
    "handleEnableDelete",
    "confirmEnableDelete",
    "invalidatesComparison",
    "selectedRowsAfterOptions",
    "scopesAfterOptions",
    "applyOptionsChange",
    "DATA_SYNC_OPTIONS_GAP",
];

pub const DATA_SYNC_OPTIONS_GAP: [&str; 5] = [
    "React hook state and useCallback wiring",
    "the delete-confirmation dialog and its open flag",
    "wizard setStep / setSyncState / setMappingResults callbacks",
    "selection setSelectedRows / setTableSelections callbacks",
    "SyncOptions conflictPolicy beyond the default abort",
];

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::ComparisonRow;

    fn input() -> OptionsChangeInput {
        OptionsChangeInput {
            previous_policy: None,
            next_policy: None,
            sync_state: DataSyncSyncState::Compared,
            step: "compare".to_owned(),
            options: SyncOptions {
                insert: true,
                update: true,
                delete: false,
            },
        }
    }

    fn mapped(name: &str) -> ComparisonTableRow {
        ComparisonTableRow {
            source_table: name.to_owned(),
            target_table: name.to_owned(),
            status: crate::ComparisonTableStatus::Matched,
            source_filter: None,
            rows: Some(vec![ComparisonRow {
                operation: DataSyncOperation::Insert,
                key: vec![json!(1)],
                selected: false,
            }]),
        }
    }

    fn selected(op: DataSyncOperation) -> SelectedComparisonRow {
        ("t".to_owned(), "t".to_owned(), op, vec![json!(1)])
    }

    fn scope() -> TableScopeWithExclusions {
        TableScopeWithExclusions {
            source_table: "t".to_owned(),
            target_table: "t".to_owned(),
            operations: vec![DataSyncOperation::Insert, DataSyncOperation::Delete],
            excluded_rows: vec![
                ScopeExclusion {
                    operation: DataSyncOperation::Insert,
                    key: vec![json!(1)],
                },
                ScopeExclusion {
                    operation: DataSyncOperation::Delete,
                    key: vec![json!(2)],
                },
            ],
        }
    }

    #[test]
    fn policy_change_invalidates_only_in_review_phases() {
        let mut item = input();
        item.next_policy = Some("skip".to_owned());
        assert!(invalidates_comparison(&item));
        item.sync_state = DataSyncSyncState::Idle;
        item.step = "compare".to_owned();
        assert!(!invalidates_comparison(&item));
        item.step = SYNC_STEP_PREVIEW.to_owned();
        assert!(invalidates_comparison(&item));
        item.step = SYNC_STEP_RESULT.to_owned();
        assert!(invalidates_comparison(&item));
        let same = input();
        assert!(!invalidates_comparison(&same));
    }

    #[test]
    fn invalidation_drops_rows_and_returns_to_setup() {
        let mut item = input();
        item.next_policy = Some("skip".to_owned());
        let effects = apply_options_change(
            &item,
            &[mapped("t")],
            &[selected(DataSyncOperation::Insert)],
            &[scope()],
        );
        assert!(effects.comparison_dropped);
        assert!(effects.mappings[0].rows.is_none());
        assert!(effects.selected_rows.is_empty());
        assert!(effects.scopes.is_empty());
        assert_eq!(effects.sync_state, DataSyncSyncState::Idle);
        assert_eq!(effects.step, "setup");
    }

    #[test]
    fn non_invalidating_change_remaps_rows_and_filters_selections() {
        let effects = apply_options_change(
            &input(),
            &[mapped("t")],
            &[
                selected(DataSyncOperation::Insert),
                selected(DataSyncOperation::Delete),
                selected(DataSyncOperation::Unchanged),
            ],
            &[scope()],
        );
        assert!(!effects.comparison_dropped);
        assert!(effects.mappings[0]
            .rows
            .as_ref()
            .is_some_and(|rows| rows[0].selected));
        assert_eq!(effects.selected_rows.len(), 1);
        assert_eq!(effects.selected_rows[0].2, DataSyncOperation::Insert);
        assert_eq!(effects.scopes[0].operations, [DataSyncOperation::Insert]);
        assert_eq!(effects.scopes[0].excluded_rows.len(), 1);
    }

    #[test]
    fn scopes_with_no_allowed_operations_are_removed() {
        let mut item = input();
        item.options = SyncOptions {
            insert: false,
            update: false,
            delete: false,
        };
        let scopes = scopes_after_options(&[scope()], &item.options);
        assert!(scopes.is_empty());
    }

    #[test]
    fn confirming_delete_sets_only_that_flag() {
        let options = SyncOptions {
            insert: true,
            update: false,
            delete: false,
        };
        let confirmed = confirm_delete(&options);
        assert!(confirmed.delete);
        assert!(confirmed.insert);
        assert!(!confirmed.update);
        assert!(!options.delete);
        assert_eq!(DATA_SYNC_OPTIONS_SURFACE.len(), 8);
        assert_eq!(DATA_SYNC_OPTIONS_GAP.len(), 5);
        assert_eq!(crate::DATA_SYNC_OPTIONS_GAP.len(), 5);
    }
}
