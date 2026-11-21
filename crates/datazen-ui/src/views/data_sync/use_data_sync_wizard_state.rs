//! The Data Sync wizard's state, ported from React
//! `packages/frontend/src/windows/data-sync/useDataSyncWizardState.ts`
//! (tasklist T497).
//!
//! The React hook is nine `useState` cells plus the `resetCompareState`
//! composite that clears the compare-half of them together; the wizard
//! window owns one hook instance for its whole lifetime. In immediate mode
//! the hook IS the state: [`DataSyncWizardState`] is the struct the window
//! host holds, and every setter becomes a plain-value method — no React
//! call site uses the updater-function arm (verified across the wizard
//! window's usage), so value-taking setters are the full contract.
//!
//! Type mirrors: `SyncOptions` / `DataSyncTableResult` /
//! `TableDiffFilter` / `SyncState` all live in the compare step's port
//! (T024) and the mapping view (T417) — single-sourced, no second copies.
//! `DEFAULT_SYNC_OPTIONS` is the mirror's `Default` impl (insert/update
//! on, delete off, primaryKey matching, batch 1000, full large-value
//! mode, abort-on-conflict — pinned equal by a test).
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useDataSyncWizardState()` | `DataSyncWizardState::default()` |
//! | `WIZARD_STEPS` / `NARROW_WIZARD_STEPS` | the two consts below |
//! | the nine setters | the same-named `set_*` methods |
//! | `resetCompareState()` | [`DataSyncWizardState::reset_compare_state`] — clears the compare-half only: the options, the step and the search survive |

use std::collections::HashSet;

use crate::views::data_sync::data_sync_compare_step::{
    DataSyncTableResult, SyncOptions, SyncState, TableDiffFilter,
};

/// React `WizardStep`: the six wizard pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // consumers: the Data Sync window track (T268, not landed yet)
pub(crate) enum WizardStep {
    Endpoints,
    Setup,
    Objects,
    Compare,
    Preview,
    Result,
}

impl WizardStep {
    /// The wire spelling (`'endpoints'`, …) — the step chip keys and the
    /// `sync.step.<step>` words ride it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Endpoints => "endpoints",
            Self::Setup => "setup",
            Self::Objects => "objects",
            Self::Compare => "compare",
            Self::Preview => "preview",
            Self::Result => "result",
        }
    }
}

/// `WIZARD_STEPS`: the full-page layout's six steps, in order.
pub(crate) const WIZARD_STEPS: [WizardStep; 6] = [
    WizardStep::Endpoints,
    WizardStep::Setup,
    WizardStep::Objects,
    WizardStep::Compare,
    WizardStep::Preview,
    WizardStep::Result,
];

/// `NARROW_WIZARD_STEPS`: the narrow (single-column) layout's three steps.
pub(crate) const NARROW_WIZARD_STEPS: [WizardStep; 3] = [
    WizardStep::Endpoints,
    WizardStep::Setup,
    WizardStep::Result,
];

/// The hook's state: the wizard's whole mutable surface.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // consumers: the Data Sync window track (T268, not landed yet)
pub(crate) struct DataSyncWizardState {
    /// The sync options the setup step edits (`DEFAULT_SYNC_OPTIONS` seed).
    pub(crate) sync_options: SyncOptions,
    /// The inspection/comparison results the wizard shows.
    pub(crate) mapping_results: Vec<DataSyncTableResult>,
    /// The tables the user disabled for this sync.
    pub(crate) disabled_tables: HashSet<String>,
    /// The wizard page the user is on (`'endpoints'` seed).
    pub(crate) step: WizardStep,
    /// Whether the inspection step has completed.
    pub(crate) inspection_complete: bool,
    /// The runner's state (`'idle'` seed).
    pub(crate) sync_state: SyncState,
    /// The mapping table's selected row key, or none.
    pub(crate) selected_table_key: Option<String>,
    /// The mapping table's diff filter (`'all'` seed).
    pub(crate) table_filter: TableDiffFilter,
    /// The mapping table's search box text (empty seed).
    pub(crate) table_search: String,
}

#[allow(dead_code)] // consumers: the Data Sync window track (T268, not landed yet)
impl DataSyncWizardState {
    /// `useDataSyncWizardState()`: the nine seeds.
    pub(crate) fn new() -> Self {
        Self {
            sync_options: SyncOptions::default(),
            mapping_results: Vec::new(),
            disabled_tables: HashSet::new(),
            step: WizardStep::Endpoints,
            inspection_complete: false,
            sync_state: SyncState::default(),
            selected_table_key: None,
            table_filter: TableDiffFilter::default(),
            table_search: String::new(),
        }
    }

    /// `setSyncOptions(options)`.
    pub(crate) fn set_sync_options(&mut self, options: SyncOptions) {
        self.sync_options = options;
    }

    /// `setMappingResults(results)`.
    pub(crate) fn set_mapping_results(&mut self, results: Vec<DataSyncTableResult>) {
        self.mapping_results = results;
    }

    /// `setDisabledTables(disabled)`.
    pub(crate) fn set_disabled_tables(&mut self, disabled: HashSet<String>) {
        self.disabled_tables = disabled;
    }

    /// `setStep(step)`.
    pub(crate) fn set_step(&mut self, step: WizardStep) {
        self.step = step;
    }

    /// `setInspectionComplete(complete)`.
    pub(crate) fn set_inspection_complete(&mut self, complete: bool) {
        self.inspection_complete = complete;
    }

    /// `setSyncState(state)`.
    pub(crate) fn set_sync_state(&mut self, state: SyncState) {
        self.sync_state = state;
    }

    /// `setSelectedTableKey(key)` — `null` clears it.
    pub(crate) fn set_selected_table_key(&mut self, key: Option<String>) {
        self.selected_table_key = key;
    }

    /// `setTableFilter(filter)`.
    pub(crate) fn set_table_filter(&mut self, filter: TableDiffFilter) {
        self.table_filter = filter;
    }

    /// `setTableSearch(search)`.
    pub(crate) fn set_table_search(&mut self, search: impl Into<String>) {
        self.table_search = search.into();
    }

    /// `resetCompareState()`: clear the compare-half — the results, the
    /// disabled set, the inspection flag, the selection and the runner
    /// state. The options, the step and the search box survive; a fresh
    /// inspection starts from `DEFAULT_SYNC_OPTIONS`-shaped options only
    /// because the window re-seeds them explicitly when it starts a sync.
    pub(crate) fn reset_compare_state(&mut self) {
        self.mapping_results = Vec::new();
        self.disabled_tables = HashSet::new();
        self.inspection_complete = false;
        self.selected_table_key = None;
        self.sync_state = SyncState::default();
    }
}

impl Default for DataSyncWizardState {
    fn default() -> Self {
        Self::new()
    }
}

/// React `dataSyncTypes.ts`'s whole content: `DataSyncWizardState =
/// ReturnType<typeof useDataSyncWizardState>` — the wizard state type,
/// named the way the window track's props spell it (T408). Rust already
/// has the concrete type (this module's [`DataSyncWizardState`]); the
/// alias keeps the React-side name reachable without a second mirror.
pub(crate) type DataSyncWizardStateOfHook = DataSyncWizardState;
