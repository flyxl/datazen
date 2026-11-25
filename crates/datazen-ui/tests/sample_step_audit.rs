//! Independent audit for T226 (the SampleStep Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/steps/SampleStep.tsx` (plus
//! the wizard's `{isStepOne && state.entry === 'sample' && <SampleStep …
//! />}` mount): the `w-full max-w-[640px]` column, the step tag / heading
//! / subtitle rhythm, the one status card and its `ready` chrome branch
//! (`border-green/25 bg-green/8` with the check-circle glyph once ready —
//! via the local status OR the `readyConnectionName` prop — versus the
//! accent chrome with the database glyph on failure and the pulsing flask
//! while preparing), the status label following the LOCAL status alone
//! (the prop's ready-but-still-preparing branch), the path line behind
//! its `title`, the fixed connection line with the `{name}` placeholder,
//! the error line on failure, and the seeding journey itself: the
//! once-per-entry guard and the two promise arms feeding the wizard's
//! sample directive. Runtime evidence comes from `Runtime::elements()`
//! with the step mounted exactly as production mounts it — as the
//! wizard's S1-sample seat inside the T072 window — never from a value
//! the component reports about itself. The glyph bodies themselves are
//! paint-only in the probe (no icon identity in `ElementInfo`); the
//! audits pin the observable halves — one icon child at 18px (13.5pt),
//! its ink, and the pulse loop's `loop_phase`.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::onboarding::onboarding_window::OnboardingWindow;
use datazen_ui::views::onboarding::onboarding_wizard::{
    Entry, OnboardingWizard, Step, SAMPLE_PRESET_SQL,
};
use datazen_ui::views::onboarding::steps::sample_step::{
    SampleStep, SampleStepVm, SampleStatus, SAMPLE_CONNECTION_GROUP, SAMPLE_CONNECTION_NAME,
    STEP_MAX_W,
};

/// The default onboarding window (960×720 — the T021/T072 audits' default).
const W: f32 = 960.0;
const H: f32 = 720.0;

#[derive(Clone)]
struct Scene {
    step: Step,
    entry: Entry,
    dark: bool,
    /// The step-local half of React's state (the seeding journey's VM).
    vm: SampleStepVm,
    /// `readyConnectionName` — the prop the wizard passes only when
    /// `state.samplePath` is already set (step 1 completed once).
    ready_connection_name: Option<String>,
}

