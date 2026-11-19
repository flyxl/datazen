//! T147-S3: the Data Transfer window's rendering.
//!
//! Ported from `packages/frontend/src/windows/data-transfer/DataTransferWindow.tsx`
//! (React 1078-1899 — the render half; the state model is S1
//! [`super::data_transfer_window_state`], the run machine S2
//! [`super::transfer_job_run`]).
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim: the root
//! `data-transfer-window`, the profile bar (`data-transfer-profile-name`
//! / `-save` / `-select` / `-load`), the destination buttons
//! (`data-transfer-destination-database` / `-sql-file`), the SQL-file
//! card (`data-transfer-source`, `-source-database`,
//! `-sql-file-dialect` / `-encoding` / `-compression`,
//! `-sql-file-target-database` / `-target-schema`), the setup radios
//! (`data-transfer-mode-data` / `-structure` / `-both`), the options
//! (`data-transfer-use-target-default-collation`,
//! `-destructive-confirm`, `-batch-size` + `-batch-size-error`,
//! `-resume-capability-hint`), the objects list (`-objects-empty`,
//! `-reinspect`, rows `data-transfer-table-row-{source}`), the preview
//! surface (`data-transfer-preview`, `-preview-error`,
//! `-preview-retry`, `-preview-back-mapping`, DDL cards
//! `data-transfer-ddl-preview-{table}[-{kind}--{i}]`, copy buttons
//! `data-transfer-copy-ddl-{table}[-{kind}]`, write plans
//! `data-transfer-write-plan-{source}`, warnings
//! `data-transfer-preview-warning-{i}`, the executing overlay
//! `-executing-overlay`), the footer (`data-transfer-back`, `-cancel`,
//! `-cancel-pending-id`, `-executing-progress`, `-execute`,
//! `-rereview`, `-rereview-blocked`, `-next`), and the error dialog
//! (`data-transfer-error`). Repeated rows suffix the React testid with
//! the row identity, per the Backup window's convention.
//!
//! ## Documented deviations from React
//!
//! * **The chrome is always drawn** (the Backup window's own deviation):
//! React's `WindowChromeSlot` renders the registered desktop title bar;
//! Rune draws its own 40pt title band with the slot's title
//! (`common.dataTransfer`).
//! * **`useMigrationJobHydration` is host-fed**: the strip data
//! (`jobs`, `activeJobs`, `verificationJobs`, `hydrationError`) arrives
//! through [`MigrationJobsHydration`] and this window renders the ported
//! [`DataTransferAttachedJobs`] / [`DataTransferTerminalResults`] strips
//! with it (React 1085-1105). The hook itself is segment-2's T564.
//! * **The mapping step body is a T267 seam**: React renders
//! `TransferMappingStep` there; that task is pending in this segment, so
//! the window renders the gate notice placeholder (T176's, inlined as a
//! labelled placeholder row) and a `data-transfer-mapping-seam` element
//! naming T267 — the mapping editor itself (T023-S2's
//! `ColumnMappingEditor`) is reached through the seam's task, not
//! re-implemented here.
//! * **The result panel is a T246 seam**: `TransferJobResultPanel` is
//! pending in this segment; the window renders a
//! `data-transfer-result-seam` element naming it while the run machine's
//! own fail-closed notices stay on the preview step, exactly as React
//! keeps them (React 1744-1755).
//! * **`LimitationsDialog` is a seam** (`components/ui/LimitationsDialog.tsx`
//! is not yet ported by any segment): the window renders a
//! `data-transfer-limitations` placeholder row while it is open. The
//! dismissal flag (`transferLimitationsPrefs.ts`, T636) stays the
//! host's.
//! * **`TransferExecuteConfirmDialog` is a T225 seam**: the destructive
//! confirm routes through a placeholder row while that task is pending
//! in this segment.
//! * **`StatusBar` is a T262 seam**: the window renders a
//! `data-transfer-status-bar-seam` row at the status bar's place.
//! * **`SqlCodeBlock` is a T219 seam**: the DDL editor body renders the
//! override hint and a placeholder box naming T219; the copy button and
//! the plain `<pre>` preview ARE ported (React 1664-1670) — only the
//! editable variant is the seam.
//! * **`MigrationRunHistoryDialog` is not ported**: React mounts it in
//! the chrome's right slot; the chrome here draws the title only. The
//! dialog is `components/migration/MigrationRunHistoryDialog.tsx` —
//! ported as a component (its module exists) but its host wiring is
//! the window host's, out of this slice's scope.
//! * **The `experimental pair` note** (React 1241-1247) renders as a
//! text row under the endpoints bar (the bar's `footer_note` slot).
//! * **Chips do not wrap / rows do not wrap**, per the Backup window's
//! documented kit limit.

use std::rc::Rc;

use rune::{col, el, icon, row, text, El, ScrollSpec, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::migration::migration_endpoints_bar::{
    EndpointsBarVm, EndpointsLayout, EndpointOption, EndpointVm, MigrationEndpointsBar,
    MigrationI18nPrefix, SelectSlot,
};
use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT};
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};
use crate::ui::toggle::{Checkbox, RadioGroup, RadioItem};
use crate::views::data_transfer::data_transfer_attached_jobs::{AttachedJob, DataTransferAttachedJobs};
use crate::views::data_transfer::data_transfer_terminal_results::{
    DataTransferTerminalResults, TerminalJobView,
};
use crate::views::data_transfer::data_transfer_window_state::{
    ConnectionOption, DataTransferWindowVm, DdlKind, NARROW_STEPS, STEPS, TransferDestinationMode,
    TransferMode, WizardStep, WriteMode,
};

// ---------------------------------------------------------------------------
// The text edits (the Backup window's `BackupWindowEdits` precedent).
// ---------------------------------------------------------------------------

/// The `TextEdit`s behind the window's four text fields: the profile
/// name, the batch size, and the SQL-file target database/schema. Rune
/// edits text through `TextEdit`; the Vm keeps the `String` model, so
/// the host owns these next to the Vm and keeps the two in sync (see
/// the Backup window's module docs — the same contract).
#[derive(Clone, Debug, Default)]
pub(crate) struct DataTransferWindowEdits {
    pub profile_name: TextEdit,
    pub batch_size: TextEdit,
    pub sql_file_database: TextEdit,
    pub sql_file_schema: TextEdit,
}

impl DataTransferWindowEdits {
    /// Fresh edits from the Vm's current text.
    pub(crate) fn new(vm: &DataTransferWindowVm) -> Self {
        Self {
            profile_name: TextEdit::new(vm.profile_name.clone()),
            batch_size: TextEdit::new(vm.batch_size_edit.clone()),
            sql_file_database: TextEdit::new(vm.sql_file_database.clone()),
            sql_file_schema: TextEdit::new(vm.sql_file_schema.clone()),
        }
    }

    /// Copy the edits' current text into the Vm. The host calls this
    /// before reading a Vm text field.
    pub(crate) fn sync_into_vm(&self, vm: &mut DataTransferWindowVm) {
        vm.profile_name = self.profile_name.text.clone();
        vm.batch_size_edit = self.batch_size.text.clone();
        vm.sql_file_database = self.sql_file_database.text.clone();
        vm.sql_file_schema = self.sql_file_schema.text.clone();
    }

