//! T542: `TableFkBreadcrumb` — segment labels (single/composite/no columns),
//! back/forward/dismiss host intents, disabled buttons, truncation, empty state
//! and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::table_fk_breadcrumb::{
    build, fk_origin_label, FkOrigin, TableFkAction, TableFkBreadcrumbVm,
};

fn origin() -> FkOrigin {
    FkOrigin {
        source_panel_id: "panel-1".to_owned(),
        source_table: "orders".to_owned(),
        local_columns: vec!["customer_id".to_owned()],
        target_table: "customers".to_owned(),
        target_columns: vec!["id".to_owned()],
        nav_entry_id: "nav-1".to_owned(),
    }
}

fn vm() -> TableFkBreadcrumbVm {
    TableFkBreadcrumbVm {
        origin: Some(origin()),
        can_go_back: true,
        can_go_forward: true,
        dismissible: true,
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: TableFkBreadcrumbVm,
    actions: Rc<RefCell<Vec<TableFkAction>>>,
    dark: bool,
}

fn scene(vm: TableFkBreadcrumbVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match action {
            TableFkAction::Back => state.vm.can_go_back = false,
            TableFkAction::Forward => state.vm.can_go_forward = false,
            TableFkAction::Dismiss => state.vm.dismissible = false,
        }
        actions.borrow_mut().push(action);
    });
    col().w_full().h_full().child(build(&s.vm, Some(callback), s.dark))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn origin_label_covers_single_composite_and_zero_columns() {
    assert_eq!(fk_origin_label(&origin()), "orders.customer_id → customers.id");
    let mut composite = origin();
    composite.local_columns = vec!["a".to_owned(), "b".to_owned()];
    composite.target_columns = vec!["x".to_owned(), "y".to_owned()];
    assert_eq!(fk_origin_label(&composite), "orders.(a, b) → customers.(x, y)");
    composite.local_columns.clear();
    composite.target_columns.clear();
    assert_eq!(fk_origin_label(&composite), "orders → customers");
}

#[test]
fn breadcrumb_keys_and_copy_are_pinned() {
    let mut state = vm();
    state.truncated = true;
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    for key in ["table-fk-breadcrumb", "table-fk-breadcrumb-label", "table-fk-breadcrumb-truncated", "table-fk-back", "table-fk-forward", "table-fk-dismiss"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    let expected = t("fkNav.breadcrumbFrom").replace("{source}", "orders.customer_id → customers.id");
    assert_eq!(element(&rt, "table-fk-breadcrumb-label").text.as_deref(), Some(expected.as_str()));
    assert_eq!(
        element(&rt, "table-fk-breadcrumb-truncated").text.as_deref(),
        Some(t("fkNav.stackTruncated"))
    );
    assert_eq!(element(&rt, "table-fk-back").label.as_deref(), Some(t("fkNav.back")));
    assert_eq!(element(&rt, "table-fk-forward").label.as_deref(), Some(t("fkNav.forward")));
}

#[test]
fn back_forward_and_dismiss_emit_host_intents() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "table-fk-back");
    assert_eq!(rt.state.actions.borrow().as_slice(), &[TableFkAction::Back]);
    click(&mut rt, "table-fk-forward");
    assert_eq!(rt.state.actions.borrow().as_slice(), &[TableFkAction::Back, TableFkAction::Forward]);
    click(&mut rt, "table-fk-dismiss");
    assert_eq!(rt.state.actions.borrow().as_slice(), &[TableFkAction::Back, TableFkAction::Forward, TableFkAction::Dismiss]);
}

#[test]
fn disabled_navigation_buttons_are_inert() {
    let mut state = vm();
    state.can_go_back = false;
    state.can_go_forward = false;
    let mut rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(!element(&rt, "table-fk-back").focusable, "back disabled");
    assert!(!element(&rt, "table-fk-forward").focusable, "forward disabled");
    click(&mut rt, "table-fk-back");
    click(&mut rt, "table-fk-forward");
    assert!(rt.state.actions.borrow().is_empty(), "disabled buttons emit nothing");
}

#[test]
fn truncation_and_dismiss_are_conditional() {
    let mut state = vm();
    state.truncated = false;
    state.dismissible = false;
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "table-fk-breadcrumb-truncated"));
    assert!(!has_key(&rt, "table-fk-dismiss"));
}

#[test]
fn missing_origin_renders_only_the_empty_marker() {
    let rt = runtime(scene(TableFkBreadcrumbVm::default(), true), (1440.0, 816.0));
    assert!(has_key(&rt, "table-fk-breadcrumb-empty"));
    assert!(!has_key(&rt, "table-fk-breadcrumb"));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = vm();
            state.truncated = true;
            let rt = runtime(scene(state, dark), size);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Dropping the composite-key parentheses made `origin_label_covers_single_composite_and_zero_columns`
//    fail on `orders.(a, b) → customers.(x, y)`.
// 2. Wiring Back to Forward made `back_forward_and_dismiss_emit_host_intents`
//    fail on `[Back, Forward, Dismiss]`.
