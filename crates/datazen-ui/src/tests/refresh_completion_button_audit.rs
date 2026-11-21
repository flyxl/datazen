//! Independent audit for T514: `RefreshCompletionButton` port.
//!
//! Drive the VM guards directly (branches the view harness only reaches
//! indirectly) and observe the button rect independently of the coder's
//! bounds-only geometry sweep.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query::toolbar::refresh_completion_button::{
    build, RefreshCompletionAction, RefreshCompletionActionFn, RefreshCompletionVm, BUTTON_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn vm() -> RefreshCompletionVm {
    RefreshCompletionVm {
        db_session_id: "sess-1".to_owned(),
        database: Some("app".to_owned()),
        icon_only: true,
        ..Default::default()
    }
}

struct Scene {
    vm: RefreshCompletionVm,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions: Rc<RefCell<Vec<RefreshCompletionAction>>> = Rc::new(RefCell::new(Vec::new()));
    let on_action: Option<RefreshCompletionActionFn<Scene>> = Some(Rc::new(
        move |state: &mut Scene, action| {
            if state.vm.begin_refresh() {
                actions.borrow_mut().push(action);
            }
        },
    ));
    col().w_full().h_full().child(build(&s.vm, on_action, s.dark))
}

fn runtime(vm: RefreshCompletionVm, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    let scene = Scene { vm, dark };
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("audit: {key} is laid out"))
}

#[test]
fn begin_refresh_guard_ignores_empty_session_and_busy_state() {
    let mut s = vm();
    s.db_session_id.clear();
    assert!(!s.begin_refresh(), "audit: empty session id must ignore the click");
    assert!(!s.busy, "audit: a refused click must not mark the button busy");

    let mut s = vm();
    s.busy = true;
    assert!(!s.begin_refresh(), "audit: an in-flight refresh must ignore a second click");

    let mut s = vm();
    s.error = "stale error".to_owned();
    s.done_message = Some("stale done".to_owned());
    assert!(s.begin_refresh(), "audit: an idle valid session accepts the click");
    assert!(s.busy, "audit: starting a refresh marks the button busy");
    assert!(s.error.is_empty(), "audit: starting a refresh clears the prior error");
    assert!(s.done_message.is_none(), "audit: starting a refresh clears the prior success message");
}

#[test]
fn error_answer_clears_busy_and_discards_a_prior_success_message() {
    let mut s = vm();
    s.begin_refresh();
    s.apply_refresh(Ok(()));
    assert!(s.done_message.is_some(), "audit setup: a success reports the done message");
    s.begin_refresh();
    s.apply_refresh(Err("schema reload failed".to_owned()));
    assert!(!s.busy, "audit: an error answer must clear busy");
    assert_eq!(s.error, "schema reload failed", "audit: the error is kept for the tooltip");
    assert_eq!(s.done_message, None, "audit: an error answer must discard the success message");
    assert_eq!(s.title(), "schema reload failed", "audit: the error replaces the default title");
}

#[test]
fn full_label_button_keeps_name_and_error_tooltip() {
    let mut s = vm();
    s.icon_only = false;
    s.error = "boom".to_owned();
    let rt = runtime(s, true, DESIGN);
    let el = element(&rt, BUTTON_KEY);
    assert_eq!(
        el.label.as_deref(),
        Some(t("query.refreshCompletion")),
        "audit: a full-label button keeps the exact accessible name"
    );
    assert_eq!(
        el.help.as_deref(),
        Some("boom"),
        "audit: the tooltip is the error text, not the button label"
    );
}

#[test]
fn default_tooltip_and_name_are_the_exact_i18n_strings() {
    let rt = runtime(vm(), false, MIN);
    let el = element(&rt, BUTTON_KEY);
    assert_eq!(
        el.label.as_deref(),
        Some(t("query.refreshCompletion")),
        "audit: accessible name is query.refreshCompletion"
    );
    assert_eq!(
        el.help.as_deref(),
        Some(t("query.refreshCompletionTitle")),
        "audit: default tooltip is query.refreshCompletionTitle"
    );
    assert_eq!(el.text.as_deref().unwrap_or(""), "", "audit: icon-only shows no text");
}

#[test]
fn button_rect_is_present_sized_and_in_bounds_at_both_sizes_and_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            for icon_only in [true, false] {
                let mut s = vm();
                s.icon_only = icon_only;
                let rt = runtime(s, dark, (w, h));
                let el = element(&rt, BUTTON_KEY);
                let [x, y, bw, bh] = el.rect;
                assert!(bw > 0.0 && bh > 0.0, "audit: button has a positive rect at {w}x{h} dark={dark} icon_only={icon_only}: {:?}", el.rect);
                assert!(x >= -0.5 && x + bw <= w + 0.5, "audit: button overflows horizontally at {w}x{h} dark={dark} icon_only={icon_only}: {:?}", el.rect);
                assert!(y >= -0.5 && y + bh <= h + 0.5, "audit: button overflows vertically at {w}x{h} dark={dark} icon_only={icon_only}: {:?}", el.rect);
            }
        }
    }
}
