//! Independent audit for T203 (the ManualConnectionStep Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/steps/ManualConnectionStep.tsx`
//! (plus its direct deps' contracts: `ConnectionFormBody.tsx`'s window
//! variant, `lib/databaseTypes.ts`'s popularity order and
//! `lib/filterDbTypes.ts`): the step tag / heading / `<b>`-bearing
//! subtitle, the 220px driver sidebar with its search box and
//! popularity-ordered type buttons, the availability and query filters,
//! the selected type's accent chrome, the shared form body in its window
//! variant with NO db-type/group select, and the Test/Save footer — the
//! result line's three inks and the Save gate (`disabled={!tested}`,
//! re-locked by a failed test). Runtime evidence comes from
//! `Runtime::elements()` with the step mounted exactly as production
//! mounts it — as the wizard's S1-manual seat inside the T072 window —
//! never from a value the component reports about itself. The pure
//! helpers (`filterDbTypesByQuery`, `sortDbTypesByPopularity`,
//! `ALL_DB_TYPES`) are asserted against the TypeScript sources directly.

use rune::{Cx, El, Env, KeyEvent, Key, Mods, Runtime, TextEdit};

use crate::components::connection::connection_form_body::{FormAction, FormBodyVm};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::onboarding::onboarding_window::OnboardingWindow;
use crate::views::onboarding::onboarding_wizard::{Entry, OnboardingWizard, Step};
use crate::views::onboarding::steps::manual_connection_step::{
    all_db_types, filter_db_types_by_query, sort_db_types_by_popularity,
    ManualConnectionStep, ManualStepVm, TestOutcome, DB_TYPE_POPULARITY_ORDER,
};

/// The default onboarding window (960×720 — the T021/T072 audits' default).
const W: f32 = 960.0;
const H: f32 = 720.0;

/// The result line's key (used often enough to name).
const RESULT_KEY: &str = "onboarding-step-s1-manual-test-result";

// Placeholder text is a drawing hint, not a `text` node in the element
// tree; pin the window-variant source at the call site instead (the T103
// ruling).
const STEP_SRC: &str = include_str!("../views/onboarding/steps/manual_connection_step.rs");

#[derive(Clone)]
struct Scene {
    step: Step,
    entry: Entry,
    dark: bool,
    /// The step-local half of React's state (this file's own `useState`s).
    manual: ManualStepVm,
    /// The form machine's snapshot (T103's `FormBodyVm`; the default is
    /// the dialog variant so the step's `variant="window"` pin is what
    /// the probe sees, not the scene's value).
    form: FormBodyVm,
    test_fired: usize,
    save_fired: usize,
}

impl Scene {
    /// The host's translation of the T103 actions — the same seam the
    /// dialog host wires (only the arms this audit drives are modelled).
    fn apply_form(&mut self, a: FormAction) {
        match a {
            FormAction::DatabaseType(value) => self.form.database_type = value,
            FormAction::Group(value) => self.form.group = value,
            FormAction::ClearStoredCredentials(on) => self.form.clear_stored_credentials = on,
            FormAction::OpenSelect(which) => self.form.open_select = which,
            FormAction::Advanced(_) => {}
        }
    }
}

