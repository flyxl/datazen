//! T170 audit: the extensions/{executionState,types}.ts port, verified
//! against the React sources re-read line by line and the React test
//! `extensions/__tests__/executionState.test.ts` (every case mirrored —
//! the StateField's transitions, the pure match helpers, the frame
//! rules; `statementExtensions.test.ts`'s registry counting is T139's
//! CodeMirror seam, documented in the module, nothing to port).
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use datazen_ui::components::sql_editor::completion::SqlTextRange;
use datazen_ui::components::sql_editor::contracts::ExecutionStatus;
use datazen_ui::components::sql_editor::extensions::{
    DEGRADE_MAX_LINES_DEFAULT, ExecutionEffect, INITIAL_EXECUTION_STATE,
    execute_shortcut_label, is_document_degraded, is_idle_or_cancelled, is_macos,
    matches_running_target, should_hide_frame, EditorExecutionState, ShouldHideFrameOpts,
};
use datazen_ui::components::sql_editor::sql_editor_context_menu::Platform;

fn start(range: Option<(usize, usize)>, version: u64) -> ExecutionEffect {
    ExecutionEffect::Start {
        target_range: range.map(|(from, to)| SqlTextRange { from, to }),
        document_version: version,
    }
}

fn running(range: (usize, usize), version: u64) -> EditorExecutionState {
    EditorExecutionState {
        status: ExecutionStatus::Running,
        target_range: Some(SqlTextRange { from: range.0, to: range.1 }),
        document_version: Some(version),
    }
}

// ---- the StateField's lifecycle ---------------------------------------------

/// React `[tester] initial state`: idle, no target, no version.
#[test]
fn initial_state_is_idle_with_no_target() {
    assert_eq!(INITIAL_EXECUTION_STATE.status, ExecutionStatus::Idle);
    assert!(INITIAL_EXECUTION_STATE.target_range.is_none());
    assert!(INITIAL_EXECUTION_STATE.document_version.is_none());
    // A fresh field value is the initial state (StateField.create).
    assert_eq!(INITIAL_EXECUTION_STATE.update(&[]), INITIAL_EXECUTION_STATE);
}

/// React `[tester] StartExecutionEffect`: running with the target range.
#[test]
fn start_transitions_to_running_with_the_target() {
    let next = INITIAL_EXECUTION_STATE.update(&[start(Some((0, 8)), 1)]);
    assert_eq!(next.status, ExecutionStatus::Running);
    assert_eq!(next.target_range, Some(SqlTextRange { from: 0, to: 8 }));
    assert_eq!(next.document_version, Some(1));
}

/// React `[tester] StartExecutionEffect`: a second start overwrites the
/// previous running state wholesale.
#[test]
fn start_overwrites_the_previous_running_state() {
    let once = INITIAL_EXECUTION_STATE.update(&[start(Some((0, 8)), 1)]);
    let twice = once.update(&[start(Some((11, 19)), 2)]);
    assert_eq!(twice.status, ExecutionStatus::Running);
    assert_eq!(twice.target_range, Some(SqlTextRange { from: 11, to: 19 }));
    assert_eq!(twice.document_version, Some(2));
}

/// React `[tester] CancelExecutionEffect`: running → cancelling, the
/// target range kept (the spinner stays pinned to the statement while
/// the cancel is in flight).
#[test]
fn cancel_keeps_the_target_range() {
    let running = INITIAL_EXECUTION_STATE.update(&[start(Some((0, 8)), 1)]);
    let cancelling = running.update(&[ExecutionEffect::Cancel]);
    assert_eq!(cancelling.status, ExecutionStatus::Cancelling);
    assert_eq!(cancelling.target_range, Some(SqlTextRange { from: 0, to: 8 }));
    assert_eq!(cancelling.document_version, Some(1));
}

/// React `[tester] FinishExecutionEffect`: running → idle (everything
/// cleared), and cancelling → idle too.
#[test]
fn finish_clears_back_to_idle_from_both_states() {
    let running = INITIAL_EXECUTION_STATE.update(&[start(Some((0, 8)), 1)]);
    let finished = running.update(&[ExecutionEffect::Finish]);
    assert_eq!(finished, INITIAL_EXECUTION_STATE);

    let cancelling = running.update(&[ExecutionEffect::Cancel]);
    let finished_from_cancel = cancelling.update(&[ExecutionEffect::Finish]);
    assert_eq!(finished_from_cancel.status, ExecutionStatus::Idle);
    assert!(finished_from_cancel.target_range.is_none());
    assert!(finished_from_cancel.document_version.is_none());
}

/// React `[tester] state snapshot immutability`: an update never mutates
/// the snapshot it started from (the value is Copy — the React
/// immutability invariant holds by construction, pinned anyway).
#[test]
fn update_never_mutates_the_previous_snapshot() {
    let before = INITIAL_EXECUTION_STATE.update(&[start(Some((0, 8)), 1)]);
    let snapshot = before;
    let _ = before.update(&[start(Some((11, 19)), 2)]);
    assert_eq!(snapshot.target_range, Some(SqlTextRange { from: 0, to: 8 }));
    assert_eq!(snapshot.document_version, Some(1));
}

