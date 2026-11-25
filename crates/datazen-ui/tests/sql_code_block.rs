//! T219: the SQL code block — coder tests.
//! The dialect map (React's four keys, the sqlserver contrast), the
//! code-prop sync (the remote-guarded replace), the syntax-preset seam,
//! the editable key path (typing streams through on_change), and the
//! mounted surface (the gutter, the line rows, the read-only inertness).

use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime, Tx};

use datazen_ui::components::sql_code_block::{resolve_dialect, SqlCodeBlock, SqlCodeBlockState};
use datazen_ui::components::sql_editor::contracts::CmSqlDialect;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

const DOC: &str = "SELECT 'hello';\n-- a comment\nFROM users";

struct Scene {
    block: SqlCodeBlockState,
    changes: Vec<String>,
    editable: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Self { block: SqlCodeBlockState::new(DOC, "postgresql"), changes: Vec::new(), editable: false }
    }
}

fn view(scene: &Scene, cx: &mut Cx<Scene>) -> El {
    let dark = cx.system_dark();
    let mut block = SqlCodeBlock::<Scene>::new(&scene.block);
    if scene.editable {
        block = block
            .state_access(|s: &mut Scene| &mut s.block)
            .on_change(|s: &mut Scene, doc| s.changes.push(doc));
    }
    let built = block.build(dark);
    col().key("page").w_full().h_full().padding(24.0).child(built)
}

fn runtime(scene: Scene, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S: 'static>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn text_key(value: &str) -> KeyEvent {
    KeyEvent { key: Key::Text(value.to_owned()), mods: Mods::default() }
}

fn plain_key(key: Key) -> KeyEvent {
    KeyEvent { key, mods: Mods::default() }
}

// ---- the dialect map --------------------------------------------------------

#[test]
fn resolve_dialect_maps_react_exactly_and_sqlserver_is_not_a_key() {
    assert_eq!(resolve_dialect("postgresql"), CmSqlDialect::PostgreSql);
    assert_eq!(resolve_dialect("mysql"), CmSqlDialect::MySql);
    assert_eq!(resolve_dialect("mariadb"), CmSqlDialect::MariaSql);
    assert_eq!(resolve_dialect("sqlite"), CmSqlDialect::Sqlite);
    // React's own map lacks sqlserver (contrast the editor contract's
    // resolve_cm_dialect): the standard grammar renders it too.
    assert_eq!(resolve_dialect("sqlserver"), CmSqlDialect::StandardSql);
    assert_eq!(resolve_dialect("kiwi"), CmSqlDialect::StandardSql);
    assert_eq!(resolve_dialect(""), CmSqlDialect::StandardSql);
    // The react_name arms the contracts test already pins; the default
    // dialect prop is postgresql.
    assert_eq!(SqlCodeBlockState::new("", "postgresql").dialect().react_name(), "PostgreSQL");
}

// ---- the code-prop sync -----------------------------------------------------

#[test]
fn set_code_replaces_only_when_the_doc_differs() {
    let mut block = SqlCodeBlockState::new("SELECT 1;", "postgresql");
    assert_eq!(block.doc(), "SELECT 1;");

    // The same code is a no-op (the selection survives).
    block.set_code("SELECT 1;");
    assert_eq!(block.doc(), "SELECT 1;");

    // A different code replaces the whole document (React's full-range
    // remote dispatch).
    block.set_code("SELECT 2;");
    assert_eq!(block.doc(), "SELECT 2;");

    // An empty replacement is still a replacement.
    block.set_code("");
    assert_eq!(block.doc(), "");
    block.set_code("SELECT 3;");
    assert_eq!(block.doc(), "SELECT 3;");
}

#[test]
fn the_syntax_preset_seam_records_the_setting() {
    let mut block = SqlCodeBlockState::new("SELECT 1;", "mysql");
    assert_eq!(block.syntax_preset(), None);

    block.set_syntax_preset(Some("monokai"));
    assert_eq!(block.syntax_preset(), Some("monokai"));

    // The setting change is the reconfigure seam React's second effect
    // owns; a later change replaces, a None clears.
    block.set_syntax_preset(Some("default"));
    assert_eq!(block.syntax_preset(), Some("default"));
    block.set_syntax_preset(None);
    assert_eq!(block.syntax_preset(), None);
}

// ---- the editable key path --------------------------------------------------

