//! Independent audit for T114 (the OnboardingWizard Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/OnboardingWizard.tsx` plus its
//! test (`__tests__/OnboardingWizard.test.tsx`): exactly one step branch
//! renders per `(step, entry)` (S0 welcome → S1 by entry → S2 AI → S3
//! done), the content area's `p-[40px_48px_32px]` padding, the footer
//! mounted at every step except S3 with the shared 56px chrome height
//! ending on the same bottom line as the sidebar's version row, the footer
//! wiring React computes (`continueVisible` = step 1 of a non-manual entry,
//! `showFinish` = s2, the fed-in `canContinue`/`busy`), `handleFinish`'s
//! credentials branch, `handleOpen`'s sample-database directive gate, the
//! `detected` resync effect, and the `visibleStepNumber` helper. Runtime
//! evidence comes from `Runtime::elements()` (runtime layout) with the
//! wizard mounted exactly as production mounts it — as the T072 window's
//! wizard slot — never from a value the component reports about itself;
//! the wiring/directive asserts are the file's own pure logic, probed
//! directly like the T072 completion-action audit.

use rune::{Cx, El, Env, Runtime};

use crate::views::onboarding::brand_sidebar::ASIDE_WIDTH;
use crate::views::onboarding::onboarding_layout::WIZARD_FOOTER_HEIGHT;
use crate::views::onboarding::onboarding_window::OnboardingWindow;
use crate::views::onboarding::onboarding_wizard::{
    FinishPath, ImportApp, ImportSource, OnboardingWizard, PostOnboardingSampleVm, SAMPLE_PRESET_SQL,
    Step, Entry, visible_step_number,
};

/// The default onboarding window (960×720 — the T021/T072 audits' default).
const W: f32 = 960.0;
const H: f32 = 720.0;

#[derive(Clone)]
struct Scene {
    step: Step,
    entry: Entry,
    can_continue: bool,
    connection_name: Option<&'static str>,
    has_credentials: bool,
    busy: bool,
    /// Feed keyed probe `El`s into every slot, as the step/footer tasks will.
    feed: bool,
    dark: bool,
    skip_fired: bool,
    back_fired: bool,
    continue_fired: bool,
    finish_fired: bool,
    open_fired: bool,
}

