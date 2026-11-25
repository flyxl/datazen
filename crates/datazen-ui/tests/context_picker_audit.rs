//! T217 independent audit: the AI context picker, fresh vectors.
//!
//! The coder tests cover one vector per rule; this audit attacks from
//! the other side: unicode queries that hit the id but not the name (and
//! the reverse), a ten-push recents interleaving with a cap at the
//! boundary, a stale active index after the rows shrink, the two-level
//! Escape walk without a session (the Tables drill-in arm ignores the
//! session flag), the mounted boundary pick (the last row via arrows),
//! a unicode-narrowed drill-in, and both windows' geometry with a
//! thirty-row list.

use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime};

use datazen_ui::components::ai::ai_egress_notice::{ContextItem, ContextKind};
use datazen_ui::components::ai::context_picker::{
    flatten_file_entries, matches_query, table_label, ContextEntry, ContextPicker,
    ContextPickerState, PickerKeyOutcome, PickerRow, PickerView, MAX_RECENT,
};
use datazen_ui::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn item(kind: ContextKind, id: &str, name: &str) -> ContextItem {
    ContextItem { kind, id: id.to_owned(), name: name.to_owned() }
}

fn entry(name: &str, path: &str, is_dir: bool, children: Vec<ContextEntry>) -> ContextEntry {
    ContextEntry { name: name.to_owned(), path: path.to_owned(), is_dir, children }
}

#[derive(Clone)]
struct Scene {
    picker: ContextPickerState,
    query: String,
    db_session: Option<String>,
    selects: Vec<ContextItem>,
    closes: usize,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            picker: ContextPickerState::new(),
            query: String::new(),
            db_session: Some("audit-conn".to_owned()),
            selects: Vec::new(),
            closes: 0,
        }
    }
}

fn view(scene: &Scene, cx: &mut Cx<Scene>) -> El {
    let dark = cx.system_dark();
    let picker = ContextPicker::<Scene>::new(&scene.picker)
        .query(scene.query.clone())
        .db_session(scene.db_session.as_deref())
        .state_access(|s: &mut Scene| &mut s.picker)
        .on_select(|s: &mut Scene, item| s.selects.push(item))
        .on_close(|s: &mut Scene| s.closes += 1)
        .build(dark);
    col().key("audit-page").w_full().h_full().padding(24.0).child(picker)
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

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key}: is pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press_key(rt: &mut Runtime<Scene>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
}

// ---- the fresh pure vectors -------------------------------------------------

#[test]
fn unicode_queries_hit_the_id_or_the_name_independently() {
    let chinese_name = item(ContextKind::Table, "cust", "客户表");
    // The name is Chinese, the id ASCII: each matches on its own field.
    assert!(matches_query(&chinese_name, "客户"));
    assert!(matches_query(&chinese_name, "cust"));
    assert!(matches_query(&chinese_name, "CU")); // case-insensitive on the id
    assert!(!matches_query(&chinese_name, "订单"));
    assert!(!matches_query(&chinese_name, "orders"));

    // The mirror: an ASCII name with a Chinese id.
    let chinese_id = item(ContextKind::File, "数据字典.sql", "dictionary");
    assert!(matches_query(&chinese_id, "数据"));
    assert!(matches_query(&chinese_id, "dict"));
    assert!(!matches_query(&chinese_id, "客户"));
}

#[test]
fn the_recents_lru_interleaves_and_caps_at_the_boundary() {
    let mut picker = ContextPickerState::new();
    for index in 0..10 {
        picker.push_recent(item(ContextKind::File, &format!("f{index}"), "f"));
    }
    // Ten pushes cap at eight: the two oldest dropped, newest first.
    assert_eq!(picker.recent().len(), MAX_RECENT);
    let ids: Vec<&str> = picker.recent().iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["f9", "f8", "f7", "f6", "f5", "f4", "f3", "f2"]);

    // A re-push of the oldest survivor moves it to the front, in place.
    picker.push_recent(item(ContextKind::File, "f2", "f"));
    let ids: Vec<&str> = picker.recent().iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["f2", "f9", "f8", "f7", "f6", "f5", "f4", "f3"]);
    assert_eq!(picker.recent().len(), MAX_RECENT);

    // The same id under a table kind coexists with the file kind.
    picker.push_recent(item(ContextKind::Table, "f2", "f2"));
    assert_eq!(picker.recent()[0].kind, ContextKind::Table);
    assert!(picker.recent().iter().any(|r| r.kind == ContextKind::File && r.id == "f2"));
}