fn base() -> Scene {
    Scene {
        step: Step::S1,
        entry: Entry::Manual,
        dark: true,
        manual: ManualStepVm::default(),
        form: FormBodyVm::default(),
        test_fired: 0,
        save_fired: 0,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    // The step the wizard hands its two VMs, built per frame over the
    // host's seams — the production wiring shape.
    let step = ManualConnectionStep::<Scene>::new(s.manual.clone(), s.form.clone())
        .on_step(|s: &mut Scene| &mut s.manual)
        .on_form_vm(|s: &mut Scene| &mut s.form)
        .on_action(|s: &mut Scene, a| s.apply_form(a))
        .on_test(|s: &mut Scene| s.test_fired += 1)
        .on_save(|s: &mut Scene| s.save_fired += 1)
        .build(s.dark, &mut layer);
    // The production mounting: the wizard's S1-manual seat inside the
    // T072 window (the seat itself gates on step+entry, exactly as
    // React's conditional).
    let wizard = OnboardingWizard::<Scene>::new()
        .step(s.step)
        .entry(s.entry)
        .manual(step);
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

/// The first text element under the element keyed `key` (a button's
/// unkeyed label — the `ai_provider_step_audit` helper).
fn text_under(rt: &Runtime<Scene>, key: &str) -> String {
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

/// The translated subtitle with its `<b>`/`</b>` tags stripped — what the
/// port's rich text must show (the tags become a bold span, never text).
fn plain_subtitle() -> String {
    t("onboarding.s1.subtitle").replace("<b>", "").replace("</b>", "")
}

// ---- the wizard's S1-manual mount -----------------------------------------------

/// React: `{state.step === 's1' && state.entry === 'manual' &&
/// <ManualConnectionStep … />}` — the step exists only there, inside the
/// wizard's reserved seat.
#[test]
fn the_step_renders_only_on_s1_manual() {
    for (step, entry) in [
        (Step::S0, Entry::Manual),
        (Step::S1, Entry::Import),
        (Step::S1, Entry::Sample),
        (Step::S2, Entry::Manual),
        (Step::S3, Entry::Manual),
    ] {
        let rt = rt(Scene { step, entry, ..base() });
        assert!(
            !has(&rt, "onboarding-step-s1-manual"),
            "{step:?}/{entry:?} does not mount the manual step"
        );
        assert!(
            !has(&rt, "onboard-test-connection"),
            "{step:?}/{entry:?} does not mount its footer either"
        );
    }
    {
        let rt = rt(Scene { ..base() });
        let root = maybe(&rt, "onboarding-step-s1-manual").expect("S1-manual mounts the step");
        let seat = maybe(&rt, "onboarding-wizard-step-s1-manual").expect("the wizard's seat");
        assert!(
            root.path.starts_with(&seat.path),
            "the step is mounted inside the wizard's S1-manual seat"
        );
    }
}

// ---- the header ------------------------------------------------------------------

/// The tag, heading and subtitle are the `onboarding.s1.*` pack entries in
/// React's rhythm: `mb-2.5` (7.5pt) between heading and subtitle, `mb-5`
/// (15pt) to the form card, the subtitle capped at 56ch and muted, its
/// one `<b>` segment present as text (bolded, never the literal tags).
#[test]
fn the_header_reads_the_s1_pack() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let k = kit(dark);
        let title = maybe(&rt, "onboarding-step-s1-manual-title").expect("the heading");
        let subtitle = maybe(&rt, "onboarding-step-s1-manual-subtitle").expect("the subtitle");
        let frame = maybe(&rt, "onboarding-step-s1-manual-frame").expect("the form card");
        assert_eq!(text_of(&rt, "onboarding-step-s1-manual-title"), t("onboarding.s1.title"));
        assert_eq!(text_of(&rt, "onboarding-step-s1-manual-subtitle"), plain_subtitle());
        assert_eq!(title.text_color, Some(k.c.text_base), "dark={dark}: text-fg");
        assert_eq!(subtitle.text_color, Some(k.c.text_muted), "dark={dark}: text-fg-secondary");
        assert!(
            (subtitle.rect[1] - (title.rect[1] + title.rect[3]) - 7.5).abs() < 0.5,
            "dark={dark}: the subtitle starts mb-2.5 (7.5pt) below the heading"
        );
        assert!(
            (frame.rect[1] - (subtitle.rect[1] + subtitle.rect[3]) - 15.0).abs() < 0.5,
            "dark={dark}: the card starts mb-5 (15pt) below the subtitle"
        );
        assert!(
            subtitle.rect[2] <= 283.5 + 0.5,
            "dark={dark}: the 56ch cap (283.5pt) binds (got {})",
            subtitle.rect[2]
        );
        // The step tag: the s1 label, uppercased (the shared chrome).
        assert_eq!(
            text_under(&rt, "onboarding-step-tag-pill"),
            t("onboarding.s1.stepLabel").to_uppercase()
        );
        // The `<b>` tags themselves never reach the rendered text.
        let rendered = text_of(&rt, "onboarding-step-s1-manual-subtitle");
        assert!(!rendered.contains("<b>") && !rendered.contains("</b>"), "tags stripped");
    }
}

// ---- the driver sidebar -----------------------------------------------------------

/// React's `ALL_DB_TYPES`: the shipped types in `DB_TYPE_POPULARITY_ORDER`
/// order — mysql first, then postgresql; vector (the last shipped id)
/// last; 22 buttons in all, each with its badge and label.
#[test]
fn the_sidebar_lists_types_in_popularity_order() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let all = all_db_types();
    assert_eq!(all.len(), 22, "the shipped-driver stand-in has 22 types");
    assert_eq!(all[0], ("mysql", "MySQL"));
    assert_eq!(all[1], ("postgresql", "PostgreSQL"));
    assert_eq!(all[21], ("vector", "Vector DB"));
    // The rendered list follows the same order, each button keyed by
    // React's testid template and labelled with the registry label.
    let mut previous_bottom: Option<f32> = None;
    for &(value, label) in all {
        let button = maybe(&rt, &format!("onboard-driver-{value}"))
            .unwrap_or_else(|| panic!("onboard-driver-{value} is laid out"));
        if let Some(bottom) = previous_bottom {
            assert!(
                button.rect[1] >= bottom - 0.5,
                "{value} stacks below the type above it"
            );
        }
        assert_eq!(text_of(&rt, &format!("onboard-driver-{value}-label")), label);
        let badge = maybe(&rt, &format!("onboard-driver-{value}-badge"))
            .unwrap_or_else(|| panic!("onboard-driver-{value} carries its DbTypeBadge"));
        assert!(
            (badge.rect[2] - 19.5).abs() < 0.5 && (badge.rect[3] - 19.5).abs() < 0.5,
            "{value}'s badge is the 26px (19.5pt) tile, got {}×{}",
            badge.rect[2],
            badge.rect[3]
        );
        assert_eq!(button.role, "button", "{value} is a raw <button>");
        assert!(button.focusable, "{value} is focusable");
        previous_bottom = Some(button.rect[1] + button.rect[3]);
    }
    // The search box leads the list (React's `p-3` wrapper, the h-8 input).
    let search = maybe(&rt, "onboarding-step-s1-manual-driver-search-input")
        .expect("the search input");
    let list = maybe(&rt, "onboarding-step-s1-manual-driver-list").expect("the list");
    assert!(
        (search.rect[3] - 24.0).abs() < 0.5,
        "the search input is h-8 (24pt), got {}",
        search.rect[3]
    );
    assert!(search.rect[1] < list.rect[1], "the search box sits above the list");
    // The sidebar itself: the fixed 220px (165pt) column, flush left in
    // the card, with the form column starting at its right edge.
    let sidebar = maybe(&rt, "onboarding-step-s1-manual-sidebar").expect("the sidebar");
    let frame = maybe(&rt, "onboarding-step-s1-manual-frame").expect("the card");
    let main = maybe(&rt, "onboarding-step-s1-manual-main").expect("the form column");
    assert!(
        (sidebar.rect[2] - 165.0).abs() < 0.5,
        "the sidebar is w-[220px] (165pt), got {}",
        sidebar.rect[2]
    );
    assert_eq!(sidebar.rect[0], frame.rect[0], "the sidebar is flush with the card's left");
    assert!(
        (main.rect[0] - (sidebar.rect[0] + sidebar.rect[2])).abs() < 0.5,
        "the form column starts at the sidebar's right edge"
    );
    assert!(
        (frame.radius - kit(true).radius.lg).abs() < 0.5,
        "the card is rounded-xl (the kit's lg radius), got {}",
        frame.radius
    );
}

