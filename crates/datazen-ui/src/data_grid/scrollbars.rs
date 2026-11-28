//! Non-overlay scrollbars for the two live database grids. Rune owns the
//! viewport; the strips consume available layout only on overflowing axes.
use std::{cell::Cell, rc::Rc, time::Duration};

use rune::{el, Color, Drag, El, Live, Phase, ScrollPos, Tx, WheelEvent};

const STRIP: f32 = 9.0;
const INSET: f32 = 2.0;
const INK_WIDTH: f32 = STRIP - 2.0 * INSET;
const MIN_THUMB: f32 = 24.0;
const PAGE_BACK: f32 = -1.0;
const PAGE_FORWARD: f32 = -2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Gutters {
    x: bool,
    y: bool,
}

pub(super) fn area_key(viewport_key: &str) -> String {
    format!("{viewport_key}-scroll-area")
}

impl Gutters {
    pub(super) fn for_content(frame: Option<[f32; 2]>, [content_w, content_h]: [f32; 2]) -> Self {
        let Some([frame_w, frame_h]) = frame else { return Self::default() };
        let x = content_w > frame_w;
        let y = content_h > frame_h;
        // Either visible bar can introduce overflow on the other axis. One
        // promotion is sufficient since both bars only reduce available room.
        Self {
            x: x || (y && content_w > (frame_w - STRIP).max(0.0)),
            y: y || (x && content_h > (frame_h - STRIP).max(0.0)),
        }
    }

    pub(super) fn right(self) -> f32 {
        if self.y { STRIP } else { 0.0 }
    }

    fn bottom(self) -> f32 {
        if self.x { STRIP } else { 0.0 }
    }
}

#[derive(Clone, Copy)]
enum Axis { X, Y }

impl Axis {
    fn index(self) -> usize {
        match self { Self::X => 0, Self::Y => 1 }
    }

    fn point(self, drag: &Drag, at: ScrollPos) -> f32 {
        match self {
            Self::X if at.direction.is_rtl() => at.drawn[0] - drag.x,
            Self::X => drag.x,
            Self::Y => drag.y,
        }
    }

    fn set(self, key: &str, value: f32, tx: &mut Tx) {
        match self {
            Self::X => tx.scroll_to_x(key, value),
            Self::Y => tx.scroll_to_y(key, value),
        }
    }

    fn page(self, key: &str, at: ScrollPos, direction: i32, tx: &mut Tx) {
        match self {
            Self::X => {
                let next = at.offset[0] + direction as f32 * at.viewport[0] * 0.9;
                tx.scroll_to_x(key, next);
            }
            Self::Y => tx.scroll_page(key, direction),
        }
    }
}

#[derive(Clone, Copy)]
struct Thumb {
    start: f32,
    len: f32,
    travel: f32,
}

impl Thumb {
    fn for_view(axis: Axis, at: ScrollPos) -> Option<Self> {
        let i = axis.index();
        let length = at.drawn[i];
        let max = at.max[i];
        if max <= 0.0 || length <= 2.0 * INSET {
            return None;
        }
        let len = (length * length / (at.viewport[i] + max))
            .round().max(MIN_THUMB).min(length);
        let travel = length - len;
        let start = travel * (at.offset[i] / max).clamp(0.0, 1.0);
        Some(Self { start, len, travel })
    }
}

#[derive(Clone)]
struct Track {
    bounds: Live<[f32; 4]>,
    thumb: Live<[f32; 4]>,
}

impl Track {
    fn new() -> Self {
        Self { bounds: Live::new([0.0; 4]), thumb: Live::new([0.0; 4]) }
    }

    fn update(&self, axis: Axis, at: ScrollPos, gutters: Gutters) {
        let [width, height] = at.drawn;
        let rtl = at.direction.is_rtl();
        let enabled = match axis { Axis::X => gutters.x, Axis::Y => gutters.y };
        let Some(thumb) = enabled.then(|| Thumb::for_view(axis, at)).flatten() else {
            self.bounds.snap([0.0; 4]);
            self.thumb.snap([0.0; 4]);
            return;
        };
        let ink_len = thumb.len - 2.0 * INSET;
        match axis {
            Axis::X => {
                let x = if rtl { gutters.right() } else { 0.0 };
                let ink = if rtl { width - thumb.start - thumb.len } else { thumb.start };
                self.bounds.snap([x, height, width, STRIP]);
                self.thumb.snap([ink + INSET, INSET, ink_len, INK_WIDTH]);
            }
            Axis::Y => {
                self.bounds.snap([if rtl { 0.0 } else { width }, 0.0, STRIP, height]);
                self.thumb.snap([INSET, thumb.start + INSET, INK_WIDTH, ink_len]);
            }
        }
    }

