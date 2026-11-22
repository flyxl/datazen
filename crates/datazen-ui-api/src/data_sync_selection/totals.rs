use crate::{
    default_row_selected, operation_allowed, DataSyncOperation, SelectedComparisonRow, SyncOptions,
};

use super::{SelectionTable, TableSelectionScope};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowCounts {
    pub inserts: usize,
    pub updates: usize,
    pub deletes: usize,
}

pub fn row_counts(table: &SelectionTable) -> RowCounts {
    let mut counts = RowCounts::default();
    if let Some(rows) = &table.rows {
        for row in rows {
            match row.operation {
                DataSyncOperation::Insert => counts.inserts += 1,
                DataSyncOperation::Update => counts.updates += 1,
                DataSyncOperation::Delete => counts.deletes += 1,
                DataSyncOperation::Unchanged => {}
            }
        }
    }
    counts
}

fn count_for(operation: DataSyncOperation, counts: &RowCounts) -> usize {
    match operation {
        DataSyncOperation::Insert => counts.inserts,
        DataSyncOperation::Update => counts.updates,
        DataSyncOperation::Delete => counts.deletes,
        DataSyncOperation::Unchanged => 0,
    }
}

fn covers(
    scope: &TableSelectionScope,
    table: &SelectionTable,
    operation: DataSyncOperation,
) -> bool {
    scope.source_table == table.source_table
        && scope.target_table == table.target_table
        && scope.operations.contains(&operation)
}

pub fn total_selected_rows(
    mappings: &[SelectionTable],
    selected_rows: &[SelectedComparisonRow],
    scopes: &[TableSelectionScope],
    options: &SyncOptions,
) -> usize {
    let scoped: usize = scopes
        .iter()
        .map(|scope| {
            let table = match mappings.iter().find(|table| {
                table.source_table == scope.source_table && table.target_table == scope.target_table
            }) {
                Some(table) => table,
                None => return 0,
            };
            let counts = row_counts(table);
            scope
                .operations
                .iter()
                .copied()
                .map(|operation| {
                    if scope.selection_mode == crate::DataSyncSelectionScopeMode::Defaults
                        && !default_row_selected(operation, options)
                    {
                        return 0;
                    }
                    let excluded = scope
                        .excluded_rows
                        .iter()
                        .filter(|row| row.operation == operation)
                        .count();
                    count_for(operation, &counts).saturating_sub(excluded)
                })
                .sum::<usize>()
        })
        .sum();

    let explicit = selected_rows
        .iter()
        .filter(|row| {
            if !operation_allowed(row.2, options) {
                return false;
            }
            !mappings.iter().any(|table| {
                table.source_table == row.0
                    && table.target_table == row.1
                    && scopes.iter().any(|scope| covers(scope, table, row.2))
            })
        })
        .count();
    scoped + explicit
}

pub fn has_selected_deletes(
    selected_rows: &[SelectedComparisonRow],
    scopes: &[TableSelectionScope],
    options: &SyncOptions,
) -> bool {
    options.delete
        && (selected_rows
            .iter()
            .any(|row| row.2 == DataSyncOperation::Delete)
            || scopes
                .iter()
                .any(|scope| scope.selects_operation(DataSyncOperation::Delete, options)))
}

#[cfg(test)]
mod tests {
    use crate::{ComparisonRow, ComparisonTableStatus, DataSyncSelectionScopeMode};

    use super::*;

    fn table() -> SelectionTable {
        SelectionTable {
            source_table: "s".to_owned(),
            target_table: "t".to_owned(),
            status: ComparisonTableStatus::Matched,
            rows: Some(vec![
                ComparisonRow {
                    operation: DataSyncOperation::Insert,
                    key: vec![serde_json::json!(1)],
                    selected: false,
                },
                ComparisonRow {
                    operation: DataSyncOperation::Insert,
                    key: vec![serde_json::json!(2)],
                    selected: false,
                },
                ComparisonRow {
                    operation: DataSyncOperation::Delete,
                    key: vec![serde_json::json!(3)],
                    selected: false,
                },
            ]),
            first_cursor: None,
            page_size: None,
        }
    }

    fn scope(mode: DataSyncSelectionScopeMode) -> TableSelectionScope {
        TableSelectionScope {
            source_table: "s".to_owned(),
            target_table: "t".to_owned(),
            selection_mode: mode,
            operations: vec![DataSyncOperation::Insert],
            excluded_rows: vec![],
        }
    }

    #[test]
    fn scoped_counts_respect_mode_defaults_and_exclusions() {
        let options = SyncOptions {
            insert: true,
            update: false,
            delete: false,
        };
        let all = scope(DataSyncSelectionScopeMode::All);
        assert_eq!(total_selected_rows(&[table()], &[], &[all], &options), 2);

        let mut excluded = scope(DataSyncSelectionScopeMode::All);
        excluded
            .excluded_rows
            .push(crate::data_sync_options::ScopeExclusion {
                operation: DataSyncOperation::Insert,
                key: vec![serde_json::json!(1)],
            });
        assert_eq!(
            total_selected_rows(&[table()], &[], &[excluded], &options),
            1
        );

        let defaults = scope(DataSyncSelectionScopeMode::Defaults);
        assert_eq!(
            total_selected_rows(&[table()], &[], &[defaults.clone()], &options),
            2
        );
        let no_insert = SyncOptions {
            insert: false,
            update: true,
            delete: false,
        };
        assert_eq!(
            total_selected_rows(&[table()], &[], &[defaults.clone()], &no_insert),
            0
        );
    }

    #[test]
    fn explicit_rows_covered_by_a_scope_are_not_double_counted() {
        let options = SyncOptions {
            insert: true,
            update: false,
            delete: false,
        };
        let row: SelectedComparisonRow = (
            "s".to_owned(),
            "t".to_owned(),
            DataSyncOperation::Insert,
            vec![serde_json::json!(1)],
        );
        assert_eq!(
            total_selected_rows(&[table()], &[row.clone()], &[], &options),
            1
        );
        assert_eq!(
            total_selected_rows(
                &[table()],
                &[row],
                &[scope(DataSyncSelectionScopeMode::All)],
                &options
            ),
            2
        );
    }

    #[test]
    fn delete_warning_needs_the_option_and_a_delete_source() {
        let options = SyncOptions {
            insert: true,
            update: true,
            delete: false,
        };
        assert!(!has_selected_deletes(
            &[(
                "s".to_owned(),
                "t".to_owned(),
                DataSyncOperation::Delete,
                vec![]
            )],
            &[],
            &options
        ));
        let with_delete = SyncOptions {
            insert: true,
            update: true,
            delete: true,
        };
        assert!(has_selected_deletes(
            &[(
                "s".to_owned(),
                "t".to_owned(),
                DataSyncOperation::Delete,
                vec![]
            )],
            &[],
            &with_delete
        ));
        let mut delete_scope = scope(DataSyncSelectionScopeMode::All);
        delete_scope.operations = vec![DataSyncOperation::Delete];
        assert!(has_selected_deletes(&[], &[delete_scope], &with_delete));
    }
}
