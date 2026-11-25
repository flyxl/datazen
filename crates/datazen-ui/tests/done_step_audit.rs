//! Independent audit for T177 (the DoneStep Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/steps/DoneStep.tsx` (plus
//! the `connectionSummary` / `importedCount` helpers of
//! `wizardState.ts` it composes and the wizard's
//! `{state.step === 's3' && <DoneStep state={state} onOpen={handleOpen}
//! />}` mount): the S3-only mount, the centered 64px green check well
//! with its 28px check glyph and 8px green ring, the heading / subtitle
//! pack entries and rhythm, the 480px summary card with its three rows
//! (the connection row's value following `connectionSummary(state)` —
//! the imported / updated count split, the created connection's name, or
//! the not-configured line — the AI row's configured-label split, the
//! fixed storage row), the skipped rows' muted check ink, the 112px label
//! cells and elided mono value cells, the sample-entry hint (💡 plus the
//! `nextHint` pack entry with its `<b>` segments stripped and bolded),
//! and the open button — the journey's only exit. Runtime evidence comes
//! from `Runtime::elements()` with the step mounted exactly as production
//! mounts it — as the wizard's S3 seat inside the T072 window — never
//! from a value the component reports about itself.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::onboarding::onboarding_window::OnboardingWindow;
use datazen_ui::views::onboarding::onboarding_wizard::{Entry, OnboardingWizard, Step};
use datazen_ui::views::onboarding::steps::done_step::{
    ConnectionSummary, DoneStep, connection_summary, imported_count,
};
use datazen_ui::views::onboarding::steps::import_step::ImportSummary;

/// The default onboarding window (960×720 — the T021/T072 audits' default).
const W: f32 = 960.0;
const H: f32 = 720.0;

/// The wizard's content column: the 264pt brand sidebar plus the 36pt
/// content padding on each side (the T021/T072 chrome).
const CONTENT_CENTER_X: f32 = 264.0 + 36.0 + (W - 264.0 - 72.0) / 2.0;

#[derive(Clone)]
struct Scene {
    step: Step,
    entry: Entry,
    dark: bool,
    import_result: Option<ImportSummary>,
    connection_name: Option<String>,
    ai_label: Option<String>,
    ai_configured: bool,
    open_fired: usize,
}

fn base() -> Scene {
    // React's fresh journey at S3: `INITIAL_STATE`'s halves (the manual
    // entry, nothing configured) — the skip path's done screen.
    Scene {
        step: Step::S3,
        entry: Entry::Manual,
        dark: true,
        import_result: None,
        connection_name: None,
        ai_label: None,
        ai_configured: false,
        open_fired: 0,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    // The step over the wizard state's feed-ins, exactly as the host
    // builds it at the S3 seat (T114's `done` slot + `on_open` seam).
    let mut step = DoneStep::<Scene>::new()
        .entry(s.entry)
        .ai_configured(s.ai_configured)
        .on_open(|s: &mut Scene| s.open_fired += 1);
    if let Some(result) = &s.import_result {
        step = step.import_result(result.clone());
    }
    if let Some(name) = &s.connection_name {
        step = step.connection_name(name.clone());
    }
    if let Some(label) = &s.ai_label {
        step = step.ai_label(label.clone());
    }
    // The production mounting: the wizard's S3 seat inside the T072
    // window (the seat itself gates on the step, exactly as React's
    // conditional).
    let wizard = OnboardingWizard::<Scene>::new().step(s.step).done(step.build(s.dark));
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

/// The translated `nextHint` with its `<b>`/`</b>` tags stripped — what
/// the port's rich hint must show after the 💡 prefix (the manual-step
/// audit's `plain_subtitle`).
fn plain_next_hint() -> String {
    t("onboarding.s3.nextHint").replace("<b>", "").replace("</b>", "")
}

// ---- the wizard's S3 mount ------------------------------------------------------

/// React: `{state.step === 's3' && <DoneStep … />}` — the step exists
/// only on S3, inside the wizard's done seat.
#[test]
fn the_step_renders_only_on_s3() {
    for step in [Step::S0, Step::S1, Step::S2] {
        let rt = rt(Scene { step, ..base() });
        assert!(!has(&rt, "onboarding-step-s3"), "{step:?} does not mount the done step");
        assert!(
            !has(&rt, "onboarding-summary-connection"),
            "{step:?} does not mount the summary rows either"
        );
    }
    {
        let rt = rt(Scene { ..base() });
        let root = maybe(&rt, "onboarding-step-s3").expect("S3 mounts the done step");
        let seat = maybe(&rt, "onboarding-wizard-step-s3").expect("the wizard's done seat");
        assert!(
            root.path.starts_with(&seat.path),
            "the step is mounted inside the wizard's S3 seat"
        );
    }
}

// ---- the centered column -----------------------------------------------------------

/// The done column centers its children (`flex flex-col items-center`):
/// the check well, the title block, the capped summary card and the open
/// button all share the content column's center line.
#[test]
fn the_done_column_centers_its_children() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        for key in [
            "onboarding-step-s3-check",
            "onboarding-step-s3-heading",
            "onboarding-step-s3-summary",
        ] {
            let el = maybe(&rt, key).unwrap_or_else(|| panic!("{key} renders"));
            assert!(
                (el.rect[0] + el.rect[2] / 2.0 - CONTENT_CENTER_X).abs() < 0.5,
                "dark={dark}: {key} centers on the column (got center {})",
                el.rect[0] + el.rect[2] / 2.0
            );
        }
        // The card caps at 480px (360pt).
        let card = maybe(&rt, "onboarding-step-s3-summary").expect("the summary card");
        assert!(
            (card.rect[2] - 360.0).abs() < 0.5,
            "dark={dark}: the card caps at 480px = 360pt, not {}",
            card.rect[2]
        );
    }
}

