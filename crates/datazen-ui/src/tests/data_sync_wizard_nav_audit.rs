//! T287 audit (tester): independent checks of the wizard nav + stepper,
//! from separately-built scenes: geometry at both window sizes, the
/// light-theme build path, and the current/past/future color triple
/// observed through the text_color channel.

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_sync::data_sync_wizard_nav::{
    DataSyncWizardNav, DataSyncWizardStepper,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Default)]
struct AuditHost;

fn nav_view(_host: &AuditHost, _cx: &mut Cx<AuditHost>) -> El {
    col().w_full().h_full().child(
        DataSyncWizardNav::<AuditHost>::new(2, false, true, true)
            .on_back(|_s| {})
            .on_next(|_s| {})
            .build(true),
    )
}

#[test]
fn audit_nav_geometry_both_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(AuditHost, nav_view, (w, h), env(true));
        rt.frame(0.0);
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-sync-wizard-nav")
                && !key.starts_with("data-sync-back")
                && !key.starts_with("data-sync-next")
            {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {}", outside.join("; "));
    }
}

#[test]
fn audit_the_stepper_color_triple_through_the_text_channel() {
    // Current step (index 2): accent word. Past (0, 1): 80% accent.
    // Future (3..5): muted. Observed on the label elements' colors —
    // three distinct values, with past == alpha-blended accent and future
    // == the muted token.
    let mut rt = Runtime::new(
        (),
        |_, _| {
            col().w_full()
                .h_full()
                .child(DataSyncWizardStepper::new(2).build(true))
        },
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    let label_color = |name: &str| {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(&format!("data-sync-step-{name}-label")))
            .and_then(|e| e.text_color)
            .expect(name)
    };
    let current = label_color("objects");
    let past = label_color("setup");
    let future = label_color("result");
    // The triple is pinned to the kit's own tokens (semantic mapping, not
    // color distinctness — a theme may legitimately spell accent and muted
    // alike, but the mapping must still be exact).
    let k = kit_dark();
    assert_eq!(current, k.c.text_accent, "the current step reads the accent token");
    assert_eq!(past, k.alpha(k.c.text_accent, 0.8), "a past step reads the 80% accent");
    assert_eq!(future, k.c.text_muted, "a future step reads the muted token");
}

fn kit_dark() -> &'static rune_kit::Kit {
    crate::ui::kit::kit(true)
}

#[test]
fn audit_the_light_theme_builds_both_components() {
    // Both components build through the light kit without panic.
    let mut nav = Runtime::new(
        AuditHost,
        |_, _| {
            col().w_full().h_full().child(
                DataSyncWizardNav::<AuditHost>::new(1, false, true, true)
                    .on_back(|_s| {})
                    .on_next(|_s| {})
                    .build(false),
            )
        },
        (960.0, 640.0),
        env(false),
    );
    nav.frame(0.0);
    assert!(nav
        .elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some("data-sync-back")));

    let mut stepper = Runtime::new(
        (),
        |_, _| {
            col().w_full()
                .h_full()
                .child(DataSyncWizardStepper::new(0).build(false))
        },
        (960.0, 640.0),
        env(false),
    );
    stepper.frame(0.0);
    assert!(stepper
        .elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some("data-sync-step-endpoints")));
}
