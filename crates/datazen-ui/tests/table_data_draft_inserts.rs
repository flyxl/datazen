//! T158: `stores/tableData/draftInserts.ts` + `enumCandidates.ts` ports —
//! draft id minting, the read helpers, every transition's effect and no-op
//! branch, the plan-trio reset, the two collection invariants (drafts never
//! touch `pending_changes` / the order array stays the render order), and
//! the enum-candidate cache semantics.

use serde_json::{json, Value};

use datazen_ui::table_data_connection_state::{
    empty_table_state, RowChangePlan, TableChangeContext, TableState,
};
use datazen_ui::table_data_draft_inserts::{
    clone_draft_values, create_draft_id, draft_insert_count, next_draft_id, ordered_drafts,
    with_draft_rows_from_copy, with_draft_value, with_duplicated_draft, with_new_draft,
    without_draft, DraftRowCopy,
};
use datazen_ui::table_data_enum_candidates::{clear_enum_candidates, set_enum_candidates};

fn context() -> TableChangeContext {
    TableChangeContext {
        connection_id: Some("conn-1".to_owned()),
        db_session_id: "sess-9".to_owned(),
        driver_type: Some("postgres".to_owned()),
        database: Some("app_db".to_owned()),
        schema: Some("public".to_owned()),
        table: "users".to_owned(),
    }
}

fn ts() -> TableState {
    empty_table_state(Some(context()))
}

/// A staged plan, so the reset assertions can watch it disappear.
fn staged_plan() -> RowChangePlan {
    RowChangePlan {
        plan_id: "p1".to_owned(),
        fingerprint: "fp".to_owned(),
        config_revision: Some(4),
        table: context(),
        updates: vec![],
        deletes: vec![],
        inserts: vec![],
        warnings: vec![],
    }
}

// ---- draft id minting ----

#[test]
fn draft_ids_carry_the_counter_and_the_random_suffix() {
    assert_eq!(create_draft_id(1, "ab12cd"), "draft-1-ab12cd");
    assert_eq!(create_draft_id(35, "zzzzzz"), "draft-z-zzzzzz");
    assert_eq!(create_draft_id(36, "000000"), "draft-10-000000");
    assert_eq!(create_draft_id(0, "abc"), "draft-0-abc");
}

#[test]
fn base36_counters_print_like_js_to_string_36() {
    // `Number.toString(36)` spellings: 0, single digits, then two digits.
    assert_eq!(create_draft_id(0, "x"), "draft-0-x");
    assert_eq!(create_draft_id(9, "x"), "draft-9-x");
    assert_eq!(create_draft_id(10, "x"), "draft-a-x");
    assert_eq!(create_draft_id(35, "x"), "draft-z-x");
    assert_eq!(create_draft_id(36, "x"), "draft-10-x");
    assert_eq!(create_draft_id(71, "x"), "draft-1z-x");
    assert_eq!(create_draft_id(123456789, "x"), "draft-21i3v9-x");
}

#[test]
fn next_draft_id_is_strictly_monotonic() {
    let a = next_draft_id("r1");
    let b = next_draft_id("r2");
    let c = next_draft_id("r3");
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_eq!(
        counter_of(&b) + 1,
        counter_of(&c),
        "the global counter advances by one"
    );
    assert_eq!(counter_of(&a) + 1, counter_of(&b));
}

/// The counter half of `draft-<counter>-<random>`, in base 36.
fn counter_of(id: &str) -> u64 {
    let body = id.strip_prefix("draft-").expect("draft- prefix");
    let (counter, _random) = body.split_once('-').expect("counter-random split");
    counter
        .chars()
        .map(|c| match c {
            '0'..='9' => c as u64 - '0' as u64,
            'a'..='z' => c as u64 - 'a' as u64 + 10,
            _ => panic!("not base36: {c}"),
        })
        .fold(0u64, |acc, d| acc * 36 + d)
}

// ---- read helpers ----

#[test]
fn draft_insert_count_is_the_map_size() {
    let mut state = ts();
    assert_eq!(draft_insert_count(&state), 0);
    with_new_draft(&mut state, "d1");
    with_new_draft(&mut state, "d2");
    assert_eq!(draft_insert_count(&state), 2);
}

#[test]
fn ordered_drafts_follow_the_order_array_not_the_map() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_new_draft(&mut state, "d2");
    with_new_draft(&mut state, "d3");
    // Deleting the middle draft leaves its order entry gone too; inserting
    // in a different map order would betray the ordering source.
    without_draft(&mut state, "d2");
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["d1", "d3"]);
}

