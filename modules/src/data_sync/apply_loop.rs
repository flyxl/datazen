//! In-memory Compare → ChangeSet → Apply → Recompare helper for the P0 acceptance loop.

use std::collections::BTreeMap;

use datazen_driver_api::Value;

use super::compare::{cmp_keys, compare_sorted_rows, extract_key};
use super::error::DataSyncError;
use super::model::{ChangeOperation, Row, RowChange, SyncOptions, TableResult};

fn key_bytes(key: &[Value]) -> Vec<u8> {
    serde_json::to_vec(key).unwrap_or_default()
}

pub fn rows_to_map(
    rows: &[Row],
    pk_indexes: &[usize],
) -> Result<BTreeMap<Vec<u8>, Row>, DataSyncError> {
    let mut map = BTreeMap::new();
    for row in rows {
        let key = extract_key(row, pk_indexes)?;
        map.insert(key_bytes(&key), row.clone());
    }
    Ok(map)
}

pub fn map_to_sorted_rows(map: &BTreeMap<Vec<u8>, Row>, pk_indexes: &[usize]) -> Vec<Row> {
    let mut rows: Vec<Row> = map.values().cloned().collect();
    rows.sort_by(|a, b| {
        let ka = extract_key(a, pk_indexes).unwrap_or_default();
        let kb = extract_key(b, pk_indexes).unwrap_or_default();
        cmp_keys(&ka, &kb)
    });
    rows
}

/// Apply a ChangeSet's selected rows onto an in-memory target table.
pub fn apply_changeset_to_rows(
    target: &mut BTreeMap<Vec<u8>, Row>,
    changes: &[RowChange],
    pk_indexes: &[usize],
) -> Result<usize, DataSyncError> {
    let mut applied = 0usize;
    for change in changes {
        match change.operation {
            ChangeOperation::Insert | ChangeOperation::Update => {
                let row = change
                    .source_row
                    .clone()
                    .ok_or_else(|| DataSyncError::validation("apply requires a source row"))?;
                let key = extract_key(&row, pk_indexes)?;
                target.insert(key_bytes(&key), row);
                applied += 1;
            }
            ChangeOperation::Delete => {
                target.remove(&key_bytes(&change.key));
                applied += 1;
            }
            ChangeOperation::Unchanged => {}
        }
    }
    Ok(applied)
}

pub fn recompare_table(
    source_rows: &[Row],
    target_rows: &[Row],
    pk_indexes: &[usize],
    column_names: &[String],
    options: &SyncOptions,
) -> Result<TableResult, DataSyncError> {
    let rows = compare_sorted_rows(source_rows, target_rows, pk_indexes, column_names, options)?;
    Ok(TableResult::matched("src", "tgt", rows))
}

pub fn remaining_mutating_changes(result: &TableResult) -> usize {
    result.insert_count() + result.update_count() + result.delete_count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_sync::changeset::ChangeSet;
    use crate::data_sync::model::{ComparisonResult, SyncOptions};

    fn i(n: i64) -> Option<Value> {
        Some(Value::Integer(n))
    }
    fn s(v: &str) -> Option<Value> {
        Some(Value::String(v.into()))
    }

    #[test]
    fn compare_apply_recompare_is_zero_with_deletes() {
        let mut options = SyncOptions::default();
        options.delete = true;
        let cols = vec!["id".into(), "name".into()];
        let pk = vec![0usize];
        let source = vec![vec![i(1), s("a")], vec![i(2), s("b2")], vec![i(3), s("c")]];
        let target = vec![vec![i(2), s("b")], vec![i(4), s("d")]];

        let first = compare_sorted_rows(&source, &target, &pk, &cols, &options).unwrap();
        assert!(first.iter().any(|r| r.operation == ChangeOperation::Insert));
        assert!(first.iter().any(|r| r.operation == ChangeOperation::Update));
        assert!(first.iter().any(|r| r.operation == ChangeOperation::Delete));

        let mut selected = first.clone();
        for row in &mut selected {
            if row.operation == ChangeOperation::Delete {
                row.selected = true;
            }
        }
        let table = TableResult::matched("users", "users", selected);
        let set = ChangeSet::from_comparison("t", &ComparisonResult::new(vec![table]), &options);
        assert!(!set.is_empty());

        let mut target_map = rows_to_map(&target, &pk).unwrap();
        let applied =
            apply_changeset_to_rows(&mut target_map, &set.tables[0].changes, &pk).unwrap();
        assert!(applied >= 3);

        let target_after = map_to_sorted_rows(&target_map, &pk);
        let second = recompare_table(&source, &target_after, &pk, &cols, &options).unwrap();
        assert_eq!(remaining_mutating_changes(&second), 0);
        assert_eq!(second.unchanged_row_count(), source.len());
    }

    #[test]
    fn without_selecting_deletes_recompare_still_has_deletes() {
        let options = SyncOptions::default();
        let cols = vec!["id".into(), "name".into()];
        let pk = vec![0usize];
        let source = vec![vec![i(1), s("a")]];
        let target = vec![vec![i(1), s("a")], vec![i(9), s("gone")]];
        let first = compare_sorted_rows(&source, &target, &pk, &cols, &options).unwrap();
        let table = TableResult::matched("t", "t", first);
        let set = ChangeSet::from_comparison("t", &ComparisonResult::new(vec![table]), &options);
        let mut target_map = rows_to_map(&target, &pk).unwrap();
        apply_changeset_to_rows(
            &mut target_map,
            set.tables
                .get(0)
                .map(|t| t.changes.as_slice())
                .unwrap_or(&[]),
            &pk,
        )
        .unwrap();
        let second = recompare_table(
            &source,
            &map_to_sorted_rows(&target_map, &pk),
            &pk,
            &cols,
            &options,
        )
        .unwrap();
        assert_eq!(second.delete_count(), 1);
        assert_eq!(second.insert_count(), 0);
        assert_eq!(second.update_count(), 0);
    }
}