/// The check well: the 64px (48pt) `rounded-full` green-bordered well
/// with the 28px (21pt) check glyph in the green ink, above the heading
/// at the root's gap-6 (18pt) rhythm.
#[test]
fn the_check_well_and_the_header_pack() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let k = kit(dark);
        let well = maybe(&rt, "onboarding-step-s3-check").expect("the check well");
        assert!(
            (well.rect[2] - 48.0).abs() < 0.5 && (well.rect[3] - 48.0).abs() < 0.5,
            "dark={dark}: the well is h-16 w-16 (48pt), got {}×{}",
            well.rect[2],
            well.rect[3]
        );
        // rounded-full paints as a circle: the radius covers at least half
        // the side (the sample-step dot's sibling form; ElementInfo reports
        // the specified 999, the renderer clamps it).
        assert!(
            well.radius >= well.rect[2] / 2.0,
            "dark={dark}: rounded-full covers half the side, got {} for {}",
            well.radius,
            well.rect[2]
        );
        assert!(
            same_color(well.fill, k.c.success.fg.alpha(0.10)),
            "dark={dark}: bg-green/10 (got {:?})",
            well.fill
        );
        let glyph = icon_under(&rt, "onboarding-step-s3-check").expect("the check glyph");
        assert!(
            (glyph.rect[2] - 21.0).abs() < 0.5 && (glyph.rect[3] - 21.0).abs() < 0.5,
            "dark={dark}: the glyph is h-7 w-7 (21pt), got {}×{}",
            glyph.rect[2],
            glyph.rect[3]
        );
        assert_eq!(glyph.text_color, Some(k.c.success.fg), "dark={dark}: text-green");
        // The header pack and rhythm.
        assert_eq!(text_of(&rt, "onboarding-step-s3-title"), t("onboarding.s3.title"));
        assert_eq!(
            text_of(&rt, "onboarding-step-s3-subtitle"),
            t("onboarding.s3.subtitle")
        );
        let title = maybe(&rt, "onboarding-step-s3-title").expect("the heading");
        let subtitle = maybe(&rt, "onboarding-step-s3-subtitle").expect("the subtitle");
        assert_eq!(title.text_color, Some(k.c.text_base), "dark={dark}: text-fg");
        assert_eq!(subtitle.text_color, Some(k.c.text_muted), "dark={dark}: text-fg-secondary");
        assert!(
            (title.rect[1] - (well.rect[1] + well.rect[3]) - 18.0).abs() < 0.5,
            "dark={dark}: the heading starts gap-6 (18pt) below the well"
        );
        assert!(
            (subtitle.rect[1] - (title.rect[1] + title.rect[3]) - 7.5).abs() < 0.5,
            "dark={dark}: the subtitle starts mb-2.5 (7.5pt) below the heading"
        );
    }
}

// ---- the summary rows ---------------------------------------------------------------