fn base() -> Scene {
    Scene {
        step: Step::S1,
        entry: Entry::Sample,
        dark: true,
        // React's mount: `status: 'preparing'`, `samplePath: ''`,
        // `error: null`.
        vm: SampleStepVm::default(),
        ready_connection_name: None,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    // The step the wizard hands its seeding VM, built per frame.
    let step = SampleStep::new(s.vm.clone())
        .ready_connection_name(s.ready_connection_name.clone())
        .build(s.dark);
    // The production mounting: the wizard's S1-sample seat inside the
    // T072 window (the seat itself gates on step+entry, exactly as
    // React's conditional).
    let wizard = OnboardingWizard::<Scene>::new()
        .step(s.step)
        .entry(s.entry)
        .sample(step);
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

/// The first text element under the element keyed `key` (the pill's
/// unkeyed label — the `ai_provider_step_audit` helper).
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

/// The icon element under the element keyed `key` (the well's glyph).
fn icon_under(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    let el = maybe(rt, key)?;
    let prefix = el.path;
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.role == "icon" && e.path.starts_with(&prefix))
}

/// Advance the clock past the kit's enter/hover motion (~0.4s) so freshly
/// mounted parts read their full geometry (the sibling audits' helper).
fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

/// Channel-wise colour equality with a small tolerance (any solving the
/// paint pipeline performs stays inside it).
fn same_color(a: rune::Color, b: rune::Color) -> bool {
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

// ---- the wizard's S1-sample mount ---------------------------------------------

/// React: `{isStepOne && state.entry === 'sample' && <SampleStep … />}` —
/// the step exists only on S1 of the sample entry, inside the wizard's
/// sample seat.
#[test]
fn the_step_renders_only_on_s1_sample() {
    for (step, entry) in [
        (Step::S0, Entry::Sample),
        (Step::S1, Entry::Manual),
        (Step::S1, Entry::Import),
        (Step::S2, Entry::Sample),
        (Step::S3, Entry::Sample),
    ] {
        let rt = rt(Scene { step, entry, ..base() });
        assert!(
            !has(&rt, "onboarding-step-s1-sample"),
            "{step:?}/{entry:?} does not mount the sample step"
        );
        assert!(
            !has(&rt, "onboarding-sample-status"),
            "{step:?}/{entry:?} does not mount the status card either"
        );
    }
    {
        let rt = rt(Scene { ..base() });
        let root = maybe(&rt, "onboarding-step-s1-sample").expect("S1-sample mounts the step");
        let seat = maybe(&rt, "onboarding-wizard-step-s1-sample").expect("the wizard's sample seat");
        assert!(
            root.path.starts_with(&seat.path),
            "the step is mounted inside the wizard's S1-sample seat"
        );
    }
}

// ---- the step column -----------------------------------------------------------

/// React root: `w-full max-w-[640px]` — full width up to the 640px (480pt)
/// cap; the status card fills the capped column.
#[test]
fn the_step_column_caps_at_640px() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let root = maybe(&rt, "onboarding-step-s1-sample").expect("the step root");
        assert!(
            (root.rect[2] - STEP_MAX_W).abs() < 0.5,
            "dark={dark}: the column caps at 640px = {}pt, not {}",
            STEP_MAX_W,
            root.rect[2]
        );
        let card = maybe(&rt, "onboarding-sample-status").expect("the status card");
        assert_eq!(card.rect[0], root.rect[0], "dark={dark}: the card is left-aligned");
        assert!((card.rect[2] - STEP_MAX_W).abs() < 0.5, "dark={dark}: the card fills the column");
        // The column starts at the seat's left edge (React's content row
        // left-aligns its single child).
        let seat = maybe(&rt, "onboarding-wizard-step-s1-sample").expect("the sample seat");
        assert_eq!(root.rect[0], seat.rect[0], "dark={dark}: left-aligned in the seat");
    }
}

/// Nothing renders outside the window horizontally, in either theme, in
/// every state of the journey (the ready state carries the path line, the
/// failed one the error line — each state's tallest content).
#[test]
fn every_part_stays_inside_the_window_horizontally() {
    for dark in [true, false] {
        for state in 0..3 {
            let mut scene = Scene { dark, ..base() };
            match state {
                0 => {} // preparing
                1 => scene.vm.seed_succeeded("/home/tester/.local/share/datazen/sample.db"),
                _ => scene.vm.seed_failed("no space left on device"),
            }
            let mut rt = rt(scene);
            settle(&mut rt);
            let mut outside = Vec::new();
            for element in rt.elements() {
                let Some(key) = &element.key else { continue };
                let [x, _y, w, _h] = element.rect;
                if x < -0.5 || x + w > W + 0.5 {
                    outside.push(format!("{key} [{x}, {w}]"));
                }
            }
            assert!(
                outside.is_empty(),
                "dark={dark}, state={state}: parts leave the window: {outside:?}"
            );
        }
    }
}

// ---- the header ----------------------------------------------------------------