fn base() -> Scene {
    Scene {
        step: Step::S0,
        entry: Entry::Manual,
        can_continue: false,
        connection_name: None,
        has_credentials: false,
        busy: false,
        feed: false,
        dark: true,
        skip_fired: false,
        back_fired: false,
        continue_fired: false,
        finish_fired: false,
        open_fired: false,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut wizard = OnboardingWizard::<Scene>::new()
        .step(s.step)
        .entry(s.entry)
        .can_continue(s.can_continue)
        .has_credentials(s.has_credentials)
        .busy(s.busy)
        .on_skip(|s: &mut Scene| s.skip_fired = true)
        .on_back(|s: &mut Scene| s.back_fired = true)
        .on_continue(|s: &mut Scene| s.continue_fired = true)
        .on_finish(|s: &mut Scene| s.finish_fired = true)
        .on_open(|s: &mut Scene| s.open_fired = true);
    if let Some(name) = s.connection_name {
        wizard = wizard.connection_name(name);
    }
    if s.feed {
        // Stand-ins for the step/footer ports: keyed probes of a known
        // size, fed exactly as the later tasks will feed their builds.
        wizard = wizard
            .welcome(rune::col().key("probe-welcome").w(200.0).h(80.0))
            .manual(rune::col().key("probe-manual").w(200.0).h(80.0))
            .import(rune::col().key("probe-import").w(200.0).h(80.0))
            .sample(rune::col().key("probe-sample").w(200.0).h(80.0))
            .ai(rune::col().key("probe-ai").w(200.0).h(80.0))
            .done(rune::col().key("probe-done").w(200.0).h(80.0))
            .footer(rune::col().key("probe-footer").w_full().h(WIZARD_FOOTER_HEIGHT));
    }
    // The production mounting: the T072 window's wizard slot.
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

// ---- the step list (runtime evidence) ---------------------------------------

/// React renders exactly one step branch per `(step, entry)` — the other
/// seats must not exist.
#[test]
fn exactly_one_step_seat_renders_per_step_and_entry() {
    let cases = [
        (Step::S0, Entry::Manual, "onboarding-wizard-step-s0"),
        (Step::S0, Entry::Import, "onboarding-wizard-step-s0"),
        (Step::S1, Entry::Manual, "onboarding-wizard-step-s1-manual"),
        (Step::S1, Entry::Import, "onboarding-wizard-step-s1-import"),
        (Step::S1, Entry::Sample, "onboarding-wizard-step-s1-sample"),
        (Step::S2, Entry::Manual, "onboarding-wizard-step-s2"),
        (Step::S2, Entry::Import, "onboarding-wizard-step-s2"),
        (Step::S3, Entry::Sample, "onboarding-wizard-step-s3"),
    ];
    let all = [
        "onboarding-wizard-step-s0",
        "onboarding-wizard-step-s1-manual",
        "onboarding-wizard-step-s1-import",
        "onboarding-wizard-step-s1-sample",
        "onboarding-wizard-step-s2",
        "onboarding-wizard-step-s3",
    ];
    for (step, entry, expected) in cases {
        let rt = rt(Scene { step, entry, ..base() });
        for key in all {
            let present = has(&rt, key);
            assert_eq!(
                present,
                key == expected,
                "{step:?}/{entry:?}: {key} {}",
                if present { "should not render" } else { "is missing" }
            );
        }
    }
}

/// A fed step lands inside its seat, under the content area's padding
/// (`p-[40px_48px_32px]` → 30/36/24pt), and the seat fills the padded
/// width.
#[test]
fn the_fed_step_lands_inside_its_seat_under_the_content_padding() {
    let rt = rt(Scene { step: Step::S1, entry: Entry::Import, feed: true, ..base() });
    let content = maybe(&rt, "onboarding-wizard-content").expect("the content area is keyed");
    let [cx, cy, cw, _] = content.rect;
    let seat = maybe(&rt, "onboarding-wizard-step-s1-import").expect("the import seat is keyed");
    let [sx, sy, sw, _] = seat.rect;
    assert_eq!((sy - cy, sx - cx), (30.0, 36.0), "pt-[40px] / px-[48px] = 30/36pt");
    assert_eq!(sw, cw - 72.0, "the seat fills the width between the 48px paddings");
    let probe = maybe(&rt, "probe-import").expect("the fed step is mounted in its seat");
    let [px, py, pw, ph] = probe.rect;
    assert_eq!((px, py), (sx, sy), "the fed step starts at the seat's top-left");
    assert_eq!((pw, ph), (200.0, 80.0), "the fed step keeps its own size");
}

/// The wizard body fills the window's `<main>` column — the T072 one-`El`
/// seam this port lands into.
#[test]
fn the_wizard_body_fills_the_windows_main_column() {
    for dark in [true, false] {
        let rt = rt(Scene { step: Step::S2, dark, ..base() });
        let main = maybe(&rt, "onboarding-window-main").expect("the <main> column is keyed");
        let body = maybe(&rt, "onboarding-wizard-body")
            .unwrap_or_else(|| panic!("the body is keyed (dark={dark})"));
        assert_eq!(body.rect, main.rect, "the body fills the main column (dark={dark})");
        assert_eq!(main.rect, [ASIDE_WIDTH, 0.0, W - ASIDE_WIDTH, H]);
    }
}

/// The footer seat: the shared 56px chrome height, ending on the window's
/// bottom line — the same line the sidebar's version row ends on (the
/// React test's alignment assertion) — with the content area ending where
/// it begins and the fed footer mounted inside.
#[test]
fn the_footer_seat_shares_the_chrome_height_and_ends_on_the_bottom_line() {
    let rt = rt(Scene { step: Step::S1, entry: Entry::Import, feed: true, ..base() });
    let footer = maybe(&rt, "onboarding-wizard-footer").expect("the footer seat is keyed");
    let [fx, fy, fw, fh] = footer.rect;
    assert_eq!(fh, WIZARD_FOOTER_HEIGHT, "React h-[56px] = 42pt");
    assert_eq!(fy + fh, H, "ends on the window's bottom line");
    assert_eq!((fx, fw), (ASIDE_WIDTH, W - ASIDE_WIDTH), "fills the main column");
    // The sidebar's version row ends on the same bottom line (T287's rule).
    let sidebar_foot = maybe(&rt, "onboarding-sidebar-foot").expect("the T021 version row");
    assert_eq!(sidebar_foot.rect[1] + sidebar_foot.rect[3], H);
    // The content area ends exactly where the footer begins.
    let content = maybe(&rt, "onboarding-wizard-content").expect("the content area is keyed");
    assert_eq!(content.rect[1] + content.rect[3], fy);
    assert!(has(&rt, "probe-footer"), "the fed footer is mounted in its seat");
}

/// React: `{state.step !== 's3' && <WizardFooter … />}` — the done screen
/// has no footer, and the content area takes the full height.
#[test]
fn the_done_screen_has_no_footer() {
    let rt = rt(Scene { step: Step::S3, feed: true, ..base() });
    assert!(
        !has(&rt, "onboarding-wizard-footer"),
        "the footer is not rendered on the done screen"
    );
    assert!(!has(&rt, "probe-footer"), "the fed footer stays unmounted too");
    let content = maybe(&rt, "onboarding-wizard-content").expect("the content area is keyed");
    assert_eq!(content.rect[1] + content.rect[3], H, "the content takes the full height");
}

#[test]
fn every_part_stays_inside_the_window() {
    for dark in [true, false] {
        let rt = rt(Scene { step: Step::S1, entry: Entry::Sample, feed: true, dark, ..base() });
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

// ---- the footer wiring (the file's own computation) --------------------------

/// React's prop computation: `continueVisible = isStepOne && entry !==
/// 'manual'`, `showFinish = step === 's2'`, with the fed-in `canContinue`
/// and `busy` passing through untouched.
#[test]
fn the_footer_wiring_is_reacts_prop_list() {
    let cases = [
        // (step, entry, show_continue, show_finish)
        (Step::S0, Entry::Manual, false, false),
        (Step::S0, Entry::Import, false, false),
        (Step::S1, Entry::Manual, false, false), // manual: the form's Save is the way forward
        (Step::S1, Entry::Import, true, false),
        (Step::S1, Entry::Sample, true, false),
        (Step::S2, Entry::Import, false, true),
        (Step::S3, Entry::Sample, false, false),
    ];
    for (step, entry, show_continue, show_finish) in cases {
        let wiring = OnboardingWizard::<Scene>::new()
            .step(step)
            .entry(entry)
            .can_continue(true)
            .busy(true)
            .footer_wiring();
        assert_eq!(wiring.step, step, "{step:?}/{entry:?}");
        assert!(wiring.can_continue && wiring.busy, "the feed-ins pass through");
        assert_eq!(wiring.show_continue, show_continue, "showContinue {step:?}/{entry:?}");
        assert_eq!(wiring.show_finish, show_finish, "showFinish {step:?}/{entry:?}");
    }
}

/// React's start: `INITIAL_STATE` is s0 + manual, so nothing is continuable
/// and no dispatch is wired.
#[test]
fn the_default_wiring_matches_initial_state() {
    let wiring = OnboardingWizard::<Scene>::new().footer_wiring();
    assert_eq!(wiring.step, Step::S0);
    assert!(
        !wiring.show_continue && !wiring.show_finish && !wiring.can_continue && !wiring.busy
    );
    assert!(
        wiring.on_skip.is_none()
            && wiring.on_back.is_none()
            && wiring.on_continue.is_none()
            && wiring.on_finish.is_none(),
        "no dispatch wired at mount"
    );
}

// ---- the completion handlers (the file's own logic) --------------------------

/// React `handleFinish`: nothing typed → `FINISH { aiLabel: null }` (the
/// journey never forces a key); credentials typed → `requestSave`.
#[test]
fn handle_finish_branches_on_credentials() {
    let unconfigured = OnboardingWizard::<Scene>::new().has_credentials(false);
    assert_eq!(unconfigured.finish_path(), FinishPath::FinishUnconfigured);
    let configured = OnboardingWizard::<Scene>::new().has_credentials(true);
    assert_eq!(configured.finish_path(), FinishPath::RequestSave);
}

/// React `handleOpen`'s stash gate: `entry === 'sample' && connectionName`.
#[test]
fn the_sample_open_directive_gates_on_entry_and_connection() {
    let sample = OnboardingWizard::<Scene>::new()
        .entry(Entry::Sample)
        .connection_name("Sample Playground");
    assert_eq!(
        sample.post_onboarding_sample(),
        Some(PostOnboardingSampleVm {
            connection_name: "Sample Playground".to_owned(),
            sql: SAMPLE_PRESET_SQL.to_owned(),
        })
    );
    // No seeded connection → no directive.
    assert_eq!(
        OnboardingWizard::<Scene>::new().entry(Entry::Sample).post_onboarding_sample(),
        None
    );
    // Other entries never stash, even with a connection name.
    assert_eq!(
        OnboardingWizard::<Scene>::new()
            .entry(Entry::Import)
            .connection_name("x")
            .post_onboarding_sample(),
        None
    );
    assert_eq!(
        OnboardingWizard::<Scene>::new()
            .entry(Entry::Manual)
            .connection_name("x")
            .post_onboarding_sample(),
        None
    );
}

/// The preset SQL is React's literal, unchanged.
#[test]
fn the_preset_sql_is_reacts_literal() {
    assert_eq!(
        SAMPLE_PRESET_SQL,
        "SELECT region, SUM(amount) AS total\nFROM demo_sales\nGROUP BY region\nORDER BY total DESC;"
    );
}

/// React's `useState('file')` + `useEffect(() => { if (detected)
/// setImportSource(detected); }, [detected])`: a detected client switches
/// the source; `None` never resets it; a later hit switches again.
#[test]
fn the_detected_source_resync_ports_the_effect() {
    let mut wizard = OnboardingWizard::<Scene>::new();
    assert_eq!(wizard.import_source(), ImportSource::File, "useState('file')");
    wizard.resync_detected(None); // nothing detected: the effect does not fire
    assert_eq!(wizard.import_source(), ImportSource::File);
    wizard.resync_detected(Some(ImportApp::Dbeaver)); // the first detected client wins
    assert_eq!(wizard.import_source(), ImportSource::App(ImportApp::Dbeaver));
    wizard.resync_detected(None); // a later empty detection never resets
    assert_eq!(wizard.import_source(), ImportSource::App(ImportApp::Dbeaver));
    wizard.resync_detected(Some(ImportApp::Navicat)); // a later hit switches, as the effect does
    assert_eq!(wizard.import_source(), ImportSource::App(ImportApp::Navicat));
}

/// React `visibleStepNumber`: no number on the entry and done screens.
#[test]
fn visible_step_number_matches_the_react_helper() {
    assert_eq!(visible_step_number(Step::S0), None);
    assert_eq!(visible_step_number(Step::S1), Some(1));
    assert_eq!(visible_step_number(Step::S2), Some(2));
    assert_eq!(visible_step_number(Step::S3), None);
}

// ---- the action slots --------------------------------------------------------

/// The five action slots fire through to the host's dispatch wiring and the
/// wizard still builds with them in place (the T072 completion-action
/// audit's pattern).
#[test]
fn the_action_slots_fire_through_to_the_host() {
    let wizard = OnboardingWizard::<Scene>::new()
        .on_skip(|s: &mut Scene| s.skip_fired = true)
        .on_back(|s: &mut Scene| s.back_fired = true)
        .on_continue(|s: &mut Scene| s.continue_fired = true)
        .on_finish(|s: &mut Scene| s.finish_fired = true)
        .on_open(|s: &mut Scene| s.open_fired = true);
    let wiring = wizard.footer_wiring();
    let open_action = wizard.open_action().expect("the open action is retained");
    let mut probe = base();
    (wiring.on_skip.clone().expect("onSkip wired"))(&mut probe);
    (wiring.on_back.clone().expect("onBack wired"))(&mut probe);
    (wiring.on_continue.clone().expect("onContinue wired"))(&mut probe);
    (wiring.on_finish.clone().expect("onFinish wired"))(&mut probe);
    open_action(&mut probe);
    assert!(
        probe.skip_fired
            && probe.back_fired
            && probe.continue_fired
            && probe.finish_fired
            && probe.open_fired,
        "every delivered action is the one supplied"
    );
    // ... and the wizard still builds with the slots in place.
    let _ = wizard.build();
}

#[test]
fn without_an_open_action_the_done_seam_stays_empty() {
    let wizard = OnboardingWizard::<Scene>::new();
    assert!(wizard.open_action().is_none());
    let _ = wizard.build();
}
