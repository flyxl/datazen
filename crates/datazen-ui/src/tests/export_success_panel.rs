//! T014: the export success panel renders React's centred green check
//! over the caller's message, at the same geometry as the batch dialog's
//! inline twin, in both themes.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::export::ExportSuccessPanel;
use crate::i18n::{has, t};
use crate::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

fn view(state: &Probe, cx: &mut Cx<Probe>) -> El {
    let dark = cx.system_dark();
    col()
        .key("page")
        .w_full()
        .h_full()
        .padding(PAD)
        .child(ExportSuccessPanel::new("export-success-panel", state.message.clone()).build(dark))
}

struct Probe {
    message: String,
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

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key}: is laid out"))
}

#[test]
fn the_panel_keeps_the_react_geometry_in_both_themes() {
    for dark in [true, false] {
        let rt = runtime(t("export.success"), DESIGN, dark);
        let root = element(&rt, "export-success-panel");
        let icon = element(&rt, "export-success-panel-icon");
        let message = element(&rt, "export-success-panel-message");

        // React: `py-6` (24pt) above and below, `gap-2` (8pt) between the
        // 48pt circle and the message; the column is `items-center`.
        assert_eq!(root.rect[0], PAD, "the panel spans the page width");
        assert_eq!(root.rect[2], DESIGN.0 - 2.0 * PAD, "w_full inside the pad");
        assert_eq!(icon.rect[2], 48.0, "React h-12 w-12 circle width");
        assert_eq!(icon.rect[3], 48.0, "React h-12 w-12 circle height");
        assert_eq!(icon.radius, 24.0, "rounded-full");
        assert!(
            (icon.rect[1] - (root.rect[1] + 24.0)).abs() <= 0.5,
            "py-6 above the circle: {:?} in {:?}",
            icon.rect,
            root.rect
        );
        assert!(
            (message.rect[1] - (icon.rect[1] + icon.rect[3]) - 8.0).abs() <= 0.5,
            "gap-2 between circle and message: {:?} then {:?}",
            icon.rect,
            message.rect
        );
        assert!(
            (message.rect[1] + message.rect[3] - (root.rect[1] + root.rect[3]) + 24.0).abs() <= 0.5,
            "py-6 below the message: {:?} in {:?}",
            message.rect,
            root.rect
        );
        let root_cx = root.rect[0] + root.rect[2] / 2.0;
        assert!(
            ((icon.rect[0] + icon.rect[2] / 2.0) - root_cx).abs() <= 0.5,
            "the circle is centred: {:?} in {:?}",
            icon.rect,
            root.rect
        );
        assert!(
            ((message.rect[0] + message.rect[2] / 2.0) - root_cx).abs() <= 0.5,
            "the message is centred: {:?} in {:?}",
            message.rect,
            root.rect
        );

        // The check itself: 24pt, inside the circle, in the success ink.
        let k = kit(dark);
        let icon_path = format!("{}/icon", icon.path);
        let check = rt
            .elements()
            .into_iter()
            .find(|e| e.path == icon_path)
            .unwrap_or_else(|| panic!("{}: the check is laid out", icon_path));
        assert_eq!(check.rect[2], 24.0, "React h-6 w-6 check width");
        assert_eq!(check.rect[3], 24.0, "React h-6 w-6 check height");
        assert!(
            check.rect[0] >= icon.rect[0]
                && check.rect[1] >= icon.rect[1]
                && check.rect[0] + check.rect[2] <= icon.rect[0] + icon.rect[2] + 0.5
                && check.rect[1] + check.rect[3] <= icon.rect[1] + icon.rect[3] + 0.5,
            "the check sits inside its circle: {:?} in {:?}",
            check.rect,
            icon.rect
        );
        assert_eq!(
            check.text_color,
            Some(k.c.success.fg),
            "React text-green-400 on the check"
        );
        assert_eq!(
            icon.fill,
            k.alpha(k.c.success.fg, 0.15),
            "React bg-green-500/15 on the circle"
        );
        assert_eq!(
            message.text.as_deref(),
            Some(t("export.success")),
            "the message renders verbatim"
        );
        assert_eq!(
            message.text_color,
            Some(k.c.text_base),
            "React text-fg on the message"
        );
    }
}

#[test]
fn the_panel_shows_the_message_it_is_given() {
    // The panel takes a resolved string, not a key: both React callers pass
    // `t('export.success')`, and any other message must render as-is.
    assert!(
        has("export.success"),
        "the callers' key exists in the locale"
    );
    let rt = runtime("Table exported to /tmp/out.csv", MIN, true);
    assert_eq!(
        element(&rt, "export-success-panel-message").text.as_deref(),
        Some("Table exported to /tmp/out.csv"),
        "a custom message is not re-keyed"
    );
    assert_eq!(
        element(&rt, "export-success-panel-icon").rect[2],
        48.0,
        "the geometry does not depend on the message"
    );
}
