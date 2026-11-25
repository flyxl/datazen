//! Independent T076 retest. Expectations come from
//! `windows/backup/ProgressLog.tsx` and `@tanstack/virtual-core@3.13.6`
//! (`calculateRange` + `defaultRangeExtractor`, lanes = 1), not from the
//! in-tree audit fixtures.
//!
//! Documented component deviations (empty log renders the spacer, no row
//! `title`, engine `virtual_rows` instead of a hand-placed `translateY`)
//! are not re-litigated here. The model claims its arithmetic window is
//! the window React shows.

use datazen_ui::views::{
    copy_text, is_near_bottom, should_auto_scroll, total_size, visible_range, ProgressLogState,
    COPIED_FEEDBACK_MS, OVERSCAN, ROW_HEIGHT,
};

/// TanStack 3.13.6 half-open render range for a fixed 20px row.
/// `outerSize <= 0` or `count == 0` yields no items. A scroll offset past
/// the last row still clamps onto that row, then overscan walks backward.
fn tanstack_exclusive(scroll_offset: f64, outer_size: f64, count: usize) -> (usize, usize) {
    if count == 0 || outer_size <= 0.0 {
        return (0, 0);
    }
    let row = 20.0;
    let start_of = |index: usize| index as f64 * row;
    let end_of = |index: usize| (index + 1) as f64 * row;
    let last = count - 1;
    let (start_index, end_index) = if count <= 1 {
        (0, last)
    } else {
        let start_index = find_nearest(count, scroll_offset, &start_of);
        let mut end_index = start_index;
        while end_index < last && end_of(end_index) < scroll_offset + outer_size {
            end_index += 1;
        }
        (start_index, end_index)
    };
    let start = start_index.saturating_sub(20);
    let end_inclusive = (end_index + 20).min(last);
    (start, end_inclusive + 1)
}

fn find_nearest(count: usize, value: f64, get: &impl Fn(usize) -> f64) -> usize {
    let mut low = 0isize;
    let mut high = count as isize - 1;
    while low <= high {
        let middle = (low + high) / 2;
        let current = get(middle as usize);
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

#[test]
fn constants_match_progress_log_tsx() {
    assert_eq!(ROW_HEIGHT, 20.0);
    assert_eq!(OVERSCAN, 20);
    assert_eq!(COPIED_FEEDBACK_MS, 1500);
    assert_eq!(total_size(0), 0.0);
    assert_eq!(total_size(7), 140.0);
}

#[test]
fn near_bottom_uses_two_rows_and_empty_log_stays_pinned() {
    // scrollTop + clientHeight >= scrollHeight - ROW_HEIGHT * 2
    assert!(is_near_bottom(100.0, 80.0, 220.0));
    assert!(is_near_bottom(100.0, 80.0, 220.0));
    assert!(is_near_bottom(99.0, 80.0, 219.0));
    assert!(!is_near_bottom(98.9, 80.0, 219.0));
    assert!(is_near_bottom(0.0, 0.0, 0.0));
    assert!(!should_auto_scroll(true, true));
    assert!(should_auto_scroll(false, true));
    assert!(!should_auto_scroll(false, false));
}

#[test]
fn copy_text_joins_with_newline_and_skips_only_a_blank_join() {
    assert_eq!(copy_text(&[]), None);
    assert_eq!(copy_text(&[String::new()]), None);
    assert_eq!(copy_text(&[String::new(), String::new()]), Some("\n".to_owned()));
    assert_eq!(
        copy_text(&["alpha".to_owned(), String::new(), "beta".to_owned()]),
        Some("alpha\n\nbeta".to_owned())
    );
}

#[test]
fn copy_feedback_is_optimistic_and_a_late_failure_cannot_clear_a_newer_click() {
    let mut state = ProgressLogState::new();
    assert!(state.auto_scroll);
    let first = state.copy_pressed();
    assert!(state.copied);
    let second = state.copy_pressed();
    assert_ne!(first, second);
    assert!(!state.expire_copy_feedback(first));
    assert!(state.copied);
    assert!(!state.fail_copy(first));
    assert!(state.copied);
    assert!(state.fail_copy(second));
    assert!(!state.copied);
    let third = state.copy_pressed();
    assert!(state.expire_copy_feedback(third));
    assert!(!state.copied);
}

#[test]
fn scroll_listener_updates_the_pin_and_empty_lines_do_not_scroll() {
    let mut state = ProgressLogState::new();
    assert!(state.on_scroll(0.0, 100.0, 400.0) == false);
    assert!(!state.should_scroll(false));
    assert!(state.on_scroll(260.0, 100.0, 400.0));
    assert!(state.should_scroll(false));
    assert!(!state.should_scroll(true));
}

#[test]
fn negative_scroll_is_clamped_like_the_browser() {
    // Documented: a browser never reports a negative scrollTop, and the
    // helper clamps. This is not the raw TanStack input, and it is not a miss.
    assert_eq!(visible_range(-40.0, 40.0, 80), visible_range(0.0, 40.0, 80));
}

#[test]
fn visible_range_matches_tanstack_virtual_core_3_13_6() {
    // Positive sizes inside the range a browser can actually scroll, plus the
    // two inputs TanStack treats specially: a zero viewport (first measure)
    // and a scroll offset past the last row.
    let cases = [
        (0.0, 0.0, 40usize),
        (0.0, -8.0, 40),
        (0.0, 40.0, 100),
        (0.0, 20.0, 50),
        (20.0, 20.0, 50),
        (25.0, 30.0, 50),
        (180.0, 40.0, 10),
        (900.0, 100.0, 50),
        (1000.0, 100.0, 50),
        (5_000.0, 200.0, 100),
        (0.0, 100.0, 1),
        (10_000.0, 40.0, 1),
        (0.0, 1.0, 5),
        (39.0, 2.0, 3),
        (200.0, 40.0, 10),
    ];
    let mut misses = Vec::new();
    for (scroll, view, count) in cases {
        let got = visible_range(scroll, view, count);
        let expect = tanstack_exclusive(scroll, view, count);
        if got != expect {
            misses.push(format!(
                "scroll={scroll} view={view} count={count} rust={got:?} tanstack={expect:?}"
            ));
        }
    }
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}
