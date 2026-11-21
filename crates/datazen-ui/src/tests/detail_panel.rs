//! T236: the detail drawer's coder tests — the pure derivations (the search
//! filter, the multi-row gate, the field classification, the display-text
//! chain, the blob line, the commit semantics, the editor key decisions) and
//! the state model (BUG-005's enter/exit cycle), plus runtime probes of the
//! mounted drawer (the closed gate, the no-selection arm, the field list,
//! the search filter, the quick-look record, the press-to-edit flow, the
//! Enter commit, the presentation entry's host call).

use rune::{col, row, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::{json, Value};

use crate::components::data_table::detail_panel::{
    blob_byte_count, blob_line, commit_field_edit, displayed_text, editor_key_decision,
    filtered_columns, field_facts, multi_row, FieldCommit, EditorKeyDecision,
};
use crate::components::data_table::quick_look_dialog::{
    is_blob_type, is_json_value, pretty_json,
};
use crate::components::data_table::table_header::Column;

// ---------------------------------------------------------------------------
// The search filter
// ---------------------------------------------------------------------------

fn columns(names: &[&str]) -> Vec<Column> {
    names
        .iter()
        .map(|name| Column::new(format!("{name}-id"), *name).ty("integer"))
        .collect()
}

#[test]
fn a_blank_query_keeps_every_column() {
    let all = columns(&["id", "name", "amount"]);
    assert_eq!(filtered_columns(&all, "").len(), 3);
    assert_eq!(filtered_columns(&all, "   ").len(), 3, "a blank trim is no filter");
}

#[test]
fn the_filter_is_a_case_insensitive_substring_on_the_name() {
    let all = columns(&["id", "CustomerName", "amount"]);
    let kept = filtered_columns(&all, "name");
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].name, "CustomerName");
    let kept = filtered_columns(&all, "CUSTOMER");
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].name, "CustomerName");
}

#[test]
fn a_query_that_matches_nothing_leaves_nothing() {
    let all = columns(&["id", "name"]);
    assert!(filtered_columns(&all, "zzz").is_empty());
}

// ---------------------------------------------------------------------------
// The multi-row gate and the badge text
// ---------------------------------------------------------------------------

#[test]
fn the_bulk_badge_needs_a_row_and_more_than_one_selected() {
    assert!(!multi_row(None, 5));
    assert!(!multi_row(Some(2), 1));
    assert!(!multi_row(Some(2), 0));
    assert!(multi_row(Some(2), 2));
    assert!(multi_row(Some(2), 7));
}

#[test]
fn the_badge_text_formats_the_count_placeholder() {
    let text = crate::components::data_table::detail_panel::editing_rows_text(3);
    assert!(!text.contains("{count}"), "{text} keeps the placeholder");
    assert!(text.contains('3'));
}

// ---------------------------------------------------------------------------
// The Quick Look helpers (T378's file, started here)
// ---------------------------------------------------------------------------

#[test]
fn blob_types_are_substring_verdicts_on_the_type_name() {
    for name in ["blob", "bytea", "binary", "image", "raw", "longblob"] {
        assert!(is_blob_type(name), "{name} reads as binary");
    }
    assert!(!is_blob_type("integer"));
    assert!(!is_blob_type("text"));
    assert!(!is_blob_type(""));
}

#[test]
fn json_values_are_objects_arrays_or_json_strings() {
    assert!(is_json_value(&json!({"a": 1})));
    assert!(is_json_value(&json!([1, 2])));
    assert!(is_json_value(&json!("{\"a\": 1}")));
    assert!(is_json_value(&json!("  [1] ")), "a padded string is trimmed first");
    assert!(!is_json_value(&Value::Null));
    assert!(!is_json_value(&json!(5)));
    assert!(!is_json_value(&json!("plain text")));
    assert!(!is_json_value(&json!("{not json")));
    assert!(!is_json_value(&json!("[5, 6, 7]".to_owned() + " trailing")), "a string that does not parse is not JSON");
}

#[test]
fn pretty_json_prints_two_space_forms_and_refuses_the_rest() {
    let pretty = pretty_json(&json!({"b": 2, "a": 1})).expect("an object pretty-prints");
    assert!(pretty.starts_with('{'));
    assert!(pretty.contains("\n  "), "the indent is two spaces");
    let from_string = pretty_json(&json!("{\"a\": 1}")).expect("a JSON string pretty-prints");
    assert!(from_string.contains("\"a\""));
    assert!(pretty_json(&json!(5)).is_none(), "a number is not an object");
    assert!(pretty_json(&json!("text")).is_none());
    assert!(pretty_json(&json!("{oops")).is_none());
    assert!(pretty_json(&Value::Null).is_none());
}