/// The step's own texts are the `onboarding.s1.*` pack entries, in React's
/// rhythm: the tag label, the heading (`mb-2.5` to the subtitle), the
/// subtitle (`mb-7` to the card, capped at 56ch — narrower than the
/// column).
#[test]
fn the_header_reads_the_s1_sample_pack() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let k = kit(dark);
        // `uppercase` renders the label through `str::to_uppercase` (a
        // no-op for the zh-CN value — the column_mapping_editor ruling).
        assert_eq!(
            label_under(&rt, "onboarding-step-tag-pill"),
            t("onboarding.s1.stepLabel").to_uppercase()
        );
        assert_eq!(
            text_of(&rt, "onboarding-step-s1-sample-title"),
            t("onboarding.s1.sampleTitle")
        );
        assert_eq!(
            text_of(&rt, "onboarding-step-s1-sample-subtitle"),
            t("onboarding.s1.sampleSubtitle")
        );
        let tag = maybe(&rt, "onboarding-step-tag").expect("the tag row");
        let title = maybe(&rt, "onboarding-step-s1-sample-title").expect("the heading");
        let subtitle = maybe(&rt, "onboarding-step-s1-sample-subtitle").expect("the subtitle");
        let card = maybe(&rt, "onboarding-sample-status").expect("the status card");
        assert_eq!(title.text_color, Some(k.c.text_base), "dark={dark}: text-fg");
        assert_eq!(subtitle.text_color, Some(k.c.text_muted), "dark={dark}: text-fg-secondary");
        assert!(
            (title.rect[1] - (tag.rect[1] + tag.rect[3]) - 12.0).abs() < 0.5,
            "dark={dark}: the heading starts mb-4 (12pt) below the tag row"
        );
        assert!(
            (subtitle.rect[1] - (title.rect[1] + title.rect[3]) - 7.5).abs() < 0.5,
            "dark={dark}: the subtitle starts mb-2.5 (7.5pt) below the heading"
        );
        assert!(
            (card.rect[1] - (subtitle.rect[1] + subtitle.rect[3]) - 21.0).abs() < 0.5,
            "dark={dark}: the card starts mb-7 (21pt) below the subtitle"
        );
        assert!(
            subtitle.rect[2] < card.rect[2] - 100.0,
            "dark={dark}: the 56ch cap binds well below the 640px column (subtitle {} vs card {})",
            subtitle.rect[2],
            card.rect[2]
        );
    }
}

// ---- the seeding journey's three states ------------------------------------------

/// The status label follows the LOCAL status alone — preparing, then ready
/// with the path line behind its `title`, or failed with the error line —
/// while the description and connection lines ride every state.
#[test]
fn the_status_label_tracks_the_seeding_journey() {
    // Preparing: React's fresh mount.
    {
        let rt = rt(Scene { ..base() });
        assert_eq!(
            text_of(&rt, "onboarding-sample-status-label"),
            t("onboarding.s1.samplePreparing")
        );
        assert!(!has(&rt, "onboarding-sample-path"), "no path line before the journey resolves");
        assert!(!has(&rt, "onboarding-sample-error"), "no error line before the journey fails");
        assert_eq!(
            text_of(&rt, "onboarding-sample-status-desc"),
            t("onboarding.s1.sampleReadyDesc")
        );
        assert_eq!(
            text_of(&rt, "onboarding-sample-connection"),
            t("onboarding.s1.sampleConnection").replace("{name}", SAMPLE_CONNECTION_NAME),
            "the {{name}} placeholder takes the fixed connection identity"
        );
    }
    // The .then arm: the path line appears, carrying the full path in its
    // title (React `title={samplePath}`).
    {
        let mut rt = rt(Scene { ..base() });
        rt.update(|s, _| s.vm.seed_succeeded("/home/tester/.local/share/datazen/sample.db"));
        settle(&mut rt);
        assert_eq!(
            text_of(&rt, "onboarding-sample-status-label"),
            t("onboarding.s1.sampleReady")
        );
        let path = maybe(&rt, "onboarding-sample-path").expect("the seeded path line");
        assert_eq!(path.text.as_deref(), Some("/home/tester/.local/share/datazen/sample.db"));
        assert_eq!(
            path.help.as_deref(),
            Some("/home/tester/.local/share/datazen/sample.db"),
            "title={{samplePath}} — the full path on hover"
        );
        assert_eq!(path.text_color, Some(kit(true).c.text_muted), "text-fg-muted");
    }
    // The .catch arm: the error line appears in the danger ink.
    {
        let mut rt = rt(Scene { ..base() });
        rt.update(|s, _| s.vm.seed_failed("no space left on device"));
        settle(&mut rt);
        assert_eq!(
            text_of(&rt, "onboarding-sample-status-label"),
            t("onboarding.s1.sampleFailed")
        );
        let error = maybe(&rt, "onboarding-sample-error").expect("the error line");
        assert_eq!(error.text.as_deref(), Some("no space left on device"));
        assert_eq!(error.text_color, Some(kit(true).c.danger.fg), "text-red");
        assert!(!has(&rt, "onboarding-sample-path"), "a failed journey shows no path");
    }
}