/// The three rows carry the wizard state's values, in React's order
/// (connection / AI / storage), with the fixed labels, the 112px label
/// cells and the mono value cells — and the skipped rows' check ink
/// muted, the others green.
#[test]
fn the_summary_rows_carry_the_react_values() {
    for dark in [true, false] {
        let k = kit(dark);
        // The skip path: nothing configured.
        {
            let mut rt = rt(Scene { dark, ..base() });
            settle(&mut rt);
            assert_eq!(
                text_of(&rt, "onboarding-summary-connection-value"),
                t("onboarding.s3.notConfigured"),
                "no connection reads the not-configured line"
            );
            assert_eq!(
                text_of(&rt, "onboarding-summary-ai-value"),
                t("onboarding.s3.aiNotConfigured"),
                "AI unconfigured reads its optional line"
            );
            assert_eq!(
                text_of(&rt, "onboarding-summary-storage-value"),
                t("onboarding.s3.storageValue"),
                "the storage row is fixed"
            );
            // Both skipped rows: the muted check ink (text-fg-muted).
            for row in ["onboarding-summary-connection", "onboarding-summary-ai"] {
                assert_eq!(
                    maybe(&rt, &format!("{row}-check")).expect("the check").text_color,
                    Some(k.c.text_muted),
                    "dark={dark}: {row} is skipped — the muted ink"
                );
            }
            assert_eq!(
                maybe(&rt, "onboarding-summary-storage-check").expect("the check").text_color,
                Some(k.c.success.fg),
                "dark={dark}: the storage row is never skipped — the green ink"
            );
        }
        // The import path: the imported entry, then the updated one.
        {
            let mut rt = rt(Scene {
                entry: Entry::Import,
                import_result: Some(ImportSummary {
                    imported: 3,
                    overwritten: 2,
                    source: "DBeaver".to_owned(),
                }),
                ..Scene { dark, ..base() }
            });
            settle(&mut rt);
            assert_eq!(
                text_of(&rt, "onboarding-summary-connection-value"),
                t("onboarding.s3.importedValue")
                    .replace("{count}", "3")
                    .replace("{source}", "DBeaver"),
                "imported > 0 reads the imported entry"
            );
            rt.update(|s, _| {
                s.import_result = Some(ImportSummary {
                    imported: 0,
                    overwritten: 4,
                    source: "Navicat".to_owned(),
                })
            });
            settle(&mut rt);
            assert_eq!(
                text_of(&rt, "onboarding-summary-connection-value"),
                t("onboarding.s3.updatedValue")
                    .replace("{count}", "4")
                    .replace("{source}", "Navicat"),
                "imported == 0 reads the updated entry"
            );
        }
        // The created-connection path (manual / sample) and the
        // configured AI label.
        {
            let mut rt = rt(Scene {
                connection_name: Some("Warehouse".to_owned()),
                ai_label: Some("deepseek-chat".to_owned()),
                ai_configured: true,
                ..Scene { dark, ..base() }
            });
            settle(&mut rt);
            assert_eq!(
                text_of(&rt, "onboarding-summary-connection-value"),
                "Warehouse",
                "a created connection reads its name"
            );
            assert_eq!(
                text_of(&rt, "onboarding-summary-ai-value"),
                "deepseek-chat",
                "a configured provider reads its label"
            );
            for row in ["onboarding-summary-connection", "onboarding-summary-ai"] {
                assert_eq!(
                    maybe(&rt, &format!("{row}-check")).expect("the check").text_color,
                    Some(k.c.success.fg),
                    "dark={dark}: {row} is configured — the green ink"
                );
            }
        }
        // The labels and the cell layout.
        {
            let mut rt = rt(Scene { dark, ..base() });
            settle(&mut rt);
            assert_eq!(
                text_of(&rt, "onboarding-summary-connection-label"),
                t("onboarding.s3.connLabel")
            );
            assert_eq!(text_of(&rt, "onboarding-summary-ai-label"), t("onboarding.s3.aiLabel"));
            assert_eq!(
                text_of(&rt, "onboarding-summary-storage-label"),
                t("onboarding.s3.storageLabel")
            );
            let label = maybe(&rt, "onboarding-summary-connection-label").expect("the label cell");
            assert!(
                (label.rect[2] - 84.0).abs() < 0.5,
                "dark={dark}: the label cell is w-[112px] (84pt), got {}",
                label.rect[2]
            );
            let value = maybe(&rt, "onboarding-summary-connection-value").expect("the value cell");
            assert!(
                value.rect[0] > label.rect[0] + label.rect[2],
                "dark={dark}: the value cell follows the label cell"
            );
            // React's order: connection, then AI, then storage — stacked
            // with no gaps (the rows carry only their bottom borders).
            let rows = ["onboarding-summary-connection", "onboarding-summary-ai", "onboarding-summary-storage"];
            for pair in rows.windows(2) {
                let top = maybe(&rt, pair[0]).expect("a row");
                let bottom = maybe(&rt, pair[1]).expect("the next row");
                assert!(
                    (bottom.rect[1] - (top.rect[1] + top.rect[3])).abs() < 0.5,
                    "dark={dark}: {} stacks directly on {} (got a {}pt seam)",
                    pair[1],
                    pair[0],
                    bottom.rect[1] - (top.rect[1] + top.rect[3])
                );
            }
        }
    }
}

// ---- the sample hint -----------------------------------------------------------------

