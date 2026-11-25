//! Port of `packages/frontend/src/hooks/useResizable.ts` (T404) —
//! the drag machine behind a resizable panel's handle.
//!
//! ## What maps where
//!
//! - The `useState` initializer (the `localStorage.getItem(
//!   'resize:' + storageKey)` read with the `Number`/NaN guard and
//!   the `min`/`max` clamp, else `initialSize`) is
//!   [`UseResizableState::mount`]: the host delivers the persisted
//!   value it read (the storage seam); an unparseable or absent one
//!   is `None`, exactly the React `if (saved)` guard.
//! - The drag listeners are [`UseResizableState::pointer_down`] /
//!   [`pointer_move`] / [`pointer_up`]: the host drives the pointer
//!   it receives into the machine (the `handleEl` listeners, the
//!   pointer capture and the body `cursor`/`userSelect` styling are
//!   the host's drag surface — Rune's grip plumbing).
//! - `onPointerDown`'s `getStartSize` adoption (a CSS-sized panel —
//!   `flex-1` — must not jump to a stale `size` when the user takes
//!   over) is the `get_start_size` parameter: a measured `> 0`,
//!   finite value becomes the drag's start AND is adopted as the
//!   size before the first move; anything else falls back to the
//!   current size (React's `typeof measured === 'number' &&
//!   Number.isFinite(measured) && measured > 0` guard).
//! - `onResizeStart` (leave an automatic sizing mode the moment the
//!   user grabs the handle) is the `on_resize_start` host slot of
//!   [`UseResizableState::pointer_down`].
//! - `onPointerMove`'s math is verbatim: `next = clamp(startSize +
//!   (reverse ? -delta : delta))` — the `reverse` flag flips the
//!   axis for right-side panels.
//! - `onPointerUp`'s persist (``localStorage.setItem(`resize:${key}`,
//!   String(sizeRef.current))``) is [`UseResizableState::pointer_up`]'s
//!   answer: the `resize:{storageKey}` write the host persists — the
//!   CURRENT size, not the drag's start.
//! - The returned `setSize` is the raw setter (the React file hands
//!   callers the unclamped `setState`; only the drag math clamps).
//!
//! Known deltas: `direction` picks the coordinate (`e.clientX` vs
//! `e.clientY`) and the body cursor — the host feeds the axis value
//! and owns the cursor, so the state only carries the flag for its
//! API surface.

// No production consumer yet (the resizable panels' wiring track
// mounts the handle); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

/// `direction: 'horizontal' | 'vertical'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeDirection {
    Horizontal,
    Vertical,
}

impl ResizeDirection {
    /// The body cursor the drag sets (`col-resize` / `row-resize`).
    pub fn cursor(self) -> &'static str {
        match self {
            Self::Horizontal => "col-resize",
            Self::Vertical => "row-resize",
        }
    }
}

/// `UseResizableOptions` (the two callbacks are the host slots of
/// the drag methods instead — `onResizeStart` rides
/// [`UseResizableState::pointer_down`], `getStartSize` is its
/// parameter).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseResizableOptions {
    pub direction: ResizeDirection,
    pub initial_size: f64,
    pub min_size: f64,
    pub max_size: f64,
    /// When true, dragging towards positive axis shrinks the panel
    /// (for right-side panels).
    pub reverse: bool,
}

impl UseResizableOptions {
    /// `clamp` — `Math.max(minSize, Math.min(maxSize, n))`.
    pub fn clamp(&self, n: f64) -> f64 {
        n.max(self.min_size).min(self.max_size)
    }

    /// `` `resize:${storageKey}` `` — the localStorage seam's key.
    pub fn storage_key(&self, storage_key: &str) -> String {
        format!("resize:{storage_key}")
    }
}

/// One drag in flight (the effect's `startPos`/`startSize`/`active`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct ResizeDrag {
    start_pos: f64,
    start_size: f64,
}

/// The hook's state: the size, the persisted-seam answer and the
/// drag in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct UseResizableState {
    options: UseResizableOptions,
    size: f64,
    drag: Option<ResizeDrag>,
}

impl UseResizableState {
    pub fn new(options: UseResizableOptions) -> Self {
        Self {
            options,
            // The useState initializer's fallback arm; a mount with a
            // persisted value ([`Self::mount`]) may replace it.
            size: options.initial_size,
            drag: None,
        }
    }

    /// The useState initializer: adopt the persisted value (clamped)
    /// when the storage seam answers one, else keep the initial size.
    pub fn mount(&mut self, saved: Option<f64>) {
        if let Some(saved) = saved {
            if saved.is_finite() {
                self.size = self.options.clamp(saved);
            }
        }
    }

    /// The current size (the `size` the hook returns).
    pub fn size(&self) -> f64 {
        self.size
    }

    /// The raw setter (the returned `setSize`): the React file hands
    /// the unclamped `setState` to its callers.
    pub fn set_size(&mut self, size: f64) {
        self.size = size;
    }

    /// Whether a drag is in flight (`active`).
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// `onPointerDown(e)`: `axis` is the direction's coordinate
    /// (`e.clientX` for horizontal, `e.clientY` for vertical).
    /// `get_start_size` is the option's override; `on_resize_start`
    /// is its callback. Adopts the measured start (a CSS-sized panel
    /// must not jump) and answers whether the drag began.
    pub fn pointer_down(
        &mut self,
        axis: f64,
        get_start_size: Option<f64>,
        mut on_resize_start: impl FnMut(),
    ) -> bool {
        // `0` (or non-finite, or absent) means the panel is not laid
        // out — not a usable start.
        let start = match get_start_size {
            Some(measured) if measured.is_finite() && measured > 0.0 => measured,
            _ => self.size,
        };
        // Adopt the measured size before the first move.
        self.size = start;
        on_resize_start();
        self.drag = Some(ResizeDrag {
            start_pos: axis,
            start_size: start,
        });
        true
    }

