//! T268-S1: `windows/data-sync/DataSyncWindow.tsx`'s state model, ported
//! to Rune (the window's composition state, React 41-124).
//!
//! The React window is a pure composition root: eleven `useDataSync*`
//! hooks build the state, and the render mounts the already-ported
//! steps. In Rune the host holds one [`DataSyncWindowVm`] — the wizard
//! state (T497's port) and the feedback store (T459's port) owned
//! whole, and every pending hook's output as a seam the window reads.
//! The rendering half is `data_sync_window` (T268-S2).
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `useDataSyncWizardState()` | `vm.wizard` (the T497 port, owned whole) |
//! | `useDataSyncFeedback()` | `vm.feedback` (the T459 port, owned whole) |
//! | `useDataSyncRefs()` | the host's `syncStateRef` write — a render-time ref assignment (React 113), documented below |
//! | `useDataSyncRunState()` | [`DataSyncRunStateSeam`] (T484) |
//! | `useDataSyncEndpoints()` | [`DataSyncEndpointsSeam`] (T441) |
//! | `useDataSyncSelectionState()` | [`DataSyncSelectionSeam`] (T491) |
//! | `useDataSyncJobLifecycle()` | `vm.job_status` (the T148 panel's controlled VM; T464 feeds it) |
//! | `useDataSyncComparison()` | [`DataSyncComparisonSeam`] (T434) |
//! | `useDataSyncOptions()` | [`DataSyncOptionsSeam`] (T469) |
//! | `useDataSyncSelection()` | [`DataSyncRowSelectionSeam`] (T488) |
//! | `useDataSyncExecution()` | [`DataSyncExecutionSeam`] (T447) |
//! | `useDataSyncProfiles()` | [`DataSyncProfilesSeam`] (T474) |
//! | `useDataSyncExplain()` | [`DataSyncExplainSeam`] (T453) |
//! | `useDataSyncWizardNavigation()` | [`DataSyncNavigationSeam`] (T494) + [`DataSyncWindowVm::step_index`] |
//! | `useMigrationJobHydration('dataSync')` | the two `job_status` fields it feeds (T376, segment 4) |
//! | `useSettings()` / `useAiStore().loadConfig` effect (115-118) | the host's mount seam: the settings and AI-config loads are the app store's own loads — no render half exists for them here |
//! | the `sync` locale gate (122-124) | nothing blocks: the zh-CN table is compiled in (`i18n.rs`), the gate never trips — every port's ruling |
//!
//! ## The window's derivations (React 126-341)
//!
//! The render reads four derived values off the state; they live here
//! so the rendering half stays a pure mount:
//!
//! * [`DataSyncWindowVm::step_index`] — `navigation.stepIndex`, the
//!   step's index in `WIZARD_STEPS` (the stepper and the nav read it).
//! * [`DataSyncWindowVm::narrow_layout`] — React 161:
//!   `NARROW_WIZARD_STEPS.includes(wizard.step)`, the `max-w-2xl` /
//!   `max-w-6xl` body switch.
//! * [`DataSyncWindowVm::show_next`] — React 313:
//!   `wizard.step !== 'preview' && wizard.step !== 'result'`.
//! * [`DataSyncWindowVm::executing`] — React 302:
//!   `wizard.syncState === 'executing'`.
//! * [`DataSyncWindowVm::status_text`] — React 319: the status bar's
//!   `feedback.statusMsg || t('common.dataSync')` (JS `||`: the empty
//!   string falls back).
//! * the three root `data-*` attributes (129-131) — the rendering
//!   half's hidden mirror reads them off the VM.
//!
//! ## Documented deviations from React
//!
//! * **The ref write (React 113) is a host seam.**
//!   `refs.syncStateRef.current = wizard.syncState` publishes the sync
//!   state to the cancel path's stable ref; Rune's host holds the
//!   wizard state itself, so the publish is the host writing its own
//!   cell whenever `setSyncState` lands — no render half exists.
//! * **`DataSyncWindowEdits`** carries the window's one `TextEdit`
//!   (the profile-name field, React 168) with the transfer window's
//!   sync contract (T147's ruling: one edits struct, re-seeded
//!   wherever the model changes).
//! * **The seam structs are display shapes, not hook ports.** Each
//!   mirrors exactly the fields the window's render reads off its
//!   hook — the owning task replaces the seam with the real port and
//!   re-points the window's reads; nothing else moves.

// The production consumer is the Data Sync window rendering (T268-S2);
// until it lands, the component tests are the only callers.
#![allow(dead_code)]

use std::collections::HashMap;

use rune::TextEdit;

use super::data_sync_compare_step::{
    CompareSummaryStats, DataSyncPageCursor, DataSyncTableResult, DataSyncTableSelection,
    SyncState,
};
use super::data_sync_job_status_panel::DataSyncJobStatusPanelVm;
use super::data_sync_preview_step::DataSyncSelectedRow;
use super::data_sync_result_step::DataSyncExecutionResult;
use super::sql_preview::SqlPreviewVm;
use super::use_data_sync_feedback::DataSyncFeedback;
use super::use_data_sync_wizard_state::{
    DataSyncWizardState, NARROW_WIZARD_STEPS, WIZARD_STEPS, WizardStep,
};
use crate::i18n::t;

