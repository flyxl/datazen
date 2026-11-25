//! Independent audit for T177 (the ImportStep Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/steps/ImportStep.tsx` (plus
//! the wizard's `{isStepOne && state.entry === 'import' && <ImportStep …
//! />}` mount and the `useConnectionImport` /
//! `ConnectionImportFields` sources it composes): the S1-import-only
//! mount, the header's step tag / heading / subtitle rhythm, the six
//! source buttons in React's `SOURCE_ORDER` with their labels
//! (the translated file label and the five client display names), the
//! detected-client checkmarks, the selection write (`setSource`) and the
//! selected `border-accent/55 bg-accent/10` chrome, the bordered form
//! card carrying the inline fields, the success banner behind its
//! `written > 0` gate with the imported/updated count split, and the
//! footer's submit button (the `primaryActionLabel` memo's three cases,
//! the busy lock, the flask glyph) and detecting line. Runtime evidence
//! comes from `Runtime::elements()` with the step mounted exactly as
//! production mounts it — as the wizard's S1-import seat inside the T072
//! window — never from a value the component reports about itself.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::components::connection::connection_import_fields::ImportVm;
use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::onboarding::onboarding_window::OnboardingWindow;
use datazen_ui::views::onboarding::onboarding_wizard::{
    Entry, ImportApp, ImportSource, OnboardingWizard, Step,
};
use datazen_ui::views::onboarding::steps::import_step::{
    ImportOutcome, ImportStep, ImportStepVm, ImportSummary, SOURCE_ORDER, import_summary,
    source_id,
};

/// The default onboarding window (960×720 — the T021/T072 audits' default).
const W: f32 = 960.0;
const H: f32 = 720.0;

#[derive(Clone)]
struct Scene {
    step: Step,
    entry: Entry,
    dark: bool,
    /// The step-local half of React's state (the chosen source) — the
    /// live VM the picker's writes land in.
    vm: ImportStepVm,
    /// The import machinery's snapshot (the T346 seam the fields render).
    imp: ImportVm,
    /// The machinery's `detecting` flag (the footer's mono line).
    detecting: bool,
    /// `importResult` — the wizard state's summary (the success banner).
    import_result: Option<ImportSummary>,
    /// `detectedApps` — every client whose config was found (T306).
    detected_apps: Vec<ImportApp>,
    /// How many times the submit action fired.
    submitted: usize,
}

