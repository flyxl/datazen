use crate::DataSyncOperation;

use super::types::{RowDiffCounts, TableDiffFilter, TableListRow, TableSyncStatus};

pub fn row_diff_counts(row: &TableListRow) -> RowDiffCounts {
    let inserts = row.insert_count.unwrap_or_else(|| {
        row.rows
            .iter()
            .filter(|op| **op == DataSyncOperation::Insert)
            .count()
    });
    let updates = row.update_count.unwrap_or_else(|| {
        row.rows
            .iter()
            .filter(|op| **op == DataSyncOperation::Update)
            .count()
    });
    let deletes = row.delete_count.unwrap_or_else(|| {
        row.rows
            .iter()
            .filter(|op| **op == DataSyncOperation::Delete)
            .count()
    });
    let unchanged = row.unchanged_count.unwrap_or(0)
        + row
            .rows
            .iter()
            .filter(|op| **op == DataSyncOperation::Unchanged)
            .count();
    RowDiffCounts {
        inserts,
        updates,
        deletes,
        unchanged,
    }
}

pub fn table_has_row_diffs(row: &TableListRow) -> bool {
    let counts = row_diff_counts(row);
    counts.inserts + counts.updates + counts.deletes > 0
}

pub fn table_is_unchanged(row: &TableListRow) -> bool {
    row.status == TableSyncStatus::Matched && !table_has_row_diffs(row)
}

pub fn display_table_name(row: &TableListRow) -> String {
    if !row.source_table.is_empty()
        && !row.target_table.is_empty()
        && row.source_table != row.target_table
    {
        format!("{} → {}", row.source_table, row.target_table)
    } else if !row.source_table.is_empty() {
        row.source_table.clone()
    } else {
        row.target_table.clone()
    }
}

pub fn table_key(row: &TableListRow) -> String {
    if !row.source_table.is_empty() {
        row.source_table.clone()
    } else {
        row.target_table.clone()
    }
}

pub fn filter_table_rows(
    rows: &[TableListRow],
    filter: TableDiffFilter,
    search: &str,
) -> Vec<usize> {
    let needle = search.to_lowercase();
    let mut indices = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let name = display_table_name(row).to_lowercase();
        if !needle.is_empty() && !name.contains(&needle) {
            continue;
        }
        let passes = match filter {
            TableDiffFilter::All => true,
            TableDiffFilter::Incompatible => row.status == TableSyncStatus::Incompatible,
            TableDiffFilter::Insert
            | TableDiffFilter::Update
            | TableDiffFilter::Delete
            | TableDiffFilter::Unchanged => {
                if row.status != TableSyncStatus::Matched {
                    false
                } else {
                    let counts = row_diff_counts(row);
                    match filter {
                        TableDiffFilter::Insert => counts.inserts > 0,
                        TableDiffFilter::Update => counts.updates > 0,
                        TableDiffFilter::Delete => counts.deletes > 0,
                        TableDiffFilter::Unchanged => table_is_unchanged(row),
                        _ => true,
                    }
                }
            }
        };
        if passes {
            indices.push(index);
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, status: TableSyncStatus, ops: Vec<DataSyncOperation>) -> TableListRow {
        TableListRow {
            source_table: name.to_owned(),
            target_table: name.to_owned(),
            status,
            rows: ops,
            insert_count: None,
            update_count: None,
            delete_count: None,
            unchanged_count: None,
        }
    }

    #[test]
    fn counts_prefer_explicit_values_and_add_unchanged() {
        let mut item = row(
            "t",
            TableSyncStatus::Matched,
            vec![DataSyncOperation::Insert],
        );
        item.insert_count = Some(5);
        item.unchanged_count = Some(2);
        let counts = row_diff_counts(&item);
        assert_eq!(counts.inserts, 5);
        assert_eq!(counts.unchanged, 2);
        assert_eq!(counts.updates, 0);
    }

    #[test]
    fn display_name_and_key_prefer_source_then_target() {
        let mut item = row("src", TableSyncStatus::Matched, Vec::new());
        item.target_table = "tgt".to_owned();
        assert_eq!(display_table_name(&item), "src → tgt");
        assert_eq!(table_key(&item), "src");
        item.source_table.clear();
        assert_eq!(display_table_name(&item), "tgt");
        assert_eq!(table_key(&item), "tgt");
    }

    #[test]
    fn filters_apply_search_then_diff_kind_then_status() {
        let rows = vec![
            row(
                "orders",
                TableSyncStatus::Matched,
                vec![DataSyncOperation::Insert],
            ),
            row(
                "users",
                TableSyncStatus::Matched,
                vec![DataSyncOperation::Update],
            ),
            row("logs", TableSyncStatus::Matched, Vec::new()),
            row(
                "bad",
                TableSyncStatus::Incompatible,
                vec![DataSyncOperation::Insert],
            ),
        ];
        assert_eq!(
            filter_table_rows(&rows, TableDiffFilter::All, ""),
            [0, 1, 2, 3]
        );
        assert_eq!(filter_table_rows(&rows, TableDiffFilter::Insert, ""), [0]);
        assert_eq!(filter_table_rows(&rows, TableDiffFilter::Update, ""), [1]);
        assert_eq!(
            filter_table_rows(&rows, TableDiffFilter::Unchanged, ""),
            [2]
        );
        assert_eq!(
            filter_table_rows(&rows, TableDiffFilter::Incompatible, ""),
            [3]
        );
        assert_eq!(filter_table_rows(&rows, TableDiffFilter::All, "ORD"), [0]);
    }

    #[test]
    fn non_matched_rows_never_pass_diff_filters() {
        let rows = vec![row(
            "x",
            TableSyncStatus::UnmappedSource,
            vec![DataSyncOperation::Delete],
        )];
        assert!(filter_table_rows(&rows, TableDiffFilter::Delete, "").is_empty());
        assert!(!table_is_unchanged(&rows[0]));
    }
}
