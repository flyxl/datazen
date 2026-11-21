//! T275: the pending-changes helpers — coder vectors.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::table_data_connection_state::{ColumnSchema, PendingInsert, PendingRowChange, TableState};
use crate::table_data_pending_changes::{
    drafts_for_wire, effective_pending_identity, find_pending_for_row, has_pending_identity_collision,
    has_pending_work, overlay_pending_rows, pending_changes_for_wire, pending_changes_signature,
    pending_work_signature, row_identity_is_unique, AMBIGUOUS_ROW_IDENTITY_ERROR,
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

/// One staged change: `changed` pairs map original → next.
fn staged(key: &str, identity: &[(&str, Value)], changed: &[(&str, Value, Value)]) -> (String, PendingRowChange) {
    let mut row_identity = HashMap::new();
    for (column, value) in identity {
        row_identity.insert(column.to_string(), value.clone());
    }
    let mut original_values = HashMap::new();
    let mut current_values = HashMap::new();
    let mut changed_columns = Vec::new();
    for (column, original, next) in changed {
        original_values.insert(column.to_string(), original.clone());
        current_values.insert(column.to_string(), next.clone());
        changed_columns.push(column.to_string());
    }
    (
        key.to_owned(),
        PendingRowChange {
            row_index: None,
            row_identity,
            original_values,
            current_values,
            changed_columns,
            delete_marked: false,
            insert_marked: false,
            expression_values: None,
        },
    )
}

fn table_state(
    rows: Vec<HashMap<String, Value>>,
    pending: Vec<(String, PendingRowChange)>,
) -> TableState {
    let mut ts = TableState::default();
    ts.columns = vec![pk_column("id"), plain_column("name")];
    ts.rows = rows;
    ts.pending_changes = pending.into_iter().collect();
    ts
}

#[test]
fn the_ambiguous_error_text_is_verbatim() {
    assert_eq!(
        AMBIGUOUS_ROW_IDENTITY_ERROR,
        "Row identity is NULL, unstable, or ambiguous; UPDATE/DELETE was not staged."
    );
}

#[test]
fn the_effective_identity_overlays_the_current_values() {
    // A staged PK edit: the current value wins over the row identity.
    let (key, change) = staged(
        "old",
        &[("id", json!(1))],
        &[("id", json!(1), json!(7))],
    );
    assert_eq!(key, "old");
    let identity = effective_pending_identity(&change, &[pk_column("id")]).unwrap();
    assert_eq!(identity.get("id"), Some(&json!(7)), "the current value overlays the identity");

    // A change with no current PK value keeps the row identity's own.
    let (_, change) = staged("k", &[("id", json!(1))], &[("name", json!("a"), json!("b"))]);
    let identity = effective_pending_identity(&change, &[pk_column("id")]).unwrap();
    assert_eq!(identity.get("id"), Some(&json!(1)));

    // A NULL current value is a present property: it overlays (and the
    // identity then refuses it — unstable).
    let (_, change) = staged("k", &[("id", json!(1))], &[("id", json!(1), json!(Value::Null))]);
    assert!(effective_pending_identity(&change, &[pk_column("id")]).is_none(),
        "a NULL current PK value makes the identity unstable");

    // No declared PK: the whole column list is the target (the
    // fallback), and an absent row identity yields nothing stable.
    let (_, change) = staged("k", &[], &[("name", json!("a"), json!("b"))]);
    let identity = effective_pending_identity(
        &change,
        &[pk_column("id"), plain_column("name")],
    );
    // id has no value anywhere: the identity refuses.
    assert!(identity.is_none());
}

#[test]
fn identity_uniqueness_ignores_the_callers_own_row() {
    let ts = table_state(vec![row(1, "a"), row(2, "b")], Vec::new());
    let identity: HashMap<String, Value> =
        [("id".to_owned(), json!(1))].into_iter().collect();
    assert!(row_identity_is_unique(&ts, 0, &identity, &[pk_column("id")]),
        "the caller's own index never disqualifies itself");
    assert!(!row_identity_is_unique(&ts, 1, &identity, &[pk_column("id")]),
        "a duplicate elsewhere blocks");
    // A broken identity (a NULL) is never unique.
    let null_identity: HashMap<String, Value> =
        [("id".to_owned(), json!(Value::Null))].into_iter().collect();
    assert!(!row_identity_is_unique(&ts, 0, &null_identity, &[pk_column("id")]));
}

#[test]
fn collision_detection_covers_own_key_and_ambiguous_siblings() {
    let (_, change) = staged("a", &[("id", json!(1))], &[]);
    let ts = table_state(vec![row(1, "a")], vec![("a".to_owned(), change)]);

    // The own key never collides with itself.
    let identity: HashMap<String, Value> =
        [("id".to_owned(), json!(1))].into_iter().collect();
    assert!(!has_pending_identity_collision(&ts, Some("a"), &identity, &[pk_column("id")]));

    // A different key owning the same identity collides.
    assert!(has_pending_identity_collision(&ts, Some("b"), &identity, &[pk_column("id")]));

    // A broken identity always collides (React's early true).
    let null_identity: HashMap<String, Value> =
        [("id".to_owned(), json!(Value::Null))].into_iter().collect();
    assert!(has_pending_identity_collision(&ts, Some("a"), &null_identity, &[pk_column("id")]));

    // An ambiguous sibling (its own effective identity broken) blocks.
    let (_, ambiguous) = staged(
        "amb",
        &[("id", json!(1))],
        &[("id", json!(1), json!(Value::Null))],
    );
    let mut ts_amb = table_state(vec![row(1, "a")], vec![("amb".to_owned(), ambiguous)]);
    ts_amb.pending_changes.remove("a");
    assert!(has_pending_identity_collision(&ts_amb, Some("other"), &identity, &[pk_column("id")]),
        "a sibling with a broken identity blocks staging");
}

#[test]
fn the_anchor_wins_over_the_direct_key_lookup() {
    let identity: HashMap<String, Value> =
        [("id".to_owned(), json!(1))].into_iter().collect();
    // React keys the map by the identity's own key — compute it the
    // same way the lookup does.
    let direct_key = crate::table_data_dirty_index::row_identity_key(&identity).unwrap();
    let (_, direct_change) = staged(&direct_key, &[("id", json!(1))], &[("name", json!("a"), json!("z"))]);
    let ts = table_state(vec![row(1, "a")], vec![(direct_key.clone(), direct_change)]);

    // No anchor: the direct key matches.
    let found = find_pending_for_row(&ts, 0, &row(1, "a"), &[pk_column("id")]).unwrap();
    assert_eq!(found.0, direct_key);

    // With an anchor pointing elsewhere, the anchor wins even though
    // the direct key also matches (a PK edit stays associated with its
    // original row).
    let (_, anchored_change) = staged("anchored", &[("id", json!(9))], &[("name", json!("q"), json!("r"))]);
    let mut ts_anchored = table_state(
        vec![row(1, "a")],
        vec![
            (direct_key.clone(), ts.pending_changes.get(&direct_key).unwrap().clone()),
            ("anchored".to_owned(), anchored_change),
        ],
    );
    ts_anchored.row_identity_anchors.insert(0, "anchored".to_owned());
    let found = find_pending_for_row(&ts_anchored, 0, &row(1, "a"), &[pk_column("id")]).unwrap();
    assert_eq!(found.0, "anchored");

    // No match and no anchor: None.
    let empty = table_state(vec![row(5, "e")], Vec::new());
    assert!(find_pending_for_row(&empty, 0, &row(5, "e"), &[pk_column("id")]).is_none());
}

#[test]
fn the_overlay_merges_uniquely_identified_staged_values() {
    // React keys the map by the identity's own key: compute it the
    // same way the lookup does.
    let identity: HashMap<String, Value> =
        [("id".to_owned(), json!(1))].into_iter().collect();
    let key = crate::table_data_dirty_index::row_identity_key(&identity).unwrap();
    let (_, change) = staged(&key, &[("id", json!(1))], &[("name", json!("a"), json!("edited"))]);
    let ts = table_state(vec![row(1, "a"), row(2, "b")], vec![(key, change)]);

    let overlaid = overlay_pending_rows(&ts, &ts.rows);
    assert_eq!(overlaid[0].get("name"), Some(&json!("edited")));
    assert_eq!(overlaid[1].get("name"), Some(&json!("b")), "the untouched row passes through");

    // No primary keys: the rows pass through unchanged.
    let mut no_pk = table_state(vec![row(1, "a")], Vec::new());
    no_pk.columns = vec![plain_column("name")];
    let (_, c) = staged("k", &[], &[("name", json!("a"), json!("x"))]);
    no_pk.pending_changes.insert("k".to_owned(), c);
    assert_eq!(overlay_pending_rows(&no_pk, &no_pk.rows)[0].get("name"), Some(&json!("a")));

    // Nothing staged: the rows pass through.
    let empty = table_state(vec![row(1, "a")], Vec::new());
    assert_eq!(overlay_pending_rows(&empty, &empty.rows)[0].get("name"), Some(&json!("a")));
}

#[test]
fn the_staged_signature_is_deterministic_across_map_orders() {
    // The same two entries in two different insertion orders produce
    // the same signature (the sort is the whole contract).
    let (_, a) = staged("a", &[("id", json!(1))], &[("name", json!("a"), json!("b"))]);
    let (_, b) = staged("b", &[("id", json!(2))], &[("name", json!("c"), json!("d"))]);
    let mut first: HashMap<String, PendingRowChange> = HashMap::new();
    first.insert("a".to_owned(), a.clone());
    first.insert("b".to_owned(), b.clone());
    let mut second: HashMap<String, PendingRowChange> = HashMap::new();
    second.insert("b".to_owned(), b);
    second.insert("a".to_owned(), a);
    let sig_first = pending_changes_signature(&first);
    let sig_second = pending_changes_signature(&second);
    assert_eq!(sig_first, sig_second);
    assert!(sig_first.contains("\"a\""), "the keys ride the signature");
    assert!(sig_first.starts_with("["), "the signature is a JSON array");

    // An empty map is the empty array.
    assert_eq!(pending_changes_signature(&HashMap::new()), "[]");
}

#[test]
fn the_wire_changes_drop_the_row_index() {
    let mut with_index = staged("k", &[("id", json!(1))], &[("name", json!("a"), json!("b"))]).1;
    with_index.row_index = Some(3);
    let mut map: HashMap<String, PendingRowChange> = HashMap::new();
    map.insert("k".to_owned(), with_index);
    let wire = pending_changes_for_wire(&map);
    assert_eq!(wire.len(), 1);
    assert_eq!(wire[0].row_index, None, "the UI-only row index never goes on the wire");
    assert_eq!(wire[0].changed_columns, vec!["name".to_owned()]);
}

#[test]
fn pending_work_counts_staged_edits_and_drafts_independently() {
    let mut ts = table_state(Vec::new(), Vec::new());
    assert!(!has_pending_work(&ts), "a clean table has no pending work");

    // Drafts alone are pending work (the + row state).
    ts.draft_inserts.insert(
        "d1".to_owned(),
        PendingInsert {
            draft_id: "d1".to_owned(),
            values: HashMap::new(),
        },
    );
    assert!(has_pending_work(&ts));

    // Staged edits alone too.
    let mut ts = table_state(Vec::new(), Vec::new());
    let (_, change) = staged("k", &[("id", json!(1))], &[("name", json!("a"), json!("b"))]);
    ts.pending_changes.insert("k".to_owned(), change);
    assert!(has_pending_work(&ts));
}

#[test]
fn drafts_go_on_the_wire_in_creation_order() {
    let mut ts = table_state(Vec::new(), Vec::new());
    for (draft_id, name) in [("d2", "second"), ("d1", "first"), ("d3", "third")] {
        let mut values = HashMap::new();
        values.insert("name".to_owned(), json!(name));
        ts.draft_inserts.insert(
            draft_id.to_owned(),
            PendingInsert {
                draft_id: draft_id.to_owned(),
                values,
            },
        );
    }
    // The creation order is NOT the id-sorted order and NOT the map's.
    ts.draft_insert_order = vec!["d2".to_owned(), "d1".to_owned(), "d3".to_owned()];

    let wire = drafts_for_wire(&ts);
    assert_eq!(
        wire.iter().map(|c| c.current_values.get("name").and_then(Value::as_str)).collect::<Vec<_>>(),
        vec![Some("second"), Some("first"), Some("third")],
        "the insertion order decides which row gets which value"
    );
    for change in &wire {
        assert!(change.insert_marked, "each draft is marked insertMarked");
        assert!(!change.delete_marked);
        assert!(change.row_identity.is_empty(), "a new row has no identity");
        assert!(change.original_values.is_empty());
        assert!(change.row_index.is_none(), "the draft id and row index never go on the wire");
        // changedColumns is the exact write list: the membership of the
        // draft's values, sorted.
        assert_eq!(change.changed_columns, vec!["name".to_owned()]);
    }

    // A draft id in the order without a draft is skipped.
    ts.draft_insert_order.push("gone".to_owned());
    assert_eq!(drafts_for_wire(&ts).len(), 3);
}

#[test]
fn the_pending_work_signature_covers_both_collections() {
    let (_, change) = staged("k", &[("id", json!(1))], &[("name", json!("a"), json!("b"))]);
    let mut pending: HashMap<String, PendingRowChange> = HashMap::new();
    pending.insert("k".to_owned(), change);
    let mut drafts: HashMap<String, PendingInsert> = HashMap::new();
    let mut values = HashMap::new();
    values.insert("name".to_owned(), json!("draft"));
    drafts.insert(
        "d1".to_owned(),
        PendingInsert {
            draft_id: "d1".to_owned(),
            values,
        },
    );

    let signature = pending_work_signature(&pending, &drafts, &["d1".to_owned()]);
    assert!(signature.contains("draft"), "the draft values ride the signature");
    assert!(signature.contains("\"d1\""), "the draft id rides it too (the order is the intent)");

    // A different draft order is a different signature.
    let mut two = HashMap::new();
    let mut v2 = HashMap::new();
    v2.insert("name".to_owned(), json!("draft"));
    two.insert("d2".to_owned(), PendingInsert {
        draft_id: "d2".to_owned(),
        values: v2,
    });
    let other = pending_work_signature(&pending, &two, &["d2".to_owned()]);
    assert_ne!(signature, other);

    // A missing draft serializes as null (React's `draft ? values : null`).
    let missing = pending_work_signature(&pending, &drafts, &["gone".to_owned()]);
    assert!(missing.contains("null"));
}
