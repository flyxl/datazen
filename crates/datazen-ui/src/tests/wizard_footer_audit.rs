//! Independent audit for T247 (the WizardFooter Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/WizardFooter.tsx` (plus the
//! wizard's `{state.step !== 's3' && <WizardFooter … />}` mount and its
//! `__tests__/OnboardingWizard.test.tsx`): the shared 56px chrome height
//! ending on the same bottom line as the sidebar's version row, the step
//! indicator's two dots and label (hidden but laid out on S0/S3), the
//! skip/back/continue/finish visibility and disabled rules, and the four
//! action wirings. Runtime evidence comes from `Runtime::elements()`
//! (runtime layout) with the footer mounted exactly as production mounts
//! it — as the wizard's footer seat inside the T072 window — never from a
//! value the component reports about itself; the S3 guards are probed on
//! a standalone mount because the shell never seats the footer at S3.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::onboarding::onboarding_layout::WIZARD_FOOTER_HEIGHT;
use crate::views::onboarding::onboarding_window::OnboardingWindow;
use crate::views::onboarding::onboarding_wizard::{OnboardingWizard, Step};
use crate::views::onboarding::wizard_footer::WizardFooter;

/// The default onboarding window (960×720 — the T021/T072 audits' default).
const W: f32 = 960.0;
const H: f32 = 720.0;

#[derive(Clone)]
struct Scene {
    step: Step,
    can_continue: bool,
    show_continue: bool,
    show_finish: bool,
    busy: bool,
    /// Mount the footer standalone (not through the wizard's seat), so the
    /// component's own S3 guards are probeable — the shell never seats the
    /// footer at S3.
    standalone: bool,
    dark: bool,
    skip_fired: usize,
    back_fired: usize,
    continue_fired: usize,
    finish_fired: usize,
}

fn base() -> Scene {
    Scene {
        step: Step::S0,
        can_continue: false,
        show_continue: false,
        show_finish: false,
        busy: false,
        standalone: false,
        dark: true,
        skip_fired: 0,
        back_fired: 0,
        continue_fired: 0,
        finish_fired: 0,
    }
}

fn footer(s: &Scene) -> El {
    WizardFooter::<Scene>::new(s.step)
        .can_continue(s.can_continue)
        .show_continue(s.show_continue)
        .show_finish(s.show_finish)
        .busy(s.busy)
        .on_skip(|s: &mut Scene| s.skip_fired += 1)
        .on_back(|s: &mut Scene| s.back_fired += 1)
        .on_continue(|s: &mut Scene| s.continue_fired += 1)
        .on_finish(|s: &mut Scene| s.finish_fired += 1)
        .build(s.dark)
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let footer = footer(s);
    if s.standalone {
        // The component on its own — its S3 guards without the seat's gate.
        return col().key("footer-standalone").w_full().child(footer);
    }
    // The production mounting: the wizard's footer seat inside the T072
    // window (the seat itself gates on the step, exactly as React's
    // conditional).
    let wizard = OnboardingWizard::<Scene>::new().step(s.step).footer(footer);
    OnboardingWindow::<Scene>::new().wizard(wizard.build()).build(s.dark)
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

/// The first text element under the element keyed `key` — a button's label
/// (the `ai_provider_step_audit` helper: a kit button keeps its ink in a
/// `role == "text"` child, and the hand-rolled skip row does the same).
fn label_under(rt: &Runtime<Scene>, key: &str) -> String {
    let Some(el) = maybe(rt, key) else {
        return String::new();
    };
    let prefix = el.path;
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.role == "text" && e.path.starts_with(&prefix))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

/// Advance the clock past the kit's enter/hover motion (~0.4s) so freshly
/// mounted parts read their full geometry (the sibling audits' helper).
fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

/// Click the element keyed `key` (pointer move first — hit-testing follows
/// the pointer; the T107 lesson).
fn press(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

// ---- the chrome row -------------------------------------------------------------

/// React's one layout rule (the file's own doc comment): the footer shares
/// its fixed 56px (42pt) height with the brand sidebar's version row, both
/// centring their content, so the step indicator and the version label end
/// on the same bottom line.
#[test]
fn the_footer_shares_the_chrome_height_with_the_sidebar() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let footer = maybe(&rt, "onboarding-footer").expect("the footer root");
        assert!(
            (footer.rect[3] - WIZARD_FOOTER_HEIGHT).abs() < 0.5,
            "dark={dark}: the footer is the shared {}pt chrome row (got {})",
            WIZARD_FOOTER_HEIGHT,
            footer.rect[3]
        );
        let sidebar_foot = maybe(&rt, "onboarding-sidebar-foot").expect("the sidebar's version row");
        let footer_bottom = footer.rect[1] + footer.rect[3];
        let sidebar_bottom = sidebar_foot.rect[1] + sidebar_foot.rect[3];
        assert!(
            (footer_bottom - sidebar_bottom).abs() < 0.5,
            "dark={dark}: both chrome rows end on the same bottom line (footer {footer_bottom} vs sidebar {sidebar_bottom})"
        );
        // The step row sits at the footer's left padding (px-[48px] = 36pt).
        let indicator = maybe(&rt, "onboarding-step-indicator").expect("the step indicator");
        assert!(
            (indicator.rect[0] - (footer.rect[0] + 36.0)).abs() < 0.5,
            "dark={dark}: the indicator starts at the footer's 48px left padding"
        );
    }
}