/// React's `dbTypes` memo, availability half: `null` availableDrivers
/// (pending or failed load — indistinguishable, both null) shows the FULL
/// list; a loaded list keeps only its members.
#[test]
fn availability_filters_the_sidebar() {
    // `None` (pending or failed): every shipped type shows.
    {
        let rt = rt(Scene { ..base() });
        assert!(has(&rt, "onboard-driver-mysql"), "None: the full list shows");
        assert!(has(&rt, "onboard-driver-vector"), "None: down to the last type");
    }
    // A loaded list of two: exactly those two buttons.
    {
        let rt = rt(Scene {
            manual: ManualStepVm {
                available_drivers: Some(vec!["postgresql".to_owned(), "redis".to_owned()]),
                ..ManualStepVm::default()
            },
            ..base()
        });
        assert!(has(&rt, "onboard-driver-postgresql"));
        assert!(has(&rt, "onboard-driver-redis"));
        assert!(!has(&rt, "onboard-driver-mysql"), "an unavailable type is hidden");
        assert!(!has(&rt, "onboard-driver-sqlite"), "an unavailable type is hidden");
    }
}

/// React's `dbTypes` memo, query half (`filterDbTypesByQuery`): the
/// trimmed, lower-cased query matches label OR value; no match renders
/// the `newConn.noDriversMatch` line instead of the buttons.
#[test]
fn the_query_filters_label_or_value() {
    // Label match, case-insensitive.
    {
        let rt = rt(Scene {
            manual: ManualStepVm {
                driver_query: TextEdit::new("POST".to_owned()),
                ..ManualStepVm::default()
            },
            ..base()
        });
        assert!(has(&rt, "onboard-driver-postgresql"), "the label matches");
        assert!(!has(&rt, "onboard-driver-mysql"), "MySQL's label does not");
    }
    // Value match (the query need not match the label).
    {
        let rt = rt(Scene {
            manual: ManualStepVm {
                driver_query: TextEdit::new("  sqlite  ".to_owned()),
                ..ManualStepVm::default()
            },
            ..base()
        });
        assert!(has(&rt, "onboard-driver-sqlite"), "the value matches after trimming");
    }
    // No match: the empty line, no buttons.
    {
        let rt = rt(Scene {
            manual: ManualStepVm {
                driver_query: TextEdit::new("zzz-no-such-driver".to_owned()),
                ..ManualStepVm::default()
            },
            ..base()
        });
        assert!(!has(&rt, "onboard-driver-mysql"), "no buttons on no match");
        assert_eq!(
            text_of(&rt, "onboarding-step-s1-manual-driver-empty"),
            t("newConn.noDriversMatch"),
            "the no-match line reads the newConn pack"
        );
    }
}

