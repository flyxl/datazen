//! Independent audit for T247 (the WelcomeStep Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/steps/WelcomeStep.tsx` (plus
//! the wizard's `{state.step === 's0' && <WelcomeStep … />}` mount and
//! `__tests__/OnboardingWizard.test.tsx`): the `w-full max-w-[640px]`
//! column, the heading/subtitle i18n and rhythm, the three stacked entry
//! cards in React's order (import / manual / sample) with their accent
//! icon wells, the FASTEST pill on the import card alone, the
//! detected-client sub-line behind its `{app}` placeholder, and the three
//! entry actions. Runtime evidence comes from `Runtime::elements()`
//! (runtime layout) with the step mounted exactly as production mounts it
//! — as the wizard's S0 seat inside the T072 window — never from a value
//! the component reports about itself. The cards' `selectedEntry` chrome
//! (the import card's accent border/ring alphas) and the hover fills are
//! paint-only in Rune's probe (no border/shadow in `ElementInfo`), so the
//! selection asserts pin what stays observable: the same tree, the same
//! actions, in both selection states.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::onboarding::onboarding_window::OnboardingWindow;
use datazen_ui::views::onboarding::onboarding_wizard::{Entry, ImportApp, OnboardingWizard, Step};
use datazen_ui::views::onboarding::steps::welcome_step::{WelcomeStep, STEP_MAX_W};

/// The default onboarding window (960×720 — the T021/T072 audits' default).
const W: f32 = 960.0;
const H: f32 = 720.0;

#[derive(Clone)]
struct Scene {
    step: Step,
    detected_app: Option<ImportApp>,
    selected_entry: Entry,
    dark: bool,
    import_fired: usize,
    manual_fired: usize,
    sample_fired: usize,
}

fn base() -> Scene {
    Scene {
        step: Step::S0,
        detected_app: None,
        // React's fresh journey: `INITIAL_STATE.entry === 'manual'`.
        selected_entry: Entry::Manual,
        dark: true,
        import_fired: 0,
        manual_fired: 0,
        sample_fired: 0,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    // The step the wizard hands its entry dispatches, built per frame.
    let mut step = WelcomeStep::<Scene>::new(s.selected_entry)
        .on_import(|s: &mut Scene| s.import_fired += 1)
        .on_manual(|s: &mut Scene| s.manual_fired += 1)
        .on_sample(|s: &mut Scene| s.sample_fired += 1);
    if let Some(app) = s.detected_app {
        step = step.detected_app(app);
    }
    // The production mounting: the wizard's S0 seat inside the T072 window
    // (the seat itself gates on the step, exactly as React's conditional).
    let wizard = OnboardingWizard::<Scene>::new().step(s.step).welcome(step.build(s.dark));
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

// ---- the wizard's S0 mount -------------------------------------------------------

/// React: `{state.step === 's0' && <WelcomeStep … />}` — the step exists
/// only on S0, inside the wizard's S0 seat.
#[test]
fn the_step_renders_only_on_s0() {
    for step in [Step::S1, Step::S2, Step::S3] {
        let rt = rt(Scene { step, ..base() });
        assert!(!has(&rt, "onboarding-step-s0"), "{step:?} does not mount the welcome step");
        assert!(
            !has(&rt, "onboarding-entry-import"),
            "{step:?} does not mount the entry cards either"
        );
    }
    {
        let rt = rt(Scene { ..base() });
        let root = maybe(&rt, "onboarding-step-s0").expect("S0 mounts the welcome step");
        let seat = maybe(&rt, "onboarding-wizard-step-s0").expect("the wizard's S0 seat");
        assert!(
            root.path.starts_with(&seat.path),
            "the step is mounted inside the wizard's S0 seat"
        );
    }
}

// ---- the step column -------------------------------------------------------------

/// React root: `w-full max-w-[640px]` — full width up to the 640px (480pt)
/// cap; the cards fill the capped column, the header starts at its left.
#[test]
fn the_step_column_caps_at_640px() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let root = maybe(&rt, "onboarding-step-s0").expect("the step root");
        assert!(
            (root.rect[2] - STEP_MAX_W).abs() < 0.5,
            "dark={dark}: the column caps at 640px = {}pt, not {}",
            STEP_MAX_W,
            root.rect[2]
        );
        let cards = maybe(&rt, "onboarding-step-s0-cards").expect("the cards column");
        let import = maybe(&rt, "onboarding-entry-import").expect("the import card");
        let sample = maybe(&rt, "onboarding-entry-sample").expect("the sample card");
        assert_eq!(cards.rect[0], root.rect[0], "dark={dark}: the cards are left-aligned");
        assert!((import.rect[2] - STEP_MAX_W).abs() < 0.5, "dark={dark}: the import card fills the column");
        assert!((sample.rect[2] - STEP_MAX_W).abs() < 0.5, "dark={dark}: the sample card fills the column");
        // The column starts at the seat's left edge (React's content row
        // left-aligns its single child).
        let seat = maybe(&rt, "onboarding-wizard-step-s0").expect("the S0 seat");
        assert_eq!(root.rect[0], seat.rect[0], "dark={dark}: left-aligned in the seat");
    }
}