// ---------------------------------------------------------------------------
// The field classification
// ---------------------------------------------------------------------------

#[test]
fn a_null_value_is_null_for_every_verdict_that_cares() {
    let facts = field_facts(Some("integer"), &Value::Null);
    assert!(facts.is_null);
    assert!(!facts.is_json);
    assert!(!facts.can_quick_look, "NULL never quick-looks");
    assert!(facts.pretty.is_none());
}

#[test]
fn the_type_name_and_the_value_both_decide_json() {
    let by_type = field_facts(Some("json"), &json!("text"));
    assert!(by_type.is_json);
    assert!(by_type.is_long_text, "JSON is long text");
    assert!(by_type.can_quick_look);

    let by_value = field_facts(Some("varchar"), &json!({"a": 1}));
    assert!(by_value.is_json, "a JSON value reads as JSON whatever the column says");
    assert!(by_value.is_long_text);
    assert!(by_value.can_quick_look);
}

#[test]
fn blob_types_are_blobs_and_long_text_and_quick_look() {
    let facts = field_facts(Some("bytea"), &json!("x'01'"));
    assert!(facts.is_blob);
    assert!(facts.is_long_text);
    assert!(facts.can_quick_look);
}

#[test]
fn plain_text_columns_are_long_text_without_quick_look() {
    let facts = field_facts(Some("text"), &json!("short"));
    assert!(facts.is_long_text);
    assert!(!facts.is_json);
    assert!(!facts.is_blob);
    assert!(!facts.can_quick_look, "a short string has no viewer");
}

#[test]
fn a_long_string_opens_the_quick_look_gate_at_the_boundary() {
    let short = "x".repeat(120);
    let long = "x".repeat(121);
    assert!(!field_facts(Some("varchar"), &json!(short)).can_quick_look);
    assert!(field_facts(Some("varchar"), &json!(long)).can_quick_look, "121 code points is over the 120 gate");
    // Code points, not React's UTF-16 units (the module's documented
    // convention): 61 emoji are 61 here (122 in React's `.length`), so the
    // gate stays closed below the boundary either way.
    let emoji = "🙂".repeat(61);
    assert!(!field_facts(Some("varchar"), &json!(emoji)).can_quick_look);
    let emoji = "🙂".repeat(121);
    assert!(field_facts(Some("varchar"), &json!(emoji)).can_quick_look);
}

// ---------------------------------------------------------------------------
// The display text chain
// ---------------------------------------------------------------------------

#[test]
fn a_null_value_displays_null_before_any_plan() {
    let facts = field_facts(Some("integer"), &Value::Null);
    assert_eq!(displayed_text(&facts, &Value::Null, None), "NULL");
}

#[test]
fn format_cell_covers_the_plain_value_when_no_plan_and_no_pretty() {
    let facts = field_facts(Some("integer"), &json!(7));
    assert_eq!(displayed_text(&facts, &json!(7), None), "7");
}

#[test]
fn pretty_json_wins_over_format_cell_for_json_values() {
    let value = json!({"a": 1});
    let facts = field_facts(Some("json"), &value);
    let text = displayed_text(&facts, &value, None);
    assert!(text.contains("\"a\": 1"), "{text} is the pretty form, not the compact one");
}

// ---------------------------------------------------------------------------
// The blob line
// ---------------------------------------------------------------------------

#[test]
fn a_string_counts_its_length_and_a_non_string_its_formatted_form() {
    assert_eq!(blob_byte_count(&json!("abcde")), 5);
    assert_eq!(blob_byte_count(&json!(7)), 1, "formatCell(7) is \"7\"");
    assert_eq!(blob_byte_count(&json!(true)), 4, "formatCell(true) is \"true\"");
}

#[test]
fn the_blob_line_is_the_packed_text() {
    let line = blob_line(&json!("abcde"));
    assert!(line.contains("abcde".len().to_string().as_str()), "{line} carries the count");
}

// ---------------------------------------------------------------------------
// The commit semantics
// ---------------------------------------------------------------------------