    /// Copy the Vm's current text into the edits. The host calls this
    /// after a host-driven reset of a Vm text field.
    pub(crate) fn reset_from_vm(&mut self, vm: &DataTransferWindowVm) {
        self.profile_name.replace_text(vm.profile_name.clone());
        self.batch_size.replace_text(vm.batch_size_edit.clone());
        self.sql_file_database.replace_text(vm.sql_file_database.clone());
        self.sql_file_schema.replace_text(vm.sql_file_schema.clone());
    }
}

// ---------------------------------------------------------------------------
// The host-fed hydration (React 1085-1105).
// ---------------------------------------------------------------------------

/// The `useMigrationJobHydration('dataTransfer')` slice the window
/// renders: the job-center strips' input, fed by the host (the hook is
/// segment-2's T564).
#[derive(Clone, Debug, Default)]
pub(crate) struct MigrationJobsHydration {
    /// React `migrationJobs.hydration?.activeJobs ?? []`.
    pub active_jobs: Vec<AttachedJob>,
    /// React `migrationJobs.hydration?.jobs ?? []`.
    pub jobs: Vec<TerminalJobView>,
    /// React `migrationJobs.hydration?.verificationJobs.length` — the
    /// amber pending-verification hint shows when non-zero.
    pub verification_pending: bool,
    /// React `migrationJobs.hydrationError`.
    pub hydration_error: Option<String>,
}

// ---------------------------------------------------------------------------
// The window.
// ---------------------------------------------------------------------------

/// The Data Transfer window. `S` is the host state that owns the Vm,
/// the edits, and the async middle; every press calls back into it.
pub(crate) struct DataTransferWindow<'a, S: 'static> {
    vm: DataTransferWindowVm,
    edits: &'a DataTransferWindowEdits,
    dark: bool,
    /// React `jobRun.applyView?.jobId` — the strips' exclusion.
    exclude_job_id: Option<String>,
    /// React `hide={jobRun.phase !== 'idle'}` on the terminal strip.
    terminal_hide: bool,
    hydration: MigrationJobsHydration,
    /// The attached-jobs strip's host state adapters (its cancel buttons
    /// write through them). The app-assembly task wires them; without
    /// them the strip does not mount.
    attached_jobs_adapters: Option<(
        fn(&S) -> &crate::views::AttachedJobsState,
        fn(&mut S, &str) -> bool,
    )>,
    on_profile_save: Option<Rc<dyn Fn(&mut S)>>,
    on_profile_load: Option<Rc<dyn Fn(&mut S)>>,
    on_destination_database: Option<Rc<dyn Fn(&mut S)>>,
    on_choose_sql_file: Option<Rc<dyn Fn(&mut S)>>,
    on_next: Option<Rc<dyn Fn(&mut S)>>,
    on_back: Option<Rc<dyn Fn(&mut S)>>,
    on_execute: Option<Rc<dyn Fn(&mut S)>>,
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
    on_re_review: Option<Rc<dyn Fn(&mut S)>>,
    on_retry_preview: Option<Rc<dyn Fn(&mut S)>>,
    on_reinspect: Option<Rc<dyn Fn(&mut S)>>,
    on_copy_ddl: Option<Rc<dyn Fn(&mut S, String)>>,
    on_dismiss_limitations: Option<Rc<dyn Fn(&mut S)>>,
    on_dismiss_confirm: Option<Rc<dyn Fn(&mut S)>>,
    on_dismiss_error: Option<Rc<dyn Fn(&mut S)>>,
    on_profile_name_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    on_batch_size_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    on_sql_file_database_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    on_sql_file_schema_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
}

fn no_op_unit<S: 'static>() -> Rc<dyn Fn(&mut S)> {
    Rc::new(|_: &mut S| {})
}