    fn view<S: 'static>(self, axis: Axis, key: &str, at: Rc<Cell<ScrollPos>>,
                       ink: Color, hover: Color) -> El {
        let track_key = match axis {
            Axis::X => "horizontal-scrollbar",
            Axis::Y => "vertical-scrollbar",
        };
        let (drag_key, wheel_key) = (key.to_owned(), key.to_owned());
        let observed = at.clone();
        let jumps = rune::system_scroller_jumps();
        el().key(track_key)
            .absolute(0.0, 0.0)
            .live_rect(&self.bounds)
            .a11y_hidden() // the real viewport exposes scroll actions
            .on_drag_any(move |drag, tx| drag_track(axis, &drag_key, at.get(), jumps, drag, tx))
            .on_wheel(move |_: &mut S, wheel, tx| wheel_over_track(&wheel_key, observed.get(), wheel, tx))
            .child(el().key("thumb").absolute(0.0, 0.0).live_rect(&self.thumb)
                .radius(INK_WIDTH / 2.0).fill(ink).hover(hover))
    }
}

pub(super) fn attach<S: 'static>(
    viewport: El, gutters: Gutters, well: Color, ink: Color, hover: Color,
) -> El {
    let (area, key, outer, viewport) = viewport.wrap_scroll_area((file!(), line!(), column!()));
    let target = format!("{area}/{key}");
    let at = Rc::new(Cell::new(ScrollPos::default()));
    let (horizontal, vertical) = (Track::new(), Track::new());
    let watch = (at.clone(), horizontal.clone(), vertical.clone());
    outer
        .pr(gutters.right()).pb(gutters.bottom()).radius(8.0).fill(well)
        .child(viewport.chain_scroll_offset(move |pos| {
            watch.0.set(pos);
            watch.1.update(Axis::X, pos, gutters);
            watch.2.update(Axis::Y, pos, gutters);
        }))
        .child(horizontal.view::<S>(Axis::X, &target, at.clone(), ink, hover))
        .child(vertical.view::<S>(Axis::Y, &target, at, ink, hover))
}

fn wheel_over_track(key: &str, at: ScrollPos, wheel: WheelEvent, tx: &mut Tx) -> bool {
    // The physical well sits beside the viewport, so a wheel over it would
    // otherwise miss the real viewport. The ordinary surface still uses
    // Rune's default routing (including its wheel gesture/axis lock).
    if wheel.dx.abs() > wheel.dy.abs() && at.max[0] > 0.0 {
        let delta = if at.direction.is_rtl() { wheel.dx } else { -wheel.dx };
        tx.live_only();
        tx.scroll_to_x(key, at.offset[0] + delta);
        true
    } else if wheel.dy != 0.0 && at.max[1] > 0.0 {
        tx.live_only();
        tx.scroll_to_y(key, at.offset[1] - wheel.dy);
        true
    } else {
        false
    }
}

