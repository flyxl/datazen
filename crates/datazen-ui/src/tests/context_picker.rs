//! T217: the AI context picker — coder tests.
//! The pure arms (the query filter, the ctx.yaml rule, the flatten walk,
//! the recents LRU, the key router) plus the mounted surface (the
//! categories, the drill-in, the picks, the loading and empty rows).

use rune::{col, Cx, El, Env, KeyEvent, Key, Mods, Runtime};

use crate::components::ai::ai_egress_notice::{ContextItem, ContextKind};
use crate::components::ai::context_picker::{
    flatten_file_entries, is_ctx_yaml, item_icon, matches_query, table_label, ContextEntry,
    ContextPicker, ContextPickerState, PickerCategory, PickerKeyOutcome, PickerRow, PickerView,
    MAX_RECENT,
};
use crate::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn item(kind: ContextKind, id: &str, name: &str) -> ContextItem {
    ContextItem { kind, id: id.to_owned(), name: name.to_owned() }
}

fn entry(name: &str, path: &str, is_dir: bool, children: Vec<ContextEntry>) -> ContextEntry {
    ContextEntry {
        name: name.to_owned(),
        path: path.to_owned(),
        is_dir,
        children,
    }
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
            db_session: Some("conn-1".to_owned()),
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
    col().key("page").w_full().h_full().padding(24.0).child(picker)
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

// ---- the query filter -------------------------------------------------------

#[test]
fn matches_query_hits_the_name_or_the_id_case_insensitively() {
    let table = item(ContextKind::Table, "public.users", "users");
    assert!(matches_query(&table, "users"));
    assert!(matches_query(&table, "US")); // name, lowercased
    assert!(matches_query(&table, "public")); // id only
    assert!(matches_query(&table, "")); // the empty query matches all
    assert!(!matches_query(&table, "orders"));
}

#[test]
fn is_ctx_yaml_accepts_both_extensions_only() {
    assert!(is_ctx_yaml("tables.ctx.yaml"));
    assert!(is_ctx_yaml("groups.ctx.yml"));
    assert!(!is_ctx_yaml("tables.yaml"));
    assert!(!is_ctx_yaml("notes.md"));
    assert!(!is_ctx_yaml("tables.ctx.yamlx"));
    assert!(!is_ctx_yaml(""));
}

#[test]
fn item_icon_picks_the_layers_glyph_only_for_ctx_yaml_files() {
    let table = item(ContextKind::Table, "t", "users");
    let dir = item(ContextKind::Dir, "d", "docs");
    let plain = item(ContextKind::File, "f", "notes.md");
    let ctx = item(ContextKind::File, "c", "tables.ctx.yaml");
    let ctx_yml = item(ContextKind::File, "c2", "groups.ctx.yml");

    // Every kind has its own static glyph…
    assert!(!std::ptr::eq(item_icon(&table), item_icon(&dir)));
    assert!(!std::ptr::eq(item_icon(&table), item_icon(&plain)));
    assert!(!std::ptr::eq(item_icon(&dir), item_icon(&plain)));
    // …and the two ctx.yaml extensions share the Layers glyph, distinct
    // from the plain File glyph.
    assert!(std::ptr::eq(item_icon(&ctx), item_icon(&ctx_yml)));
    assert!(!std::ptr::eq(item_icon(&ctx), item_icon(&plain)));
}

// ---- the data mapping -------------------------------------------------------

#[test]
fn table_label_joins_the_schema_only_when_truthy() {
    assert_eq!(table_label(Some("public"), "users"), "public.users");
    assert_eq!(table_label(None, "users"), "users");
    // An empty schema is falsy in JS: the bare name.
    assert_eq!(table_label(Some(""), "users"), "users");
}

#[test]
fn flatten_walks_the_tree_depth_first_with_dirs_as_items() {
    let tree = vec![
        entry("schema.sql", "schema.sql", false, vec![]),
        entry(
            "migrations",
            "migrations",
            true,
            vec![
                entry("001_init.sql", "migrations/001_init.sql", false, vec![]),
                entry(
                    "nested",
                    "migrations/nested",
                    true,
                    vec![entry("deep.sql", "migrations/nested/deep.sql", false, vec![])],
                ),
            ],
        ),
    ];
    let flat = flatten_file_entries(&tree);
    let ids: Vec<&str> = flat.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "schema.sql",
            "migrations",
            "migrations/001_init.sql",
            "migrations/nested",
            "migrations/nested/deep.sql",
        ]
    );
    // The kinds follow is_dir; the names travel with the entries.
    assert_eq!(flat[0].kind, ContextKind::File);
    assert_eq!(flat[1].kind, ContextKind::Dir);
    assert_eq!(flat[3].kind, ContextKind::Dir);
    assert_eq!(flat[0].name, "schema.sql");
}

