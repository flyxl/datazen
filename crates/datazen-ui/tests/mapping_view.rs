//! T417: the mapping view's pure functions — the wire-counter/fallback
//! split, the label keys, display/key spellings, the summaries, the
//! selection-vs-options rules, the filter, and the two merges.

use std::collections::HashSet;

use serde_json::json;

use datazen_ui::views::data_sync::data_sync_compare_step::{
    DataSyncMappingStatus, DataSyncOperation, DataSyncRowChange, DataSyncTableResult,
    SyncOptions, TableDiffFilter,
};
use datazen_ui::views::data_sync::mapping_view::*;

fn change(operation: DataSyncOperation, selected: bool) -> DataSyncRowChange {
    DataSyncRowChange {
        operation,
        key: vec![json!(1)],
        source_row: None,
        target_row: None,
        changed_columns: Vec::new(),
        selected,
    }
}

fn opts(insert: bool, update: bool, delete: bool) -> SyncOptions {
    SyncOptions {
        insert,
        update,
        delete,
        ..SyncOptions::default()
    }
}

#[test]
fn wire_counters_win_and_unchanged_adds() {
    let mut row = DataSyncTableResult::default();
    row.insert_count = Some(7);
    row.update_count = Some(3);
    // delete_count stays absent -> counted from the rows.
    row.rows = vec![
        change(DataSyncOperation::Delete, false),
        change(DataSyncOperation::Unchanged, false),
        change(DataSyncOperation::Unchanged, false),
    ];
    let counts = row_diff_counts(&row);
    assert_eq!(counts.inserts, 7);
    assert_eq!(counts.updates, 3);
    assert_eq!(counts.deletes, 1);
    // unchanged = unchangedCount (absent -> 0) + UNCHANGED rows.
    assert_eq!(counts.unchanged, 2);

    // With the wire counter PRESENT the two add: 1 + 2 UNCHANGED rows = 3
    // (the counter never replaces the rows' contribution).
    row.unchanged_count = Some(1);
    assert_eq!(row_diff_counts(&row).unchanged, 3);
}

#[test]
fn absent_counters_recompute_from_rows() {
    let mut row = DataSyncTableResult::default();
    row.rows = vec![
        change(DataSyncOperation::Insert, false),
        change(DataSyncOperation::Insert, false),
        change(DataSyncOperation::Update, false),
    ];
    let counts = row_diff_counts(&row);
    assert_eq!(counts.inserts, 2);
    assert_eq!(counts.updates, 1);
    assert_eq!(counts.deletes, 0);
}

#[test]
fn row_diff_presence_and_unchanged() {
    let mut changed = DataSyncTableResult::default();
    changed.status = DataSyncMappingStatus::Matched;
    changed.insert_count = Some(1);
    assert!(table_has_row_diffs(&changed));
    assert!(!table_is_unchanged(&changed));

    // Matched with no diffs: unchanged.
    let mut clean = DataSyncTableResult::default();
    clean.status = DataSyncMappingStatus::Matched;
    assert!(!table_has_row_diffs(&clean));
    assert!(table_is_unchanged(&clean));

    // A non-matched table is never "unchanged" by this predicate.
    let mut incompatible = DataSyncTableResult::default();
    incompatible.status = DataSyncMappingStatus::Incompatible;
    assert!(!table_is_unchanged(&incompatible));
}

#[test]
fn the_label_keys_map_one_to_one() {
    assert_eq!(mapping_label_key(DataSyncMappingStatus::Matched), "sync.mappingMatched");
    assert_eq!(
        mapping_label_key(DataSyncMappingStatus::UnmappedSource),
        "sync.mappingUnmappedSource"
    );
    assert_eq!(
        mapping_label_key(DataSyncMappingStatus::UnmappedTarget),
        "sync.mappingUnmappedTarget"
    );
    assert_eq!(mapping_label_key(DataSyncMappingStatus::Disabled), "sync.mappingDisabled");
    assert_eq!(
        mapping_label_key(DataSyncMappingStatus::Incompatible),
        "sync.mappingIncompatible"
    );
}

