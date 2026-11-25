//! Independent audit for T072 (the OnboardingWindow Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/OnboardingWindow.tsx` plus the
//! shell chrome it absorbs from `OnboardingWizard.tsx`: the root row with
//! the wizard's `onboarding-wizard` testid filling the window over
//! `bg-surface`, the brand sidebar composed on the left with its version
//! footer (em-dash placeholder until the host delivers), the `<main>`
//! column taking the remaining width, the wizard slot painting only when
//! the host supplies it, and the completion action seam retaining React's
//! `handleComplete` (`invoke('onboarding_complete')`) for the wizard.
//! Element evidence comes from `Runtime::elements()` (runtime layout),
//! never from a value the component reports about itself.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::ui::kit::kit;
use datazen_ui::views::onboarding::brand_sidebar::ASIDE_WIDTH;
use datazen_ui::views::onboarding::onboarding_window::OnboardingWindow;

/// The default onboarding window (960×720 — the size the desktop bootstrap
/// creates; the T021 audit's default too).
const W: f32 = 960.0;
const H: f32 = 720.0;

#[derive(Clone)]
struct Scene {
    version: Option<&'static str>,
    wizard: bool,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut window = OnboardingWindow::<Scene>::new();
    if let Some(version) = s.version {
        window = window.version(version);
    }
    if s.wizard {
        // A stand-in for T114's slot: one keyed column mounted as the
        // children of React's `<main>`.
        window = window.wizard(
            rune::col()
                .key("probe-wizard")
                .w_full()
                .grow(1.0)
                .child(rune::el().key("probe-wizard-body").h(240.0)),
        );
    }
    window.build(s.dark)
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (W, H),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<Scene>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

// ---- runtime evidence -------------------------------------------------------

#[test]
fn the_root_row_fills_the_window_over_the_surface_fill() {
    for dark in [true, false] {
        let rt = rt(Scene { version: None, wizard: false, dark });
        let root = maybe(&rt, "onboarding-wizard")
            .unwrap_or_else(|| panic!("the wizard-root testid is keyed (dark={dark})"));
        assert_eq!(
            root.rect,
            [0.0, 0.0, W, H],
            "React `flex h-screen` fills the window (dark={dark})"
        );
        assert_eq!(root.fill, kit(dark).c.bg_base, "bg-surface (dark={dark})");
    }
}

#[test]
fn the_brand_sidebar_is_composed_on_the_left() {
    let rt = rt(Scene { version: None, wizard: false, dark: true });
    let aside = maybe(&rt, "onboarding-sidebar")
        .expect("the T021 sidebar is composed into the shell");
    assert_eq!(
        aside.rect,
        [0.0, 0.0, ASIDE_WIDTH, H],
        "the sidebar anchors the left edge at its 352px width"
    );
    assert_eq!(
        text_of(&rt, "onboarding-version-label"),
        "v…",
        "BrandSidebar's useState('…') default rides through the shell"
    );
}

#[test]
fn a_delivered_version_reaches_the_sidebar_label() {
    let rt = rt(Scene { version: Some("0.9.7"), wizard: false, dark: true });
    assert_eq!(text_of(&rt, "onboarding-version-label"), "v0.9.7");
}

#[test]
fn the_main_column_takes_the_remaining_width_and_stays_empty_without_a_slot() {
    for dark in [true, false] {
        let rt = rt(Scene { version: None, wizard: false, dark });
        let main = maybe(&rt, "onboarding-window-main")
            .unwrap_or_else(|| panic!("the <main> column is keyed (dark={dark})"));
        assert_eq!(
            main.rect,
            [ASIDE_WIDTH, 0.0, W - ASIDE_WIDTH, H],
            "React `flex flex-1` of the root row (dark={dark})"
        );
        assert!(
            !has(&rt, "probe-wizard"),
            "the slot paints nothing until the host supplies it (dark={dark})"
        );
    }
}

#[test]
fn the_wizard_slot_paints_inside_the_main_column() {
    let rt = rt(Scene { version: None, wizard: true, dark: true });
    let main = maybe(&rt, "onboarding-window-main").expect("main");
    let [mx, my, mw, mh] = main.rect;
    let probe = maybe(&rt, "probe-wizard").expect("the host-fed slot is mounted");
    let [px, py, pw, ph] = probe.rect;
    assert!(px >= mx - 0.5 && py >= my - 0.5, "the slot starts inside main");
    assert!(
        px + pw <= mx + mw + 0.5 && py + ph <= my + mh + 0.5,
        "the slot ends inside main ({px}+{pw} vs {mx}+{mw}, {py}+{ph} vs {my}+{mh})"
    );
    assert_eq!(pw, mw, "the slot fills the column's width");
    assert_eq!(ph, mh, "the slot fills the column's height (grow)");
    assert!(has(&rt, "probe-wizard-body"), "the slot's own tree passes through");
}

#[test]
fn every_part_stays_inside_the_window() {
    for dark in [true, false] {
        let rt = rt(Scene { version: None, wizard: true, dark });
        let mut outside = Vec::new();
        for element in rt.elements() {
            let Some(key) = &element.key else { continue };
            let [x, y, w, h] = element.rect;
            if x < -0.5 || y < -0.5 || x + w > W + 0.5 || y + h > H + 0.5 {
                outside.push(format!("{key} [{x}, {y}, {w}, {h}]"));
            }
        }
        assert!(
            outside.is_empty(),
            "dark={dark}: parts leave the window: {outside:?}"
        );
    }
}

// ---- the completion seam (the React file's one piece of logic) --------------

struct Probe {
    fired: bool,
}

#[test]
fn the_completion_action_retains_reacts_handler() {
    // React: `handleComplete = async () => { await invoke('onboarding_complete'); }`,
    // handed to the wizard as its `onComplete` prop. The shell never fires
    // it; it must retain and re-deliver it for the wizard seam.
    let window = OnboardingWindow::<Probe>::new()
        .on_complete(|s: &mut Probe| s.fired = true);
    let action = window.completion_action().expect("the handler is retained");
    let mut probe = Probe { fired: false };
    action(&mut probe);
    assert!(probe.fired, "the delivered action is the one supplied");
    // ... and the shell still builds with the action in place.
    let _ = window.build(true);
}

#[test]
fn without_a_completion_action_the_seam_stays_empty() {
    let window = OnboardingWindow::<Probe>::new();
    assert!(window.completion_action().is_none());
    let _ = window.build(false);
}