// ---- the rows memo ----------------------------------------------------------

#[test]
fn root_rows_carry_both_categories_with_a_session_and_files_without() {
    let mut picker = ContextPickerState::new();
    let with_session = picker.rows("", true);
    assert_eq!(
        with_session,
        vec![
            PickerRow::Category { id: PickerCategory::Tables, label: t("context.tables").to_owned() },
            PickerRow::Category { id: PickerCategory::Files, label: t("context.files").to_owned() },
        ]
    );
    let without_session = picker.rows("", false);
    assert_eq!(without_session.len(), 1);
    assert_eq!(
        without_session[0],
        PickerRow::Category { id: PickerCategory::Files, label: t("context.files").to_owned() }
    );
}

#[test]
fn root_rows_add_a_recent_section_only_when_recents_survive_the_query() {
    let mut picker = ContextPickerState::new();
    picker.set_recent(vec![
        item(ContextKind::Table, "users", "users"),
        item(ContextKind::File, "schema.sql", "schema.sql"),
    ]);
    let rows = picker.rows("", true);
    assert_eq!(rows.len(), 5); // two categories + section + two items
    assert_eq!(rows[2], PickerRow::Section { label: t("context.recent").to_owned() });
    assert_eq!(rows[3], PickerRow::Item(item(ContextKind::Table, "users", "users")));
    assert_eq!(rows[4], PickerRow::Item(item(ContextKind::File, "schema.sql", "schema.sql")));

    // The query narrows the recents (and only the recents at root).
    let filtered = picker.rows("schema", true);
    assert_eq!(filtered.len(), 4); // two categories + section + one survivor
    assert_eq!(filtered[2], PickerRow::Section { label: t("context.recent").to_owned() });
    assert_eq!(filtered[3], PickerRow::Item(item(ContextKind::File, "schema.sql", "schema.sql")));

    // A query that matches nothing drops the section entirely.
    let none = picker.rows("zzz", true);
    assert_eq!(none.len(), 2); // the categories stay
}

#[test]
fn drill_in_rows_filter_their_lists_by_the_query() {
    let mut picker = ContextPickerState::new();
    picker.set_tables(vec![
        item(ContextKind::Table, "users", "users"),
        item(ContextKind::Table, "orders", "orders"),
    ]);
    picker.set_view(PickerView::Tables);
    assert_eq!(picker.rows("", true).len(), 2);
    let filtered = picker.rows("user", true);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0], PickerRow::Item(item(ContextKind::Table, "users", "users")));

    // The Tables arm ignores the session flag: the rows are the tables
    // list regardless (React's rows memo has no showTablesCategory check
    // on the drill-in arms).
    assert_eq!(picker.rows("user", false).len(), 1);

    picker.set_files(vec![item(ContextKind::File, "a.sql", "a.sql")]);
    picker.set_view(PickerView::Files);
    assert_eq!(picker.rows("", false).len(), 1);
    assert_eq!(picker.rows("b", false).len(), 0);
}

// ---- the recents LRU --------------------------------------------------------

