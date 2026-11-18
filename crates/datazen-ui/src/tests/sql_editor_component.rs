//! T011: the SqlEditor assembly component port
//! (`components/sql-editor/SqlEditor.tsx` → `components/sql_editor/sql_editor.rs`).
//!
//! Three layers, mirroring the React file:
//! 1. view/geometry (keys, placeholder, executing badge, two sizes × dark/light);
//! 2. the `SqlEditorHandle` surface (`insertAt` padding, `toggleLineComment`,
//!    `rawInsert` clamping, `formatDocument` seam, `insertSnippet`);
//! 3. the update-listener and base keymap events via `on_key`.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

use crate::components::sql_editor::{
    on_key, EditorEvent, ExecutionStatus, SqlEditorComponent, SqlEditorProps, SqlEditorState,
};
use crate::sql_editor::SqlEditor;

fn env(dark: bool) -> Env {
    Env { system_dark: dark, scale: 1.0 }
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn rect_of<S>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn text_key(s: &str) -> KeyEvent {
    KeyEvent { key: Key::Text(s.to_string()), mods: Mods::default() }
}

fn cmd_enter() -> KeyEvent {
    KeyEvent { key: Key::Enter, mods: Mods { cmd: true, ..Mods::default() } }
}

fn cmd_shift_enter() -> KeyEvent {
    KeyEvent {
        key: Key::Enter,
        mods: Mods { cmd: true, shift: true, ..Mods::default() },
    }
}

fn cmd(ch: char) -> KeyEvent {
    KeyEvent { key: Key::Command(ch), mods: Mods::default() }
}

// ---- Harness ---------------------------------------------------------------

#[derive(Default)]
struct Harness {
    state: SqlEditorState,
    events: Vec<EditorEvent>,
    focus_presses: usize,
    accepted: Vec<usize>,
    executing: bool,
    dark: bool,
}

fn plain_harness_view(h: &Harness, _cx: &mut Cx<Harness>) -> El {
    let props = SqlEditorProps::default();
    col().w_full().h_full().padding(16.0).child(
        SqlEditorComponent::<Harness>::new(&h.state, &props).build(h.dark),
    )
}

fn harness_view(h: &Harness, _cx: &mut Cx<Harness>) -> El {
    let props = SqlEditorProps {
        placeholder: Some("Type SQL here".to_string()),
        execution_status: if h.executing { ExecutionStatus::Running } else { ExecutionStatus::Idle },
        ..SqlEditorProps::default()
    };
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .child(
            SqlEditorComponent::<Harness>::new(&h.state, &props)
                .on_focus(|s: &mut Harness| {
                    s.focus_presses += 1;
                    s.state.focus();
                })
                .on_completion_accept(|s: &mut Harness, index| {
                    s.accepted.push(index);
                    s.state.editor.completion_accept_index(index);
                })
                .build(true),
        )
}

fn record(h: &mut Harness, ev: KeyEvent) -> (bool, Vec<EditorEvent>) {
    let (consumed, events) = on_key(&mut h.state, &ev, &mut Tx::default());
    h.events.extend(events.clone());
    (consumed, events)
}

// ---- 1. View and geometry --------------------------------------------------

#[test]
fn renders_expected_keys_with_content() {
    let mut h = Harness::default();
    h.state.set_value("SELECT 1;\nFROM x");
    h.state.focus();
    let mut rt = Runtime::new(h, harness_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    for key in ["sql-editor", "sql-editor-content", "sql-editor-doc", "sql-editor-line-0", "sql-editor-line-1"] {
        assert!(has_key(&rt, key), "{key} is laid out");
    }
    assert!(has_key(&rt, "sql-editor-caret"), "a focused editor draws its caret");
    assert!(!has_key(&rt, "sql-editor-placeholder"), "no placeholder over a non-empty doc");
    assert!(!has_key(&rt, "sql-editor-executing"), "no executing badge while idle");
}

#[test]
fn placeholder_only_on_empty_document() {
    let mut h = Harness::default();
    h.state.focus();
    let mut rt = Runtime::new(h, harness_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(has_key(&rt, "sql-editor-placeholder"), "the empty doc shows the placeholder");
    // React's cmPlaceholder disappears as soon as the doc has any content.
    rt.update(|s, _| s.state.set_value("S"));
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, "sql-editor-placeholder"), "one character clears the placeholder");
}

#[test]
fn executing_badge_appears_only_while_running() {
    let mut h = Harness::default();
    h.state.set_value("SELECT 1;");
    h.executing = true;
    h.state.set_execution(ExecutionStatus::Running, Some((0, 9)));
    let mut rt = Runtime::new(h, harness_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(has_key(&rt, "sql-editor-executing"));
    assert!(has_key(&rt, "sql-editor-executing-spinner"));
    rt.update(|s, _| {
        s.executing = false;
        s.state.set_execution(ExecutionStatus::Idle, None);
    });
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, "sql-editor-executing"), "the badge leaves with the status");
}