/// The selection chrome: the form machine's current type wears
/// `bg-accent/10` + `text-fg` + the accent right border; every other
/// button has no fill and the secondary ink. Pressing a type routes
/// `handleDatabaseTypeChange` through the shared T103 action seam.
#[test]
fn the_selected_type_wears_the_accent_chrome_and_presses_route() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    // The scene's form starts at postgresql (the FormBodyVm default).
    let k = kit(true);
    let selected = maybe(&rt, "onboard-driver-postgresql").expect("the selected type");
    assert!(
        (selected.fill.a - 0.10).abs() < 0.02,
        "bg-accent/10 on the selected type (got alpha {})",
        selected.fill.a
    );
    assert_eq!(
        maybe(&rt, "onboard-driver-postgresql-label").unwrap().text_color,
        Some(k.c.text_base),
        "text-fg on the selected label"
    );
    let other = maybe(&rt, "onboard-driver-mysql").expect("an unselected type");
    assert_eq!(other.fill.a, 0.0, "no fill on an unselected type");
    assert_eq!(
        maybe(&rt, "onboard-driver-mysql-label").unwrap().text_color,
        Some(k.c.text_muted),
        "text-fg-secondary on an unselected label"
    );
    // Press mysql → the form machine's databaseType becomes mysql and the
    // chrome moves (the same FormAction::DatabaseType the body's own
    // select dispatches).
    press(&mut rt, "onboard-driver-mysql");
    assert_eq!(rt.state.form.database_type, "mysql", "the press routes the type change");
    let now = maybe(&rt, "onboard-driver-mysql").unwrap();
    assert!(
        (now.fill.a - 0.10).abs() < 0.02,
        "the accent fill moved to mysql (got alpha {})",
        now.fill.a
    );
    assert_eq!(
        maybe(&rt, "onboard-driver-postgresql").unwrap().fill.a,
        0.0,
        "postgresql lost its fill"
    );
}