#[test]
fn push_recent_moves_the_pick_to_the_front_and_caps_at_eight() {
    let mut picker = ContextPickerState::new();
    for index in 0..MAX_RECENT {
        picker.push_recent(item(ContextKind::Table, &format!("t{index}"), "t"));
    }
    assert_eq!(picker.recent().len(), MAX_RECENT);
    assert_eq!(picker.recent()[0].id, "t7"); // newest first

    // The ninth pick drops the oldest, not the newest.
    picker.push_recent(item(ContextKind::Table, "t8", "t"));
    assert_eq!(picker.recent().len(), MAX_RECENT);
    assert_eq!(picker.recent()[0].id, "t8");
    assert_eq!(picker.recent()[7].id, "t1");
    assert!(!picker.recent().iter().any(|r| r.id == "t0"));

    // A re-push of an existing entry moves it to the front (dedupe by
    // kind + id).
    picker.push_recent(item(ContextKind::Table, "t1", "t"));
    assert_eq!(picker.recent()[0].id, "t1");
    assert_eq!(picker.recent().len(), MAX_RECENT);

    // The same id under another kind coexists.
    picker.push_recent(item(ContextKind::File, "t1", "t1"));
    assert_eq!(picker.recent()[0].kind, ContextKind::File);
    assert!(picker.recent().iter().filter(|r| r.id == "t1").count() >= 2);
}

// ---- the key router ---------------------------------------------------------

#[test]
fn the_key_router_moves_clamps_and_consumes() {
    let mut picker = ContextPickerState::new();
    let rows = picker.rows("", true); // two categories
    let event = |key: Key| KeyEvent { key, mods: Mods::default() };

    assert_eq!(picker.handle_key(&event(Key::Down), &rows), PickerKeyOutcome::Consumed);
    assert_eq!(picker.active_index(), 1);
    // Down clamps at the last row.
    picker.handle_key(&event(Key::Down), &rows);
    assert_eq!(picker.active_index(), 1);
    // Up clamps at zero.
    picker.handle_key(&event(Key::Up), &rows);
    assert_eq!(picker.active_index(), 0);
    picker.handle_key(&event(Key::Up), &rows);
    assert_eq!(picker.active_index(), 0);

    // An unhandled key is ignored (the caller lets it through).
    assert_eq!(picker.handle_key(&event(Key::Backspace), &rows), PickerKeyOutcome::Ignored);
    assert_eq!(picker.handle_key(&event(Key::Tab), &rows), PickerKeyOutcome::Ignored);
}

#[test]
fn enter_on_a_category_switches_the_view_and_resets_the_active_row() {
    let mut picker = ContextPickerState::new();
    let rows = picker.rows("", true);
    let event = |key: Key| KeyEvent { key, mods: Mods::default() };

    // Arrow to the Files category (index 1), Enter drills in.
    picker.handle_key(&event(Key::Down), &rows);
    assert_eq!(
        picker.handle_key(&event(Key::Enter), &rows),
        PickerKeyOutcome::Consumed
    );
    assert_eq!(picker.view(), PickerView::Files);
    assert_eq!(picker.active_index(), 0);

    // Escape pops the drill-in back to root.
    assert_eq!(
        picker.handle_key(&event(Key::Escape), &rows),
        PickerKeyOutcome::Consumed
    );
    assert_eq!(picker.view(), PickerView::Root);

    // Escape at root asks the host to close.
    assert_eq!(
        picker.handle_key(&event(Key::Escape), &rows),
        PickerKeyOutcome::Close
    );
}

#[test]
fn enter_on_an_item_pushes_the_recents_and_reports_the_pick() {
    let mut picker = ContextPickerState::new();
    picker.set_recent(vec![item(ContextKind::Table, "users", "users")]);
    let rows = picker.rows("", true); // cats + section + one item (index 3)
    let event = |key: Key| KeyEvent { key, mods: Mods::default() };

    picker.handle_key(&event(Key::Down), &rows);
    picker.handle_key(&event(Key::Down), &rows);
    picker.handle_key(&event(Key::Down), &rows);
    assert_eq!(picker.active_index(), 3);
    assert_eq!(
        picker.handle_key(&event(Key::Enter), &rows),
        PickerKeyOutcome::Selected(item(ContextKind::Table, "users", "users"))
    );
    // The recents advanced exactly once (the pick re-pushed the head).
    assert_eq!(picker.recent().len(), 1);

    // Enter on a section header consumes the key and does nothing.
    picker.set_view(PickerView::Root);
    let rows = picker.rows("", true);
    picker.handle_key(&event(Key::Down), &rows);
    picker.handle_key(&event(Key::Down), &rows);
    assert_eq!(picker.active_index(), 2); // the section
    assert_eq!(
        picker.handle_key(&event(Key::Enter), &rows),
        PickerKeyOutcome::Consumed
    );
    assert_eq!(picker.view(), PickerView::Root);
    assert_eq!(picker.recent().len(), 1); // nothing was pushed
}

