//! T489: `PanelTabBar` port — React testids/roles, tab label composition,
//! selection/close/keyboard actions, the pinned 48pt header height and
//! rect-based geometry at both window sizes in both themes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, el, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::panel_tab_bar::{
    build, PanelTabAction, PanelTabVm, PanelType, BAR_HEIGHT,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn panel(id: &str, name: &str) -> PanelTabVm {
    PanelTabVm {
        id: id.to_owned(),
        panel_type: PanelType::Query,
        name: name.to_owned(),
        database: Some("app".to_owned()),
        connection_name: "Local".to_owned(),
        session_database: None,
    }
}

fn panels() -> Vec<PanelTabVm> {
    let mut second = panel("panel-two", "Query 2");
    second.database = None;
    second.session_database = Some("session_db".to_owned());
    vec![panel("panel-one", "Query 1"), second]
}

#[derive(Clone)]
struct Scene {
    panels: Vec<PanelTabVm>,
    active: Option<String>,
    actions: Rc<RefCell<Vec<PanelTabAction>>>,
    dark: bool,
}

fn scene(panels: Vec<PanelTabVm>, dark: bool) -> Scene {
    Scene {
        panels,
        active: Some("panel-one".to_owned()),
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        // React's host sets activePanelId in onSelectPanel; the roving focus
        // target only becomes focusable once that state lands.
        if let PanelTabAction::Select(id) = &action {
            state.active = Some(id.clone());
        }
        actions.borrow_mut().push(action);
    });
    let mounted = build(&s.panels, s.active.as_deref(), Some(callback), s.dark);
    let child = match mounted {
        Some(bar) => bar,
        None => el().key("no-tab-bar"),
    };
    col().w_full().h_full().child(child)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
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

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent { key, mods: Mods::default() });
    for _ in 0..3 {
        rt.invalidate();
        rt.frame(1.0 / 60.0);
    }
}

// --- keys, labels and a11y ----------------------------------------------------

#[test]
fn empty_panel_list_mounts_nothing() {
    let rt = runtime(scene(Vec::new(), true), DESIGN);
    assert!(!has_key(&rt, "panel-tab-bar"), "React returns null for no panels");
    assert!(has_key(&rt, "no-tab-bar"), "probe fallback mounted");
}

#[test]
fn tablist_roles_labels_and_close_titles_match_react() {
    let rt = runtime(scene(panels(), true), DESIGN);
    for key in [
        "panel-tab-bar",
        "panel-tab-list",
        "panel-tab",
        "panel-tab-select",
        "panel-tab-close",
        "panel-tab-panel-one",
        "panel-tab-select-panel-one",
        "panel-tab-close-panel-one",
        "panel-tab-select-panel-two",
        "panel-tab-underline-panel-one",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(element(&rt, "panel-tab-list").label.as_deref(), Some(t("panel.tabListLabel")));

    let first_label = "Local · app · Query 1";
    assert_eq!(element(&rt, "panel-tab-select-panel-one").label.as_deref(), Some(first_label));
    assert_eq!(
        element(&rt, "panel-tab-close-panel-one").label.as_deref(),
        Some(t("panel.closeTab").replace("{title}", first_label).as_str()),
        "close uses the exact panel.closeTab key with the composed title"
    );
    // The second panel has no database of its own; React falls back to the
    // session database from the schema store.
    assert_eq!(
        element(&rt, "panel-tab-select-panel-two").label.as_deref(),
        Some("Local · session_db · Query 2")
    );
}

#[test]
fn roving_focus_marks_only_the_active_tab_selectable() {
    let rt = runtime(scene(panels(), true), DESIGN);
    assert!(element(&rt, "panel-tab-select-panel-one").focusable, "active tab is the Tab stop");
    assert!(!element(&rt, "panel-tab-select-panel-two").focusable, "inactive tab is not a Tab stop");
    assert!(has_key(&rt, "panel-tab-underline-panel-one"), "active underline mounted");
    assert!(!has_key(&rt, "panel-tab-underline-panel-two"), "inactive underline absent");
}

#[test]
fn localized_panel_types_use_exact_keys() {
    let mut fixed = panel("panel-fixed", "ignored-name");
    fixed.panel_type = PanelType::CreateTable;
    let rt = runtime(scene(vec![fixed], true), DESIGN);
    assert_eq!(
        element(&rt, "panel-tab-select-panel-fixed").label.as_deref(),
        Some(format!("Local · app · {}", t("common.newTable")).as_str())
    );
}

// --- actions ------------------------------------------------------------------

#[test]
fn pressing_a_tab_selects_it_and_close_emits_close() {
    let mut rt = runtime(scene(panels(), true), DESIGN);
    click(&mut rt, "panel-tab-select-panel-two");
    assert_eq!(
        rt.state.actions.borrow().first(),
        Some(&PanelTabAction::Select("panel-two".to_owned())),
        "pressing a tab selects its panel"
    );

    let mut rt = runtime(scene(panels(), true), DESIGN);
    click(&mut rt, "panel-tab-close-panel-one");
    assert_eq!(
        rt.state.actions.borrow().first(),
        Some(&PanelTabAction::Close("panel-one".to_owned())),
        "the close button emits Close, not Select"
    );
}

#[test]
fn arrow_home_and_end_move_selection_with_wraparound() {
    let mut rt = runtime(scene(panels(), true), DESIGN);
    // A click gives the active tab keyboard focus, as React's ref.focus() does.
    click(&mut rt, "panel-tab-select-panel-one");
    rt.state.actions.borrow_mut().clear();

    press(&mut rt, Key::Right);
    assert_eq!(
        rt.state.actions.borrow().first(),
        Some(&PanelTabAction::Select("panel-two".to_owned())),
        "ArrowRight selects the next tab"
    );
    rt.state.actions.borrow_mut().clear();

    press(&mut rt, Key::Right);
    assert_eq!(
        rt.state.actions.borrow().first(),
        Some(&PanelTabAction::Select("panel-one".to_owned())),
        "ArrowRight wraps at the end"
    );
    rt.state.actions.borrow_mut().clear();

    press(&mut rt, Key::End);
    assert_eq!(rt.state.actions.borrow().first(), Some(&PanelTabAction::Select("panel-two".to_owned())));
    rt.state.actions.borrow_mut().clear();

    press(&mut rt, Key::Home);
    assert_eq!(rt.state.actions.borrow().first(), Some(&PanelTabAction::Select("panel-one".to_owned())));
}

// --- pinned height + geometry -------------------------------------------------

#[test]
fn bar_is_a_pinned_48pt_header_and_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(scene(panels(), dark), size);
            let bar = element(&rt, "panel-tab-bar");
            assert_eq!(BAR_HEIGHT, 48.0, "the declared contract is h-12 (48pt)");
            assert!(
                (bar.rect[3] - 48.0).abs() <= 0.5,
                "the tab bar keeps its h-12 height: {size:?} dark={dark} rect={:?}",
                bar.rect
            );
            let outside: Vec<String> = rt
                .elements()
                .iter()
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
// 1. Dropping the session-database fallback made `tablist_roles_labels_and_close_titles_match_react`
//    fail on the second tab's composed label.
// 2. Changing BAR_HEIGHT to 40 made `bar_is_a_pinned_48pt_header...` fail with
//    `the tab bar keeps its h-12 height`.