/// The search field is a live edit: typing into it narrows the list (the
/// full select-all-then-paste journey, per the Runtime essentials).
#[test]
fn typing_in_the_search_field_narrows_the_list() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    assert!(has(&rt, "onboard-driver-mysql"), "all 22 show before typing");
    let (x, y) = rt
        .find("onboarding-step-s1-manual-driver-search-input")
        .expect("the search input is laid out");
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.key(KeyEvent {
        key: Key::Command('a'),
        mods: Mods { cmd: true, ..Mods::default() },
    });
    rt.frame(1.0 / 60.0);
    rt.paste("redis");
    settle(&mut rt);
    assert_eq!(rt.state.manual.driver_query.text, "redis", "the edit landed in the query");
    assert!(has(&rt, "onboard-driver-redis"), "redis still shows");
    assert!(!has(&rt, "onboard-driver-mysql"), "mysql is filtered out");
}

// ---- the shared form body ---------------------------------------------------------

/// `<ConnectionFormBody form={form} variant="window" />` — the body mounts
/// with its React keys, and the step's pins hold: the window variant (the
/// scene's VM says dialog), no db-type select and no group select (the
/// sidebar is the only picker).
#[test]
fn the_shared_form_body_mounts_in_its_window_variant() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    for key in [
        "connection-form-body",
        "new-conn-name",
        "new-conn-host",
        "new-conn-port",
        "new-conn-database",
        "new-conn-password",
        "new-conn-advanced-toggle",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    // React's step passes neither `showDbTypeSelect` nor `groupOptions`.
    assert!(!has(&rt, "new-conn-db-type-field-trigger"), "no db-type select");
    assert!(!has(&rt, "new-conn-group-field-trigger"), "no group select");
    // The window variant is the step's own pin (placeholders are drawing
    // hints, not text nodes — the T103 source-pin ruling).
    assert!(
        STEP_SRC.contains("form.is_window = true;"),
        "the step pins variant=\"window\" on the rendered snapshot"
    );
    // The body scrolls inside its own area below the sidebar's search box.
    let form_area = maybe(&rt, "onboarding-step-s1-manual-form").expect("the form area");
    let footer = maybe(&rt, "onboarding-step-s1-manual-footer").expect("the footer");
    assert!(
        footer.rect[1] >= form_area.rect[1] + form_area.rect[3] - 1.0,
        "the footer sits below the form area"
    );
}

// ---- the Test/Save footer ---------------------------------------------------------

/// The Test button: the secondary action with the flask glyph, its label
/// swapping to the testing pack entry and locking while a test runs.
#[test]
fn the_test_button_swaps_to_the_testing_label_and_locks() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    assert_eq!(
        text_under(&rt, "onboard-test-connection"),
        t("onboarding.s1.testBtn"),
        "the rest label is the s1 pack entry"
    );
    press(&mut rt, "onboard-test-connection");
    assert_eq!(rt.state.test_fired, 1, "the press reaches the host's test flow");
    // `handleTest`'s opening: testing + the result line cleared.
    rt.update(|s, _| s.manual.begin_test());
    settle(&mut rt);
    assert_eq!(
        text_under(&rt, "onboard-test-connection"),
        t("newConn.testing"),
        "the busy label is the newConn pack entry"
    );
    let busy = maybe(&rt, "onboard-test-connection").expect("the test button");
    assert!(!busy.focusable, "disabled while testing locks the button");
    press(&mut rt, "onboard-test-connection");
    assert_eq!(rt.state.test_fired, 1, "the locked button never fires");
}