fn commit(ty: &str, trimmed: &str, display: &str, null: bool) -> FieldCommit {
    commit_field_edit(ty, trimmed, display, null)
}

#[test]
fn an_unchanged_or_blank_on_null_edit_commits_nothing() {
    assert_eq!(commit("integer", "7", "7", false), FieldCommit::NoChange);
    assert_eq!(commit("integer", "", "", true), FieldCommit::NoChange);
}

#[test]
fn an_empty_edit_on_a_non_null_value_commits_null() {
    assert!(matches!(commit("integer", "", "7", false), FieldCommit::Null));
}

#[test]
fn integer_serial_and_bigint_types_commit_numbers() {
    assert_eq!(commit("integer", "3", "7", false), FieldCommit::Value(json!(3)));
    assert_eq!(commit("bigserial", "3", "7", false), FieldCommit::Value(json!(3)));
    assert_eq!(commit("bigint", "3", "7", false), FieldCommit::Value(json!(3)));
    assert_eq!(commit("int", "-12", "7", false), FieldCommit::Value(json!(-12)));
    // JS commits NaN for unparseable numbers, and the wire carries it as
    // null.
    assert!(matches!(commit("integer", "abc", "7", false), FieldCommit::Null));
}

#[test]
fn float_families_commit_numbers_and_keep_their_fraction() {
    assert_eq!(commit("float", "2.5", "7", false), FieldCommit::Value(json!(2.5)));
    assert_eq!(commit("double precision", "2.5", "7", false), FieldCommit::Value(json!(2.5)));
    assert_eq!(commit("numeric", "2.5", "7", false), FieldCommit::Value(json!(2.5)));
    assert_eq!(commit("decimal", "2.5", "7", false), FieldCommit::Value(json!(2.5)));
    assert_eq!(commit("real", "2.5", "7", false), FieldCommit::Value(json!(2.5)));
}

#[test]
fn bool_types_commit_the_string_comparison() {
    assert_eq!(commit("boolean", "true", "false", false), FieldCommit::Value(json!(true)));
    assert_eq!(commit("boolean", "anything", "false", false), FieldCommit::Value(json!(false)));
}

#[test]
fn json_types_commit_the_parse_or_the_raw_string() {
    assert_eq!(
        commit("json", "{\"a\": 1}", "7", false),
        FieldCommit::Value(json!({"a": 1}))
    );
    assert_eq!(
        commit("json", "not json", "7", false),
        FieldCommit::Value(json!("not json"))
    );
}

#[test]
fn every_other_type_commits_the_string() {
    assert_eq!(
        commit("varchar", "hello", "7", false),
        FieldCommit::Value(json!("hello"))
    );
}

// ---------------------------------------------------------------------------
// The editor key decisions
// ---------------------------------------------------------------------------

#[test]
fn escape_cancels_and_enter_commits_only_single_line_fields() {
    assert_eq!(editor_key_decision(&Key::Escape, false), EditorKeyDecision::Cancel);
    assert_eq!(editor_key_decision(&Key::Escape, true), EditorKeyDecision::Cancel);
    assert_eq!(editor_key_decision(&Key::Enter, false), EditorKeyDecision::Commit);
    assert_eq!(editor_key_decision(&Key::Enter, true), EditorKeyDecision::None, "a long-text field keeps its newlines");
    assert_eq!(editor_key_decision(&Key::Tab, false), EditorKeyDecision::None);
}

// ---------------------------------------------------------------------------
// The state model (BUG-005)
// ---------------------------------------------------------------------------

#[test]
fn entering_the_editor_seeds_from_the_value_and_leaving_drops_the_local() {
    use crate::components::data_table::detail_panel::DetailPanelState;

    let mut state = DetailPanelState::new();
    state.enter_edit("note", &json!("hello"));
    let edit = state.field_editor("note").expect("the field is editing");
    assert_eq!(edit.text, "hello", "the seed is toEditString(value), never the presented form");
    state.exit_edit("note");
    assert!(state.field_editor("note").is_none(), "the display state is the default");
}

#[test]
fn a_null_value_seeds_the_empty_string() {
    use crate::components::data_table::detail_panel::DetailPanelState;

    let mut state = DetailPanelState::new();
    state.enter_edit("note", &Value::Null);
    assert_eq!(state.field_editor("note").unwrap().text, "");
}

