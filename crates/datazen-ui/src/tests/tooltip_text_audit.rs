//! T0-11 audit: the compact tooltip's 320pt measure, real text wrapping, and
//! all four anchor directions.

use rune::{col, row, text, Cx, El, Env, Runtime};
use rune_kit::Placement;

use crate::ui::{kit::kit, tooltip};

const LONG: &str = "A long hint that keeps talking well past the edge of a narrow window and \
must therefore wrap instead of running out of the page: it names the command, the scope it \
applies to, the shortcut that opens it elsewhere, and the one caveat worth remembering before \
the first press.";
const SHORT: &str = "Save changes";
const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

struct Probe {
    text: &'static str,
    placement: Placement,
    corner: Corner,
}

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let k = kit(true);
    let mut layer = Vec::new();
    let trigger = row()
        .key("tip-trigger")
        .size(120.0, 28.0)
        .center()
        .child(text("Hover"));
    let built = tooltip::Tooltip::new("tip")
        .trigger(trigger)
        .text(state.text)
        .open(true)
        .placement(state.placement)
        .build(k, &mut layer);
    let anchor = match state.corner {
        Corner::TopLeft => col()
            .key("tip-anchor")
            .pin(Some(24.0), Some(24.0), None, None),
        Corner::TopRight => col()
            .key("tip-anchor")
            .pin(None, Some(24.0), Some(24.0), None),
        Corner::BottomLeft => col()
            .key("tip-anchor")
            .pin(Some(24.0), None, None, Some(24.0)),
        Corner::BottomRight => col()
            .key("tip-anchor")
            .pin(None, None, Some(24.0), Some(24.0)),
    }
    .child(built);
    rune_kit::root(col().w_full().h_full().child(anchor), layer)
}

fn runtime(state: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(state, view, size, env());
    rt.frame(0.0);
    rt.settle_all();
    rt
}

fn rect(rt: &Runtime<Probe>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
        .rect
}

fn text_rect(rt: &Runtime<Probe>, prefix: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.role == "text" && e.text.as_deref().is_some_and(|t| t.starts_with(prefix)))
        .unwrap_or_else(|| panic!("text starting {prefix:?} is laid out"))
        .rect
}

fn inside(rt: &Runtime<Probe>, (w, h): (f32, f32)) -> Vec<String> {
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
    outside
}

fn long(corner: Corner, placement: Placement, size: (f32, f32)) -> Runtime<Probe> {
    runtime(
        Probe {
            text: LONG,
            placement,
            corner,
        },
        size,
    )
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0
}

#[test]
fn the_long_hint_wraps_into_multiple_lines_and_obeys_the_320pt_measure() {
    for size in [MIN, DESIGN] {
        let rt = long(Corner::BottomRight, Placement::BottomStart, size);
        let tip = rect(&rt, "tip-tooltip");
        let body = text_rect(&rt, "A long hint");
        assert!(
            (300.0..=320.5).contains(&tip[2]),
            "{size:?}: the long hint must use the 320pt measure, got {tip:?}"
        );
        assert!(
            body[3] > 48.0,
            "{size:?}: the text box is {body:?}; one line is ~12pt, so this would be a clipped single line, not a wrap"
        );
        assert!(
            body[1] >= tip[1] && body[1] + body[3] <= tip[1] + tip[3],
            "{size:?}: the wrapped text {body:?} is clipped by its tooltip {tip:?}"
        );
        assert!(
            tip[3] > body[3],
            "{size:?}: the panel {tip:?} did not grow around the text {body:?}"
        );
        let short = runtime(
            Probe {
                text: SHORT,
                placement: Placement::BottomStart,
                corner: Corner::BottomRight,
            },
            size,
        );
        let short_body = text_rect(&short, "Save changes");
        assert!(
            body[3] >= short_body[3] * 4.0,
            "{size:?}: long text {body:?} vs one-line {short_body:?} — not multiple line boxes"
        );
        let outside = inside(&rt, size);
        assert!(outside.is_empty(), "{size:?}: {outside:?}");
    }
}

#[test]
fn a_short_hint_stays_on_one_line() {
    for size in [MIN, DESIGN] {
        let rt = runtime(
            Probe {
                text: SHORT,
                placement: Placement::BottomStart,
                corner: Corner::BottomRight,
            },
            size,
        );
        let tip = rect(&rt, "tip-tooltip");
        let body = text_rect(&rt, "Save changes");
        assert!(body[3] < 20.0, "{size:?}: short text {body:?} wrapped");
        assert!(tip[3] < 30.0, "{size:?}: short tooltip {tip:?} grew");
        assert!(
            tip[2] < 160.0,
            "{size:?}: short tooltip {tip:?} took the measure"
        );
        let outside = inside(&rt, size);
        assert!(outside.is_empty(), "{size:?}: {outside:?}");
    }
}

#[test]
fn all_four_anchor_directions_keep_their_side_and_alignment() {
    let cases = [
        (Corner::TopLeft, Placement::BottomStart, false, true),
        (Corner::TopRight, Placement::BottomEnd, false, false),
        (Corner::BottomLeft, Placement::TopStart, true, true),
        (Corner::BottomRight, Placement::TopEnd, true, false),
    ];
    for size in [MIN, DESIGN] {
        for (corner, placement, above, start) in cases {
            let rt = long(corner, placement, size);
            let tip = rect(&rt, "tip-tooltip");
            let trigger = rect(&rt, "tip-trigger");
            if above {
                assert!(
                    tip[1] + tip[3] <= trigger[1] - 3.5,
                    "{size:?} {placement:?}: tip {tip:?} is not above trigger {trigger:?}"
                );
            } else {
                assert!(
                    tip[1] >= trigger[1] + trigger[3] + 3.5,
                    "{size:?} {placement:?}: tip {tip:?} is not below trigger {trigger:?}"
                );
            }
            if start {
                assert!(
                    near(tip[0], trigger[0]),
                    "{size:?} {placement:?}: tip {tip:?} lost start alignment with {trigger:?}"
                );
            } else {
                assert!(
                    near(tip[0] + tip[2], trigger[0] + trigger[2]),
                    "{size:?} {placement:?}: tip {tip:?} lost end alignment with {trigger:?}"
                );
            }
            let outside = inside(&rt, size);
            assert!(outside.is_empty(), "{size:?} {placement:?}: {outside:?}");
        }
    }
}

#[test]
fn every_placement_and_corner_stays_inside_the_window() {
    let placements = [
        Placement::TopStart,
        Placement::TopEnd,
        Placement::BottomStart,
        Placement::BottomEnd,
    ];
    let corners = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];
    for size in [MIN, DESIGN] {
        for placement in placements {
            for corner in corners {
                let rt = long(corner, placement, size);
                let outside = inside(&rt, size);
                assert!(
                    outside.is_empty(),
                    "{size:?} {placement:?} {corner:?}: {outside:?}"
                );
            }
        }
    }
}
