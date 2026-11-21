//! T538: `TableColumnFilter` — toggle/badge, popover rows and PK badges,
//! column toggles, select/clear/reset, search filtering, Escape close and rect
//! geometry at both sizes in both palettes. Every change is a recorded
//! [`TableColumnFilterAction`]; no fake IPC.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::table_column_filter::{
    build, TableColumnFilterAction, TableColumnFilterActionFn, TableColumnFilterVm,
    BADGE_KEY, CLEAR_ALL_KEY, POPOVER_KEY, RESET_KEY, SEARCH_KEY, SELECT_ALL_KEY, TOGGLE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: TableColumnFilterVm,
    actions: Rc<RefCell<Vec<TableColumnFilterAction>>>,
    dark: bool,
}

fn column(name: &str, data_type: &str, pk: bool) -> crate::views::table_column_filter::FilterColumn {
    crate::views::table_column_filter::FilterColumn {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        is_primary_key: pk,
    }
}

fn vm() -> TableColumnFilterVm {
    TableColumnFilterVm {
        columns: vec![
            column("id", "int", true),
            column("username", "varchar(50)", false),
            column("email", "varchar(100)", false),
            column("created_at", "timestamp", false),
        ],
        ..Default::default()
    }
}

fn scene(vm: TableColumnFilterVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let on_action: Option<TableColumnFilterActionFn<Scene>> =
        Some(Rc::new(move |_st, action| actions.borrow_mut().push(action)));
    let edit: Rc<dyn for<'b> Fn(&'b mut Scene) -> &'b mut TextEdit> =
        Rc::new(|st: &mut Scene| &mut st.vm.search_query);
    let mut layer = Vec::new();
    build(&s.vm, &on_action, Some(edit), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn default_state_badge_and_popover_rows() {
    let rt = runtime(scene(vm(), true), DESIGN);
    assert!(has_key(&rt, TOGGLE_KEY));
    assert_eq!(text(&rt, TOGGLE_KEY), ""); // toggle text lives in children
    assert!(has_key(&rt, "table-column-filter-label"));
    assert_eq!(text(&rt, "table-column-filter-label"), t("tableData.columnFilter"));
    assert!(!has_key(&rt, BADGE_KEY), "no badge when all columns visible");
    assert!(!has_key(&rt, POPOVER_KEY));

    let mut filtered = vm();
    filtered.visible_columns = Some(vec!["id".to_owned(), "username".to_owned()]);
    let rt = runtime(scene(filtered, false), MIN);
    assert_eq!(text(&rt, BADGE_KEY), "2/4");

    let mut open = vm();
    open.open = true;
    let rt = runtime(scene(open, true), DESIGN);
    assert!(has_key(&rt, POPOVER_KEY));
    assert_eq!(text(&rt, "table-column-filter-header"), "");
    for name in ["id", "username", "email", "created_at"] {
        assert!(has_key(&rt, &format!("column-filter-item-{name}")));
        assert!(has_key(&rt, &format!("column-filter-checkbox-{name}")));
    }
    assert!(has_key(&rt, "column-filter-pk-id"), "primary key shows PK badge");
    assert!(!has_key(&rt, RESET_KEY), "reset hidden when not filtered");
}

#[test]
fn column_toggles_and_select_clear_reset_actions() {
    let mut state = vm();
    state.open = true;
    let mut rt = runtime(scene(state, false), DESIGN);
    click(&mut rt, "column-filter-checkbox-email");
    assert!(rt.state.actions.borrow().iter().any(|a| matches!(
        a,
        TableColumnFilterAction::ToggleColumn { name, next }
            if name == "email" && next.as_ref().is_some_and(|v| !v.contains(&"email".to_owned()))
    )));

    click(&mut rt, SELECT_ALL_KEY);
    assert!(rt.state.actions.borrow().contains(&TableColumnFilterAction::SelectAll));
    click(&mut rt, CLEAR_ALL_KEY);
    assert!(rt.state.actions.borrow().contains(&TableColumnFilterAction::ClearAll));

    let mut filtered = vm();
    filtered.open = true;
    filtered.visible_columns = Some(vec!["id".to_owned()]);
    let mut rt = runtime(scene(filtered, true), MIN);
    assert!(has_key(&rt, RESET_KEY));
    click(&mut rt, RESET_KEY);
    assert!(rt.state.actions.borrow().contains(&TableColumnFilterAction::Reset));
}

#[test]
fn search_filters_columns_and_escape_closes() {
    let mut state = vm();
    state.open = true;
    let mut rt = runtime(scene(state, true), DESIGN);
    let (x, y) = rt.find(SEARCH_KEY).unwrap_or_else(|| panic!("search field"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("user");
    rt.frame(0.0);
    assert_eq!(rt.state.vm.search_query.text, "user");
    assert!(has_key(&rt, "column-filter-item-username"));
    assert!(!has_key(&rt, "column-filter-item-email"), "non-matching column hidden");

    rt.paste("nonexistent");
    rt.frame(0.0);
    assert!(has_key(&rt, "table-column-filter-no-columns"));

    // Escape on the focused popover closes it. Click its padding corner (no
    // child there) so focus lands on the popover itself, not the search field.
    let [px, py, _, _] = element(&rt, POPOVER_KEY).rect;
    rt.pointer_down(px + 3.0, py + 3.0);
    rt.pointer_up(px + 3.0, py + 3.0);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(rt.state.actions.borrow().contains(&TableColumnFilterAction::Close));
}

#[test]
fn helpers_and_geometry() {
    let mut state = vm();
    assert_eq!(state.count_text(), "4/4");
    assert!(!state.is_filtered());
    state.visible_columns = Some(vec!["id".to_owned()]);
    assert!(state.is_filtered());
    assert_eq!(state.toggle_column("id"), Some(Vec::new()));
    state.visible_columns = Some(vec!["id".to_owned(), "email".to_owned(), "username".to_owned()]);
    assert_eq!(state.toggle_column("created_at"), None, "re-adding the last column means all");

    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut state = vm();
            state.open = true;
            state.visible_columns = Some(vec!["id".to_owned(), "username".to_owned()]);
            let rt = runtime(scene(state, dark), (w, h));
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "{w}x{h} dark={dark}: {:?} {:?} horizontal", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "{w}x{h} dark={dark}: {:?} {:?} vertical", el.key, el.rect);
            }
        }
    }
}