/// Nothing renders outside the window horizontally, in either theme.
#[test]
fn every_part_stays_inside_the_window_horizontally() {
    for dark in [true, false] {
        let mut rt = rt(Scene { step: Step::S1, show_continue: true, dark, ..base() });
        settle(&mut rt);
        let mut outside = Vec::new();
        for element in rt.elements() {
            let Some(key) = &element.key else { continue };
            let [x, _y, w, _h] = element.rect;
            if x < -0.5 || x + w > W + 0.5 {
                outside.push(format!("{key} [{x}, {w}]"));
            }
        }
        assert!(outside.is_empty(), "dark={dark}: parts leave the window: {outside:?}");
    }
}

// ---- the step indicator ----------------------------------------------------------

/// The indicator is hidden — but still laid out — on the entry (S0) and
/// done (S3) screens: React's `invisible` keeps the geometry, so the port
/// paints it at opacity 0 (never an unmounted subtree, which would move
/// nothing but change the a11y tree).
#[test]
fn the_step_indicator_hides_but_lays_out_on_s0_and_s3() {
    // S0, through the production seat.
    {
        let mut rt = rt(Scene { ..base() });
        settle(&mut rt);
        let indicator = maybe(&rt, "onboarding-step-indicator").expect("the indicator is laid out on S0");
        assert!(
            (indicator.opacity - 0.0).abs() < 0.01,
            "S0: the indicator paints at opacity 0 (invisible), got {}",
            indicator.opacity
        );
        assert!(indicator.rect[2] > 0.0, "S0: the indicator keeps its geometry");
        // The hidden indicator never crowds the actions: both dots and the
        // label are inside its box.
        let label = maybe(&rt, "onboarding-step-label").expect("the label is laid out on S0");
        assert!(
            label.rect[0] >= indicator.rect[0]
                && label.rect[0] + label.rect[2] <= indicator.rect[0] + indicator.rect[2] + 0.5,
            "S0: the label rides inside the hidden indicator"
        );
    }

    // S3, on a standalone mount (the shell never seats the footer there).
    {
        let mut rt = rt(Scene { step: Step::S3, standalone: true, ..base() });
        settle(&mut rt);
        let indicator = maybe(&rt, "onboarding-step-indicator").expect("the indicator is laid out at S3");
        assert!(
            (indicator.opacity - 0.0).abs() < 0.01,
            "S3: the indicator paints at opacity 0 (invisible), got {}",
            indicator.opacity
        );
    }
}

