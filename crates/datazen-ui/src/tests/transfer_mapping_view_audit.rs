//! T305 independent audit: fresh vectors — the normalize's
//! interplay between the listed sources and a repeated stored
//! mapping, the auto-match's matched-then-cleared round trip, the
//! unmapped-targets' skipped-and-empty distinction, and the gate
//! pair's mixed-rows matrix.

use crate::views::{
    auto_match_column_mappings, clear_unmapped_column_mappings, mapping_gate_allows_advance,
    mapping_gate_block_reason, normalize_column_mappings, unmapped_target_columns,
    TransferColumnMapping, TransferMappingStatus, TransferTableVm,
};

fn mapping(source: &str, target: &str, skip: bool) -> TransferColumnMapping {
    TransferColumnMapping {
        source_column: source.to_owned(),
        target_column: target.to_owned(),
        skip,
        target_native_type: None,
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

/// The listed sources win the row set, and a repeated stored mapping
/// keeps only its last entry — even when the list came from the
/// fallback.
#[test]
fn the_normalize_interplay_of_list_and_repeats() {
    // The fallback list dedupes by FIRST-SEEN order…
    let t = table(&[mapping("b", "x", false), mapping("a", "y", false), mapping("b", "z", false)]);
    let normalized = normalize_column_mappings(&t);
    assert_eq!(
        normalized.iter().map(|m| m.source_column.as_str()).collect::<Vec<_>>(),
        vec!["b", "a"]
    );
    // …and the LAST stored mapping for a repeated source wins.
    assert_eq!(normalized[0].target_column, "z");
    assert!(!normalized[0].skip);
    assert_eq!(normalized[1].target_column, "y");

    // An explicit list overrides the fallback entirely — a stored
    // mapping for a source OUTSIDE the list never surfaces.
    let t = table(&[mapping("a", "y", false), mapping("zz", "q", false)]);
    let mut with_list = t.clone();
    with_list.source_columns = Some(vec!["a".to_owned()]);
    let normalized = normalize_column_mappings(&with_list);
    assert_eq!(normalized.len(), 1, "the listed set is the row set");
    assert_eq!(normalized[0].source_column, "a");
}

/// The auto-match → clear round trip: a matched row survives the
/// clear, an unmatched row was already skipped, and a row the user
/// manually mapped to a target that does not exist by name still
/// survives the clear (the clear only empties blank targets).
#[test]
fn the_auto_match_then_clear_round_trip() {
    let sources = vec!["id".to_owned(), "name".to_owned()];
    let targets = vec!["id".to_owned()];
    let existing = vec![mapping("name", "display_name", false)];
    let matched = auto_match_column_mappings(&sources, &targets, &existing);
    // id matched; name was manually mapped (the auto-match DISCARDS
    // the manual target — matched ? source : '').
    assert_eq!(matched[1].target_column, "");
    assert!(matched[1].skip);
    // The clear keeps the shape.
    let cleared = clear_unmapped_column_mappings(&matched);
    assert_eq!(cleared[1].target_column, "");
    assert!(cleared[1].skip);
    // id survives the clear with its matched target.
    assert_eq!(cleared[0].target_column, "id");
    assert!(!cleared[0].skip);

    // A manual mapping to a name NOT in the target list survives the
    // clear (only the blank targets empty).
    let manual = vec![mapping("a", "custom_target", false)];
    let cleared = clear_unmapped_column_mappings(&manual);
    assert_eq!(cleared[0].target_column, "custom_target");
    assert!(!cleared[0].skip);
}

/// The unmapped-targets' distinction: a skipped row's target counts
/// as unmapped, an empty target in an active row counts as nothing
/// (nothing references it), and a disabled… wait — the table-level
/// flag the helper reads is create_new, not enabled.
#[test]
fn the_unmapped_targets_skipped_and_empty_distinction() {
    let t = table(&[mapping("a", "ta", false), mapping("b", "tb", true), mapping("c", "", false)]);
    let mut with_targets = t.clone();
    with_targets.target_columns =
        Some(vec!["ta".to_owned(), "tb".to_owned(), "tc".to_owned()]);
    assert_eq!(
        unmapped_target_columns(&with_targets),
        vec!["tb".to_owned(), "tc".to_owned()],
        "the skipped row's target and the unreferenced one, in target order"
    );

    // An active row with an EMPTY target references nothing.
    let t = table(&[mapping("a", "", false)]);
    let mut with_targets = t.clone();
    with_targets.target_columns = Some(vec!["ta".to_owned()]);
    assert_eq!(unmapped_target_columns(&with_targets), vec!["ta".to_owned()]);
}

/// The gate pair's mixed-rows matrix: one clearing row both allows
/// and silences the reason; the unnamed-create-new reason requires
/// active mappings; a create-new row with a NAME clears on the
/// mappings alone.
#[test]
fn the_gate_pairs_mixed_rows_matrix() {
    // A nameless create-new with mappings + a clearing row: no block.
    let mut unnamed = table(&[mapping("a", "ta", false)]);
    unnamed.create_new = true;
    unnamed.target_table = String::new();
    let clearing = table(&[mapping("b", "tb", false)]);
    assert!(mapping_gate_allows_advance(&[unnamed.clone(), clearing.clone()]));
    assert_eq!(mapping_gate_block_reason(&[unnamed.clone(), clearing]), None);

    // Alone, the unnamed row blocks with the reason.
    assert_eq!(
        mapping_gate_block_reason(&[unnamed.clone()]),
        Some("transfer.mapping.targetNameRequired")
    );

    // A NAMED create-new row with mappings clears (the gate is the
    // mappings' own).
    let mut named = table(&[mapping("a", "ta", false)]);
    named.create_new = true;
    named.target_table = "fresh".to_owned();
    assert!(mapping_gate_allows_advance(&[named]));

    // An unnamed create-new with NOTHING mapped: no reason (the
    // mappings are the missing half).
    let mut empty_named = table(&[mapping("a", "", true)]);
    empty_named.create_new = true;
    empty_named.target_table = String::new();
    assert_eq!(mapping_gate_block_reason(&[empty_named]), None);
}
