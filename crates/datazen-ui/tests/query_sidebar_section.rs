//! T508: `QuerySidebarSection` — panel visibility, favorites root/open/delete,
//! history scope groups/search, both context menus, host actions and rect
//! geometry at both window sizes in both themes. No fake IPC or clipboard.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, row, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::views::query::query_sidebar_section::{
    build, group_history, FavoriteVm, HistoryEntryVm, HistoryScope, QuerySidebarAction,
    QuerySidebarActionFn, SearchEditFn, SidebarSectionVm, FAVORITES_PANEL_KEY,
    FAVORITES_REFRESH_KEY, FAVORITES_ROOT_KEY, FALLBACK_KEY, FAVORITE_MENU_KEY, GROUP_LABEL_KEY,
    HISTORY_ITEM_KEY, HISTORY_MENU_KEY, HISTORY_PANEL_KEY, HISTORY_SEARCH_KEY,
    HISTORY_HEADER_MENU_KEY, SCOPE_ALL_KEY, SCOPE_CURRENT_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: SidebarSectionVm,
    actions: Rc<RefCell<Vec<QuerySidebarAction>>>,
    dark: bool,
}

fn favorite(id: &str, title: &str) -> FavoriteVm {
    FavoriteVm {
        id: id.to_owned(),
        title: title.to_owned(),
        sql: format!("SELECT '{title}';"),
    }
}

fn history(sql: &str, database: Option<&str>, at: &str, success: bool) -> HistoryEntryVm {
    HistoryEntryVm {
        id: sql.to_owned(),
        sql: sql.to_owned(),
        database: database.map(str::to_owned),
        success,
        execution_time_ms: 12,
        rows_affected: Some(3),
        executed_at: at.to_owned(),
    }
}

fn vm() -> SidebarSectionVm {
    SidebarSectionVm {
        favorites_visible: true,
        history_visible: true,
        favorites_root: Some("/datazen/favorites".to_owned()),
        favorites: vec![favorite("f1", "Nightly recon"), favorite("f2", "Daily check")],
        history: vec![
            history("SELECT * FROM analytics_events", Some("analytics"), "2026-03-03T00:00:00Z", true),
            history("SELECT * FROM analytics_other", Some("analytics"), "2026-03-02T00:00:00Z", true),
            history("SELECT * FROM app_orders", Some("app"), "2026-03-01T00:00:00Z", true),
            history("UPDATE app_users SET name = 'x'", Some("app"), "2026-02-28T00:00:00Z", false),
            history("DELETE FROM app_cache", Some("app"), "2026-02-27T00:00:00Z", true),
        ],
        selected_database: Some("app".to_owned()),
        history_search: TextEdit::new(String::new()),
        history_scope: HistoryScope::Current,
        ..Default::default()
    }
}

fn scene(vm: SidebarSectionVm, dark: bool) -> Scene {
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let on_action: Option<QuerySidebarActionFn<Scene>> = Some(Rc::new(move |state: &mut Scene, action| {
        state.vm.reduce(&action);
        actions.borrow_mut().push(action);
    }));
    let edit: Option<SearchEditFn<Scene>> = Some(Rc::new(|state: &mut Scene| &mut state.vm.history_search));
    build(&s.vm, &on_action, edit, s.dark, &mut layer);
    row().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn label_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.label)
        .unwrap_or_default()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn right_click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
}

fn select_label(rt: &Runtime<Scene>) -> bool {
    matches!(
        rt.state.actions.borrow().last(),
        Some(QuerySidebarAction::SelectHistory(_))
    )
}

#[test]
fn panels_mount_only_when_visible() {
    let mut hidden = vm();
    hidden.favorites_visible = false;
    let rt = runtime(scene(hidden, true), MIN);
    assert!(!has_key(&rt, FAVORITES_PANEL_KEY));
    assert!(has_key(&rt, HISTORY_PANEL_KEY));

    let mut hidden = vm();
    hidden.history_visible = false;
    let rt = runtime(scene(hidden, false), MIN);
    assert!(has_key(&rt, FAVORITES_PANEL_KEY));
    assert!(!has_key(&rt, HISTORY_PANEL_KEY));
}

#[test]
fn favorites_header_root_rows_and_actions() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    rt.frame(0.0);
    assert!(has_key(&rt, FAVORITES_PANEL_KEY));
    assert_eq!(label_of(&rt, FAVORITES_REFRESH_KEY), t("query.favoritesRefresh"));
    assert_eq!(text_of(&rt, FAVORITES_ROOT_KEY), "");
    assert_eq!(text_of(&rt, "query-favorites-root-path"), "/datazen/favorites");
    assert_eq!(text_of(&rt, "favorite-title-0"), "Nightly recon");
    assert_eq!(text_of(&rt, "favorite-sql-0"), "SELECT 'Nightly recon';");

    click(&mut rt, FAVORITES_REFRESH_KEY);
    assert_eq!(
        rt.state.actions.borrow().first(),
        Some(&QuerySidebarAction::RefreshFavorites)
    );
    click(&mut rt, "favorite-row-0");
    assert!(matches!(
        rt.state.actions.borrow().last(),
        Some(QuerySidebarAction::OpenFavorite(f)) if f.title == "Nightly recon"
    ));
    click(&mut rt, "favorite-delete-0");
    assert_eq!(
        rt.state.actions.borrow().last(),
        Some(&QuerySidebarAction::DeleteFavorite("f1".to_owned()))
    );
}

