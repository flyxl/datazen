//! T158 independent audit (tester): fresh vectors — base-36 minting at the
//! power-of-36 boundaries, order-array authority under an arbitrary manual
//! order, typed `Value` writes, duplicate-id appends (the React `set` +
//! `push` pair is faithfully duplicated, including its duplicated order
//! entries), a self-duplication, sibling order preservation on removal, an
//! empty enum-candidate list (stored, not dropped), and drafts surviving
//! the plan-trio reset.

use serde_json::json;

use datazen_ui::table_data_connection_state::{empty_table_state, PendingStatus, TableState};
use datazen_ui::table_data_draft_inserts::{
    create_draft_id, draft_insert_count, next_draft_id, ordered_drafts, with_draft_value,
    with_duplicated_draft, with_new_draft, without_draft,
};
use datazen_ui::table_data_enum_candidates::{clear_enum_candidates, set_enum_candidates};

fn ts() -> TableState {
    empty_table_state(None)
}

// ---- base-36 boundaries ----

#[test]
fn base36_boundaries_match_js_to_string_36() {
    // 36^k edges, straight from the positional system.
    assert_eq!(create_draft_id(1296, "r"), "draft-100-r");
    assert_eq!(create_draft_id(46_656, "r"), "draft-1000-r");
    assert_eq!(create_draft_id(1_679_616, "r"), "draft-10000-r");
    assert_eq!(create_draft_id(35, "r"), "draft-z-r");
    assert_eq!(create_draft_id(36 + 35, "r"), "draft-1z-r");
}

#[test]
fn next_draft_id_stays_wellformed_under_repeated_calls() {
    for _ in 0..8 {
        let id = next_draft_id("abc123");
        assert!(id.starts_with("draft-"));
        assert!(id.ends_with("-abc123"));
        let body = &id["draft-".len()..id.len() - "-abc123".len()];
        assert!(
            body.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "counter half stays base-36 lowercase: {body}"
        );
    }
}

// ---- order authority ----

#[test]
fn ordered_drafts_follows_an_arbitrary_manual_order() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_new_draft(&mut state, "d2");
    with_new_draft(&mut state, "d3");
    // The store owns the order array; a hand-built reorder must be obeyed,
    // whatever the map's hash iteration happens to be.
    state.draft_insert_order = vec!["d3".to_owned(), "d1".to_owned(), "d2".to_owned()];
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["d3", "d1", "d2"]);
}

#[test]
fn a_duplicate_id_append_covers_the_map_and_duplicates_the_order() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_draft_value(&mut state, "d1", "name", Some(&json!("first")));
    // React: `draftInserts.set(draftId, …)` overwrites, then
    // `draftInsertOrder.push(draftId)` appends a second entry — the
    // duplicated order entry is the source's own behaviour, not an error.
    with_new_draft(&mut state, "d1");
    assert_eq!(draft_insert_count(&state), 1, "the map holds one draft");
    assert_eq!(state.draft_inserts["d1"].values.get("name"), None,
        "the overwrite is a fresh, empty draft (values reset)");
    assert_eq!(state.draft_insert_order, vec!["d1".to_owned(), "d1".to_owned()]);
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["d1", "d1"], "the same draft renders twice, as React would");
}

#[test]
fn a_self_duplication_covers_the_source_and_appends_the_order() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_draft_value(&mut state, "d1", "name", Some(&json!("self")));
    // Duplicating a draft onto its own id: set(overwrite) + push(again).
    assert!(with_duplicated_draft(&mut state, "d1", "d1"));
    assert_eq!(draft_insert_count(&state), 1);
    assert_eq!(
        state.draft_inserts["d1"].values.get("name"),
        Some(&json!("self")),
        "the copy of itself keeps the values"
    );
    assert_eq!(state.draft_insert_order, vec!["d1".to_owned(), "d1".to_owned()]);
}

#[test]
fn removal_keeps_the_siblings_relative_order() {
    let mut state = ts();
    for id in ["a", "b", "c", "d", "e"] {
        with_new_draft(&mut state, id);
    }
    without_draft(&mut state, "b");
    without_draft(&mut state, "d");
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["a", "c", "e"]);
}

// ---- typed writes ----

#[test]
fn typed_json_values_round_trip_through_draft_cells() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    for (column, value) in [
        ("n", json!(42)),
        ("f", json!(4.5)),
        ("b", json!(false)),
        ("s", json!("text")),
        ("arr", json!([1, 2])),
        ("obj", json!({"k": "v"})),
        ("null", json!(null)),
    ] {
        assert!(with_draft_value(&mut state, "d1", column, Some(&value)));
    }
    let values = &state.draft_inserts["d1"].values;
    assert_eq!(values["n"], json!(42));
    assert_eq!(values["f"], json!(4.5));
    assert_eq!(values["b"], json!(false));
    assert_eq!(values["s"], json!("text"));
    assert_eq!(values["arr"], json!([1, 2]));
    assert_eq!(values["obj"], json!({"k": "v"}));
    assert_eq!(values["null"], json!(null), "a JSON null stays an explicit SQL NULL");
    // Removing it, then writing null back, keeps the two statements distinct.
    assert!(with_draft_value(&mut state, "d1", "null", None));
    assert!(
        !state.draft_inserts["d1"].values.contains_key("null"),
        "removed: not written"
    );
    assert!(with_draft_value(&mut state, "d1", "null", Some(&json!(null))));
    assert_eq!(state.draft_inserts["d1"].values["null"], json!(null));
}

// ---- drafts survive the reset ----

#[test]
fn the_plan_trio_reset_leaves_the_drafts_alone() {
    let mut state = ts();
    with_new_draft(&mut state, "d1");
    with_draft_value(&mut state, "d1", "name", Some(&json!("kept")));
    state.pending_status = PendingStatus::Committing;
    state.error = Some("err".to_owned());
    // The next transition resets the trio but must not touch the drafts.
    with_new_draft(&mut state, "d2");
    assert_eq!(state.pending_status, PendingStatus::Idle);
    assert!(state.error.is_none());
    assert_eq!(state.draft_inserts["d1"].values.get("name"), Some(&json!("kept")));
    assert_eq!(state.draft_inserts["d2"].values.len(), 0);
    let order: Vec<&str> = ordered_drafts(&state).iter().map(|d| d.draft_id.as_str()).collect();
    assert_eq!(order, vec!["d1", "d2"]);
}

// ---- enum candidate edges ----

#[test]
fn an_empty_candidate_list_is_stored_not_dropped() {
    let mut state = ts();
    set_enum_candidates(&mut state, "color", &[]);
    // React stores `[...values]` = `[]`: an empty entry exists; only clear
    // (or a page/table change) removes it.
    assert!(state.enum_candidates.contains_key("color"));
    assert!(state.enum_candidates["color"].is_empty());
    assert!(clear_enum_candidates(&mut state));
    assert!(!state.enum_candidates.contains_key("color"));
}

#[test]
fn clearing_one_column_requires_reseting_the_whole_panel_cache() {
    let mut state = ts();
    set_enum_candidates(&mut state, "a", &["1".to_owned()]);
    set_enum_candidates(&mut state, "b", &["2".to_owned()]);
    // React has no per-column forget: the slice only clears the panel.
    assert!(clear_enum_candidates(&mut state));
    assert!(!state.enum_candidates.contains_key("a"));
    assert!(!state.enum_candidates.contains_key("b"));
}