fn base() -> Scene {
    // React's realistic import-entry mount: the wizard's `initialSource`
    // defaults to 'file' (T114's `useState('file')`) until a detected
    // client adopts it; the machinery's app-flow snapshot shows the
    // detected data path.
    let mut imp = ImportVm::default();
    imp.app_import = true;
    imp.path_found = true;
    Scene {
        step: Step::S1,
        entry: Entry::Import,
        dark: true,
        vm: ImportStepVm::default(),
        imp,
        detecting: false,
        import_result: None,
        detected_apps: Vec::new(),
        submitted: 0,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    // The step over the same seams production wires: the step VM and the
    // machinery VM live in `S` (the picker's and the fields' writes land
    // there), the submit action counts, and the detected clients ride
    // the feed-in.
    let mut step = ImportStep::<Scene>::new(s.vm.clone(), s.imp.clone())
        .detecting(s.detecting)
        .detected_apps(s.detected_apps.clone())
        .on_step(|s: &mut Scene| &mut s.vm)
        .on_vm(|s: &mut Scene| &mut s.imp)
        .on_submit(|s: &mut Scene| s.submitted += 1);
    if let Some(result) = &s.import_result {
        step = step.import_result(result.clone());
    }
    // The production mounting: the wizard's S1-import seat inside the
    // T072 window (the seat itself gates on step+entry, exactly as
    // React's conditional).
    let wizard = OnboardingWizard::<Scene>::new()
        .step(s.step)
        .entry(s.entry)
        .import(step.build(s.dark));
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

/// The first text element under the element keyed `key` (the kit button's
/// label — the manual-step audit's `text_under`).
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

/// Channel-wise colour equality with a small tolerance (any solving the
/// paint pipeline performs stays inside it).
fn same_color(a: rune::Color, b: rune::Color) -> bool {
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

// ---- the wizard's S1-import mount ----------------------------------------------

/// React: `{isStepOne && state.entry === 'import' && <ImportStep … />}` —
/// the step exists only on S1 of the import entry, inside the wizard's
/// import seat.
#[test]
fn the_step_renders_only_on_s1_import() {
    for (step, entry) in [
        (Step::S0, Entry::Import),
        (Step::S1, Entry::Manual),
        (Step::S1, Entry::Sample),
        (Step::S2, Entry::Import),
        (Step::S3, Entry::Import),
    ] {
        let rt = rt(Scene { step, entry, ..base() });
        assert!(
            !has(&rt, "onboarding-step-s1-import"),
            "{step:?}/{entry:?} does not mount the import step"
        );
        assert!(
            !has(&rt, "onboarding-import-form"),
            "{step:?}/{entry:?} does not mount the form card either"
        );
    }
    {
        let rt = rt(Scene { ..base() });
        let root = maybe(&rt, "onboarding-step-s1-import").expect("S1-import mounts the step");
        let seat = maybe(&rt, "onboarding-wizard-step-s1-import").expect("the wizard's import seat");
        assert!(
            root.path.starts_with(&seat.path),
            "the step is mounted inside the wizard's S1-import seat"
        );
    }
}

// ---- the header ------------------------------------------------------------------

/// The step's own texts are the `onboarding.s1.*` pack entries, in React's
/// rhythm: the tag label, the heading (`mb-2.5` to the subtitle), the
/// subtitle (`mb-5` to the picker, capped at 56ch — narrower than the
/// column).
#[test]
fn the_header_reads_the_s1_import_pack() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let k = kit(dark);
        assert_eq!(
            text_under(&rt, "onboarding-step-tag-pill"),
            t("onboarding.s1.stepLabel").to_uppercase()
        );
        assert_eq!(
            text_of(&rt, "onboarding-step-s1-import-title"),
            t("onboarding.s1.importTitle")
        );
        assert_eq!(
            text_of(&rt, "onboarding-step-s1-import-subtitle"),
            t("onboarding.s1.importSubtitle")
        );
        let title = maybe(&rt, "onboarding-step-s1-import-title").expect("the heading");
        let subtitle = maybe(&rt, "onboarding-step-s1-import-subtitle").expect("the subtitle");
        let picker = maybe(&rt, "onboarding-import-sources").expect("the picker row");
        let card = maybe(&rt, "onboarding-import-form").expect("the form card");
        assert_eq!(title.text_color, Some(k.c.text_base), "dark={dark}: text-fg");
        assert_eq!(subtitle.text_color, Some(k.c.text_muted), "dark={dark}: text-fg-secondary");
        assert!(
            (subtitle.rect[1] - (title.rect[1] + title.rect[3]) - 7.5).abs() < 0.5,
            "dark={dark}: the subtitle starts mb-2.5 (7.5pt) below the heading"
        );
        assert!(
            (picker.rect[1] - (subtitle.rect[1] + subtitle.rect[3]) - 15.0).abs() < 0.5,
            "dark={dark}: the picker starts mb-5 (15pt) below the subtitle"
        );
        assert!(
            (card.rect[1] - (picker.rect[1] + picker.rect[3]) - 12.0).abs() < 0.5,
            "dark={dark}: the card starts mb-4 (12pt) below the picker"
        );
        assert!(
            subtitle.rect[2] < card.rect[2] - 100.0,
            "dark={dark}: the 56ch cap binds well below the card (subtitle {} vs card {})",
            subtitle.rect[2],
            card.rect[2]
        );
    }
}

// ---- the source picker -------------------------------------------------------------

/// React `SOURCE_ORDER`: the picker renders exactly the six sources, in
/// React's order (file first, then dbeaver / datagrip / navicat /
/// tableplus / dbx), each labelled with the translated file entry or the
/// client's display name from `CONNECTION_IMPORT_APP_LABEL`.
#[test]
fn the_picker_shows_six_sources_in_react_order() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let expected_labels = [
        t("onboarding.s1.importFileSource"),
        "DBeaver",
        "DataGrip",
        "Navicat",
        "TablePlus",
        "DBX",
    ];
    for (i, source) in SOURCE_ORDER.iter().enumerate() {
        let id = source_id(*source);
        let button = maybe(&rt, &format!("onboarding-import-source-{id}"))
            .unwrap_or_else(|| panic!("source {id} renders"));
        assert_eq!(
            text_of(&rt, &format!("onboarding-import-source-{id}-label")),
            expected_labels[i],
            "source {id} carries its label"
        );
        // role=button, focusable — React's `<button type="button">`.
        assert_eq!(button.role, "button", "source {id} is a button");
        assert!(button.focusable, "source {id} is focusable");
    }
    // React's order: reading the row left-to-right (top-to-bottom when it
    // wraps) yields SOURCE_ORDER.
    let mut placed: Vec<(f32, f32, &'static str)> = SOURCE_ORDER
        .iter()
        .map(|s| {
            let id = source_id(*s);
            let button = maybe(&rt, &format!("onboarding-import-source-{id}"))
                .unwrap_or_else(|| panic!("source {id} renders"));
            (button.rect[1], button.rect[0], id)
        })
        .collect();
    placed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    let order: Vec<&str> = placed.iter().map(|p| p.2).collect();
    let want: Vec<&str> = SOURCE_ORDER.iter().map(|s| source_id(*s)).collect();
    assert_eq!(order, want, "the six sources render in React's SOURCE_ORDER");
    // Nothing else wears the picker's testid prefix.
    assert!(
        !has(&rt, "onboarding-import-source-vscode"),
        "no seventh source exists"
    );
}

/// React: `detectedApps.includes(s)` — every DETECTED client's button
/// grows the green check; the undetected ones and the file button never
/// do.
#[test]
fn the_detected_checkmarks_ride_only_detected_clients() {
    let mut rt = rt(Scene {
        detected_apps: vec![ImportApp::Dbeaver, ImportApp::Navicat],
        ..base()
    });
    settle(&mut rt);
    let k = kit(true);
    for id in ["dbeaver", "navicat"] {
        let check = maybe(&rt, &format!("onboarding-import-found-{id}"))
            .unwrap_or_else(|| panic!("detected {id} shows the check"));
        assert_eq!(check.text_color, Some(k.c.success.fg), "text-green");
        assert!(
            (check.rect[2] - 6.75).abs() < 0.5 && (check.rect[3] - 6.75).abs() < 0.5,
            "h-3 w-3 (6.75pt), got {}×{}",
            check.rect[2],
            check.rect[3]
        );
    }
    for id in ["datagrip", "tableplus", "dbx", "file"] {
        assert!(
            !has(&rt, &format!("onboarding-import-found-{id}")),
            "undetected {id} shows no check"
        );
    }
}

/// React: `onClick={() => setSource(s)}` — pressing a source writes the
/// step-local state, and the pressed source takes the selected chrome
/// (`border-accent/55 bg-accent/10 text-fg`) while the previous one
/// returns to the plain `border-edge bg-surface` chrome.
#[test]
fn pressing_a_source_button_selects_it() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let k = kit(true);
    // React's fresh mount: the wizard's `initialSource` default 'file'.
    assert_eq!(rt.state.vm.source, ImportSource::File);
    assert!(
        same_color(
            maybe(&rt, "onboarding-import-source-file").expect("the file button").fill,
            k.c.bg_accent.alpha(0.10)
        ),
        "the initial source wears bg-accent/10"
    );
    press(&mut rt, "onboarding-import-source-navicat");
    assert_eq!(
        rt.state.vm.source,
        ImportSource::App(ImportApp::Navicat),
        "the press writes setSource(navicat)"
    );
    assert!(
        same_color(
            maybe(&rt, "onboarding-import-source-navicat").expect("the navicat button").fill,
            k.c.bg_accent.alpha(0.10)
        ),
        "the pressed source takes bg-accent/10"
    );
    assert!(
        same_color(
            maybe(&rt, "onboarding-import-source-file").expect("the file button").fill,
            k.c.surface_base
        ),
        "the previous source returns to bg-surface"
    );
    // The selected label's ink turns text-fg; the others stay secondary.
    assert_eq!(
        maybe(&rt, "onboarding-import-source-navicat-label").expect("the label").text_color,
        Some(k.c.text_base),
        "selected text-fg"
    );
    assert_eq!(
        maybe(&rt, "onboarding-import-source-file-label").expect("the label").text_color,
        Some(k.c.text_muted),
        "unselected text-fg-secondary"
    );
}