/// The chrome branch: `ready = status === 'ready' ||
/// Boolean(readyConnectionName)` — the card and the well go green on
/// EITHER, and the label keeps following the local status alone (the
/// prop's ready-but-still-preparing branch: green chrome, preparing
/// label, no path line).
#[test]
fn the_ready_chrome_gates_on_the_local_status_or_the_prop() {
    // Preparing, no prop: the plain edge/surface chrome.
    {
        let mut rt = rt(Scene { ..base() });
        settle(&mut rt);
        let k = kit(true);
        let card = maybe(&rt, "onboarding-sample-status").expect("the status card");
        let well = maybe(&rt, "onboarding-sample-status-icon").expect("the icon well");
        assert!(
            same_color(card.fill, k.c.surface_base),
            "bg-surface (got {:?}, want {:?})",
            card.fill,
            k.c.surface_base
        );
        assert!(
            (well.fill.a - 0.12).abs() < 0.02,
            "the preparing well is bg-accent/12 (got alpha {})",
            well.fill.a
        );
        assert!(
            !same_color(well.fill, k.c.success.fg.alpha(0.12)),
            "the preparing well is the accent tint, not the green one"
        );
    }
    // Ready via the local status: `border-green/25 bg-green/8` on the
    // card, `bg-green/12` in the well.
    {
        let mut rt = rt(Scene { ..base() });
        rt.update(|s, _| s.vm.seed_succeeded("/home/tester/.local/share/datazen/sample.db"));
        settle(&mut rt);
        let k = kit(true);
        let card = maybe(&rt, "onboarding-sample-status").expect("the status card");
        let well = maybe(&rt, "onboarding-sample-status-icon").expect("the icon well");
        assert!(
            same_color(card.fill, k.c.success.fg.alpha(0.08)),
            "bg-green/8 (got {:?}, want {:?})",
            card.fill,
            k.c.success.fg.alpha(0.08)
        );
        assert!(
            same_color(well.fill, k.c.success.fg.alpha(0.12)),
            "bg-green/12 (got {:?}, want {:?})",
            well.fill,
            k.c.success.fg.alpha(0.12)
        );
    }
    // Ready via the prop ALONE (re-entering step 1 after a completed
    // seeding): the green chrome shows, but the label still reads the
    // preparing entry and no path line exists — React's exact split.
    {
        let mut rt = rt(Scene {
            ready_connection_name: Some(SAMPLE_CONNECTION_NAME.to_owned()),
            ..base()
        });
        settle(&mut rt);
        let k = kit(true);
        let card = maybe(&rt, "onboarding-sample-status").expect("the status card");
        assert!(
            same_color(card.fill, k.c.success.fg.alpha(0.08)),
            "the prop alone greens the card (got {:?})",
            card.fill
        );
        assert_eq!(
            text_of(&rt, "onboarding-sample-status-label"),
            t("onboarding.s1.samplePreparing"),
            "the label keeps following the local status alone"
        );
        assert!(!has(&rt, "onboarding-sample-path"), "the fresh mount has no path yet");
    }
}

