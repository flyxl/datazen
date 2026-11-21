//! T500 audit (tester): independent checks of the Data Sync utilities and
//! the transfer limitation keys. Fresh vectors: numeric-prefixed sort
//! order, a 'public'-only list, a previous that equals the empty string,
//! and the limitation keys' i18n resolvability (every key must exist in the
//! zh-CN packs — the sentences the window actually renders).

use crate::i18n::has;
use crate::views::data_sync::data_sync_compare_step::SyncState;
use crate::views::data_sync::utils::{
    is_compared_sync_state, pick_default_schema, unique_schemas_from_tables,
};
use crate::views::data_transfer::transfer_limitation_keys::TRANSFER_LIMITATION_KEYS;
use crate::views::schema_diff::schema_diff_table_names::TableInfoVm;

fn audit_table(schema: Option<&str>) -> TableInfoVm {
    TableInfoVm {
        name: "t".to_string(),
        schema: schema.map(str::to_string),
        table_type: crate::views::schema_diff::schema_diff_table_names::TableTypeVm::View,
        row_count: None,
    }
}

#[test]
fn audit_the_sort_is_bytewise_ascending() {
    let tables = vec![
        audit_table(Some("b10")),
        audit_table(Some("b2")),
        audit_table(Some("B")),
        audit_table(Some("a")),
    ];
    // JS sort() on unique strings: uppercase sorts before lowercase
    // (UTF-16 code units), and "B" < "a" bytewise too.
    assert_eq!(
        unique_schemas_from_tables(&tables),
        vec!["B".to_string(), "a".to_string(), "b10".to_string(), "b2".to_string()]
    );
}

#[test]
fn audit_a_public_only_list_picks_public_even_with_a_previous() {
    let schemas = vec!["public".to_string()];
    assert_eq!(pick_default_schema(&schemas, Some("public")), "public");
    // A previous that is absent cannot win: 'public' is the pick.
    assert_eq!(pick_default_schema(&schemas, Some("other")), "public");
}

#[test]
fn audit_an_empty_previous_does_not_match_the_empty_list() {
    // previous='' is falsy in React (`previous && ...`): it must not be
    // returned even when the list is empty; '' comes from the final
    // fallback instead, which is the same string here — so pin the branch
    // with a NON-empty list: '' is not in it, 'public' is.
    let schemas = vec!["public".to_string(), "x".to_string()];
    assert_eq!(pick_default_schema(&schemas, Some("")), "public");
}

#[test]
fn audit_the_predicate_is_false_only_before_comparison() {
    // The complement view: exactly the three states before comparison are
    // not reviewable — everything else is.
    for state in [SyncState::Idle, SyncState::Inspecting, SyncState::Comparing] {
        assert!(!is_compared_sync_state(state));
    }
    for state in [
        SyncState::Compared,
        SyncState::Executing,
        SyncState::Unknown,
        SyncState::Done,
    ] {
        assert!(is_compared_sync_state(state));
    }
}

#[test]
fn audit_every_limitation_key_resolves_in_the_packs() {
    // The keys are useless unless the zh-CN packs define their sentences —
    // this is the observation the window renders on.
    assert_eq!(TRANSFER_LIMITATION_KEYS.len(), 5);
    for key in TRANSFER_LIMITATION_KEYS {
        assert!(has(key), "{key} has no zh-CN entry — the window would render the raw key");
    }
}

#[test]
fn audit_the_keys_are_distinct() {
    let mut keys = TRANSFER_LIMITATION_KEYS.to_vec();
    keys.sort();
    let before = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), before, "no two limitation keys may coincide");
}
