//! Port of `packages/frontend/src/hooks/useVirtualTable.ts` (T457,
//! split with `virtual_table_helpers.rs`).
//!
//! The React hook wraps `@tanstack/react-virtual`'s `useVirtualizer`:
//! given `rows`, a uniform `rowHeight`, an `overscan` (default **10**)
//! and the scroll container, it returns the slice of rows the
//! renderer should mount (`virtualRows`), the spacer height
//! (`totalHeight`) and `scrollToRow`. React measures the container
//! through a DOM ref; Rune has no DOM, so — the established host-seam
//! convention — the surface's metrics arrive as arguments and the
//! scroll ask leaves as a return value the host applies.
//!
//! ## What maps where
//!
//! - `count: rows.length` / `estimateSize: () => rowHeight` /
//!   `overscan` fold into [`VirtualTableOptions`] (`row_count`,
//!   `row_height`, `overscan`); `rows: unknown[][]` carries no data
//!   the window needs, so the port takes the length
//!   ([`UseVirtualTable::new`] / `set_rows_len`).
//! - `getScrollElement: () => scrollElement` becomes the two metric
//!   entry points: [`UseVirtualTable::note_viewport`] (the
//!   container's `clientHeight`) and [`UseVirtualTable::on_scroll`]
//!   (its live `scrollTop`).
//! - `getVirtualItems()` is [`UseVirtualTable::virtual_rows`]: the
//!   window `[first_visible - overscan, last_visible + overscan)`
//!   clamped to the row count, with every row carrying its absolute
//!   `start` (`index * rowHeight`) — a partially visible tail row
//!   counts as visible (`ceil`), and a degenerate list (no rows, no
//!   measured viewport, zero row height) mounts nothing.
//! - `getTotalSize()` is [`UseVirtualTable::total_height`]:
//!   `row_count * row_height` exactly.
//! - `scrollToIndex` is [`UseVirtualTable::scroll_to_row`]: the
//!   top-aligned target `index * rowHeight` clamped to the maximum
//!   offset, and `None` when the content fits (nothing to scroll).
//!   tanstack's `align: 'auto'` nudge-into-view is DOM-side
//!   measurement; the port returns the clamped top-aligned ask.
//!
//! Known deltas: none behavioural — the window arithmetic, the
//! default overscan and the clamp are the React file's verbatim.

use crate::hooks::virtual_table_helpers::{max_scroll_offset, VirtualRow, OVERSCAN_DEFAULT};

/// The virtualizer's configured dimensions — the React options that
/// survive the loss of the DOM ref.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VirtualTableOptions {
    /// `count` — `rows.length`.
    pub(crate) row_count: usize,
    /// `estimateSize()` — the uniform row height (px).
    pub(crate) row_height: f32,
    /// `overscan` — rows rendered beyond each viewport edge.
    pub(crate) overscan: usize,
}

/// `useVirtualTable`'s state, embeddable in the host's `S`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UseVirtualTable {
    options: VirtualTableOptions,
    /// The scroll container's `clientHeight`, as last reported.
    viewport: f32,
    /// The scroll container's `scrollTop`, as last reported.
    offset: f32,
}

impl Default for UseVirtualTable {
    fn default() -> Self {
        Self {
            options: VirtualTableOptions {
                row_count: 0,
                row_height: 0.0,
                overscan: OVERSCAN_DEFAULT,
            },
            viewport: 0.0,
            offset: 0.0,
        }
    }
}

impl UseVirtualTable {
    /// `useVirtualTable({ rows, rowHeight })` — the default-overscan
    /// construction.
    pub(crate) fn new(rows_len: usize, row_height: f32) -> Self {
        Self {
            options: VirtualTableOptions {
                row_count: rows_len,
                row_height,
                overscan: OVERSCAN_DEFAULT,
            },
            ..Self::default()
        }
    }

    /// `useVirtualTable({ ..., overscan })` — the explicit-overscan
    /// construction (`VirtualBody` passes 12).
    pub(crate) fn with_overscan(mut self, overscan: usize) -> Self {
        self.options.overscan = overscan;
        self
    }

    /// The rows array was replaced (`count` follows `rows.length`).
    pub(crate) fn set_rows_len(&mut self, rows_len: usize) {
        self.options.row_count = rows_len;
    }

    /// The row height changed (`estimateSize` re-measured).
    pub(crate) fn set_row_height(&mut self, row_height: f32) {
        self.options.row_height = row_height;
    }

    /// The scroll container mounted or resized: its `clientHeight`.
    pub(crate) fn note_viewport(&mut self, viewport: f32) {
        self.viewport = viewport;
    }