    /// `onPointerMove(e)`: the direction's coordinate. Answers the
    /// next size (the clamped `startSize ± delta`), or `None` while
    /// no drag is in flight (the `if (!active …)` guard; the pointer
    /// capture is the host's).
    pub fn pointer_move(&mut self, axis: f64) -> Option<f64> {
        let drag = self.drag?;
        let delta = axis - drag.start_pos;
        let next = if self.options.reverse {
            drag.start_size - delta
        } else {
            drag.start_size + delta
        };
        let next = self.options.clamp(next);
        self.size = next;
        Some(next)
    }

    /// `onPointerUp(e)`: ends the drag and answers the persist write
    /// — `("resize:{storageKey}", size)` — when a storage key exists
    /// (the CURRENT size, not the drag's start).
    pub fn pointer_up(
        &mut self,
        storage_key: Option<&str>,
    ) -> Option<(String, f64)> {
        self.drag = None;
        storage_key.map(|key| (self.options.storage_key(key), self.size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> UseResizableOptions {
        UseResizableOptions {
            direction: ResizeDirection::Horizontal,
            initial_size: 300.0,
            min_size: 120.0,
            max_size: 600.0,
            reverse: false,
        }
    }

    #[test]
    fn mount_adopts_the_persisted_value_clamped() {
        let mut state = UseResizableState::new(options());
        // No persisted value: the initial size.
        state.mount(None);
        assert_eq!(state.size(), 300.0);
        // A persisted value within range: adopted verbatim.
        state.mount(Some(420.0));
        assert_eq!(state.size(), 420.0);
        // Out-of-range values clamp (the initializer's
        // `Math.max(min, Math.min(max, n))`).
        state.mount(Some(50.0));
        assert_eq!(state.size(), 120.0);
        state.mount(Some(9_999.0));
        assert_eq!(state.size(), 600.0);
        // A NaN persisted value is the `if (saved)` NaN guard: the
        // initial size stays.
        let mut state = UseResizableState::new(options());
        state.mount(Some(f64::NAN));
        assert_eq!(state.size(), 300.0);
    }

    #[test]
    fn a_drag_moves_from_the_adopted_start() {
        let mut state = UseResizableState::new(options());
        // The CSS-sized panel measures 500: the down adopts it.
        let mut started = 0;
        assert!(state.pointer_down(100.0, Some(500.0), || { started += 1; }));
        assert_eq!(started, 1, "onResizeStart fired");
        assert_eq!(state.size(), 500.0, "adopted before the first move");
        assert!(state.dragging());
        assert_eq!(state.pointer_move(140.0), Some(540.0));
        assert_eq!(state.size(), 540.0);
        // The clamp caps the far end.
        assert_eq!(state.pointer_move(1000.0), Some(600.0));
    }

    #[test]
    fn an_unusable_measure_falls_back_to_the_current_size() {
        let mut state = UseResizableState::new(options());
        state.set_size(260.0);
        // 0 (not laid out) falls back.
        state.pointer_down(10.0, Some(0.0), || {});
        assert_eq!(state.pointer_move(50.0), Some(300.0), "260 + 40");
        // Absent measure: the same fallback.
        let mut state = UseResizableState::new(options());
        state.set_size(200.0);
        state.pointer_down(0.0, None, || {});
        assert_eq!(state.pointer_move(30.0), Some(230.0));
        // Non-finite: the same fallback.
        let mut state = UseResizableState::new(options());
        state.set_size(200.0);
        state.pointer_down(0.0, Some(f64::NAN), || {});
        assert_eq!(state.pointer_move(30.0), Some(230.0));
    }

    #[test]
    fn reverse_flips_the_axis() {
        let mut opts = options();
        opts.reverse = true;
        let mut state = UseResizableState::new(opts);
        state.pointer_down(100.0, Some(400.0), || {});
        // Dragging right (positive) SHRINKS the right-side panel.
        assert_eq!(state.pointer_move(150.0), Some(350.0));
        assert_eq!(state.pointer_move(50.0), Some(450.0));
    }

    #[test]
    fn pointer_up_answers_the_persist_write() {
        let mut state = UseResizableState::new(options());
        state.pointer_down(0.0, Some(300.0), || {});
        state.pointer_move(90.0);
        // With a storage key: the resize-prefixed key + the current
        // size (not the drag's start).
        assert_eq!(
            state.pointer_up(Some("sidebar")),
            Some(("resize:sidebar".to_owned(), 390.0))
        );
        assert!(!state.dragging());
        // Without a storage key: nothing to persist.
        let mut state = UseResizableState::new(options());
        state.pointer_down(0.0, None, || {});
        assert_eq!(state.pointer_up(None), None);
        // A move after the up is inert (the `active` guard).
        assert_eq!(state.pointer_move(500.0), None);
        assert_eq!(state.size(), 300.0);
    }

    #[test]
    fn the_direction_spells_its_cursor() {
        assert_eq!(ResizeDirection::Horizontal.cursor(), "col-resize");
        assert_eq!(ResizeDirection::Vertical.cursor(), "row-resize");
    }

    #[test]
    fn the_setter_is_raw_not_clamped() {
        let mut state = UseResizableState::new(options());
        state.set_size(5_000.0);
        assert_eq!(state.size(), 5_000.0, "the returned setSize is the raw setState");
    }
}