// ---------------------------------------------------------------------------
// Runtime probes of the mounted drawer
// ---------------------------------------------------------------------------

/// The probe host: the drawer's own state, the per-frame data and the
/// recorded host calls.
#[derive(Clone)]
struct Probe {
    panel: crate::components::data_table::detail_panel::DetailPanelState,
    dark: bool,
    columns: Vec<Column>,
    row: Option<Value>,
    row_index: Option<usize>,
    selected: usize,
    editable: bool,
    edits: Vec<(usize, String, Value)>,
    presentations: Vec<String>,
}

fn probe(columns: Vec<Column>, row: Option<Value>) -> Probe {
    let row_index = row.as_ref().map(|_| 4);
    Probe {
        panel: crate::components::data_table::detail_panel::DetailPanelState::new(),
        dark: false,
        columns,
        row,
        row_index,
        selected: 1,
        editable: false,
        edits: Vec::new(),
        presentations: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    use crate::components::data_table::detail_panel::DetailPanel;

    let mut layer = Vec::new();
    let drawer = DetailPanel::<Probe>::new(&s.panel)
        .open(true)
        .columns(&s.columns)
        .row(s.row.as_ref())
        .row_index(s.row_index)
        .editable(s.editable)
        .selected_rows(s.selected)
        .on_field_edit(|s, row, column, value| {
            s.edits.push((row, column.to_owned(), value));
        })
        .on_open_column_presentation(|s, column| {
            s.presentations.push(column.to_owned());
        })
        .host(|s| &mut s.panel);
    // The real host mounts the drawer beside a grid (a row child), not as
    // the window root — the probe composes the same shape, so the aside's
    // fixed 288pt width is exercised the way production lays it out.
    let aside = drawer
        .build(s.dark, &mut layer)
        .expect("the probe always opens the drawer");
    row().w_full().h_full().key("probe-root").child(aside)
}

fn runtime(p: Probe) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, (1440.0, 816.0), Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text.clone())
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}
#[test]
fn a_closed_drawer_mounts_nothing() {
    use crate::components::data_table::detail_panel::DetailPanel;

    let state = crate::components::data_table::detail_panel::DetailPanelState::new();
    let mut layer = Vec::new();
    let built = DetailPanel::<Probe>::new(&state).open(false).build(false, &mut layer);
    assert!(built.is_none(), "React's `!open` early return mounts nothing");
}

#[test]
fn a_row_mounts_the_fields_and_the_search_box() {
    let columns = columns(&["id", "name"]);
    let row = json!({"id": 7, "name": "acme"});
    let rt = runtime(probe(columns, Some(row)));

    for key in [
        "detail-panel",
        "detail-panel-header",
        "detail-panel-title",
        "detail-panel-search",
        "detail-panel-search-input",
        "detail-panel-fields",
        "detail-field-0",
        "detail-field-1",
    ] {
        assert!(has(&rt, key), "{key} is laid out");
    }
    // The single-row selection keeps the badge off.
    assert!(!has(&rt, "detail-panel-editing-rows"));
    // The values come from the row.
    assert_eq!(text_of(&rt, "detail-field-id-value"), "7");
    assert_eq!(text_of(&rt, "detail-field-name-value"), "acme");
}

#[test]
fn a_null_row_shows_the_no_selection_line_and_no_search() {
    let rt = runtime(probe(columns(&["id"]), None));
    assert!(has(&rt, "detail-panel-no-selection"));
    assert!(!has(&rt, "detail-panel-search"), "the search lives in the non-null arm");
}

#[test]
fn the_bulk_edit_badge_mounts_above_one_selected_row() {
    let mut p = probe(columns(&["id"]), Some(json!({"id": 7})));
    p.selected = 3;
    let rt = runtime(p);
    let badge = text_of(&rt, "detail-panel-editing-rows");
    assert!(badge.contains('3'), "{badge} carries the count");
}