// ---- the mounted surface ----------------------------------------------------

#[test]
fn the_root_mounts_both_categories_with_a_session() {
    let rt = runtime(Scene::default(), DESIGN, true);
    assert!(has(&rt, "context-picker"));
    assert!(has(&rt, "context-cat-tables"));
    assert!(has(&rt, "context-cat-files"));
    assert!(!has(&rt, "context-picker-back"));

    let mut scene = Scene::default();
    scene.db_session = None;
    let rt = runtime(scene, DESIGN, true);
    assert!(has(&rt, "context-cat-files"));
    assert!(!has(&rt, "context-cat-tables"));
}

#[test]
fn recent_items_render_under_their_section_at_root() {
    let mut scene = Scene::default();
    scene.picker.set_recent(vec![item(ContextKind::Table, "users", "users")]);
    let rt = runtime(scene, DESIGN, true);
    assert!(has(&rt, "context-section-最近"));
    let label = element(&rt, "context-section-最近-label").text;
    assert_eq!(label.as_deref(), Some(t("context.recent")));
    assert!(has(&rt, "context-item-Table:users"));
    let name = element(&rt, "context-item-Table:users-name").text;
    assert_eq!(name.as_deref(), Some("users"));
}

#[test]
fn drilling_into_tables_mounts_the_back_row_and_the_items() {
    let mut scene = Scene::default();
    scene.picker.set_tables(vec![
        item(ContextKind::Table, "users", "users"),
        item(ContextKind::Table, "orders", "orders"),
    ]);
    let mut rt = runtime(scene, DESIGN, true);

    click(&mut rt, "context-cat-tables");
    assert!(has(&rt, "context-picker-back"));
    assert!(has(&rt, "context-item-Table:users"));
    assert!(has(&rt, "context-item-Table:orders"));
    assert!(!has(&rt, "context-cat-files"));

    // The back row returns to root.
    click(&mut rt, "context-picker-back");
    assert!(has(&rt, "context-cat-files"));
    assert!(!has(&rt, "context-picker-back"));
}

#[test]
fn a_query_narrows_the_drill_in_but_not_the_root_categories() {
    let mut scene = Scene::default();
    scene.picker.set_tables(vec![
        item(ContextKind::Table, "users", "users"),
        item(ContextKind::Table, "orders", "orders"),
    ]);
    // Root: the query keeps the categories (the recents-only filter).
    let rt = runtime(scene.clone(), DESIGN, true);
    assert!(has(&rt, "context-cat-tables"));
    assert!(has(&rt, "context-cat-files"));

    // Drilled in: the same query narrows the tables list.
    scene.picker.set_view(PickerView::Tables);
    scene.query = "user".to_owned();
    let rt = runtime(scene, DESIGN, true);
    assert!(has(&rt, "context-item-Table:users"));
    assert!(!has(&rt, "context-item-Table:orders"));
}

#[test]
fn an_item_press_selects_and_advances_the_recents() {
    let mut scene = Scene::default();
    scene.picker.set_files(vec![
        item(ContextKind::File, "schema.sql", "schema.sql"),
        item(ContextKind::File, "users.sql", "users.sql"),
    ]);
    scene.picker.set_view(PickerView::Files);
    let mut rt = runtime(scene, DESIGN, true);

    click(&mut rt, "context-item-File:schema.sql");
    assert_eq!(rt.state.selects.len(), 1);
    assert_eq!(rt.state.selects[0], item(ContextKind::File, "schema.sql", "schema.sql"));
    assert_eq!(rt.state.picker.recent().len(), 1);
    assert_eq!(rt.state.picker.recent()[0].id, "schema.sql");
}

