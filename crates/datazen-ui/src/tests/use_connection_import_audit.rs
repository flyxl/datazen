//! Independent audit for T346: `useConnectionImport` hook port.
//!
//! Fill branches the coder does not drive: stale-generation guards on the
//! other three answers, a no-file pick that must keep an existing selection,
//! and the full derived widget-VM projection.

use crate::components::connection::connection_import_fields::PasswordPolicy;
use crate::components::connection::connection_share_dialog::{BrowseKind, ImportSource};
use crate::components::connection::use_connection_import::{ImportInput, ImportState, ImportStrings};

fn strings() -> ImportStrings {
    ImportStrings {
        path_required: "path".to_owned(),
        password_required: "password".to_owned(),
        import_failed: "failed".to_owned(),
        choose_file: "choose".to_owned(),
        import_action: "import".to_owned(),
    }
}

fn app_state() -> ImportState {
    ImportState::new(ImportSource::TablePlus, true)
}

/// The coder pins a stale `Detected` answer; the other generation-guarded
/// answers (DetectFailed / BrowsePicked / BrowseFailed) must also be inert.
#[test]
fn stale_detect_and_browse_answers_are_all_ignored() {
    let mut state = app_state();
    state.set_data_path("/kept");
    state.set_path_found(true);

    let stale_detect = state.step(
        ImportInput::DetectFailed { generation: 99 },
        &strings(),
    );
    assert!(stale_detect.effects.is_empty() && stale_detect.events.is_empty());
    assert_eq!(state.data_path(), "/kept", "a stale detect failure changes nothing");
    assert!(state.path_found());

    let stale_pick = state.step(
        ImportInput::BrowsePicked {
            path: "/stale".to_owned(),
            generation: 99,
        },
        &strings(),
    );
    assert!(stale_pick.effects.is_empty() && stale_pick.events.is_empty());
    assert_eq!(state.data_path(), "/kept", "a stale browse pick changes nothing");

    state.set_local_error("kept error");
    let stale_failed = state.step(
        ImportInput::BrowseFailed {
            raw: "boom".to_owned(),
            generation: 99,
        },
        &strings(),
    );
    assert!(stale_failed.effects.is_empty() && stale_failed.events.is_empty());
    assert_eq!(state.local_error(), "kept error", "a stale browse error is not reported");
}

/// React's `if (picked) setSelectedFile(picked)` leaves an existing selection
/// untouched when the picker is dismissed.
#[test]
fn a_dismissed_file_picker_keeps_the_current_selection() {
    let mut state = ImportState::new(ImportSource::File, true);
    let _ = state.step(
        ImportInput::FilePicked(Some("existing.json".to_owned())),
        &strings(),
    );
    let step = state.step(ImportInput::FilePicked(None), &strings());
    assert!(step.effects.is_empty() && step.events.is_empty());
    assert_eq!(state.selected_file(), Some("existing.json"));
    assert!(!state.submitting(), "a dismissed picker ends submitting");
}

/// The widget VM mirrors every derived flag React returns, including busy,
/// password-required on an encrypted file, and the caller-supplied inline bit.
#[test]
fn view_model_mirrors_all_derived_flags() {
    let mut state = ImportState::new(ImportSource::File, true);
    let _ = state.step(
        ImportInput::FilePicked(Some("backup.datazenconnection".to_owned())),
        &strings(),
    );
    state.set_password("pw");
    state.set_local_error("notice");
    let vm = state.to_vm(true);
    assert_eq!(vm.selected_file.as_deref(), Some("backup.datazenconnection"));
    assert_eq!(vm.local_error, "notice");
    assert!(!vm.app_import);
    assert!(!vm.busy);
    assert!(!vm.submitting);
    assert_eq!(vm.password_required, true, "the encrypted file requires a password");
    assert!(vm.inline);
}
