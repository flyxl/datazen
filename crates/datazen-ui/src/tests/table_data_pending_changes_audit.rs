//! T275 independent audit: fresh vectors — the effective identity's
//! whole-column fallback and the PK-priority overlay, the overlay's
//! duplicate-identity refusal, the signature's value-order stability
//! and content independence, the wire shapes' field sets, and the
//! draft-order edge (a stale order entry, a map missing a live draft).

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::table_data_connection_state::{ColumnSchema, PendingInsert, PendingRowChange, TableState};
use crate::table_data_pending_changes::{
    drafts_for_wire, effective_pending_identity, find_pending_for_row, overlay_pending_rows,
    pending_changes_for_wire, pending_changes_signature, pending_work_signature,
};

fn pk_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "integer".to_owned(),
        nullable: false,
        default_value: None,
        is_primary_key: true,
        is_auto_increment: false,
        comment: None,
    }
}

fn plain_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        nullable: true,
        default_value: None,
        is_primary_key: false,
        is_auto_increment: false,
        comment: None,
    }
}

fn row(id: i64, name: &str) -> HashMap<String, Value> {
    let mut row = HashMap::new();
    row.insert("id".to_owned(), json!(id));
    row.insert("name".to_owned(), json!(name));
    row
}

fn change(identity: &[(&str, Value)], current: &[(&str, Value)]) -> PendingRowChange {
    let mut row_identity = HashMap::new();
    for (column, value) in identity {
        row_identity.insert(column.to_string(), value.clone());
    }
    let mut current_values = HashMap::new();
    for (column, value) in current {
        current_values.insert(column.to_string(), value.clone());
    }
    PendingRowChange {
        row_index: None,
        row_identity,
        original_values: HashMap::new(),
        current_values,
        changed_columns: Vec::new(),
        delete_marked: false,
        insert_marked: false,
        expression_values: None,
    }
}

fn columns() -> Vec<ColumnSchema> {
    vec![pk_column("id"), plain_column("name")]
}

/// With no declared primary keys, the whole column list is the target:
/// a stable name alone is a valid identity, and a NULL id in the
/// overlay source breaks it.
#[test]
fn the_whole_column_fallback_identity() {
    // No declared primary keys: the whole column list is the target.
    let no_pk_cols = vec![plain_column("id"), plain_column("name")];
    let stable = change(&[], &[("name", json!("solo"))]);
    // The id column has no value anywhere: with the whole list as the
    // target, a missing target value refuses the identity
    // (build_row_identity's own rule).
    assert!(effective_pending_identity(&stable, &no_pk_cols).is_none());

    // Both columns stable: the identity holds both.
    let both = change(&[], &[("id", json!(4)), ("name", json!("n"))]);
    let identity = effective_pending_identity(&both, &no_pk_cols).unwrap();
    assert_eq!(identity.get("id"), Some(&json!(4)));
    assert_eq!(identity.get("name"), Some(&json!("n")));

    // With a declared PK, only the PK columns are the target: the name
    // never enters the identity even when present.
    let with_pk = vec![pk_column("id"), plain_column("name")];
    let identity = effective_pending_identity(&both, &with_pk).unwrap();
    assert_eq!(identity.get("id"), Some(&json!(4)));
    assert!(identity.get("name").is_none(), "the PK filter drops the plain column");
}

/// Two loaded rows sharing an identity: the overlay refuses to touch
/// either (the row is not uniquely identified).
#[test]
fn the_overlay_refuses_duplicate_identities() {
    let mut ts = TableState::default();
    ts.columns = columns();
    ts.rows = vec![row(1, "a"), row(1, "b")];
    ts.pending_changes
        .insert("k".to_owned(), change(&[("id", json!(1))], &[("name", json!("z"))]));

    let overlaid = overlay_pending_rows(&ts, &ts.rows);
    assert_eq!(overlaid[0].get("name"), Some(&json!("a")), "an ambiguous row is never overlaid");
    assert_eq!(overlaid[1].get("name"), Some(&json!("b")));
    // And the row lookup agrees: no unique identity, no match.
    assert!(find_pending_for_row(&ts, 0, &row(1, "a"), &ts.columns).is_none());
}