/// Nothing renders outside the window horizontally, in either theme.
#[test]
fn every_part_stays_inside_the_window_horizontally() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, detected_app: Some(ImportApp::Dbeaver), ..base() });
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

// ---- the header ------------------------------------------------------------------

/// The heading and the subtitle are the `onboarding.s0.*` pack entries, in
/// React's rhythm: `mb-2.5` (7.5pt) between them, `mb-7` (21pt) to the
/// cards, the subtitle capped at 56ch — narrower than the column.
#[test]
fn the_header_reads_the_s0_pack() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let k = kit(dark);
        let title = maybe(&rt, "onboarding-step-s0-title").expect("the heading");
        let subtitle = maybe(&rt, "onboarding-step-s0-subtitle").expect("the subtitle");
        let cards = maybe(&rt, "onboarding-step-s0-cards").expect("the cards column");
        assert_eq!(text_of(&rt, "onboarding-step-s0-title"), t("onboarding.s0.title"));
        assert_eq!(text_of(&rt, "onboarding-step-s0-subtitle"), t("onboarding.s0.subtitle"));
        assert_eq!(title.text_color, Some(k.c.text_base), "dark={dark}: text-fg");
        assert_eq!(subtitle.text_color, Some(k.c.text_muted), "dark={dark}: text-fg-secondary");
        assert!(
            (subtitle.rect[1] - (title.rect[1] + title.rect[3]) - 7.5).abs() < 0.5,
            "dark={dark}: the subtitle starts mb-2.5 (7.5pt) below the heading"
        );
        assert!(
            (cards.rect[1] - (subtitle.rect[1] + subtitle.rect[3]) - 21.0).abs() < 0.5,
            "dark={dark}: the cards start mb-7 (21pt) below the subtitle"
        );
        assert!(
            subtitle.rect[2] < cards.rect[2] - 100.0,
            "dark={dark}: the 56ch cap binds well below the 640px column (subtitle {} vs cards {})",
            subtitle.rect[2],
            cards.rect[2]
        );
    }
}

// ---- the three cards ---------------------------------------------------------------

