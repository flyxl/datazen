use crate::{
    operation_allowed, selected_row_token, DataSyncOperation, DataSyncSelectionScopeMode,
    SelectedComparisonRow, SyncOptions,
};

use super::{RowChange, TableSelectionScope};

fn same_table(scope: &TableSelectionScope, source: &str, target: &str) -> bool {
    scope.source_table == source && scope.target_table == target
}

fn strip_operation(
    scopes: &[TableSelectionScope],
    source: &str,
    target: &str,
    operation: DataSyncOperation,
) -> Vec<TableSelectionScope> {
    scopes
        .iter()
        .map(|scope| {
            if !same_table(scope, source, target) {
                return scope.clone();
            }
            TableSelectionScope {
                operations: scope
                    .operations
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != operation)
                    .collect(),
                excluded_rows: scope
                    .excluded_rows
                    .iter()
                    .filter(|row| row.operation != operation)
                    .cloned()
                    .collect(),
                ..scope.clone()
            }
        })
        .filter(|scope| !scope.operations.is_empty())
        .collect()
}

pub fn set_table_operation_scope(
    scopes: &[TableSelectionScope],
    source: &str,
    target: &str,
    operation: DataSyncOperation,
    options: &SyncOptions,
) -> Vec<TableSelectionScope> {
    if !operation_allowed(operation, options) {
        return scopes.to_vec();
    }
    let all_scope = scopes.iter().find(|scope| {
        same_table(scope, source, target) && scope.selection_mode == DataSyncSelectionScopeMode::All
    });
    if all_scope.is_some_and(|scope| scope.operations.contains(&operation)) {
        return scopes.to_vec();
    }
    let had_all = all_scope.is_some();
    let cleaned = strip_operation(scopes, source, target, operation);
    if had_all {
        return cleaned
            .into_iter()
            .map(|scope| {
                if same_table(&scope, source, target)
                    && scope.selection_mode == DataSyncSelectionScopeMode::All
                {
                    let mut operations = scope.operations.clone();
                    operations.push(operation);
                    TableSelectionScope {
                        operations,
                        ..scope
                    }
                } else {
                    scope
                }
            })
            .collect();
    }
    let mut result = cleaned;
    result.push(TableSelectionScope {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        selection_mode: DataSyncSelectionScopeMode::All,
        operations: vec![operation],
        excluded_rows: Vec::new(),
    });
    result
}

pub fn clear_table_operation_scope(
    scopes: &[TableSelectionScope],
    source: &str,
    target: &str,
    operation: DataSyncOperation,
) -> Vec<TableSelectionScope> {
    let cleaned = strip_operation(scopes, source, target, operation);
    let had_defaults = cleaned.iter().any(|scope| {
        same_table(scope, source, target)
            && scope.selection_mode == DataSyncSelectionScopeMode::Defaults
    });
    if had_defaults {
        return cleaned
            .into_iter()
            .map(|scope| {
                if same_table(&scope, source, target)
                    && scope.selection_mode == DataSyncSelectionScopeMode::Defaults
                {
                    let mut operations = scope.operations.clone();
                    operations.push(operation);
                    TableSelectionScope {
                        operations,
                        ..scope
                    }
                } else {
                    scope
                }
            })
            .collect();
    }
    let mut result = cleaned;
    result.push(TableSelectionScope {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        selection_mode: DataSyncSelectionScopeMode::Defaults,
        operations: vec![operation],
        excluded_rows: Vec::new(),
    });
    result
}

pub fn is_table_operation_scoped(
    scopes: &[TableSelectionScope],
    source: &str,
    target: &str,
    operation: DataSyncOperation,
) -> bool {
    scopes
        .iter()
        .any(|scope| same_table(scope, source, target) && scope.operations.contains(&operation))
}

fn covering_scope<'a>(
    scopes: &'a [TableSelectionScope],
    source: &str,
    target: &str,
    operation: DataSyncOperation,
    options: &SyncOptions,
) -> Option<&'a TableSelectionScope> {
    scopes.iter().find(|scope| {
        same_table(scope, source, target)
            && scope.operations.contains(&operation)
            && scope.selects_operation(operation, options)
    })
}