// ---- the form card and the success banner -------------------------------------------

/// React: the card carries `ConnectionImportFields` inline — the app flow
/// shows the data-path block, the file flow the format hint — and the
/// success banner appears only when `importResult && written > 0`, with
/// the imported entry when the run created connections and the updated
/// entry when it only replaced them.
#[test]
fn the_success_banner_gates_on_written_and_the_count_split() {
    // The app flow's fields render inside the card.
    {
        let mut rt = rt(Scene { ..base() });
        settle(&mut rt);
        assert!(has(&rt, "onboarding-import-form"), "the form card renders");
        assert!(has(&rt, "import-data-path"), "the app flow's data-path field renders");
        assert!(!has(&rt, "onboarding-import-success"), "no banner before an import");
    }
    // The file flow swaps to the format hint.
    {
        let mut rt = rt(Scene {
            imp: ImportVm {
                app_import: false,
                ..ImportVm::default()
            },
            ..base()
        });
        settle(&mut rt);
        assert!(has(&rt, "import-formats-hint"), "the file flow's hint renders");
        assert!(!has(&rt, "import-data-path"), "no data-path field in the file flow");
    }
    // A run that created connections: the imported entry.
    {
        let mut rt = rt(Scene {
            import_result: Some(ImportSummary {
                imported: 3,
                overwritten: 2,
                source: "DBeaver".to_owned(),
            }),
            ..base()
        });
        settle(&mut rt);
        let k = kit(true);
        let banner = maybe(&rt, "onboarding-import-success").expect("the banner after a run");
        assert!(
            same_color(banner.fill, k.c.success.fg.alpha(0.10)),
            "bg-green/10 (got {:?})",
            banner.fill
        );
        assert_eq!(
            text_of(&rt, "onboarding-import-success-text"),
            t("onboarding.s1.importSuccess")
                .replace("{count}", "3")
                .replace("{source}", "DBeaver"),
            "imported > 0 reads the imported entry"
        );
        assert!(
            !has(&rt, "onboarding-summary-connection"),
            "the done step's rows do not leak into the import step"
        );
    }
    // A run that only replaced: the updated entry.
    {
        let rt = rt(Scene {
            import_result: Some(ImportSummary {
                imported: 0,
                overwritten: 4,
                source: "Navicat".to_owned(),
            }),
            ..base()
        });
        assert_eq!(
            text_of(&rt, "onboarding-import-success-text"),
            t("onboarding.s1.importUpdated")
                .replace("{count}", "4")
                .replace("{source}", "Navicat"),
            "imported == 0 reads the updated entry"
        );
    }
    // A run that wrote nothing, and no run at all: no banner.
    {
        let mut rt = rt(Scene {
            import_result: Some(ImportSummary {
                imported: 0,
                overwritten: 0,
                source: String::new(),
            }),
            ..base()
        });
        settle(&mut rt);
        assert!(!has(&rt, "onboarding-import-success"), "written == 0 hides the banner");
    }
}

