//! T305: the transfer mapping view — coder vectors: the seven pure
//! functions' rules (the normalize fallback and last-wins, the auto-match
//! and the native-type survival, the clear pass, the unmapped targets, the
//! gate pair and the block reason), each against its TS source's own
//! edge.

use datazen_ui::views::{
    auto_match_column_mappings, clear_unmapped_column_mappings, mapping_gate_allows_advance,
    mapping_gate_block_reason, normalize_column_mappings, unmapped_target_columns,
    ColumnMappingTablePatch, TransferColumnMapping, TransferMappingStatus, TransferTableVm,
};

fn mapping(source: &str, target: &str, skip: bool) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: None,
    }
}

fn mapping_typed(source: &str, target: &str, native: Option<&str>) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip: false,
        target_native_type: native.map(str::to_owned),
    }
}

fn table(mappings: &[TransferColumnMapping]) -> TransferTableVm {
    TransferTableVm {
        source_table: "users".to_owned(),
        target_table: "users".to_owned(),
        status: TransferMappingStatus::Matched,
        create_new: false,
        enabled: true,
        column_mappings: mappings.to_vec(),
        source_columns: None,
        target_columns: None,
        source_column_types: Vec::new(),
        source_primary_keys: None,
        recordset: None,
    }
}

// -- normalizeColumnMappings (TS 4-17) ----------------------------------------

#[test]
fn the_normalize_falls_back_to_the_distinct_sources() {
    // No source_columns: the distinct sources of the stored mappings,
    // first-seen order.
    let t = table(&[mapping("b", "x", false), mapping("a", "y", false), mapping("b", "z", false)]);
    let normalized = normalize_column_mappings(&t);
    assert_eq!(
        normalized.iter().map(|m| m.source_column.as_str()).collect::<Vec<_>>(),
        vec!["b", "a"],
        "first-seen, deduped"
    );
    // The last one wins for a repeated source.
    assert_eq!(normalized[0].target_column, "z");

    // A source with no stored mapping is synthesised skipped.
    let t = table(&[mapping("a", "y", false)]);
    let mut with_source = t.clone();
    with_source.source_columns = Some(vec!["a".to_owned(), "c".to_owned()]);
    let normalized = normalize_column_mappings(&with_source);
    assert_eq!(normalized[1].source_column, "c");
    assert_eq!(normalized[1].target_column, "");
    assert!(normalized[1].skip, "the synthesised row is skipped");
    // The listed order wins over the mapping order.
    assert_eq!(
        normalized.iter().map(|m| m.source_column.as_str()).collect::<Vec<_>>(),
        vec!["a", "c"]
    );
}

// -- autoMatchColumnMappings (TS 20-37) ----------------------------------------

#[test]
fn the_auto_match_matches_by_identical_name() {
    let sources = vec!["id".to_owned(), "name".to_owned(), "email".to_owned()];
    let targets = vec!["id".to_owned(), "email".to_owned(), "extra".to_owned()];
    let matched = auto_match_column_mappings(&sources, &targets, &[]);
    assert_eq!(matched[0].target_column, "id", "id matches");
    assert!(!matched[0].skip);
    assert_eq!(matched[1].target_column, "", "name does not match");
    assert!(matched[1].skip);
    assert_eq!(matched[2].target_column, "email", "email matches");
    assert!(!matched[2].skip);
}

#[test]
fn the_auto_match_survives_the_native_type_of_the_last_row() {
    let sources = vec!["id".to_owned()];
    let targets = vec!["id".to_owned()];
    // A repeated source: the last row's native type survives.
    let existing = vec![
        mapping_typed("id", "old", Some("INTEGER")),
        mapping_typed("id", "newer", Some("BIGINT")),
    ];
    let matched = auto_match_column_mappings(&sources, &targets, &existing);
    assert_eq!(matched[0].target_native_type.as_deref(), Some("BIGINT"));
    // A source with no existing row: no native type.
    let matched = auto_match_column_mappings(&sources, &targets, &[]);
    assert_eq!(matched[0].target_native_type, None);
}

// -- clearUnmappedColumnMappings (TS 40-45) ------------------------------------

