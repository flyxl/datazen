//! Resizable splitters (L2): a drag handle that changes one layout size and
//! persists it through `Setting::Layout` once the gesture ends.

use crate::AppState;
use rune::{el, oklch, Drag, El, Phase, Tx};

/// The direction a split bar divides space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitAxis {
    /// The bar runs vertically and the drag changes a width (left/right).
    Vertical,
    /// The bar runs horizontally and the drag changes a height (up/down).
    Horizontal,
}

/// Clamp a drag to a new size in integer points. `travel` is the pointer
/// travel along the split axis in window points (positive = right/down,
/// negative = left/up), `start` is the size held at the press, and the result
/// stays inside `[min, max]`.
pub(crate) fn size_after_drag(travel: f32, start: u32, min: u32, max: u32) -> u32 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    let size = start as f32 + travel;
    size.round().clamp(lo as f32, hi as f32) as u32
}

/// The step a keyboard nudge moves the split, in points: arrows move one,
/// Page keys move ten (rune's `on_step` already maps keys to ±1/±10).
pub(crate) fn size_after_step(current: u32, delta: i32, min: u32, max: u32) -> u32 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    let size = current as i64 + delta as i64;
    size.clamp(lo as i64, hi as i64) as u32
}

/// The size in effect for one region: a drag in progress shows its preview,
/// otherwise the persisted setting, otherwise the built-in default.
pub(crate) fn splitter_size(state: &AppState, key: &str, default: u32) -> u32 {
    if let Some((region, value)) = &state.splitter_preview {
        if region == key {
            return *value;
        }
    }
    state.layout_size(key).unwrap_or(default)
}

/// The key stored in `Drag::data` and the preview slot while a gesture runs.
fn begin_drag(state: &mut AppState, key: &str, start: u32, drag: &mut Drag) {
    drag.data = start as f32;
    state.splitter_preview = Some((key.to_string(), start));
}

fn drag_travel(axis: SplitAxis, drag: &Drag) -> f32 {
    match axis {
        SplitAxis::Vertical => drag.dx,
        SplitAxis::Horizontal => drag.dy,
    }
}

/// Apply a move: keep the press-start in `drag.data` and publish the live
/// preview size.
fn apply_move(state: &mut AppState, key: &str, value: u32, start: u32, drag: &mut Drag) {
    drag.data = start as f32;
    state.splitter_preview = Some((key.to_string(), value));
}

/// Finish a gesture: `None` means cancelled (discard the preview), `Some`
/// commits the size.
pub(crate) fn finish_drag(state: &mut AppState, key: &str, commit: Option<u32>) {
    state.splitter_preview = None;
    if let Some(value) = commit {
        let _ = state.set_layout_size(key, value);
    }
}

/// The committed size for a finished gesture, or `None` when cancelled.
pub(crate) fn drag_commit(
    cancelled: bool, travel: f32, start: u32, min: u32, max: u32,
) -> Option<u32> {
    if cancelled {
        None
    } else {
        Some(size_after_drag(travel, start, min, max))
    }
}

/// Build the drag handle for one split. `key` names the region in the layout
/// settings; `default` is the size before the first resize.
pub(crate) fn splitter_handle(
    state: &AppState,
    key: &'static str,
    axis: SplitAxis,
    min: u32,
    max: u32,
    default: u32,
) -> El {
    let size = splitter_size(state, key, default);
    let label = match axis {
        SplitAxis::Vertical => "Resize sidebar",
        SplitAxis::Horizontal => "Resize panel",
    };
    let mut handle = el()
        .key(format!("splitter:{key}"))
        .role("separator")
        .label(label)
        .a11y_value(format!("{size} pt"))
        .focusable(true)
        .fill(oklch(0.5, 0.0, 0.0)); // subtle neutral handle; themed later
    handle = match axis {
        SplitAxis::Vertical => handle.w(5.0).h_full(),
        SplitAxis::Horizontal => handle.w_full().h(5.0),
    };

    // Drag resizes: Down seeds the start, Move previews, Up persists (or the
    // cancel path discards the preview).
    handle = handle.on_drag(move |s: &mut AppState, drag: &mut Drag, _: &mut Tx| {
        let travel = drag_travel(axis, drag);
        let start = drag.data.round().max(0.0) as u32;
        match drag.phase {
            Phase::Down => {
                let start = splitter_size(s, key, default);
                begin_drag(s, key, start, drag);
            }
            Phase::Move => {
                let value = size_after_drag(travel, start, min, max);
                apply_move(s, key, value, start, drag);
            }
            Phase::Up => {
                let commit = drag_commit(drag.cancelled(), travel, start, min, max);
                finish_drag(s, key, commit);
            }
        }
    });

    // A double press resets the region to its built-in default.
    handle = handle.on_double_press(move |s: &mut AppState| {
        s.splitter_preview = None;
        let _ = s.set_layout_size(key, default);
    });

    // Keyboard micro-adjust (arrows ±1, Page ±10) and ends (Home/End).
    handle = handle
        .on_step(move |s: &mut AppState, delta: i32, _: &mut Tx| {
            let current = splitter_size(s, key, default);
            // For a horizontal bar, "down" (rune: -1) should grow the pane below,
            // so flip the sign to keep arrow direction intuitive.
            let delta = match axis {
                SplitAxis::Vertical => delta,
                SplitAxis::Horizontal => -delta,
            };
            let value = size_after_step(current, delta, min, max);
            let _ = s.set_layout_size(key, value);
        })
        .on_step_limit(move |s: &mut AppState, to_max: bool, _: &mut Tx| {
            let value = if to_max { max } else { min };
            let _ = s.set_layout_size(key, value);
        });

    handle
}

