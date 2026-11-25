//! T459: the Data Sync window's feedback state — the four cells, their
//! plain-value setters, and the close-keeps-message behavior the error
//! dialog depends on.

use datazen_ui::views::data_sync::use_data_sync_feedback::DataSyncFeedback;

#[test]
fn the_hook_seeds_four_empty_cells() {
    // useDataSyncFeedback() = useState('') x3 + useState(false).
    let feedback = DataSyncFeedback::default();
    assert_eq!(feedback.error_msg, "");
    assert!(!feedback.error_open);
    assert_eq!(feedback.status_msg, "");
    assert_eq!(feedback.execute_progress, "");
}

#[test]
fn the_setters_write_their_own_cell_only() {
    let mut feedback = DataSyncFeedback::default();
    feedback.set_error_msg("missing fields");
    feedback.set_error_open(true);
    feedback.set_status_msg("profile saved");
    feedback.set_execute_progress("12/40");
    assert_eq!(feedback.error_msg, "missing fields");
    assert!(feedback.error_open);
    assert_eq!(feedback.status_msg, "profile saved");
    assert_eq!(feedback.execute_progress, "12/40");

    // One setter never disturbs the other three cells.
    feedback.set_status_msg("profile deleted");
    assert_eq!(feedback.error_msg, "missing fields");
    assert!(feedback.error_open);
    assert_eq!(feedback.execute_progress, "12/40");
}

#[test]
fn closing_the_error_dialog_keeps_the_message() {
    // The window's onCloseError is setErrorOpen(false) only; React clears
    // nothing on close, so a re-open after a close shows the same message.
    let mut feedback = DataSyncFeedback::default();
    feedback.set_error_msg(String::from("connection refused"));
    feedback.set_error_open(true);
    feedback.set_error_open(false);
    assert!(!feedback.error_open);
    assert_eq!(feedback.error_msg, "connection refused");
    feedback.set_error_open(true);
    assert_eq!(feedback.error_msg, "connection refused");
}

#[test]
fn a_setter_overwrites_the_previous_value() {
    // The updater-function arm of SetStateAction is unused by every React
    // call site; the last plain value wins.
    let mut feedback = DataSyncFeedback::default();
    feedback.set_status_msg("first");
    feedback.set_status_msg("second");
    assert_eq!(feedback.status_msg, "second");
    feedback.set_execute_progress("");
    assert_eq!(feedback.execute_progress, "");
}