/// `useDataSyncRunState()`'s output as the window reads it (T484).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncRunStateSeam {
    /// React `runState.writeOutcomeUncertain` — the root attribute and
    /// the execute-confirm dialog's uncertainty warning.
    pub write_outcome_uncertain: bool,
    /// React `runState.scopeReconfirmationRequired` — the endpoints
    /// step's reconfirmation notice (167).
    pub scope_reconfirmation_required: bool,
    /// React `runState.lastExecutionResult` — the result step's input.
    pub last_execution_result: Option<DataSyncExecutionResult>,
}

/// `useDataSyncEndpoints()`'s output as the window reads it (T441).
/// The twenty-eight-field `endpoints={{…}}` prop bundle (React
/// 176-207) belongs to the EndpointsBar mount slot the endpoints step
/// already renders (T322); only the fields the OTHER mounts read are
/// modelled here.
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncEndpointsSeam {
    /// React `endpoints.sourceSession?.dbSessionId` — the preview
    /// step's source session (271).
    pub source_db_session_id: Option<String>,
    /// React `endpoints.targetSession?.dbSessionId` (272).
    pub target_db_session_id: Option<String>,
    /// React `endpoints.sourceDatabase` — the preview step's source
    /// name (273).
    pub source_database: String,
    /// React `endpoints.targetDatabase` (274).
    pub target_database: String,
    /// React `endpoints.sourceSchema` (275).
    pub source_schema: String,
    /// React `endpoints.targetSchema` (276).
    pub target_schema: String,
    /// React `endpoints.targetReadOnly` — the execute bar's gate (301).
    pub target_read_only: bool,
}

/// `useDataSyncSelectionState()`'s output as the window reads it
/// (T491).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncSelectionSeam {
    /// React `selection.tableSelections` — the compare and preview
    /// steps' server-owned selections (249, 280).
    pub table_selections: Vec<DataSyncTableSelection>,
    /// React `selection.selectedRows` — the preview step's reviewed
    /// rows (279).
    pub selected_rows: Vec<DataSyncSelectedRow>,
    /// React `selection.pageLoading` (261).
    pub page_loading: bool,
    /// React `selection.pageIndex` (262).
    pub page_index: HashMap<String, usize>,
    /// React `selection.pageCursors` (263).
    pub page_cursors: HashMap<String, DataSyncPageCursor>,
}

/// `useDataSyncSelection()`'s output as the window reads it (T488).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncRowSelectionSeam {
    /// React `rowSelection.selectedTable` — the compare step's diff
    /// detail input (247).
    pub selected_table: Option<DataSyncTableResult>,
    /// React `rowSelection.totalSelectedRows` — the execute bar's
    /// count (299).
    pub total_selected_rows: usize,
    /// React `rowSelection.hasSelectedDeletes` — the execute bar's
    /// delete warning (300).
    pub has_selected_deletes: bool,
}

/// `useDataSyncComparison()`'s output as the window reads it (T434).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncComparisonSeam {
    /// React `comparison.compared` — the compare step's gate (234).
    pub compared: bool,
    /// React `comparison.compareStats` — the compare and result steps'
    /// stats (236, 287).
    pub compare_stats: CompareSummaryStats,
}

/// `useDataSyncOptions()`'s output as the window reads it (T469).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncOptionsSeam {
    /// React `options.deleteConfirmOpen` — the delete-confirm dialog's
    /// open flag (326).
    pub delete_confirm_open: bool,
}

/// `useDataSyncExecution()`'s output as the window reads it (T447).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncExecutionSeam {
    /// React `execution.canExecute` — the execute bar's gate (303).
    pub can_execute: bool,
    /// React `execution.executeConfirmOpen` — the execute-confirm
    /// dialog's open flag (333).
    pub execute_confirm_open: bool,
}

/// `useDataSyncProfiles()`'s output as the window reads it (T474). The
/// profile name is the window's own `TextEdit` (see
/// [`DataSyncWindowEdits`]); the seam carries the rest.
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncProfilesSeam {
    /// React `profiles.syncProfiles` — the endpoints step's profile
    /// select list (170).
    pub profiles: Vec<super::data_sync_endpoint_step::SyncProfile>,
    /// React `profiles.selectedProfileId` (171).
    pub selected_profile_id: String,
}

/// `useDataSyncExplain()`'s output as the window reads it (T453).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncExplainSeam {
    /// React `explain.explainOpen` (329).
    pub explain_open: bool,
    /// React `explain.explainLoading` — the compare and result steps'
    /// button gate (239, 290).
    pub explain_loading: bool,
    /// React `explain.explainText` (331).
    pub explain_text: String,
}