impl<'a, S: 'static> DataTransferWindow<'a, S> {
    pub(crate) fn new(vm: DataTransferWindowVm, edits: &'a DataTransferWindowEdits) -> Self {
        Self {
            vm,
            edits,
            dark: true,
            exclude_job_id: None,
            terminal_hide: false,
            hydration: MigrationJobsHydration::default(),
            attached_jobs_adapters: None,
            on_profile_save: None,
            on_profile_load: None,
            on_destination_database: None,
            on_choose_sql_file: None,
            on_next: None,
            on_back: None,
            on_execute: None,
            on_cancel: None,
            on_re_review: None,
            on_retry_preview: None,
            on_reinspect: None,
            on_copy_ddl: None,
            on_dismiss_limitations: None,
            on_dismiss_confirm: None,
            on_dismiss_error: None,
            on_profile_name_edit: None,
            on_batch_size_edit: None,
            on_sql_file_database_edit: None,
            on_sql_file_schema_edit: None,
        }
    }

    /// React `jobRun.applyView?.jobId` (React 1097, 1101) and the
    /// terminal strip's `hide={jobRun.phase !== 'idle'}` (React 1104).
    pub(crate) fn run_view(mut self, exclude_job_id: Option<String>, terminal_hide: bool) -> Self {
        self.exclude_job_id = exclude_job_id;
        self.terminal_hide = terminal_hide;
        self
    }

    /// The job-center strips' input (React 1085-1105).
    pub(crate) fn hydration(mut self, hydration: MigrationJobsHydration) -> Self {
        self.hydration = hydration;
        self
    }

    /// React `saveCurrentProfile` (the profile save button).
    pub(crate) fn on_profile_save(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_profile_save = Some(Rc::new(f));
        self
    }

    /// React `loadSelectedProfile` (the profile load button).
    pub(crate) fn on_profile_load(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_profile_load = Some(Rc::new(f));
        self
    }

    /// React `setDestinationMode('database')` (React 1196).
    pub(crate) fn on_destination_database(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_destination_database = Some(Rc::new(f));
        self
    }

    /// React `chooseSqlFile` (the save dialog round trip).
    pub(crate) fn on_choose_sql_file(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_choose_sql_file = Some(Rc::new(f));
        self
    }

    /// React `goNext` (the footer's Next).
    pub(crate) fn on_next(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_next = Some(Rc::new(f));
        self
    }

    /// React `goBack` (the footer's Back).
    pub(crate) fn on_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back = Some(Rc::new(f));
        self
    }

    /// React `handleExecuteClick` (the Execute button).
    pub(crate) fn on_execute(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_execute = Some(Rc::new(f));
        self
    }

    /// React `handleCancel` (the Cancel button).
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// React `handleReReviewPlan` (the result step's re-review).
    pub(crate) fn on_re_review(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_re_review = Some(Rc::new(f));
        self
    }

    /// React `runPreview({ quiet: true })` from the error card (React 1591).
    pub(crate) fn on_retry_preview(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_retry_preview = Some(Rc::new(f));
        self
    }

    /// React `runInspect` from the empty state (React 1530).
    pub(crate) fn on_reinspect(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_reinspect = Some(Rc::new(f));
        self
    }

    /// React `navigator.clipboard.writeText(ddlValue)` (React 1659): the
    /// host copies the DDL value for the named table.
    pub(crate) fn on_copy_ddl(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_copy_ddl = Some(Rc::new(f));
        self
    }

    /// React `setLimitationsOpen(false)` (React 1865).
    pub(crate) fn on_dismiss_limitations(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_limitations = Some(Rc::new(f));
        self
    }

    /// React `setExecuteConfirmOpen(false)` (React 1877).
    pub(crate) fn on_dismiss_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_confirm = Some(Rc::new(f));
        self
    }

    /// React `setErrorOpen(false)` (React 1883, 1886).
    pub(crate) fn on_dismiss_error(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_error = Some(Rc::new(f));
        self
    }

    /// The profile-name field's edit binding (Rune edits text through
    /// `TextEdit`, so the host hands the builder the accessor to its
    /// edit). Must be wired for the field to take focus, as the Backup
    /// window's fields.
    pub(crate) fn on_profile_name_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_profile_name_edit = Some(Rc::new(f));
        self
    }

    /// The batch-size field's edit binding.
    pub(crate) fn on_batch_size_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_batch_size_edit = Some(Rc::new(f));
        self
    }

    /// The SQL-file target-database field's edit binding.
    pub(crate) fn on_sql_file_database_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_sql_file_database_edit = Some(Rc::new(f));
        self
    }

    /// The SQL-file target-schema field's edit binding.
    pub(crate) fn on_sql_file_schema_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_sql_file_schema_edit = Some(Rc::new(f));
        self
    }

    /// The window on its own, exactly as the probes mount it. The
    /// attached-jobs strip reads the host state through its adapters, so
    /// the build takes it (unlike the Backup window, whose strips have no
    /// state adapters yet).
    pub(crate) fn build(self, s: &S, dark: bool) -> El {
        let (root, layer) = self.build_inner(s, dark);
        rune_kit::root(root, layer)
    }

    /// The same window without its kit root: the window element plus
    /// the overlay layer (the error dialog; the seams' dialogs).
    pub(crate) fn build_inner(mut self, s: &S, dark: bool) -> (El, Vec<El>) {
        self.dark = dark;
        let k = kit(dark);
        let vm = self.view_vm();
        let mut layer: Vec<El> = Vec::new();
        let mut root = col().key("data-transfer-window").w_full().h_full().min_h(0.0);
        root = root.child(self.chrome(&k));
        root = root.child(self.hydration_hints(&vm, &k));
        root = root.child(self.job_strips(s, &vm));
        root = root.child(self.step_rail(&vm, &k));
        root = root.child(self.step_body(&vm, &k, &mut layer));
        root = root.child(self.footer(&vm, &k));
        root = root.child(self.status_bar_seam(&k));
        self.error_dialog(&vm, &k, &mut layer);
        self.limitations_seam(&vm, &k, &mut layer);
        self.execute_confirm_seam(&vm, &k, &mut layer);
        (root, layer)
    }

    /// The Vm this frame renders: the host's Vm with the edits' current
    /// text overlaid, so typing shows up without a host sync step.
    fn view_vm(&self) -> DataTransferWindowVm {
        let mut vm = self.vm.clone();
        vm.profile_name = self.edits.profile_name.text.clone();
        vm.batch_size_edit = self.edits.batch_size.text.clone();
        vm.sql_file_database = self.edits.sql_file_database.text.clone();
        vm.sql_file_schema = self.edits.sql_file_schema.text.clone();
        vm
    }

    /// React `<WindowChromeSlot title={t('common.dataTransfer')} />`: the
    /// 40pt title band.
    fn chrome(&self, k: &Kit) -> El {
        row()
            .key("data-transfer-window-chrome")
            .window_drag()
            .w_full()
            .h(40.0)
            .center_y()
            .px(12.0)
            .child(
                text(t("common.dataTransfer").to_owned())
                    .key("data-transfer-window-title")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_base)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
    }

    /// React 1085-1094: the amber hint bars above the strips.
    fn hydration_hints(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        let mut rows = col().key("data-transfer-hydration-hints").w_full();
        if self.hydration.verification_pending {
            rows = rows.child(self.amber_hint(
                "data-transfer-pending-verification-hint",
                t("migrationJob.pendingVerificationHint").to_owned(),
                k,
            ));
        }
        if let Some(error) = &self.hydration.hydration_error {
            rows = rows.child(self.amber_hint(
                "data-transfer-hydration-error",
                error.clone(),
                k,
            ));
        }
        let _ = vm;
        rows
    }

    /// The shared 24pt amber hint bar (React `text-xs text-amber-400`).
    fn amber_hint(&self, key: &str, message: String, k: &Kit) -> El {
        row()
            .key(key.to_owned())
            .w_full()
            .h(24.0)
            .center_y()
            .px(24.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                text(message)
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg)
                    .truncate(),
            )
    }

    /// React 1095-1105: the attached-jobs strip and the terminal-results
    /// strip, fed by the hydration.
    fn job_strips(&self, s: &S, vm: &DataTransferWindowVm) -> El {
        // The attached-jobs strip needs the host's state adapters (its
        // `read_state` / `begin_cancel` fn pointers — the cancel buttons
        // write through them); the app-assembly task wires them. Without
        // them the strip does not mount (React renders it from the same
        // hydration data, React 1095-1098).
        let attached = match self.attached_jobs_adapters {
            Some((read_state, begin_cancel)) => DataTransferAttachedJobs::<S>::new(
                self.hydration.active_jobs.clone(),
                self.exclude_job_id.clone(),
                read_state,
                begin_cancel,
            )
            .build(s, self.dark),
            None => el(),
        };
        let terminal = DataTransferTerminalResults::<S>::new(
            self.hydration.jobs.clone(),
            self.exclude_job_id.clone(),
            self.terminal_hide || vm.job_run.phase != crate::views::TransferRunPhase::Idle,
        )
        .build(self.dark);
        col()
            .key("data-transfer-job-strips")
            .w_full()
            .child(attached)
            .child(terminal)
    }

    /// React 1107-1140: the step rail — the six steps with chevrons,
    /// numbered discs, and the `transfer.step.*` labels.
    fn step_rail(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        let step_index = vm.step.index();
        let mut rail = row()
            .key("data-transfer-step-rail")
            .w_full()
            .h(44.0)
            .center()
            .gap(4.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
        for (i, step) in STEPS.iter().enumerate() {
            if i > 0 {
                rail = rail.child(
                    icon(&ICON_CHEVRON_RIGHT, 12.0)
                        .key(format!("data-transfer-step-chevron-{i}"))
                        .color(k.c.text_faint),
                );
            }
            let label = text(t(step.label_key()).to_owned())
                .key(format!("data-transfer-step-{}", step.as_key()))
                .font(if i == step_index {
                    k.ty.text_xsmall_medium
                } else {
                    k.ty.text_xsmall
                })
                .color(if i == step_index {
                    k.c.text_accent
                } else if i < step_index {
                    k.c.text_accent
                } else {
                    k.c.text_faint
                });
            let disc = row()
                .key(format!("data-transfer-step-disc-{}", step.as_key()))
                .w(20.0)
                .h(20.0)
                .center()
                .radius(10.0)
                .fill(if i == step_index {
                    k.c.bg_accent
                } else {
                    k.c.surface_raised
                })
                .child(
                    text((i + 1).to_string())
                        .font(TextStyle::new(10.0, 600.0).lh(12.0))
                        .color(if i == step_index {
                            rune::Color::WHITE
                        } else if i < step_index {
                            k.c.text_accent
                        } else {
                            k.c.text_faint
                        }),
                );
            rail = rail.child(
                row()
                    .key(format!("data-transfer-step-cell-{}", step.as_key()))
                    .center_y()
                    .gap(6.0)
                    .child(disc)
                    .child(label),
            );
        }
        rail
    }

    /// React 1142-1757: the step body column (narrow for the three
    /// narrow steps, React 1146).
    fn step_body(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) -> El {
        let narrow = NARROW_STEPS.contains(&vm.step);
        let mut body = col()
            .key("data-transfer-step-body")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(row().key("data-transfer-step-pad-top").h(32.0));
        let card = match vm.step {
            WizardStep::Endpoints => self.endpoints_step(vm, k, layer),
            WizardStep::Setup => self.setup_step(vm, k, layer),
            WizardStep::Objects => self.objects_step(vm, k),
            WizardStep::Mapping => self.mapping_step(vm, k),
            WizardStep::Preview => self.preview_step(vm, k),
            WizardStep::Result => self.result_step(vm, k),
        };
        body = body.child(
            el()
                .key("data-transfer-step-column")
                .w(if narrow { 672.0 } else { 1152.0 })
                .child(card),
        );
        body
    }

    // -- the endpoints step (React 1149-1398) ---------------------------------

    /// React 1151-1192: the profile row — name field, save button,
    /// profile select, load button.
    fn profile_row(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut name = TextField::new("data-transfer-profile-name", &self.edits.profile_name)
            .height(28.0)
            .placeholder(t("transfer.profile.namePlaceholder").to_owned());
        if let Some(edit) = &self.on_profile_name_edit {
            let edit = edit.clone();
            name = name.on_edit(move |s: &mut S| edit(s));
        }
        let mut profile_options: Vec<String> =
            self.vm.transfer_profiles.iter().map(|p| p.name.clone()).collect();
        profile_options.push(t("transfer.profile.select").to_owned());
        let selected_profile_name = if vm.selected_profile_id.is_empty() {
            None
        } else {
            Some(
                self.vm
                    .transfer_profiles
                    .iter()
                    .find(|p| p.id == vm.selected_profile_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_default(),
            )
        };
        let save = self.on_profile_save.clone().unwrap_or_else(no_op_unit::<S>);
        let load = self.on_profile_load.clone().unwrap_or_else(no_op_unit::<S>);
        let load_disabled = vm.selected_profile_id.is_empty();
        row()
            .key("data-transfer-profile-row")
            .w_full()
            .wrap()
            .gap(8.0)
            .px(12.0)
            .py(12.0)
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(name.build(self.dark).w(192.0))
            .child(
                Button::new(t("transfer.profile.save"))
                    .variant(ButtonVariant::Neutral)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| save(s))
                    .build(k)
                    .key("data-transfer-profile-save"),
            )
            .child(
                Select::new("data-transfer-profile-select", &profile_options)
                    .placeholder(t("transfer.profile.select").to_owned())
                    .current(selected_profile_name.as_deref())
                    .on_toggle(|_s: &mut S| {})
                    .on_close(|_s: &mut S| {})
                    .build(k, layer),
            )
            .child(
                Button::new(t("transfer.profile.load"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .disabled(load_disabled)
                    .on_press(move |s: &mut S| load(s))
                    .build(k)
                    .key("data-transfer-profile-load"),
            )
    }

    /// React 1193-1210: the destination buttons.
    fn destination_row(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        let database = self.on_destination_database.clone().unwrap_or_else(no_op_unit::<S>);
        let sql_file = self.on_choose_sql_file.clone().unwrap_or_else(no_op_unit::<S>);
        let sql_label = if vm.sql_file_target.is_some() {
            t("transfer.destination.sqlFileSelected")
        } else {
            t("transfer.destination.sqlFile")
        };
        row()
            .key("data-transfer-destination-row")
            .w_full()
            .gap(8.0)
            .px(8.0)
            .py(8.0)
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(
                Button::new(t("transfer.destination.database"))
                    .variant(if vm.destination_mode == TransferDestinationMode::Database {
                        ButtonVariant::Submit
                    } else {
                        ButtonVariant::Neutral
                    })
                    .on_press(move |s: &mut S| database(s))
                    .build(k)
                    .key("data-transfer-destination-database"),
            )
            .child(
                Button::new(sql_label)
                    .variant(if vm.destination_mode == TransferDestinationMode::SqlFile {
                        ButtonVariant::Submit
                    } else {
                        ButtonVariant::Neutral
                    })
                    .on_press(move |s: &mut S| sql_file(s))
                    .build(k)
                    .key("data-transfer-destination-sql-file"),
            )
    }

    /// React 1211-1249: the database destination — the endpoints bar.
    fn endpoints_bar(&self, vm: &DataTransferWindowVm) -> El {
        let footer_note = self.endpoints_footer_note(vm);
        let bar_vm = EndpointsBarVm {
            test_id_prefix: "data-transfer".to_owned(),
            i18n_prefix: Some(MigrationI18nPrefix::Transfer),
            layout: EndpointsLayout::Grid,
            source: EndpointVm {
                connection_id: vm.source_id.clone(),
                options: self.options(vm.conn_options()),
                database: vm.source_database.clone(),
                databases: vm.source_databases.clone(),
                ..EndpointVm::default()
            },
            target: EndpointVm {
                connection_id: vm.target_id.clone(),
                options: self.options(vm.target_options()),
                database: vm.target_database.clone(),
                databases: vm.target_databases.clone(),
                read_only_hint: vm.target_read_only(),
                ..EndpointVm::default()
            },
            include_empty_connection_option: true,
            hide_database_until_connected: true,
            show_swap: Some(false),
            show_compare: Some(false),
            source_label_key: Some("transfer.source".to_owned()),
            target_label_key: Some("transfer.target".to_owned()),
            footer_note,
            ..EndpointsBarVm::default()
        };
        MigrationEndpointsBar::<S>::new(bar_vm).build(self.dark)
    }

    /// React 1235-1248: the endpoints bar's `footerNote` — the pairing
    /// refusal's reason (`TransferPairingNote`, T286's file in this
    /// segment; the note body is the reason string) or the experimental
    /// pair hint.
    fn endpoints_footer_note(&self, vm: &DataTransferWindowVm) -> Option<String> {
        if let Some(pairing) = vm.pairing() {
            if !pairing.supported {
                return pairing.reason.clone();
            }
        } else if let (Some(source), Some(target)) = (vm.source_conn(), vm.target_conn()) {
            if !crate::views::is_verified_migration_pair(&source.database_type, &target.database_type) {
                // React 1239-1247: the experimental pair hint.
                return Some(t("common.experimentalPairHint").to_owned());
            }
        }
        None
    }

    /// The `{value, label, disabled}` bar options from the window's
    /// option lists.
    fn options(&self, options: Vec<ConnectionOption>) -> Vec<EndpointOption> {
        options
            .into_iter()
            .map(|option| EndpointOption::new(option.value, option.label).disabled(option.disabled))
            .collect()
    }

    /// React 1250-1397: the sql-file destination card.
    fn sql_file_card(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) -> El {
        let conn_labels: Vec<String> = self.vm.conn_options().iter().map(|o| o.label.clone()).collect();
        let dialect_label = if vm.sql_file_dialect == "source" {
            t("transfer.destination.sourceDialect").to_owned()
        } else {
            vm.sql_file_dialect.clone()
        };
        let mut card = col()
            .key("data-transfer-sql-file-card")
            .w_full()
            .gap(12.0)
            .px(20.0)
            .py(20.0)
            .radius(8.0)
            .border(1.0, k.c.border_base);
        // React 1252-1268: the source connection + database selects.
        card = card.child(
            Select::new("data-transfer-source", &conn_labels)
                .placeholder(t("transfer.selectSource").to_owned())
                .on_toggle(|_s: &mut S| {})
                .on_close(|_s: &mut S| {})
                .build(k, layer),
        );
        card = card.child(
            Select::new(
                "data-transfer-source-database",
                &vm.source_databases,
            )
            .placeholder(t("transfer.selectDatabase").to_owned())
            .on_toggle(|_s: &mut S| {})
            .on_close(|_s: &mut S| {})
            .build(k, layer),
        );
        // React 1269-1299: the dialect select with the source-dialect
        // option first (React 1274-1287).
        let mut dialect_options = vec![dialect_label.clone()];
        for dialect in &vm.available_sql_dialects {
            dialect_options.push(dialect.clone());
        }
        card = card.child(
            Select::new("data-transfer-sql-file-dialect", &dialect_options)
                .placeholder(dialect_label)
                .on_toggle(|_s: &mut S| {})
                .on_close(|_s: &mut S| {})
                .build(k, layer),
        );
        card = card.child(
            text(t("transfer.destination.sqlDialectHint").to_owned())
                .key("data-transfer-sql-dialect-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );
        // React 1307-1333: the encoding select.
        card = card.child(
            Select::new(
                "data-transfer-sql-file-encoding",
                &[
                    t("transfer.destination.sqlEncodingUtf8").to_owned(),
                    t("transfer.destination.sqlEncodingUtf8Bom").to_owned(),
                    t("transfer.destination.sqlEncodingUtf16Le").to_owned(),
                    t("transfer.destination.sqlEncodingUtf16Be").to_owned(),
                ],
            )
            .on_toggle(|_s: &mut S| {})
            .on_close(|_s: &mut S| {})
            .build(k, layer),
        );
        // React 1338-1356: the compression select.
        card = card.child(
            Select::new(
                "data-transfer-sql-file-compression",
                &[
                    t("transfer.destination.sqlCompressionNone").to_owned(),
                    t("transfer.destination.sqlCompressionGzip").to_owned(),
                ],
            )
            .on_toggle(|_s: &mut S| {})
            .on_close(|_s: &mut S| {})
            .build(k, layer),
        );
        // React 1357-1388: the target database / schema inputs.
        let mut database_field = TextField::new(
            "data-transfer-sql-file-target-database",
            &self.edits.sql_file_database,
        )
        .height(28.0)
        .placeholder(t("transfer.destination.targetDatabasePlaceholder").to_owned());
        if let Some(edit) = &self.on_sql_file_database_edit {
            let edit = edit.clone();
            database_field = database_field.on_edit(move |s: &mut S| edit(s));
        }
        card = card.child(database_field.build(self.dark));
        let mut schema_field = TextField::new(
            "data-transfer-sql-file-target-schema",
            &self.edits.sql_file_schema,
        )
        .height(28.0)
        .placeholder(t("transfer.destination.targetSchemaPlaceholder").to_owned());
        if let Some(edit) = &self.on_sql_file_schema_edit {
            let edit = edit.clone();
            schema_field = schema_field.on_edit(move |s: &mut S| edit(s));
        }
        card = card.child(schema_field.build(self.dark));
        // React 1389-1396: the two hints.
        card = card.child(
            text(t("transfer.destination.targetScopeHint").to_owned())
                .key("data-transfer-sql-scope-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );
        let chosen_hint = if vm.sql_file_target.is_some() {
            t("transfer.destination.sqlFileHint")
        } else {
            t("transfer.destination.chooseHint")
        };
        card = card.child(
            text(chosen_hint.to_owned())
                .key("data-transfer-sql-file-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );
        card
    }

    /// React 1149-1400: the whole endpoints step.
    fn endpoints_step(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut step = col()
            .key("data-transfer-endpoints-step")
            .w_full()
            .gap(16.0);
        step = step.child(self.profile_row(vm, k, layer));
        step = step.child(self.destination_row(vm, k));
        if vm.destination_mode == TransferDestinationMode::Database {
            step = step.child(self.endpoints_bar(vm));
        } else {
            step = step.child(self.sql_file_card(vm, k, layer));
        }
        step
    }

    // -- the setup step (React 1402-1510) ------------------------------------

    /// React 1404-1428: the three mode radio cards.
    fn mode_cards(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        let modes = [
            (
                TransferMode::Data,
                "data",
                t("common.dataOnly"),
                t("transfer.mode.dataHint"),
            ),
            (
                TransferMode::Structure,
                "structure",
                t("common.structureOnly"),
                t("transfer.mode.structureHint"),
            ),
            (
                TransferMode::StructureAndData,
                "both",
                t("transfer.mode.both"),
                t("transfer.mode.bothHint"),
            ),
        ];
        // The React radios' `data-testid`s (`data-transfer-mode-data`
        // / `-structure` / `-both`) are the group's items; the kit radio
        // item carries no per-item key, so the group's key is the one the
        // tree observes (a documented deviation).
        let mut group = RadioGroup::new("data-transfer-mode").label(t("transfer.setup.modeSection").to_owned());
        for (mode, _key, label, hint) in modes {
            group = group.item(
                RadioItem::new(label, vm.mode == mode).description(hint),
            );
        }
        group.build(k)
    }

    /// React 1429-1508: the options column.
    fn setup_step(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut step = col()
            .key("data-transfer-setup-step")
            .w_full()
            .gap(12.0)
            .px(24.0)
            .py(24.0)
            .radius(8.0)
            .border(1.0, k.c.border_base);
        step = step.child(self.mode_cards(vm, k));
        // React 1433-1450: the collation checkbox (structure mode or the
        // drop-create write mode).
        let show_collation = vm.mode != TransferMode::Data || vm.write_mode == WriteMode::DropCreateInsert;
        if show_collation {
            step = step.child(
                Checkbox::new("data-transfer-use-target-default-collation", vm.use_target_default_collation)
                    .label(t("transfer.mapping.targetDefaultCollation"))
                    .description(t("transfer.mapping.targetDefaultCollationHint"))
                    .build(k),
            );
        }
        // React 1451-1464: the write mode select.
        step = step.child(
            Select::new(
                "data-transfer-write-mode",
                &[
                    t("transfer.writeMode.insert").to_owned(),
                    t("transfer.writeMode.truncateInsert").to_owned(),
                    t("transfer.writeMode.dropCreate").to_owned(),
                ],
            )
            .on_toggle(|_s: &mut S| {})
            .on_close(|_s: &mut S| {})
            .build(k, layer),
        );
        // React 1465-1474: the destructive confirm.
        if vm.write_mode != crate::views::WriteMode::Insert {
            step = step.child(
                Checkbox::new("data-transfer-destructive-confirm", vm.confirmed_destructive)
                    .label(t("transfer.destructiveConfirm"))
                    .build(k),
            );
        }
        // React 1475-1493: the batch size + the error banner.
        let mut batch = TextField::new("data-transfer-batch-size", &self.edits.batch_size)
            .height(28.0);
        if let Some(edit) = &self.on_batch_size_edit {
            let edit = edit.clone();
            batch = batch.on_edit(move |s: &mut S| edit(s));
        }
        step = step.child(batch.build(self.dark).w(192.0));
        if !vm.valid_batch_size() {
            if let Some(banner) = Banner::new(
                "data-transfer-batch-size-error",
                t("transfer.batchSizeLimit"),
            )
            .variant(BannerVariant::Plain)
            .as_element(BannerElement::P) // React <ErrorBanner as="p">
            .build(k)
            {
                step = step.child(banner);
            }
        }
        // React 1494-1500: the resume hint.
        step = step.child(
            text(t("transfer.resumeCapabilityHint").to_owned())
                .key("data-transfer-resume-capability-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );
        // React 1501-1507: the stop-on-error checkbox.
        step = step.child(
            Checkbox::new("data-transfer-stop-on-error", vm.stop_on_error)
                .label(t("transfer.stopOnError"))
                .build(k),
        );
        step
    }

    // -- the objects step (React 1512-1555) ----------------------------------

    fn objects_step(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        if vm.loading {
            // React 1514-1517: the centered spinner.
            return row()
                .key("data-transfer-objects-loading")
                .w_full()
                .h(288.0)
                .center()
                .child(
                    Spinner::new("data-transfer-objects-spinner")
                        .size(SpinnerSize::Lg)
                        .tone(SpinnerTone::Accent)
                        .build(self.dark),
                );
        }
        if vm.tables.is_empty() {
            // React 1518-1535: the empty state with the re-inspect button.
            let reinspect = self.on_reinspect.clone().unwrap_or_else(no_op_unit::<S>);
            return col()
                .key("data-transfer-objects-empty")
                .w_full()
                .gap(8.0)
                .px(16.0)
                .py(32.0)
                .center()
                .radius(8.0)
                .border(1.0, k.c.border_base)
                .child(
                    text(t("transfer.objects.noTablesFound").to_owned())
                        .key("data-transfer-objects-empty-title")
                        .font(k.ty.small_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text(t("transfer.objects.noTablesHint").to_owned())
                        .key("data-transfer-objects-empty-hint")
                        .font(k.ty.small)
                        .color(k.c.text_faint),
                )
                .child(
                    Button::new(t("transfer.objects.reInspect"))
                        .variant(ButtonVariant::Submit)
                        .control_size(ControlSize::Small)
                        .on_press(move |s: &mut S| reinspect(s))
                        .build(k)
                        .key("data-transfer-reinspect"),
                );
        }
        // React 1536-1553: the table rows.
        let mut list = col()
            .key("data-transfer-objects-list")
            .w_full()
            .radius(8.0)
            .border(1.0, k.c.border_base);
        for table in &vm.tables {
            let table = table.clone();
            let source = table.vm.source_table.clone();
            let status = table.vm.status.spelling().to_owned();
            let target = {
                let target = table.vm.target_table.clone();
                if target.is_empty() {
                    "—".to_owned()
                } else {
                    target
                }
            };
            let enabled = table.vm.enabled;
            list = list.child(
                row()
                    .key(format!("data-transfer-table-row-{source}"))
                    .w_full()
                    .h(32.0)
                    .center_y()
                    .gap(8.0)
                    .px(12.0)
                    .border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_base)
                    .child(Checkbox::new(format!("data-transfer-table-check-{source}"), enabled).build(k))
                    .child(
                        text(source.clone())
                            .key(format!("data-transfer-table-name-{source}"))
                            .font(k.ty.text_xsmall)
                            .grow(1.0)
                            .min_w(0.0)
                            .truncate(),
                    )
                    .child(
                        text(format!("→ {target}"))
                            .key(format!("data-transfer-table-target-{source}"))
                            .font(k.ty.small)
                            .color(k.c.text_faint),
                    )
                    .child(
                        text(status)
                            .key(format!("data-transfer-table-status-{source}"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_faint),
                    ),
            );
        }
        list
    }

    // -- the mapping step (React 1557-1567) ----------------------------------

    fn mapping_step(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        let mut step = col()
            .key("data-transfer-mapping-step")
            .w_full()
            .gap(8.0);
        // The T267 seam: the mapping editor's table shell is that task's
        // (this segment's); the window only renders the gate notice and
        // the seam row. React 1557-1566.
        step = step.child(
            row()
                .key("data-transfer-mapping-seam")
                .w_full()
                .h(48.0)
                .center_y()
                .px(12.0)
                .radius(8.0)
                .border(1.0, k.c.border_base)
                .child(
                    text(t("transfer.step.mapping").to_owned())
                        .font(k.ty.small)
                        .color(k.c.text_faint)
                        .grow(1.0),
                )
                .child(
                    text("T267".to_owned())
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_faint),
                ),
        );
        // The T176 gate-notice placeholder: the gate error the
        // post-prepare re-check recorded, or the block reason the rows
        // report (React 1569-1573, MappingGateNotice.tsx).
        if !vm.mapping_gate_error.is_empty() {
            step = step.child(
                text(vm.mapping_gate_error.clone())
                    .key("data-transfer-mapping-gate-notice")
                    .font(k.ty.small)
                    .color(k.c.warning.fg),
            );
        } else if let Some(reason) = crate::views::mapping_gate_block_reason(&vm.table_vms()) {
            step = step.child(
                text(t(reason).to_owned())
                    .key("data-transfer-mapping-gate-notice")
                    .font(k.ty.small)
                    .color(k.c.warning.fg),
            );
        }
        step
    }

    // -- the preview step (React 1575-1720) ----------------------------------

    fn preview_step(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        if vm.loading {
            return row()
                .key("data-transfer-preview-loading")
                .w_full()
                .h(288.0)
                .center()
                .child(
                    Spinner::new("data-transfer-preview-spinner")
                        .size(SpinnerSize::Lg)
                        .tone(SpinnerTone::Accent)
                        .build(self.dark),
                );
        }
        let Some(preview) = &vm.preview else {
            // React 1575-1605: the error card.
            let retry = self.on_retry_preview.clone().unwrap_or_else(no_op_unit::<S>);
            let back = self.on_back.clone().unwrap_or_else(no_op_unit::<S>);
            let message = if vm.preview_error.is_empty() {
                t("transfer.preview.failedHint").to_owned()
            } else {
                vm.preview_error.clone()
            };
            return col()
                .key("data-transfer-preview-error")
                .w_full()
                .gap(12.0)
                .px(16.0)
                .py(32.0)
                .center()
                .radius(8.0)
                .border(1.0, k.c.border_base)
                .child(
                    text(t("transfer.preview.failed").to_owned())
                        .key("data-transfer-preview-error-title")
                        .font(k.ty.small_medium)
                        .color(k.c.text_base),
                )
                .child(
                    CopyableError::new("data-transfer-preview-error-message", &message).build::<S>(k),
                )
                .child(
                    row()
                        .key("data-transfer-preview-error-actions")
                        .gap(8.0)
                        .child(
                            Button::new(t("transfer.preview.retry"))
                                .variant(ButtonVariant::Submit)
                                .control_size(ControlSize::Small)
                                .on_press(move |s: &mut S| retry(s))
                                .build(k)
                                .key("data-transfer-preview-retry"),
                        )
                        .child(
                            Button::new(t("transfer.preview.backToMapping"))
                                .variant(ButtonVariant::Ghost)
                                .control_size(ControlSize::Small)
                                .on_press(move |s: &mut S| back(s))
                                .build(k)
                                .key("data-transfer-preview-back-mapping"),
                        ),
                );
        };
        // React 1613-1719: the preview body.
        let mut body = col()
            .key("data-transfer-preview")
            .w_full()
            .gap(12.0);
        if vm.executing {
            // React 1615-1623: the executing overlay strip.
            body = body.child(
                col()
                    .key("data-transfer-executing-overlay")
                    .w_full()
                    .h(96.0)
                    .center()
                    .gap(8.0)
                    .radius(8.0)
                    .child(
                        Spinner::new("data-transfer-executing-spinner")
                            .size(SpinnerSize::Lg)
                            .tone(SpinnerTone::Accent)
                            .build(self.dark),
                    )
                    .child(
                        text(if vm.execute_progress.is_empty() {
                            t("transfer.executing").to_owned()
                        } else {
                            vm.execute_progress.clone()
                        })
                        .font(k.ty.small)
                        .color(k.c.text_faint),
                    ),
            );
        }
        if let Some(block) = &preview.block_reason {
            // React 1624-1628: the block reason.
            body = body.child(
                text(block.clone())
                    .key("data-transfer-preview-block-reason")
                    .font(k.ty.small)
                    .color(k.c.warning.fg),
            );
        }
        // React 1629-1690: the DDL cards.
        for (index, item) in preview.ddl.iter().enumerate() {
            body = body.child(self.ddl_card(vm, item, index, k));
        }
        // React 1691-1713: the write plans.
        for plan in &preview.write_plans {
            let source = &plan.source_table;
            let rows = plan
                .estimated_rows
                .map(|rows| rows.to_string())
                .unwrap_or_else(|| "—".to_owned());
            body = body.child(
                col()
                    .key(format!("data-transfer-write-plan-{source}"))
                    .w_full()
                    .gap(4.0)
                    .px(12.0)
                    .py(12.0)
                    .radius(8.0)
                    .border(1.0, k.c.border_base)
                    .child(
                        row()
                            .key(format!("data-transfer-write-plan-head-{source}"))
                            .child(
                                text(format!(
                                    "{} → {} ({})",
                                    plan.source_table, plan.target_table, plan.write_mode.as_str()
                                ))
                                .font(k.ty.small)
                                .color(k.c.text_base),
                            ),
                    )
                    .child(
                        text(format!("{}: {rows}", t("transfer.estimatedRows")))
                            .font(k.ty.small)
                            .color(k.c.text_faint),
                    ),
            );
        }
        // React 1714-1718: the warnings.
        for (index, warning) in preview.warnings.iter().enumerate() {
            body = body.child(
                text(warning.clone())
                    .key(format!("data-transfer-preview-warning-{index}"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            );
        }
        // React 1730-1742: the cancel disposition notice.
        if vm.step == WizardStep::Preview && vm.job_run.cancel_requested {
            let message = if vm.job_run.cancel_unknown_job {
                t("migration.cancel.unknownJob")
            } else {
                t("migration.cancel.requestedInFlight")
            };
            body = body.child(
                text(message.to_owned())
                    .key("data-transfer-cancel-disposition")
                    .font(k.ty.small)
                    .color(k.c.text_faint),
            );
        }
        body
    }

    /// React 1629-1690: one DDL card — the label row (with the copy
    /// button) and the `<pre>` body (the editable variant is T219's).
    fn ddl_card(
        &self,
        vm: &DataTransferWindowVm,
        item: &crate::views::TransferDdlPreviewVm,
        index: usize,
        k: &Kit,
    ) -> El {
        let kind = item.kind.unwrap_or(DdlKind::Table);
        let kind_label = match kind {
            DdlKind::Table => None,
            DdlKind::Index => Some(t("transfer.ddlKind.index")),
            DdlKind::ForeignKey => Some(t("transfer.ddlKind.foreignKey")),
            DdlKind::DropTable => Some(t("transfer.ddlKind.dropTable")),
        };
        let head = if let Some(kind_label) = kind_label {
            format!("{} → {} ({})", item.source_table, item.target_table, kind_label)
        } else {
            format!("{} → {}", item.source_table, item.target_table)
        };
        // The value React copies / edits: the override when the wizard
        // holds one, else the plan's own DDL (React 1631-1635).
        let table_override = vm
            .tables
            .iter()
            .find(|row| row.vm.source_table == item.source_table)
            .and_then(|row| row.ddl_override.clone());
        let sql_file = vm.destination_mode == TransferDestinationMode::SqlFile;
        let ddl_value = if sql_file || kind != DdlKind::Table {
            item.ddl.clone()
        } else {
            table_override.unwrap_or_else(|| item.ddl.clone())
        };
        let copy = self.on_copy_ddl.clone().unwrap_or_else(|| {
            Rc::new(|_: &mut S, _: String| {})
        });
        let ddl_for_copy = ddl_value.clone();
        let kind_key = kind.as_key();
        let mut card = col()
            .key(format!("data-transfer-ddl-card-{}-{kind_key}-{index}", item.source_table))
            .w_full()
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(
                row()
                    .key(format!(
                        "data-transfer-ddl-head-{}-{kind_key}-{index}",
                        item.source_table
                    ))
                    .w_full()
                    .h(28.0)
                    .center_y()
                    .px(12.0)
                    .gap(8.0)
                    .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                    .child(
                        text(head)
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_faint)
                            .grow(1.0)
                            .min_w(0.0)
                            .truncate(),
                    )
                    .child(
                        Button::new(t("common.copyDdl"))
                            .variant(ButtonVariant::Ghost)
                            .control_size(ControlSize::Small)
                            .on_press(move |s: &mut S| copy(s, ddl_for_copy.clone()))
                            .build(k)
                            .key(format!(
                                "data-transfer-copy-ddl-{}{}",
                                item.source_table,
                                if kind == DdlKind::Table {
                                    String::new()
                                } else {
                                    format!("-{kind_key}")
                                }
                            )),
                    ),
            );
        // React 1664-1687: the plain `<pre>` body (or the T219 editor
        // seam for the editable database-table variant).
        if sql_file || kind != DdlKind::Table {
            card = card.child(
                text(ddl_value)
                    .key(format!(
                        "data-transfer-ddl-preview-{}-{kind_key}-{index}",
                        item.source_table
                    ))
                    .font(k.ty.mono)
                    .color(k.c.text_base)
                    .w_full()
                    .min_h(192.0)
                    .padding(12.0),
            );
        } else {
            card = card.child(
                col()
                    .key(format!("data-transfer-ddl-editor-{}", item.source_table))
                    .w_full()
                    .min_h(192.0)
                    .center()
                    .child(
                        text(t("transfer.ddlOverrideHint").to_owned())
                            .key(format!("data-transfer-ddl-override-hint-{}", item.source_table))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_faint),
                    )
                    .child(
                        text("T219".to_owned())
                            .key(format!("data-transfer-ddl-editor-seam-{}", item.source_table))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_faint),
                    ),
            );
        }
        card
    }

    // -- the result step (React 1722-1728) -----------------------------------

    fn result_step(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        // The T246 seam: the result panel is that task's. The window
        // renders the seam row while the run machine's slots stay
        // readable through the Vm.
        let settled = vm.job_run.apply_view.is_some();
        row()
            .key("data-transfer-result-seam")
            .w_full()
            .h(if settled { 96.0 } else { 0.0 })
            .center_y()
            .px(12.0)
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(
                text(t("migration.verdict.rereview").to_owned())
                    .font(k.ty.small)
                    .color(k.c.text_faint)
                    .grow(1.0),
            )
            .child(
                text("T246".to_owned())
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            )
    }

    // -- the footer (React 1759-1859) -----------------------------------------

    fn footer(&self, vm: &DataTransferWindowVm, k: &Kit) -> El {
        let back = self.on_back.clone().unwrap_or_else(no_op_unit::<S>);
        let mut actions = row().key("data-transfer-footer-actions").gap(8.0);
        match vm.step {
            WizardStep::Preview => {
                // React 1767-1778: the cancel button while a prepare view
                // exists; the executing progress / pending-id notices.
                if vm.job_run.prepare_view.is_some() {
                    let cancel = self.on_cancel.clone().unwrap_or_else(no_op_unit::<S>);
                    let addressable = vm.job_run.cancel_target_job_id().is_some();
                    actions = actions.child(
                        Button::new(t("transfer.cancel"))
                            .variant(ButtonVariant::Ghost)
                            .disabled(!addressable)
                            .on_press(move |s: &mut S| cancel(s))
                            .build(k)
                            .key("data-transfer-cancel"),
                    );
                }
                if vm.executing {
                    actions = actions.child(
                        text(if vm.execute_progress.is_empty() {
                            t("transfer.executing").to_owned()
                        } else {
                            vm.execute_progress.clone()
                        })
                        .key("data-transfer-executing-progress")
                        .font(k.ty.small)
                        .color(k.c.text_faint),
                    );
                    if vm.job_run.cancel_target_job_id().is_none() {
                        // React 1799-1807: during the admission round trip
                        // no apply id exists yet.
                        actions = actions.child(
                            text(t("migration.cancel.unknownJob").to_owned())
                                .key("data-transfer-cancel-pending-id")
                                .font(k.ty.small)
                                .color(k.c.text_faint),
                        );
                    }
                }
                // React 1808-1817: the execute button.
                let execute = self.on_execute.clone().unwrap_or_else(no_op_unit::<S>);
                let disabled = !vm.can_execute() || vm.executing;
                let label = if vm.executing {
                    t("transfer.executing")
                } else {
                    t("transfer.execute")
                };
                actions = actions.child(
                    Button::new(label)
                        .variant(ButtonVariant::Submit)
                        .disabled(disabled)
                        .on_press(move |s: &mut S| execute(s))
                        .build(k)
                        .key("data-transfer-execute"),
                );
            }
            WizardStep::Result => {
                // React 1818-1847: the re-review button and its blocked
                // notice while a reconcile is pending.
                let re_review = self.on_re_review.clone().unwrap_or_else(no_op_unit::<S>);
                let blocked = vm.reconcile_pending();
                actions = actions.child(
                    Button::new(t("migration.verdict.rereview"))
                        .variant(ButtonVariant::Submit)
                        .disabled(blocked)
                        .on_press(move |s: &mut S| re_review(s))
                        .build(k)
                        .key("data-transfer-rereview"),
                );
                if blocked {
                    actions = actions.child(
                        text(t("migration.verdict.rereviewBlocked").to_owned())
                            .key("data-transfer-rereview-blocked")
                            .font(k.ty.small)
                            .color(k.c.text_faint),
                    );
                }
            }
            _ => {
                // React 1848-1857: the next button.
                let next = self.on_next.clone().unwrap_or_else(no_op_unit::<S>);
                let disabled = vm.next_disabled();
                actions = actions.child(
                    Button::new(t("transfer.next"))
                        .disabled(disabled)
                        .on_press(move |s: &mut S| next(s))
                        .build(k)
                        .key("data-transfer-next"),
                );
            }
        }
        row()
            .key("data-transfer-footer")
            .w_full()
            .h(52.0)
            .center_y()
            .px(24.0)
            .justify_between()
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(
                Button::new(t("transfer.back"))
                    .variant(ButtonVariant::Ghost)
                    .disabled(vm.back_disabled())
                    .on_press(move |s: &mut S| back(s))
                    .build(k)
                    .key("data-transfer-back"),
            )
            .child(actions)
    }

    /// The T262 status-bar seam (React 1861: `<StatusBar />`).
    fn status_bar_seam(&self, k: &Kit) -> El {
        row()
            .key("data-transfer-status-bar-seam")
            .w_full()
            .h(28.0)
            .center_y()
            .px(12.0)
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(
                text("T262".to_owned())
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            )
    }

    // -- the dialogs (React 1863-1897) ---------------------------------------

    /// React 1881-1897: the error dialog — the copyable message and the
    /// `common.ok` footer button.
    fn error_dialog(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) {
        if !vm.error_open {
            return;
        }
        let dismiss = self.on_dismiss_error.clone().unwrap_or_else(no_op_unit::<S>);
        let dismiss_close = dismiss.clone();
        Dialog::new("data-transfer-error-dialog", t("transfer.error").to_owned())
            .open(true)
            .fit()
            .width(480.0)
            .child(
                CopyableError::new("data-transfer-error", &vm.error_msg).build::<S>(k),
            )
            .footer(
                Button::new(t("common.ok"))
                    .variant(ButtonVariant::Submit)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| dismiss(s))
                    .build(k)
                    .key("data-transfer-error-ok"),
            )
            .on_close(move |s: &mut S| dismiss_close(s))
            .build(k, layer);
    }

    /// The limitations dialog seam (T-limitations): React 1863-1871
    /// mounts `LimitationsDialog`; until the ui component lands this
    /// renders the open placeholder row with the dismissal wiring.
    fn limitations_seam(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) {
        if !vm.limitations_open {
            return;
        }
        let dismiss = self
            .on_dismiss_limitations
            .clone()
            .unwrap_or_else(no_op_unit::<S>);
        let dismiss_close = dismiss.clone();
        Dialog::new("data-transfer-limitations-dialog", t("transfer.limitations.title").to_owned())
                .open(true)
                .fit()
                .width(480.0)
                .child(
                    row()
                        .key("data-transfer-limitations")
                        .w_full()
                        .py(16.0)
                        .center_y()
                        .child(
                            text(t("transfer.limitations.dontShowAgain").to_owned())
                                .font(k.ty.small)
                                .color(k.c.text_base)
                                .grow(1.0),
                        )
                        .child(
                            text("LimitationsDialog".to_owned())
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_faint),
                        ),
                )
                .footer(
                    Button::new(t("common.ok"))
                        .variant(ButtonVariant::Submit)
                        .control_size(ControlSize::Small)
                        .on_press(move |s: &mut S| dismiss(s))
                        .build(k)
                        .key("data-transfer-limitations-ok"),
                )
                .on_close(move |s: &mut S| dismiss_close(s))
                .build(k, layer);
    }

    /// The execute confirm seam (T225): React 1873-1879 mounts
    /// `TransferExecuteConfirmDialog`; until that task lands this
    /// renders the open placeholder with the confirm wiring.
    fn execute_confirm_seam(&self, vm: &DataTransferWindowVm, k: &Kit, layer: &mut Vec<El>) {
        if !vm.execute_confirm_open {
            return;
        }
        let dismiss = self
            .on_dismiss_confirm
            .clone()
            .unwrap_or_else(no_op_unit::<S>);
        let dismiss_close = dismiss.clone();
        Dialog::new(
            "data-transfer-execute-confirm",
            t("transfer.executeConfirm.title").to_owned(),
        )
                .open(true)
                .fit()
                .width(480.0)
                .child(
                    row()
                        .key("data-transfer-execute-confirm-seam")
                        .w_full()
                        .py(16.0)
                        .center_y()
                        .child(
                            text(t("transfer.destructiveConfirm").to_owned())
                                .font(k.ty.small)
                                .color(k.c.text_base)
                                .grow(1.0),
                        )
                        .child(
                            text("T225".to_owned())
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_faint),
                        ),
                )
                .footer(
                    Button::new(t("common.ok"))
                        .variant(ButtonVariant::Danger)
                        .control_size(ControlSize::Small)
                        .on_press(move |s: &mut S| dismiss(s))
                        .build(k)
                        .key("data-transfer-execute-confirm-ok"),
                )
                .on_close(move |s: &mut S| dismiss_close(s))
                .build(k, layer);
    }
}