#[test]
fn typing_through_the_editable_block_streams_the_document() {
    let mut block = SqlCodeBlockState::new("", "postgresql");
    block.focus();

    // An unfocused engine drops keys: the focus press is the editable
    // surface's entry.
    let mut unfocused = SqlCodeBlockState::new("", "postgresql");
    let mut tx = Tx::default();
    let (consumed, changed) = unfocused.handle_key(&text_key("S"), &mut tx);
    assert!(!consumed);
    assert_eq!(changed, None);

    let (consumed, changed) = block.handle_key(&text_key("S"), &mut tx);
    assert!(consumed);
    assert_eq!(changed.as_deref(), Some("S"));

    let (consumed, changed) = block.handle_key(&text_key("Q"), &mut tx);
    assert!(consumed);
    assert_eq!(changed.as_deref(), Some("SQ"));

    // Enter inserts a newline; Backspace consumes it back out.
    let (consumed, changed) = block.handle_key(&plain_key(Key::Enter), &mut tx);
    assert!(consumed);
    assert_eq!(changed.as_deref(), Some("SQ\n"));
    let (consumed, changed) = block.handle_key(&plain_key(Key::Backspace), &mut tx);
    assert!(consumed);
    assert_eq!(changed.as_deref(), Some("SQ"));

    // A remote set_code never streams: the guard is the write path
    // itself (the host calls set_code, not the key router).
    block.set_code("SELECT 1;");
    assert_eq!(block.doc(), "SELECT 1;");
}

// ---- the mounted surface ----------------------------------------------------

#[test]
fn the_block_mounts_the_gutter_and_the_line_rows() {
    let scene = Scene::default();
    let rt = runtime(scene, DESIGN, true);
    assert!(has(&rt, "sql-code-block"));
    assert!(has(&rt, "sql-code-block-gutters"));
    // One gutter number per line (three lines in DOC).
    assert!(has(&rt, "sql-code-block-gutter-0"));
    assert!(has(&rt, "sql-code-block-gutter-1"));
    assert!(has(&rt, "sql-code-block-gutter-2"));
    assert!(!has(&rt, "sql-code-block-gutter-3"));
    // The engine's line rows carry the code.
    assert!(has(&rt, "sql-code-block-content"));
    assert!(has(&rt, "sql-line-0"));
    assert!(has(&rt, "sql-line-2"));
    // The gutter numbers count from one.
    let numbers: Vec<String> = ["sql-code-block-gutter-0", "sql-code-block-gutter-1", "sql-code-block-gutter-2"]
        .iter()
        .map(|key| element(&rt, key).text.unwrap_or_default())
        .collect();
    assert_eq!(numbers, vec!["1".to_owned(), "2".to_owned(), "3".to_owned()]);
}

#[test]
fn a_read_only_block_is_inert() {
    let scene = Scene::default();
    let rt = runtime(scene, DESIGN, true);
    // React's readOnly view: not editable, no focus surface.
    let root = element(&rt, "sql-code-block");
    assert!(!root.focusable);
    // The code still renders (the text is selectable by default).
    assert!(has(&rt, "sql-line-0"));
}

#[test]
fn an_editable_block_takes_focus_and_streams_typing() {
    let mut scene = Scene::default();
    scene.editable = true;
    scene.block.set_code("");
    let mut rt = runtime(scene, DESIGN, true);

    // The surface is focusable; the press focuses the engine.
    let root = element(&rt, "sql-code-block");
    assert!(root.focusable);
    let (x, y) = rt.find("sql-code-block").expect("the block is pressable");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.block.focused());

    // Typing streams through on_change.
    rt.key(text_key("S"));
    rt.frame(1.0 / 60.0);
    rt.key(text_key("Q"));
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.changes, vec!["S", "SQ"]);
    assert_eq!(rt.state.block.doc(), "SQ");
}

#[test]
fn the_block_fits_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let mut scene = Scene::default();
            scene.block.set_code(
                "SELECT a_long_column_name, another_long_column FROM a_table WHERE x = 'value';\n-- comment\n",
            );
            let rt = runtime(scene, (w, h), dark);
            let outside: Vec<String> = rt
                .elements()
                .iter()
                .filter(|e| {
                    if e.virtual_extent {
                        return false;
                    }
                    let v = e.visible;
                    if v[2] <= 0.0 || v[3] <= 0.0 {
                        return false;
                    }
                    v[0] < -0.5 || v[1] < -0.5 || v[0] + v[2] > w + 0.5 || v[1] + v[3] > h + 0.5
                })
                .map(|e| format!("{:?}", e.key.clone().unwrap_or(e.path.clone())))
                .collect();
            assert!(
                outside.is_empty(),
                "dark={dark} {w}x{h}: out of window: {outside:?}"
            );
        }
    }
}
