//! T076-S1: the `ProgressLog` model — virtual-window math, the
//! auto-scroll decision, and copy feedback (`windows/backup/ProgressLog.tsx`).
//!
//! This slice is pure logic plus state only; the component that renders
//! it (S2) reads these helpers and owns the scroll container. React
//! renders through `@tanstack/react-virtual` (`useVirtualizer`, fixed
//! `estimateSize` of [`ROW_HEIGHT`], `overscan` of [`OVERSCAN`]); the
//! helpers below reproduce that window arithmetically, because every
//! row really is [`ROW_HEIGHT`] high — there is no measurement cache
//! to drift from the estimate, so the arithmetic window is the window
//! React shows.
//!
//! Where React lives in the Backup window, see
//! [`super::backup_window_state::progress_log_mount`]: S2 replaces the
//! inline log rendering in [`super::backup_window`] with the real
//! `ProgressLog`, keeping the same keys.
//!
//! ## Deviations from React, by design
//!
//! * **The virtual window is computed, not measured.** TanStack keeps a
//!   measurement cache and can shift the window by a row while it
//!   measures; here every row is exactly [`ROW_HEIGHT`], so
//!   [`visible_range`] is `calculateRange` + `defaultRangeExtractor`
//!   (one lane, overscan 20). A viewport that is not positive renders
//!   no rows. A scroll offset past the last row clamps onto that row.
//! * **Scroll geometry is `f64`, not `f32`.** React reads `scrollTop` /
//!   `clientHeight` / `scrollHeight` as JS numbers (f64); the helpers
//!   take the same type instead of narrowing to `f32`.
//! * **The copy-feedback timer lives on the host.** React's
//!   `useCopyFeedback` arms a `setTimeout`; Rune has no component-local
//!   timers, so [`ProgressLogState::copy_pressed`] only marks the
//!   confirmation and returns a request id — the host arms the
//!   [`COPIED_FEEDBACK_MS`] timer and reports back through
//!   [`ProgressLogState::expire_copy_feedback`] /
//!   [`ProgressLogState::fail_copy`], exactly like T060's
//!   `AiCodeBlockState`.
//! * **Row height ignores content.** A row is [`ROW_HEIGHT`] whether the
//!   line is ASCII, carries a U+FEFF prefix, or is full of multi-byte
//!   characters — the log pump (`super::backup_progress`) truncates
//!   over-long lines, but nothing here measures text. Do not re-introduce
//!   the `trim_line` byte-vs-char question here; it does not apply.

/// React `ROW_HEIGHT` (`ProgressLog.tsx`): every log row is 20pt high,
/// with a matching 20px `line-height`.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub const ROW_HEIGHT: f64 = 20.0;

/// React `overscan: 20` (`ProgressLog.tsx`): rows rendered above and
/// below the visible window.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub const OVERSCAN: usize = 20;

/// React `COPIED_FEEDBACK_MS` (`ProgressLog.tsx`): how long the "log
/// copied" label stays before reverting to the copy label.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub const COPIED_FEEDBACK_MS: u64 = 1500;

/// The near-bottom margin in React's scroll listener
/// (`ProgressLog.tsx`): `scrollHeight - ROW_HEIGHT * 2`. A scroll
/// position within two rows of the bottom still counts as "at the
/// bottom", so appending lines keeps the view pinned.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub const NEAR_BOTTOM_MARGIN: f64 = ROW_HEIGHT * 2.0;

/// React's virtualizer window for a scroll position, as a half-open
/// `(start, end)` row range. This is `@tanstack/virtual-core@3.13`
/// `calculateRange` plus `defaultRangeExtractor` for one lane of fixed
/// [`ROW_HEIGHT`] rows and [`OVERSCAN`] 20.
///
/// A non-positive viewport renders nothing: `calculateRange` requires
/// `outerSize > 0`. Otherwise the anchor is the greatest row whose
/// origin is `<= scroll_top`, never past the last row, so a scroll
/// offset beyond the log still paints that row and the overscan above
/// it. `end` walks forward while the row's end is strictly above the
/// bottom of the viewport, then overscan is applied on both sides.
/// A negative `scroll_top` cannot happen in a browser and is clamped
/// to 0 before the search.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub fn visible_range(scroll_top: f64, viewport_height: f64, count: usize) -> (usize, usize) {
    if count == 0 || viewport_height <= 0.0 {
        return (0, 0);
    }
    let scroll_offset = scroll_top.max(0.0);
    let last = count - 1;
    let (start_index, end_index) = if count == 1 {
        (0, last)
    } else {
        let start_index = nearest_row_start(count, scroll_offset);
        let mut end_index = start_index;
        let bottom = scroll_offset + viewport_height;
        while end_index < last && (end_index + 1) as f64 * ROW_HEIGHT < bottom {
            end_index += 1;
        }
        (start_index, end_index)
    };
    let start = start_index.saturating_sub(OVERSCAN);
    let end_inclusive = end_index.saturating_add(OVERSCAN).min(last);
    (start, end_inclusive + 1)
}