/// The two dots: the current step's is the wide accent bar (`w-5` =
/// 15pt), a passed step's the accent pin at 40%, a future step's the
/// edge-coloured pin — every dot 7px (5.25pt) tall.
#[test]
fn the_dots_mark_the_current_and_passed_steps() {
    for (step, wide, narrow) in [(Step::S1, "onboarding-footer-dot-1", "onboarding-footer-dot-2"), (Step::S2, "onboarding-footer-dot-2", "onboarding-footer-dot-1")] {
        let mut rt = rt(Scene { step, ..base() });
        settle(&mut rt);
        let current = maybe(&rt, wide).unwrap_or_else(|| panic!("{step:?}'s current dot"));
        let other = maybe(&rt, narrow).unwrap_or_else(|| panic!("{step:?}'s other dot"));
        assert!(
            (current.rect[2] - 15.0).abs() < 0.5,
            "{step:?}: the current dot is w-5 (15pt), got {}",
            current.rect[2]
        );
        assert!(
            (other.rect[2] - 5.25).abs() < 0.5,
            "{step:?}: the other dot is w-[7px] (5.25pt), got {}",
            other.rect[2]
        );
        for dot in [current, other] {
            assert!(
                (dot.rect[3] - 5.25).abs() < 0.5,
                "{step:?}: every dot is h-[7px] (5.25pt), got {}",
                dot.rect[3]
            );
            assert!(dot.radius >= dot.rect[3] / 2.0, "{step:?}: rounded-full (a pill)");
        }
    }
    // S1: dot 2 is the future pin (edge ink, a solid surface).
    {
        let rt = rt(Scene { step: Step::S1, ..base() });
        let future = maybe(&rt, "onboarding-footer-dot-2").expect("S1's future dot");
        assert!(
            (future.fill.a - 1.0).abs() < 0.02,
            "S1's future dot is the opaque bg-edge pin (got alpha {})",
            future.fill.a
        );
    }
    // S2: dot 1 is the passed pin — the accent at 40%.
    {
        let rt = rt(Scene { step: Step::S2, ..base() });
        let passed = maybe(&rt, "onboarding-footer-dot-1").expect("S2's passed dot");
        assert!(
            (passed.fill.a - 0.40).abs() < 0.02,
            "S2's passed dot is bg-accent/40 (got alpha {})",
            passed.fill.a
        );
    }
}

/// The label: the shared chrome text (`WIZARD_CHROME_TEXT_CLASS`'s mono
/// half) reading step 1's / step 2's own pack entry.
#[test]
fn the_label_reads_the_steps_own_pack_entry() {
    {
        let rt = rt(Scene { step: Step::S1, ..base() });
        assert_eq!(text_of(&rt, "onboarding-step-label"), t("onboarding.s1.stepLabel"));
    }
    {
        let rt = rt(Scene { step: Step::S2, ..base() });
        assert_eq!(text_of(&rt, "onboarding-step-label"), t("onboarding.s2.stepLabel"));
    }
    // The hidden S0 indicator still labels itself step 1's way (React
    // renders the same span; `invisible` only stops the painting).
    {
        let rt = rt(Scene { ..base() });
        assert_eq!(text_of(&rt, "onboarding-step-label"), t("onboarding.s1.stepLabel"));
    }
}

// ---- the buttons -----------------------------------------------------------------

/// Skip: every step except S3 (the component's own guard — the shell's
/// seat adds the same gate in production). Back: S1 and S2 only.
#[test]
fn skip_and_back_follow_their_step_guards() {
    // S0: skip yes, back no (the cards are the only way back).
    {
        let rt = rt(Scene { ..base() });
        assert!(has(&rt, "onboarding-skip"), "S0 shows skip");
        assert!(!has(&rt, "onboarding-back"), "S0 hides back");
    }
    // S1 and S2: both.
    for step in [Step::S1, Step::S2] {
        let rt = rt(Scene { step, ..base() });
        assert!(has(&rt, "onboarding-skip"), "{step:?} shows skip");
        assert!(has(&rt, "onboarding-back"), "{step:?} shows back");
    }
    // S3 (standalone mount): neither — the footer's own guards.
    {
        let rt = rt(Scene { step: Step::S3, standalone: true, ..base() });
        assert!(!has(&rt, "onboarding-skip"), "S3 hides skip (the component's own guard)");
        assert!(!has(&rt, "onboarding-back"), "S3 hides back");
    }
    // The skip button's X glyph leads its label, in the muted chrome ink.
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let x = maybe(&rt, "onboarding-footer-skip-icon").expect("the skip X glyph");
    let skip = maybe(&rt, "onboarding-skip").expect("the skip button");
    let label = rt
        .elements()
        .into_iter()
        .find(|e| !e.exiting && e.role == "text" && e.path.starts_with(&skip.path))
        .expect("the skip label");
    assert!(x.rect[0] < label.rect[0], "the X glyph leads the label");
    assert_eq!(label.text.as_deref(), Some(t("onboarding.common.skip")));
    assert!((x.rect[3] - 9.0).abs() < 0.5, "the glyph is h-3 (12px = 9pt), got {}", x.rect[3]);
    assert_eq!(label.text_color, Some(crate::ui::kit::kit(true).c.text_muted), "text-fg-muted");
}