#[test]
fn favorites_context_menu_lists_all_four_react_entries() {
    let mut rt = runtime(scene(vm(), false), DESIGN);
    right_click(&mut rt, "favorite-item-0");
    assert!(has_key(&rt, &format!("{FAVORITE_MENU_KEY}-menu")));
    assert_eq!(label_of(&rt, &format!("{FAVORITE_MENU_KEY}-item0")), t("query.openInNewTab"));
    assert_eq!(label_of(&rt, &format!("{FAVORITE_MENU_KEY}-item3")), t("common.delete"));
    click(&mut rt, &format!("{FAVORITE_MENU_KEY}-item1"));
    assert_eq!(
        rt.state.actions.borrow().iter().find(|a| matches!(a, QuerySidebarAction::ApplySql(_))),
        Some(&QuerySidebarAction::ApplySql("SELECT 'Nightly recon';".to_owned()))
    );
}

#[test]
fn history_scope_groups_and_fallback_follow_react() {
    let rt = runtime(scene(vm(), true), DESIGN);
    assert!(has_key(&rt, HISTORY_PANEL_KEY));
    assert_eq!(label_of(&rt, SCOPE_CURRENT_KEY), t("query.historyScopeCurrent"));
    assert_eq!(label_of(&rt, SCOPE_ALL_KEY), t("query.historyScopeAll"));
    assert_eq!(text_of(&rt, "history-sql-0-0"), "SELECT * FROM app_orders");
    assert!(!has_key(&rt, "history-group-0"), "current scope has no group header");

    let mut all = vm();
    all.history_scope = HistoryScope::All;
    let rt = runtime(scene(all, true), DESIGN);
    assert_eq!(text_of(&rt, "history-group-text-0"), "analytics (2)");
    assert_eq!(text_of(&rt, "history-group-text-1"), "app (3)");
    assert!(has_key(&rt, &format!("{HISTORY_ITEM_KEY}-0-0")));

    let mut fallback = vm();
    fallback.selected_database = Some("reporting".to_owned());
    let rt = runtime(scene(fallback, false), MIN);
    assert!(has_key(&rt, FALLBACK_KEY));
    assert_eq!(text_of(&rt, "history-group-text-0"), "analytics (2)");
}

#[test]
fn history_search_filters_within_the_active_scope() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    let (x, y) = rt.find(HISTORY_SEARCH_KEY).unwrap_or_else(|| panic!("search input"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("SELECT * FROM");
    rt.frame(0.0);
    assert_eq!(rt.state.vm.history_search.text, "SELECT * FROM");
    assert_eq!(text_of(&rt, "history-sql-0-0"), "SELECT * FROM app_orders");
    assert!(!has_key(&rt, "history-sql-0-1"), "the update no longer matches");
    click(&mut rt, SCOPE_ALL_KEY);
    assert_eq!(text_of(&rt, "history-group-text-0"), "analytics (2)");
    assert_eq!(text_of(&rt, "history-group-text-1"), "app (1)");
}

#[test]
fn history_item_clear_and_both_header_menus() {
    let mut rt = runtime(scene(vm(), false), MIN);
    click(&mut rt, &format!("{HISTORY_ITEM_KEY}-0-0"));
    assert!(select_label(&rt), "clicking a history row applies its SQL");
    click(&mut rt, "query-history-clear");
    assert_eq!(
        rt.state.actions.borrow().last(),
        Some(&QuerySidebarAction::ClearHistory)
    );
    right_click(&mut rt, &format!("{HISTORY_ITEM_KEY}-0-0"));
    assert!(has_key(&rt, &format!("{HISTORY_MENU_KEY}-menu")));
    assert_eq!(label_of(&rt, &format!("{HISTORY_MENU_KEY}-item0")), t("query.applySql"));

    // A fresh runtime avoids the still-open row menu's dismiss scrim.
    let mut rt = runtime(scene(vm(), false), MIN);
    right_click(&mut rt, "query-history-header");
    assert!(has_key(&rt, &format!("{HISTORY_HEADER_MENU_KEY}-menu")));
}

#[test]
fn empty_history_hides_scope_controls_and_shows_the_empty_copy() {
    let mut empty = vm();
    empty.history.clear();
    let rt = runtime(scene(empty, true), MIN);
    assert_eq!(text_of(&rt, "query-no-history-text"), t("query.noHistory"));
    assert!(!has_key(&rt, SCOPE_CURRENT_KEY));
    assert!(!has_key(&rt, FALLBACK_KEY));
}

#[test]
fn grouping_and_context_level_helpers_keep_order_and_identity() {
    let groups = group_history(&vm().history, t("query.historyUnknownDb"));
    assert_eq!(groups[0].key, "analytics");
    assert_eq!(groups[1].key, "app");
    let path = vec!["db".to_owned(), "public".to_owned()];
    assert_eq!(
        SidebarSectionVm::select_context_level(&path, 1, "other"),
        Some(vec!["db".to_owned(), "other".to_owned()])
    );
    assert_eq!(SidebarSectionVm::select_context_level(&path, 0, ""), None);
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut rt = runtime(scene(vm(), dark), (width, height));
            rt.frame(0.0);
            for phase in 0..2 {
                if phase == 1 {
                    settle(&mut rt);
                    right_click(&mut rt, "favorite-item-0");
                }
                for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                    let [x, y, w, h] = element.rect;
                    assert!(
                        x >= -0.5 && x + w <= width + 0.5,
                        "{width}x{height} dark={dark} phase={phase}: {:?} {:?} overflows horizontally",
                        element.key,
                        element.rect
                    );
                    assert!(
                        y >= -0.5 && y + h <= height + 0.5,
                        "{width}x{height} dark={dark} phase={phase}: {:?} {:?} overflows vertically",
                        element.key,
                        element.rect
                    );
                }
            }
        }
    }
}