#[test]
fn flatten_keeps_a_three_level_order_with_sibling_dirs() {
    let tree = vec![
        entry(
            "alpha",
            "alpha",
            true,
            vec![
                entry("a.sql", "alpha/a.sql", false, vec![]),
                entry(
                    "beta",
                    "alpha/beta",
                    true,
                    vec![entry("b.ctx.yaml", "alpha/beta/b.ctx.yaml", false, vec![])],
                ),
            ],
        ),
        entry("root.sql", "root.sql", false, vec![]),
    ];
    let flat = flatten_file_entries(&tree);
    let paths: Vec<&str> = flat.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(paths, vec!["alpha", "alpha/a.sql", "alpha/beta", "alpha/beta/b.ctx.yaml", "root.sql"]);
    assert_eq!(flat[0].kind, ContextKind::Dir);
    assert_eq!(flat[3].kind, ContextKind::File); // the ctx.yaml is a plain file item
    assert_eq!(flat[3].name, "b.ctx.yaml");
}

#[test]
fn table_label_fresh_boundary() {
    assert_eq!(table_label(Some("sales"), "orders"), "sales.orders");
    assert_eq!(table_label(None, "orders"), "orders");
    // The falsy empty schema keeps the bare name (JS parity).
    assert_eq!(table_label(Some(""), "orders"), "orders");
}

// ---- the fresh key-router vectors --------------------------------------------

#[test]
fn a_stale_active_index_clamps_into_the_next_rows() {
    let mut picker = ContextPickerState::new();
    picker.set_files(vec![
        item(ContextKind::File, "one", "one"),
        item(ContextKind::File, "two", "two"),
        item(ContextKind::File, "three", "three"),
    ]);
    picker.set_view(PickerView::Files);
    // Arrows walk to the last row (index 2).
    let rows = picker.rows("", false);
    let event = |key: Key| KeyEvent { key, mods: Mods::default() };
    picker.handle_key(&event(Key::Down), &rows);
    picker.handle_key(&event(Key::Down), &rows);
    assert_eq!(picker.active_index(), 2);

    // The list shrinks to one row: the stale active index clamps in.
    picker.set_files(vec![item(ContextKind::File, "only", "only")]);
    picker.clamp_active(picker.rows("", false).len());
    assert_eq!(picker.active_index(), 0);
}

#[test]
fn escape_walks_two_levels_without_a_session() {
    // Without a session the root holds the Files category alone: Enter
    // drills into Files, Escape walks back, and the second Escape
    // closes — the whole keyboard path with no tables category.
    let mut picker = ContextPickerState::new();
    let rows = picker.rows("", false); // [cat files]
    assert_eq!(rows.len(), 1);
    let event = |key: Key| KeyEvent { key, mods: Mods::default() };

    assert_eq!(picker.handle_key(&event(Key::Enter), &rows), PickerKeyOutcome::Consumed);
    assert_eq!(picker.view(), PickerView::Files);

    assert_eq!(picker.handle_key(&event(Key::Escape), &rows), PickerKeyOutcome::Consumed);
    assert_eq!(picker.view(), PickerView::Root);
    assert_eq!(picker.handle_key(&event(Key::Escape), &rows), PickerKeyOutcome::Close);

    // The Tables drill-in arm ignores the session flag: rows() stays the
    // tables list even when show_tables_category is false.
    picker.set_tables(vec![item(ContextKind::Table, "t", "t")]);
    picker.set_view(PickerView::Tables);
    assert_eq!(picker.rows("", false).len(), 1);
}

// ---- the fresh mounted vectors -----------------------------------------------

