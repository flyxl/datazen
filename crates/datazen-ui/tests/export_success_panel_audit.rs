//! T014 independent audit: fresh vectors for the export success panel.
//!
//! Deliberately different from the coder's `export_success_panel.rs`:
//! minimum window + light theme, two panels sharing one view (key
//! namespacing), an empty message, a 200-char message, and the total-height
//! arithmetic that pins the panel to the batch dialog's inline twin
//! (`py(24) + 48 + gap(8) + message + py(24)`).

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::export::ExportSuccessPanel;
use datazen_ui::i18n::{has, t};
use datazen_ui::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

struct Probe {
    message: String,
}

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    col()
        .key("page")
        .w_full()
        .h_full()
        .padding(PAD)
        .child(ExportSuccessPanel::new("audit-panel", state.message.clone()).build(dark))
}

fn runtime(message: &str, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe {
            message: message.to_string(),
        },
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element<S: 'static>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

fn assert_inside_window<S: 'static>(rt: &Runtime<S>, size: (f32, f32)) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        {
            outside.push(format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), r));
        }
    }
    assert!(outside.is_empty(), "nothing crosses the window: {outside:?}");
}

#[test]
fn audit_exact_zh_text_and_geometry_at_min_light() {
    // The key resolves to the literal zh-CN string in the React pack
    // (`packages/frontend/src/locales/zh-CN/query.ts:760`), not just `has`.
    assert!(has("export.success"), "export.success exists in the pack");
    assert_eq!(t("export.success"), "导出成功", "the exact zh-CN text");
    let rt = runtime(t("export.success"), MIN, false);
    let k = kit(false);
    let root = element(&rt, "audit-panel");
    let icon = element(&rt, "audit-panel-icon");
    let message = element(&rt, "audit-panel-message");

    assert_eq!(root.rect[0], PAD, "panel spans the page width");
    assert_eq!(root.rect[2], MIN.0 - 2.0 * PAD, "w_full inside the pad");
    assert_eq!(icon.rect[2], 48.0, "React h-12 w-12 circle width");
    assert_eq!(icon.rect[3], 48.0, "React h-12 w-12 circle height");
    assert_eq!(icon.radius, 24.0, "rounded-full");
    assert_eq!(icon.fill, k.alpha(k.c.success.fg, 0.15), "bg-green-500/15");
    assert_eq!(
        message.text.as_deref(),
        Some("导出成功"),
        "the resolved message renders verbatim"
    );
    assert_eq!(message.text_color, Some(k.c.text_base), "text-fg");
    // Total height pins the panel to the batch dialog's inline twin:
    // py(24) + 48pt circle + gap(8) + message + py(24).
    let expected = 24.0 + 48.0 + 8.0 + message.rect[3] + 24.0;
    assert!(
        (root.rect[3] - expected).abs() <= 0.5,
        "py-6 + circle + gap-2 + message + py-6: root {:?} vs {expected}",
        root.rect
    );
    assert_inside_window(&rt, MIN);
}