#[test]
fn the_empty_row_follows_the_react_ternary() {
    // Tables with no list: 暂无表.
    let mut scene = Scene::default();
    scene.picker.set_view(PickerView::Tables);
    let rt = runtime(scene.clone(), DESIGN, true);
    let label = element(&rt, "context-picker-empty-label").text;
    assert_eq!(label.as_deref(), Some(t("context.noTables")));

    // Files with a query that matches nothing: 未找到匹配的文件.
    scene.picker.set_view(PickerView::Files);
    scene.picker.set_files(vec![item(ContextKind::File, "a.sql", "a.sql")]);
    scene.query = "zzz".to_owned();
    let rt = runtime(scene.clone(), DESIGN, true);
    let label = element(&rt, "context-picker-empty-label").text;
    assert_eq!(label.as_deref(), Some(t("context.noResults")));

    // Files with no list and no query: the empty-directory line.
    scene.query = String::new();
    scene.picker.set_files(vec![]);
    let rt = runtime(scene, DESIGN, true);
    let label = element(&rt, "context-picker-empty-label").text;
    assert_eq!(label.as_deref(), Some(t("context.noFiles")));
}

#[test]
fn the_loading_row_replaces_the_list_while_a_fetch_is_in_flight() {
    let mut scene = Scene::default();
    scene.picker.set_view(PickerView::Tables);
    scene.picker.set_loading_tables(true);
    let rt = runtime(scene.clone(), DESIGN, true);
    assert!(has(&rt, "context-picker-loading"));
    assert!(has(&rt, "context-picker-loading-spinner"));
    assert!(!has(&rt, "context-picker-empty"));
    // The row label rides along the spinner.
    let label = element(&rt, "context-picker-loading-label").text;
    assert_eq!(label.as_deref(), Some(t("common.loading")));

    // The write end clears the flag and mounts the list.
    let mut scene = scene;
    scene.picker.set_tables(vec![item(ContextKind::Table, "users", "users")]);
    let rt = runtime(scene, DESIGN, true);
    assert!(!has(&rt, "context-picker-loading"));
    assert!(has(&rt, "context-item-Table:users"));
}

#[test]
fn the_keyboard_router_works_from_a_focused_row() {
    let mut scene = Scene::default();
    scene.picker.set_files(vec![item(ContextKind::File, "schema.sql", "schema.sql")]);
    scene.picker.set_view(PickerView::Files);
    let mut rt = runtime(scene, DESIGN, true);

    // Enter on the focused item row picks it.
    click(&mut rt, "context-item-File:schema.sql");
    assert_eq!(rt.state.selects.len(), 1);

    // Escape in the drill-in pops to root…
    press_key(&mut rt, Key::Escape);
    assert_eq!(rt.state.picker.view(), PickerView::Root);
    assert_eq!(rt.state.closes, 0);

    // …and Escape at root, from any focused row, closes the picker.
    for _ in 0..6 {
        if element(&rt, "context-cat-tables").focused
            || element(&rt, "context-cat-files").focused
        {
            break;
        }
        press_key(&mut rt, Key::Tab);
    }
    assert!(
        element(&rt, "context-cat-tables").focused || element(&rt, "context-cat-files").focused,
        "Tab lands on a router row"
    );
    press_key(&mut rt, Key::Escape);
    assert_eq!(rt.state.closes, 1);
}

#[test]
fn the_picker_fits_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [MIN, DESIGN] {
            let mut scene = Scene::default();
            scene.picker.set_tables(vec![
                item(ContextKind::Table, "users", "users"),
                item(ContextKind::Table, "a-very-long-table-name-here", "a-very-long-table-name-here"),
            ]);
            scene.picker.set_recent(vec![
                item(ContextKind::File, "a-very-long-file-name.csv", "a-very-long-file-name.csv"),
                item(ContextKind::Dir, "docs", "docs"),
            ]);
            let rt = runtime(scene, (w, h), dark);
            // The visible rect (the scroll viewport clips the tail rows
            // the same way the history dialog's audit measures it).
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