#[test]
fn a_json_field_offers_quick_look_and_the_blob_line_renders() {
    let columns = vec![
        Column::new("m-id", "meta").ty("json"),
        Column::new("b-id", "blob").ty("bytea"),
        Column::new("i-id", "id").ty("integer"),
    ];
    let row = json!({"meta": {"ok": true}, "blob": "abcde", "id": 1});
    let rt = runtime(probe(columns, Some(row)));

    assert!(has(&rt, "detail-field-0-quick-look"), "the JSON field quick-looks");
    assert!(has(&rt, "detail-field-1-quick-look"), "the blob field quick-looks");
    assert!(!has(&rt, "detail-field-2-quick-look"), "a short integer does not");
    // The blob line carries the packed text.
    let blob = text_of(&rt, "detail-field-blob-blob");
    assert!(blob.contains("5"), "{blob} carries the byte count");
    // The JSON value pretty-prints.
    let meta = text_of(&rt, "detail-field-meta-value");
    assert!(meta.contains("\"ok\""), "{meta} is the pretty form");
}

#[test]
fn a_null_field_displays_null() {
    let columns = vec![Column::new("n-id", "note").ty("text")];
    let row = json!({"note": null});
    let rt = runtime(probe(columns, Some(row)));
    assert_eq!(text_of(&rt, "detail-field-note-value"), "NULL");
}

#[test]
fn the_presentation_entry_calls_the_host_with_the_column() {
    let columns = vec![Column::new("n-id", "note").ty("text")];
    let mut rt = runtime(probe(columns, Some(json!({"note": "x"}))));
    assert!(has(&rt, "detail-field-presentation-note"), "React's data-testid shape is the key");
    click(&mut rt, "detail-field-presentation-note");
    assert_eq!(rt.state.presentations, vec!["note".to_owned()]);
}

#[test]
fn quick_look_presses_record_the_column_awaiting_t378() {
    let columns = vec![Column::new("m-id", "meta").ty("json")];
    let row = json!({"meta": {"a": 1}});
    let mut rt = runtime(probe(columns, Some(row)));
    click(&mut rt, "detail-field-0-quick-look");
    assert_eq!(rt.state.panel.quick_look.as_deref(), Some("meta"));
}

#[test]
fn an_editable_field_presses_into_the_editor_and_return_commits() {
    let columns = vec![Column::new("n-id", "note").ty("varchar")];
    let row = json!({"note": "old"});
    let mut p = probe(columns, Some(row));
    p.editable = true;
    let mut rt = runtime(p);

    // The display container is the default (BUG-005).
    assert!(has(&rt, "detail-field-0-display"));
    assert!(!has(&rt, "detail-field-note-editor"));

    // A press enters the editor, seeded from the value (BUG-005: the
    // editor takes its caret on the field's own click — no autoFocus).
    click(&mut rt, "detail-field-0-display");
    assert!(has(&rt, "detail-field-note-editor"));
    assert_eq!(rt.state.panel.field_editor("note").unwrap().text, "old");

    // Click the editor for the caret, select the seed, paste over it, and
    // Return submits the commit (the T023-S3 shape: commits land on Return).
    click(&mut rt, "detail-field-note-editor");
    rt.key(KeyEvent { key: Key::Command('a'), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    rt.paste("new");
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.panel.field_editor("note").map(|e| e.text.clone()),
        Some("new".to_owned()),
        "the paste replaced the seed"
    );
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.edits, vec![(4, "note".to_owned(), json!("new"))]);
    // The commit returned the field to the display state.
    assert!(!has(&rt, "detail-field-note-editor"));
}

#[test]
fn escape_blurs_without_committing_and_keeps_the_editor_mounted() {
    let columns = vec![Column::new("n-id", "note").ty("varchar")];
    let row = json!({"note": "old"});
    let mut p = probe(columns, Some(row));
    p.editable = true;
    let mut rt = runtime(p);

    click(&mut rt, "detail-field-0-display");
    click(&mut rt, "detail-field-note-editor");
    rt.key(KeyEvent { key: Key::Command('a'), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    rt.paste("typed");
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(1.0 / 60.0);

    // Escape commits nothing (React parity) and blurs the caret; with no
    // field-level Escape hook (the module's documented deviation), the
    // editing entry stays mounted until a submit — the pinned deviation.
    assert!(rt.state.edits.is_empty(), "Escape commits nothing");
    assert!(has(&rt, "detail-field-note-editor"), "the documented deviation: the entry stays until a submit");
    assert!(!rt.state.panel.field_editor("note").unwrap().focused, "the runtime blurred the field");

    // The next activation re-seeds the local from the value — React's
    // Escape restore.
    let mut state = crate::components::data_table::detail_panel::DetailPanelState::new();
    state.enter_edit("note", &json!("old"));
    assert_eq!(state.field_editor("note").unwrap().text, "old");
}