/// The result line: empty and muted with no result, `✓` + the success
/// entry in green on ok, `✕` + the failure entry in red on fail.
#[test]
fn the_result_line_reports_the_outcome() {
    let k = kit(true);
    {
        let mut rt = rt(Scene { ..base() });
        settle(&mut rt);
        let line = maybe(&rt, RESULT_KEY).expect("the result span is always mounted");
        assert_eq!(line.text.as_deref().unwrap_or(""), "", "no text with no result");
        assert_eq!(line.text_color, Some(k.c.text_muted), "text-fg-muted with no result");
    }
    {
        let mut rt = rt(Scene { ..base() });
        rt.update(|s, _| s.manual.test_succeeded());
        settle(&mut rt);
        let line = maybe(&rt, RESULT_KEY).expect("the result span");
        assert_eq!(
            text_of(&rt, RESULT_KEY),
            format!("✓ {}", t("onboarding.s1.testSuccess")),
            "the ok line is ✓ + the pack entry"
        );
        assert_eq!(line.text_color, Some(k.c.success.fg), "text-green on ok");
    }
    {
        let mut rt = rt(Scene { ..base() });
        rt.update(|s, _| s.manual.test_failed());
        settle(&mut rt);
        let line = maybe(&rt, RESULT_KEY).expect("the result span");
        assert_eq!(
            text_of(&rt, RESULT_KEY),
            format!("✕ {}", t("onboarding.s1.testFail")),
            "the fail line is ✕ + the pack entry"
        );
        assert_eq!(line.text_color, Some(k.c.danger.fg), "text-red on fail");
    }
}

/// `handleTest`'s transitions on the VM: begin clears the result and sets
/// testing (but NOT `tested` — a previously passed test keeps Save open
/// while re-testing, exactly React's setState set); success arms Save;
/// failure re-locks it.
#[test]
fn the_test_lifecycle_is_react_handle_test() {
    let mut vm = ManualStepVm::default();
    vm.begin_test();
    assert!(vm.testing, "setTesting(true)");
    assert_eq!(vm.test_result, None, "setTestResult(null)");
    assert!(!vm.tested, "tested is untouched by begin");
    vm.test_succeeded();
    assert!(!vm.testing, "finally: setTesting(false)");
    assert!(vm.tested, "setTested(true)");
    assert_eq!(vm.test_result, Some(TestOutcome::Ok), "setTestResult('ok')");
    // A re-test after a success: the result line clears, Save stays armed.
    vm.begin_test();
    assert!(vm.testing);
    assert_eq!(vm.test_result, None, "a new test clears the old result");
    assert!(vm.tested, "a passed test stays passed while re-testing");
    // The catch arm: failure re-locks Save.
    vm.test_failed();
    assert!(!vm.testing);
    assert!(!vm.tested, "setTested(false) on failure");
    assert_eq!(vm.test_result, Some(TestOutcome::Fail), "setTestResult('fail')");
}

/// The Save gate: the journey's only way forward starts locked
/// (`disabled={!tested}`), unlocks after a passed test, and a FAILED test
/// re-locks it.
#[test]
fn save_unlocks_only_after_a_passed_test() {
    // Fresh journey: locked, and the locked button never fires.
    {
        let mut rt = rt(Scene { ..base() });
        settle(&mut rt);
        let save = maybe(&rt, "onboard-s1-save").expect("the save button");
        assert!(!save.focusable, "Save starts disabled (never tested)");
        assert_eq!(
            text_under(&rt, "onboard-s1-save"),
            t("common.save"),
            "the label is the common pack entry"
        );
        press(&mut rt, "onboard-s1-save");
        assert_eq!(rt.state.save_fired, 0, "the locked Save never fires");
    }
    // A passed test unlocks it, and the press reaches the host's save flow.
    {
        let mut rt = rt(Scene { ..base() });
        rt.update(|s, _| s.manual.test_succeeded());
        settle(&mut rt);
        assert!(
            maybe(&rt, "onboard-s1-save").unwrap().focusable,
            "a passed test unlocks Save"
        );
        press(&mut rt, "onboard-s1-save");
        assert_eq!(rt.state.save_fired, 1, "the press reaches the host's save flow");
    }
    // A failed test re-locks it.
    {
        let mut rt = rt(Scene { ..base() });
        rt.update(|s, _| s.manual.test_failed());
        settle(&mut rt);
        assert!(
            !maybe(&rt, "onboard-s1-save").unwrap().focusable,
            "a failed test re-locks Save"
        );
        press(&mut rt, "onboard-s1-save");
        assert_eq!(rt.state.save_fired, 0, "the locked Save never fires");
    }
}

