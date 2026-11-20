//! T417 audit (tester): independent checks of the mapping view. Fresh
//! vectors: mixed wire/absent counters in one row, the page-key NUL
//! boundary, a comparison keyed by the target table when the mapping has
//! no source, disabled re-marking idempotence, and the label keys'
//! resolvability in the zh-CN packs.

use std::collections::HashSet;

use serde_json::json;

use crate::i18n::has;
use crate::views::data_sync::data_sync_compare_step::{
    DataSyncMappingStatus, DataSyncOperation, DataSyncRowChange, DataSyncTableResult,
    SyncOptions, TableDiffFilter,
};
use crate::views::data_sync::mapping_view::*;

fn audit_change(operation: DataSyncOperation) -> DataSyncRowChange {
    DataSyncRowChange {
        operation,
        key: vec![json!("k")],
        source_row: None,
        target_row: None,
        changed_columns: Vec::new(),
        selected: false,
    }
}

fn audit_opts(insert: bool, update: bool, delete: bool) -> SyncOptions {
    SyncOptions {
        insert,
        update,
        delete,
        ..SyncOptions::default()
    }
}

#[test]
fn audit_mixed_counters_split_wire_and_rows() {
    // One row of each kind, with the INSERT wire counter present and the
    // rest absent: 1 wire + 0 counted for inserts, the rows count the rest.
    let mut row = DataSyncTableResult::default();
    row.insert_count = Some(1);
    row.rows = vec![
        audit_change(DataSyncOperation::Insert),
        audit_change(DataSyncOperation::Insert),
        audit_change(DataSyncOperation::Update),
        audit_change(DataSyncOperation::Delete),
        audit_change(DataSyncOperation::Delete),
        audit_change(DataSyncOperation::Unchanged),
    ];
    let counts = row_diff_counts(&row);
    assert_eq!(counts.inserts, 1, "the wire counter wins over the 2 rows");
    assert_eq!(counts.updates, 1);
    assert_eq!(counts.deletes, 2);
    assert_eq!(counts.unchanged, 1);
}

#[test]
fn audit_the_page_key_survives_empty_sides() {
    let mut row = DataSyncTableResult::default();
    row.source_table = "a".to_string();
    assert_eq!(table_page_key(&row), "a\u{0}");
    row.source_table.clear();
    row.target_table = "b".to_string();
    assert_eq!(table_page_key(&row), "\u{0}b");
    // A NUL inside a table name cannot be confused with the separator: the
    // React key has the same property only by convention of never storing
    // NULs — the port keeps the same spelling regardless.
    row.source_table = "x\u{0}y".to_string();
    assert_eq!(table_page_key(&row), "x\u{0}y\u{0}b");
}

#[test]
fn audit_a_comparison_keyed_by_target_merges_a_sourceless_mapping() {
    // The mapping has only a target; the comparison also has only a
    // target: both keys fall to the target spelling and the merge fires.
    let mut mapping = DataSyncTableResult::default();
    mapping.target_table = "tgt".to_string();
    mapping.status = DataSyncMappingStatus::Matched;

    let mut compared = DataSyncTableResult::default();
    compared.target_table = "tgt".to_string();
    compared.insert_count = Some(4);

    let merged = merge_compare_into_mappings(&[mapping], &[compared]);
    assert_eq!(merged[0].insert_count, Some(4));
}

#[test]
fn audit_disabled_marking_is_idempotent_and_reversible_by_rerun() {
    let mut matched = DataSyncTableResult::default();
    matched.source_table = "t".to_string();
    matched.status = DataSyncMappingStatus::Matched;
    matched.rows = vec![audit_change(DataSyncOperation::Update)];

    let mut disabled = HashSet::new();
    disabled.insert("t".to_string());
    let first = mark_disabled_tables(&[matched.clone()], &disabled);
    assert_eq!(first[0].status, DataSyncMappingStatus::Disabled);
    // Re-marking the already-disabled row: it is no longer MATCHED, so the
    // pass leaves it (status and emptied rows unchanged).
    let second = mark_disabled_tables(&first, &disabled);
    assert_eq!(second[0].status, DataSyncMappingStatus::Disabled);
    assert!(second[0].rows.is_empty());
    // And an empty disabled set marks nothing.
    let untouched = mark_disabled_tables(&[matched], &HashSet::new());
    assert_eq!(untouched[0].status, DataSyncMappingStatus::Matched);
    assert_eq!(untouched[0].rows.len(), 1);
}

#[test]
fn audit_every_label_key_resolves_in_the_packs() {
    for status in [
        DataSyncMappingStatus::Matched,
        DataSyncMappingStatus::UnmappedSource,
        DataSyncMappingStatus::UnmappedTarget,
        DataSyncMappingStatus::Disabled,
        DataSyncMappingStatus::Incompatible,
    ] {
        let key = mapping_label_key(status);
        assert!(has(key), "{key} has no zh-CN entry");
    }
}

#[test]
fn audit_the_filter_query_uses_the_display_name_not_the_key() {
    // A renamed pair (src → tgt): the query matches the "src → tgt"
    // spelling, not the bare source table.
    let mut row = DataSyncTableResult::default();
    row.source_table = "legacy".to_string();
    row.target_table = "modern".to_string();
    row.status = DataSyncMappingStatus::Matched;
    assert!(table_matches_filter(&row, TableDiffFilter::All, "legacy \u{2192} modern"));
    assert!(table_matches_filter(&row, TableDiffFilter::All, "modern"));
    // Lowercased name vs uppercased query.
    assert!(table_matches_filter(&row, TableDiffFilter::All, "MODERN"));
    // The NUL key spelling is NOT what the query sees.
    assert!(!table_matches_filter(&row, TableDiffFilter::All, "\u{0}"));
}

#[test]
fn audit_row_key_string_escaping_matches_json() {
    // A string member with a quote and a backslash escapes exactly as
    // JSON.stringify would.
    assert_eq!(
        row_key_string(&[json!("a\"b\\c")]),
        r#"["a\"b\\c"]"#
    );
    // Numbers keep their JSON spelling; a float stays a float.
    assert_eq!(row_key_string(&[json!(1.5), json!(-0)]), "[1.5,0]");
}