#[test]
fn display_names_follow_the_arrow_rule() {
    let mut both = DataSyncTableResult::default();
    both.source_table = "src_users".to_string();
    both.target_table = "tgt_users".to_string();
    assert_eq!(display_table_name(&both), "src_users \u{2192} tgt_users");

    // Same name on both sides: one spelling.
    let mut same = DataSyncTableResult::default();
    same.source_table = "users".to_string();
    same.target_table = "users".to_string();
    assert_eq!(display_table_name(&same), "users");

    // Only the target exists.
    let mut target_only = DataSyncTableResult::default();
    target_only.target_table = "tgt".to_string();
    assert_eq!(display_table_name(&target_only), "tgt");
}

#[test]
fn keys_pick_source_else_target_and_page_key_joins_with_nul() {
    let mut row = DataSyncTableResult::default();
    row.source_table = "a".to_string();
    row.target_table = "b".to_string();
    assert_eq!(table_key(&row), "a");
    assert_eq!(table_page_key(&row), "a\u{0}b");

    row.source_table.clear();
    assert_eq!(table_key(&row), "b");
    assert_eq!(table_page_key(&row), "\u{0}b");
}

#[test]
fn both_summaries_bucket_the_statuses() {
    let mut matched = DataSyncTableResult::default();
    matched.status = DataSyncMappingStatus::Matched;
    matched.insert_count = Some(2);
    matched.update_count = Some(5);

    let mut incompatible = DataSyncTableResult::default();
    incompatible.status = DataSyncMappingStatus::Incompatible;

    let mut unmapped_source = DataSyncTableResult::default();
    unmapped_source.status = DataSyncMappingStatus::UnmappedSource;

    let mut unmapped_target = DataSyncTableResult::default();
    unmapped_target.status = DataSyncMappingStatus::UnmappedTarget;

    let mut disabled = DataSyncTableResult::default();
    disabled.status = DataSyncMappingStatus::Disabled;

    let rows = [matched, incompatible, unmapped_source, unmapped_target, disabled];
    let summary = summarize_mappings(&rows);
    assert_eq!(
        (summary.matched, summary.incompatible, summary.unmapped, summary.disabled),
        (1, 1, 2, 1)
    );

    let stats = summarize_compare(&rows);
    // INCOMPATIBLE counts once and stops; UNMAPPED/DISABLED rows contribute
    // nothing; the matched row contributes its counters.
    assert_eq!(
        (stats.inserts, stats.updates, stats.deletes, stats.unchanged_tables, stats.incompatible),
        (2, 5, 0, 0, 1)
    );
}

#[test]
fn summarize_compare_counts_clean_tables_as_unchanged() {
    let mut clean = DataSyncTableResult::default();
    clean.status = DataSyncMappingStatus::Matched;
    let mut dirty = DataSyncTableResult::default();
    dirty.status = DataSyncMappingStatus::Matched;
    dirty.delete_count = Some(1);
    let stats = summarize_compare(&[clean, dirty]);
    assert_eq!(stats.unchanged_tables, 1);
    assert_eq!(stats.deletes, 1);
}

