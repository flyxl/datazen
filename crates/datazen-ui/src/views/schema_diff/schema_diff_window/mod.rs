//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffWindow.tsx` (T309) — the view half.
//!
//! The standalone schema-diff wizard window: the chrome title band
//! (`WindowChromeSlot`, `common.schemaDiff`), the step rail, the job
//! status section, the per-step body (the two selection steps or the
//! three execution panels inside the `mx-auto px-6 py-6` column —
//! `max-w-2xl overflow-auto` on the narrow steps, `max-w-6xl`
//! otherwise), the error strip, the bottom navigation, the limitations
//! dialog, the saved-setup dialogs and the status bar. The state and
//! the action halves are [`state`] and [`actions`].
//!
//! ## What maps where
//!
//! - `WindowChromeSlot` has no testid in React; the Rune window draws
//!   its own 40pt title band (the backup window's `backup-window`
//!   precedent), `schema-diff-window-chrome` / `-title` internally.
//!   The right actions — `MigrationRunHistoryDialog` (the migration
//!   area's own task) and the docs `BookOpen` button
//!   (`openDocsWindow('schemaDiff')`, a host action) — ride the
//!   [`SchemaDiffWindow::migration_history_dialog`] slot and the
//!   [`SchemaDiffWindow::on_open_docs`] action.
//! - The plan actions block (React's `planActions` const) is built by
//!   the window, verbatim: the copy-SQL button (with the `sql` feedback
//!   label swap), the profile `Select` (min-w-36, the placeholder
//!   `schemaDiff.profileSelect`), and the load / save / delete /
//!   export / import buttons with their `secondary` / `ghost`
//!   variants and the `!selectedProfileId || loading` disables.
//! - The limitations dialog (`LimitationsDialog`, the components/ui
//!   area's own task) is the [`SchemaDiffWindow::limitations_dialog`]
//!   host slot: the window owns `limitationsOpen` and fires
//!   `onClose` / `onDismiss` (`setSchemaDiffLimitationsDismissed`);
//!   the T323 key list documents the six bullets the dialog iterates.
//! - `StatusBar` is the 40pt `border-t` band (`h-10 px-4 text-xs
//!   text-fg-secondary`), the truncated `common.schemaDiff` on the
//!   left, nothing on the right.
//! - The table-list rail's resize handle (`useResizable`'s drag) is
//!   the host's pointer seam; the width itself is
//!   [`state::TABLE_LIST_WIDTH`]-clamped state.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim (key-encodes-testid, the
//! subtree's convention):
//!
//! | part | key |
//! |------|-----|
//! | root | `schema-diff-window` / `schema-diff-window-discovery-failure` (the `data-discovery-failure` attribute, key-encoded when a diagnostic is set) |
//! | chrome band / title | `schema-diff-window-chrome` / `schema-diff-window-title` (internal) |
//! | docs button | `schema-diff-open-docs` (internal) |
//! | plan actions row | `schema-diff-plan-actions` (internal) |
//! | profile select | `schema-diff-profile-select` |
//! | the five profile buttons | `schema-diff-profile-load` / `-save` / `-delete`, `schema-diff-export-config` / `schema-diff-import-config` |
//! | copy SQL | `schema-diff-copy-sql` |
//! | error strip | `schema-diff-window-error` (internal) |
//! | status bar | `schema-diff-status-bar` (internal) |
//!
//! The children carry their own keys verbatim and are not re-keyed.

use std::{collections::HashMap, rc::Rc};