/// The three cards stack in React's order — import, manual, sample —
/// `gap-3` (12pt) apart, each with its own pack title and description.
#[test]
fn the_three_cards_stack_in_react_order() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let cards = [
        ("onboarding-entry-import", "onboarding.s0.importCard", "onboarding.s0.importCardDesc"),
        ("onboarding-entry-manual", "onboarding.s0.manualCard", "onboarding.s0.manualCardDesc"),
        ("onboarding-entry-sample", "onboarding.s0.sampleCard", "onboarding.s0.sampleCardDesc"),
    ];
    let mut previous_bottom: Option<f32> = None;
    for (testid, title_key, desc_key) in cards {
        let card = maybe(&rt, testid).unwrap_or_else(|| panic!("{testid} is laid out"));
        if let Some(bottom) = previous_bottom {
            assert!(
                (card.rect[1] - bottom - 12.0).abs() < 0.5,
                "{testid} starts gap-3 (12pt) below the card above (got {} - {})",
                card.rect[1],
                bottom
            );
        }
        assert_eq!(text_of(&rt, &format!("{testid}-title")), t(title_key), "{testid}'s title");
        assert_eq!(text_of(&rt, &format!("{testid}-desc")), t(desc_key), "{testid}'s description");
        previous_bottom = Some(card.rect[1] + card.rect[3]);
    }
    // The import card's title is the semibold `text-fg` ink.
    let title = maybe(&rt, "onboarding-entry-import-title").expect("the import title");
    assert_eq!(title.text_color, Some(kit(true).c.text_base), "text-fg");
}

/// Every card's icon well: the 40px (30pt) accent square — `rounded-[11px]`,
/// `bg-accent/12` — leading the text column.
#[test]
fn the_icon_wells_lead_the_cards() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        for testid in ["onboarding-entry-import", "onboarding-entry-manual", "onboarding-entry-sample"] {
            let well = maybe(&rt, &format!("{testid}-well")).unwrap_or_else(|| panic!("{testid}'s well"));
            let text = maybe(&rt, &format!("{testid}-text")).unwrap_or_else(|| panic!("{testid}'s text column"));
            assert!(
                (well.rect[2] - 30.0).abs() < 0.5 && (well.rect[3] - 30.0).abs() < 0.5,
                "dark={dark}: {testid}'s well is h-10 w-10 (30pt), got {}×{}",
                well.rect[2],
                well.rect[3]
            );
            assert!(
                (well.radius - 8.25).abs() < 0.5,
                "dark={dark}: {testid}'s well is rounded-[11px] (8.25pt), got {}",
                well.radius
            );
            assert!(
                (well.fill.a - 0.12).abs() < 0.02,
                "dark={dark}: {testid}'s well is bg-accent/12 (got alpha {})",
                well.fill.a
            );
            assert!(well.rect[0] < text.rect[0], "dark={dark}: {testid}'s well leads its text");
        }
    }
}

/// The FASTEST pill rides the import card alone: `rounded-full bg-accent`
/// with the uppercase pack text in the on-accent ink, between the text
/// column and the trailing arrow.
#[test]
fn the_fastest_pill_rides_only_the_import_card() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let pill = maybe(&rt, "onboarding-entry-import-badge").expect("the import card's FASTEST pill");
        assert!(!has(&rt, "onboarding-entry-manual-badge"), "dark={dark}: the manual card has no pill");
        assert!(!has(&rt, "onboarding-entry-sample-badge"), "dark={dark}: the sample card has no pill");
        assert_eq!(
            label_under(&rt, "onboarding-entry-import-badge"),
            t("onboarding.s0.importFastest").to_uppercase(),
            "dark={dark}: the pill's uppercase pack text"
        );
        assert!(pill.radius >= 8.0, "dark={dark}: rounded-full (a pill)");
        let k = kit(dark);
        assert!(
            (pill.fill.r - k.c.bg_accent.r).abs() < 0.02
                && (pill.fill.g - k.c.bg_accent.g).abs() < 0.02
                && (pill.fill.b - k.c.bg_accent.b).abs() < 0.02
                && (pill.fill.a - k.c.bg_accent.a).abs() < 0.02,
            "dark={dark}: bg-accent (got {:?}, want {:?})",
            pill.fill,
            k.c.bg_accent
        );
        // The pill sits between the text column and the arrow.
        let text = maybe(&rt, "onboarding-entry-import-text").expect("the text column");
        let arrow = maybe(&rt, "onboarding-entry-import-arrow").expect("the trailing arrow");
        assert!(
            pill.rect[0] > text.rect[0] + text.rect[2] - 1.0 && pill.rect[0] < arrow.rect[0],
            "dark={dark}: the pill rides between the text column and the arrow"
        );
    }
}

