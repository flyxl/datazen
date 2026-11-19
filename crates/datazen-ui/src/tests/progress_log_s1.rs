//! T076-S1: `ProgressLog` model tests — virtual-window math, total
//! size / row offsets, the near-bottom rule, the auto-scroll guard,
//! the copy text, and the copy-feedback state machine
//! (`views/backup/progress_log_state.rs`, ported from
//! `windows/backup/ProgressLog.tsx`).

use crate::views::{
    copy_text, is_near_bottom, row_offset, should_auto_scroll, total_size, visible_range,
    ProgressLogState, COPIED_FEEDBACK_MS, OVERSCAN, ROW_HEIGHT,
};

fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

// --- visible_range ---

#[test]
fn visible_range_empty_log_is_empty_at_any_scroll() {
    assert_eq!(visible_range(0.0, 200.0, 0), (0, 0));
    assert_eq!(visible_range(5000.0, 200.0, 0), (0, 0));
}

#[test]
fn visible_range_single_row_renders_it() {
    assert_eq!(visible_range(0.0, 200.0, 1), (0, 1));
    assert_eq!(visible_range(100.0, 200.0, 1), (0, 1));
}

#[test]
fn visible_range_exactly_one_screen_clamps_overscan() {
    // 10 rows * 20pt = 200pt viewport: every row visible, overscan has
    // nowhere to go on either side.
    assert_eq!(visible_range(0.0, 200.0, 10), (0, 10));
}

#[test]
fn visible_range_mid_scroll_adds_overscan_both_sides() {
    // scroll_top 1000 -> first visible row 50; viewport 200 -> 10 rows
    // visible (50..60); overscan 20 each side -> (30, 80).
    assert_eq!(visible_range(1000.0, 200.0, 100), (30, 80));
}

#[test]
fn visible_range_top_clamps_overscan_at_zero() {
    // scroll_top 100 -> first visible row 5; overscan would start at -15.
    assert_eq!(visible_range(100.0, 200.0, 100), (0, 35));
}

#[test]
fn visible_range_bottom_clamps_overscan_at_count() {
    // scroll_top 1900 -> first visible row 95; the window runs past the
    // last row (99) and clamps the end at 100.
    assert_eq!(visible_range(1900.0, 200.0, 100), (75, 100));
}

#[test]
fn visible_range_scrolled_past_end_is_empty() {
    assert_eq!(visible_range(5000.0, 200.0, 100), (100, 100));
    // Exactly at the end (scroll_top == total height) the overscan band
    // still shows the tail rows; only far past the end is it empty.
    assert_eq!(visible_range(2000.0, 200.0, 100), (80, 100));
}

#[test]
fn visible_range_negative_scroll_is_treated_as_top() {
    assert_eq!(visible_range(-50.0, 200.0, 100), (0, 30));
    assert_eq!(
        visible_range(-50.0, 200.0, 100),
        visible_range(0.0, 200.0, 100)
    );
}

#[test]
fn visible_range_zero_viewport_still_covers_overscan() {
    // No visible rows, but the overscan band around row 20 renders.
    assert_eq!(visible_range(400.0, 0.0, 100), (0, 40));
}

#[test]
fn visible_range_fractional_scroll_counts_partial_rows() {
    // scroll_top 25.5 -> first visible row 1 (row 1 is 5.5pt in view);
    // bottom edge 125.5 -> row 6 is partially visible, ceil -> 7.
    assert_eq!(visible_range(25.5, 100.0, 100), (0, 27));
}

#[test]
fn visible_range_exact_row_boundary() {
    // scroll_top 200 -> first visible row 10 exactly; bottom edge 400
    // -> row 19 is the last fully visible row, ceil -> 20.
    assert_eq!(visible_range(200.0, 200.0, 100), (0, 40));
}

#[test]
fn visible_range_never_exceeds_count_and_start_never_exceeds_end() {
    for count in [0usize, 1, 2, 19, 20, 21, 100, 1500] {
        for scroll_top in [-100.0, 0.0, 37.0, 400.0, 1999.0, 29999.0, 1.0e6] {
            let (start, end) = visible_range(scroll_top, 200.0, count);
            assert!(
                start <= end,
                "count={count} scroll_top={scroll_top}: {start} > {end}"
            );
            assert!(
                end <= count,
                "count={count} scroll_top={scroll_top}: end {end} > {count}"
            );
        }
    }
}