#[test]
fn row_key_string_is_the_json_spelling() {
    assert_eq!(row_key_string(&[json!(1), json!("x")]), r#"[1,"x"]"#);
    assert_eq!(row_key_string(&[]), "[]");
    assert_eq!(row_key_string(&[json!(null)]), "[null]");
}

#[test]
fn default_selection_follows_options_and_never_delete() {
    let options = opts(true, true, true);
    assert!(default_row_selected(DataSyncOperation::Insert, &options));
    assert!(default_row_selected(DataSyncOperation::Update, &options));
    assert!(!default_row_selected(DataSyncOperation::Delete, &options), "DELETE never pre-selected");
    assert!(!default_row_selected(DataSyncOperation::Unchanged, &options));
    // Options can switch the two kinds off.
    let off = opts(false, false, true);
    assert!(!default_row_selected(DataSyncOperation::Insert, &off));
    assert!(!default_row_selected(DataSyncOperation::Update, &off));
}

#[test]
fn operation_allowed_is_the_three_switches() {
    let options = opts(true, false, true);
    assert!(operation_allowed(DataSyncOperation::Insert, &options));
    assert!(!operation_allowed(DataSyncOperation::Update, &options));
    assert!(operation_allowed(DataSyncOperation::Delete, &options));
    assert!(!operation_allowed(DataSyncOperation::Unchanged, &options));
}

#[test]
fn apply_options_deselects_disallowed_operations() {
    let rows = vec![
        change(DataSyncOperation::Insert, true),
        change(DataSyncOperation::Update, true),
        change(DataSyncOperation::Delete, true),
        change(DataSyncOperation::Insert, false),
    ];
    // insert allowed, update not: rows keep data, only `selected` moves.
    let applied = apply_options_to_rows(&rows, &opts(true, false, false));
    assert_eq!(applied.len(), 4);
    assert!(applied[0].selected);
    assert!(!applied[1].selected, "an allowed-off operation deselects");
    assert!(!applied[2].selected);
    assert!(!applied[3].selected, "an unselected row stays unselected");
    assert_eq!(applied[0].key, vec![json!(1)], "the row data is unchanged");
}

#[test]
fn selected_row_count_excludes_unchanged_and_disallowed() {
    let mut table = DataSyncTableResult::default();
    table.rows = vec![
        change(DataSyncOperation::Insert, true),
        change(DataSyncOperation::Update, true),
        change(DataSyncOperation::Unchanged, true),
        change(DataSyncOperation::Delete, true),
        change(DataSyncOperation::Insert, false),
    ];
    // insert+update allowed, delete off: 2 selected-and-allowed rows.
    assert_eq!(selected_row_count(&table, &opts(true, true, false)), 2);
}

#[test]
fn the_filter_applies_query_then_status_then_counts() {
    let mut matched_dirty = DataSyncTableResult::default();
    matched_dirty.source_table = "users".to_string();
    matched_dirty.status = DataSyncMappingStatus::Matched;
    matched_dirty.insert_count = Some(3);

    let mut incompatible = DataSyncTableResult::default();
    incompatible.source_table = "legacy".to_string();
    incompatible.status = DataSyncMappingStatus::Incompatible;

    let mut clean = DataSyncTableResult::default();
    clean.source_table = "audit".to_string();
    clean.status = DataSyncMappingStatus::Matched;

    let mut unmapped = DataSyncTableResult::default();
    unmapped.source_table = "orphan".to_string();
    unmapped.status = DataSyncMappingStatus::UnmappedSource;

    // all: everything passes without a query.
    for row in [&matched_dirty, &incompatible, &clean, &unmapped] {
        assert!(table_matches_filter(row, TableDiffFilter::All, ""));
    }
    // incompatible: only the incompatible one.
    assert!(table_matches_filter(&incompatible, TableDiffFilter::Incompatible, ""));
    assert!(!table_matches_filter(&matched_dirty, TableDiffFilter::Incompatible, ""));
    // insert: only the dirty one; unmapped is not MATCHED so fails outright.
    assert!(table_matches_filter(&matched_dirty, TableDiffFilter::Insert, ""));
    assert!(!table_matches_filter(&clean, TableDiffFilter::Insert, ""));
    assert!(!table_matches_filter(&unmapped, TableDiffFilter::Insert, ""));
    // unchanged: only the clean matched one.
    assert!(table_matches_filter(&clean, TableDiffFilter::Unchanged, ""));
    assert!(!table_matches_filter(&matched_dirty, TableDiffFilter::Unchanged, ""));

    // The query matches the display name case-insensitively.
    assert!(table_matches_filter(&matched_dirty, TableDiffFilter::All, "USE"));
    assert!(!table_matches_filter(&matched_dirty, TableDiffFilter::All, "nope"));
    // An empty query never filters.
    assert!(table_matches_filter(&matched_dirty, TableDiffFilter::All, ""));
}

#[test]
fn compare_merges_into_mappings_by_key_with_disabled_protection() {
    let mut mapping_kept = DataSyncTableResult::default();
    mapping_kept.source_table = "kept".to_string();
    mapping_kept.status = DataSyncMappingStatus::UnmappedSource;
    mapping_kept.warnings = vec!["old warning".to_string()];

    let mut mapping_disabled = DataSyncTableResult::default();
    mapping_disabled.source_table = "dis".to_string();
    mapping_disabled.status = DataSyncMappingStatus::Disabled;

    let mut mapping_merged = DataSyncTableResult::default();
    mapping_merged.source_table = "users".to_string();
    mapping_merged.status = DataSyncMappingStatus::Matched;
    mapping_merged.warnings = vec!["stale".to_string()];

    let mut compared_users = DataSyncTableResult::default();
    compared_users.source_table = "users".to_string();
    compared_users.status = DataSyncMappingStatus::Matched;
    compared_users.insert_count = Some(9);

    let merged = merge_compare_into_mappings(
        &[mapping_kept, mapping_disabled, mapping_merged],
        &[compared_users],
    );
    // No comparison for "kept": the mapping survives untouched.
    assert_eq!(merged[0].status, DataSyncMappingStatus::UnmappedSource);
    assert_eq!(merged[0].warnings, vec!["old warning".to_string()]);
    // DISABLED is never overwritten.
    assert_eq!(merged[1].status, DataSyncMappingStatus::Disabled);
    // The compared fields replace the mapping's (counters carried over),
    // and the empty compared warnings fall back to the mapping's.
    assert_eq!(merged[2].insert_count, Some(9));
    assert_eq!(merged[2].warnings, vec!["stale".to_string()]);
}

#[test]
fn compare_warnings_win_over_mapping_ones_when_present() {
    let mut mapping = DataSyncTableResult::default();
    mapping.source_table = "t".to_string();
    mapping.warnings = vec!["stale".to_string()];
    let mut compared = DataSyncTableResult::default();
    compared.source_table = "t".to_string();
    compared.warnings = vec!["fresh".to_string()];
    let merged = merge_compare_into_mappings(&[mapping], &[compared]);
    assert_eq!(merged[0].warnings, vec!["fresh".to_string()]);
}

#[test]
fn disabled_marking_drops_rows_and_only_touches_matched() {
    let mut matched = DataSyncTableResult::default();
    matched.source_table = "users".to_string();
    matched.status = DataSyncMappingStatus::Matched;
    matched.rows = vec![change(DataSyncOperation::Insert, true)];

    let mut also_matched = DataSyncTableResult::default();
    also_matched.source_table = "logs".to_string();
    also_matched.status = DataSyncMappingStatus::Matched;

    let mut incompatible = DataSyncTableResult::default();
    incompatible.source_table = "users".to_string();
    incompatible.status = DataSyncMappingStatus::Incompatible;

    let mut disabled_set = HashSet::new();
    disabled_set.insert("users".to_string());
    let marked = mark_disabled_tables(&[matched, also_matched, incompatible], &disabled_set);
    assert_eq!(marked[0].status, DataSyncMappingStatus::Disabled);
    assert!(marked[0].rows.is_empty(), "rows are dropped on disabling");
    assert_eq!(marked[1].status, DataSyncMappingStatus::Matched);
    // A non-MATCHED table in the set stays as it was.
    assert_eq!(marked[2].status, DataSyncMappingStatus::Incompatible);
}

#[test]
fn tables_for_compare_lists_matched_sources_only() {
    let mut matched = DataSyncTableResult::default();
    matched.source_table = "users".to_string();
    matched.status = DataSyncMappingStatus::Matched;

    let mut matched_no_source = DataSyncTableResult::default();
    matched_no_source.target_table = "targeted".to_string();
    matched_no_source.status = DataSyncMappingStatus::Matched;

    let mut unmapped = DataSyncTableResult::default();
    unmapped.source_table = "orphan".to_string();
    unmapped.status = DataSyncMappingStatus::UnmappedSource;

    assert_eq!(
        tables_for_compare(&[matched, matched_no_source, unmapped]),
        vec!["users".to_string()]
    );
}