/// The detected-client sub-line: absent until a client config is found,
/// then the `importDetectedApp` pack entry with the client's own display
/// name in the `{app}` placeholder, under the description in the green
/// mono ink.
#[test]
fn the_detected_line_reports_the_found_client() {
    // Nothing detected (or detection not finished): no third line.
    {
        let rt = rt(Scene { ..base() });
        assert!(!has(&rt, "onboarding-detected-app"), "no detected line without a client");
        // The manual and sample cards never carry it either.
        assert!(!has(&rt, "onboarding-entry-manual-detected"), "only the import card can");
    }
    // A DBeaver config on this machine.
    {
        let mut rt = rt(Scene { detected_app: Some(ImportApp::Dbeaver), ..base() });
        settle(&mut rt);
        let detected = maybe(&rt, "onboarding-detected-app").expect("the detected line");
        assert_eq!(
            text_of(&rt, "onboarding-detected-app"),
            t("onboarding.s0.importDetectedApp").replace("{app}", "DBeaver"),
            "the {{app}} placeholder takes the client's display name"
        );
        assert_eq!(detected.text_color, Some(kit(true).c.success.fg), "text-green");
        let desc = maybe(&rt, "onboarding-entry-import-desc").expect("the import description");
        assert!(
            (detected.rect[1] - (desc.rect[1] + desc.rect[3]) - 1.5).abs() < 0.5,
            "the detected line starts mt-0.5 (1.5pt) below the description"
        );
    }
    // Every client label renders (the CONNECTION_IMPORT_APP_LABEL map).
    for (app, label) in [
        (ImportApp::Dbx, "DBX"),
        (ImportApp::Navicat, "Navicat"),
        (ImportApp::Datagrip, "DataGrip"),
        (ImportApp::Dbeaver, "DBeaver"),
        (ImportApp::Tableplus, "TablePlus"),
    ] {
        let rt = rt(Scene { detected_app: Some(app), ..base() });
        assert_eq!(
            text_of(&rt, "onboarding-detected-app"),
            t("onboarding.s0.importDetectedApp").replace("{app}", label),
            "{app:?}'s display name is {label}"
        );
    }
}

/// The selected entry keeps the tree intact: whichever entry the machine
/// has picked, all three cards stay mounted and pressable (the border/ring
/// sharpening is paint-only in the probe — see the module docs).
#[test]
fn every_entry_state_keeps_the_cards_pressable() {
    for selected in [Entry::Manual, Entry::Import, Entry::Sample] {
        let mut rt = rt(Scene { selected_entry: selected, ..base() });
        settle(&mut rt);
        for testid in ["onboarding-entry-import", "onboarding-entry-manual", "onboarding-entry-sample"] {
            assert!(has(&rt, testid), "{selected:?}: {testid} stays mounted");
        }
        press(&mut rt, "onboarding-entry-import");
        press(&mut rt, "onboarding-entry-manual");
        press(&mut rt, "onboarding-entry-sample");
        assert_eq!(rt.state.import_fired, 1, "{selected:?}: the import card fires");
        assert_eq!(rt.state.manual_fired, 1, "{selected:?}: the manual card fires");
        assert_eq!(rt.state.sample_fired, 1, "{selected:?}: the sample card fires");
    }
}

/// The three cards are raw buttons: focusable and role button.
#[test]
fn the_cards_are_buttons() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    for testid in ["onboarding-entry-import", "onboarding-entry-manual", "onboarding-entry-sample"] {
        let card = maybe(&rt, testid).unwrap_or_else(|| panic!("{testid} is laid out"));
        assert_eq!(card.role, "button", "{testid} is a raw <button>");
        assert!(card.focusable, "{testid} is focusable");
    }
}