    /// `onScroll`: the container's live `scrollTop`.
    pub(crate) fn on_scroll(&mut self, offset: f32) {
        self.offset = offset;
    }

    /// The container's live `scrollTop`, as last reported.
    pub(crate) fn offset(&self) -> f32 {
        self.offset
    }

    /// The container's `clientHeight`, as last reported.
    pub(crate) fn viewport(&self) -> f32 {
        self.viewport
    }

    /// `getVirtualItems()`: the rows to mount, windowed to the
    /// viewport with overscan on both edges. A list with no rows, an
    /// unmeasured (or empty) viewport, or a zero row height mounts
    /// nothing.
    pub(crate) fn virtual_rows(&self) -> Vec<VirtualRow> {
        let viewport = self.viewport;
        if self.options.row_count == 0 || viewport <= 0.0 || self.options.row_height <= 0.0 {
            return Vec::new();
        }
        let offset = self.offset;
        let first_visible = (offset / self.options.row_height).floor() as usize;
        let last_visible = ((offset + viewport) / self.options.row_height).ceil() as usize;
        let start = first_visible.saturating_sub(self.options.overscan);
        let end = (last_visible + self.options.overscan).min(self.options.row_count);
        (start..end)
            .map(|index| VirtualRow {
                index,
                start: index as f32 * self.options.row_height,
                size: self.options.row_height,
            })
            .collect()
    }

    /// `getTotalSize()`: the spacer's height — the full extent of the
    /// list, not just the mounted window.
    pub(crate) fn total_height(&self) -> f32 {
        self.options.row_count as f32 * self.options.row_height
    }

    /// The furthest offset for the given viewport height.
    fn max_offset(&self, viewport: f32) -> f32 {
        max_scroll_offset(self.total_height(), viewport)
    }

