use super::*;
use crate::DataSyncOperation;
use serde_json::json;

fn row(
    source: &str,
    target: &str,
    status: TableSyncStatus,
    operations: Vec<DataSyncOperation>,
) -> CompareReportRow {
    CompareReportRow {
        table: TableListRow {
            source_table: source.to_owned(),
            target_table: target.to_owned(),
            status,
            rows: operations,
            insert_count: None,
            update_count: None,
            delete_count: None,
            unchanged_count: None,
        },
        incompatible_reason: None,
    }
}

fn matched(source: &str, operations: Vec<DataSyncOperation>) -> CompareReportRow {
    row(source, source, TableSyncStatus::Matched, operations)
}

#[test]
fn summary_counts_only_matched_and_incompatible_rows() {
    let mut with_changes = matched("a", vec![DataSyncOperation::Insert, DataSyncOperation::Update]);
    with_changes.table.insert_count = Some(5);
    with_changes.table.unchanged_count = Some(2);
    let clean = matched("b", vec![DataSyncOperation::Unchanged]);
    let mut incompatible = row("c", "d", TableSyncStatus::Incompatible, Vec::new());
    incompatible.incompatible_reason = Some("type mismatch".to_owned());
    let unmapped = row("e", "", TableSyncStatus::UnmappedSource, Vec::new());
    let disabled = row("f", "", TableSyncStatus::Disabled, Vec::new());

    let stats = summarize_compare(&[
        with_changes,
        clean,
        incompatible,
        unmapped,
        disabled,
    ]);
    assert_eq!(stats.inserts, 5);
    assert_eq!(stats.updates, 1);
    assert_eq!(stats.deletes, 0);
    assert_eq!(stats.unchanged_tables, 1);
    assert_eq!(stats.incompatible, 1);
}

#[test]
fn text_report_pins_the_header_and_every_row_shape() {
    let rows = vec![
        matched("a", vec![DataSyncOperation::Insert, DataSyncOperation::Delete]),
        {
            let mut item = row("old", "new", TableSyncStatus::Incompatible, Vec::new());
            item.incompatible_reason = Some("type mismatch".to_owned());
            item
        },
        row("no-target", "", TableSyncStatus::UnmappedTarget, Vec::new()),
        matched("clean", vec![]),
    ];
    let text = build_compare_report_text(&rows, None);
    assert_eq!(
        text,
        [
            "Data Sync Compare Report",
            "Inserts: 1",
            "Updates: 0",
            "Deletes: 1",
            "Unchanged tables: 1",
            "Incompatible tables: 1",
            "",
            "Tables:",
            "- a: +1 / ~0 / −1 (0 unchanged rows)",
            "- old → new: INCOMPATIBLE — type mismatch",
            "- no-target: UNMAPPED_TARGET",
            "- clean: +0 / ~0 / −0 (0 unchanged rows)",
        ]
        .join("\n")
    );
}

#[test]
fn an_incompatible_row_without_a_reason_has_no_dash_suffix() {
    let rows = vec![row("x", "y", TableSyncStatus::Incompatible, Vec::new())];
    let text = build_compare_report_text(&rows, None);
    assert!(text.ends_with("- x → y: INCOMPATIBLE"));
    assert!(!text.contains(" — "));
    let mut empty_reason = row("x", "y", TableSyncStatus::Incompatible, Vec::new());
    empty_reason.incompatible_reason = Some(String::new());
    assert!(build_compare_report_text(&[empty_reason], None).ends_with("- x → y: INCOMPATIBLE"));
}

#[test]
fn explicit_stats_override_the_derived_summary() {
    let stats = CompareSummaryStats {
        inserts: 9,
        updates: 8,
        deletes: 7,
        unchanged_tables: 6,
        incompatible: 5,
    };
    let rows = vec![matched("a", vec![DataSyncOperation::Insert])];
    let text = build_compare_report_text(&rows, Some(&stats));
    assert!(text.contains("Inserts: 9"));
    assert!(text.contains("Unchanged tables: 6"));
    assert!(text.contains("Incompatible tables: 5"));
    assert!(text.contains("- a: +1 / ~0 / −0 (0 unchanged rows)"));
}

#[test]
fn an_empty_report_keeps_the_header_and_tables_label() {
    let text = build_compare_report_text(&[], None);
    assert_eq!(
        text,
        "Data Sync Compare Report\nInserts: 0\nUpdates: 0\nDeletes: 0\nUnchanged tables: 0\nIncompatible tables: 0\n\nTables:"
    );
}

#[test]
fn json_report_pins_summary_and_each_table_shape() {
    let mut incompatible = row("old", "new", TableSyncStatus::Incompatible, Vec::new());
    incompatible.incompatible_reason = Some("type mismatch".to_owned());
    let rows = vec![
        matched("a", vec![DataSyncOperation::Update, DataSyncOperation::Unchanged]),
        incompatible,
        row("e", "", TableSyncStatus::Disabled, Vec::new()),
    ];
    let text = build_compare_report_json(&rows, None);
    assert!(text.contains("\n  \"summary\": {"));
    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => panic!("report must be valid json: {error}"),
    };
    assert_eq!(parsed["summary"]["inserts"], 0);
    assert_eq!(parsed["summary"]["updates"], 1);
    assert_eq!(parsed["summary"]["unchangedTables"], 0);
    assert_eq!(parsed["summary"]["incompatible"], 1);
    assert_eq!(
        parsed["tables"][0],
        json!({"name": "a", "status": "MATCHED", "inserts": 0, "updates": 1, "deletes": 0, "unchangedRows": 1})
    );
    assert_eq!(
        parsed["tables"][1],
        json!({"name": "old → new", "status": "INCOMPATIBLE", "incompatibleReason": "type mismatch"})
    );
    assert_eq!(parsed["tables"][2], json!({"name": "e", "status": "DISABLED"}));
    assert_eq!(parsed["tables"][2].as_object().map(|o| o.len()), Some(2));
}

#[test]
fn json_report_uses_incompatible_null_and_explicit_stats() {
    let rows = vec![row("x", "y", TableSyncStatus::Incompatible, Vec::new())];
    let parsed: serde_json::Value = serde_json::from_str(&build_compare_report_json(&rows, None))
        .unwrap_or(json!(null));
    assert!(parsed["tables"][0]["incompatibleReason"].is_null());

    let stats = CompareSummaryStats {
        inserts: 1,
        updates: 2,
        deletes: 3,
        unchanged_tables: 4,
        incompatible: 5,
    };
    let overridden: serde_json::Value =
        serde_json::from_str(&build_compare_report_json(&rows, Some(&stats)))
            .unwrap_or(json!(null));
    assert_eq!(overridden["summary"]["deletes"], 3);
    assert_eq!(overridden["summary"]["incompatible"], 5);
}

#[test]
fn crate_root_reexports_the_report_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::CompareReportRow>();
    resolves::<crate::CompareSummaryStats>();
    let text = crate::build_compare_report_text(&[], None);
    assert!(text.starts_with("Data Sync Compare Report"));
    assert_eq!(crate::summarize_compare(&[]).inserts, 0);
}
