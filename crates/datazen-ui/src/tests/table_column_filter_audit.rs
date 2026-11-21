//! Independent audit for T538: `TableColumnFilter` port.
//!
//! Cover helper branches and states the coder does not drive, and pin two
//! React contracts from `TableColumnFilter.tsx`/`.test.tsx` that the port
//! appears to omit: `aria-expanded` on the toggle and outside-press close.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::views::table_column_filter::{
    build, FilterColumn, TableColumnFilterAction, TableColumnFilterActionFn, TableColumnFilterVm,
    TOGGLE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: TableColumnFilterVm,
    actions: Rc<RefCell<Vec<TableColumnFilterAction>>>,
    dark: bool,
}

fn column(name: &str, data_type: &str, pk: bool) -> FilterColumn {
    FilterColumn { name: name.to_owned(), data_type: data_type.to_owned(), is_primary_key: pk }
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

fn runtime(vm: TableColumnFilterVm, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark },
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("audit: {key} is laid out"))
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("audit: {key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn filtered_columns_matches_data_type_case_insensitively() {
    let mut s = vm();
    s.search_query = TextEdit::new("TIME".to_owned());
    let names: Vec<_> = s.filtered_columns().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["created_at"], "audit: dataType is matched case-insensitively");
    s.search_query = TextEdit::new("  USER  ".to_owned());
    let names: Vec<_> = s.filtered_columns().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["username"], "audit: name match trims the query");
}

#[test]
fn is_filtered_requires_strictly_fewer_visible_than_total() {
    let mut s = vm();
    assert!(!s.is_filtered(), "audit: None visible means all shown");
    s.visible_columns = Some(s.all_column_names());
    assert!(!s.is_filtered(), "audit: all visible is not filtered");
    s.visible_columns = Some(Vec::new());
    assert!(s.is_filtered(), "audit: empty visible is filtered");
    assert_eq!(s.count_text(), "0/4");
}

#[test]
fn toggle_column_adds_removes_and_collapses_to_all() {
    let mut s = vm();
    s.visible_columns = Some(vec!["id".to_owned(), "email".to_owned()]);
    assert_eq!(s.toggle_column("email"), Some(vec!["id".to_owned()]));
    s.visible_columns = Some(vec!["id".to_owned()]);
    assert_eq!(s.toggle_column("username"), Some(vec!["id".to_owned(), "username".to_owned()]));
    s.visible_columns = Some(vec!["id".to_owned()]);
    assert_eq!(s.toggle_column("id"), Some(Vec::new()), "audit: removing the last visible column leaves none");
    s.visible_columns = Some(vec!["id".to_owned(), "email".to_owned(), "username".to_owned()]);
    assert_eq!(s.toggle_column("created_at"), None, "audit: adding the last hidden column collapses to None");
}

#[test]
fn search_clear_and_no_columns_states_are_exact() {
    let mut s = vm();
    s.open = true;
    let rt = runtime(s.clone(), false, MIN);
    assert!(!has_key(&rt, "table-column-filter-search-clear"), "audit: no clear control for empty search");
    let mut s = s;
    s.search_query = TextEdit::new("nomatch".to_owned());
    let rt = runtime(s, true, MIN);
    assert!(has_key(&rt, "table-column-filter-search-clear"), "audit: clear control appears for a non-empty search");
    assert!(has_key(&rt, "table-column-filter-no-columns"), "audit: no-match state is shown");
}

#[test]
fn disabled_toggle_is_inert_at_both_sizes_and_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut s = vm();
            s.disabled = true;
            s.open = true;
            let mut rt = runtime(s, dark, (w, h));
            assert!(!element(&rt, TOGGLE_KEY).focusable, "audit: disabled toggle is not focusable");
            click(&mut rt, TOGGLE_KEY);
            assert!(rt.state.actions.borrow().is_empty(), "audit: disabled toggle emits nothing");
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "audit: {:?} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "audit: {:?} vertical overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
            }
        }
    }
}

// ----- React parity assertions -----

#[test]
fn react_toggle_exposes_aria_expanded() {
    let mut s = vm();
    s.open = true;
    let rt = runtime(s, true, DESIGN);
    assert_eq!(
        element(&rt, TOGGLE_KEY).expanded,
        Some(true),
        "DEFECT: React toggle sets aria-expanded={{open}} (TableColumnFilter.tsx:118)"
    );
}

#[test]
fn react_outside_press_closes_the_popover() {
    let mut s = vm();
    s.open = true;
    let mut rt = runtime(s, false, DESIGN);
    // Bottom-left corner is outside both the toggle (top-left) and the
    // right-anchored popover; React's document mousedown listener closes there.
    rt.pointer_down(10.0, DESIGN.1 - 10.0);
    rt.pointer_up(10.0, DESIGN.1 - 10.0);
    rt.frame(0.0);
    assert!(
        rt.state.actions.borrow().iter().any(|a| matches!(a, TableColumnFilterAction::Close)),
        "DEFECT: React closes the popover on an outside mousedown (TableColumnFilter.test.tsx:152-164)"
    );
}
