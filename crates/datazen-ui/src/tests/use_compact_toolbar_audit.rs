//! T233 audit: the useCompactToolbar port, verified against the React
//! source re-read line by line and
//! `hooks/__tests__/useCompactToolbar.test.tsx` (every case mirrored).
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use crate::hooks::use_compact_toolbar::{
    estimate_expanded_toolbar_width, CompactToolbarState, DEFAULT_TOOLBAR_THRESHOLD,
    TOOLBAR_EXPANDED_BUTTON_WIDTH, TOOLBAR_GAP, TOOLBAR_HYSTERESIS,
    TOOLBAR_HORIZONTAL_PADDING,
};

// ---- estimateExpandedToolbarWidth (the React suite, case for case) ----------

#[test]
fn zero_buttons_is_the_padding_only_width() {
    assert_eq!(estimate_expanded_toolbar_width(0, 0.0), 32.0);
    assert_eq!(TOOLBAR_HORIZONTAL_PADDING, 32.0);
}

#[test]
fn the_estimate_scales_with_buttons_and_extras() {
    assert_eq!(estimate_expanded_toolbar_width(6, 120.0), 32.0 + 6.0 * 96.0 + 5.0 * 8.0 + 120.0);
    assert_eq!(TOOLBAR_EXPANDED_BUTTON_WIDTH, 96.0);
    assert_eq!(TOOLBAR_GAP, 8.0);
    // One button has no gap; extras ride after.
    assert_eq!(estimate_expanded_toolbar_width(1, 10.0), 32.0 + 96.0 + 10.0);
    // Extras never go negative.
    assert_eq!(estimate_expanded_toolbar_width(2, -5.0), 32.0 + 2.0 * 96.0 + 8.0);
}

// ---- the hysteresis state machine (the React suite, case for case) ----------

#[test]
fn starts_expanded_at_or_above_the_threshold() {
    let mut state = CompactToolbarState::new();
    state.on_resize(400.0, 400.0);
    assert!(!state.compact(), "width == threshold stays expanded");
    let mut state = CompactToolbarState::new();
    state.on_resize(600.0, 400.0);
    assert!(!state.compact(), "width above threshold stays expanded");
}

#[test]
fn compacts_below_the_threshold() {
    let mut state = CompactToolbarState::new();
    state.on_resize(300.0, 400.0);
    assert!(state.compact(), "width below threshold compacts");
    // Even when the content exactly matches the container (the React
    // suite's scrollWidth == clientWidth arm).
    let mut state = CompactToolbarState::new();
    state.on_resize(350.0, 400.0);
    assert!(state.compact());
}

#[test]
fn expanding_again_needs_the_hysteresis_slack() {
    assert_eq!(TOOLBAR_HYSTERESIS, 16.0);
    let mut state = CompactToolbarState::new();
    state.on_resize(300.0, 400.0);
    assert!(state.compact());

    // threshold + 16 exactly: expand (the rule is `< threshold + 16`
    // keeps compact, so 416 expands).
    state.on_resize(416.0, 400.0);
    assert!(!state.compact(), "threshold + 16 expands");
}

#[test]
fn stays_compact_between_threshold_and_threshold_plus_slack() {
    let mut state = CompactToolbarState::new();
    state.on_resize(300.0, 400.0);
    assert!(state.compact());
    // 410: >= threshold but < threshold + 16 — the dead zone keeps
    // compact (no flicker at the boundary).
    state.on_resize(410.0, 400.0);
    assert!(state.compact());
    // ...and it recovers only past the slack.
    state.on_resize(417.0, 400.0);
    assert!(!state.compact());
}

/// The React suite's core-bug case: a client width AT the threshold
/// stays expanded even when the content overflows slightly (the rule
/// never consults scrollWidth — the host feeds only the container
/// width, structurally).
#[test]
fn the_threshold_is_authoritative_no_scroll_width() {
    let mut state = CompactToolbarState::new();
    state.on_resize(400.0, 400.0);
    assert!(!state.compact(), "at the threshold, expanded — overflow is not a signal");
}

/// Widths without layout (`<= 0`) are ignored (React's early return).
#[test]
fn zero_widths_are_ignored() {
    let mut state = CompactToolbarState::new();
    state.on_resize(300.0, 400.0);
    assert!(state.compact());
    state.on_resize(0.0, 400.0);
    assert!(state.compact(), "a zero measurement changes nothing");
    let mut state = CompactToolbarState::new();
    state.on_resize(-5.0, 400.0);
    assert!(!state.compact(), "a negative measurement changes nothing");
}

/// The hook's default threshold is 920 (React's parameter default).
#[test]
fn the_default_threshold_is_920() {
    assert_eq!(DEFAULT_TOOLBAR_THRESHOLD, 920.0);
    let mut state = CompactToolbarState::new();
    state.on_resize(919.0, DEFAULT_TOOLBAR_THRESHOLD);
    assert!(state.compact());
    let mut state = CompactToolbarState::new();
    state.on_resize(920.0, DEFAULT_TOOLBAR_THRESHOLD);
    assert!(!state.compact());
}