#[test]
fn visible_range_content_never_changes_row_height() {
    // Row height is a fixed 20pt: a U+FEFF prefix, emoji, and CJK text
    // occupy exactly one row each, like ASCII.
    let log = lines(&["\u{feff}first", "second 🧱 line", "第三行"]);
    assert_eq!(log.len(), 3);
    assert_eq!(visible_range(0.0, 200.0, log.len()), (0, 3));
    assert_eq!(total_size(log.len()), 60.0);
    assert_eq!(row_offset(2), 40.0);
}

// --- total_size / row_offset ---

#[test]
fn total_size_is_count_times_row_height() {
    assert_eq!(total_size(0), 0.0);
    assert_eq!(total_size(1), 20.0);
    assert_eq!(total_size(10), 200.0);
    assert_eq!(total_size(1500), 30000.0);
}

#[test]
fn row_offset_is_index_times_row_height() {
    assert_eq!(row_offset(0), 0.0);
    assert_eq!(row_offset(1), 20.0);
    assert_eq!(row_offset(7), 140.0);
    assert_eq!(row_offset(1499), 29980.0);
}

#[test]
fn row_offsets_and_total_size_agree() {
    // The last row starts at total_size(count - 1) and ends exactly at
    // total_size(count): no gaps, no overlap.
    for count in [1usize, 2, 50, 1500] {
        assert_eq!(row_offset(count - 1) + ROW_HEIGHT, total_size(count));
    }
}

// --- is_near_bottom ---

#[test]
fn is_near_bottom_true_at_the_bottom() {
    assert!(is_near_bottom(800.0, 200.0, 1000.0));
    assert!(is_near_bottom(1000.0, 200.0, 1000.0));
}

#[test]
fn is_near_bottom_threshold_is_two_rows_above_the_bottom() {
    // scrollHeight 1000, clientHeight 200: the threshold is scrollTop
    // >= 760. One pixel either side flips the answer.
    assert!(is_near_bottom(760.0, 200.0, 1000.0));
    assert!(!is_near_bottom(759.0, 200.0, 1000.0));
}

#[test]
fn is_near_bottom_false_when_scrolled_up() {
    assert!(!is_near_bottom(0.0, 200.0, 1000.0));
    assert!(!is_near_bottom(500.0, 200.0, 1000.0));
    assert!(!is_near_bottom(700.0, 200.0, 1000.0));
}

#[test]
fn is_near_bottom_true_for_an_empty_log() {
    // React's verbatim rule: scrollHeight 0 makes the threshold -40,
    // so a fresh log starts pinned. Kept, not "fixed".
    assert!(is_near_bottom(0.0, 0.0, 0.0));
    assert!(is_near_bottom(0.0, 200.0, 0.0));
}

#[test]
fn is_near_bottom_true_when_overscrolled_past_the_end() {
    assert!(is_near_bottom(1200.0, 200.0, 1000.0));
}

// --- should_auto_scroll ---

#[test]
fn should_auto_scroll_only_with_lines_and_pinned_view() {
    assert!(should_auto_scroll(false, true));
    assert!(!should_auto_scroll(true, true));
    assert!(!should_auto_scroll(false, false));
    assert!(!should_auto_scroll(true, false));
}

// --- copy_text ---

#[test]
fn copy_text_empty_log_copies_nothing() {
    assert_eq!(copy_text(&[]), None);
}

#[test]
fn copy_text_single_empty_line_copies_nothing() {
    // React: [""] joins to "" which is falsy, so copyAll returns early.
    assert_eq!(copy_text(&lines(&[""])), None);
}

#[test]
fn copy_text_two_empty_lines_copy_the_newline() {
    // React's exact rule, not a tidier one: ["", ""] joins to "\n",
    // which is truthy, so it copies. Unreachable from the Backup window
    // (the pump drops blank lines) but pinned here.
    assert_eq!(copy_text(&lines(&["", ""])), Some("\n".to_string()));
}

#[test]
fn copy_text_joins_lines_with_newlines_and_no_trailing_newline() {
    assert_eq!(
        copy_text(&lines(&["one", "two", "three"])),
        Some("one\ntwo\nthree".to_string())
    );
    assert_eq!(copy_text(&lines(&["only"])), Some("only".to_string()));
}

