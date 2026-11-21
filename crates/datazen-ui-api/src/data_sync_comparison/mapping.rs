use std::collections::BTreeSet;

use crate::{DataSyncOperation, SyncOptions};

use super::{ComparisonRow, ComparisonTableRow, ComparisonTableStatus};

pub type MappedRow = ComparisonTableRow;

pub fn mark_disabled_tables(rows: &[MappedRow], disabled: &BTreeSet<String>) -> Vec<MappedRow> {
    rows.iter()
        .map(|row| {
            if disabled.contains(&row.source_table) {
                MappedRow {
                    status: ComparisonTableStatus::Disabled,
                    rows: None,
                    ..row.clone()
                }
            } else {
                MappedRow {
                    status: ComparisonTableStatus::Matched,
                    ..row.clone()
                }
            }
        })
        .collect()
}

pub fn tables_for_compare(rows: &[MappedRow]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.status == ComparisonTableStatus::Matched)
        .map(|row| row.source_table.clone())
        .collect()
}

fn default_selected(operation: DataSyncOperation, options: &SyncOptions) -> bool {
    match operation {
        DataSyncOperation::Insert => options.insert,
        DataSyncOperation::Update => options.update,
        DataSyncOperation::Delete | DataSyncOperation::Unchanged => false,
    }
}

pub fn apply_options_to_rows(rows: &[ComparisonRow], options: &SyncOptions) -> Vec<ComparisonRow> {
    rows.iter()
        .map(|row| ComparisonRow {
            selected: default_selected(row.operation, options),
            ..row.clone()
        })
        .collect()
}

pub fn merge_compare_into_mappings(
    mappings: &[MappedRow],
    compared: &[MappedRow],
) -> Vec<MappedRow> {
    mappings
        .iter()
        .map(|mapping| {
            match compared
                .iter()
                .find(|row| row.source_table == mapping.source_table)
            {
                Some(found) => MappedRow {
                    rows: found.rows.clone(),
                    status: found.status,
                    ..mapping.clone()
                },
                None => mapping.clone(),
            }
        })
        .collect()
}

pub fn toggle_disabled_status(status: ComparisonTableStatus) -> (ComparisonTableStatus, bool) {
    match status {
        ComparisonTableStatus::Disabled => (ComparisonTableStatus::Matched, false),
        ComparisonTableStatus::Matched => (ComparisonTableStatus::Disabled, true),
        other => (other, false),
    }
}

pub fn update_source_filter(row: &MappedRow, filter: Option<&str>) -> MappedRow {
    MappedRow {
        source_filter: filter.map(str::to_owned),
        rows: None,
        ..row.clone()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn row(name: &str, status: ComparisonTableStatus) -> MappedRow {
        MappedRow {
            source_table: name.to_owned(),
            target_table: name.to_owned(),
            status,
            source_filter: None,
            rows: Some(Vec::new()),
        }
    }

    fn cmp_row(op: DataSyncOperation, selected: bool) -> ComparisonRow {
        ComparisonRow {
            operation: op,
            key: vec![json!(1)],
            selected,
        }
    }

    #[test]
    fn disabled_profile_tables_are_marked_and_cleared() {
        let rows = [
            row("a", ComparisonTableStatus::Matched),
            row("b", ComparisonTableStatus::Matched),
        ];
        let disabled = BTreeSet::from(["a".to_owned()]);
        let marked = mark_disabled_tables(&rows, &disabled);
        assert_eq!(marked[0].status, ComparisonTableStatus::Disabled);
        assert_eq!(marked[0].rows, None);
        assert_eq!(marked[1].status, ComparisonTableStatus::Matched);
    }

    #[test]
    fn only_matched_tables_go_to_compare() {
        let rows = [
            row("a", ComparisonTableStatus::Matched),
            row("b", ComparisonTableStatus::Disabled),
        ];
        assert_eq!(tables_for_compare(&rows), ["a"]);
    }

    #[test]
    fn options_recompute_default_row_selection() {
        let rows = [
            cmp_row(DataSyncOperation::Insert, false),
            cmp_row(DataSyncOperation::Update, true),
            cmp_row(DataSyncOperation::Delete, true),
            cmp_row(DataSyncOperation::Unchanged, true),
        ];
        let options = SyncOptions {
            insert: true,
            update: false,
            delete: false,
        };
        let result = apply_options_to_rows(&rows, &options);
        assert!(result[0].selected);
        assert!(!result[1].selected);
        assert!(!result[2].selected);
        assert!(!result[3].selected);
    }

    #[test]
    fn compare_results_merge_by_source_table_and_keep_mapping_identity() {
        let mappings = [
            row("a", ComparisonTableStatus::Matched),
            row("b", ComparisonTableStatus::Matched),
        ];
        let mut compared = row("a", ComparisonTableStatus::Matched);
        compared.rows = Some(vec![cmp_row(DataSyncOperation::Insert, false)]);
        let merged = merge_compare_into_mappings(&mappings, &[compared]);
        assert_eq!(merged[0].rows.as_ref().map(Vec::len), Some(1));
        assert_eq!(merged[0].target_table, "a");
        assert_eq!(merged[1].rows.as_ref().map(Vec::len), Some(0));
    }

    #[test]
    fn toggle_and_filter_updates_are_observable() {
        assert_eq!(
            toggle_disabled_status(ComparisonTableStatus::Matched),
            (ComparisonTableStatus::Disabled, true)
        );
        assert_eq!(
            toggle_disabled_status(ComparisonTableStatus::Disabled),
            (ComparisonTableStatus::Matched, false)
        );
        let mut item = row("a", ComparisonTableStatus::Matched);
        item.rows = Some(vec![cmp_row(DataSyncOperation::Insert, false)]);
        let updated = update_source_filter(&item, Some("where x=1"));
        assert_eq!(updated.source_filter.as_deref(), Some("where x=1"));
        assert_eq!(updated.rows, None);
        assert!(item.rows.is_some());
    }
}
