//! T459 audit (tester): independent checks of the Data Sync feedback state.
//! The coder's tests exercise setters one at a time; this audit drives the
//! four cells through interleaved sequences and observes the fields from
//! a separate construction site, so a field wired to the wrong setter
//! cannot cancel out against the coder's file.

use datazen_ui::views::data_sync::use_data_sync_feedback::DataSyncFeedback;

#[test]
fn audit_defaults_and_interleaved_writes() {
    let mut feedback = DataSyncFeedback::default();
    assert_eq!(
        (feedback.error_msg.as_str(), feedback.error_open, feedback.status_msg.as_str(), feedback.execute_progress.as_str()),
        ("", false, "", ""),
        "the hook seeds no error, no status and no progress"
    );

    // Interleave the four writers the way the sibling hooks do:
    // profiles write error+status, comparison writes progress.
    feedback.set_status_msg("loaded");
    feedback.set_error_msg("reconcile busy");
    feedback.set_execute_progress("3 tables");
    feedback.set_error_open(true);
    feedback.set_status_msg("deleted");
    assert_eq!(feedback.error_msg, "reconcile busy");
    assert!(feedback.error_open);
    assert_eq!(feedback.status_msg, "deleted");
    assert_eq!(feedback.execute_progress, "3 tables");

    // The window's close path.
    feedback.set_error_open(false);
    assert!(!feedback.error_open);
    assert_eq!(feedback.error_msg, "reconcile busy");
}

#[test]
fn audit_two_instances_are_independent() {
    // Each useDataSyncFeedback() call is its own state; two windows never
    // share cells.
    let mut first = DataSyncFeedback::default();
    let mut second = DataSyncFeedback::default();
    first.set_error_msg("first window");
    first.set_error_open(true);
    second.set_status_msg("second window");
    assert_eq!(second.error_msg, "");
    assert!(!second.error_open);
    assert_eq!(first.status_msg, "");
    assert_eq!(first.error_msg, "first window");
    assert_eq!(second.status_msg, "second window");
}

#[test]
fn audit_the_error_round_trip_survives_a_progress_reset() {
    let mut feedback = DataSyncFeedback::default();
    feedback.set_execute_progress("1/2");
    feedback.set_error_msg("job failed");
    feedback.set_error_open(true);
    // A finished execution clears the progress text while the error stays.
    feedback.set_execute_progress("");
    assert_eq!(feedback.execute_progress, "");
    assert_eq!(feedback.error_msg, "job failed");
    assert!(feedback.error_open);
    // PartialEq: the whole struct round-trips a clone.
    let snapshot = feedback.clone();
    feedback.set_error_open(false);
    assert_ne!(feedback, snapshot);
    feedback.set_error_open(true);
    assert_eq!(feedback, snapshot);
}