#[test]
fn copy_text_keeps_feff_and_multibyte_content_verbatim() {
    assert_eq!(
        copy_text(&lines(&["\u{feff}bom", "emoji 🧱", "中文行"])),
        Some("\u{feff}bom\nemoji 🧱\n中文行".to_string())
    );
}

// --- ProgressLogState: constants ---

#[test]
fn constants_match_react() {
    assert_eq!(ROW_HEIGHT, 20.0);
    assert_eq!(OVERSCAN, 20);
    assert_eq!(COPIED_FEEDBACK_MS, 1500);
}

// --- ProgressLogState: defaults ---

#[test]
fn state_defaults_to_uncopied_with_no_request_and_pinned() {
    let state = ProgressLogState::new();
    assert!(!state.copied);
    assert_eq!(state.copy_request, 0);
    assert!(state.auto_scroll);
    assert_eq!(state, ProgressLogState::default());
}

// --- ProgressLogState: copy feedback ---

#[test]
fn copy_pressed_marks_copied_and_returns_a_fresh_request() {
    let mut state = ProgressLogState::new();
    let first = state.copy_pressed();
    assert_eq!(first, 1);
    assert!(state.copied);
    assert_eq!(state.copy_request, 1);
    let second = state.copy_pressed();
    assert_eq!(second, 2);
    assert!(state.copied);
    assert_eq!(state.copy_request, 2);
}

#[test]
fn expire_copy_feedback_clears_only_the_current_request() {
    let mut state = ProgressLogState::new();
    let request = state.copy_pressed();
    assert!(state.expire_copy_feedback(request));
    assert!(!state.copied);
}

#[test]
fn expire_copy_feedback_ignores_a_stale_request() {
    let mut state = ProgressLogState::new();
    let first = state.copy_pressed();
    let second = state.copy_pressed();
    assert!(!state.expire_copy_feedback(first));
    assert!(state.copied);
    assert_eq!(state.copy_request, second);
    assert!(state.expire_copy_feedback(second));
    assert!(!state.copied);
}

#[test]
fn expire_copy_feedback_before_any_copy_is_a_noop() {
    let mut state = ProgressLogState::new();
    assert!(!state.expire_copy_feedback(1));
    assert!(!state.copied);
}

#[test]
fn fail_copy_rolls_back_only_the_current_request() {
    let mut state = ProgressLogState::new();
    let request = state.copy_pressed();
    assert!(state.fail_copy(request));
    assert!(!state.copied);
}

#[test]
fn fail_copy_ignores_a_stale_failure() {
    let mut state = ProgressLogState::new();
    let first = state.copy_pressed();
    let second = state.copy_pressed();
    assert!(!state.fail_copy(first));
    assert!(state.copied);
    assert!(state.fail_copy(second));
    assert!(!state.copied);
}

#[test]
fn expire_then_fail_on_the_same_request_keeps_copied_false() {
    // The request is still current after expiry, so fail_copy reports
    // true (it matched) — but there is nothing left to roll back.
    let mut state = ProgressLogState::new();
    let request = state.copy_pressed();
    assert!(state.expire_copy_feedback(request));
    assert!(state.fail_copy(request));
    assert!(!state.copied);
}

// --- ProgressLogState: scroll tracking ---

#[test]
fn on_scroll_pins_when_near_bottom_and_unpins_when_scrolled_up() {
    let mut state = ProgressLogState::new();
    assert!(state.on_scroll(800.0, 200.0, 1000.0));
    assert!(state.auto_scroll);
    assert!(!state.on_scroll(500.0, 200.0, 1000.0));
    assert!(!state.auto_scroll);
    assert!(state.on_scroll(760.0, 200.0, 1000.0));
    assert!(state.auto_scroll);
}

#[test]
fn should_scroll_follows_lines_and_the_scroll_position() {
    let mut state = ProgressLogState::new();
    assert!(state.should_scroll(false));
    assert!(!state.should_scroll(true));
    state.on_scroll(0.0, 200.0, 1000.0);
    assert!(!state.should_scroll(false));
    assert!(!state.should_scroll(true));
}
