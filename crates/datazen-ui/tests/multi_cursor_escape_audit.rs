//! T283 audit: the multiCursorEscape decision, verified against the
//! React source re-read line by line (the escape journey tests in
//! `paste/__tests__/multiCursorExitEscape.journey.test.ts` pin the
//! keymap layer; the decision table below is re-derived from the
//! command's own body).
//!
//! Tester-written: every expectation below is re-derived from the React
//! file.

use datazen_ui::components::sql_editor::multi_cursor_escape::{
    exit_multi_cursor_decision, EscapeDecision,
};

/// S0 — one cursor (empty or not): the command declines, leaving
/// Escape to `simplifySelection` (a non-empty range becomes a cursor,
/// an empty one a no-op). It never shadows the single-cursor case.
#[test]
fn one_range_declines() {
    assert_eq!(exit_multi_cursor_decision(1, 42), EscapeDecision::Decline);
    // A zero-count cannot occur (a selection always holds at least one
    // range), but the guard's shape covers it the same way.
    assert_eq!(exit_multi_cursor_decision(0, 0), EscapeDecision::Decline);
}

/// S1 — two or more cursors: collapse to exactly one bare cursor at the
/// main range's head (the last-moved range), dropping any rectangular
/// state.
#[test]
fn two_or_more_ranges_collapse_to_the_main_head() {
    assert_eq!(
        exit_multi_cursor_decision(2, 17),
        EscapeDecision::CollapseToMainHead { head: 17 }
    );
    assert_eq!(
        exit_multi_cursor_decision(5, 0),
        EscapeDecision::CollapseToMainHead { head: 0 }
    );
    assert_eq!(
        exit_multi_cursor_decision(64, 1_000_000),
        EscapeDecision::CollapseToMainHead { head: 1_000_000 }
    );
}

/// The exit state is unambiguous: the surviving range is EMPTY (a bare
/// cursor), not the main range kept as it was — a Mod-d chain of
/// selected words does not keep its highlight, and a second Escape is a
/// defined no-op (S1 collapses to S0, which declines).
#[test]
fn the_collapse_is_bare_and_idempotent() {
    // The decision carries only the head: the applied selection is
    // EditorSelection.single(head) — an empty range by construction.
    let EscapeDecision::CollapseToMainHead { head } =
        exit_multi_cursor_decision(3, 91)
    else {
        panic!("three ranges collapse");
    };
    assert_eq!(head, 91);
    // After the collapse the state is S0: the next Escape declines.
    assert_eq!(exit_multi_cursor_decision(1, head), EscapeDecision::Decline);
}