// ---- the footer ---------------------------------------------------------------------

/// React: the submit button's label is the hook's `primaryActionLabel`
/// memo — `chooseImportFile` in the file flow before a file is chosen,
/// `importAction` otherwise — swapped to `importing` and locked while
/// busy; the detecting line follows `imp.detecting` alone.
#[test]
fn the_submit_button_label_and_disabled_rule() {
    // App flow: the plain import action.
    {
        let mut rt = rt(Scene { ..base() });
        settle(&mut rt);
        assert_eq!(
            text_under(&rt, "onboarding-import-submit"),
            t("connShare.importAction"),
            "the app flow's primary label"
        );
        assert!(
            maybe(&rt, "onboarding-import-submit").expect("the submit button").focusable,
            "idle submit is focusable"
        );
        assert_eq!(
            text_of(&rt, "onboarding-import-detecting"),
            "",
            "no detecting line while idle"
        );
    }
    // File flow without a chosen file: the two-step pick-file label.
    {
        let rt = rt(Scene {
            imp: ImportVm {
                app_import: false,
                ..ImportVm::default()
            },
            ..base()
        });
        assert_eq!(
            text_under(&rt, "onboarding-import-submit"),
            t("connShare.chooseImportFile"),
            "the file flow's pick-file label"
        );
    }
    // File flow WITH a chosen file: back to the import action.
    {
        let rt = rt(Scene {
            imp: ImportVm {
                app_import: false,
                selected_file: Some("/tmp/connections.datazenconnections".to_owned()),
                ..ImportVm::default()
            },
            ..base()
        });
        assert_eq!(
            text_under(&rt, "onboarding-import-submit"),
            t("connShare.importAction"),
            "a chosen file returns the import label"
        );
    }
    // Busy: the importing label, the lock, and the detecting line.
    {
        let mut rt = rt(Scene {
            imp: ImportVm {
                busy: true,
                ..ImportVm::default()
            },
            detecting: true,
            ..base()
        });
        settle(&mut rt);
        assert_eq!(
            text_under(&rt, "onboarding-import-submit"),
            t("onboarding.s1.importing"),
            "busy reads the importing entry"
        );
        assert!(
            !maybe(&rt, "onboarding-import-submit").expect("the submit button").focusable,
            "busy locks the submit button"
        );
        assert_eq!(
            text_of(&rt, "onboarding-import-detecting"),
            t("onboarding.s1.importDetecting"),
            "detecting reads its own line"
        );
    }
}