/// The React update loop returns at the transaction's FIRST effect — a
/// transaction carrying several effects lets the first decide.
#[test]
fn the_first_effect_in_a_transaction_decides() {
    let state = INITIAL_EXECUTION_STATE.update(&[
        start(Some((0, 8)), 1),
        ExecutionEffect::Finish,
        ExecutionEffect::Cancel,
    ]);
    // Start won; the later Finish and Cancel never ran.
    assert_eq!(state.status, ExecutionStatus::Running);
    assert_eq!(state.document_version, Some(1));
}

// ---- matchesRunningTarget / isIdleOrCancelled --------------------------------

/// React: true only when status, version and range all match.
#[test]
fn matches_running_target_requires_all_three() {
    let state = running((0, 8), 1);
    assert!(matches_running_target(&state, 1, SqlTextRange { from: 0, to: 8 }));
    // status not running.
    let idle = EditorExecutionState {
        status: ExecutionStatus::Idle,
        target_range: Some(SqlTextRange { from: 0, to: 8 }),
        document_version: Some(1),
    };
    assert!(!matches_running_target(&idle, 1, SqlTextRange { from: 0, to: 8 }));
    // version mismatch.
    assert!(!matches_running_target(&state, 2, SqlTextRange { from: 0, to: 8 }));
    // from mismatch.
    assert!(!matches_running_target(&state, 1, SqlTextRange { from: 5, to: 8 }));
    // to mismatch.
    assert!(!matches_running_target(&state, 1, SqlTextRange { from: 0, to: 10 }));
    // no target at all.
    let untargeted = EditorExecutionState {
        status: ExecutionStatus::Running,
        target_range: None,
        document_version: Some(1),
    };
    assert!(!matches_running_target(&untargeted, 1, SqlTextRange { from: 0, to: 8 }));
}

/// React: idle and cancelling are "no longer running"; running is not.
#[test]
fn idle_or_cancelled_covers_both_non_running_states() {
    assert!(is_idle_or_cancelled(&INITIAL_EXECUTION_STATE));
    let cancelling = INITIAL_EXECUTION_STATE
        .update(&[start(Some((0, 8)), 1)])
        .update(&[ExecutionEffect::Cancel]);
    assert!(is_idle_or_cancelled(&cancelling));
    assert!(!is_idle_or_cancelled(&running((0, 8), 1)));
}

// ---- types.ts: the frame rules ------------------------------------------------

/// `isDocumentDegraded`: strictly beyond the threshold; the React default
/// is 500.
#[test]
fn degrade_is_strictly_beyond_the_threshold() {
    assert_eq!(DEGRADE_MAX_LINES_DEFAULT, 500);
    assert!(!is_document_degraded(500, DEGRADE_MAX_LINES_DEFAULT));
    assert!(is_document_degraded(501, DEGRADE_MAX_LINES_DEFAULT));
    assert!(!is_document_degraded(10, 100));
    assert!(is_document_degraded(101, 100));
}

/// `shouldHideFrame`: degraded hides; a multi-line non-empty selection
/// hides; single-line selections and empty ones do not.
#[test]
fn hide_frame_rules() {
    fn opts(degraded: bool, empty: bool, from: usize, to: usize, text: &str) -> ShouldHideFrameOpts<'_> {
        ShouldHideFrameOpts {
            degraded,
            selection_empty: empty,
            selection_from: from,
            selection_to: to,
            doc_line_count: 3,
            doc_text: text,
        }
    }
    // Degraded always hides, selection or not.
    assert!(should_hide_frame(&opts(true, true, 0, 0, "SELECT 1")));
    // Multi-line selection hides.
    assert!(should_hide_frame(&opts(false, false, 0, 12, "SELECT 1;\nSELECT 2;")));
    // Single-line selection does not.
    assert!(!should_hide_frame(&opts(false, false, 0, 8, "SELECT 1;\nSELECT 2;")));
    // Empty selection does not (even with from == to set).
    assert!(!should_hide_frame(&opts(false, true, 3, 3, "SELECT 1;\nSELECT 2;")));
    // from == to with selectionEmpty=false is not a selection either.
    assert!(!should_hide_frame(&opts(false, false, 5, 5, "SELECT 1;\nSELECT 2;")));
    // A selection spanning lines in reverse order still hides (min/max).
    assert!(should_hide_frame(&opts(false, false, 12, 0, "SELECT 1;\nSELECT 2;")));
}

/// `isMacOS` / `executeShortcutLabel`: the platform-aware labels, the
/// same seam `sqlEditorContextMenu` established (⌘ on macOS, Ctrl
/// elsewhere).
#[test]
fn platform_labels() {
    assert!(is_macos(Platform::Macos));
    assert!(!is_macos(Platform::Windows));
    assert!(!is_macos(Platform::Linux));
    assert!(!is_macos(Platform::Unknown));
    assert_eq!(execute_shortcut_label(Platform::Macos), "⌘+Enter");
    assert_eq!(execute_shortcut_label(Platform::Windows), "Ctrl+Enter");
    assert_eq!(execute_shortcut_label(Platform::Linux), "Ctrl+Enter");
}
