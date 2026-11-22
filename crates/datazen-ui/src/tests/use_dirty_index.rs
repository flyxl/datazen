//! T614: pure `useDirtyIndex` / `dirtyIndex` logic — cell keys, really-changed
//! columns, delete marks, ambiguous identities, anchors, off-page counts,
//! structural sharing and the empty index. No store, IPC or selection state.

use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{json, Value};

use crate::views::use_dirty_index::{
    build_dirty_index, dirty_cell_key, select_dirty_counts, select_dirty_row, values_equal,
    DirtyColumnVm, DirtyIndexVm, DirtyRowState, DirtyTableVm, PendingChangeVm,
};

fn columns() -> Vec<DirtyColumnVm> {
    vec![
        DirtyColumnVm { name: "id".to_owned(), is_primary_key: true },
        DirtyColumnVm { name: "name".to_owned(), is_primary_key: false },
        DirtyColumnVm { name: "score".to_owned(), is_primary_key: false },
    ]
}

fn row(id: i64, name: Option<&str>, score: i64) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    values.insert("id".to_owned(), json!(id));
    values.insert("name".to_owned(), name.map_or(Value::Null, |n| json!(n)));
    values.insert("score".to_owned(), json!(score));
    values
}

fn change(original: &[(&str, Value)], current: &[(&str, Value)], changed: &[&str]) -> PendingChangeVm {
    PendingChangeVm {
        row_identity: BTreeMap::from([("id".to_owned(), json!(1))]),
        original_values: original.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
        current_values: current.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
        changed_columns: changed.iter().map(|c| (*c).to_owned()).collect(),
        delete_marked: false,
        expression_values: BTreeMap::new(),
    }
}

fn table(rows: Vec<BTreeMap<String, Value>>, changes: &[(&str, PendingChangeVm)]) -> DirtyTableVm {
    DirtyTableVm {
        columns: columns(),
        rows,
        pending_changes: changes.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
        row_identity_anchors: BTreeMap::new(),
    }
}

#[test]
fn cell_keys_are_injective_and_values_equal_follows_json() {
    const NUL: char = '\0';
    let identity = "[\"id\":1]";
    let keys = [
        dirty_cell_key(identity, &format!("a{NUL}b")),
        dirty_cell_key(identity, "a:b"),
        dirty_cell_key(identity, "a|b"),
        dirty_cell_key(identity, "a,b"),
        dirty_cell_key(identity, "a\"b"),
        dirty_cell_key(identity, ""),
        dirty_cell_key("[\"id\":2]", "x"),
    ];
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(unique.len(), keys.len(), "NUL join is injective");
    assert_eq!(dirty_cell_key(identity, "name"), format!("{identity}{NUL}name"));

    assert!(values_equal(None, Some(&Value::Null)), "undefined and NULL compare equal");
    assert!(values_equal(Some(&json!(f64::NAN)), Some(&Value::Null)), "NaN serialises to null");
    assert!(!values_equal(Some(&json!("1")), Some(&json!(1))), "type-sensitive");
    assert!(!values_equal(Some(&json!("")), Some(&Value::Null)), "empty string is a change");
}

