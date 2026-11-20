//! T408: `windows/data-sync/dataSyncTypes.ts` — the wizard-state type's
//! React-side alias, proven identical to the hook's own Rust type.

use std::collections::HashSet;

use crate::views::data_sync::use_data_sync_wizard_state::{
    DataSyncWizardState, DataSyncWizardStateOfHook, WizardStep,
};

/// The alias and the concrete type are one type: a value built as one is
/// readable as the other and compares equal — `ReturnType<typeof
/// useDataSyncWizardState>` could not be spelled more literally in Rust.
#[test]
fn the_alias_is_the_hook_state_type() {
    let state: DataSyncWizardStateOfHook = DataSyncWizardState::new();
    let same: DataSyncWizardState = state.clone();
    assert_eq!(state, same);
    assert_eq!(state.step, WizardStep::Endpoints);
    assert!(state.disabled_tables.is_empty());
}