/// The icon well: the 36px (27pt) `rounded-lg` square leading the text
/// column, with exactly one 18px (13.5pt) glyph whose pulse loop runs
/// only while preparing (`animate-pulse` rides the flask alone).
#[test]
fn the_well_swaps_its_glyph_and_pulses_only_while_preparing() {
    for dark in [true, false] {
        // Preparing: the flask pulses.
        {
            let mut rt = rt(Scene { dark, ..base() });
            settle(&mut rt);
            let well = maybe(&rt, "onboarding-sample-status-icon").expect("the icon well");
            let text = maybe(&rt, "onboarding-sample-status-text").expect("the text column");
            assert!(
                (well.rect[2] - 27.0).abs() < 0.5 && (well.rect[3] - 27.0).abs() < 0.5,
                "dark={dark}: the well is h-9 w-9 (27pt), got {}×{}",
                well.rect[2],
                well.rect[3]
            );
            assert!(
                (well.radius - 6.0).abs() < 0.5,
                "dark={dark}: the well is rounded-lg (6pt), got {}",
                well.radius
            );
            assert!(well.rect[0] < text.rect[0], "dark={dark}: the well leads the text column");
            let glyph = icon_under(&rt, "onboarding-sample-status-icon").expect("the well's glyph");
            assert!(
                (glyph.rect[2] - 13.5).abs() < 0.5 && (glyph.rect[3] - 13.5).abs() < 0.5,
                "dark={dark}: the glyph is h-[18px] w-[18px] (13.5pt), got {}×{}",
                glyph.rect[2],
                glyph.rect[3]
            );
            assert!(glyph.loop_phase.is_some(), "dark={dark}: the preparing flask pulses");
        }

        // Ready: the static check-circle (no loop, green ink).
        {
            let mut rt = rt(Scene { dark, ..base() });
            rt.update(|s, _| s.vm.seed_succeeded("/home/tester/.local/share/datazen/sample.db"));
            settle(&mut rt);
            let glyph = icon_under(&rt, "onboarding-sample-status-icon").expect("the ready glyph");
            assert_eq!(glyph.loop_phase, None, "dark={dark}: the ready glyph is static");
            assert_eq!(glyph.text_color, Some(kit(dark).c.success.fg), "dark={dark}: text-green");
        }

        // Failed: the static database glyph in the accent ink.
        {
            let mut rt = rt(Scene { dark, ..base() });
            rt.update(|s, _| s.vm.seed_failed("no space left on device"));
            settle(&mut rt);
            let glyph = icon_under(&rt, "onboarding-sample-status-icon").expect("the failed glyph");
            assert_eq!(glyph.loop_phase, None, "dark={dark}: the failed glyph is static");
            assert_eq!(glyph.text_color, Some(kit(dark).c.text_accent), "dark={dark}: text-accent");
        }
    }
}

// ---- the seeding journey's guard and the wizard link ------------------------------

/// React's `seedingRef` guard: the seeding work runs exactly once per
/// entry into the step — a re-built step re-observes the one run instead
/// of re-running it (the StrictMode remount rule).
#[test]
fn the_seeding_runs_once_per_entry() {
    let mut vm = SampleStepVm::default();
    assert_eq!(vm.status, SampleStatus::Preparing, "React's initial status");
    assert!(vm.begin_seeding(), "the first call starts the seeding");
    assert!(!vm.begin_seeding(), "a re-mount does not re-run it");
    assert!(!vm.begin_seeding(), "nor does any later frame");
    // The two arms set exactly their own halves.
    vm.seed_failed("locked");
    assert_eq!(vm.status, SampleStatus::Failed);
    assert_eq!(vm.error.as_deref(), Some("locked"));
    assert!(vm.sample_path.is_empty(), "a failure leaves the path empty");
    let mut fresh = SampleStepVm::default();
    fresh.begin_seeding();
    fresh.seed_succeeded("/tmp/sample.db");
    assert_eq!(fresh.status, SampleStatus::Ready);
    assert_eq!(fresh.sample_path, "/tmp/sample.db");
    assert_eq!(fresh.error, None, "a success leaves no error");
}

/// The seeded connection's fixed identity (the constants the host reads
/// for its SAMPLE_READY dispatch and the connection it must reuse rather
/// than duplicate), and the journey link: a succeeded seeding feeds the
/// wizard's sample directive — the `SAMPLE_PRESET_SQL` T114 already owns
/// — while a non-sample entry stashes nothing.
#[test]
fn the_fixed_identity_feeds_the_wizards_sample_directive() {
    assert_eq!(SAMPLE_CONNECTION_NAME, "Sample Playground");
    assert_eq!(SAMPLE_CONNECTION_GROUP, "Sample");
    let wizard = OnboardingWizard::<Scene>::new()
        .entry(Entry::Sample)
        .connection_name(SAMPLE_CONNECTION_NAME);
    let directive = wizard.post_onboarding_sample().expect("the sample directive");
    assert_eq!(directive.connection_name, SAMPLE_CONNECTION_NAME);
    assert_eq!(directive.sql, SAMPLE_PRESET_SQL, "T114's preset, reused not duplicated");
    let other = OnboardingWizard::<Scene>::new()
        .entry(Entry::Manual)
        .connection_name(SAMPLE_CONNECTION_NAME);
    assert!(
        other.post_onboarding_sample().is_none(),
        "only the sample entry stashes the directive"
    );
}