/// The footer's layout: Test left, the flex-1 result line between, Save
/// right of it — all on one row inside the card.
#[test]
fn the_footer_rows_test_result_save() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let test = maybe(&rt, "onboard-test-connection").expect("the test button");
    let result = maybe(&rt, RESULT_KEY).expect("the result line");
    let save = maybe(&rt, "onboard-s1-save").expect("the save button");
    assert!(
        test.rect[0] < result.rect[0] && result.rect[0] < save.rect[0],
        "test < result < save on the row"
    );
    assert!(
        (test.rect[1] - save.rect[1]).abs() < 1.0,
        "the buttons share the row's center line"
    );
}

// ---- geometry ---------------------------------------------------------------------

/// Nothing renders outside the window horizontally, in either theme.
#[test]
fn every_part_stays_inside_the_window_horizontally() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
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

// ---- the pure helpers (asserted against the TypeScript sources) -------------------

/// `filterDbTypesByQuery` (`lib/filterDbTypes.ts`): trimmed lower-case
/// query over label OR value; empty (or whitespace) keeps everything.
#[test]
fn filter_matches_label_or_value_case_insensitively() {
    let items = [("postgresql", "PostgreSQL"), ("mysql", "MySQL"), ("sqlite", "SQLite")];
    assert_eq!(filter_db_types_by_query(&items, ""), items.to_vec(), "empty keeps all");
    assert_eq!(filter_db_types_by_query(&items, "   "), items.to_vec(), "trimmed empty keeps all");
    assert_eq!(
        filter_db_types_by_query(&items, "POST"),
        vec![("postgresql", "PostgreSQL")],
        "label match, case-insensitive"
    );
    assert_eq!(
        filter_db_types_by_query(&items, "MySQL"),
        vec![("mysql", "MySQL")],
        "value match"
    );
    assert_eq!(
        filter_db_types_by_query(&items, "my"),
        vec![("mysql", "MySQL")],
        "value substring match"
    );
    assert!(filter_db_types_by_query(&items, "zzz").is_empty(), "no match drops all");
}

/// `sortDbTypesByPopularity` (`lib/databaseTypes.ts:63`): rank order
/// first, ties broken by id; unknown ids rank after every known one.
#[test]
fn popularity_sorts_known_first_then_by_id() {
    // The known ids come out in `DB_TYPE_POPULARITY_ORDER` order whatever
    // the input order.
    let mut reversed: Vec<(&'static str, &'static str)> = DB_TYPE_POPULARITY_ORDER
        .iter()
        .map(|id| (*id, ""))
        .collect();
    reversed.reverse();
    let sorted = sort_db_types_by_popularity(reversed);
    assert_eq!(
        sorted.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        DB_TYPE_POPULARITY_ORDER.to_vec(),
        "rank order, independent of the input order"
    );
    // Unknown ids sort after all known ones, by id.
    let sorted = sort_db_types_by_popularity(vec![
        ("zz-unknown", "X"),
        ("mysql", "MySQL"),
        ("aa-unknown", "Y"),
    ]);
    assert_eq!(sorted[0], ("mysql", "MySQL"), "the known id leads");
    assert_eq!(sorted[1], ("aa-unknown", "Y"), "unknown ties break by id");
    assert_eq!(sorted[2], ("zz-unknown", "X"), "unknown ties break by id");
}
