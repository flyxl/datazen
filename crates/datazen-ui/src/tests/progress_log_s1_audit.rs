//! T076-S1 independent audit: `ProgressLog` model cross-checked against
//! an independent re-implementation of the TanStack virtualizer, plus
//! copy-feedback ordering the coder tests do not combine
//! (`views/backup/progress_log_state.rs`, ported from
//! `windows/backup/ProgressLog.tsx`).
//!
//! The TanStack model below is written from the installed
//! `@tanstack/virtual-core` 3.13.23 source (`calculateRange` +
//! `defaultRangeExtractor`, lanes = 1, fixed 20pt rows, no
//! `measureElement` attached — `ProgressLog.tsx` never measures rows,
//! so the estimate is exact and no measurement cache can drift), not
//! from `progress_log_state.rs`. The sweep only covers the
//! browser-reachable domain (`0 <= scroll_top <= count * 20 -
//! viewport`, `viewport > 0`); outside it the two deliberately differ
//! — see `visible_range_past_end_differs_from_tanstack_by_design`.

use crate::views::{visible_range, ProgressLogState};

/// Independent model of TanStack's window for fixed 20pt rows, as a
/// half-open `(start, end)` range. Returns `(0, 0)` where TanStack
/// returns no items at all (empty list, or a zero-height viewport:
/// `calculateRange` requires `outerSize > 0`).
fn tanstack_window(scroll_top: f64, viewport_height: f64, count: usize) -> (usize, usize) {
    if count == 0 || viewport_height <= 0.0 {
        return (0, 0);
    }
    let top = scroll_top.max(0.0);
    let last = count - 1;
    // startIndex: greatest i with i * 20 <= top (binary search over
    // the row starts), clamped to the last row.
    let start_index = ((top / 20.0).floor() as usize).min(last);
    // endIndex: smallest i with (i + 1) * 20 >= top + viewport.
    let mut end_index = start_index;
    while end_index < last && (end_index as f64 + 1.0) * 20.0 < top + viewport_height {
        end_index += 1;
    }
    // defaultRangeExtractor: overscan is per side, inclusive end.
    let start = start_index.saturating_sub(20);
    let end = (end_index + 20).min(last);
    (start, end + 1)
}

#[test]
fn audit_visible_range_matches_tanstack_on_the_reachable_domain() {
    for count in [1usize, 2, 10, 21, 100, 1500] {
        for viewport in [1.0f64, 20.0, 50.0, 200.0, 333.0] {
            let max_scroll = (count as f64 * 20.0 - viewport).max(0.0);
            // Exact boundaries, fractional offsets, and the maximum
            // reachable scroll position (scrollTop clamps to
            // scrollHeight - clientHeight in a browser).
            let mut positions = vec![0.0, 0.5, 19.9, 20.0, 37.0, 100.0, 400.0, max_scroll];
            if max_scroll > 0.0 {
                positions.push((max_scroll - 0.5).max(0.0));
            }
            for scroll_top in positions {
                if scroll_top > max_scroll {
                    continue;
                }
                assert_eq!(
                    visible_range(scroll_top, viewport, count),
                    tanstack_window(scroll_top, viewport, count),
                    "count={count} viewport={viewport} scroll_top={scroll_top}"
                );
            }
        }
    }
}

#[test]
fn audit_visible_range_past_end_differs_from_tanstack_by_design() {
    // A browser clamps scrollTop to scrollHeight - clientHeight, so
    // scrollTop == total height is unreachable; the brief specifies
    // the empty (count, count) range there instead of TanStack's
    // tail-overscan band. Pinned so the divergence is a decision,
    // not an accident. (TanStack would give (79, 100) here: its
    // startIndex clamps to the last row, 99, minus overscan 20.)
    assert_eq!(visible_range(2000.0, 200.0, 100), (80, 100));
    assert_eq!(visible_range(5000.0, 200.0, 100), (100, 100));
    assert_eq!(tanstack_window(2000.0, 200.0, 100), (79, 100));
}

#[test]
fn audit_visible_range_zero_viewport_differs_from_tanstack_by_design() {
    // TanStack returns no items when outerSize is 0 (calculateRange
    // requires outerSize > 0); the helper still returns the overscan
    // band around the scroll position. A zero-height container shows
    // nothing either way, so this is unreachable in practice.
    assert_eq!(visible_range(400.0, 0.0, 100), (0, 40));
    assert_eq!(tanstack_window(400.0, 0.0, 100), (0, 0));
}

#[test]
fn audit_copy_feedback_fail_then_repress_then_stale_reports() {
    // A failed first copy rolls back; a second click owns a fresh
    // full window; stale reports from the first click change nothing.
    let mut state = ProgressLogState::new();
    let first = state.copy_pressed();
    assert!(state.fail_copy(first));
    assert!(!state.copied);
    let second = state.copy_pressed();
    assert!(state.copied);
    assert!(!state.expire_copy_feedback(first));
    assert!(!state.fail_copy(first));
    assert!(state.copied);
    assert_eq!(state.copy_request, second);
    assert!(state.fail_copy(second));
    assert!(!state.copied);
}

#[test]
fn audit_copy_feedback_second_click_survives_first_timer() {
    // useCopyFeedback clears the first timer on a second click, so
    // the second click owns the full window. Here the same guarantee
    // comes from the request guard: the first timer's expiry is stale
    // and ignored, and only the second timer clears the mark.
    let mut state = ProgressLogState::new();
    let first = state.copy_pressed();
    let second = state.copy_pressed();
    assert!(!state.expire_copy_feedback(first));
    assert!(state.copied);
    assert!(state.expire_copy_feedback(second));
    assert!(!state.copied);
}