#[test]
fn really_changed_delete_marks_and_off_page_counts() {
    let ts = table(
        vec![row(1, Some("Alice"), 10)],
        &[(
            "[\"id\":1]",
            change(&[("name", json!("Alice")), ("score", json!(10))], &[("name", json!("Alicia")), ("score", json!(10))], &["name", "score"]),
        )],
    );
    let index = build_dirty_index(&ts, None);
    let entry = index.by_row_index.get(&0).expect("row 0 marked");
    assert_eq!(entry.state, DirtyRowState::Dirty);
    assert_eq!(entry.changed_columns, vec!["name"]);
    assert_eq!(entry.cells.len(), 1);
    assert_eq!(index.counts.total_cells, 1);
    assert_eq!(index.counts.cells_on_page, 1);
    assert_eq!(index.counts.cells_off_page, 0);
    assert!(index.off_page_keys.is_empty());

    let mut deleted = change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"]);
    deleted.delete_marked = true;
    let ts = table(vec![row(1, Some("Alice"), 10)], &[("[\"id\":1]", deleted)]);
    let index = build_dirty_index(&ts, None);
    let entry = index.by_row_index.get(&0).expect("deleted row marked");
    assert_eq!(entry.state, DirtyRowState::Deleted);
    assert!(entry.changed_columns.is_empty());
    assert!(entry.cells.is_empty());
    assert_eq!(index.counts.delete_rows, 1);
    assert_eq!(index.counts.update_rows, 0);
    assert_eq!(index.counts.total_cells, 0);

    let off = table(vec![row(2, Some("Bob"), 20)], &[("[\"id\":1]", change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"]))]);
    let index = build_dirty_index(&off, None);
    assert_eq!(index.counts.cells_on_page, 0);
    assert_eq!(index.counts.cells_off_page, 1);
    assert_eq!(index.off_page_keys, vec!["[\"id\":1]"]);
}

#[test]
fn ambiguous_rows_and_no_primary_key_are_off_page() {
    let ts = table(
        vec![row(1, Some("Alice"), 10), row(1, Some("Bob"), 20)],
        &[("[\"id\":1]", change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"]))],
    );
    let index = build_dirty_index(&ts, None);
    assert!(index.by_row_index.is_empty(), "ambiguous identity is never marked");
    assert_eq!(index.off_page_keys, vec!["[\"id\":1]"]);
    assert_eq!(index.counts.total_cells, 1, "the staged cell still counts");

    let mut no_pk = table(vec![row(1, Some("Alice"), 10)], &[("[\"id\":1]", change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"]))]);
    no_pk.columns = vec![DirtyColumnVm { name: "name".to_owned(), is_primary_key: false }];
    let index = build_dirty_index(&no_pk, None);
    assert!(index.by_row_index.is_empty());
    assert_eq!(index.off_page_keys, vec!["[\"id\":1]"], "no primary key -> all off-page");
}

#[test]
fn anchors_override_identity_and_entries_are_structurally_shared() {
    let mut ts = table(vec![row(2, Some("Alicia"), 10)], &[(
        "[\"id\":1]",
        change(&[("id", json!(1))], &[("id", json!(2))], &["id"]),
    )]);
    ts.row_identity_anchors.insert(0, "[\"id\":1]".to_owned());
    let index = build_dirty_index(&ts, None);
    let entry = index.by_row_index.get(&0).expect("anchored row marked");
    assert_eq!(entry.identity_key, "[\"id\":1]");
    assert_eq!(entry.changed_columns, vec!["id"]);

    let two_rows = table(
        vec![row(1, Some("Alice"), 10), row(2, Some("Bob"), 20)],
        &[
            ("[\"id\":1]", change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"])),
            ("[\"id\":2]", change(&[("name", json!("Bob"))], &[("name", json!("Bobby"))], &["name"])),
        ],
    );
    let first = build_dirty_index(&two_rows, None);
    let second_input = table(
        vec![row(1, Some("Alicia"), 10), row(2, Some("Bobby"), 20)],
        &[
            ("[\"id\":1]", change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"])),
            ("[\"id\":2]", change(&[("name", json!("Bob"))], &[("name", json!("Robert"))], &["name"])),
        ],
    );
    let second = build_dirty_index(&second_input, Some(&first));
    let first_row0: Rc<_> = first.by_row_index.get(&0).expect("first row 0").clone();
    let second_row0 = second.by_row_index.get(&0).expect("second row 0").clone();
    assert!(Rc::ptr_eq(&first_row0, &second_row0), "untouched entry keeps identity");
    let second_row1 = second.by_row_index.get(&1).expect("second row 1").clone();
    let first_row1 = first.by_row_index.get(&1).expect("first row 1").clone();
    assert!(!Rc::ptr_eq(&first_row1, &second_row1), "changed entry is new");
}

#[test]
fn empty_index_counts_and_selectors() {
    let empty = build_dirty_index(&table(vec![row(1, Some("Alice"), 10)], &[]), None);
    assert_eq!(empty, DirtyIndexVm::default());
    assert!(empty.by_row_index.is_empty());
    assert!(empty.off_page_keys.is_empty());
    assert_eq!(empty.counts.total_rows, 0);

    let counts = select_dirty_counts(&BTreeMap::from([
        ("[\"id\":1]".to_owned(), change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"])),
        ("[\"id\":2]".to_owned(), { let mut c = change(&[], &[], &[]); c.row_identity.insert("id".to_owned(), json!(2)); c.delete_marked = true; c }),
    ]));
    assert_eq!((counts.update_rows, counts.delete_rows, counts.total_rows), (1, 1, 2));
    assert_eq!(counts.total_cells, 1);
    assert_eq!(counts.cells_on_page, 0, "counts alone know no page");
    assert_eq!(counts.cells_off_page, 1);

    let ts = table(vec![row(1, Some("Alice"), 10)], &[("[\"id\":1]", change(&[("name", json!("Alice"))], &[("name", json!("Alicia"))], &["name"]))]);
    let index = build_dirty_index(&ts, None);
    assert_eq!(select_dirty_row(Some(&index), 0).map(|entry| entry.state), Some(DirtyRowState::Dirty));
    assert!(select_dirty_row(Some(&index), 7).is_none());
    assert!(select_dirty_row(None, 0).is_none());
}
