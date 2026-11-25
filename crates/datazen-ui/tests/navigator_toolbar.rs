//! T465: `NavigatorToolbar` — exact React testids and i18n labels, every host
//! action through an injected port, optional/required button mounting, the
//! controlled search input and rect geometry at both window sizes in both
//! themes. Actions are recorded by name; no fake IPC, clipboard or timer.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::navigator::navigator_toolbar::{
    build, NavigatorToolbarHostPort, NavigatorToolbarVm, QueryEditFn, NEW_CONNECTION_KEY,
    REFRESH_KEY, SEARCH_INPUT_KEY, SEARCH_VALUES_KEY, TITLE_ROW_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: NavigatorToolbarVm,
    actions: Rc<RefCell<Vec<&'static str>>>,
    dark: bool,
    /// Whether the full host port is wired (optional buttons mount only then).
    wired: bool,
    /// Whether the controlled search input has an edit accessor.
    editable: bool,
}

fn scene(dark: bool, wired: bool, editable: bool) -> Scene {
    Scene {
        vm: NavigatorToolbarVm::default(),
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
        wired,
        editable,
    }
}

fn recorder(
    actions: Rc<RefCell<Vec<&'static str>>>,
    name: &'static str,
) -> Rc<dyn Fn(&mut Scene)> {
    Rc::new(move |_s: &mut Scene| actions.borrow_mut().push(name))
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let host = if s.wired {
        let a = s.actions.clone();
        NavigatorToolbarHostPort {
            on_export: Some(recorder(a.clone(), "export")),
            on_import: Some(recorder(a.clone(), "import")),
            on_new_connection: Some(recorder(a.clone(), "new-connection")),
            on_new_group: Some(recorder(a.clone(), "new-group")),
            on_collapse_all: Some(recorder(a.clone(), "collapse-all")),
            on_refresh: Some(recorder(a.clone(), "refresh")),
            on_collapse_sidebar: Some(recorder(a.clone(), "collapse-sidebar")),
            on_search_values: Some(recorder(a, "search-values")),
        }
    } else {
        NavigatorToolbarHostPort::<Scene>::default()
    };
    let edit: Option<QueryEditFn<Scene>> = if s.editable {
        Some(Rc::new(|state: &mut Scene| &mut state.vm.search_query))
    } else {
        None
    };
    build(&s.vm, &host, edit, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn label_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.label)
        .unwrap_or_default()
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn actions(rt: &Runtime<Scene>) -> Vec<&'static str> {
    rt.state.actions.borrow().clone()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn type_into(rt: &mut Runtime<Scene>, key: &str, value: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste(value);
    rt.frame(0.0);
}

#[test]
fn minimal_toolbar_keeps_the_react_testids_and_required_buttons() {
    let rt = runtime(scene(true, false, true), DESIGN);
    for key in [
        TITLE_ROW_KEY,
        NEW_CONNECTION_KEY,
        "navigator-new-group",
        "navigator-collapse-all",
        SEARCH_INPUT_KEY,
    ] {
        assert!(has_key(&rt, key), "{key} mounts in the minimal toolbar");
    }
    assert_eq!(text_of(&rt, "navigator-title"), t("nav.databases"));
    assert_eq!(label_of(&rt, NEW_CONNECTION_KEY), t("common.newConnection"));
    assert_eq!(label_of(&rt, "navigator-new-group"), t("common.newGroup"));
    assert_eq!(label_of(&rt, "navigator-collapse-all"), t("connWin.collapseAll"));
    // The kit puts the accessibility name (falling back to the placeholder)
    // on the inner editor keyed `input`, not on the field's outer key.
    let input_label = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("input"))
        .and_then(|e| e.label)
        .unwrap_or_default();
    assert_eq!(input_label, t("main.searchPlaceholder"));
    // React's own test pins that the value-search toggle is not here.
    assert!(!has_key(&rt, "global-object-search-toggle"));
    assert!(!has_key(&rt, SEARCH_VALUES_KEY), "value search needs its prop");
    for key in ["navigator-export", "navigator-import", REFRESH_KEY, "navigator-collapse-sidebar"] {
        assert!(!has_key(&rt, key), "optional {key} is absent without its callback");
    }
}

#[test]
fn every_button_routes_through_its_host_callback() {
    let mut rt = runtime(scene(false, true, true), DESIGN);
    rt.frame(0.0);
    for key in [
        "navigator-export",
        "navigator-import",
        NEW_CONNECTION_KEY,
        "navigator-new-group",
        "navigator-collapse-all",
        REFRESH_KEY,
        "navigator-collapse-sidebar",
        SEARCH_VALUES_KEY,
    ] {
        assert!(has_key(&rt, key), "{key} mounts when wired");
        click(&mut rt, key);
    }
    assert_eq!(
        actions(&rt),
        vec![
            "export",
            "import",
            "new-connection",
            "new-group",
            "collapse-all",
            "refresh",
            "collapse-sidebar",
            "search-values",
        ],
        "buttons emit actions in React's DOM order"
    );
    assert_eq!(label_of(&rt, "navigator-export"), t("common.exportConnections"));
    assert_eq!(label_of(&rt, "navigator-import"), t("common.importConnections"));
    assert_eq!(label_of(&rt, REFRESH_KEY), t("connWin.refresh"));
    assert_eq!(label_of(&rt, "navigator-collapse-sidebar"), t("connWin.collapseSidebar"));
    assert_eq!(label_of(&rt, SEARCH_VALUES_KEY), t("valueSearch.toolbarEntry"));
}

#[test]
fn unwired_buttons_are_inert() {
    let mut rt = runtime(scene(true, false, true), MIN);
    rt.frame(0.0);
    for key in [NEW_CONNECTION_KEY, "navigator-new-group", "navigator-collapse-all"] {
        click(&mut rt, key);
    }
    assert!(actions(&rt).is_empty(), "required buttons do nothing with no host port");
}

#[test]
fn the_search_input_is_controlled_and_can_be_disabled() {
    let mut rt = runtime(scene(true, true, true), DESIGN);
    rt.frame(0.0);
    type_into(&mut rt, SEARCH_INPUT_KEY, "orders");
    assert_eq!(rt.state.vm.search_text(), "orders", "typing updates the VM query");
    type_into(&mut rt, SEARCH_INPUT_KEY, "-detail");
    assert_eq!(rt.state.vm.search_text(), "orders-detail", "typing appends to the query");

    let mut disabled = runtime(scene(false, true, false), DESIGN);
    disabled.frame(0.0);
    type_into(&mut disabled, SEARCH_INPUT_KEY, "ignored");
    assert_eq!(disabled.state.vm.search_text(), "", "without an edit accessor the field is inert");
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut rt = runtime(scene(dark, true, true), (width, height));
            rt.frame(0.0);
            type_into(&mut rt, SEARCH_INPUT_KEY, "customer_email");
            for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, w, h] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows horizontally",
                    element.key,
                    element.rect
                );
                assert!(
                    y >= -0.5 && y + h <= height + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows vertically",
                    element.key,
                    element.rect
                );
            }
        }
    }
}
