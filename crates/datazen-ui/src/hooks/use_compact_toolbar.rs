//! Port of `packages/frontend/src/hooks/useCompactToolbar.ts` (T233) —
//! hide toolbar labels when the container cannot fit expanded buttons.
//!
//! ## What maps where
//!
//! - `ResizeObserver` is the host's frame measurement: the host knows
//!   the toolbar's container width each layout pass and feeds it to
//!   [`CompactToolbarState::on_resize`]; a width `<= 0` (no layout yet)
//!   is ignored exactly as React's early return.
//! - The hysteresis rule is the state machine, verbatim: once compact,
//!   expanding again requires `threshold + 16` of room (a little slack
//!   so the toolbar does not flicker at the boundary); crossing down
//!   happens at the plain threshold. `scrollWidth` is deliberately NOT
//!   consulted — it can be slightly larger than the client width from
//!   unaccounted elements (separators, badges, keyboard hints), and
//!   forcing compact off that minor overflow wedged the toolbar
//!   icon-only because hysteresis then needed threshold+16 to recover
//!   (the React file's own bug note).
//!
//! Known deltas: none behavioural — the estimate and the hysteresis
//! are the React file's verbatim.

/// Estimated width of one expanded toolbar button (icon + label + padding).
pub(crate) const TOOLBAR_EXPANDED_BUTTON_WIDTH: f32 = 96.0;
/// The gap between toolbar buttons.
pub(crate) const TOOLBAR_GAP: f32 = 8.0;
/// The toolbar's horizontal padding.
pub(crate) const TOOLBAR_HORIZONTAL_PADDING: f32 = 32.0;

/// The hysteresis slack before an expanded toolbar may re-expand.
pub(crate) const TOOLBAR_HYSTERESIS: f32 = 16.0;

/// The React hook's default threshold (`useCompactToolbar(threshold =
/// 920)`).
pub(crate) const DEFAULT_TOOLBAR_THRESHOLD: f32 = 920.0;

/// `estimateExpandedToolbarWidth`: the minimum toolbar width required to
/// show text labels — padding, the buttons, the gaps between them and
/// any fixed extras. A zero button count is the padding-only width.
pub(crate) fn estimate_expanded_toolbar_width(
    expanded_button_count: usize,
    fixed_extra_width: f32,
) -> f32 {
    let count = expanded_button_count.max(0);
    TOOLBAR_HORIZONTAL_PADDING
        + count as f32 * TOOLBAR_EXPANDED_BUTTON_WIDTH
        + count.saturating_sub(1).max(0) as f32 * TOOLBAR_GAP
        + fixed_extra_width.max(0.0)
}

/// The compact-toolbar state (React's `useState(compact)` plus the
/// hysteresis closure's rule).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompactToolbarState {
    compact: bool,
}

impl CompactToolbarState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Whether the toolbar renders icon-only (the labels hidden).
    pub(crate) fn compact(&self) -> bool {
        self.compact
    }

    /// One container measurement (the ResizeObserver's update): widths
    /// `<= 0` are ignored (no layout yet). The hysteresis rule —
    /// compact → needs `threshold + 16` to expand; expanded → compacts
    /// below `threshold`.
    pub(crate) fn on_resize(&mut self, width: f32, threshold: f32) {
        if width <= 0.0 {
            return;
        }
        if self.compact {
            self.compact = width < threshold + TOOLBAR_HYSTERESIS;
        } else {
            self.compact = width < threshold;
        }
    }
}