/// TanStack `findNearestBinarySearch` over row origins `index * ROW_HEIGHT`:
/// the greatest index whose origin is `<= value`, or 0 when `value` is
/// above the first row.
fn nearest_row_start(count: usize, value: f64) -> usize {
    let mut low = 0isize;
    let mut high = count as isize - 1;
    while low <= high {
        let middle = (low + high) / 2;
        let current = middle as f64 * ROW_HEIGHT;
        if current < value {
            low = middle + 1;
        } else if current > value {
            high = middle - 1;
        } else {
            return middle as usize;
        }
    }
    if low > 0 {
        (low - 1) as usize
    } else {
        0
    }
}

/// React `virtualizer.getTotalSize()`: the inner spacer's height, one
/// [`ROW_HEIGHT`] per line.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub fn total_size(count: usize) -> f64 {
    count as f64 * ROW_HEIGHT
}

/// React `row.start`: the `translateY` offset of row `index`.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub fn row_offset(index: usize) -> f64 {
    index as f64 * ROW_HEIGHT
}

/// React's scroll listener (`ProgressLog.tsx`): the view is at the
/// bottom when `scrollTop + clientHeight >= scrollHeight -
/// ROW_HEIGHT * 2`. Verbatim, including the empty-log case: with no
/// content `scroll_height` is 0 (or the container's own height) and
/// the comparison still holds, so a fresh log starts pinned — which is
/// what the auto-scroll effect needs.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub fn is_near_bottom(scroll_top: f64, client_height: f64, scroll_height: f64) -> bool {
    scroll_top + client_height >= scroll_height - NEAR_BOTTOM_MARGIN
}

/// React's auto-scroll effect (`ProgressLog.tsx`): `if (lines.length
/// === 0 || !shouldAutoScroll.current) return;` — the host scrolls to
/// the bottom only when there is something to show and the user has
/// not scrolled away from it.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub fn should_auto_scroll(lines_empty: bool, near_bottom: bool) -> bool {
    !lines_empty && near_bottom
}

/// React `copyAll` (`ProgressLog.tsx`): `lines.join('\n')`, and no
/// copy at all when the joined text is empty. Note the edge React's
/// `if (!text) return` creates: a single empty line joins to `""` and
/// copies nothing (`None`), while two empty lines join to `"\n"` —
/// non-empty — and copy that newline (`Some("\n")`). The log pump
/// never produces blank lines, so the second case is unreachable from
/// the Backup window, but the helper keeps React's exact rule rather
/// than guessing a tidier one.
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub fn copy_text(lines: &[String]) -> Option<String> {
    let text = lines.join("\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// React's `useState` / `useRef` pieces for `ProgressLog`, kept in the
/// app's state `S` because Rune has no component-local state:
/// `useCopyFeedback`'s confirmation plus the `shouldAutoScroll` ref
/// (seeded `true`, like React).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
pub struct ProgressLogState {
    /// React `copied`: the "log copied" confirmation is showing.
    pub copied: bool,
    /// Monotonic id of the most recent copy request; `useCopyFeedback`'s
    /// `requestIdRef`, so a late failure or timer cannot clear a newer
    /// confirmation.
    pub copy_request: u64,
    /// React `shouldAutoScroll.current`: whether new lines pin the view
    /// to the bottom. Seeded `true`, updated on every scroll event.
    pub auto_scroll: bool,
}

impl Default for ProgressLogState {
    fn default() -> Self {
        Self {
            copied: false,
            copy_request: 0,
            auto_scroll: true,
        }
    }
}

#[allow(dead_code)] // T076-S2 wires this into the ProgressLog component
impl ProgressLogState {
    /// A fresh state: nothing copied, no request yet, pinned to the
    /// bottom like React's `useRef(true)`.
    pub fn new() -> Self {
        Self::default()
    }

    /// React `copy`: the optimistic mark. Sets the confirmation, arms
    /// a fresh full window (a second click owns a new request id), and
    /// returns that id for the host's failure / expiry report. The
    /// clipboard write itself stays on the host: this only records
    /// that a copy was requested.
    pub fn copy_pressed(&mut self) -> u64 {
        self.copy_request += 1;
        self.copied = true;
        self.copy_request
    }

    /// The `COPIED_FEEDBACK_MS` timer firing: clears the confirmation
    /// only when `request` is still the most recent one. Returns whether
    /// it cleared.
    pub fn expire_copy_feedback(&mut self, request: u64) -> bool {
        if request == self.copy_request {
            self.copied = false;
            true
        } else {
            false
        }
    }

    /// A failed clipboard write: rolls the confirmation back, only when
    /// `request` is still the most recent one — a late failure cannot
    /// erase a newer, successful confirmation. Returns whether it
    /// rolled back.
    pub fn fail_copy(&mut self, request: u64) -> bool {
        if request == self.copy_request {
            self.copied = false;
            true
        } else {
            false
        }
    }

    /// React's scroll listener: recomputes `shouldAutoScroll` from the
    /// scroll geometry and returns the new value.
    pub fn on_scroll(
        &mut self,
        scroll_top: f64,
        client_height: f64,
        scroll_height: f64,
    ) -> bool {
        self.auto_scroll = is_near_bottom(scroll_top, client_height, scroll_height);
        self.auto_scroll
    }

    /// React's auto-scroll effect for this state: scroll to the bottom
    /// when there are lines and the user has not scrolled away.
    pub fn should_scroll(&self, lines_empty: bool) -> bool {
        should_auto_scroll(lines_empty, self.auto_scroll)
    }
}
