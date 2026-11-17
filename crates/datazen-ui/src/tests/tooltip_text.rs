//! T0-11 patch: a compact tooltip's long single-line hint stays inside the window.
//!
//! The kit's Compact appearance leaves the text width uncapped, so a hint long enough to
//! outgrow the page laid out unwrapped and left the window. The wrapper now hands the kit a
//! style whose measure matches the Standard tooltip's 320pt; these tests pin that on the
//! runtime's own geometry: the long hint wraps, a short hint stays one line, and the five-line
//! child scene still flips above its trigger.

use rune::{col, row, text, Cx, El, Env, Runtime};
use rune_kit::Placement;

use crate::ui::{kit::kit, tooltip};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

/// About 250 characters of ordinary prose, one line in the source.
const LONG: &str = "A long hint that keeps talking well past the edge of a narrow window and \
must therefore wrap instead of running out of the page: it names the command, the scope it \
applies to, the shortcut that opens it elsewhere, and the one caveat worth remembering before \
the first press.";

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone, Copy, Default)]
enum Content {
    #[default]
    Long,
    Short,
    FiveLines,
}

struct Probe {
    content: Content,
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let trigger = row()
        .key("tip-trigger")
        .size(120.0, 28.0)
        .center()
        .child(text("Hover"));
    let tooltip = tooltip::Tooltip::new("tip")
        .trigger(trigger)
        .open(true)
        .placement(Placement::BottomStart);
    let tooltip = match state.content {
        Content::Long => tooltip.text(LONG),
        Content::Short => tooltip.text("Save changes"),
        Content::FiveLines => tooltip
            .child(
                col()
                    .key("tip-body")
                    .gap(2.0)
                    .children((0..5).map(|line| text(format!("tooltip line {line}")))),
            )
            .label("More info"),
    };
    let built = tooltip.build(k, &mut layer);
    let anchored = col()
        .key("tip-anchor")
        .pin(None, None, Some(24.0), Some(24.0))
        .child(built);
    rune_kit::root(col().w_full().h_full().child(anchored), layer)
}

fn rect(rt: &Runtime<Probe>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect
}

fn mounted(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn runtime(content: Content, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(Probe { content }, view, size, env());
    rt.frame(0.0);
    rt
}

fn assert_inside(rt: &Runtime<Probe>, (w, h): (f32, f32)) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let [x, y, ew, eh] = e.rect;
        if x < -0.5 || y < -0.5 || x + ew > w + 0.5 || y + eh > h + 0.5 {
            outside.push(format!(
                "{} {:?}",
                e.key.as_deref().unwrap_or(&e.path),
                e.rect
            ));
        }
    }
    assert!(
        outside.is_empty(),
        "{} tooltip node(s) cross the {w}x{h} window: {outside:?}",
        outside.len()
    );
}

#[test]
fn a_long_single_line_hint_wraps_inside_both_windows() {
    for size in [MIN, DESIGN] {
        let rt = runtime(Content::Long, size);
        let tip = rect(&rt, "tip-tooltip");
        assert!(
            tip[0] >= -0.5 && tip[0] + tip[2] <= size.0 + 0.5,
            "{size:?}: the long hint leaves the window horizontally: {tip:?}"
        );
        assert!(
            tip[2] <= 320.5,
            "{size:?}: the hint is wider than the 320pt measure: {tip:?}"
        );
        assert!(
            tip[3] > 22.5,
            "{size:?}: the long hint did not wrap onto a second line: {tip:?}"
        );
        assert_inside(&rt, size);
    }
}

#[test]
fn a_short_hint_still_takes_one_line() {
    for size in [MIN, DESIGN] {
        let rt = runtime(Content::Short, size);
        let tip = rect(&rt, "tip-tooltip");
        assert!(
            tip[3] < 30.5,
            "{size:?}: a short hint grew to {:?}, past one padded line",
            tip
        );
        assert!(
            tip[2] < 160.0,
            "{size:?}: a short hint took the full measure: {tip:?}"
        );
        assert_inside(&rt, size);
    }
}

#[test]
fn the_five_line_child_scene_still_flips_above_its_trigger() {
    for size in [MIN, DESIGN] {
        let rt = runtime(Content::FiveLines, size);
        assert!(
            mounted(&rt, "tip-body"),
            "{size:?}: the child body is mounted"
        );
        let tip = rect(&rt, "tip-tooltip");
        let trigger = rect(&rt, "tip-trigger");
        assert!(
            tip[1] + tip[3] <= trigger[1] + 0.5,
            "{size:?}: tooltip {tip:?} was not flipped above trigger {trigger:?}"
        );
        assert_inside(&rt, size);
    }
}