pub fn reconcile_selected_rows(
    previous: &[SelectedComparisonRow],
    source: &str,
    target: &str,
    rows: &[RowChange],
    scopes: &[TableSelectionScope],
    options: &SyncOptions,
) -> Vec<SelectedComparisonRow> {
    let page_tokens: std::collections::BTreeSet<String> = rows
        .iter()
        .filter(|row| row.operation != DataSyncOperation::Unchanged)
        .map(|row| {
            selected_row_token(&crate::DataSyncSelectedRow {
                source_table: source.to_owned(),
                target_table: target.to_owned(),
                operation: row.operation,
                key: row.key.clone(),
            })
        })
        .collect();

    let retained: Vec<SelectedComparisonRow> = previous
        .iter()
        .filter(|row| {
            if !page_tokens.contains(&selected_row_token(&crate::DataSyncSelectedRow {
                source_table: row.0.clone(),
                target_table: row.1.clone(),
                operation: row.2,
                key: row.3.clone(),
            })) {
                return true;
            }
            covering_scope(scopes, source, target, row.2, options).is_some()
        })
        .cloned()
        .collect();

    let next: Vec<SelectedComparisonRow> = rows
        .iter()
        .filter(|row| row.selected && row.operation != DataSyncOperation::Unchanged)
        .filter(|row| covering_scope(scopes, source, target, row.operation, options).is_none())
        .map(|row| {
            (
                source.to_owned(),
                target.to_owned(),
                row.operation,
                row.key.clone(),
            )
        })
        .collect();

    let known: std::collections::BTreeSet<String> = retained
        .iter()
        .map(|row| {
            selected_row_token(&crate::DataSyncSelectedRow {
                source_table: row.0.clone(),
                target_table: row.1.clone(),
                operation: row.2,
                key: row.3.clone(),
            })
        })
        .collect();
    let mut result = retained;
    result.extend(next.into_iter().filter(|row| {
        !known.contains(&selected_row_token(&crate::DataSyncSelectedRow {
            source_table: row.0.clone(),
            target_table: row.1.clone(),
            operation: row.2,
            key: row.3.clone(),
        }))
    }));
    result
}

