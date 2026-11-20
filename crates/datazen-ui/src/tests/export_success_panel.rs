//! T014: the `ExportSuccessPanel` port.
//!
//! Every observation comes from `rt.elements()` — measured rects, the message
//! element's text and the paint channel — never from a value the component
//! reports about itself. The probe mounts the pane in a window-sized host, as
//! an export dialog would.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::export::{
    ExportSuccessPanel, BADGE_KEY, ICON_KEY, MESSAGE_KEY, PANEL_KEY,
};
use crate::ui::kit::kit;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

struct Probe {
    message: String,
    dark: bool,
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    col()
        .key("host")
        .w_full()
        .h_full()
        .child(ExportSuccessPanel::new(s.message.clone()).build(s.dark))
}

fn runtime(message: &str, size: (f32, f32), dark: bool) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe { message: message.to_owned(), dark },
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn center(r: [f32; 4]) -> (f32, f32) {
    (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5)
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: out of window: {outside:?}");
}

#[test]
fn the_check_glyph_is_24pt_and_centered_in_the_48pt_wash_circle() {
    let rt = runtime("导出完成", DESIGN, true);
    let badge = element(&rt, BADGE_KEY).rect;
    let icon = element(&rt, ICON_KEY).rect;
    assert_eq!((badge[2], badge[3]), (48.0, 48.0), "React h-12 w-12 badge");
    assert_eq!((icon[2], icon[3]), (24.0, 24.0), "React h-6 w-6 check");
    let (bx, by) = center(badge);
    let (ix, iy) = center(icon);
    assert!((ix - bx).abs() < 0.5 && (iy - by).abs() < 0.5, "check centred in badge: {icon:?} vs {badge:?}");
}

#[test]
fn the_message_line_shows_the_callers_text_through_the_paint_channel() {
    let rt = runtime("已导出 12 行到 users.csv", DESIGN, true);
    let msg = element(&rt, MESSAGE_KEY);
    assert_eq!(
        msg.text.as_deref(),
        Some("已导出 12 行到 users.csv"),
        "the message is the caller's text, verbatim"
    );
    assert_eq!(msg.text_color, Some(kit(true).c.text_base), "React text-fg");
    let badge = element(&rt, BADGE_KEY);
    assert_eq!(
        badge.fill,
        kit(true).alpha(kit(true).c.success.fg, 0.15),
        "React bg-green-500/15 via the kit success token"
    );
}

#[test]
fn badge_and_message_share_the_panes_horizontal_center_line() {
    let rt = runtime("done", DESIGN, false);
    let root = element(&rt, PANEL_KEY).rect;
    let badge = element(&rt, BADGE_KEY).rect;
    let message = element(&rt, MESSAGE_KEY).rect;
    let (root_x, _) = center(root);
    let (badge_x, _) = center(badge);
    let (message_x, _) = center(message);
    assert!((badge_x - root_x).abs() < 0.5, "badge centred: {badge_x} vs {root_x}");
    assert!((message_x - root_x).abs() < 0.5, "message centred: {message_x} vs {root_x}");
    assert!(badge[1] + badge[3] <= message[1] + 0.5, "React gap stacks icon above message");
}

#[test]
fn both_window_sizes_and_themes_keep_every_node_inside() {
    for (size, dark) in [(MIN, true), (MIN, false), (DESIGN, true), (DESIGN, false)] {
        let rt = runtime("导出完成，文件已保存", size, dark);
        assert_in_window(&rt, size.0, size.1, &format!("{size:?} dark={dark}"));
        assert!(rt.find(PANEL_KEY).is_some(), "the pane is mounted");
    }
}
