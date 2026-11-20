//! T500: the Data Sync utilities — the schema set of a table list, the
//! default-schema pick chain, the reviewable-state predicate — and the
//! transfer limitation keys.

use crate::views::data_sync::data_sync_compare_step::SyncState;
use crate::views::data_sync::utils::{
    is_compared_sync_state, pick_default_schema, unique_schemas_from_tables,
};
use crate::views::data_transfer::transfer_limitation_keys::TRANSFER_LIMITATION_KEYS;
use crate::views::schema_diff::schema_diff_table_names::TableInfoVm;

fn table(schema: Option<&str>, name: &str) -> TableInfoVm {
    TableInfoVm {
        name: name.to_string(),
        schema: schema.map(str::to_string),
        table_type: crate::views::schema_diff::schema_diff_table_names::TableTypeVm::Table,
        row_count: None,
    }
}

#[test]
fn the_schema_set_deduplicates_and_sorts() {
    let tables = vec![
        table(Some("sales"), "orders"),
        table(Some("public"), "users"),
        table(Some("sales"), "events"),
        table(None, "rootless"),
    ];
    assert_eq!(
        unique_schemas_from_tables(&tables),
        vec!["public".to_string(), "sales".to_string()]
    );
}

#[test]
fn the_schema_set_trims_and_drops_whitespace_only() {
    let tables = vec![
        table(Some("  sales "), "a"),
        table(Some("sales"), "b"),
        table(Some("   "), "c"),
        table(Some(""), "d"),
    ];
    // "  sales " and "sales" trim to the same name (one entry); a
    // whitespace-only and an empty name never enter the set.
    assert_eq!(
        unique_schemas_from_tables(&tables),
        vec!["sales".to_string()]
    );
}

#[test]
fn an_empty_table_list_has_no_schemas() {
    assert!(unique_schemas_from_tables(&[]).is_empty());
}

#[test]
fn the_previous_schema_wins_when_it_survives() {
    let schemas: Vec<String> = ["public".to_string(), "sales".to_string()].to_vec();
    assert_eq!(pick_default_schema(&schemas, Some("sales")), "sales");
    // A previous that is not in the list falls through.
    assert_eq!(pick_default_schema(&schemas, Some("gone")), "public");
    assert_eq!(pick_default_schema(&schemas, None), "public");
}

#[test]
fn public_wins_over_the_first_schema() {
    let schemas: Vec<String> = ["alpha".to_string(), "public".to_string()].to_vec();
    assert_eq!(pick_default_schema(&schemas, None), "public");
    // Without 'public', the first schema.
    let schemas: Vec<String> = ["beta".to_string(), "gamma".to_string()].to_vec();
    assert_eq!(pick_default_schema(&schemas, Some("gamma")), "gamma");
    assert_eq!(pick_default_schema(&schemas, None), "beta");
}

#[test]
fn an_empty_list_picks_the_empty_string() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(pick_default_schema(&empty, Some("anything")), "");
    assert_eq!(pick_default_schema(&empty, None), "");
}

#[test]
fn the_reviewable_states_are_the_four_past_comparison() {
    assert!(!is_compared_sync_state(SyncState::Idle));
    assert!(!is_compared_sync_state(SyncState::Inspecting));
    assert!(!is_compared_sync_state(SyncState::Comparing));
    assert!(is_compared_sync_state(SyncState::Compared));
    assert!(is_compared_sync_state(SyncState::Executing));
    assert!(is_compared_sync_state(SyncState::Unknown));
    assert!(is_compared_sync_state(SyncState::Done));
}

#[test]
fn the_limitation_keys_pin_the_react_order() {
    assert_eq!(
        TRANSFER_LIMITATION_KEYS,
        [
            "transfer.limitations.noViews",
            "transfer.limitations.noFkIndexes",
            "transfer.limitations.crossDialect",
            "transfer.limitations.baseTables",
            "transfer.limitations.noResume",
        ]
    );
}