#[test]
fn ordered_drafts_skip_orphaned_order_entries() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    // An order entry with no draft in the map is skipped (React's
    // `if (draft)` guard), never a panic or a placeholder.
    state.draft_insert_order.push("ghost".to_owned());
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["d1"]);
}

#[test]
fn clone_draft_values_deep_copies_nested_json() {
    let mut values = std::collections::HashMap::new();
    values.insert("tags".to_owned(), json!(["a", ["b", "c"]]));
    values.insert("meta".to_owned(), json!({"k": {"deep": 1}}));
    let mut copy = clone_draft_values(&values);
    // Mutating the copy must not leak into the original.
    if let Some(Value::Array(items)) = copy.get_mut("tags") {
        if let Some(Value::String(s)) = items.first_mut() {
            *s = "mutated".to_owned();
        }
    }
    assert_eq!(
        values.get("tags"),
        Some(&json!(["a", ["b", "c"]])),
        "the original is untouched"
    );
    assert_eq!(
        copy.get("tags"),
        Some(&json!(["mutated", ["b", "c"]])),
        "the mutation landed in the copy only"
    );
}

// ---- transitions ----

#[test]
fn with_new_draft_appends_an_empty_legal_draft() {
    let mut state = ts();
    assert!(with_new_draft(&mut state, "d1"));
    let draft = state.draft_inserts.get("d1").expect("draft stored");
    assert_eq!(draft.draft_id, "d1");
    assert!(draft.values.is_empty(), "an empty draft is a legal insert");
    assert_eq!(state.draft_insert_order, vec!["d1".to_owned()]);
}

#[test]
fn with_draft_value_writes_null_and_removes_on_none() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    // A JSON null is SQL NULL.
    assert!(with_draft_value(&mut state, "d1", "name", Some(&Value::Null)));
    assert_eq!(state.draft_inserts["d1"].values.get("name"), Some(&Value::Null));
    // A real value replaces it.
    assert!(with_draft_value(&mut state, "d1", "name", Some(&json!("Ada"))));
    assert_eq!(state.draft_inserts["d1"].values.get("name"), Some(&json!("Ada")));
    // `None` (React `undefined`) removes the column — "not written".
    assert!(with_draft_value(&mut state, "d1", "name", None));
    assert!(!state.draft_inserts["d1"].values.contains_key("name"));
    assert!(state.draft_inserts["d1"].values.is_empty());
}

#[test]
fn with_draft_value_rejects_absent_drafts_without_touching_state() {
    let mut state = ts();
    let before = state.clone();
    assert!(!with_draft_value(&mut state, "ghost", "name", None));
    assert!(!with_draft_value(&mut state, "ghost", "name", Some(&json!(1))));
    assert_eq!(state, before, "the no-op returns false and changes nothing");
}

#[test]
fn without_draft_removes_the_map_entry_and_the_order_entry() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_new_draft(&mut state, "d2");
    assert!(without_draft(&mut state, "d1"));
    assert!(!state.draft_inserts.contains_key("d1"));
    assert_eq!(state.draft_insert_order, vec!["d2".to_owned()]);
    // A second removal of the same id is a no-op.
    assert!(!without_draft(&mut state, "d1"));
}

#[test]
fn with_draft_rows_from_copy_appends_in_paste_order() {
    let mut state = ts();
    let rows = vec![
        DraftRowCopy {
            draft_id: "c1".to_owned(),
            values: [("name".to_owned(), json!("Row 1"))].into_iter().collect(),
        },
        DraftRowCopy {
            draft_id: "c2".to_owned(),
            values: [("name".to_owned(), Value::Null)].into_iter().collect(),
        },
    ];
    assert!(with_draft_rows_from_copy(&mut state, &rows));
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["c1", "c2"]);
    // A null value stays an explicit SQL NULL, an absent key stays absent.
    assert_eq!(state.draft_inserts["c1"].values.get("name"), Some(&json!("Row 1")));
    assert_eq!(state.draft_inserts["c2"].values.get("name"), Some(&Value::Null));
    // The pasted values are copies, not shared cells.
    let pasted = state.draft_inserts["c1"].values.clone();
    assert_eq!(pasted, rows[0].values);
}

#[test]
fn with_draft_rows_from_copy_is_a_noop_on_an_empty_list() {
    let mut state = ts();
    let before = state.clone();
    assert!(!with_draft_rows_from_copy(&mut state, &[]));
    assert_eq!(state, before);
}