/// React: `onClick={() => void imp.submit()}` — the button fires the
/// host's submit action once per press.
#[test]
fn pressing_submit_fires_the_action() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    assert_eq!(rt.state.submitted, 0);
    press(&mut rt, "onboarding-import-submit");
    assert_eq!(rt.state.submitted, 1, "one press, one submit");
    press(&mut rt, "onboarding-import-submit");
    assert_eq!(rt.state.submitted, 2, "and it keeps firing");
}

// ---- the summary seam -----------------------------------------------------------------

/// React `handleSuccess`: the summary the wizard's `IMPORT_SUCCESS`
/// dispatch carries — the backend's format name when it reports one,
/// else the picker's label for the source that ran.
#[test]
fn the_summary_seam_maps_the_result() {
    // The backend's format name wins.
    let outcome = ImportOutcome {
        imported: 5,
        overwritten: 1,
        source_format: Some("SQLite 连接文件".to_owned()),
    };
    assert_eq!(
        import_summary(ImportSource::App(ImportApp::Dbeaver), &outcome),
        ImportSummary {
            imported: 5,
            overwritten: 1,
            source: "SQLite 连接文件".to_owned(),
        }
    );
    // No format name: the picker's label — the translated file entry in
    // the file flow, the client's display name in an app flow.
    let no_format = ImportOutcome { imported: 2, overwritten: 0, source_format: None };
    assert_eq!(
        import_summary(ImportSource::File, &no_format).source,
        t("onboarding.s1.importFileSource")
    );
    assert_eq!(
        import_summary(ImportSource::App(ImportApp::Datagrip), &no_format).source,
        "DataGrip"
    );
    assert_eq!(import_summary(ImportSource::File, &no_format).imported, 2);
    assert_eq!(import_summary(ImportSource::File, &no_format).overwritten, 0);
}

// ---- the geometry guard ---------------------------------------------------------------

/// Nothing renders outside the window horizontally, in either theme, in
/// the busiest states (detected checkmarks, a success banner, the file
/// flow's chosen-file row).
#[test]
fn every_part_stays_inside_the_window_horizontally() {
    for dark in [true, false] {
        for state in 0..3 {
            let mut scene = Scene {
                dark,
                detected_apps: vec![
                    ImportApp::Dbeaver,
                    ImportApp::Datagrip,
                    ImportApp::Navicat,
                    ImportApp::Tableplus,
                    ImportApp::Dbx,
                ],
                ..base()
            };
            match state {
                0 => {} // idle app flow
                1 => {
                    scene.import_result = Some(ImportSummary {
                        imported: 3,
                        overwritten: 2,
                        source: "DBeaver".to_owned(),
                    });
                }
                _ => {
                    scene.imp = ImportVm {
                        app_import: false,
                        selected_file: Some("/tmp/a-very-long-connections-file.datazenconnections".to_owned()),
                        ..ImportVm::default()
                    };
                }
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
