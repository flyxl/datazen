//! T236 independent audit — fresh vectors against the drawer port, none
//! shared with the coder tests: the search journey through the runtime
//! (type, filter, no-match, clear), the aside's exact geometry in both
//! palettes, the twelve-column list, the missing-field NULL arm, the
//! presentation entry's conditional mount, the dialog mount gate, the
//! zh-CN pack's exact badge and blob lines, and the commit chain's wire
//! shapes (exponents, the space-padded refusal, fractions, object
//! round-trips).

use rune::{col, row, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use serde_json::{json, Value};

use crate::components::data_table::column_presentation_dialog::ValuePresentationSnapshot;
use crate::components::data_table::column_presentation_dialog_state::{
    ColumnPresentationDialogConfig, ColumnPresentationDialogState,
};
use crate::components::data_table::column_presentation_plan::ColumnSource;
use crate::components::data_table::column_presentation_rules::ValuePresentationRegistry;
use crate::components::data_table::detail_panel::{
    blob_line, commit_field_edit, editing_rows_text, filtered_columns, FieldCommit,
};
use crate::components::data_table::table_header::Column;

fn column(id: &str, name: &str, ty: &str) -> Column {
    Column::new(id, name).ty(ty)
}

/// The audit's host: like the coder probe, plus the presentation dialog's
/// own state (for the mount gate) and a flag for the entry's existence.
#[derive(Clone)]
struct Scene {
    panel: crate::components::data_table::detail_panel::DetailPanelState,
    dark: bool,
    columns: Vec<Column>,
    row: Option<Value>,
    editable: bool,
    entry: bool,
    edits: Vec<(usize, String, Value)>,
    presentations: Vec<String>,
    dialog: ColumnPresentationDialogState,
    snapshot: ValuePresentationSnapshot,
    registry: ValuePresentationRegistry,
}

fn scene(columns: Vec<Column>, row: Option<Value>) -> Scene {
    Scene {
        panel: crate::components::data_table::detail_panel::DetailPanelState::new(),
        dark: false,
        columns,
        row,
        editable: false,
        entry: true,
        edits: Vec::new(),
        presentations: Vec::new(),
        dialog: ColumnPresentationDialogState::new(ColumnPresentationDialogConfig {
            column_name: "note".to_owned(),
            column_type: Some("text".to_owned()),
            cell_type_hints: None,
            source: None,
            sample_value: None,
            current_override: None,
            has_on_save: false,
            has_on_enable_time_display: false,
        }),
        snapshot: ValuePresentationSnapshot::default(),
        registry: ValuePresentationRegistry::new(),
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    use crate::components::data_table::detail_panel::DetailPanel;

    let mut layer = Vec::new();
    let mut drawer = DetailPanel::<Scene>::new(&s.panel)
        .open(true)
        .columns(&s.columns)
        .row(s.row.as_ref())
        .row_index(s.row.as_ref().map(|_| 9))
        .editable(s.editable)
        .selected_rows(1)
        .presentation_snapshot(&s.snapshot)
        .presentation_registry(&s.registry)
        .presentation_dialog(&s.dialog, |s: &mut Scene| &mut s.dialog)
        .on_field_edit(|s, row, column, value| {
            s.edits.push((row, column.to_owned(), value));
        })
        .host(|s| &mut s.panel);
    if s.entry {
        drawer = drawer.on_open_column_presentation(|s, column| {
            s.presentations.push(column.to_owned());
        });
    }
    let aside = drawer.build(s.dark, &mut layer).expect("the audit opens the drawer");
    // The real host composes a grid beside the drawer and owns the overlay
    // layer the dialogs render into (the `connection_page` shape: a grow
    // slot for the grid, the aside flush right, the layer mounted once).
    row().w_full().h_full().key("audit-root")
        .child(col().grow(1.0).min_h(0.0))
        .child(aside)
        .children(layer)
}

fn runtime(s: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = s.dark;
    let mut rt = Runtime::new(s, view, size, Env { system_dark: dark, scale: 1.0 });
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

fn rect_of<S>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .map(|e| e.rect)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---------------------------------------------------------------------------
// The search journey (typed, filtered, empty, cleared — all through the
// runtime, not the pure function)
// ---------------------------------------------------------------------------

#[test]
fn the_search_journey_filters_and_restores_through_the_runtime() {
    let columns = vec![
        column("a", "CustomerName", "varchar"),
        column("b", "amount", "numeric"),
        column("c", "id", "integer"),
    ];
    let mut rt = runtime(scene(columns, Some(json!({"CustomerName": "acme", "amount": 1, "id": 7}))), (1440.0, 816.0));

    // Type "am" into the search box: two of the three columns match.
    click(&mut rt, "detail-panel-search-input");
    rt.paste("am");
    rt.frame(1.0 / 60.0);
    assert!(has(&rt, "detail-field-0"), "CustomerName survives");
    assert!(has(&rt, "detail-field-1"), "amount survives");
    assert!(!has(&rt, "detail-field-2"), "id is filtered out");
    // The clear button mounts with a typed query.
    assert!(has(&rt, "detail-panel-search-clear"));

    // A query nothing matches paints the no-columns line instead (select
    // the typed query first, then type over it).
    rt.key(KeyEvent { key: Key::Command('a'), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent { key: Key::Text("zzz".to_owned()), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert!(has(&rt, "detail-panel-no-columns"));
    assert!(!has(&rt, "detail-field-0"));

    // The clear press empties the query and every column returns.
    click(&mut rt, "detail-panel-search-clear");
    assert!(!has(&rt, "detail-panel-no-columns"));
    for key in ["detail-field-0", "detail-field-1", "detail-field-2"] {
        assert!(has(&rt, key), "{key} is back");
    }
    assert!(!has(&rt, "detail-panel-search-clear"), "an empty query hides the clear button");
}

#[test]
fn a_blank_query_keeps_every_column_mounted() {
    let columns = vec![column("a", "one", "integer"), column("b", "two", "integer")];
    let rt = runtime(scene(columns, Some(json!({"one": 1, "two": 2}))), (1440.0, 816.0));
    assert!(has(&rt, "detail-field-0") && has(&rt, "detail-field-1"));
}

// ---------------------------------------------------------------------------
// Geometry (both palettes, two window sizes)
// ---------------------------------------------------------------------------

#[test]
fn the_aside_keeps_its_react_geometry_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let columns = vec![column("a", "note", "varchar")];
            let mut s = scene(columns, Some(json!({"note": "x"})));
            s.dark = dark;
            let rt = runtime(s, size);

            // w-72 → 288pt; h-full → the window height.
            let aside = rect_of(&rt, "detail-panel");
            assert_eq!(aside[2], 288.0, "w-72 at {size:?} dark={dark}");
            assert!((aside[3] - size.1).abs() < 0.5, "h-full at {size:?} dark={dark}");
            // h-10 → 40pt header; h-7 → 28pt search input.
            assert_eq!(rect_of(&rt, "detail-panel-header")[3], 40.0, "h-10 dark={dark}");
            assert_eq!(rect_of(&rt, "detail-panel-search-input")[3], 28.0, "h-7 dark={dark}");
            // The aside starts where the window ends minus its width (the
            // drawer is the row's last child).
            assert!((aside[0] + aside[2] - size.0).abs() < 0.5, "flush right at {size:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// The list, the missing field, the type label
// ---------------------------------------------------------------------------

#[test]
fn twelve_columns_mount_twelve_indexed_field_rows() {
    let columns: Vec<Column> = (0..12)
        .map(|index| column(&format!("c{index}"), &format!("col{index}"), "integer"))
        .collect();
    let row: Value = (0..12).map(|index| (format!("col{index}"), json!(index))).collect();
    let rt = runtime(scene(columns, Some(row)), (1440.0, 816.0));
    for index in 0..12 {
        assert!(has(&rt, &format!("detail-field-{index}")), "row {index} is mounted");
        assert_eq!(
            text_of(&rt, &format!("detail-field-col{index}-value")),
            index.to_string()
        );
    }
    // The pure filter agrees with the mounted list for a one-column hit.
    let all = (0..12).map(|index| column("c", &format!("col{index}"), "integer")).collect::<Vec<_>>();
    assert_eq!(filtered_columns(&all, "col7").len(), 1);
}

#[test]
fn a_missing_row_field_displays_null_like_reacts_undefined() {
    let columns = vec![column("a", "note", "varchar")];
    // The row exists but has no `note` key: React's `row[col.name]` is
    // `undefined`, which the null verdicts cover.
    let rt = runtime(scene(columns, Some(json!({"other": 1}))), (1440.0, 816.0));
    assert_eq!(text_of(&rt, "detail-field-note-value"), "NULL");
}

#[test]
fn the_type_label_renders_the_raw_type_name() {
    let columns = vec![column("a", "meta", "jsonb")];
    let rt = runtime(scene(columns, Some(json!({"meta": {"x": 1}}))), (1440.0, 816.0));
    assert_eq!(text_of(&rt, "detail-field-0-type"), "jsonb");
}

// ---------------------------------------------------------------------------
// The entry's conditional mount and the dialog gate
// ---------------------------------------------------------------------------

#[test]
fn the_presentation_entry_mounts_only_when_the_host_wired_it() {
    let columns = vec![column("a", "note", "varchar")];
    let mut s = scene(columns, Some(json!({"note": "x"})));
    s.entry = false;
    let rt = runtime(s, (1440.0, 816.0));
    assert!(
        !has(&rt, "detail-field-presentation-note"),
        "React's conditional mount: no callback, no button"
    );
}

#[test]
fn the_presentation_dialog_mounts_only_for_an_open_state_and_a_living_column() {
    let columns = vec![column("a", "note", "varchar")];
    let mut s = scene(columns.clone(), Some(json!({"note": "x"})));

    // Closed state: nothing in the layer.
    let mut rt = runtime(s.clone(), (1440.0, 816.0));
    assert!(!has(&rt, "column-presentation-dialog"));

    // Open for a column the drawer no longer shows: still nothing.
    s.dialog.open_dialog(&s.snapshot);
    s.columns = Vec::new();
    rt = runtime(s.clone(), (1440.0, 816.0));
    assert!(!has(&rt, "column-presentation-dialog"), "a dropped column unmounts the dialog");

    // Open for a column the drawer shows: the shared dialog mounts.
    s.columns = columns;
    rt = runtime(s, (1440.0, 816.0));
    assert!(has(&rt, "column-presentation-dialog"));
}

// ---------------------------------------------------------------------------
// The pack's exact strings
// ---------------------------------------------------------------------------

#[test]
fn the_badge_and_blob_lines_use_the_pack_text_verbatim() {
    assert_eq!(editing_rows_text(3), "正在编辑 3 行");
    assert_eq!(blob_line(&json!("abcde")), "BLOB 数据 · 5 字节");
}

// ---------------------------------------------------------------------------
// The commit chain's wire shapes (fresh vectors)
// ---------------------------------------------------------------------------

#[test]
fn the_commit_chain_handles_exponents_fractions_and_refusals() {
    // The exponent form f64 shares with JS Number.
    assert_eq!(
        commit_field_edit("integer", "2e3", "7", false),
        FieldCommit::Value(json!(2000))
    );
    // A fraction survives an int-typed column (JS Number keeps it).
    assert_eq!(
        commit_field_edit("int", "3.5", "7", false),
        FieldCommit::Value(json!(3.5))
    );
    // The documented deviation: JS trims whitespace before Number, f64 does
    // not — a padded number degrades to the null commit, not to 7.
    assert_eq!(
        commit_field_edit("integer", " 7", "7", false),
        FieldCommit::Null
    );
    // An equal text never commits, whatever the type.
    assert_eq!(
        commit_field_edit("float", "7.5", "7.5", false),
        FieldCommit::NoChange
    );
    // Clearing a null value stays a no-change.
    assert_eq!(commit_field_edit("varchar", "", "", true), FieldCommit::NoChange);
}

#[test]
fn a_json_commit_round_trips_the_parsed_object() {
    let commit = commit_field_edit("json", "{\"b\": 2, \"a\": [1, true]}", "7", false);
    let FieldCommit::Value(value) = commit else {
        panic!("the JSON branch commits");
    };
    // The parsed object survives to the wire with its shape (dictionary key
    // order — the documented to_edit_string deviation class).
    assert_eq!(value, json!({"a": [1, true], "b": 2}));
}

#[test]
fn bool_commits_compare_the_exact_string() {
    assert_eq!(commit_field_edit("boolean", "True", "false", false), FieldCommit::Value(json!(false)));
    assert_eq!(commit_field_edit("boolean", "true", "false", false), FieldCommit::Value(json!(true)));
}

// ---------------------------------------------------------------------------
// The search filter's unicode and case behaviour (pure, fresh vectors)
// ---------------------------------------------------------------------------

#[test]
fn the_filter_matches_unicode_names_case_insensitively() {
    let all = vec![
        column("a", "客户名", "varchar"),
        column("b", "AMOUNT", "numeric"),
    ];
    assert_eq!(filtered_columns(&all, "客户").len(), 1);
    assert_eq!(filtered_columns(&all, "amount").len(), 1);
    // The needle is trimmed before it filters.
    assert_eq!(filtered_columns(&all, "  amount  ").len(), 1);
    assert_eq!(filtered_columns(&all, "客").len(), 1, "a partial CJK name matches");
}

// ---------------------------------------------------------------------------
// The editable row's press journeys (long text and JSON — the textarea arms)
// ---------------------------------------------------------------------------

#[test]
fn a_long_text_field_keeps_its_newlines_on_return() {
    let columns = vec![column("a", "note", "text")];
    let mut s = scene(columns, Some(json!({"note": "line"})));
    s.editable = true;
    let mut rt = runtime(s, (1440.0, 816.0));

    click(&mut rt, "detail-field-0-display");
    click(&mut rt, "detail-field-note-editor");
    // Return in the long-text editor: a newline, not a submit.
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert!(rt.state.edits.is_empty(), "Return in a textarea is a newline, not a commit");
    assert_eq!(
        rt.state.panel.field_editor("note").map(|e| e.text.clone()),
        Some("line\n".to_owned())
    );
}

#[test]
fn a_json_field_acts_like_reacts_textarea_not_a_single_line_commit() {
    // A JSON column is long text (`type.includes('json')`), so React mounts
    // the textarea and Return inserts a newline — no commit path exists in
    // the drawer for long-text edits (React commits those on blur, and the
    // port has no blur hook: the pinned deviation in the module header).
    let columns = vec![column("a", "meta", "json")];
    let mut s = scene(columns, Some(json!({"meta": {"old": 1}})));
    s.editable = true;
    let mut rt = runtime(s, (1440.0, 816.0));

    click(&mut rt, "detail-field-0-display");
    click(&mut rt, "detail-field-meta-editor");
    rt.key(KeyEvent { key: Key::Command('a'), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    rt.paste("{\"new\": 2}");
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert!(rt.state.edits.is_empty(), "Return in the JSON textarea is a newline (React parity)");
    assert_eq!(
        rt.state.panel.field_editor("meta").map(|e| e.text.clone()),
        Some("{\"new\": 2}\n".to_owned())
    );
    // The entry stays mounted — the documented no-blur-hook deviation.
    assert!(has(&rt, "detail-field-meta-editor"));
}

#[test]
fn the_drawer_keeps_working_after_a_commit_returns_it_to_display() {
    let columns = vec![column("a", "note", "varchar")];
    let mut s = scene(columns, Some(json!({"note": "old"})));
    s.editable = true;
    let mut rt = runtime(s, (1440.0, 816.0));

    // One full edit cycle.
    click(&mut rt, "detail-field-0-display");
    click(&mut rt, "detail-field-note-editor");
    rt.key(KeyEvent { key: Key::Command('a'), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    rt.paste("first");
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.edits.len(), 1);

    // The second cycle still works: press, edit, commit.
    click(&mut rt, "detail-field-0-display");
    click(&mut rt, "detail-field-note-editor");
    rt.key(KeyEvent { key: Key::Command('a'), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    rt.paste("second");
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.edits,
        vec![
            (9, "note".to_owned(), json!("first")),
            (9, "note".to_owned(), json!("second")),
        ]
    );
    assert!(!has(&rt, "detail-field-note-editor"), "the drawer displays again");
}

// ---------------------------------------------------------------------------
// The no-selection arm (the drawer open with no row — the coder test probed
// the null row; this pins what the open drawer still mounts around it)
// ---------------------------------------------------------------------------

#[test]
fn an_open_drawer_without_a_row_mounts_the_header_but_no_fields() {
    let columns = vec![column("a", "note", "varchar")];
    let rt = runtime(scene(columns, None), (1440.0, 816.0));
    assert!(has(&rt, "detail-panel"));
    assert!(has(&rt, "detail-panel-title"));
    assert!(has(&rt, "detail-panel-no-selection"));
    assert!(!has(&rt, "detail-panel-search"), "the search box lives in the row arm");
    assert!(!has(&rt, "detail-field-0"));
}
