//! T497 audit (tester): independent checks of the wizard state, from
//! separately-written mutation sequences: a full wizard walk (every step
//! in order, both layouts), a re-run after reset (the restart workflow),
//! and the options-vs-state independence.

use crate::views::data_sync::data_sync_compare_step::{SyncState, TableDiffFilter};
use crate::views::data_sync::use_data_sync_wizard_state::{
    DataSyncWizardState, NARROW_WIZARD_STEPS, WIZARD_STEPS, WizardStep,
};

#[test]
fn audit_a_full_wizard_walk_visits_every_step_in_order() {
    let mut state = DataSyncWizardState::new();
    for step in WIZARD_STEPS {
        state.set_step(step);
        assert_eq!(state.step, step);
    }
    // The narrow layout's three steps are a subsequence of the full six.
    let positions: Vec<usize> = NARROW_WIZARD_STEPS
        .iter()
        .map(|narrow| WIZARD_STEPS.iter().position(|full| full == narrow).expect("narrow step is a full step"))
        .collect();
    assert_eq!(positions, vec![0, 1, 5], "the narrow walk skips the middle but keeps the order");
    assert_eq!(state.step, WizardStep::Result, "the walk ended on the last step");
}

#[test]
fn audit_a_rerun_after_reset_starts_a_clean_compare() {
    // The restart workflow: a finished sync's compare-half is wiped, then
    // a fresh inspection fills it again — without re-creating the state.
    let mut state = DataSyncWizardState::new();
    state.set_sync_state(SyncState::Done);
    state.set_inspection_complete(true);
    state.set_selected_table_key(Some("t".to_string()));
    state.reset_compare_state();

    // The fresh inspection's first writes land on clean cells.
    assert_eq!(state.sync_state, SyncState::Idle);
    state.set_sync_state(SyncState::Inspecting);
    state.set_inspection_complete(true);
    assert_eq!(state.sync_state, SyncState::Inspecting);
    assert!(state.inspection_complete);
    // A second reset of already-clean cells is a quiet no-op.
    state.reset_compare_state();
    assert!(!state.inspection_complete);
    assert_eq!(state.sync_state, SyncState::Idle);
}

#[test]
fn audit_the_options_cell_is_independent_of_the_compare_cells() {
    let mut state = DataSyncWizardState::new();
    // Mutating the compare half never disturbs the options…
    state.set_sync_state(SyncState::Comparing);
    state.set_table_filter(TableDiffFilter::Unchanged);
    assert!(state.sync_options.insert);
    // …and editing the options never resets the compare half.
    let mut options = state.sync_options.clone();
    options.update = false;
    state.set_sync_options(options);
    assert!(!state.sync_options.update);
    assert_eq!(state.sync_state, SyncState::Comparing);
    // The reset, too, leaves the edited options alone.
    state.reset_compare_state();
    assert!(!state.sync_options.update);
}

#[test]
fn audit_two_wizard_instances_never_share_cells() {
    let mut first = DataSyncWizardState::new();
    let mut second = DataSyncWizardState::new();
    first.set_step(WizardStep::Setup);
    first.set_table_search("first");
    second.set_sync_state(SyncState::Executing);
    assert_eq!(second.step, WizardStep::Endpoints);
    assert_eq!(second.table_search, "");
    assert_eq!(first.sync_state, SyncState::Idle);
    assert_eq!(first.table_search, "first");
}
