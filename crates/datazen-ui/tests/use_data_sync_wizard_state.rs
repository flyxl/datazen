//! T497: the Data Sync wizard state — the nine seeds, the setter methods,
//! the reset composite, and the two step arrays.

use std::collections::HashSet;

use datazen_ui::views::data_sync::data_sync_compare_step::{
    DataSyncTableResult, SyncState, TableDiffFilter,
};
use datazen_ui::views::data_sync::use_data_sync_wizard_state::{
    DataSyncWizardState, NARROW_WIZARD_STEPS, WIZARD_STEPS, WizardStep,
};

#[test]
fn the_seeds_match_the_react_hook() {
    let state = DataSyncWizardState::new();
    // DEFAULT_SYNC_OPTIONS: insert+update on, delete off.
    assert!(state.sync_options.insert);
    assert!(state.sync_options.update);
    assert!(!state.sync_options.delete);
    assert!(state.mapping_results.is_empty());
    assert!(state.disabled_tables.is_empty());
    assert_eq!(state.step, WizardStep::Endpoints);
    assert!(!state.inspection_complete);
    assert_eq!(state.sync_state, SyncState::Idle);
    assert_eq!(state.selected_table_key, None);
    assert_eq!(state.table_filter, TableDiffFilter::All);
    assert_eq!(state.table_search, "");
}

#[test]
fn the_step_arrays_pin_the_react_orders() {
    assert_eq!(
        WIZARD_STEPS,
        [
            WizardStep::Endpoints,
            WizardStep::Setup,
            WizardStep::Objects,
            WizardStep::Compare,
            WizardStep::Preview,
            WizardStep::Result,
        ]
    );
    assert_eq!(
        NARROW_WIZARD_STEPS,
        [WizardStep::Endpoints, WizardStep::Setup, WizardStep::Result]
    );
}

#[test]
fn every_setter_writes_only_its_cell() {
    let mut state = DataSyncWizardState::new();
    state.set_step(WizardStep::Compare);
    state.set_inspection_complete(true);
    state.set_sync_state(SyncState::Executing);
    state.set_selected_table_key(Some("users".to_string()));
    state.set_table_filter(TableDiffFilter::Insert);
    state.set_table_search("use");
    assert_eq!(state.step, WizardStep::Compare);
    assert!(state.inspection_complete);
    assert_eq!(state.sync_state, SyncState::Executing);
    assert_eq!(state.selected_table_key.as_deref(), Some("users"));
    assert_eq!(state.table_filter, TableDiffFilter::Insert);
    assert_eq!(state.table_search, "use");
    // …and the untouched cells stay seeded.
    assert!(state.mapping_results.is_empty());
    assert_eq!(state.sync_options.insert, true);
}

#[test]
fn the_options_setter_replaces_the_whole_struct() {
    let mut state = DataSyncWizardState::new();
    let mut off = datazen_ui::views::data_sync::data_sync_compare_step::SyncOptions::default();
    off.insert = false;
    off.delete = true;
    state.set_sync_options(off.clone());
    assert!(!state.sync_options.insert);
    assert!(state.sync_options.delete);
    assert!(state.sync_options.update, "the untouched field carries over");
}

#[test]
fn the_results_and_disabled_setters_swap_wholesale() {
    let mut state = DataSyncWizardState::new();
    let mut row = DataSyncTableResult::default();
    row.source_table = "users".to_string();
    state.set_mapping_results(vec![row]);
    assert_eq!(state.mapping_results.len(), 1);

    let mut disabled = HashSet::new();
    disabled.insert("users".to_string());
    state.set_disabled_tables(disabled);
    assert!(state.disabled_tables.contains("users"));
}

#[test]
fn reset_clears_the_compare_half_only() {
    let mut state = DataSyncWizardState::new();
    state.set_step(WizardStep::Preview);
    state.set_inspection_complete(true);
    state.set_sync_state(SyncState::Done);
    state.set_selected_table_key(Some("users".to_string()));
    state.set_table_filter(TableDiffFilter::Insert);
    state.set_table_search("query");
    state.set_mapping_results(vec![DataSyncTableResult::default()]);
    let mut disabled = HashSet::new();
    disabled.insert("x".to_string());
    state.set_disabled_tables(disabled);

    state.reset_compare_state();
    // The compare half is cleared.
    assert!(state.mapping_results.is_empty());
    assert!(state.disabled_tables.is_empty());
    assert!(!state.inspection_complete);
    assert_eq!(state.selected_table_key, None);
    assert_eq!(state.sync_state, SyncState::Idle);
    // The durable half survives.
    assert_eq!(state.step, WizardStep::Preview);
    assert_eq!(state.table_filter, TableDiffFilter::Insert);
    assert_eq!(state.table_search, "query");
    assert!(state.sync_options.insert);
}

#[test]
fn the_selected_table_key_clears_with_none() {
    let mut state = DataSyncWizardState::new();
    state.set_selected_table_key(Some("t".to_string()));
    assert_eq!(state.selected_table_key.as_deref(), Some("t"));
    state.set_selected_table_key(None);
    assert_eq!(state.selected_table_key, None);
}