pub fn update_exclusions(
    scopes: &[TableSelectionScope],
    source: &str,
    target: &str,
    rows: &[RowChange],
    options: &SyncOptions,
) -> Vec<TableSelectionScope> {
    scopes
        .iter()
        .map(|scope| {
            if !same_table(scope, source, target) {
                return scope.clone();
            }
            let candidate_rows: Vec<&RowChange> = rows
                .iter()
                .filter(|row| scope.operations.contains(&row.operation))
                .collect();
            let key_of = |operation: DataSyncOperation, key: &[serde_json::Value]| {
                format!("{}:{}", operation.as_str(), crate::row_key_string(key))
            };
            let exclusions: Vec<crate::data_sync_options::ScopeExclusion> = scope
                .excluded_rows
                .iter()
                .filter(|excluded| {
                    !candidate_rows.iter().any(|row| {
                        key_of(row.operation, &row.key) == key_of(excluded.operation, &excluded.key)
                    })
                })
                .cloned()
                .collect();
            let mut additions: Vec<crate::data_sync_options::ScopeExclusion> = Vec::new();
            for row in candidate_rows {
                if scope.selects_operation(row.operation, options) && !row.selected {
                    let exclusion = crate::data_sync_options::ScopeExclusion {
                        operation: row.operation,
                        key: row.key.clone(),
                    };
                    let candidate_key = key_of(exclusion.operation, &exclusion.key);
                    if !exclusions
                        .iter()
                        .any(|existing| key_of(existing.operation, &existing.key) == candidate_key)
                    {
                        additions.push(exclusion);
                    }
                }
            }
            let mut all = exclusions;
            all.extend(additions);
            TableSelectionScope {
                excluded_rows: all,
                ..scope.clone()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::DataSyncSelectionScopeMode;

    use super::TableSelectionScope;

    use super::*;

    fn options() -> SyncOptions {
        SyncOptions {
            insert: true,
            update: false,
            delete: false,
        }
    }

    fn scope(
        mode: DataSyncSelectionScopeMode,
        operations: Vec<DataSyncOperation>,
    ) -> TableSelectionScope {
        TableSelectionScope {
            source_table: "s".to_owned(),
            target_table: "t".to_owned(),
            selection_mode: mode,
            operations,
            excluded_rows: Vec::new(),
        }
    }

    #[test]
    fn setting_a_scope_is_idempotent_and_removes_conflicting_ops() {
        let scopes = vec![scope(
            DataSyncSelectionScopeMode::Defaults,
            vec![DataSyncOperation::Insert],
        )];
        let set =
            set_table_operation_scope(&scopes, "s", "t", DataSyncOperation::Insert, &options());
        assert_eq!(set.len(), 1);
        assert_eq!(set[0].selection_mode, DataSyncSelectionScopeMode::All);
        let again =
            set_table_operation_scope(&set, "s", "t", DataSyncOperation::Insert, &options());
        assert_eq!(again, set);

        let not_allowed =
            set_table_operation_scope(&scopes, "s", "t", DataSyncOperation::Delete, &options());
        assert_eq!(not_allowed, scopes);
    }

    #[test]
    fn clearing_a_scope_falls_back_to_defaults() {
        let scopes = vec![scope(
            DataSyncSelectionScopeMode::All,
            vec![DataSyncOperation::Insert],
        )];
        let cleared = clear_table_operation_scope(&scopes, "s", "t", DataSyncOperation::Insert);
        assert_eq!(cleared.len(), 1);
        assert_eq!(
            cleared[0].selection_mode,
            DataSyncSelectionScopeMode::Defaults
        );
        assert!(!is_table_operation_scoped(
            &scopes,
            "s",
            "t",
            DataSyncOperation::Delete
        ));
        assert!(is_table_operation_scoped(
            &scopes,
            "s",
            "t",
            DataSyncOperation::Insert
        ));
    }

    #[test]
    fn row_reconciliation_prefers_scopes_and_deduplicates() {
        let row = |op, selected| RowChange {
            operation: op,
            key: vec![serde_json::json!(1)],
            selected,
        };
        let scopes = vec![scope(
            DataSyncSelectionScopeMode::All,
            vec![DataSyncOperation::Insert],
        )];
        let previous: Vec<SelectedComparisonRow> = vec![(
            "s".to_owned(),
            "t".to_owned(),
            DataSyncOperation::Insert,
            vec![serde_json::json!(1)],
        )];
        let result = reconcile_selected_rows(
            &previous,
            "s",
            "t",
            &[row(DataSyncOperation::Insert, false)],
            &scopes,
            &options(),
        );
        assert_eq!(result.len(), 1);

        let no_scope = reconcile_selected_rows(
            &[],
            "s",
            "t",
            &[
                row(DataSyncOperation::Insert, true),
                row(DataSyncOperation::Insert, true),
            ],
            &[],
            &options(),
        );
        assert_eq!(no_scope.len(), 2);
    }

    #[test]
    fn exclusions_drop_stale_and_add_unselected_scoped_rows() {
        let mut scopes = vec![scope(
            DataSyncSelectionScopeMode::All,
            vec![DataSyncOperation::Insert],
        )];
        scopes[0]
            .excluded_rows
            .push(crate::data_sync_options::ScopeExclusion {
                operation: DataSyncOperation::Insert,
                key: vec![serde_json::json!(99)],
            });
        let rows = [RowChange {
            operation: DataSyncOperation::Insert,
            key: vec![serde_json::json!(1)],
            selected: false,
        }];
        let updated = update_exclusions(&scopes, "s", "t", &rows, &options());
        assert_eq!(updated[0].excluded_rows.len(), 2);
        assert_eq!(updated[0].excluded_rows[0].key, vec![serde_json::json!(99)]);
        assert_eq!(updated[0].excluded_rows[1].key, vec![serde_json::json!(1)]);
    }
}
