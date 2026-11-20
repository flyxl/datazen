//! T489 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 7 tests do not pin: the secondary
//! (context-menu) press, tab-label edge cases, the active-only underline, and
//! a non-vacuous `visible`-rect geometry channel for the pinned 48pt strip.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, el, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::panel_tab_bar::{build, PanelTabAction, PanelTabVm, PanelType, BAR_HEIGHT};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn tab(id: &str, kind: PanelType) -> PanelTabVm {
    PanelTabVm {
        id: id.to_owned(),
        panel_type: kind,
        name: "Users".to_owned(),
        database: Some("app".to_owned()),
        connection_name: "Local".to_owned(),
        session_database: None,
    }
}

#[derive(Clone)]
struct Scene {
    panels: Vec<PanelTabVm>,
    active: Option<String>,
    actions: Rc<RefCell<Vec<PanelTabAction>>>,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let cb: Rc<dyn Fn(&mut Scene, PanelTabAction)> =
        Rc::new(move |_s, a| actions.borrow_mut().push(a));
    let mounted = build(&s.panels, s.active.as_deref(), Some(cb), s.dark);
    col().w_full().h_full().child(mounted.unwrap_or_else(|| el().key("no-tab-bar")))
}

fn runtime(panels: Vec<PanelTabVm>, active: Option<&str>, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let scene = Scene {
        panels,
        active: active.map(str::to_owned),
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    };
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn secondary_press_on_a_tab_emits_context_menu() {
    let mut rt = runtime(vec![tab("one", PanelType::Query)], Some("one"), DESIGN, true);
    let (x, y) = rt.find("panel-tab-one").unwrap_or_else(|| panic!("tab laid out"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
    assert_eq!(
        rt.state.actions.borrow().as_slice(),
        &[PanelTabAction::ContextMenu("one".to_owned())],
        "a secondary press routes the tab's context-menu intent to the host"
    );
}

#[test]
fn tab_label_handles_empty_name_and_database_precedence() {
    let mut vm = tab("one", PanelType::Query);
    vm.name = String::new();
    vm.database = None;
    vm.session_database = None;
    assert_eq!(vm.tab_label(), "Local", "empty name and no database leave only the connection");

    vm.name = "Users".to_owned();
    vm.session_database = Some("  session_db  ".to_owned());
    assert_eq!(vm.tab_label(), "Local · session_db · Users", "a whitespace session db is trimmed");

    vm.database = Some("  app  ".to_owned());
    assert_eq!(vm.tab_label(), "Local · app · Users", "an explicit database wins over the session one");
}

#[test]
fn only_the_active_tab_shows_an_underline() {
    let panels = vec![tab("one", PanelType::Query), tab("two", PanelType::Table)];
    let rt = runtime(panels, Some("one"), DESIGN, true);
    assert!(has_key(&rt, "panel-tab-underline-one"), "active tab has the underline");
    assert!(!has_key(&rt, "panel-tab-underline-two"), "inactive tab has none");
}

#[test]
fn pinned_strip_stays_inside_via_visible_rects() {
    assert_eq!(BAR_HEIGHT, 48.0);
    let panels = vec![tab("one", PanelType::Query), tab("two", PanelType::Table), tab("three", PanelType::View)];
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(panels.clone(), Some("one"), size, dark);
            assert!(has_key(&rt, "panel-tab-bar"), "{size:?} dark={dark}: root mounted");
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                );
            }
        }
    }
}

#[test]
fn tablist_label_uses_the_exact_locale_key() {
    let rt = runtime(vec![tab("one", PanelType::Query)], Some("one"), DESIGN, true);
    let list = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("panel-tab-list"));
    assert_eq!(list.and_then(|e| e.label), Some(t("panel.tabListLabel").to_owned()));
}
