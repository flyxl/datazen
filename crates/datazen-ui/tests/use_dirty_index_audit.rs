//! Independent audit for T614: pure `useDirtyIndex` / dirtyIndex logic.
//!
//! Pin expression/count branches and two React contracts the port diverges on:
//! `sameCells` ignoring expression-only changes, and JSON-escaped string PKs in
//! `stableSerialize`/`rowIdentityKey`.

use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{json, Value};

use datazen_ui::views::use_dirty_index::{
    build_dirty_index, dirty_cell_key, values_equal, DirtyColumnVm, DirtyRowState, DirtyTableVm,
    PendingChangeVm,
};

fn pk_column(name: &str) -> DirtyColumnVm {
    DirtyColumnVm { name: name.to_owned(), is_primary_key: true }
}

fn change(key: &str, original: &[(&str, Value)], current: &[(&str, Value)], changed: &[&str]) -> DirtyTableVm {
    let c = PendingChangeVm {
        original_values: original.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
        current_values: current.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
        changed_columns: changed.iter().map(|c| (*c).to_owned()).collect(),
        ..Default::default()
    };
    DirtyTableVm {
        columns: vec![pk_column("id"), DirtyColumnVm { name: "name".to_owned(), is_primary_key: false }],
        rows: vec![{
            let mut r = BTreeMap::new();
            r.insert("id".to_owned(), json!(1));
            r.insert("name".to_owned(), json!("Alice"));
            r
        }],
        pending_changes: BTreeMap::from([(key.to_owned(), c)]),
        ..Default::default()
    }
}

const KEY: &str = "[\"id\":1]";

#[test]
fn expression_only_change_is_a_real_change() {
    let mut ts = change(KEY, &[("name", json!("Alice"))], &[("name", json!("Alice"))], &["name"]);
    ts.pending_changes.get_mut(KEY).unwrap().expression_values.insert("name".to_owned(), "expr-1".to_owned());
    let index = build_dirty_index(&ts, None);
    let entry = index.by_row_index.get(&0).expect("expression-only change still marks the row");
    assert_eq!(entry.changed_columns, vec!["name"], "audit: a staged expression keeps its column");
    assert_eq!(entry.cells[0].expression.as_deref(), Some("expr-1"));
    assert_eq!(index.counts.total_cells, 1);
}

#[test]
fn deleted_row_entries_are_structurally_shared() {
    let mut ts = change(KEY, &[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"]);
    ts.pending_changes.get_mut(KEY).unwrap().delete_marked = true;
    let first = build_dirty_index(&ts, None);
    assert_eq!(first.by_row_index[&0].state, DirtyRowState::Deleted);
    let second = build_dirty_index(&ts, Some(&first));
    assert!(
        Rc::ptr_eq(&first.by_row_index[&0], &second.by_row_index[&0]),
        "audit: an unchanged deleted entry keeps identity"
    );
}

#[test]
fn value_equality_edges_and_cell_keys() {
    assert!(values_equal(Some(&json!(true)), Some(&json!(true))));
    assert!(!values_equal(Some(&json!(true)), Some(&json!(false))));
    assert!(values_equal(Some(&json!([1, 2])), Some(&json!([1, 2]))));
    assert!(!values_equal(Some(&json!([1, 2])), Some(&json!([2, 1]))));
    assert_ne!(
        dirty_cell_key("[\"id\":1]", "a\0b"),
        dirty_cell_key("[\"id\":1]", "a"),
        "audit: NUL stays a distinct separator"
    );
}

// ----- React parity assertions -----

#[test]
fn react_structural_sharing_ignores_expression_only_changes() {
    let first_input = change(KEY, &[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"]);
    let first = build_dirty_index(&first_input, None);
    let mut second_input = first_input.clone();
    second_input.pending_changes.get_mut(KEY).unwrap().expression_values.insert("name".to_owned(), "expr-1".to_owned());
    let second = build_dirty_index(&second_input, Some(&first));
    assert!(
        Rc::ptr_eq(&first.by_row_index[&0], &second.by_row_index[&0]),
        "DEFECT: React sameCells compares only column/name/values, not expression (dirtyIndex.ts:203-212)"
    );
}

#[test]
fn react_identity_key_escapes_quotes_in_string_pk() {
    // React rowIdentityKey({name: 'a"b'}) == ["name":"a\"b"] (JSON.stringify escapes the quote).
    let react_key = "[\"name\":\"a\\\"b\"]";
    let c = PendingChangeVm {
        original_values: BTreeMap::from([("name".to_owned(), json!("a\"b"))]),
        current_values: BTreeMap::from([("name".to_owned(), json!("z"))]),
        changed_columns: vec!["name".to_owned()],
        ..Default::default()
    };
    let ts = DirtyTableVm {
        columns: vec![pk_column("name")],
        rows: vec![BTreeMap::from([("name".to_owned(), json!("a\"b"))])],
        pending_changes: BTreeMap::from([(react_key.to_owned(), c)]),
        ..Default::default()
    };
    let index = build_dirty_index(&ts, None);
    assert!(
        index.by_row_index.contains_key(&0),
        "DEFECT: React stableSerialize JSON-escapes string identity values (tableChanges.ts:203-204)"
    );
}
