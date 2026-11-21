//! T219 independent audit: the SQL code block, fresh vectors.
//!
//! The coder tests cover one vector per rule; this audit attacks from
//! the other side: the live contrast between the component's dialect
//! map and the editor contract's (`sqlserver` resolves differently),
//! the command-gated typing arms, an IME commit through the editable
//! path, the remote sync's caret reset, the gutter's React-verbatim
//! washes in both themes, and a forty-line document's geometry.

use rune::{col, Cx, El, Env, Ime, KeyEvent, Key, Mods, Runtime};

use crate::components::sql_code_block::{resolve_dialect, SqlCodeBlock, SqlCodeBlockState};
use crate::components::sql_editor::contracts::{resolve_cm_dialect, CmSqlDialect};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

const AUDIT_DOC: &str = "-- the DDL\nCREATE TABLE t (id INT);\nDROP TABLE t;";

struct Scene {
    block: SqlCodeBlockState,
    changes: Vec<String>,
    editable: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            block: SqlCodeBlockState::new(AUDIT_DOC, "mysql"),
            changes: Vec::new(),
            editable: false,
        }
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
    col().key("audit-page").w_full().h_full().padding(24.0).child(built)
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

fn text_key(value: &str, mods: Mods) -> KeyEvent {
    KeyEvent { key: Key::Text(value.to_owned()), mods }
}

// ---- the fresh dialect vectors ----------------------------------------------

#[test]
fn the_component_map_contrasts_with_the_editor_contract_on_sqlserver() {
    // The editor contract's map carries sqlserver; the component's own
    // React map does not, and that difference is asserted live.
    assert_eq!(resolve_cm_dialect(Some("sqlserver"), None), CmSqlDialect::MsSql);
    assert_eq!(resolve_dialect("sqlserver"), CmSqlDialect::StandardSql);

    // The map is exact: a capitalised key is an unknown key.
    assert_eq!(resolve_dialect("PostgreSQL"), CmSqlDialect::StandardSql);
    assert_eq!(resolve_dialect("MySQL"), CmSqlDialect::StandardSql);

    // The contract resolves family fallbacks the component never sees.
    assert_eq!(resolve_cm_dialect(Some("kiwi"), Some("mysql")), CmSqlDialect::MySql);
    assert_eq!(resolve_dialect("kiwi"), CmSqlDialect::StandardSql);
}

// ---- the fresh key / ime vectors --------------------------------------------

#[test]
fn command_gated_typing_never_inserts() {
    let mut block = SqlCodeBlockState::new("AB", "sqlite");
    block.focus();
    let mut tx = rune::Tx::default();

    // The engine's gate: Text keys with cmd or ctrl are not edits.
    let (consumed, changed) = block.handle_key(&text_key("x", Mods { cmd: true, ..Default::default() }), &mut tx);
    assert_eq!((consumed, changed.is_none()), (false, true));
    let (consumed, changed) = block.handle_key(&text_key("x", Mods { ctrl: true, ..Default::default() }), &mut tx);
    assert_eq!((consumed, changed.is_none()), (false, true));
    assert_eq!(block.doc(), "AB");

    // A plain insert still streams.
    let (consumed, changed) = block.handle_key(&text_key("C", Mods::default()), &mut tx);
    assert!(consumed);
    assert_eq!(changed.as_deref(), Some("CAB"));
}

#[test]
fn an_ime_commit_streams_through_the_editable_path() {
    let mut block = SqlCodeBlockState::new("", "postgresql");
    block.focus();

    // A focused engine consumes the ping without touching the document.
    let (consumed, changed) = block.handle_ime(&Ime::Enabled);
    assert_eq!((consumed, changed.is_none()), (true, true));

    // A commit inserts at the caret and streams once.
    let (consumed, changed) = block.handle_ime(&Ime::Commit("表".to_owned()));
    assert!(consumed);
    assert_eq!(changed.as_deref(), Some("表"));
    assert_eq!(block.doc(), "表");

    // An unfocused engine drops the commit too.
    let mut unfocused = SqlCodeBlockState::new("", "postgresql");
    let (consumed, changed) = unfocused.handle_ime(&Ime::Commit("x".to_owned()));
    assert_eq!((consumed, changed.is_none()), (false, true));
    assert_eq!(unfocused.doc(), "");
}

