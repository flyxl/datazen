//! Settings' classic overflow scrollbar. It follows the real viewport and delegates
//! all movement to Rune; the desktop system's page-versus-jump preference is retained.
use std::{cell::Cell, rc::Rc, time::Duration};

use rune::{el, Drag, El, Live, Phase, ScrollPos, Tx};
use rune_kit::Kit;

pub(super) const STRIP: f32 = 9.0;
const INSET: f32 = 2.0;
const THUMB_WIDTH: f32 = STRIP - 2.0 * INSET;
const MIN_THUMB: f32 = 24.0;
const PAGE_UP: f32 = -1.0;
const PAGE_DOWN: f32 = -2.0;

#[derive(Clone, Copy)]
struct Thumb {
    top: f32,
    len: f32,
    travel: f32,
}

impl Thumb {
    fn for_view(at: ScrollPos) -> Option<Self> {
        let height = at.drawn[1];
        let max = at.max[1];
        if max <= 0.0 || height <= 2.0 * INSET {
            return None;
        }
        let len = (height * height / (at.viewport[1] + max))
            .round()
            .max(MIN_THUMB)
            .min(height);
        let travel = height - len;
        let top = travel * (at.offset[1] / max).clamp(0.0, 1.0);
        Some(Self { top, len, travel })
    }
}

pub(super) fn attach(viewport: El, k: &Kit) -> El {
    let (area, key, outer, viewport) = viewport.wrap_scroll_area((file!(), line!(), column!()));
    attach_to(outer, viewport, format!("{area}/{key}"), "settings-content-scrollbar", k)
}

/// Keep an intrinsic-height preview in flow: short text has no blank well,
/// while longer text retains its own viewport independently of the page.
pub(in crate::views::settings) fn with_preview_scrollbar(viewport: El, key: &str, k: &Kit) -> El {
    let area = format!("{key}-scroll-area");
    attach_to(
        rune::col().key(area.clone()).w_full(),
        viewport,
        format!("{area}/{key}"),
        &format!("{key}-scrollbar"),
        k,
    )
}

fn attach_to(outer: El, viewport: El, target: String, track_key: &str, k: &Kit) -> El {
    let at = Rc::new(Cell::new(ScrollPos::default()));
    let track = Live::new([0.0; 4]);
    let thumb_rect = Live::new([0.0; 4]);
    let (observe, watch_track, watch_thumb) = (at.clone(), track.clone(), thumb_rect.clone());
    let jumps = rune::system_scroller_jumps();
    outer
        .child(viewport.on_scroll_offset(move |pos| {
            observe.set(pos);
            if let Some(thumb) = Thumb::for_view(pos) {
                let x = if pos.direction.is_rtl() {
                    0.0
                } else {
                    (pos.drawn[0] - STRIP).max(0.0)
                };
                watch_track.snap([x, 0.0, STRIP, pos.drawn[1]]);
                watch_thumb.snap([
                    INSET,
                    thumb.top + INSET,
                    THUMB_WIDTH,
                    thumb.len - 2.0 * INSET,
                ]);
            } else {
                // No visible strip, and zero hit area when the real body fits.
                watch_track.snap([0.0; 4]);
                watch_thumb.snap([0.0; 4]);
            }
        }))
        .child(
            el().key(track_key.to_owned())
                .absolute(0.0, 0.0)
                .live_rect(&track)
                // The underlying scroll viewport remains the accessible scroll control.
                .a11y_hidden()
                .on_drag_any(move |drag, tx| drag_bar(&target, at.get(), jumps, drag, tx))
                .child(
                    el().key(format!("{track_key}-thumb"))
                        .absolute(0.0, 0.0)
                        .live_rect(&thumb_rect)
                        .radius(THUMB_WIDTH / 2.0)
                        .fill(k.c.border_strong)
                        .hover(k.c.text_faint),
                ),
        )
}

fn drag_bar(key: &str, at: ScrollPos, jumps: bool, drag: &mut Drag, tx: &mut Tx) {
    tx.live_only();
    let Some(thumb) = Thumb::for_view(at) else { return };
    match drag.phase {
        Phase::Down => {
            if (thumb.top..=thumb.top + thumb.len).contains(&drag.y) {
                drag.data = drag.y - thumb.top;
                // Grabbing at rest must not move the viewport.
                return;
            }
            if jumps == tx.mods().alt {
                drag.data = if drag.y < thumb.top { PAGE_UP } else { PAGE_DOWN };
                tx.scroll_page(key, if drag.data == PAGE_UP { -1 } else { 1 });
                tx.drag_repeat_after(Duration::from_millis(300));
                return;
            }
            drag.data = thumb.len / 2.0;
        }
        Phase::Up => return,
        Phase::Move if drag.data < 0.0 => {
            if tx.drag_repeat() {
                tx.drag_repeat_after(Duration::from_millis(60));
                if drag.data == PAGE_UP && drag.y < thumb.top {
                    tx.scroll_page(key, -1);
                } else if drag.data == PAGE_DOWN && drag.y > thumb.top + thumb.len {
                    tx.scroll_page(key, 1);
                }
            }
            return;
        }
        Phase::Move => {}
    }
    if thumb.travel > 0.0 {
        let ratio = ((drag.y - drag.data) / thumb.travel).clamp(0.0, 1.0);
        tx.scroll_to_y(key, ratio * at.max[1]);
    }
}