fn drag_track(axis: Axis, key: &str, at: ScrollPos, jumps: bool, drag: &mut Drag, tx: &mut Tx) {
    tx.live_only();
    let Some(thumb) = Thumb::for_view(axis, at) else { return };
    let point = axis.point(drag, at);
    match drag.phase {
        Phase::Down => {
            if (thumb.start..=thumb.start + thumb.len).contains(&point) {
                drag.data = point - thumb.start;
                return;
            }
            if jumps == tx.mods().alt {
                let before = point < thumb.start;
                drag.data = if before { PAGE_BACK } else { PAGE_FORWARD };
                axis.page(key, at, if before { -1 } else { 1 }, tx);
                tx.drag_repeat_after(Duration::from_millis(300));
                return;
            }
            drag.data = thumb.len / 2.0;
        }
        Phase::Up => return,
        Phase::Move if drag.data < 0.0 => {
            if tx.drag_repeat() {
                tx.drag_repeat_after(Duration::from_millis(60));
                if drag.data == PAGE_BACK && point < thumb.start {
                    axis.page(key, at, -1, tx);
                } else if drag.data == PAGE_FORWARD && point > thumb.start + thumb.len {
                    axis.page(key, at, 1, tx);
                }
            }
            return;
        }
        Phase::Move => {}
    }
    if thumb.travel > 0.0 {
        let ratio = ((point - drag.data) / thumb.travel).clamp(0.0, 1.0);
        axis.set(key, at.max[axis.index()] * ratio, tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{col, Cx, Env, Runtime, ScrollBars};

    #[test]
    fn one_classic_strip_can_create_real_overflow_in_the_other_axis() {
        let sizes = Some([100.0, 100.0]);
        for (content, x, y) in [
            ([100.0, 100.0], false, false),
            ([101.0, 91.0], true, false),
            ([91.0, 101.0], false, true),
            ([101.0, 92.0], true, true),
            ([92.0, 101.0], true, true),
            ([101.0, 101.0], true, true),
        ] {
            assert_eq!(Gutters::for_content(sizes, content), Gutters { x, y });
        }
        assert_eq!(Gutters::for_content(None, [300.0, 400.0]), Gutters::default());
    }

    fn fixture(_: &(), _: &mut Cx<()>) -> El {
        let key = "viewport";
        let content = [300.0, 400.0];
        let frame = rune::memo::layout_size(&area_key(key));
        let viewport = col()
            .key(key)
            .w_full().h_full()
            .scroll_xy()
            .child(col().size(content[0], content[1])
                .child(el().key("marker").absolute(160.0, 280.0).size(10.0, 10.0)));
        attach::<()>(
            viewport, Gutters::for_content(frame, content),
            Color::WHITE, Color::BLACK, Color::BLACK,
        )
    }

    fn rect(rt: &Runtime<()>, key: &str) -> [f32; 4] {
        rt.elements().into_iter().find(|node| !node.exiting && node.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing {key}"))
            .rect
    }

    fn drag(rt: &mut Runtime<()>, x: f32, y: f32, target_x: f32, target_y: f32) {
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_move(target_x, target_y);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(target_x, target_y);
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn idle_classic_tracks_reserve_room_and_drive_the_real_two_axis_viewport() {
        let mut rt = Runtime::new((), fixture, (200.0, 160.0), Env::new(false, 1.0));
        rt.set_system_scroll_bars(ScrollBars::WhenScrolling);
        rt.frame(0.0);
        assert_eq!(rect(&rt, "viewport"), [0.0, 0.0, 191.0, 151.0]);
        assert_eq!(rect(&rt, "horizontal-scrollbar"), [0.0, 151.0, 191.0, 9.0]);
        assert_eq!(rect(&rt, "vertical-scrollbar"), [191.0, 0.0, 9.0, 151.0]);

        drag(&mut rt, 30.0, 155.0, 155.0, 155.0);
        assert_eq!(rect(&rt, "marker")[0], 51.0, "horizontally dragged to the real content end");
        drag(&mut rt, 195.0, 10.0, 195.0, 145.0);
        assert_eq!(rect(&rt, "marker")[1], 31.0, "vertically dragged to the real content end");

        rt.update(|_, tx| tx.scroll_to_y("viewport", 0.0));
        rt.frame(1.0 / 60.0);
        rt.pointer_move(195.0, 100.0);
        rt.wheel_event(WheelEvent::new(0.0, -25.0).at(195.0, 100.0));
        rt.frame(1.0 / 60.0);
        assert_eq!(rect(&rt, "marker")[1], 255.0, "the visible track routes a real wheel to its viewport");

        rt.set_viewport((500.0, 500.0), 1.0);
        rt.frame(1.0 / 60.0);
        assert_eq!(rect(&rt, "viewport"), [0.0, 0.0, 500.0, 500.0]);
        for key in ["horizontal-scrollbar", "vertical-scrollbar"] {
            assert_eq!(rect(&rt, key)[2..], [0.0, 0.0], "fitting content has no bar hit area");
        }
    }
}