#[test]
fn a_remote_sync_resets_the_caret_and_keeps_editing() {
    let mut block = SqlCodeBlockState::new("DROP", "mariadb");
    block.focus();
    let mut tx = rune::Tx::default();

    // The remote sync replaces the whole document (the caret resets with it).
    block.set_code("CREATE TABLE t (id INT);");
    assert_eq!(block.doc(), "CREATE TABLE t (id INT);");

    // Editing after the sync inserts at the reset caret: the head.
    let (consumed, changed) = block.handle_key(&text_key("-", Mods::default()), &mut tx);
    assert!(consumed);
    assert_eq!(changed.as_deref(), Some("-CREATE TABLE t (id INT);"));

    // A second identical sync is a no-op.
    block.set_code("CREATE TABLE t (id INT);");
    assert_eq!(block.doc(), "CREATE TABLE t (id INT);");
}

#[test]
fn the_preset_seam_survives_a_doc_sync() {
    let mut block = SqlCodeBlockState::new("SELECT 1;", "postgresql");
    block.set_syntax_preset(Some("monokai"));
    block.set_code("SELECT 2;");
    assert_eq!(block.syntax_preset(), Some("monokai"));
    assert_eq!(block.doc(), "SELECT 2;");
}

// ---- the fresh mounted vectors ----------------------------------------------

#[test]
fn the_gutter_washes_follow_the_react_hexes_in_both_themes() {
    for dark in [true, false] {
        let rt = runtime(Scene::default(), DESIGN, dark);
        let gutter = element(&rt, "sql-code-block-gutters");
        let number = element(&rt, "sql-code-block-gutter-0");

        let expected_bg = if dark { "#111827" } else { "#f1f5f9" };
        let expected_fg = if dark { "#9ca3af" } else { "#64748b" };
        let bg = rune::color::hex(expected_bg).unwrap_or(rune::Color::WHITE);
        let fg = rune::color::hex(expected_fg).unwrap_or(rune::Color::WHITE);
        assert_eq!(gutter.fill, bg, "the {expected_bg} wash (dark={dark})");
        assert_eq!(number.text_color, Some(fg), "the {expected_fg} ink (dark={dark})");
    }
}

#[test]
fn a_read_only_block_swallows_typing_without_streaming() {
    let rt = runtime(Scene::default(), DESIGN, true);
    // React's readOnly view: the doc never changes and nothing streams.
    // (The read-only surface is not focusable, so keys fall through.)
    assert!(!element(&rt, "sql-code-block").focusable);
}

#[test]
fn an_editable_block_streams_after_a_remote_seed() {
    let mut scene = Scene::default();
    scene.editable = true;
    scene.block.set_code("SEED");
    let mut rt = runtime(scene, DESIGN, true);

    let (x, y) = rt.find("sql-code-block").expect("the block is pressable");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);

    rt.key(KeyEvent { key: Key::Text("L".to_owned()), mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.changes, vec!["LSEED"]);
    assert_eq!(rt.state.block.doc(), "LSEED");

    // A remote re-seed mid-editing never streams: only local edits do.
    rt.state.block.set_code("REPLACED");
    assert_eq!(rt.state.block.doc(), "REPLACED");
    assert_eq!(rt.state.changes.len(), 1);
}

#[test]
fn a_forty_line_document_fits_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let mut scene = Scene::default();
            let doc: String = (0..40)
                .map(|index| format!("SELECT {index} AS col_{index} FROM table_{index};"))
                .collect::<Vec<_>>()
                .join("\n");
            scene.block.set_code(&doc);
            let rt = runtime(scene, (w, h), dark);
            assert!(has(&rt, "sql-code-block-gutter-0"));
            assert!(has(&rt, "sql-code-block-gutter-39"));
            assert!(has(&rt, "sql-line-0"));
            // The visible rect: the scroll viewport clips the tail lines.
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