    /// `scrollToRow` (`virtualizer.scrollToIndex`): the offset the
    /// host should apply to bring `index` to the viewport's top,
    /// clamped so the scroll cannot pass the last row. `None` when
    /// the content already fits — there is nothing to scroll to.
    pub(crate) fn scroll_to_row(&self, index: usize) -> Option<f32> {
        let viewport = self.viewport;
        let max = self.max_offset(viewport);
        if max <= 0.0 {
            return None;
        }
        let target = index as f32 * self.options.row_height;
        Some(target.min(max))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 1000 rows of 10px: the shared fixture. `viewport` 100px shows
    /// rows 50..60 when scrolled to 500.
    fn scrolled(offset: f32, viewport: f32, overscan: usize) -> UseVirtualTable {
        let mut table = UseVirtualTable::new(1000, 10.0).with_overscan(overscan);
        table.note_viewport(viewport);
        table.on_scroll(offset);
        table
    }

    #[test]
    fn defaults_match_the_react_signature() {
        let table = UseVirtualTable::default();
        assert_eq!(table.options.overscan, 10, "overscan = 10 default");
        assert_eq!(table.options.row_count, 0);
        assert_eq!(table.viewport(), 0.0);
        assert_eq!(table.offset(), 0.0);
        // An unmeasured surface mounts nothing (no clientHeight yet).
        assert!(table.virtual_rows().is_empty());
    }

    #[test]
    fn window_pads_overscan_on_both_edges() {
        // Viewport shows 50..60; overscan 10 mounts 40..70.
        let table = scrolled(500.0, 100.0, 10);
        let rows = table.virtual_rows();
        assert_eq!(rows.first().map(|r| r.index), Some(40), "pad above");
        assert_eq!(rows.last().map(|r| r.index), Some(69), "pad below");
        assert_eq!(rows.len(), 30);
    }

    #[test]
    fn a_partial_tail_row_counts_as_visible() {
        // offset 5000 + viewport 105 reaches 5105: row 510 is 5px
        // visible, so the window's end is past it. Overscan 0 to
        // isolate the ceil rule from the pad.
        let table = scrolled(5000.0, 105.0, 0);
        let rows = table.virtual_rows();
        assert_eq!(rows.first().map(|r| r.index), Some(500));
        assert_eq!(rows.last().map(|r| r.index), Some(510), "ceil includes the partial tail");
    }

    #[test]
    fn the_window_clamps_to_the_row_count() {
        // Near the end: last_visible + overscan runs past 1000.
        let table = scrolled(9950.0, 100.0, 10);
        let rows = table.virtual_rows();
        assert_eq!(rows.last().map(|r| r.index), Some(999));
        assert_eq!(rows.len(), 15, "rows 985..1000");
    }

    #[test]
    fn rows_carry_absolute_offsets() {
        let table = scrolled(500.0, 100.0, 10);
        let rows = table.virtual_rows();
        let first = rows[0];
        assert_eq!(first.index, 40);
        assert_eq!(first.start, 400.0, "start = index * rowHeight");
        assert_eq!(first.size, 10.0);
        // Every row's start is its own index times the height.
        for row in &rows {
            assert_eq!(row.start, row.index as f32 * 10.0);
        }
    }

    #[test]
    fn degenerate_dimensions_mount_nothing() {
        // No rows.
        let mut table = UseVirtualTable::new(0, 10.0);
        table.note_viewport(100.0);
        assert!(table.virtual_rows().is_empty());
        // Viewport not yet measured (or zero).
        let mut table = UseVirtualTable::new(10, 10.0);
        table.note_viewport(0.0);
        assert!(table.virtual_rows().is_empty());
        // A zero row height cannot place a row.
        let mut table = UseVirtualTable::new(10, 0.0);
        table.note_viewport(100.0);
        assert!(table.virtual_rows().is_empty(), "zero row height mounts nothing");
    }

    #[test]
    fn total_height_is_the_full_list_extent() {
        let table = UseVirtualTable::new(7, 13.5);
        assert_eq!(table.total_height(), 94.5);
        // The window may be small; the spacer is not.
        let mut scrolled_down = UseVirtualTable::new(1000, 10.0);
        scrolled_down.note_viewport(100.0);
        scrolled_down.on_scroll(500.0);
        assert_eq!(scrolled_down.total_height(), 10000.0);
        assert_eq!(scrolled_down.virtual_rows().len(), 30);
    }

    #[test]
    fn a_fitting_content_refuses_to_scroll() {
        // 10 rows of 10px (100 total) inside a 200px viewport.
        let mut table = UseVirtualTable::new(10, 10.0);
        table.note_viewport(200.0);
        assert_eq!(table.scroll_to_row(0), None);
        assert_eq!(table.scroll_to_row(5), None, "nothing to scroll when it fits");
    }

    #[test]
    fn scroll_targets_clamp_to_the_last_page() {
        // 1000 rows of 10px, viewport 100: the furthest offset is
        // 9900 - ... = 10000 - 100 = 9900? No: 10000 - 100 = 9900.
        let table = scrolled(0.0, 100.0, 10);
        assert_eq!(table.scroll_to_row(0), Some(0.0));
        assert_eq!(table.scroll_to_row(50), Some(500.0));
        // Row 999's top (9990) would pass the end; clamped to 9900.
        assert_eq!(table.scroll_to_row(999), Some(9900.0), "clamped to max offset");
        assert_eq!(table.scroll_to_row(2000), Some(9900.0), "past-the-end index clamps too");
    }

    #[test]
    fn on_scroll_moves_the_window() {
        let mut table = UseVirtualTable::new(1000, 10.0).with_overscan(12);
        table.note_viewport(300.0);
        table.on_scroll(0.0);
        assert_eq!(table.virtual_rows().first().map(|r| r.index), Some(0));
        // The VirtualBody scroll: 12 overscan, 300px viewport.
        table.on_scroll(3000.0);
        let rows = table.virtual_rows();
        assert_eq!(rows.first().map(|r| r.index), Some(288));
        assert_eq!(rows.last().map(|r| r.index), Some(341));
        assert_eq!(table.offset(), 3000.0);
    }

    #[test]
    fn replacing_the_rows_rewindows() {
        let mut table = UseVirtualTable::new(1000, 10.0);
        table.note_viewport(100.0);
        table.on_scroll(5000.0);
        assert_eq!(table.virtual_rows().len(), 30);
        // The query result shrank to 60 rows while scrolled at 5000:
        // the stale offset windows past the end and mounts nothing —
        // tanstack leaves the clamp to the scroll element, so the
        // host re-reports the clamped offset like a DOM would.
        table.set_rows_len(60);
        assert!(table.virtual_rows().is_empty(), "a stale offset past the end mounts nothing");
        assert_eq!(table.total_height(), 600.0);
        table.on_scroll(500.0);
        let rows = table.virtual_rows();
        assert_eq!(rows.first().map(|r| r.index), Some(40));
        assert_eq!(rows.last().map(|r| r.index), Some(59), "clamped to the new count");
        // And a later resize of the row height re-derives offsets.
        table.set_row_height(20.0);
        assert_eq!(table.total_height(), 1200.0);
        let rerowed = table.virtual_rows();
        assert_eq!(rerowed[0].size, 20.0);
        assert_eq!(rerowed[0].start, rerowed[0].index as f32 * 20.0);
    }
}