#[test]
fn geometry_two_sizes_two_appearances() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut h = Harness::default();
            h.dark = dark;
            h.state.set_value("SELECT 1;\n-- two short lines");
            let mut rt = Runtime::new(h, plain_harness_view, (width, height), env(dark));
            rt.frame(0.0);
            assert_in_window(&rt, width, height, &format!("{width}x{height} dark={dark}"));
        }
    }
}

#[test]
fn long_line_keeps_code_mirror_no_wrap_geometry() {
    let mut h = Harness::default();
    h.state.set_value(&format!("SELECT {} FROM t;", "very_long_column_".repeat(40)));
    let mut rt = Runtime::new(h, harness_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    // CodeMirror with lineWrapping off: the line is as wide as the unwrapped
    // surface and the viewport scrolls, not the text.
    let doc = rect_of(&rt, "sql-editor-doc");
    assert!(
        (doc[2] - crate::sql_editor::EDITOR_SURFACE_WIDTH).abs() < 0.5,
        "doc column is the unwrapped surface, got {doc:?}"
    );
    let content = rect_of(&rt, "sql-editor-content");
    assert!(content[2] <= 960.0, "the viewport stays in the window, got {content:?}");
}

// ---- 2. Interaction --------------------------------------------------------

#[test]
fn pressing_the_surface_focuses_the_editor() {
    let mut h = Harness::default();
    h.state.set_value("SELECT 1;");
    let mut rt = Runtime::new(h, harness_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(!rt.state.state.editor.focused());
    press(&mut rt, "sql-editor-content");
    assert_eq!(rt.state.focus_presses, 1, "the focus callback fired once");
    assert!(rt.state.state.editor.focused());
    rt.frame(1.0 / 60.0);
    assert!(has_key(&rt, "sql-editor-caret"), "the focused editor draws its caret");
}

#[test]
fn completion_list_click_accepts_the_item() {
    let mut h = Harness::default();
    h.state.focus();
    // Type an uppercase SQL keyword prefix through the component's own key
    // routing so the engine opens its keyword completion.
    let (consumed, _) = record(&mut h, text_key("S"));
    assert!(consumed);
    assert!(!h.state.editor.completion_items().is_empty(), "the engine opened completions");

    let mut rt = Runtime::new(h, harness_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    assert!(has_key(&rt, "sql-completion"), "the completion list is laid out");
    assert!(has_key(&rt, "sql-completion-item-0"));
    click(&mut rt, "sql-completion-item-0");
    let s = &rt.state;
    assert_eq!(s.accepted, vec![0], "the accept callback ran with the item index");
    assert!(s.state.editor.completion_items().is_empty(), "accepting closed the list");
    assert!(s.state.get_document().starts_with("S"), "the item replaced the prefix");
}

// ---- 3. Handle surface (contracts.ts SqlEditorHandle) ---------------------

#[test]
fn insert_at_replaces_an_empty_document() {
    let mut s = SqlEditorState::default();
    s.insert_at("SELECT 1;", None);
    assert_eq!(s.get_document(), "SELECT 1;");
    assert_eq!(s.get_cursor_offset(), 9, "the caret lands after the text");
}

#[test]
fn insert_at_pads_with_blank_lines_between_statements() {
    let mut s = SqlEditorState::default();
    s.set_value("SELECT 1;");
    s.insert_at("SELECT 2;", None);
    assert_eq!(s.get_document(), "SELECT 1;\n\nSELECT 2;");

    let mut s = SqlEditorState::default();
    s.set_value("SELECT 1;\n");
    s.insert_at("SELECT 2;", None);
    assert_eq!(
        s.get_document(),
        "SELECT 1;\n\nSELECT 2;",
        "a single trailing newline gets one more, so statements stay blank-line separated"
    );

    let mut s = SqlEditorState::default();
    s.set_value("SELECT 1;\n\n");
    s.insert_at("SELECT 2;", None);
    assert_eq!(
        s.get_document(),
        "SELECT 1;\n\nSELECT 2;",
        "two trailing newlines need no padding"
    );
}

#[test]
fn insert_at_position_inserts_between_with_padding() {
    let mut s = SqlEditorState::default();
    s.set_value("AA\nBB");
    s.insert_at("X", Some(3));
    // React pads both sides: before ends with one \n (prefix \n), after does
    // not start with \n (suffix \n\n) — exactly the dispatch in the handle.
    assert_eq!(s.get_document(), "AA\n\nX\n\nBB");
}

#[test]
fn raw_insert_clamps_the_position() {
    let mut s = SqlEditorState::default();
    s.set_value("ab");
    s.raw_insert("X", 99);
    assert_eq!(s.get_document(), "abX");
    let mut s = SqlEditorState::default();
    s.set_value("ab");
    s.raw_insert("X", 1);
    assert_eq!(s.get_document(), "aXb");
    assert_eq!(s.get_cursor_offset(), 2);
}

#[test]
fn toggle_line_comment_adds_then_strips() {
    let mut s = SqlEditorState::default();
    s.set_value("SELECT 1;\nSELECT 2;");
    s.editor.set_main_selection(0, 20);
    s.toggle_line_comment();
    assert_eq!(s.get_document(), "-- SELECT 1;\n-- SELECT 2;");
    // The replaced region ends up selected, as in React.
    assert_eq!(s.get_selection(), "-- SELECT 1;\n-- SELECT 2;");

    s.toggle_line_comment();
    assert_eq!(s.get_document(), "SELECT 1;\nSELECT 2;", "all-commented strips the prefix");
}

#[test]
fn toggle_line_comment_targets_the_caret_line_only() {
    let mut s = SqlEditorState::default();
    s.set_value("SELECT 1;\nSELECT 2;\nSELECT 3;");
    s.editor.set_main_caret(11); // start of line 2
    s.toggle_line_comment();
    assert_eq!(s.get_document(), "SELECT 1;\n-- SELECT 2;\nSELECT 3;");
}

#[test]
fn toggle_line_comment_prefixes_the_line_start_like_react() {
    // React's add branch is `-- ${l}` on the whole line: the indent is not
    // preserved on ADD (only the strip regex `^(\s*)-- ?` keeps it).
    let mut s = SqlEditorState::default();
    s.set_value("  SELECT 1;\n\n  SELECT 2;");
    s.editor.set_main_selection(0, 24);
    s.toggle_line_comment();
    assert_eq!(s.get_document(), "--   SELECT 1;\n\n--   SELECT 2;");
    // Stripping DOES keep the indent (`^(\s*)-- ?`).
    s.editor.set_main_selection(0, 28);
    s.toggle_line_comment();
    assert_eq!(s.get_document(), "  SELECT 1;\n\n  SELECT 2;");
}

#[test]
fn format_document_formats_selection_or_whole_doc() {
    let mut s = SqlEditorState::default();
    s.set_value("select one");
    let formatted = s.format_document(&|text| format!("FORMATTED[{text}]"));
    assert!(formatted, "the injected formatter ran");
    assert_eq!(s.get_document(), "FORMATTED[select one]");

    // A selection formats only the selection.
    let mut s = SqlEditorState::default();
    s.set_value("aaaa bbbb");
    s.editor.set_main_selection(5, 9);
    assert!(s.format_document(&|text| format!("<{text}>")));
    assert_eq!(s.get_document(), "aaaa <bbbb>");
    assert_eq!(s.get_selection(), "<bbbb>");

    // A formatter that returns the input unchanged is a no-op.
    let mut s = SqlEditorState::default();
    s.set_value("same");
    assert!(!s.format_document(&|text| text.to_string()));
    assert_eq!(s.get_document(), "same");
}

#[test]
fn insert_snippet_replaces_the_selection() {
    let mut s = SqlEditorState::default();
    s.set_value("SELECT 1;");
    s.editor.set_main_selection(0, 8);
    s.insert_snippet("SELECT * FROM t");
    assert_eq!(s.get_document(), "SELECT * FROM t;");
    assert_eq!(s.get_cursor_offset(), 15, "caret after the template");
}

#[test]
fn set_value_bumps_the_document_version_only_on_change() {
    let mut s = SqlEditorState::default();
    assert_eq!(s.get_document_version(), 0);
    s.set_value("A");
    assert_eq!(s.get_document_version(), 1, "external replacement bumps the version");
    s.set_value("A");
    assert_eq!(s.get_document_version(), 1, "an unchanged value is not dispatched");
    // User typing never bumps the version (React only bumps in the [value] effect).
    s.focus();
    let _ = on_key(&mut s, &text_key("B"), &mut Tx::default());
    assert_eq!(s.get_document(), "AB");
    assert_eq!(s.get_document_version(), 1);
}

#[test]
fn execution_mark_pins_the_document_version() {
    let mut s = SqlEditorState::default();
    s.set_value("SELECT pg_sleep(1);");
    s.set_execution(ExecutionStatus::Running, Some((0, 18)));
    let mark = s.execution_mark().unwrap();
    assert_eq!(mark.target_range, Some((0, 18)));
    assert_eq!(mark.document_version, 1, "the mark pins the version at start");

    s.set_execution(ExecutionStatus::Cancelling, None);
    assert!(s.execution_mark().is_none(), "cancelling clears like idle");

    s.set_execution(ExecutionStatus::Running, None);
    assert!(s.execution_mark().is_some());
    s.set_execution(ExecutionStatus::Idle, None);
    assert!(s.execution_mark().is_none());
}

// ---- 4. Update listener + base keymap (on_key) -----------------------------

#[test]
fn base_keymap_shortcuts_fire_events() {
    let mut s = SqlEditorState::default();
    s.focus();
    let (consumed, events) = on_key(&mut s, &cmd_enter(), &mut Tx::default());
    assert!(consumed);
    assert_eq!(events, vec![EditorEvent::Execute]);

    let (consumed, events) = on_key(&mut s, &cmd_shift_enter(), &mut Tx::default());
    assert!(consumed);
    assert_eq!(events, vec![EditorEvent::ExecuteAll]);

    let (consumed, events) = on_key(&mut s, &cmd('s'), &mut Tx::default());
    assert!(consumed);
    assert_eq!(events, vec![EditorEvent::SaveQuery]);
}

#[test]
fn shortcuts_need_focus() {
    let mut s = SqlEditorState::default();
    let (consumed, events) = on_key(&mut s, &cmd_enter(), &mut Tx::default());
    assert!(!consumed, "an unfocused editor lets the host see the key");
    assert!(events.is_empty());
}

#[test]
fn typing_reports_change_and_selection_events() {
    let mut s = SqlEditorState::default();
    s.focus();
    let (_, events) = on_key(&mut s, &text_key("a"), &mut Tx::default());
    assert_eq!(events, vec![EditorEvent::Changed("a".to_string())]);

    // Selecting text reports the selection (Shift+arrows are engine moves;
    // set a selection directly and edit to observe the selection event).
    let mut s = SqlEditorState::default();
    s.set_value("hello");
    s.focus();
    s.editor.set_main_selection(0, 5);
    let (_, events) = on_key(&mut s, &text_key("x"), &mut Tx::default());
    assert_eq!(
        events,
        vec![
            EditorEvent::Changed("x".to_string()),
            EditorEvent::SelectionChanged(String::new()),
        ],
        "replacing a selection reports the new empty selection"
    );
    assert_eq!(s.get_document(), "x");
}

#[test]
fn qualified_path_events_follow_the_caret() {
    let mut h = Harness::default();
    h.state.focus();
    // "db." — a dangling qualified prefix reports [db].
    for ch in ['d', 'b', '.'] {
        record(&mut h, text_key(&ch.to_string()));
    }
    assert!(h
        .events
        .iter()
        .any(|e| matches!(e, EditorEvent::QualifiedPath(p) if p == &vec!["db".to_string()])));

    // "db.users" — the parents are still [db] (users is the leaf), so the
    // update listener stays quiet: no new QualifiedPath event.
    h.events.clear();
    for ch in ['u', 's', 'e', 'r', 's'] {
        record(&mut h, text_key(&ch.to_string()));
    }
    assert!(
        !h.events
            .iter()
            .any(|e| matches!(e, EditorEvent::QualifiedPath(_))),
        "completing the leaf segment does not change the parents"
    );

    // "db.users." — a second dangling dot deepens the path to [db, users].
    h.events.clear();
    record(&mut h, text_key("."));
    assert!(h
        .events
        .iter()
        .any(|e| matches!(e, EditorEvent::QualifiedPath(p)
            if p == &vec!["db".to_string(), "users".to_string()])));
    assert_eq!(h.state.last_parents(), vec!["db".to_string(), "users".to_string()]);
}

#[test]
fn mount_parents_seed_the_last_parents_ref() {
    let editor = SqlEditor::from_doc("SELECT * FROM analytics.public.users");
    let mut s = SqlEditorState::new(editor);
    s.editor.set_main_caret(31); // inside "public"
    let parents = crate::components::sql_editor::parse_qualified_path_parents(
        s.get_document(),
        s.get_cursor_offset(),
    );
    assert_eq!(
        parents,
        vec!["analytics".to_string(), "public".to_string()],
        "the caret inside `public` makes users the leaf, analytics+public the parents"
    );
    // The mount seeding used the initial caret (offset 0): no parents there.
    assert_eq!(s.mount_parents(), Vec::<String>::new());
}

// ---- 5. Pure helpers (lib ports) -------------------------------------------

#[test]
fn parse_qualified_path_parents_cases() {
    let p = |text: &str, pos: usize| {
        crate::components::sql_editor::parse_qualified_path_parents(text, pos)
    };
    // db.schema.table with the caret at the end: the table is dropped.
    assert_eq!(p("db.public.users", 15), vec!["db", "public"]);
    // Dangling dot: everything before it.
    assert_eq!(p("db.public.", 10), vec!["db", "public"]);
    // Single segment: no parents.
    assert_eq!(p("users", 5), Vec::<String>::new());
    // Inside quotes: no parents.
    assert_eq!(p("'db.users'", 4), Vec::<String>::new());
    // Empty doc.
    assert_eq!(p("", 0), Vec::<String>::new());
}

#[test]
fn toggle_sql_line_comment_rules() {
    // Adds to non-empty lines only.
    assert_eq!(
        crate::components::sql_editor::toggle_sql_line_comments("a\n\nb"),
        "-- a\n\n-- b"
    );
    // Strips when every non-empty line is commented.
    assert_eq!(
        crate::components::sql_editor::toggle_sql_line_comments("-- a\n\n  -- b"),
        "a\n\n  b"
    );
    // Mixed: still adds (React only strips when ALL non-empty lines match).
    assert_eq!(
        crate::components::sql_editor::toggle_sql_line_comments("-- a\nb"),
        "-- -- a\n-- b"
    );
}