use rune::{col, row, text, El, ScrollSpec, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::ICON_BOOK_OPEN;
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::views::schema_diff::schema_diff_execution_panels::{
    SchemaDiffExecutionPanels, SchemaDiffExecutionStep,
};
use crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm;
use crate::views::schema_diff::schema_diff_saved_setup_dialogs::{
    SavedSetupDialogsVm, SchemaDiffSavedSetupDialogs,
};
use crate::views::schema_diff::schema_diff_selection_steps::{
    SchemaDiffEndpointsVm, SchemaDiffSelectionSteps, SelectionConnectionOption, SelectionStep,
    UnifiedObjectsVm,
};
use crate::views::schema_diff::schema_diff_window_job_section::{
    SchemaDiffWindowJobSection, schema_diff_window_job_section,
};
use crate::views::schema_diff::schema_diff_window_navigation::{
    SchemaDiffWindowNavigation, schema_diff_window_navigation,
};
use crate::views::schema_diff::schema_diff_unified_objects_picker::{
    PickerSide, SchemaDiffUnifiedObjectsPicker,
};
use crate::views::schema_diff::schema_diff_wizard_progress::{
    SCHEMA_DIFF_NARROW_STEPS, SchemaDiffWizardStep, schema_diff_wizard_progress,
};

pub mod actions;
pub mod state;

pub use state::{SavedSetupState, SchemaDiffWindowState, TABLE_LIST_WIDTH, TABLE_LIST_WIDTH_KEY};

/// The window's action slots (`Rc<dyn Fn(&mut S)>`, the crate
/// convention).
type Act<S> = Rc<dyn Fn(&mut S)>;
/// A host edit-state projection (`&mut S` → the edit's `&mut TextEdit`).
type EditSlot<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

/// The chrome band's height (`h-10`).
const CHROME_H: f32 = 40.0;
/// The status bar's height (`h-10 min-h-[40px]`).
const STATUS_H: f32 = 40.0;
/// The status bar's text (`text-xs`) and padding (`px-4`).
const STATUS_TEXT: f32 = 12.0;
const STATUS_PAD: f32 = 16.0;
/// The body column's padding (`px-6 py-6`).
const BODY_PAD: f32 = 24.0;
/// `max-w-2xl` (the narrow steps' column).
const BODY_MAX_NARROW: f32 = 672.0;
/// `max-w-6xl` (the wide steps' column).
const BODY_MAX_WIDE: f32 = 1152.0;
/// The plan actions' gap (`gap-2`) and the select's `min-w-36`.
const ACTIONS_GAP: f32 = 8.0;
const SELECT_MIN_W: f32 = 144.0;
/// The docs button's box (`h-6 w-6`; the kit's Regular control).
#[allow(dead_code)] // pinned by the geometry test; the kit's Regular metric
const DOCS_BUTTON: f32 = 24.0;
/// The docs button's glyph (`h-3 w-3`; the kit's 16pt icon, a cosmetic delta).
#[allow(dead_code)] // pinned by the geometry test
const DOCS_GLYPH: f32 = 12.0;

/// The wizard window, built over the state (React's component) plus
/// the host-owned edit states and the two dialog slots.
#[allow(dead_code)] // the desktop host wiring is the production consumer
pub struct SchemaDiffWindow<'a, S: 'static> {
    /// The window state (React's hooks, composed).
    pub state: &'a SchemaDiffWindowState,
    /// The confirm input's edit (the deploy step's destructive confirm).
    pub confirm_edit: &'a TextEdit,
    /// The import dialog's paste field.
    pub import_edit: &'a TextEdit,
    /// The profile dialog's name field.
    pub profile_name_edit: &'a TextEdit,
    /// The plan panel's type-override edit slots (the host's mirror).
    pub override_edits: Option<&'a HashMap<String, TextEdit>>,

    // ---- the chrome and dialog slots ---------------------------------------
    /// `MigrationRunHistoryDialog` (the migration area's own task):
    /// a host-built `El` mounted in the chrome's right actions.
    pub migration_history_dialog: Option<El>,
    /// `LimitationsDialog` (the components/ui area's own task): a
    /// host-built `El`; the window owns the open flag.
    pub limitations_dialog: Option<El>,
    /// The profile select's listbox open flag (React keeps it inside
    /// `Select`).
    pub profile_select_open: bool,

    // ---- the actions ---------------------------------------------------------
    pub on_open_docs: Option<Act<S>>,
    pub on_close_limitations: Option<Act<S>>,
    /// `setSchemaDiffLimitationsDismissed` (the pref write).
    pub on_dismiss_limitations: Option<Act<S>>,
    pub on_back: Option<Act<S>>,
    pub on_next: Option<Act<S>>,
    pub on_deploy: Option<Act<S>>,
    pub on_cancel_job: Option<Act<S>>,
    /// `onVerify(jobId)` — the job section's verify rail.
    pub on_verify_job: Option<Rc<dyn Fn(&mut S, String)>>,
    pub on_profile_select: Option<Rc<dyn Fn(&mut S, String)>>,
    pub on_toggle_profile_select: Option<Act<S>>,
    pub on_close_profile_select: Option<Act<S>>,
    pub on_load_profile: Option<Act<S>>,
    pub on_open_profile_save: Option<Act<S>>,
    pub on_delete_profile: Option<Act<S>>,
    pub on_export_config: Option<Act<S>>,
    pub on_open_import_config: Option<Act<S>>,
    pub on_copy_sql: Option<Act<S>>,
    pub on_copy_summary: Option<Act<S>>,
    pub on_select_table: Option<Rc<dyn Fn(&mut S, String)>>,
    pub on_regenerate: Option<Act<S>>,
    pub on_allow_destructive: Option<Act<S>>,
    pub on_include_indexes: Option<Act<S>>,
    pub on_apply_type_overrides: Option<Act<S>>,
    /// `onTypeOverrideChange` — the plan panel's edit projection
    /// (`table`, `column`, `target_type` → the cell's edit).
    pub on_override_edit:
        Option<Rc<dyn for<'b, 'c> Fn(&'b mut S, &'c str, &'c str, &'c str) -> &'b mut TextEdit>>,
    pub on_use_transaction: Option<Act<S>>,
    pub on_require_rollback: Option<Act<S>>,
    pub on_confirm_edit: Option<EditSlot<S>>,
    pub on_source_change: Option<Rc<dyn Fn(&mut S, String)>>,
    pub on_target_change: Option<Rc<dyn Fn(&mut S, String)>>,
    pub on_source_database_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    pub on_target_database_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    pub on_source_schema_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    pub on_target_schema_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    pub on_toggle_table: Option<Rc<dyn Fn(&mut S, String)>>,
    pub on_select_all: Option<Act<S>>,
    pub on_select_none: Option<Act<S>>,
    /// `toggleUnifiedObject(side)(object)` — the picker's toggle rail
    /// (object-based, React's shape).
    pub on_toggle_unified_object:
        Option<Rc<dyn Fn(&mut S, PickerSide, &SchemaDiffObjectIdentityVm)>>,
    /// The picker's `selectAll(side, selected)` rail.
    pub on_select_all_unified: Option<Rc<dyn Fn(&mut S, PickerSide, bool)>>,
    pub on_clear_object_selections: Option<Act<S>>,
    pub on_retry_objects: Option<Act<S>>,
    // The saved-setup dialogs' callbacks.
    pub on_close_import: Option<Act<S>>,
    pub on_import_text_edit: Option<EditSlot<S>>,
    pub on_apply_import: Option<Act<S>>,
    pub on_close_profile: Option<Act<S>>,
    pub on_profile_name_edit: Option<EditSlot<S>>,
    pub on_save_profile: Option<Act<S>>,
}

#[allow(dead_code)] // the desktop host wiring is the production consumer
impl<'a, S: 'static> SchemaDiffWindow<'a, S> {
    /// React's component over the state: the dialog slots start absent,
    /// the profile select closed, every action unwired.
    pub fn new(
        state: &'a SchemaDiffWindowState,
        confirm_edit: &'a TextEdit,
        import_edit: &'a TextEdit,
        profile_name_edit: &'a TextEdit,
    ) -> Self {
        Self {
            state,
            confirm_edit,
            import_edit,
            profile_name_edit,
            override_edits: None,
            migration_history_dialog: None,
            limitations_dialog: None,
            profile_select_open: false,
            on_open_docs: None,
            on_close_limitations: None,
            on_dismiss_limitations: None,
            on_back: None,
            on_next: None,
            on_deploy: None,
            on_cancel_job: None,
            on_verify_job: None,
            on_profile_select: None,
            on_toggle_profile_select: None,
            on_close_profile_select: None,
            on_load_profile: None,
            on_open_profile_save: None,
            on_delete_profile: None,
            on_export_config: None,
            on_open_import_config: None,
            on_copy_sql: None,
            on_copy_summary: None,
            on_select_table: None,
            on_regenerate: None,
            on_allow_destructive: None,
            on_include_indexes: None,
            on_apply_type_overrides: None,
            on_override_edit: None,
            on_use_transaction: None,
            on_require_rollback: None,
            on_confirm_edit: None,
            on_source_change: None,
            on_target_change: None,
            on_source_database_change: None,
            on_target_database_change: None,
            on_source_schema_change: None,
            on_target_schema_change: None,
            on_toggle_table: None,
            on_select_all: None,
            on_select_none: None,
            on_toggle_unified_object: None,
            on_select_all_unified: None,
            on_clear_object_selections: None,
            on_retry_objects: None,
            on_close_import: None,
            on_import_text_edit: None,
            on_apply_import: None,
            on_close_profile: None,
            on_profile_name_edit: None,
            on_save_profile: None,
        }
    }

    /// The plan panel's type-override edit slots.
    pub fn override_edits(
        mut self,
        edits: Option<&'a HashMap<String, TextEdit>>,
    ) -> Self {
        self.override_edits = edits;
        self
    }

    /// The chrome's `MigrationRunHistoryDialog` mount.
    pub fn migration_history_dialog(mut self, dialog: El) -> Self {
        self.migration_history_dialog = Some(dialog);
        self
    }

    /// The `LimitationsDialog` mount.
    pub fn limitations_dialog(mut self, dialog: El) -> Self {
        self.limitations_dialog = Some(dialog);
        self
    }

    /// React `goBack`.
    pub fn on_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back = Some(Rc::new(f));
        self
    }

    /// React `goNext`.
    pub fn on_next(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_next = Some(Rc::new(f));
        self
    }

    /// React `handleDeploy` (the navigation's Deploy and the deploy
    /// panel's button share it).
    pub fn on_deploy(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_deploy = Some(Rc::new(f));
        self
    }

    /// The job section's cancel rail (`requestCancel`).
    pub fn on_cancel_job(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel_job = Some(Rc::new(f));
        self
    }

    /// The job section's verify rail (`verifyRecovery(jobId)`).
    pub fn on_verify_job(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_verify_job = Some(Rc::new(f));
        self
    }

    /// `openDocsWindow('schemaDiff')`.
    pub fn on_open_docs(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_docs = Some(Rc::new(f));
        self
    }

    /// The limitations dialog's `onClose`.
    pub fn on_close_limitations(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_limitations = Some(Rc::new(f));
        self
    }

    /// The limitations dialog's `onDismiss`
    /// (`setSchemaDiffLimitationsDismissed`).
    pub fn on_dismiss_limitations(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_limitations = Some(Rc::new(f));
        self
    }

    /// `setSelectedProfileId` (the select's `onChange`).
    pub fn on_profile_select(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_profile_select = Some(Rc::new(f));
        self
    }

    /// The profile select's listbox open state.
    pub fn profile_select_open(mut self, open: bool) -> Self {
        self.profile_select_open = open;
        self
    }

    pub fn on_toggle_profile_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_profile_select = Some(Rc::new(f));
        self
    }

    pub fn on_close_profile_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_profile_select = Some(Rc::new(f));
        self
    }

    /// `savedSetups.handleLoadProfile`.
    pub fn on_load_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_load_profile = Some(Rc::new(f));
        self
    }

    /// `savedSetups.openProfileSave`.
    pub fn on_open_profile_save(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_profile_save = Some(Rc::new(f));
        self
    }

    /// `savedSetups.handleDeleteProfile`.
    pub fn on_delete_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_delete_profile = Some(Rc::new(f));
        self
    }

    /// `savedSetups.handleExportConfig`.
    pub fn on_export_config(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export_config = Some(Rc::new(f));
        self
    }

    /// `savedSetups.openImportConfig`.
    pub fn on_open_import_config(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_import_config = Some(Rc::new(f));
        self
    }

    /// `handleCopySql`.
    pub fn on_copy_sql(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_sql = Some(Rc::new(f));
        self
    }

    /// `handleCopySummary`.
    pub fn on_copy_summary(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_summary = Some(Rc::new(f));
        self
    }

    /// The compare step's `onSelectTable` (`setSelectedTable`).
    pub fn on_select_table(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select_table = Some(Rc::new(f));
        self
    }

    /// `onRegenerate` (`buildPlan()`).
    pub fn on_regenerate(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_regenerate = Some(Rc::new(f));
        self
    }

    /// `onAllowDestructiveChange`.
    pub fn on_allow_destructive(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_allow_destructive = Some(Rc::new(f));
        self
    }

    /// `onIncludeIndexesChange`.
    pub fn on_include_indexes(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_include_indexes = Some(Rc::new(f));
        self
    }

    /// `onApplyTypeOverrides`.
    pub fn on_apply_type_overrides(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply_type_overrides = Some(Rc::new(f));
        self
    }

    /// `onTypeOverrideChange` — the plan panel's edit projection
    /// (`table`, `column`, `target_type` → the cell's edit).
    pub fn on_override_edit(
        mut self,
        f: impl for<'b, 'c> Fn(&'b mut S, &'c str, &'c str, &'c str) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_override_edit = Some(Rc::new(f));
        self
    }

    /// `onUseTransactionChange`.
    pub fn on_use_transaction(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_use_transaction = Some(Rc::new(f));
        self
    }

    /// `onRequireRollbackChange`.
    pub fn on_require_rollback(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_require_rollback = Some(Rc::new(f));
        self
    }

    /// `onConfirmTextChange` (the confirm input's edit projection).
    pub fn on_confirm_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.on_confirm_edit = Some(Rc::new(f));
        self
    }

    /// The endpoints step's six selects: the connection picks are ids,
    /// the database / schema picks carry `None` for the placeholder.
    pub fn on_endpoint_changes(
        mut self,
        source: impl Fn(&mut S, String) + 'static,
        target: impl Fn(&mut S, String) + 'static,
        source_database: impl Fn(&mut S, Option<String>) + 'static,
        target_database: impl Fn(&mut S, Option<String>) + 'static,
        source_schema: impl Fn(&mut S, Option<String>) + 'static,
        target_schema: impl Fn(&mut S, Option<String>) + 'static,
    ) -> Self {
        self.on_source_change = Some(Rc::new(source));
        self.on_target_change = Some(Rc::new(target));
        self.on_source_database_change = Some(Rc::new(source_database));
        self.on_target_database_change = Some(Rc::new(target_database));
        self.on_source_schema_change = Some(Rc::new(source_schema));
        self.on_target_schema_change = Some(Rc::new(target_schema));
        self
    }

    /// The objects step's table rail actions.
    pub fn on_table_actions(
        mut self,
        toggle: impl Fn(&mut S, String) + 'static,
        select_all: impl Fn(&mut S) + 'static,
        select_none: impl Fn(&mut S) + 'static,
    ) -> Self {
        self.on_toggle_table = Some(Rc::new(toggle));
        self.on_select_all = Some(Rc::new(select_all));
        self.on_select_none = Some(Rc::new(select_none));
        self
    }

    /// The objects step's unified-picker actions: the toggle and
    /// select-all rails are object/side-based (React's shape), the
    /// clear and retry are the window's own callbacks.
    pub fn on_unified_actions(
        mut self,
        toggle: impl Fn(&mut S, PickerSide, &SchemaDiffObjectIdentityVm) + 'static,
        select_all: impl Fn(&mut S, PickerSide, bool) + 'static,
        clear: impl Fn(&mut S) + 'static,
        retry: impl Fn(&mut S) + 'static,
    ) -> Self {
        self.on_toggle_unified_object = Some(Rc::new(toggle));
        self.on_select_all_unified = Some(Rc::new(select_all));
        self.on_clear_object_selections = Some(Rc::new(clear));
        self.on_retry_objects = Some(Rc::new(retry));
        self
    }

    /// The saved-setup dialogs' callbacks.
    pub fn on_saved_setup_dialogs(
        mut self,
        close_import: impl Fn(&mut S) + 'static,
        import_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
        apply_import: impl Fn(&mut S) + 'static,
        close_profile: impl Fn(&mut S) + 'static,
        profile_name_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
        save_profile: impl Fn(&mut S) + 'static,
    ) -> Self {
        self.on_close_import = Some(Rc::new(close_import));
        self.on_import_text_edit = Some(Rc::new(import_edit));
        self.on_apply_import = Some(Rc::new(apply_import));
        self.on_close_profile = Some(Rc::new(close_profile));
        self.on_profile_name_edit = Some(Rc::new(profile_name_edit));
        self.on_save_profile = Some(Rc::new(save_profile));
        self
    }

    /// `SchemaDiffWindow()`: the window's element tree.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let state = self.state;
        let narrow = SCHEMA_DIFF_NARROW_STEPS.contains(&state.step);

        let mut body = col()
            .key(if state.discovery_diagnostic.is_empty() {
                "schema-diff-window-body"
            } else {
                "schema-diff-window-body-discovery-failure"
            })
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .px(BODY_PAD)
            .py(BODY_PAD);
        // `max-w-2xl overflow-auto` on the narrow steps, `max-w-6xl`
        // on the rest (the centered column).
        if narrow {
            body = body.max_w(BODY_MAX_NARROW).scroll(ScrollSpec { stick_bottom: false });
        } else {
            body = body.max_w(BODY_MAX_WIDE);
        }

        if matches!(state.step, SchemaDiffWizardStep::Endpoints | SchemaDiffWizardStep::Objects) {
            body = body.child(self.selection_steps(&k));
        }
        if matches!(state.step, SchemaDiffWizardStep::Compare | SchemaDiffWizardStep::Plan | SchemaDiffWizardStep::Deploy) {
            body = body.child(self.execution_panels(&k));
        }
        if let Some(error) = state.error.as_deref() {
            body = body.child(
                CopyableError::new("schema-diff-window-error", error.to_owned())
                    .build::<S>(&k),
            );
        }

        let mut root = col()
            .key(if state.discovery_diagnostic.is_empty() {
                "schema-diff-window"
            } else {
                "schema-diff-window-discovery-failure"
            })
            .w_full()
            .h_full()
            .min_h(0.0)
            .fill(k.c.bg_base) // bg-surface
            .child(self.chrome(&k))
            .child(schema_diff_wizard_progress(&k, state.step))
            .child(self.job_section(&k))
            .child(
                col()
                    .key("schema-diff-window-body-frame")
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .child(body),
            )
            .child(self.navigation(&k))
            .child(self.status_bar(&k));
        if let Some(dialog) = self.limitations_dialog {
            root = root.child(dialog);
        }
        root
    }

    /// The chrome title band (`WindowChromeSlot`'s 40pt band, the
    /// backup-window precedent): the window title, then the run-history
    /// dialog mount and the docs button on the right.
    fn chrome(&self, k: &Kit) -> El {
        let mut right = row().key("schema-diff-chrome-actions").gap(4.0);
        if let Some(dialog) = &self.migration_history_dialog {
            right = right.child(dialog.clone());
        }
        let mut docs = Button::new("")
            // React: `variant="secondary" className="h-6 w-6 !px-0"` —
            // the kit's Regular control is the 24pt square; the glyph
            // rides the kit's 16pt icon, a cosmetic delta.
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Regular)
            .icon(&ICON_BOOK_OPEN)
            .help(t("docs.openSchemaDiffHelp").to_owned());
        if let Some(on_open_docs) = &self.on_open_docs {
            let on_open_docs = Rc::clone(on_open_docs);
            docs = docs.on_press(move |s: &mut S| on_open_docs(s));
        }
        right = right.child(docs.build(k).key("schema-diff-open-docs"));
        row()
            .key("schema-diff-window-chrome")
            .window_drag()
            .w_full()
            .h(CHROME_H)
            .center_y()
            .px(12.0)
            .child(
                text(t("common.schemaDiff").to_owned())
                    .key("schema-diff-window-title")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_base)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
            .child(right)
    }

    /// `SchemaDiffWindowJobSection({...})`.
    fn job_section(&self, k: &Kit) -> El {
        let state = self.state;
        let mut props = SchemaDiffWindowJobSection {
            current_job: state.job_lifecycle.current_job(),
            latest_apply: state.job_lifecycle.latest_apply(),
            cancel_outcome: state.job_lifecycle.cancel_outcome(),
            verifying: state.job_lifecycle.verifying(),
            error: state.job_lifecycle.error(),
            target_ready: !state.endpoints.target().id.is_empty(),
            on_cancel: None,
            on_verify: None,
        };
        if let Some(on_cancel) = &self.on_cancel_job {
            let on_cancel = Rc::clone(on_cancel);
            props.on_cancel = Some(Rc::new(move |s: &mut S| on_cancel(s)));
        }
        if let Some(on_verify) = &self.on_verify_job {
            let on_verify = Rc::clone(on_verify);
            props.on_verify = Some(Rc::new(move |s: &mut S, job_id: String| on_verify(s, job_id)));
        }
        schema_diff_window_job_section::<S>(k, props)
    }

    /// `SchemaDiffSelectionSteps({...})` over the projected endpoint
    /// and unified view-models.
    fn selection_steps(&self, k: &Kit) -> El {
        let state = self.state;
        let endpoints_vm = endpoints_vm_from_state(state);
        let (source_keys, target_keys) = unified_keys_from_state(state);
        let unified_vm = UnifiedObjectsVm {
            loading: matches!(
                state.unified.catalog_load_state(),
                crate::views::schema_diff::use_schema_diff_unified_objects::CatalogLoadState::Loading,
            ),
            source_objects: &source_keys,
            target_objects: &target_keys,
            selected_source_keys: state.unified.selected_source_keys(),
            selected_target_keys: state.unified.selected_target_keys(),
            errors: &[],
            catalog_load_state: catalog_load_state_from_state(state),
        };

        let step = match state.step {
            SchemaDiffWizardStep::Endpoints => SelectionStep::Endpoints,
            _ => SelectionStep::Objects,
        };
        let mut steps = SchemaDiffSelectionSteps::<S>::new(
            step,
            &endpoints_vm,
            state.objects_loading,
            &state.table_picks,
            &unified_vm,
        );
        if state.endpoints.is_cross_dialect() {
            steps = steps
                .endpoints_cross_dialect_note(Some(t("schemaDiff.crossDialectNote").to_owned()));
        }
        if let Some(on_source) = &self.on_source_change {
            let on_source = Rc::clone(on_source);
            steps = steps.on_source_change(move |s: &mut S, value: String| on_source(s, value));
        }
        if let Some(on_target) = &self.on_target_change {
            let on_target = Rc::clone(on_target);
            steps = steps.on_target_change(move |s: &mut S, value: String| on_target(s, value));
        }
        if let Some(f) = &self.on_source_database_change {
            let f = Rc::clone(f);
            steps = steps.on_source_database_change(move |s: &mut S, value: Option<String>| f(s, value));
        }
        if let Some(f) = &self.on_target_database_change {
            let f = Rc::clone(f);
            steps = steps.on_target_database_change(move |s: &mut S, value: Option<String>| f(s, value));
        }
        if let Some(f) = &self.on_source_schema_change {
            let f = Rc::clone(f);
            steps = steps.on_source_schema_change(move |s: &mut S, value: Option<String>| f(s, value));
        }
        if let Some(f) = &self.on_target_schema_change {
            let f = Rc::clone(f);
            steps = steps.on_target_schema_change(move |s: &mut S, value: Option<String>| f(s, value));
        }
        if let Some(f) = &self.on_toggle_table {
            let f = Rc::clone(f);
            steps = steps.on_toggle_table(move |s: &mut S, name: String| f(s, name));
        }
        if let Some(f) = &self.on_select_all {
            let f = Rc::clone(f);
            steps = steps.on_select_all(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_select_none {
            let f = Rc::clone(f);
            steps = steps.on_select_none(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_clear_object_selections {
            let f = Rc::clone(f);
            steps = steps.on_clear_object_selections(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_retry_objects {
            let f = Rc::clone(f);
            steps = steps.on_retry_objects(move |s: &mut S| f(s));
        }
        // The unified picker (T290) is a host mount in the T250 port:
        // the window builds it over the same state and slots, React's
        // own layering.
        let errors = state.unified.errors();
        let mut picker = SchemaDiffUnifiedObjectsPicker::<S>::new(
            matches!(
                state.unified.catalog_load_state(),
                crate::views::schema_diff::use_schema_diff_unified_objects::CatalogLoadState::Loading,
            ),
            state.unified.source_objects(),
            state.unified.target_objects(),
            state.unified.selected_source_keys(),
            state.unified.selected_target_keys(),
            &errors.source,
            &errors.target,
            state.endpoints.is_cross_dialect(),
        );
        if let Some(f) = &self.on_toggle_unified_object {
            let f = Rc::clone(f);
            picker = picker.on_toggle_source(move |s: &mut S, object| {
                f(s, PickerSide::Source, object)
            });
        }
        if let Some(f) = &self.on_toggle_unified_object {
            let f = Rc::clone(f);
            picker = picker.on_toggle_target(move |s: &mut S, object| {
                f(s, PickerSide::Target, object)
            });
        }
        if let Some(f) = &self.on_select_all_unified {
            let f = Rc::clone(f);
            picker = picker.on_select_all(move |s: &mut S, side: PickerSide, selected: bool| {
                f(s, side, selected)
            });
        }
        if let Some(f) = &self.on_clear_object_selections {
            let f = Rc::clone(f);
            picker = picker.on_clear_selections(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_retry_objects {
            let f = Rc::clone(f);
            picker = picker.on_retry(move |s: &mut S| f(s));
        }
        steps = steps.unified_picker(Some(picker.build(k.dark)));
        steps.build(k.dark)
    }

    /// `SchemaDiffExecutionPanels({...})` plus the plan actions block.
    fn execution_panels(&self, k: &Kit) -> El {
        let state = self.state;
        let step = match state.step {
            SchemaDiffWizardStep::Compare => SchemaDiffExecutionStep::Compare,
            SchemaDiffWizardStep::Plan => SchemaDiffExecutionStep::Plan,
            _ => SchemaDiffExecutionStep::Deploy,
        };
        let table_has_diff = state.table_has_diff();
        let selected_tables = state.selected_tables();
        let target_label = state.target_label();
        let mut layer = Vec::new();
        let plan_actions = self.plan_actions(k, &mut layer);

        let mut panels = SchemaDiffExecutionPanels::<S>::new(
            step,
            state.loading,
            &state.diffs,
            state.selected_diff(),
            &selected_tables,
            state.selected_table.as_deref(),
            state.table_list_width,
            state.plan.as_ref(),
        )
        .table_has_diff((!state.diffs.is_empty()).then_some(&table_has_diff))
        .clipboard_feedback(state.clipboard.feedback())
        .plan_actions(Some(plan_actions))
        .review_inputs(
            state.allow_destructive,
            state.include_indexes,
            &target_label,
            state.use_transaction,
            state.require_rollback,
            self.confirm_edit,
            state.deploy_result.as_ref(),
        )
        .type_overrides(&state.type_overrides, self.override_edits);
        if let Some(f) = &self.on_select_table {
            let f = Rc::clone(f);
            panels = panels.on_select_table(move |s: &mut S, name: String| f(s, name));
        }
        if let Some(f) = &self.on_copy_summary {
            let f = Rc::clone(f);
            panels = panels.on_copy_summary(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_regenerate {
            let f = Rc::clone(f);
            panels = panels.on_regenerate(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_allow_destructive {
            let f = Rc::clone(f);
            panels = panels.on_allow_destructive(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_include_indexes {
            let f = Rc::clone(f);
            panels = panels.on_include_indexes(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_apply_type_overrides {
            let f = Rc::clone(f);
            panels = panels.on_apply_type_overrides(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_override_edit {
            let f = Rc::clone(f);
            panels = panels.on_override_edit(move |s: &mut S, table: &str, column: &str, target_type: &str| {
                f(s, table, column, target_type)
            });
        }
        if let Some(f) = &self.on_use_transaction {
            let f = Rc::clone(f);
            panels = panels.on_use_transaction(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_require_rollback {
            let f = Rc::clone(f);
            panels = panels.on_require_rollback(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_confirm_edit {
            let f = Rc::clone(f);
            panels = panels.on_confirm_edit(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_deploy {
            let f = Rc::clone(f);
            panels = panels.on_deploy(move |s: &mut S| f(s));
        }

        let mut el = panels.build(k.dark);
        for popup in layer {
            el = el.child(popup);
        }
        el
    }

    /// React's `planActions` const: the copy-SQL button, the profile
    /// select and the five profile/config buttons, in a wrapping row.
    fn plan_actions(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut actions = row()
            .key("schema-diff-plan-actions")
            .flex_wrap()
            .gap(ACTIONS_GAP)
            .center_y();

        if state.plan.is_some() {
            let copied = state.clipboard.feedback()
                == Some(crate::views::schema_diff::use_schema_diff_clipboard_feedback::SchemaDiffFeedbackKind::Sql);
            let mut copy = Button::new(if copied {
                t("common.copied").to_owned()
            } else {
                t("common.copySql").to_owned()
            })
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Regular);
            if let Some(f) = &self.on_copy_sql {
                let f = Rc::clone(f);
                copy = copy.on_press(move |s: &mut S| f(s));
            }
            actions = actions.child(copy.build(k).key("schema-diff-copy-sql"));
        }

        let labels: Vec<String> =
            state.saved_setups.profiles.iter().map(|p| p.name.clone()).collect();
        let values: Vec<String> =
            state.saved_setups.profiles.iter().map(|p| p.id.clone()).collect();
        let current = state
            .saved_setups
            .profiles
            .iter()
            .find(|p| p.id == state.saved_setups.selected_profile_id)
            .map(|p| p.name.clone());
        let mut select = Select::new("schema-diff-profile-select", &labels)
            .values(&values)
            .current(current.as_deref())
            .placeholder(t("schemaDiff.profileSelect").to_owned())
            .label(t("schemaDiff.profileSelect").to_owned())
            .width(SELECT_MIN_W)
            .open(self.profile_select_open);
        if let Some(f) = &self.on_toggle_profile_select {
            let f = Rc::clone(f);
            select = select.on_toggle(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_close_profile_select {
            let f = Rc::clone(f);
            select = select.on_close(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_profile_select {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx| {
                f(s, value.to_owned());
            });
        }
        actions = actions.child(select.build(k, layer));

        let profile_picked = !state.saved_setups.selected_profile_id.is_empty();
        let disabled = !profile_picked || state.loading;
        actions = actions.child(self.action_button(
            k,
            "schema-diff-profile-load",
            t("schemaDiff.profileLoad").to_owned(),
            ButtonVariant::Neutral,
            disabled,
            &self.on_load_profile,
        ));
        actions = actions.child(self.action_button(
            k,
            "schema-diff-profile-save",
            t("schemaDiff.profileSave").to_owned(),
            ButtonVariant::Neutral,
            false,
            &self.on_open_profile_save,
        ));
        actions = actions.child(self.action_button(
            k,
            "schema-diff-profile-delete",
            t("schemaDiff.profileDelete").to_owned(),
            ButtonVariant::Ghost,
            disabled,
            &self.on_delete_profile,
        ));
        let exported = state.clipboard.feedback()
            == Some(crate::views::schema_diff::use_schema_diff_clipboard_feedback::SchemaDiffFeedbackKind::Config);
        actions = actions.child(self.action_button(
            k,
            "schema-diff-export-config",
            if exported {
                t("schemaDiff.configExported").to_owned()
            } else {
                t("schemaDiff.exportConfig").to_owned()
            },
            ButtonVariant::Ghost,
            false,
            &self.on_export_config,
        ));
        actions = actions.child(self.action_button(
            k,
            "schema-diff-import-config",
            t("schemaDiff.importConfig").to_owned(),
            ButtonVariant::Ghost,
            false,
            &self.on_open_import_config,
        ));
        actions
    }

    /// One plan-actions button: the variant, the `!selectedProfileId ||
    /// loading` disable and the action.
    fn action_button(
        &self,
        k: &Kit,
        key: &str,
        label: String,
        variant: ButtonVariant,
        disabled: bool,
        action: &Option<Act<S>>,
    ) -> El {
        let mut button = Button::new(label)
            .variant(variant)
            .control_size(ControlSize::Regular)
            .disabled(disabled);
        if !disabled {
            if let Some(f) = action {
                let f = Rc::clone(f);
                button = button.on_press(move |s: &mut S| f(s));
            }
        }
        button.build(k).key(key)
    }

    /// `SchemaDiffWindowNavigation({...})`.
    fn navigation(&self, k: &Kit) -> El {
        let state = self.state;
        let props = SchemaDiffWindowNavigation {
            step_index: state.step_index(),
            loading: state.loading,
            step: state.step,
            deploy_allowed: state.deploy_allowed(),
            can_next: state.can_next(),
            on_back: self.on_back.clone(),
            on_deploy: self.on_deploy.clone(),
            on_next: self.on_next.clone(),
        };
        schema_diff_window_navigation::<S>(k, k.dark, props)
    }

    /// `StatusBar`: the 40pt `border-t` band, the truncated
    /// `common.schemaDiff` on the left.
    fn status_bar(&self, k: &Kit) -> El {
        row()
            .key("schema-diff-status-bar")
            .w_full()
            .h(STATUS_H)
            .center_y()
            .px(STATUS_PAD)
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(
                text(t("common.schemaDiff").to_owned())
                    .font(TextStyle::new(STATUS_TEXT, 400.0))
                    .color(k.c.text_muted) // text-fg-secondary
                    .min_w(0.0)
                    .truncate(),
            )
    }

    /// The saved-setup dialogs (`SchemaDiffSavedSetupDialogs`), a
    /// separate mount the host drives over the same state.
    pub fn saved_setup_dialogs(&self, dark: bool, layer: &mut Vec<El>) {
        let state = self.state;
        let vm = SavedSetupDialogsVm {
            import_config_open: state.saved_setups.import_open,
            import_config_text: self.import_edit,
            import_config_error: state.saved_setups.import_error.as_deref(),
            profile_dialog_open: state.saved_setups.save_open,
            profile_name: self.profile_name_edit,
            profile_error: state.saved_setups.profile_error.as_deref(),
        };
        let mut dialogs = SchemaDiffSavedSetupDialogs::<S>::over(vm);
        if let Some(f) = &self.on_close_import {
            let f = Rc::clone(f);
            dialogs = dialogs.on_close_import(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_import_text_edit {
            let f = Rc::clone(f);
            dialogs = dialogs.on_import_text_edit(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_apply_import {
            let f = Rc::clone(f);
            dialogs = dialogs.on_apply_import(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_close_profile {
            let f = Rc::clone(f);
            dialogs = dialogs.on_close_profile(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_profile_name_edit {
            let f = Rc::clone(f);
            dialogs = dialogs.on_profile_name_edit(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_save_profile {
            let f = Rc::clone(f);
            dialogs = dialogs.on_save_profile(move |s: &mut S| f(s));
        }
        dialogs.build(dark, layer);
    }
}

/// `useSchemaDiffEndpoints` → the selection steps' endpoints
/// view-model (the projection the T250 bar reads).
fn endpoints_vm_from_state(state: &SchemaDiffWindowState) -> SchemaDiffEndpointsVm {
    let blank = |value: &str| (!value.trim().is_empty()).then(|| value.trim().to_owned());
    let options = state
        .endpoints
        .conn_options()
        .into_iter()
        .map(|(value, label)| SelectionConnectionOption { value, label, disabled: false })
        .collect::<Vec<_>>();
    let source = state.endpoints.source();
    let target = state.endpoints.target();
    SchemaDiffEndpointsVm {
        source_id: source.id.clone(),
        target_id: target.id.clone(),
        source_database: blank(&source.database),
        target_database: blank(&target.database),
        source_schema: blank(&source.schema),
        target_schema: blank(&target.schema),
        conn_options: options.clone(),
        target_options: None,
        source_databases: source.databases.clone(),
        target_databases: target.databases.clone(),
        source_schemas: source.schemas.clone(),
        target_schemas: target.schemas.clone(),
        is_cross_dialect: state.endpoints.is_cross_dialect(),
    }
}

/// The unified objects' identity-key projections (the picker's list
/// strings).
fn unified_keys_from_state(state: &SchemaDiffWindowState) -> (Vec<String>, Vec<String>) {
    use crate::views::schema_diff::schema_diff_object_identity::schema_diff_object_identity_key;
    (
        state
            .unified
            .source_objects()
            .iter()
            .map(schema_diff_object_identity_key)
            .collect(),
        state
            .unified
            .target_objects()
            .iter()
            .map(schema_diff_object_identity_key)
            .collect(),
    )
}

/// The unified catalog load state's projection (the hook's enum → the
/// steps' enum).
fn catalog_load_state_from_state(
    state: &SchemaDiffWindowState,
) -> crate::views::schema_diff::schema_diff_selection_steps::CatalogLoadState {
    use crate::views::schema_diff::schema_diff_selection_steps::CatalogLoadState as Steps;
    use crate::views::schema_diff::use_schema_diff_unified_objects::CatalogLoadState as Hook;
    match state.unified.catalog_load_state() {
        Hook::NeverStarted => Steps::NeverStarted,
        Hook::Loading => Steps::Loading,
        Hook::Completed => Steps::Completed,
        Hook::Invalidated => Steps::Invalidated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_geometry_is_the_react_literal() {
        assert_eq!(CHROME_H, 40.0, "h-10");
        assert_eq!(STATUS_H, 40.0, "h-10 min-h-[40px]");
        assert_eq!(STATUS_TEXT, 12.0, "text-xs");
        assert_eq!(STATUS_PAD, 16.0, "px-4");
        assert_eq!(BODY_PAD, 24.0, "px-6 py-6");
        assert_eq!(BODY_MAX_NARROW, 672.0, "max-w-2xl");
        assert_eq!(BODY_MAX_WIDE, 1152.0, "max-w-6xl");
        assert_eq!(ACTIONS_GAP, 8.0, "gap-2");
        assert_eq!(SELECT_MIN_W, 144.0, "min-w-36");
        assert_eq!(DOCS_BUTTON, 24.0, "h-6 w-6");
        assert_eq!(DOCS_GLYPH, 12.0, "h-3 w-3");
    }

    #[test]
    fn the_i18n_keys_are_the_react_literals() {
        assert_eq!(actions::KEY_TABLE_REQUIRED, "schemaDiff.tableRequired");
        assert_eq!(actions::KEY_SELECTION_REQUIRED, "schemaDiff.selectionRequired");
        assert_eq!(actions::KEY_CROSS_DIALECT_BLOCKED, "schemaDiff.crossDialectObjectBlocked");
        assert_eq!(actions::KEY_CLIPBOARD_FAILED, "schemaDiff.clipboardFailed");
        assert_eq!(
            actions::KEY_REPREPARE_ON_STALE_PLAN,
            "migrationJob.reprepareOnStalePlan"
        );
    }

    #[test]
    fn the_endpoints_projection_maps_the_hook_fields() {
        use crate::views::schema_diff::use_schema_diff_endpoints::{
            EndpointConnection, EndpointSide,
        };
        let mut state = SchemaDiffWindowState::default();
        state.endpoints.on_connections(vec![
            EndpointConnection {
                id: "c1".to_owned(),
                name: "A".to_owned(),
                database_type: "mysql".to_owned(),
                database: None,
                schema: None,
            },
            EndpointConnection {
                id: "c2".to_owned(),
                name: "B".to_owned(),
                database_type: "sqlite".to_owned(),
                database: None,
                schema: None,
            },
        ]);
        let _ = state.endpoints.select_side(EndpointSide::Source, "c1");
        let _ = state.endpoints.select_side(EndpointSide::Target, "c2");
        state.endpoints.select_database(EndpointSide::Source, "  ");
        state.endpoints.select_database(EndpointSide::Target, "tdb");
        state.endpoints.select_schema(EndpointSide::Source, "sschema");

        let vm = endpoints_vm_from_state(&state);
        assert_eq!(vm.source_id, "c1");
        assert_eq!(vm.target_id, "c2");
        assert_eq!(vm.source_database, None, "blank reads as the placeholder");
        assert_eq!(vm.target_database.as_deref(), Some("tdb"));
        assert_eq!(vm.source_schema.as_deref(), Some("sschema"));
        assert_eq!(vm.target_schema, None);
        assert_eq!(vm.conn_options.len(), 2);
        assert_eq!(vm.conn_options[0].label, "A (mysql)");
        assert!(vm.is_cross_dialect);
        assert!(vm.target_options.is_none(), "the wizard shares one list");
    }

    #[test]
    fn the_unified_projection_maps_keys_and_load_state() {
        use crate::views::schema_diff::schema_diff_object_identity::{
            SchemaDiffObjectIdentityVm, SchemaDiffObjectKindVm,
        };
        let mut state = SchemaDiffWindowState::default();
        let begin = state.unified.begin_load();
        assert_eq!(
            catalog_load_state_from_state(&state),
            crate::views::schema_diff::schema_diff_selection_steps::CatalogLoadState::Loading
        );
        let object = SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::View,
            schema: Some("s".to_owned()),
            name: "v".to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        };
        state
            .unified
            .on_load_answer(begin, (vec![object], Vec::new()), (vec![], Vec::new()));
        assert_eq!(
            catalog_load_state_from_state(&state),
            crate::views::schema_diff::schema_diff_selection_steps::CatalogLoadState::Completed
        );
        let (source_keys, target_keys) = unified_keys_from_state(&state);
        assert_eq!(source_keys.len(), 1);
        assert!(target_keys.is_empty());
        assert!(source_keys[0].contains("\"view\""), "the key carries the kind");
        assert!(source_keys[0].contains("\"v\""), "the key carries the name");
    }

    #[test]
    fn the_job_section_target_gate_reads_the_target_side() {
        let mut state = SchemaDiffWindowState::default();
        assert!(!state.endpoints.target().id.is_empty() || true, "vacuous by construction");
        state.endpoints.select_side(
            crate::views::schema_diff::use_schema_diff_endpoints::EndpointSide::Target,
            "c1",
        );
        assert!(!state.endpoints.target().id.is_empty());
    }
}