/// React: `state.entry === 'sample'` — the 💡 hint renders only on the
/// sample entry, carrying the `nextHint` pack entry with its `<b>`
/// segments stripped (the port bolds them as spans instead).
#[test]
fn the_sample_hint_gates_on_the_sample_entry() {
    // The manual and import entries show no hint.
    for entry in [Entry::Manual, Entry::Import] {
        let rt = rt(Scene { entry, ..base() });
        assert!(!has(&rt, "onboarding-step-s3-hint"), "{entry:?} shows no hint");
    }
    // The sample entry shows it.
    let mut rt = rt(Scene { entry: Entry::Sample, ..base() });
    settle(&mut rt);
    let hint = maybe(&rt, "onboarding-step-s3-hint").expect("the sample hint");
    assert_eq!(text_of(&rt, "onboarding-step-s3-hint"), format!("💡 {}", plain_next_hint()));
    assert_eq!(hint.text_color, Some(kit(true).c.text_accent), "text-accent");
    let rendered = text_of(&rt, "onboarding-step-s3-hint");
    assert!(!rendered.contains("<b>"), "the raw tags never reach the screen");
}

// ---- the open button -------------------------------------------------------------------

/// React: the primary button labelled `openBtn` — the journey's only
/// exit, firing `onOpen` (the host's `handleOpen`) once per press.
#[test]
fn the_open_button_fires_the_open_action() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        assert_eq!(
            text_under(&rt, "onboard-open-datazen"),
            t("onboarding.s3.openBtn"),
            "dark={dark}: the open button's label"
        );
        assert_eq!(rt.state.open_fired, 0);
        press(&mut rt, "onboard-open-datazen");
        assert_eq!(rt.state.open_fired, 1, "dark={dark}: one press, one open");
    }
}

// ---- the summary seam --------------------------------------------------------------------

/// React `connectionSummary` / `importedCount` (`wizardState.ts`): the
/// import entry with a non-zero written count reports its counts and
/// source; else a created connection reports its name; else nothing was
/// configured. Re-importing only overwrites still counts.
#[test]
fn the_connection_summary_follows_the_wizard_state() {
    let imported = ImportSummary {
        imported: 3,
        overwritten: 2,
        source: "DBeaver".to_owned(),
    };
    assert_eq!(imported_count(Some(&imported)), 5, "new plus replaced");
    assert_eq!(imported_count(None), 0, "no import, no count");
    // The import path with a written count.
    assert_eq!(
        connection_summary(Entry::Import, Some(&imported), None),
        ConnectionSummary::Imported {
            imported: 3,
            overwritten: 2,
            source: "DBeaver".to_owned(),
        }
    );
    // An overwrite-only re-import still counts (written > 0).
    let reimport = ImportSummary { imported: 0, overwritten: 4, source: "Navicat".to_owned() };
    assert_eq!(
        connection_summary(Entry::Import, Some(&reimport), Some("Old")),
        ConnectionSummary::Imported { imported: 0, overwritten: 4, source: "Navicat".to_owned() },
        "the import summary wins over a stale connection name"
    );
    // The import entry that wrote NOTHING falls through to the created
    // connection (or none), exactly as React's early-return chain.
    let empty = ImportSummary { imported: 0, overwritten: 0, source: String::new() };
    assert_eq!(
        connection_summary(Entry::Import, Some(&empty), Some("Warehouse")),
        ConnectionSummary::Connection { name: "Warehouse".to_owned() }
    );
    assert_eq!(
        connection_summary(Entry::Import, Some(&empty), None),
        ConnectionSummary::None
    );
    // The manual / sample paths read the created connection.
    assert_eq!(
        connection_summary(Entry::Manual, Some(&imported), Some("Warehouse")),
        ConnectionSummary::Connection { name: "Warehouse".to_owned() },
        "a non-import entry never reports an import"
    );
    assert_eq!(
        connection_summary(Entry::Sample, None, Some("Sample Playground")),
        ConnectionSummary::Connection { name: "Sample Playground".to_owned() }
    );
    assert_eq!(connection_summary(Entry::Manual, None, None), ConnectionSummary::None);
}

// ---- the geometry guard --------------------------------------------------------------------

/// Nothing renders outside the window horizontally, in either theme, in
/// the widest states (the imported summary, the sample hint, the long
/// created-connection name).
#[test]
fn every_part_stays_inside_the_window_horizontally() {
    for dark in [true, false] {
        for state in 0..3 {
            let mut scene = Scene { dark, ..base() };
            match state {
                0 => {
                    scene.entry = Entry::Import;
                    scene.import_result = Some(ImportSummary {
                        imported: 12,
                        overwritten: 7,
                        source: "DBeaver".to_owned(),
                    });
                }
                1 => {
                    scene.entry = Entry::Sample;
                    scene.connection_name =
                        Some("Sample Playground — a deliberately long connection name".to_owned());
                }
                _ => {
                    scene.ai_configured = true;
                    scene.ai_label = Some("a-deliberately-long-provider-label".to_owned());
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