#[test]
fn with_duplicated_draft_deep_copies_and_appends_last() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_draft_value(&mut state, "d1", "tags", Some(&json!(["a", ["b"]])));
    with_new_draft(&mut state, "d9");
    assert!(with_duplicated_draft(&mut state, "d1", "d1-copy"));
    // The copy lands at the end of the order.
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["d1", "d9", "d1-copy"]);
    // The copy's values are a deep copy: mutating the copy's nested array
    // inside the state cannot leak into the source draft.
    if let Some(Value::Array(items)) = state
        .draft_inserts
        .get_mut("d1-copy")
        .and_then(|draft| draft.values.get_mut("tags"))
    {
        if let Some(Value::Array(inner)) = items.get_mut(1) {
            inner.push(Value::String("x".to_owned()));
        }
    }
    assert_eq!(
        state.draft_inserts["d1"].values.get("tags"),
        Some(&json!(["a", ["b"]])),
        "the source draft keeps its own nested array"
    );
    assert_eq!(
        state.draft_inserts["d1-copy"].values.get("tags"),
        Some(&json!(["a", ["b", "x"]])),
        "the mutation landed in the copy only"
    );
}

#[test]
fn with_duplicated_draft_rejects_a_missing_source() {
    let mut state = ts();
    assert!(!with_duplicated_draft(&mut state, "ghost", "new"));
    assert!(state.draft_inserts.is_empty());
    assert!(state.draft_insert_order.is_empty());
}

// ---- the plan-trio reset ----

#[test]
fn every_draft_transition_resets_the_plan_trio() {
    use datazen_ui::table_data_connection_state::PendingStatus;
    let transitions: Vec<Box<dyn Fn(&mut TableState)>> = vec![
        Box::new(|ts: &mut TableState| {
            with_new_draft(ts, "d1");
        }),
        Box::new(|ts: &mut TableState| {
            with_new_draft(ts, "d1");
            with_draft_value(ts, "d1", "name", Some(&json!(1)));
        }),
        Box::new(|ts: &mut TableState| {
            with_new_draft(ts, "d1");
            without_draft(ts, "d1");
        }),
        Box::new(|ts: &mut TableState| {
            with_draft_rows_from_copy(
                ts,
                &[DraftRowCopy {
                    draft_id: "c1".to_owned(),
                    values: Default::default(),
                }],
            );
        }),
        Box::new(|ts: &mut TableState| {
            with_new_draft(ts, "d1");
            with_duplicated_draft(ts, "d1", "d2");
        }),
    ];
    for apply in transitions {
        let mut state = ts();
        // Stage a plan, a busy status, and an error, as the store would
        // before the user edits a draft.
        state.preview_plan = Some(staged_plan());
        state.pending_status = PendingStatus::Previewing;
        state.error = Some("plan stale".to_owned());
        apply(&mut state);
        assert!(state.preview_plan.is_none(), "preview_plan reset");
        assert_eq!(
            state.pending_status,
            PendingStatus::Idle,
            "pending_status back to idle"
        );
        assert!(state.error.is_none(), "error cleared");
    }
}

#[test]
fn draft_transitions_never_touch_pending_changes() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_draft_value(&mut state, "d1", "name", Some(&json!("x")));
    without_draft(&mut state, "d1");
    with_draft_rows_from_copy(
        &mut state,
        &[DraftRowCopy {
            draft_id: "c1".to_owned(),
            values: Default::default(),
        }],
    );
    assert!(
        state.pending_changes.is_empty(),
        "pending_changes stays untouched by every draft transition"
    );
}

// ---- enum candidates ----

#[test]
fn set_enum_candidates_copies_the_list_per_column() {
    let mut state = ts();
    let values = vec!["red".to_owned(), "green".to_owned()];
    set_enum_candidates(&mut state, "color", &values);
    assert_eq!(state.enum_candidates["color"], vec!["red".to_owned(), "green".to_owned()]);
    // The caller's buffer may be reused: the cache holds its own copy.
    let mut reused = values.clone();
    reused.push("blue".to_owned());
    set_enum_candidates(&mut state, "color", &reused);
    assert_eq!(
        state.enum_candidates["color"],
        vec!["red".to_owned(), "green".to_owned(), "blue".to_owned()]
    );
    // Another column caches independently.
    set_enum_candidates(&mut state, "size", &["S".to_owned()]);
    assert_eq!(state.enum_candidates["size"], vec!["S".to_owned()]);
}

#[test]
fn clear_enum_candidates_reports_whether_anything_dropped() {
    let mut state = ts();
    assert!(
        !clear_enum_candidates(&mut state),
        "an empty cache is React's empty Partial: nothing changed"
    );
    set_enum_candidates(&mut state, "color", &["red".to_owned()]);
    assert!(clear_enum_candidates(&mut state));
    assert!(state.enum_candidates.is_empty());
    assert!(!clear_enum_candidates(&mut state));
}