#[cfg(test)]
mod tests {
    use super::{drag_commit, finish_drag, size_after_drag, size_after_step, splitter_size};
    use crate::AppState;

    #[test]
    fn drag_clamps_and_rounds() {
        assert_eq!(size_after_drag(20.4, 280, 200, 500), 300);
        assert_eq!(size_after_drag(20.6, 280, 200, 500), 301);
        assert_eq!(size_after_drag(-200.0, 280, 200, 500), 200, "clamps to min");
        assert_eq!(size_after_drag(500.0, 280, 200, 500), 500, "clamps to max");
        assert_eq!(size_after_drag(0.0, 280, 200, 500), 280, "no travel keeps start");
    }

    #[test]
    fn drag_handles_swapped_bounds() {
        assert_eq!(size_after_drag(0.0, 280, 500, 200), 280);
        assert_eq!(size_after_drag(-200.0, 280, 500, 200), 200);
    }

    #[test]
    fn step_moves_and_clamps() {
        assert_eq!(size_after_step(280, 1, 200, 500), 281);
        assert_eq!(size_after_step(280, -1, 200, 500), 279);
        assert_eq!(size_after_step(280, 10, 200, 500), 290);
        assert_eq!(size_after_step(280, -10, 200, 500), 270);
        assert_eq!(size_after_step(200, -1, 200, 500), 200);
        assert_eq!(size_after_step(500, 10, 200, 500), 500);
    }

    #[test]
    fn cancel_discards_the_preview_and_keeps_the_persisted_size() {
        let mut state = AppState::default();
        state.set_layout_size("navigator-width", 280).unwrap();
        state.splitter_preview = Some(("navigator-width".into(), 340));
        assert_eq!(splitter_size(&state, "navigator-width", 224), 340, "preview wins while dragging");

        // Cancelled gesture: no commit, preview cleared, persisted value stands.
        assert_eq!(drag_commit(true, 60.0, 280, 200, 500), None);
        finish_drag(&mut state, "navigator-width", None);
        assert_eq!(state.splitter_preview, None);
        assert_eq!(state.layout_size("navigator-width"), Some(280));
        assert_eq!(splitter_size(&state, "navigator-width", 224), 280);
    }

    #[test]
    fn commit_persists_the_dragged_size() {
        let mut state = AppState::default();
        let commit = drag_commit(false, 40.0, 280, 200, 500);
        assert_eq!(commit, Some(320));
        finish_drag(&mut state, "navigator-width", commit);
        assert_eq!(state.layout_size("navigator-width"), Some(320));
        assert_eq!(state.splitter_preview, None);
    }

    #[test]
    fn reset_restores_the_default() {
        let mut state = AppState::default();
        state.set_layout_size("navigator-width", 340).unwrap();
        // A double press resets: drop preview and persist the default.
        state.splitter_preview = Some(("navigator-width".into(), 360));
        finish_drag(&mut state, "navigator-width", Some(280));
        assert_eq!(state.layout_size("navigator-width"), Some(280));
        assert_eq!(splitter_size(&state, "navigator-width", 280), 280);
    }
}
