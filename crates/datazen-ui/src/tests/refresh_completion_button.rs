//! T514: `RefreshCompletionButton` — exact accessible name/tooltip, all three
//! disabled sources, the double-click guard, the refresh action and rect
//! geometry at both window sizes in both themes. The host callback is recorded;
//! no schema command or clipboard write runs here.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query::toolbar::refresh_completion_button::{
    build, RefreshCompletionAction, RefreshCompletionActionFn, RefreshCompletionVm, BUTTON_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: RefreshCompletionVm,
    actions: Rc<RefCell<Vec<RefreshCompletionAction>>>,
    dark: bool,
}

fn vm() -> RefreshCompletionVm {
    RefreshCompletionVm {
        db_session_id: "sess-1".to_owned(),
        database: Some("app".to_owned()),
        icon_only: true,
        ..Default::default()
    }
}

fn scene(vm: RefreshCompletionVm, dark: bool) -> Scene {
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let on_action: Option<RefreshCompletionActionFn<Scene>> = Some(Rc::new(
        move |state: &mut Scene, action| {
            if state.vm.begin_refresh() {
                actions.borrow_mut().push(action);
            }
        },
    ));
    let mounted = build(&s.vm, on_action, s.dark);
    col().w_full().h_full().child(mounted)
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
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn icon_only_button_uses_the_exact_label_and_tooltip() {
    let rt = runtime(scene(vm(), true), DESIGN);
    assert_eq!(element(&rt, BUTTON_KEY).label.as_deref(), Some(t("query.refreshCompletion")));
    assert_eq!(element(&rt, BUTTON_KEY).help.as_deref(), Some(t("query.refreshCompletionTitle")));
    assert!(element(&rt, BUTTON_KEY).focusable, "an enabled trigger takes focus");
    assert_eq!(element(&rt, BUTTON_KEY).text.as_deref().unwrap_or(""), "", "icon-only shows no text");
}

#[test]
fn all_three_disabled_sources_block_press_and_focus() {
    for mut state in [
        { let mut s = vm(); s.disabled = true; s },
        { let mut s = vm(); s.db_session_id.clear(); s },
        { let mut s = vm(); s.busy = true; s },
    ] {
        let mut rt = runtime(scene(state.clone(), false), MIN);
        assert!(!element(&rt, BUTTON_KEY).focusable, "disabled state blocks focus");
        click(&mut rt, BUTTON_KEY);
        assert!(rt.state.actions.borrow().is_empty(), "a disabled trigger emits no action");
        state.apply_refresh(Ok(()));
    }
    let rt = runtime(scene(vm(), true), MIN);
    assert!(element(&rt, BUTTON_KEY).focusable, "the default state is enabled");
}

#[test]
fn error_replaces_the_title_and_only_success_reports_done() {
    let mut state = vm();
    state.apply_refresh(Err("metadata refresh failed".to_owned()));
    assert_eq!(state.title(), "metadata refresh failed");
    assert_eq!(state.done_message, None, "a failure never reports success");
    let rt = runtime(scene(state, true), MIN);
    assert_eq!(element(&rt, BUTTON_KEY).help.as_deref(), Some("metadata refresh failed"));

    let mut state = vm();
    state.begin_refresh();
    state.apply_refresh(Ok(()));
    assert_eq!(state.title(), t("query.refreshCompletionTitle"));
    assert_eq!(state.done_message.as_deref(), Some(t("query.refreshCompletionDone")));
}

#[test]
fn a_click_emits_refresh_once_and_the_busy_guard_holds() {
    let mut rt = runtime(scene(vm(), false), DESIGN);
    click(&mut rt, BUTTON_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![RefreshCompletionAction::Refresh]);
    assert!(rt.state.vm.busy, "the host marks the button busy while refreshing");
    assert!(!element(&rt, BUTTON_KEY).focusable, "the busy button is disabled");
    click(&mut rt, BUTTON_KEY);
    assert_eq!(rt.state.actions.borrow().len(), 1, "a second click is ignored while busy");
    rt.state.vm.apply_refresh(Ok(()));
    assert!(!rt.state.vm.busy, "the answer clears busy");
    assert_eq!(
        rt.state.vm.done_message.as_deref(),
        Some(t("query.refreshCompletionDone")),
    );
    // A fresh runtime built after the answer mounts an enabled trigger.
    let fresh = runtime(scene(rt.state.vm.clone(), true), MIN);
    assert!(element(&fresh, BUTTON_KEY).focusable, "the button is enabled once the answer lands");
}

#[test]
fn full_label_button_keeps_the_same_key_and_action() {
    let mut state = vm();
    state.icon_only = false;
    let mut rt = runtime(scene(state, true), MIN);
    assert_eq!(element(&rt, BUTTON_KEY).label.as_deref(), Some(t("query.refreshCompletion")));
    click(&mut rt, BUTTON_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![RefreshCompletionAction::Refresh]);
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            for icon_only in [true, false] {
                let mut state = vm();
                state.icon_only = icon_only;
                state.error = "a long metadata refresh failure message".to_owned();
                let rt = runtime(scene(state, dark), (width, height));
                for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                    let [x, y, w, h] = element.rect;
                    assert!(
                        x >= -0.5 && x + w <= width + 0.5,
                        "{width}x{height} dark={dark} icon_only={icon_only}: {:?} {:?} overflows horizontally",
                        element.key,
                        element.rect
                    );
                    assert!(
                        y >= -0.5 && y + h <= height + 0.5,
                        "{width}x{height} dark={dark} icon_only={icon_only}: {:?} {:?} overflows vertically",
                        element.key,
                        element.rect
                    );
                }
            }
        }
    }
}