/// `useDataSyncWizardNavigation()`'s output as the window reads it
/// (T494). `stepIndex` is not a seam field: it derives from the
/// wizard step (see [`DataSyncWindowVm::step_index`]).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncNavigationSeam {
    /// React `navigation.busy` — the endpoint step's busy gate and the
    /// nav's spinner state (194, 311).
    pub busy: bool,
    /// React `navigation.canNext` — the nav's next gate (312).
    pub can_next: bool,
    /// React `navigation.compareDisabled` — the endpoints step's
    /// compare gate (195).
    pub compare_disabled: bool,
}

/// The four dialog epochs the window bumps every time it opens a
/// dialog (the T074 port's controlled contract: a reopened dialog
/// restarts at the top).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncDialogEpochs {
    /// The error dialog's epoch.
    pub error: u64,
    /// The delete-confirm dialog's epoch.
    pub delete_confirm: u64,
    /// The explain dialog's epoch.
    pub explain: u64,
    /// The execute-confirm dialog's epoch.
    pub execute_confirm: u64,
}

/// The window's whole state: the two owned ports plus the pending
/// hooks' seams. Every field the render half (T268-S2) reads lives
/// here; the host feeds the seams when the owning tasks land.
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncWindowVm {
    /// The wizard state (T497's port) — the nine cells.
    pub wizard: DataSyncWizardState,
    /// The feedback store (T459's port) — the status line, the error
    /// dialog's pair, the progress text.
    pub feedback: DataSyncFeedback,
    /// The job status panel's controlled VM (T148) — fed by the job
    /// lifecycle (T464) and the migration-job hydration (T376).
    pub job_status: DataSyncJobStatusPanelVm,
    /// `useDataSyncRunState` (T484).
    pub run: DataSyncRunStateSeam,
    /// `useDataSyncEndpoints` (T441).
    pub endpoints: DataSyncEndpointsSeam,
    /// `useDataSyncSelectionState` (T491).
    pub selection: DataSyncSelectionSeam,
    /// `useDataSyncSelection` (T488).
    pub row_selection: DataSyncRowSelectionSeam,
    /// `useDataSyncComparison` (T434).
    pub comparison: DataSyncComparisonSeam,
    /// `useDataSyncOptions` (T469).
    pub options: DataSyncOptionsSeam,
    /// `useDataSyncExecution` (T447).
    pub execution: DataSyncExecutionSeam,
    /// `useDataSyncProfiles` (T474).
    pub profiles: DataSyncProfilesSeam,
    /// `useDataSyncExplain` (T453).
    pub explain: DataSyncExplainSeam,
    /// `useDataSyncWizardNavigation` (T494).
    pub navigation: DataSyncNavigationSeam,
    /// The preview's controlled state (T371's contract: the host owns
    /// the statements, the filter, the loading flag and the copy
    /// feedback; the preview step forwards its three callbacks).
    pub preview: SqlPreviewVm,
    /// The four dialog epochs (the T074 port's contract).
    pub dialog_epochs: DataSyncDialogEpochs,
}

impl DataSyncWindowVm {
    /// `navigation.stepIndex`: the step's index in `WIZARD_STEPS` —
    /// the stepper (155) and the nav (310) read it.
    pub(crate) fn step_index(&self) -> usize {
        WIZARD_STEPS
            .iter()
            .position(|step| *step == self.wizard.step)
            .unwrap_or(0)
    }

    /// React 161: `NARROW_WIZARD_STEPS.includes(wizard.step)` — the
    /// body column's `max-w-2xl` (narrow) / `max-w-6xl` (wide) switch.
    pub(crate) fn narrow_layout(&self) -> bool {
        NARROW_WIZARD_STEPS.contains(&self.wizard.step)
    }

    /// React 313: `wizard.step !== 'preview' && wizard.step !== 'result'`
    /// — the nav's next button shows only before the preview.
    pub(crate) fn show_next(&self) -> bool {
        !matches!(self.wizard.step, WizardStep::Preview | WizardStep::Result)
    }

    /// React 302: `wizard.syncState === 'executing'` — the execute bar's
    /// executing gate.
    pub(crate) fn executing(&self) -> bool {
        self.wizard.sync_state == SyncState::Executing
    }

    /// React 319: `feedback.statusMsg || t('common.dataSync')` — the
    /// status bar's left line (JS `||`: the empty string falls back).
    pub(crate) fn status_text(&self) -> String {
        if self.feedback.status_msg.is_empty() {
            t("common.dataSync").to_owned()
        } else {
            self.feedback.status_msg.clone()
        }
    }
}

/// The window's own edit state: the profile-name field's `TextEdit`
/// (React 168, the endpoints step's controlled input), with the
/// transfer window's sync contract — the host re-seeds the buffer
/// wherever the profile model changes.
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncWindowEdits {
    /// The profile-name field's edit buffer.
    pub profile_name: TextEdit,
}
