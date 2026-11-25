//! T268-S2: `windows/data-sync/DataSyncWindow.tsx`'s rendering, ported
//! to Rune (React 126-341).
//!
//! The render half of the composition root: the window frame (the
//! hidden `data-*` mirror, the 40pt chrome with the run-history
//! dialog's trigger, the job-status strip, the stepper, the
//! scrollable step column, the T322 execute-bar seam, the wizard nav,
//! the status bar, and the four dialog layers) around the six
//! already-ported steps, each fed from the window Vm's seams
//! (T268-S1's `data_sync_window_state`).
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-sync-window` |
//! | the hidden attrs mirror (React 129-131) | `data-sync-window-attrs` |
//! | chrome band (internal) | `data-sync-window-chrome` |
//! | chrome title (internal) | `data-sync-window-title` |
//! | job status strip (the panel's own keys) | `DataSyncJobStatusPanel`'s |
//! | stepper (its own keys) | `DataSyncWizardStepper`'s |
//! | scroll body (internal) | `data-sync-step-scroll` |
//! | step column (internal) | `data-sync-step-column` |
//! | the T322 execute-bar seam | `data-sync-execute-bar-seam` |
//! | status bar (its own keys) | `StatusBar`'s |
//!
//! The steps mount under their own keys (`data-sync-endpoint-step`,
//! `data-sync-options-step`, `data-sync-objects-step`, the compare
//! step's, `data-sync-preview-step`, the result step's); the window
//! adds nothing above them.
//!
//! ## What mounts from where
//!
//! | React | here |
//! |---|---|
//! | `<WindowChromeSlot title rightContent>` (134-142) | the chrome band with the ported run-history dialog (T140) as the right slot — the trigger in-band, the dialog layer in the window layer |
//! | `<DataSyncJobStatusPanel …>` (144-153) | the T148 panel over `vm.job_status` |
//! | `<DataSyncWizardStepper stepIndex>` (155) | the T287 stepper over `vm.step_index()` |
//! | the six step bodies (164-293) | the six ported steps, their VMs assembled from the seams |
//! | `<ExecuteBar …>` (297-307) | the T322 seam — React mounts the ported-pending bar; the seam names it and its two callbacks travel with it when T322 lands |
//! | `<DataSyncWizardNav …>` (309-316) | the T287 nav |
//! | `<StatusBar left>` (318-320) | the T262 status bar over `vm.status_text()` |
//! | `<DataSyncDialogs …>` (322-339) | the T074 dialogs over the seams + `vm.dialog_epochs` |
//!
//! ## Documented deviations from React
//!
//! * **`WindowChromeSlot`** is the chrome band the transfer window
//!   draws (T147's ruling): 40pt, the title left, the window-drag
//!   region, no chrome-fallback interim state.
//! * **The run-history dialog's data feeds stay the host's.** React
//!   embeds it with two props; the port's controlled contract (open
//!   flag, epoch, callbacks) is the window builder's, and the
//!   items/loading/error feeds are the hydration host's (T376,
//!   segment 4) — the dialog layer mounts empty until then, exactly
//!   as the transfer window's own hydration seams do.
//! * **The execute bar is a seam, not a placeholder box.** React
//!   297-307 mounts it only under the preview step; the seam row
//!   does the same, naming T322 and carrying nothing else (its
//!   selectedRows/hasDeletes/targetReadOnly/executing/canExecute
//!   inputs are the seam's, drawn from the same VM fields the bar
//!   will read).
//! * **The step column caps, not centers.** React's `mx-auto max-w-*`
//!   centers the column; the transfer window's ruling applies — the
//!   column takes the cap width (`max-w-2xl` 672 / `max-w-6xl`
//!   1152) without the auto margins, the wizard window filling its
//!   frame.
//! * **The locale gate (122-124) never trips**: the zh-CN table is
//!   compiled in, every port's ruling.
//! * **One root, one layer.** The window roots once
//!   (`build`/`build_inner`); the endpoint/compare/dialog builders'
//!   overlay layers fold into the window's layer.

// The production consumer is the app's window wiring (the host mounts
// the window when the Data Sync window opens); until then the
// component tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec};
use rune_kit::Kit;

use crate::components::migration::migration_run_history_dialog::{
    MigrationOperation, MigrationRunHistoryDialog,
};
use crate::components::status_bar::StatusBar;
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::data_transfer::source_filter_editor::StructuredSourceFilter;

use super::data_sync_compare_step::{DataSyncCompareStep, DataSyncCompareStepVm, SyncState};
use super::data_sync_dialogs::{DataSyncDialogs, DataSyncDialogsVm};
use super::data_sync_endpoint_step::{DataSyncEndpointStep, DataSyncEndpointStepVm};
use super::data_sync_job_status_panel::DataSyncJobStatusPanel;
use super::data_sync_options_step::DataSyncOptionsStep;
use super::data_sync_objects_step::DataSyncObjectsStep;
use super::data_sync_preview_step::{DataSyncPreviewStep, DataSyncPreviewStepVm};
use super::data_sync_result_step::DataSyncResultStep;
use super::data_sync_wizard_nav::{DataSyncWizardNav, DataSyncWizardStepper};
use super::data_sync_window_state::{DataSyncWindowEdits, DataSyncWindowVm};
use super::use_data_sync_wizard_state::WizardStep;

/// The Data Sync window. `S` is the host state that owns the Vm, the
/// edits, and every action the window forwards.
pub struct DataSyncWindow<'a, S: 'static> {
    vm: DataSyncWindowVm,
    edits: &'a DataSyncWindowEdits,
    dark: bool,
    /// The endpoint step's profile-select listbox open flag (the
    /// host's cell, T114's contract).
    profile_select_open: bool,
    /// The setup step's conflict-policy select listbox open flag (the
    /// host's cell, T203's contract).
    policy_select_open: bool,
    /// The run-history dialog's open flag (the host's cell, the T140
    /// port's controlled contract).
    history_open: bool,
    history_open_epoch: u64,
    // -- the run-history dialog (React 136-141) --
    on_history_open: Option<Rc<dyn Fn(&mut S)>>,
    on_history_close: Option<Rc<dyn Fn(&mut S)>>,
    on_reconcile_unknown_run: Option<Rc<dyn Fn(&mut S, String)>>,
    // -- the job-status panel (React 151-152) --
    on_verify_recovery: Option<Rc<dyn Fn(&mut S, super::data_sync_job_status_panel::DataSyncJobDetailsVm)>>,
    on_cancel_job: Option<Rc<dyn Fn(&mut S, String)>>,
    // -- the endpoints step (React 164-208) --
    on_confirm_scope: Option<Rc<dyn Fn(&mut S)>>,
    on_profile_name_edit: Option<Rc<dyn Fn(&mut S) -> &mut rune::TextEdit>>,
    on_profile_name_change: Option<Rc<dyn Fn(&mut S, &mut rune::Tx)>>,
    on_select_profile: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_save_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_load_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_delete_profile: Option<Rc<dyn Fn(&mut S)>>,
    on_select_toggle: Option<Rc<dyn Fn(&mut S)>>,
    on_select_close: Option<Rc<dyn Fn(&mut S)>>,
    // -- the setup step (React 211-217) --
    on_options_change: Option<Rc<dyn Fn(&mut S, super::data_sync_compare_step::SyncOptions)>>,
    on_enable_delete: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_policy: Option<Rc<dyn Fn(&mut S)>>,
    on_close_policy: Option<Rc<dyn Fn(&mut S)>>,
    // -- the objects step (React 219-229) --
    on_toggle_disabled: Option<Rc<dyn Fn(&mut S, String)>>,
    on_open_schema_diff: Option<Rc<dyn Fn(&mut S)>>,
    on_open_data_transfer: Option<Rc<dyn Fn(&mut S)>>,
    on_update_source_filter: Option<Rc<dyn Fn(&mut S, String, Option<StructuredSourceFilter>)>>,
    on_toggle_filter_editor: Option<Rc<dyn Fn(&mut S, String)>>,
    // -- the compare step (React 240) --
    on_cancel_compare: Option<Rc<dyn Fn(&mut S)>>,
    // -- the result step (React 288-291) --
    on_copy_report: Option<Rc<dyn Fn(&mut S)>>,
    on_explain_diff: Option<Rc<dyn Fn(&mut S)>>,
    on_re_compare: Option<Rc<dyn Fn(&mut S)>>,
    // -- the preview step (T371's contract) --
    on_preview_filter: Option<Rc<dyn Fn(&mut S, super::client_sql_preview::OpFilter)>>,
    on_preview_refresh: Option<Rc<dyn Fn(&mut S)>>,
    on_preview_copy: Option<Rc<dyn Fn(&mut S, String)>>,
    // -- the nav (React 314-315) --
    on_go_back: Option<Rc<dyn Fn(&mut S)>>,
    on_go_next: Option<Rc<dyn Fn(&mut S)>>,
    // -- the dialogs (React 322-339) --
    on_close_error: Option<Rc<dyn Fn(&mut S)>>,
    on_close_delete_confirm: Option<Rc<dyn Fn(&mut S)>>,
    on_confirm_enable_delete: Option<Rc<dyn Fn(&mut S)>>,
    on_close_explain: Option<Rc<dyn Fn(&mut S)>>,
    on_close_execute_confirm: Option<Rc<dyn Fn(&mut S)>>,
    on_confirm_execute: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> DataSyncWindow<'a, S> {
    /// The window over its Vm and the host's edits.
    pub fn new(vm: DataSyncWindowVm, edits: &'a DataSyncWindowEdits) -> Self {
        Self {
            vm,
            edits,
            dark: true,
            profile_select_open: false,
            policy_select_open: false,
            history_open: false,
            history_open_epoch: 0,
            on_history_open: None,
            on_history_close: None,
            on_reconcile_unknown_run: None,
            on_verify_recovery: None,
            on_cancel_job: None,
            on_confirm_scope: None,
            on_profile_name_edit: None,
            on_profile_name_change: None,
            on_select_profile: None,
            on_save_profile: None,
            on_load_profile: None,
            on_delete_profile: None,
            on_select_toggle: None,
            on_select_close: None,
            on_options_change: None,
            on_enable_delete: None,
            on_toggle_policy: None,
            on_close_policy: None,
            on_toggle_disabled: None,
            on_open_schema_diff: None,
            on_open_data_transfer: None,
            on_update_source_filter: None,
            on_toggle_filter_editor: None,
            on_cancel_compare: None,
            on_copy_report: None,
            on_explain_diff: None,
            on_re_compare: None,
            on_preview_filter: None,
            on_preview_refresh: None,
            on_preview_copy: None,
            on_go_back: None,
            on_go_next: None,
            on_close_error: None,
            on_close_delete_confirm: None,
            on_confirm_enable_delete: None,
            on_close_explain: None,
            on_close_execute_confirm: None,
            on_confirm_execute: None,
        }
    }

    /// The window's theme.
    pub fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    /// The endpoint step's profile-select listbox open flag.
    pub fn profile_select_open(mut self, open: bool) -> Self {
        self.profile_select_open = open;
        self
    }

    /// The setup step's conflict-policy select listbox open flag.
    pub fn policy_select_open(mut self, open: bool) -> Self {
        self.policy_select_open = open;
        self
    }

    /// The conflict-policy select's open press (T203's contract — the
    /// select asserts without a toggle when enabled).
    pub fn on_toggle_policy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_policy = Some(Rc::new(f));
        self
    }

    /// The conflict-policy select's close (presses outside, Escape).
    pub fn on_close_policy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_policy = Some(Rc::new(f));
        self
    }

    /// The run-history dialog's open flag and epoch (the T140 port's
    /// controlled contract).
    pub fn history_open(mut self, open: bool, epoch: u64) -> Self {
        self.history_open = open;
        self.history_open_epoch = epoch;
        self
    }

    // -- the builder callbacks (React prop for prop) ----------------------

    /// React 139: `profiles.reconcileUnknownRun` — the run-history
    /// dialog's reconcile action.
    pub fn on_reconcile_unknown_run(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_reconcile_unknown_run = Some(Rc::new(f));
        self
    }

    /// The run-history dialog's open press (React 136-141's trigger).
    pub fn on_history_open(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_history_open = Some(Rc::new(f));
        self
    }

    /// The run-history dialog's close (its header close / Escape).
    pub fn on_history_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_history_close = Some(Rc::new(f));
        self
    }

    /// React 151: `onVerifyRecovery(details)`.
    pub fn on_verify_recovery(
        mut self,
        f: impl Fn(&mut S, super::data_sync_job_status_panel::DataSyncJobDetailsVm) + 'static,
    ) -> Self {
        self.on_verify_recovery = Some(Rc::new(f));
        self
    }

    /// React 152: `onCancelJob(jobId)`.
    pub fn on_cancel_job(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_cancel_job = Some(Rc::new(f));
        self
    }

    /// React 167: `runState.setScopeReconfirmationRequired(false)`.
    pub fn on_confirm_scope(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_scope = Some(Rc::new(f));
        self
    }

    /// The profile-name field's edit accessor (T114's contract).
    pub fn on_profile_name_edit(
        mut self,
        f: impl Fn(&mut S) -> &mut rune::TextEdit + 'static,
    ) -> Self {
        self.on_profile_name_edit = Some(Rc::new(f));
        self
    }

    /// React 169: `profiles.setProfileName`.
    pub fn on_profile_name_change(
        mut self,
        f: impl Fn(&mut S, &mut rune::Tx) + 'static,
    ) -> Self {
        self.on_profile_name_change = Some(Rc::new(f));
        self
    }

    /// React 172: `profiles.setSelectedProfileId`.
    pub fn on_select_profile(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_select_profile = Some(Rc::new(f));
        self
    }

    /// React 173: `profiles.saveCurrentProfile()`.
    pub fn on_save_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_profile = Some(Rc::new(f));
        self
    }

    /// React 174: `profiles.loadSelectedProfile`.
    pub fn on_load_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_load_profile = Some(Rc::new(f));
        self
    }

    /// React 175: `profiles.deleteSelectedProfile()`.
    pub fn on_delete_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_delete_profile = Some(Rc::new(f));
        self
    }

    /// The profile select's listbox toggle (T114's contract).
    pub fn on_select_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_toggle = Some(Rc::new(f));
        self
    }

    /// Presses outside and Escape close the profile select (T114's
    /// contract).
    pub fn on_select_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_close = Some(Rc::new(f));
        self
    }

    /// React 214: `options.handleOptionsChange`.
    pub fn on_options_change(
        mut self,
        f: impl Fn(&mut S, super::data_sync_compare_step::SyncOptions) + 'static,
    ) -> Self {
        self.on_options_change = Some(Rc::new(f));
        self
    }

    /// React 215: `options.handleEnableDelete`.
    pub fn on_enable_delete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_enable_delete = Some(Rc::new(f));
        self
    }

    /// React 224: `comparison.toggleDisabledTable`.
    pub fn on_toggle_disabled(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_disabled = Some(Rc::new(f));
        self
    }

    /// React 226: `openSchemaDiffWindow`.
    pub fn on_open_schema_diff(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_schema_diff = Some(Rc::new(f));
        self
    }

    /// React 227: `openDataTransferWindow`.
    pub fn on_open_data_transfer(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_data_transfer = Some(Rc::new(f));
        self
    }

    /// React 225: `comparison.updateSourceFilter`.
    pub fn on_update_source_filter(
        mut self,
        f: impl Fn(&mut S, String, Option<StructuredSourceFilter>) + 'static,
    ) -> Self {
        self.on_update_source_filter = Some(Rc::new(f));
        self
    }

    /// The mapping panel's expand toggle (React 225's editor pair).
    pub fn on_toggle_filter_editor(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_toggle_filter_editor = Some(Rc::new(f));
        self
    }

    /// React 240: `comparison.handleCancel()`.
    pub fn on_cancel_compare(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel_compare = Some(Rc::new(f));
        self
    }

    /// React 288: `comparison.handleCopyCompareReport`.
    pub fn on_copy_report(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_report = Some(Rc::new(f));
        self
    }

    /// React 289/238: `explain.handleExplainDiff`.
    pub fn on_explain_diff(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_explain_diff = Some(Rc::new(f));
        self
    }

    /// React 291: `comparison.handleReCompare`.
    pub fn on_re_compare(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_re_compare = Some(Rc::new(f));
        self
    }

    /// The preview's op filter press (T371's contract).
    pub fn on_preview_filter(
        mut self,
        f: impl Fn(&mut S, super::client_sql_preview::OpFilter) + 'static,
    ) -> Self {
        self.on_preview_filter = Some(Rc::new(f));
        self
    }

    /// The preview's refresh press (T371's contract).
    pub fn on_preview_refresh(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_preview_refresh = Some(Rc::new(f));
        self
    }

    /// The preview's copy press (T371's contract).
    pub fn on_preview_copy(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_preview_copy = Some(Rc::new(f));
        self
    }

    /// React 314: `navigation.goBack`.
    pub fn on_go_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_go_back = Some(Rc::new(f));
        self
    }

    /// React 315: `navigation.goNext()`.
    pub fn on_go_next(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_go_next = Some(Rc::new(f));
        self
    }

    /// React 325: `feedback.setErrorOpen(false)`.
    pub fn on_close_error(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_error = Some(Rc::new(f));
        self
    }

    /// React 327: `options.setDeleteConfirmOpen(false)`.
    pub fn on_close_delete_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_delete_confirm = Some(Rc::new(f));
        self
    }

    /// React 328: `options.confirmEnableDelete`.
    pub fn on_confirm_enable_delete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_enable_delete = Some(Rc::new(f));
        self
    }

    /// React 332: `explain.setExplainOpen(false)`.
    pub fn on_close_explain(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_explain = Some(Rc::new(f));
        self
    }

    /// React 334: `execution.setExecuteConfirmOpen(false)`.
    pub fn on_close_execute_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_execute_confirm = Some(Rc::new(f));
        self
    }

    /// React 335-338: the execute-confirm's run — closes the dialog
    /// and starts the execution.
    pub fn on_confirm_execute(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_execute = Some(Rc::new(f));
        self
    }

    // -- the mount --------------------------------------------------------

    /// The window on its own kit root.
    pub fn build(self, dark: bool) -> El {
        let (root, layer) = self.build_inner(dark);
        rune_kit::root(root, layer)
    }

    /// The same window without its kit root: the window element plus
    /// the overlay layer (the run-history dialog, the profile select's
    /// listbox, the compare step's, the four dialogs).
    pub fn build_inner(mut self, dark: bool) -> (El, Vec<El>) {
        self.dark = dark;
        let k = kit(dark);
        let mut layer: Vec<El> = Vec::new();
        let vm = &self.vm;

        // React 126-132: the root column with the hidden attrs.
        let mut root = col()
            .key("data-sync-window")
            .w_full()
            .h_full()
            .min_h(0.0)
            .fill(k.c.surface_base)
            .child(self.attrs_mirror(vm));
        root = root.child(self.chrome(&k, &mut layer));
        root = root.child(self.job_status_strip(dark));
        root = root.child(
            DataSyncWizardStepper::new(vm.step_index()).build(dark),
        );
        root = root.child(self.step_scroll(dark, &mut layer));
        if vm.wizard.step == WizardStep::Preview {
            root = root.child(self.execute_bar_seam(&k));
        }
        root = root.child(self.wizard_nav(dark));
        root = root.child(
            StatusBar::new("status-bar")
                .left(
                    text(vm.status_text())
                        .key("data-sync-status-message")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .truncate(),
                )
                .build(dark),
        );
        root = root.child(self.dialogs_layer(dark, &mut layer));
        (root, layer)
    }

    /// React 129-131: the three root `data-*` attributes on the
    /// hidden mirror (the terminal-results ruling, T113).
    fn attrs_mirror(&self, vm: &DataSyncWindowVm) -> El {
        // `SyncState`'s wire spelling (utils.ts:18) — the compare
        // step's port carries no `as_str`, so the window spells it
        // where only the mirror reads it.
        let sync_state = match vm.wizard.sync_state {
            SyncState::Idle => "idle",
            SyncState::Inspecting => "inspecting",
            SyncState::Comparing => "comparing",
            SyncState::Compared => "compared",
            SyncState::Executing => "executing",
            SyncState::Unknown => "unknown",
            SyncState::Done => "done",
        };
        row()
            .key("data-sync-window-attrs")
            .size(0.0, 0.0)
            .absolute(0.0, 0.0)
            .a11y_hidden()
            .label(format!(
                "sync_state={sync_state}|step={}|write_outcome_uncertain={}",
                vm.wizard.step.as_str(),
                vm.run.write_outcome_uncertain
            ))
    }

    /// React 134-142: the chrome band — the title left, the
    /// run-history dialog's trigger right, its layer above.
    fn chrome(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut dialog = MigrationRunHistoryDialog::<S>::new(MigrationOperation::DataSync)
            .open(self.history_open)
            .open_epoch(self.history_open_epoch);
        if let Some(on_open) = &self.on_history_open {
            let action = Rc::clone(on_open);
            dialog = dialog.on_open(move |s: &mut S| action(s));
        }
        if let Some(on_close) = &self.on_history_close {
            let action = Rc::clone(on_close);
            dialog = dialog.on_close(move |s: &mut S| action(s));
        }
        if let Some(on_reconcile) = &self.on_reconcile_unknown_run {
            let action = Rc::clone(on_reconcile);
            dialog = dialog.on_reconcile(move |s: &mut S, run_id: String| action(s, run_id));
        }
        // The dialog's own data feeds (items/loading/error) stay the
        // hydration host's (T376, segment 4) — see the module docs.
        let mut parts = dialog.build(self.dark).into_iter();
        let trigger = parts.next().unwrap_or_else(rune::el);
        for dialog_el in parts {
            layer.push(dialog_el);
        }

        row()
            .key("data-sync-window-chrome")
            .window_drag()
            .w_full()
            .h(40.0)
            .center_y()
            .px(12.0)
            .gap(8.0)
            .child(
                text(t("common.dataSyncTitle").to_owned())
                    .key("data-sync-window-title")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_base)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
            .child(trigger)
    }

    /// React 144-153: the job-status strip over the panel's Vm.
    fn job_status_strip(&self, dark: bool) -> El {
        let mut panel = DataSyncJobStatusPanel::<S>::new(self.vm.job_status.clone());
        if let Some(on_verify) = &self.on_verify_recovery {
            let action = Rc::clone(on_verify);
            panel = panel.on_verify_recovery(move |s: &mut S, details| action(s, details));
        }
        if let Some(on_cancel) = &self.on_cancel_job {
            let action = Rc::clone(on_cancel);
            panel = panel.on_cancel_job(move |s: &mut S, job_id| action(s, job_id));
        }
        panel.build(dark)
    }

    /// React 157-163: the scrollable body, the capped step column,
    /// the mounted step.
    fn step_scroll(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let vm = &self.vm;
        let narrow = vm.narrow_layout();
        let step = match vm.wizard.step {
            WizardStep::Endpoints => self.endpoints_step(vm, layer),
            WizardStep::Setup => self.setup_step(vm, dark, layer),
            WizardStep::Objects => self.objects_step(vm, dark),
            WizardStep::Compare => self.compare_step(vm, layer),
            WizardStep::Preview => self.preview_step(vm, dark),
            WizardStep::Result => self.result_step(vm, dark),
        };
        col()
            .key("data-sync-step-scroll")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(
                col()
                    .key("data-sync-step-column")
                    .w_full()
                    .max_w(if narrow { 672.0 } else { 1152.0 })
                    .px(24.0)
                    .py(24.0)
                    .min_h(0.0)
                    .child(step),
            )
    }

    /// React 164-208: the endpoints step over its Vm (the
    /// twenty-eight-field endpoints bundle is T322's mount, inside
    /// the step itself).
    fn endpoints_step(&self, vm: &DataSyncWindowVm, layer: &mut Vec<El>) -> El {
        let step = DataSyncEndpointStepVm {
            scope_reconfirmation_required: vm.run.scope_reconfirmation_required,
            profiles: vm.profiles.profiles.clone(),
            selected_profile_id: vm.profiles.selected_profile_id.clone(),
        };
        let mut builder = DataSyncEndpointStep::<S>::new(step)
            .profile_name_edit(&self.edits.profile_name)
            .select_open(self.profile_select_open);
        if let Some(on_confirm_scope) = &self.on_confirm_scope {
            let action = Rc::clone(on_confirm_scope);
            builder = builder.on_confirm_scope(move |s: &mut S| action(s));
        }
        if let Some(on_edit) = &self.on_profile_name_edit {
            let action = Rc::clone(on_edit);
            builder = builder.on_profile_name_edit(move |s: &mut S| action(s));
        }
        if let Some(on_change) = &self.on_profile_name_change {
            let action = Rc::clone(on_change);
            builder = builder.on_profile_name_change(move |s: &mut S, tx| action(s, tx));
        }
        if let Some(on_select) = &self.on_select_profile {
            let action = Rc::clone(on_select);
            builder = builder.on_select_profile(move |s: &mut S, id| action(s, id));
        }
        if let Some(on_save) = &self.on_save_profile {
            let action = Rc::clone(on_save);
            builder = builder.on_save_profile(move |s: &mut S| action(s));
        }
        if let Some(on_load) = &self.on_load_profile {
            let action = Rc::clone(on_load);
            builder = builder.on_load_profile(move |s: &mut S| action(s));
        }
        if let Some(on_delete) = &self.on_delete_profile {
            let action = Rc::clone(on_delete);
            builder = builder.on_delete_profile(move |s: &mut S| action(s));
        }
        if let Some(on_toggle) = &self.on_select_toggle {
            let action = Rc::clone(on_toggle);
            builder = builder.on_select_toggle(move |s: &mut S| action(s));
        }
        if let Some(on_close) = &self.on_select_close {
            let action = Rc::clone(on_close);
            builder = builder.on_select_close(move |s: &mut S| action(s));
        }
        let (step_el, step_layer) = builder.build_inner(self.dark);
        for overlay in step_layer {
            layer.push(overlay);
        }
        step_el
    }

    /// React 211-217: the setup step over the wizard's options.
    fn setup_step(&self, vm: &DataSyncWindowVm, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let mut builder = DataSyncOptionsStep::<S>::new(vm.wizard.sync_options.clone())
            .policy_select_open(self.policy_select_open);
        if let Some(on_change) = &self.on_options_change {
            let action = Rc::clone(on_change);
            builder = builder.on_change(move |s: &mut S, next| action(s, next));
        }
        if let Some(on_enable_delete) = &self.on_enable_delete {
            let action = Rc::clone(on_enable_delete);
            builder = builder.on_enable_delete(move |s: &mut S| action(s));
        }
        if let Some(on_toggle) = &self.on_toggle_policy {
            let action = Rc::clone(on_toggle);
            builder = builder.on_toggle_policy(move |s: &mut S| action(s));
        }
        if let Some(on_close) = &self.on_close_policy {
            let action = Rc::clone(on_close);
            builder = builder.on_close_policy(move |s: &mut S| action(s));
        }
        // The options step's layer (the policy select's listbox) folds
        // into the window's layer.
        builder.build_inner(&k, layer)
    }

    /// React 219-229: the objects step over the wizard's rows.
    fn objects_step(&self, vm: &DataSyncWindowVm, dark: bool) -> El {
        let mut builder = DataSyncObjectsStep::<S>::new(
            vm.wizard.sync_state,
            vm.wizard.mapping_results.clone(),
            vm.wizard.disabled_tables.clone(),
        )
        .dark(dark);
        if let Some(on_toggle_disabled) = &self.on_toggle_disabled {
            let action = Rc::clone(on_toggle_disabled);
            builder = builder.on_toggle_disabled(move |s: &mut S, table| action(s, table));
        }
        if let Some(on_open_schema_diff) = &self.on_open_schema_diff {
            let action = Rc::clone(on_open_schema_diff);
            builder = builder.on_open_schema_diff(move |s: &mut S| action(s));
        }
        if let Some(on_open_data_transfer) = &self.on_open_data_transfer {
            let action = Rc::clone(on_open_data_transfer);
            builder = builder.on_open_data_transfer(move |s: &mut S| action(s));
        }
        if let Some(on_update) = &self.on_update_source_filter {
            let action = Rc::clone(on_update);
            builder = builder.on_update_source_filter(
                move |s: &mut S, table: String, filter| action(s, table, filter),
            );
        }
        if let Some(on_toggle) = &self.on_toggle_filter_editor {
            let action = Rc::clone(on_toggle);
            builder = builder.on_toggle_filter_editor(move |s: &mut S, table| action(s, table));
        }
        builder.build(dark)
    }

    /// React 231-264: the compare step over the wizard + the
    /// comparison/selection/row-selection seams.
    fn compare_step(&self, vm: &DataSyncWindowVm, layer: &mut Vec<El>) -> El {
        let step_vm = DataSyncCompareStepVm {
            sync_state: vm.wizard.sync_state,
            compared: vm.comparison.compared,
            rows: vm.wizard.mapping_results.clone(),
            stats: vm.comparison.compare_stats.clone(),
            filter: vm.wizard.table_filter,
            search: vm.wizard.table_search.clone(),
            selected_table_key: vm.wizard.selected_table_key.clone(),
            selected_table: vm.row_selection.selected_table.clone(),
            options: vm.wizard.sync_options.clone(),
            table_selections: vm.selection.table_selections.clone(),
            page_loading: vm.selection.page_loading,
            page_index: vm.selection.page_index.clone(),
            page_cursors: vm.selection.page_cursors.clone(),
            explain_loading: vm.explain.explain_loading,
        };
        let mut builder = DataSyncCompareStep::<S>::new(step_vm);
        if let Some(on_cancel) = &self.on_cancel_compare {
            let action = Rc::clone(on_cancel);
            builder = builder.on_cancel(move |s: &mut S| action(s));
        }
        let (step_el, step_layer) = builder.build_inner(self.dark);
        for overlay in step_layer {
            layer.push(overlay);
        }
        step_el
    }

    /// React 267-281: the preview step over the wizard + the
    /// endpoints/selection seams + the preview state.
    fn preview_step(&self, vm: &DataSyncWindowVm, dark: bool) -> El {
        let step_vm = DataSyncPreviewStepVm {
            sync_state: vm.wizard.sync_state,
            execute_progress: vm.feedback.execute_progress.clone(),
            source_db_session_id: vm.endpoints.source_db_session_id.clone(),
            target_db_session_id: vm.endpoints.target_db_session_id.clone(),
            tables: vm.wizard.mapping_results.clone(),
            options: vm.wizard.sync_options.clone(),
            selected_rows: vm.selection.selected_rows.clone(),
            table_selections: vm.selection.table_selections.clone(),
            preview: vm.preview.clone(),
        };
        let mut builder = DataSyncPreviewStep::<S>::new(step_vm).dark(dark);
        if let Some(on_filter) = &self.on_preview_filter {
            let action = Rc::clone(on_filter);
            builder = builder.on_filter(move |s: &mut S, filter| action(s, filter));
        }
        if let Some(on_refresh) = &self.on_preview_refresh {
            let action = Rc::clone(on_refresh);
            builder = builder.on_refresh(move |s: &mut S| action(s));
        }
        if let Some(on_copy) = &self.on_preview_copy {
            let action = Rc::clone(on_copy);
            builder = builder.on_copy(move |s: &mut S, copied| action(s, copied));
        }
        builder.build(dark)
    }

    /// React 284-292: the result step over the run/compare seams.
    /// The result step's stats are the mapping view's twin of the
    /// compare step's `CompareSummaryStats` — the same five fields,
    /// kept by their owning modules (the T246 failure-twin ruling);
    /// the bridge is a plain field-by-field conversion.
    fn result_step(&self, vm: &DataSyncWindowVm, dark: bool) -> El {
        let stats = super::mapping_view::CompareSummaryStats {
            inserts: vm.comparison.compare_stats.inserts,
            updates: vm.comparison.compare_stats.updates,
            deletes: vm.comparison.compare_stats.deletes,
            unchanged_tables: vm.comparison.compare_stats.unchanged_tables,
            incompatible: vm.comparison.compare_stats.incompatible,
        };
        let mut builder =
            DataSyncResultStep::<S>::new(vm.run.last_execution_result.clone(), stats)
            .explain_loading(vm.explain.explain_loading);
        if let Some(on_copy_report) = &self.on_copy_report {
            let action = Rc::clone(on_copy_report);
            builder = builder.on_copy_report(move |s: &mut S| action(s));
        }
        if let Some(on_explain_diff) = &self.on_explain_diff {
            let action = Rc::clone(on_explain_diff);
            builder = builder.on_explain_diff(move |s: &mut S| action(s));
        }
        if let Some(on_re_compare) = &self.on_re_compare {
            let action = Rc::clone(on_re_compare);
            builder = builder.on_re_compare(move |s: &mut S| action(s));
        }
        builder.build(dark)
    }

    /// React 297-307: the T322 execute-bar seam — the bar mounts only
    /// under the preview step (the caller's gate), and its props ride
    /// the seam's label until T322 lands.
    fn execute_bar_seam(&self, k: &Kit) -> El {
        let vm = &self.vm;
        row()
            .key("data-sync-execute-bar-seam")
            .w_full()
            .h(48.0)
            .center_y()
            .px(12.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
            .label(format!(
                "T322|rows={}|deletes={}|readonly={}|executing={}|can={}",
                vm.row_selection.total_selected_rows,
                vm.row_selection.has_selected_deletes,
                vm.endpoints.target_read_only,
                vm.executing(),
                vm.execution.can_execute
            ))
            .child(
                text(t("sync.execute").to_owned())
                    .key("data-sync-execute-bar-seam-hint")
                    .font(k.ty.small)
                    .color(k.c.text_faint)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
    }

    /// React 309-316: the wizard nav.
    fn wizard_nav(&self, dark: bool) -> El {
        let vm = &self.vm;
        let mut nav = DataSyncWizardNav::<S>::new(
            vm.step_index(),
            vm.navigation.busy,
            vm.navigation.can_next,
            vm.show_next(),
        );
        if let Some(on_back) = &self.on_go_back {
            let action = Rc::clone(on_back);
            nav = nav.on_back(move |s: &mut S| action(s));
        }
        if let Some(on_next) = &self.on_go_next {
            let action = Rc::clone(on_next);
            nav = nav.on_next(move |s: &mut S| action(s));
        }
        nav.build(dark)
    }

    /// React 322-339: the four dialogs over the seams + the epochs —
    /// the dialogs' fragment root mounts last (its own key), the four
    /// dialog layers ride the window's layer.
    fn dialogs_layer(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let vm = &self.vm;
        let dialogs_vm = DataSyncDialogsVm {
            error_open: vm.feedback.error_open,
            error_msg: vm.feedback.error_msg.clone(),
            error_epoch: vm.dialog_epochs.error,
            delete_confirm_open: vm.options.delete_confirm_open,
            delete_confirm_epoch: vm.dialog_epochs.delete_confirm,
            explain_open: vm.explain.explain_open,
            explain_loading: vm.explain.explain_loading,
            explain_text: vm.explain.explain_text.clone(),
            explain_epoch: vm.dialog_epochs.explain,
            execute_confirm_open: vm.execution.execute_confirm_open,
            execute_confirm_epoch: vm.dialog_epochs.execute_confirm,
        };
        let mut builder = DataSyncDialogs::<S>::new(dialogs_vm);
        if let Some(on_close) = &self.on_close_error {
            let action = Rc::clone(on_close);
            builder = builder.on_close_error(move |s: &mut S| action(s));
        }
        if let Some(on_close) = &self.on_close_delete_confirm {
            let action = Rc::clone(on_close);
            builder = builder.on_close_delete_confirm(move |s: &mut S| action(s));
        }
        if let Some(on_confirm) = &self.on_confirm_enable_delete {
            let action = Rc::clone(on_confirm);
            builder = builder.on_confirm_enable_delete(move |s: &mut S| action(s));
        }
        if let Some(on_close) = &self.on_close_explain {
            let action = Rc::clone(on_close);
            builder = builder.on_close_explain(move |s: &mut S| action(s));
        }
        if let Some(on_close) = &self.on_close_execute_confirm {
            let action = Rc::clone(on_close);
            builder = builder.on_close_execute_confirm(move |s: &mut S| action(s));
        }
        if let Some(on_confirm) = &self.on_confirm_execute {
            let action = Rc::clone(on_confirm);
            builder = builder.on_confirm_execute(move |s: &mut S| action(s));
        }
        let (fragment, dialogs_layer) = builder.build_inner(dark);
        for overlay in dialogs_layer {
            layer.push(overlay);
        }
        fragment
    }
}