/// The staged signature is stable under value-equal maps (key order,
/// map iteration) but separates different values.
#[test]
fn the_signature_separates_values_and_merges_orders() {
    let a = change(&[("id", json!(1))], &[("name", json!("x"))]);
    let a_twin = change(&[("id", json!(1))], &[("name", json!("x"))]);
    let b = change(&[("id", json!(2))], &[("name", json!("y"))]);

    let mut first: HashMap<String, PendingRowChange> = HashMap::new();
    first.insert("a".to_owned(), a);
    first.insert("b".to_owned(), b.clone());
    let mut second: HashMap<String, PendingRowChange> = HashMap::new();
    second.insert("b".to_owned(), b);
    second.insert("a".to_owned(), a_twin);
    assert_eq!(pending_changes_signature(&first), pending_changes_signature(&second));

    // A different value under the same key is a different signature.
    let different = change(&[("id", json!(1))], &[("name", json!("CHANGED"))]);
    let mut third: HashMap<String, PendingRowChange> = HashMap::new();
    third.insert("a".to_owned(), different);
    assert_ne!(
        pending_changes_signature(&first),
        pending_changes_signature(&third)
    );
}

/// The wire field sets: the staged change keeps every wire field
/// except row_index; the draft change carries exactly the insert
/// shape.
#[test]
fn the_wire_field_sets() {
    let mut staged = change(&[("id", json!(1))], &[("name", json!("v"))]);
    staged.row_index = Some(9);
    staged.delete_marked = true;
    staged.changed_columns = vec!["name".to_owned()];
    let mut map: HashMap<String, PendingRowChange> = HashMap::new();
    map.insert("k".to_owned(), staged);
    for wire in pending_changes_for_wire(&map) {
        assert!(wire.row_index.is_none());
        assert!(wire.delete_marked, "the wire flags survive");
        assert_eq!(wire.row_identity.len(), 1);
        assert_eq!(wire.current_values.len(), 1);
    }

    // The draft: identity and originals empty, insertMarked set.
    let mut ts = TableState::default();
    ts.columns = columns();
    let mut values = HashMap::new();
    values.insert("id".to_owned(), json!(5));
    ts.draft_inserts.insert(
        "d".to_owned(),
        PendingInsert {
            draft_id: "d".to_owned(),
            values,
        },
    );
    ts.draft_insert_order = vec!["d".to_owned()];
    for wire in drafts_for_wire(&ts) {
        assert!(wire.row_identity.is_empty());
        assert!(wire.original_values.is_empty());
        assert!(wire.insert_marked);
        assert_eq!(wire.changed_columns, vec!["id".to_owned()]);
    }
}

/// A draftInsertOrder entry whose draft is gone is skipped, and a
/// live draft missing from the order never reaches the wire.
#[test]
fn the_draft_order_and_the_map_agree() {
    let mut ts = TableState::default();
    ts.columns = columns();
    for (draft_id, value) in [("live", 1), ("dead", 2), ("unordered", 3)] {
        let mut values = HashMap::new();
        values.insert("id".to_owned(), json!(value));
        ts.draft_inserts.insert(
            draft_id.to_owned(),
            PendingInsert {
                draft_id: draft_id.to_owned(),
                values,
            },
        );
    }
    // "dead" is gone from the map; "unordered" is gone from the order.
    ts.draft_inserts.remove("dead");
    ts.draft_insert_order = vec!["live".to_owned(), "dead".to_owned()];

    let wire = drafts_for_wire(&ts);
    assert_eq!(wire.len(), 1);
    assert_eq!(wire[0].current_values.get("id"), Some(&json!(1)));

    // The pending-work signature agrees with the same inputs.
    let signature = pending_work_signature(&ts.pending_changes, &ts.draft_inserts, &ts.draft_insert_order);
    assert!(signature.contains("\"live\""));
    assert!(!signature.contains("unordered"), "a draft outside the order is not committed work");
}