#[test]
fn audit_two_panels_with_different_keys_do_not_collide() {
    struct Two {
        first: String,
        second: String,
    }
    fn two_view(state: &Two, cx: &mut Cx<Two>) -> El {
        let dark = cx.system_dark();
        col()
            .key("page")
            .w_full()
            .h_full()
            .padding(PAD)
            .child(
                ExportSuccessPanel::new("audit-first", state.first.clone()).build(dark),
            )
            .child(
                ExportSuccessPanel::new("audit-second", state.second.clone()).build(dark),
            )
    }
    let mut rt = Runtime::new(
        Two {
            first: "导出成功".to_string(),
            second: "Table exported to /tmp/out.csv".to_string(),
        },
        two_view,
        DESIGN,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    let first_icon = element(&rt, "audit-first-icon");
    let second_icon = element(&rt, "audit-second-icon");
    assert_eq!(
        element(&rt, "audit-first-message").text.as_deref(),
        Some("导出成功"),
        "first panel keeps its own message"
    );
    assert_eq!(
        element(&rt, "audit-second-message").text.as_deref(),
        Some("Table exported to /tmp/out.csv"),
        "second panel keeps its own message"
    );
    assert!(
        second_icon.rect[1] > first_icon.rect[1] + first_icon.rect[3],
        "the second panel lays out below the first: {:?} then {:?}",
        first_icon.rect,
        second_icon.rect
    );
    assert_eq!(first_icon.rect[2], 48.0, "first circle keeps its size");
    assert_eq!(second_icon.rect[2], 48.0, "second circle keeps its size");
    assert_inside_window(&rt, DESIGN);
}

#[test]
fn audit_empty_message_still_renders_the_circle() {
    let rt = runtime("", MIN, true);
    let icon = element(&rt, "audit-panel-icon");
    let message = element(&rt, "audit-panel-message");
    assert_eq!(icon.rect[2], 48.0, "the circle does not depend on the message");
    assert_eq!(icon.rect[3], 48.0, "the circle does not depend on the message");
    assert_eq!(
        message.text.as_deref(),
        Some(""),
        "an empty message renders empty, not the key"
    );
    assert_inside_window(&rt, MIN);
}

#[test]
fn audit_long_message_stays_inside_the_min_window() {
    // A realistic long message — a full export path, ~55 chars — stays
    // inside the minimum window, centred, with the circle intact.
    let message = "Table exported to /tmp/very/long/directory/name/out.csv";
    let rt = runtime(message, MIN, true);
    let icon = element(&rt, "audit-panel-icon");
    let rendered = element(&rt, "audit-panel-message");
    assert_eq!(icon.rect[2], 48.0, "the circle keeps its size under a long message");
    assert_eq!(
        rendered.text.as_deref(),
        Some(message),
        "the long message is not re-keyed or cut in the model"
    );
    assert_inside_window(&rt, MIN);
}

#[test]
fn audit_very_long_message_overflow_is_a_known_gap() {
    // NON-BLOCKING gap, pinned so it stays visible: a ~200-char message
    // overflows the window horizontally (the text lays out at its natural
    // 1355pt width, centred, so x goes negative). React wraps the span
    // inside its `flex-col items-center` parent; Rune's `text()` here has
    // no wrap, so the panel does not. Real inputs are short — both React
    // callers pass `t('export.success')` (4 chars zh-CN, 29 en) — so this
    // only bites on caller-supplied prose far outside the contract.
    let long = "导出成功 · ".repeat(20);
    let rt = runtime(&long, MIN, true);
    let message = element(&rt, "audit-panel-message");
    assert!(
        message.rect[0] < -0.5 || message.rect[0] + message.rect[2] > MIN.0 + 0.5,
        "a 200-char message overflows the 960pt window (known gap): {:?}",
        message.rect
    );
}

#[test]
fn audit_check_sits_centred_inside_the_circle_in_both_themes() {
    for dark in [true, false] {
        let rt = runtime(t("export.success"), DESIGN, dark);
        let k = kit(dark);
        let icon = element(&rt, "audit-panel-icon");
        let icon_path = format!("{}/icon", icon.path);
        let check = rt
            .elements()
            .into_iter()
            .find(|e| e.path == icon_path)
            .unwrap_or_else(|| panic!("{}: the check is laid out", icon_path));
        assert_eq!(check.rect[2], 24.0, "React h-6 w-6 check width");
        assert_eq!(check.rect[3], 24.0, "React h-6 w-6 check height");
        let cx = icon.rect[0] + icon.rect[2] / 2.0;
        let cy = icon.rect[1] + icon.rect[3] / 2.0;
        assert!(
            ((check.rect[0] + check.rect[2] / 2.0) - cx).abs() <= 0.5
                && ((check.rect[1] + check.rect[3] / 2.0) - cy).abs() <= 0.5,
            "the check is centred in its circle: {:?} in {:?}",
            check.rect,
            icon.rect
        );
        assert_eq!(
            check.text_color,
            Some(k.c.success.fg),
            "React text-green-400 on the check"
        );
        // The registry draws the check at stroke-width 2 (React inline svg
        // uses 2.5): the path itself must be React's exact `M20 6 9 17l-5-5`.
        assert!(
            datazen_ui::icons::ICON_CHECK.body.contains("M20 6 9 17l-5-5"),
            "ICON_CHECK carries React's exact check path"
        );
    }
}