/// Continue appears only when the step asks for it, and only a complete
/// step 1 unlocks it (React's `disabled={!canContinue}`).
#[test]
fn continue_shows_and_unlocks_with_the_step() {
    // Hidden on S0 (the entry cards are the way forward).
    {
        let rt = rt(Scene { ..base() });
        assert!(!has(&rt, "onboarding-continue"), "S0 shows no continue");
    }
    // Shown but disabled while step 1 is incomplete.
    {
        let mut rt = rt(Scene { step: Step::S1, show_continue: true, can_continue: false, ..base() });
        settle(&mut rt);
        let continue_ = maybe(&rt, "onboarding-continue").expect("S1 shows continue");
        assert!(
            !continue_.focusable,
            "an incomplete step 1 keeps Continue disabled (not focusable)"
        );
        press(&mut rt, "onboarding-continue");
        assert_eq!(rt.state.continue_fired, 0, "the disabled Continue never fires");
    }
    // Complete: enabled, and the press reaches the host.
    {
        let mut rt = rt(Scene { step: Step::S1, show_continue: true, can_continue: true, ..base() });
        settle(&mut rt);
        let continue_ = maybe(&rt, "onboarding-continue").expect("S1 shows continue");
        assert!(continue_.focusable, "a complete step 1 unlocks Continue");
        press(&mut rt, "onboarding-continue");
        assert_eq!(rt.state.continue_fired, 1, "the unlocked Continue fires the host action");
        assert_eq!(
            label_under(&rt, "onboarding-continue"),
            t("onboarding.common.continue"),
            "the button's label is the pack entry"
        );
    }
}

/// Finish replaces Continue on S2 (`showFinish`), gated by the AI draft's
/// saving state (React's `disabled={busy}`).
#[test]
fn finish_replaces_continue_on_s2() {
    // S2, not busy: Finish shown and enabled, no Continue in sight.
    {
        let mut rt = rt(Scene { step: Step::S2, show_finish: true, ..base() });
        settle(&mut rt);
        let finish = maybe(&rt, "onboarding-finish").expect("S2 shows finish");
        assert!(!has(&rt, "onboarding-continue"), "S2 shows no continue");
        assert!(finish.focusable, "the idle Finish is enabled");
        press(&mut rt, "onboarding-finish");
        assert_eq!(rt.state.finish_fired, 1, "the enabled Finish fires the host action");
    }
    // Busy: disabled, never fires.
    {
        let mut rt = rt(Scene { step: Step::S2, show_finish: true, busy: true, ..base() });
        settle(&mut rt);
        let finish = maybe(&rt, "onboarding-finish").expect("S2 shows finish");
        assert!(!finish.focusable, "the busy Finish is disabled (not focusable)");
        press(&mut rt, "onboarding-finish");
        assert_eq!(rt.state.finish_fired, 0, "the busy Finish never fires");
        assert_eq!(
            label_under(&rt, "onboarding-finish"),
            t("onboarding.common.finish"),
            "the button's label is the pack entry"
        );
    }
}

/// Back and skip fire their host actions at the steps that show them.
#[test]
fn back_and_skip_fire_their_actions() {
    let mut rt = rt(Scene { step: Step::S1, ..base() });
    press(&mut rt, "onboarding-skip");
    assert_eq!(rt.state.skip_fired, 1, "skip fires");
    press(&mut rt, "onboarding-back");
    assert_eq!(rt.state.back_fired, 1, "back fires");
    assert_eq!(
        label_under(&rt, "onboarding-back"),
        t("onboarding.common.back"),
        "the button's label is the pack entry"
    );
}

/// The actions sit at the footer's right padding (`ml-auto`): the spacer
/// between the indicator and the buttons consumes the free width.
#[test]
fn the_actions_push_to_the_right_edge() {
    let mut rt = rt(Scene { step: Step::S1, show_continue: true, ..base() });
    settle(&mut rt);
    let footer = maybe(&rt, "onboarding-footer").expect("the footer root");
    let actions = maybe(&rt, "onboarding-footer-actions").expect("the actions row");
    let spacer = maybe(&rt, "onboarding-footer-spacer").expect("the ml-auto spacer");
    assert!(spacer.rect[2] > 0.0, "the spacer consumes the free width: {:?}", spacer.rect);
    assert!(
        (actions.rect[0] + actions.rect[2] - (footer.rect[0] + footer.rect[2] - 36.0)).abs() < 0.5,
        "the actions end at the footer's 48px right padding (got {})",
        actions.rect[0] + actions.rect[2]
    );
}
