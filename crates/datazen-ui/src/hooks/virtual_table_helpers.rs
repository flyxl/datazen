//! Split half of the `useVirtualTable` port (T457) — the pure layout
//! vocabulary the hook's state machine computes with, kept next to it
//! the way `transfer_job_run_helpers.rs` sits beside T438's machine.
//!
//! `@tanstack/react-virtual` derives these numbers inside its
//! virtualizer instance; the port spreads them out as free functions
//! and the `VirtualRow` projection so the state machine stays a thin,
//! testable window over them.

/// `useVirtualTable`'s default `overscan` (rows rendered beyond each
/// edge of the viewport) — the React signature's `overscan = 10`.
pub const OVERSCAN_DEFAULT: usize = 10;

/// One of the virtualizer's `getVirtualItems()` entries — the row the
/// renderer absolutely positions inside the total-height spacer.
///
/// React's `VirtualItem` also carries a `key` and a `lane`; both are
/// the index on this single-lane list, so the port keeps the triple
/// that positions a row: which row, where it starts (px from the
/// spacer's top), and how tall it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VirtualRow {
    /// The row's index into `rows`.
    pub index: usize,
    /// The row's top edge, `index * rowHeight` (px).
    pub start: f32,
    /// The uniform `rowHeight` (px).
    pub size: f32,
}

/// The furthest a scroll offset may go: everything past the last row
/// minus the viewport, floored at zero. A content that fits never
/// scrolls (`max <= 0.0` is the hook's no-scroll verdict).
pub fn max_scroll_offset(total_height: f32, viewport: f32) -> f32 {
    (total_height - viewport).max(0.0)
}