#[test]
fn the_clear_empties_and_skips_the_unmapped_rows() {
    let mappings = vec![
        mapping("a", "ta", false),
        mapping("b", "", false),
        mapping("c", "  ", false),
    ];
    let cleared = clear_unmapped_column_mappings(&mappings);
    assert_eq!(cleared[0].target_column, "ta", "the mapped row passes through");
    assert!(!cleared[0].skip);
    assert_eq!(cleared[1].target_column, "");
    assert!(cleared[1].skip, "an empty target skips");
    assert_eq!(cleared[2].target_column, "", "a whitespace target is emptied");
    assert!(cleared[2].skip);
}

// -- unmappedTargetColumns (TS 48-56) ------------------------------------------

#[test]
fn the_unmapped_targets_report_the_unused_columns() {
    // Targets not referenced by any active mapping: a skipped row's
    // target does NOT count as used.
    let t = table(&[mapping("a", "ta", false), mapping("b", "tb", true)]);
    let mut with_targets = t.clone();
    with_targets.target_columns = Some(vec!["ta".to_owned(), "tb".to_owned(), "tc".to_owned()]);
    assert_eq!(
        unmapped_target_columns(&with_targets),
        vec!["tb".to_owned(), "tc".to_owned()],
        "the skipped row's target and the unreferenced one, in target order"
    );
}

#[test]
fn the_unmapped_targets_guard_the_create_new_and_empty_cases() {
    // A create-new table has no unmapped targets.
    let t = table(&[]);
    let mut create_new = t.clone();
    create_new.create_new = true;
    create_new.target_columns = Some(vec!["ta".to_owned()]);
    assert!(unmapped_target_columns(&create_new).is_empty());

    // No target columns: none.
    let t = table(&[mapping("a", "ta", false)]);
    assert!(unmapped_target_columns(&t).is_empty());
}

// -- the gate pair (TS 57-100) ---------------------------------------------------

#[test]
fn the_row_clears_gate_through_the_three_rules() {
    // A disabled row never clears.
    let mut t = table(&[mapping("a", "ta", false)]);
    t.enabled = false;
    assert!(!mapping_gate_allows_advance(&[t.clone()]), "a disabled row never clears");

    // An enabled create-new row with a blank target name does not.
    let mut t = table(&[mapping("a", "ta", false)]);
    t.create_new = true;
    t.target_table = "   ".to_owned();
    assert!(!mapping_gate_allows_advance(&[t]), "a nameless create-new row does not clear");

    // An enabled, named row with an active mapping clears.
    let t = table(&[mapping("a", "ta", false)]);
    assert!(mapping_gate_allows_advance(&[t]));

    // An enabled row with only skipped mappings does not.
    let t = table(&[mapping("a", "", true)]);
    assert!(!mapping_gate_allows_advance(&[t]));
}

#[test]
fn the_gate_allows_when_any_row_clears() {
    let rows = vec![
        table(&[mapping("a", "", true)]),
        table(&[mapping("b", "tb", false)]),
    ];
    assert!(mapping_gate_allows_advance(&rows), "one clearing row allows");
    let rows = vec![table(&[mapping("a", "", true)])];
    assert!(!mapping_gate_allows_advance(&rows));
}

#[test]
fn the_block_reason_reports_only_the_unnamed_create_new() {
    // An unnamed create-new row with active mappings: the reason.
    let mut t = table(&[mapping("a", "ta", false)]);
    t.create_new = true;
    t.target_table = String::new();
    assert_eq!(
        mapping_gate_block_reason(&[t]),
        Some("transfer.mapping.targetNameRequired")
    );

    // Nothing mapped yet: no reason (the state the user is working in).
    let t = table(&[mapping("a", "", true)]);
    assert_eq!(mapping_gate_block_reason(&[t]), None);

    // A clearing row: no reason at all.
    let t = table(&[mapping("a", "ta", false)]);
    assert_eq!(mapping_gate_block_reason(&[t]), None);

    // An unnamed create-new with nothing mapped: no reason (the
    // mappings are the missing half, not the name).
    let mut t = table(&[mapping("a", "", true)]);
    t.create_new = true;
    t.target_table = String::new();
    assert_eq!(mapping_gate_block_reason(&[t]), None);
}

/// Keeps the patch type imported: the shell's own tests exercise
/// `apply_to`; this slice pins the view's helpers.
#[test]
fn the_patch_applies_over_the_table() {
    let mut t = table(&[mapping("a", "ta", false)]);
    let patch = ColumnMappingTablePatch {
        target_table: Some("renamed".to_owned()),
        create_new: None,
        column_mappings: None,
        recordset: None,
    };
    patch.apply_to(&mut t);
    assert_eq!(t.target_table, "renamed");
}