#[test]
fn the_last_row_is_reachable_by_arrows_and_picks() {
    let mut scene = Scene::default();
    scene.picker.set_files(vec![
        item(ContextKind::File, "one.sql", "one.sql"),
        item(ContextKind::File, "two.sql", "two.sql"),
        item(ContextKind::File, "three.sql", "three.sql"),
    ]);
    scene.picker.set_view(PickerView::Files);
    let mut rt = runtime(scene, DESIGN, true);

    // Focus the first row (its press picks it), arrow to the last, Enter
    // picks that one too: two selects, the recents holding the last pick.
    click(&mut rt, "context-item-File:one.sql");
    press_key(&mut rt, Key::Down);
    press_key(&mut rt, Key::Down);
    assert_eq!(rt.state.picker.active_index(), 2);
    press_key(&mut rt, Key::Enter);
    assert_eq!(rt.state.selects.len(), 2);
    assert_eq!(rt.state.selects[0].id, "one.sql");
    assert_eq!(rt.state.selects[1].id, "three.sql");
    assert_eq!(rt.state.picker.recent()[0].id, "three.sql");
}

#[test]
fn a_unicode_query_narrows_the_drill_in_on_the_id() {
    let mut scene = Scene::default();
    scene.picker.set_files(vec![
        item(ContextKind::File, "数据字典.sql", "dictionary"), // the id is Chinese
        item(ContextKind::File, "notes.md", "notes.md"),
    ]);
    scene.picker.set_view(PickerView::Files);
    scene.query = "数据".to_owned();
    let rt = runtime(scene, DESIGN, true);
    // The query hit the id (the name is English): exactly one row.
    assert!(has(&rt, "context-item-File:数据字典.sql"));
    assert!(!has(&rt, "context-item-File:notes.md"));
}

#[test]
fn the_mounted_boundary_pick_presses_once_and_pushes_once() {
    let mut scene = Scene::default();
    scene.picker.set_files(vec![item(ContextKind::File, "solo.sql", "solo.sql")]);
    scene.picker.set_view(PickerView::Files);
    let mut rt = runtime(scene, DESIGN, true);

    click(&mut rt, "context-item-File:solo.sql");
    click(&mut rt, "context-item-File:solo.sql");
    // Two presses pick twice, but the recents stay deduped.
    assert_eq!(rt.state.selects.len(), 2);
    assert_eq!(rt.state.picker.recent().len(), 1);
}

#[test]
fn the_tables_drill_in_without_a_session_still_lists_and_empties() {
    let mut scene = Scene::default();
    scene.db_session = None;
    scene.picker.set_view(PickerView::Tables);
    // No session, no tables, not loading: the drill-in arm still shows
    // its own empty line (noTables), not the files line.
    let rt = runtime(scene.clone(), DESIGN, true);
    let label = element(&rt, "context-picker-empty-label").text;
    assert_eq!(label.as_deref(), Some(t("context.noTables")));

    // With tables set, the rows mount even without a session.
    scene.picker.set_tables(vec![item(ContextKind::Table, "users", "users")]);
    let rt = runtime(scene, DESIGN, true);
    assert!(has(&rt, "context-item-Table:users"));
}

#[test]
fn the_loading_gate_reads_the_session_and_the_view_together() {
    // Files loading drives the loading row.
    let mut scene = Scene::default();
    scene.picker.set_view(PickerView::Files);
    scene.picker.set_loading_files(true);
    let rt = runtime(scene.clone(), DESIGN, true);
    assert!(has(&rt, "context-picker-loading"));

    // Tables loading without a session does NOT: needsTables is gated on
    // the session, so the empty row shows instead.
    scene.picker.set_view(PickerView::Tables);
    scene.picker.set_loading_files(false);
    scene.picker.set_loading_tables(true);
    scene.db_session = None;
    let rt = runtime(scene, DESIGN, true);
    assert!(!has(&rt, "context-picker-loading"));
    assert!(has(&rt, "context-picker-empty"));
}

#[test]
fn a_thirty_row_list_fits_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let mut scene = Scene::default();
            let files: Vec<ContextItem> = (0..30)
                .map(|index| item(ContextKind::File, &format!("file-{index}.sql"), &format!("file-{index}.sql")))
                .collect();
            scene.picker.set_files(files);
            scene.picker.set_view(PickerView::Files);
            let rt = runtime(scene, (w, h), dark);
            assert!(has(&rt, "context-item-File:file-0.sql"));
            assert!(has(&rt, "context-item-File:file-29.sql"));
            // The visible rect: the scroll viewport clips the tail rows
            // below the 240pt cap (the same instrument the history
            // dialog's audit uses).
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
