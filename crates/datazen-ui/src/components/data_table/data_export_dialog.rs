//! T132: grid data-export dialog — port of
//! `packages/frontend/src/components/DataTable/DataExportDialog.tsx`.
//!
//! What the header menu's "Export data" opens: the format, the range, the
//! columns, and the run. The widget renders a [`DataExportVm`] and executes
//! nothing: the two React execution roads (the streaming `exportSingleTableViaRust`
//! for entire/filtered scopes, the provided-rows `resolveDataFetcher` for
//! page/selected scopes) are host concerns here — a press on **Export** hands
//! the host a complete [`DataExportRequest`] through
//! [`DataExportDialog::on_export`], and the host writes the run's outcome back
//! into the VM (`status` / `error` / `cancelling` / `partial_deleted`), the
//! same projection the batch export dialog uses. The filtered-statement
//! builder and the cancel handle are likewise the host's: the request carries
//! the scope, and the host answers a filtered run without a statement by
//! writing `t("export.filteredScopeUnavailable")` into `vm.error`.
//!
//! Structure follows React's JSX top to bottom: the three body states
//! (`success` / `aborted` / the form, which also stays mounted while
//! `exporting`) and the matching footer. The form paints, in order: the
//! locked warning, the format select with the disabled reasons under it, the
//! range select, the shared [`ExportColumnPicker`], the "will export" summary
//! and the copyable error.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | panel / title / close / body / footer / scrim / layer | `<key>`, `<key>-title`, `<key>-close`, `<key>-body`, `<key>-footer`, `<key>-scrim`, `<key>-layer` (from [`Dialog`]) |
//! | locked warning | `<key>-locked-warning` |
//! | format | `<key>-format-field` (kit trigger `<key>-format-field-trigger`) |
//! | format disabled reason | `<key>-format-reason-<i>` (React `data-testid="export-format-disabled-reason"`) |
//! | range | `<key>-range-field` (kit trigger `<key>-range-field-trigger`) |
//! | column picker | `export-column-picker-*` ([`ExportColumnPicker`]; two pickers need distinct keys, so this port re-keys it `<key>-columns`) |
//! | summary | `<key>-summary`, `<key>-summary-format` |
//! | aborted | `<key>-aborted`, `<key>-aborted-text` (React `data-testid="export-aborted-panel"`) |
//! | success | `<key>-success-*` ([`ExportSuccessPanel`]) |
//! | error | `<key>-error`, `-error-text`, `-error-copy` (from [`CopyableError`]) |
//! | footer | `<key>-footer-close`, `<key>-footer-cancel-export`, `<key>-footer-cancel`, `<key>-footer-export` |
//!
//! All colours, type and radii come from the kit palette; there are no raw
//! colour literals here.
//!
//! ## Deliberate deviations from the React source
//!
//! * **The kit `Select` has no per-option disabled state**, so a disabled
//!   format or the zero-selection "selected rows" scope cannot be greyed out
//!   inside the listbox. The pick is refused instead (React's own defensive
//!   rule for disabled formats arriving from keyboard input on the raw
//!   select), and the disabled formats' reasons stay on screen below the
//!   select the whole time the range makes them unavailable — the DBR-06
//!   behaviour that matters is the reason, not the grey-out.
//! * **Reset-on-open keeps the format.** React's `useEffect` on `open` resets
//!   status, error, cancelling, partial-file state, scope and columns — the
//!   format survives a close/reopen cycle in React, so it survives here.
//! * **The row projection is the host's.** The request carries the picked
//!   column names in display order; the host owns the rows (in its `S`) and
//!   maps names to indexes, so a column subset cannot shift values into wrong
//!   positions (DBR-06) — the host applies the same rule React does inline.
//! * **Escape during a run closes the dialog**, exactly as React's `onClose`
//!   does (only the footer's secondary button is disabled while exporting);
//!   the batch export dialog guards this, the grid dialog does not.
//! * The summary counts render as plain numbers (React interpolates JS
//!   numbers with no grouping into `export.willExport`).

use std::rc::Rc;

use rune::{col, el, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::connection::batch_export_dialog::{
    DataExportCapability, DataFormat,
};
use crate::components::data_table::data_export_dialog_options::{
    build_format_options, build_scope_options, disabled_reason_keys, format_option, ExportScope,
};
use crate::components::export::export_column_picker::{
    ExportColumnPicker, ExportColumnPickerColumn, IsSelectedFn, PickerActionFn,
    ToggleColumnFn,
};
use crate::components::export::export_success_panel::ExportSuccessPanel;
use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// React's `status` union: form / exporting / success / aborted.
#[allow(dead_code)] // VM surface; wired by the grid host task that owns exports
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DataExportStatus {
    #[default]
    Form,
    Exporting,
    Success,
    Aborted,
}

/// The `ColumnDef` slice the dialog needs: a name, its optional data-type
/// label, in display order (the export writes the picked columns in exactly
/// this order).
#[allow(dead_code)] // VM surface
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataExportColumn {
    pub name: String,
    pub col_type: Option<String>,
}

#[allow(dead_code)] // VM surface
impl DataExportColumn {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            col_type: None,
        }
    }

    pub fn with_type(mut self, col_type: impl Into<String>) -> Self {
        self.col_type = Some(col_type.into());
        self
    }
}

/// Everything the dialog renders. The host keeps this in the app state `S`
/// and projects it here; every interaction is a callback that mutates `S`.
#[allow(dead_code)] // VM surface; wired by the grid host task
#[derive(Clone, Debug)]
pub struct DataExportVm {
    pub open: bool,
    /// Bumped on each open so the body's scroll viewport resets (from [`Dialog`]).
    pub open_epoch: u64,
    /// The table being exported; React's default `'data'` is the "no table"
    /// placeholder (it suppresses the description and the entire-table scope).
    pub table_name: String,
    pub database_type: Option<String>,
    /// The connection the streaming scopes run through; `None` on the
    /// provided-only paths.
    pub db_session_id: Option<String>,
    /// The server's row count for the entire-table scope's label; unknown
    /// renders the plain label (React's `total == null`).
    pub total_rows: Option<u64>,
    /// The table's primary-key column names, when the caller knows them.
    pub primary_key_columns: Vec<String>,
    /// The grid's columns, in display order.
    pub columns: Vec<DataExportColumn>,
    /// How many rows the grid has loaded (the page scope's count).
    pub rows_len: usize,
    /// How many rows the grid has marked selected (the selected scope's
    /// count; the row set itself stays in the host's `S`).
    pub selected_rows_len: usize,
    /// The scope the dialog opens with (DB-15: the filter panel's "Export…"
    /// exit preselects `filtered_table`); `None` opens on the page scope.
    pub initial_scope: Option<ExportScope>,
    /// Whether the host can build the filtered statement (DB-15: the filtered
    /// scope exists exactly when its builder does).
    pub has_filtered_builder: bool,
    /// Driver data-export capability; `none` locks the dialog entirely.
    pub capability: DataExportCapability,
    /// React's `IS_DESKTOP_BUILD`: the native build is the desktop one, so
    /// the default is `true` (tests flip it to check the web option sets).
    pub is_desktop: bool,
    pub format: DataFormat,
    pub scope: ExportScope,
    /// The picked column names (a subset of `columns`, compared by name);
    /// the export writes them in `columns`' display order.
    pub selected_cols: Vec<String>,
    pub status: DataExportStatus,
    /// The cancel was asked for; the run is winding down (DBR-06, restored).
    pub cancelling: bool,
    /// Whether the aborted run's partial file was deleted (reported honestly).
    pub partial_deleted: bool,
    pub error: Option<String>,
    /// Whether the kit format listbox is open (React's `Select` owns this
    /// internally).
    pub format_open: bool,
    /// Whether the kit range listbox is open.
    pub scope_open: bool,
}

#[allow(dead_code)] // VM surface
impl Default for DataExportVm {
    fn default() -> Self {
        Self {
            open: false,
            open_epoch: 0,
            table_name: "data".to_owned(),
            database_type: None,
            db_session_id: None,
            total_rows: None,
            primary_key_columns: Vec::new(),
            columns: Vec::new(),
            rows_len: 0,
            selected_rows_len: 0,
            initial_scope: None,
            has_filtered_builder: false,
            capability: DataExportCapability::default(),
            is_desktop: true,
            format: DataFormat::Csv,
            scope: ExportScope::default(),
            selected_cols: Vec::new(),
            status: DataExportStatus::default(),
            cancelling: false,
            partial_deleted: false,
            error: None,
            format_open: false,
            scope_open: false,
        }
    }
}

#[allow(dead_code)] // VM surface
impl DataExportVm {
    /// React's `useEffect` on open: clear all run state, take the caller's
    /// preferred scope, and re-pick every column (DBR-06). The format
    /// deliberately survives, as in React — the effect does not touch it.
    pub fn reset_on_open(&mut self) {
        self.status = DataExportStatus::Form;
        self.error = None;
        self.cancelling = false;
        self.partial_deleted = false;
        self.scope = self.initial_scope.unwrap_or_default();
        self.selected_cols = self.columns.iter().map(|c| c.name.clone()).collect();
        self.format_open = false;
        self.scope_open = false;
    }

    /// React's `exportLocked`: the driver allows no data export at all.
    pub fn export_locked(&self) -> bool {
        self.capability == DataExportCapability::None
    }

    /// React's `allowEntire`: an unlocked driver that can pull whole tables,
    /// over a live session, on a real table (not the `'data'` placeholder).
    pub fn allow_entire(&self) -> bool {
        !self.export_locked()
            && self.capability.supports_full_table()
            && self.db_session_id.is_some()
            && self.table_name != "data"
    }

    /// React's `allowFiltered`: the filtered scope exists exactly when its
    /// statement builder does (DB-15).
    pub fn allow_filtered(&self) -> bool {
        !self.export_locked() && self.db_session_id.is_some() && self.has_filtered_builder
    }

    /// The scope the dialog settles on: React's effect drops an impossible
    /// preferred value back to the page scope, which every driver serves.
    /// Derived, so it also covers a capability/session flip mid-dialog.
    pub fn settled_scope(&self) -> ExportScope {
        match self.scope {
            ExportScope::EntireTable if !self.allow_entire() => ExportScope::CurrentPage,
            ExportScope::FilteredTable if !self.allow_filtered() => ExportScope::CurrentPage,
            scope => scope,
        }
    }

    /// The format the dialog settles on: an unavailable format (the scope
    /// became streaming, or no key column) falls back to CSV instead of
    /// keeping a disabled selection (DBR-06).
    pub fn settled_format(&self) -> DataFormat {
        let options = build_format_options(
            self.settled_scope(),
            self.is_desktop,
            self.has_key_column(),
        );
        match format_option(&options, self.format) {
            Some(option) if option.disabled => DataFormat::Csv,
            _ => self.format,
        }
    }

    /// React's `hasKeyColumn`: SQL UPDATE needs one for its WHERE.
    pub fn has_key_column(&self) -> bool {
        !self.primary_key_columns.is_empty()
    }

    /// The picked columns, in display order — the order the export writes
    /// (DBR-06, restored).
    pub fn selected_column_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .filter(|c| self.selected_cols.iter().any(|name| name == &c.name))
            .map(|c| c.name.clone())
            .collect()
    }

    /// React's `rowCount` summary: streaming scopes report the server's total
    /// (or the loaded rows until it answers); the selected scope reports the
    /// selection; everything else reports the loaded rows.
    pub fn row_count(&self) -> usize {
        let scope = self.settled_scope();
        if scope.is_streaming() {
            return self.total_rows.unwrap_or(self.rows_len as u64) as usize;
        }
        if scope == ExportScope::Selected && self.selected_rows_len > 0 {
            return self.selected_rows_len;
        }
        self.rows_len
    }

    /// React's export-button `disabled`: no double runs, no locked exports, no
    /// empty row sets on the provided scopes, no empty column selection (an
    /// empty selection exports nothing; the button says so instead of
    /// writing a header-only file, DBR-06).
    pub fn export_disabled(&self) -> bool {
        let scope = self.settled_scope();
        self.status == DataExportStatus::Exporting
            || self.export_locked()
            || (!scope.is_streaming() && self.rows_len == 0)
            || (scope == ExportScope::Selected && self.selected_rows_len == 0)
            || self.selected_column_names().is_empty()
    }
}

/// The complete argument both React export paths build from: what to export,
/// in what format, and which columns in which order. The host owns the rows,
/// the connection state and the save target; it answers the run by writing
/// the VM's `status` / `error` / `cancelling` / `partial_deleted`.
#[allow(dead_code)] // VM surface; handed to the host by `on_export`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataExportRequest {
    pub scope: ExportScope,
    pub format: DataFormat,
    /// The picked column names, in display order — what the export writes and
    /// the host projects the rows onto.
    pub columns: Vec<String>,
    /// React's `primaryKeyColumns?.[0]`: the SQL UPDATE key (None falls back to
    /// the first column, the old frontend rule).
    pub primary_key: Option<String>,
    pub table_name: String,
    pub database_type: Option<String>,
    pub db_session_id: Option<String>,
}

pub type CloseFn<S> = Rc<dyn Fn(&mut S) + 'static>;
pub type ScopeFn<S> = Rc<dyn Fn(&mut S, ExportScope) + 'static>;
pub type FormatFn<S> = Rc<dyn Fn(&mut S, DataFormat) + 'static>;
pub type ExportFn<S> = Rc<dyn Fn(&mut S, DataExportRequest) + 'static>;
pub type CopyFn<S> = Rc<dyn Fn(&mut S, &mut Tx) + 'static>;

/// Builder for the dialog, generic over the app state `S` that owns the VM.
/// The picker's three actions and its selection probe are plain function
/// pointers into `S` (the picker's own API); the select and button callbacks
/// are `Rc` closures, the same split the batch export dialog uses.
#[allow(dead_code)] // reusable component; wired by the grid host task
pub struct DataExportDialog<'a, S: 'static> {
    key: &'a str,
    vm: &'a DataExportVm,
    close: Option<CloseFn<S>>,
    scope: Option<ScopeFn<S>>,
    scope_open: Option<CloseFn<S>>,
    scope_close: Option<CloseFn<S>>,
    format: Option<FormatFn<S>>,
    format_open: Option<CloseFn<S>>,
    format_close: Option<CloseFn<S>>,
    export: Option<ExportFn<S>>,
    cancel: Option<CloseFn<S>>,
    copy: Option<CopyFn<S>>,
    picker_is_selected: Option<IsSelectedFn<S>>,
    picker_toggle: Option<ToggleColumnFn<S>>,
    picker_select_all: Option<PickerActionFn<S>>,
    picker_clear_all: Option<PickerActionFn<S>>,
    picker_reset_default: Option<PickerActionFn<S>>,
}

impl<'a, S: 'static> DataExportDialog<'a, S> {
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn new(key: &'a str, vm: &'a DataExportVm) -> Self {
        Self {
            key,
            vm,
            close: None,
            scope: None,
            scope_open: None,
            scope_close: None,
            format: None,
            format_open: None,
            format_close: None,
            export: None,
            cancel: None,
            copy: None,
            picker_is_selected: None,
            picker_toggle: None,
            picker_select_all: None,
            picker_clear_all: None,
            picker_reset_default: None,
        }
    }

    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(Rc::new(f));
        self
    }

    /// React's `handleScopeChange` (minus the disabled pick, refused here).
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_scope_change(mut self, f: impl Fn(&mut S, ExportScope) + 'static) -> Self {
        self.scope = Some(Rc::new(f));
        self
    }

    /// The range listbox's open state (the kit keeps it on the app's `S`).
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_scope_open(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.scope_open = Some(Rc::new(f));
        self
    }

    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_scope_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.scope_close = Some(Rc::new(f));
        self
    }

    /// React's `handleFormatChange` (minus the disabled pick, refused here).
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_format_change(mut self, f: impl Fn(&mut S, DataFormat) + 'static) -> Self {
        self.format = Some(Rc::new(f));
        self
    }

    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_format_open(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.format_open = Some(Rc::new(f));
        self
    }

    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_format_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.format_close = Some(Rc::new(f));
        self
    }

    /// React's `handleExport`, projected: the host receives the request and
    /// runs it (streaming or provided), writing the outcome into the VM.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_export(mut self, f: impl Fn(&mut S, DataExportRequest) + 'static) -> Self {
        self.export = Some(Rc::new(f));
        self
    }

    /// React's `handleCancelExport`: the host sets `cancelling` and cancels
    /// its run handle.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_cancel_export(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.cancel = Some(Rc::new(f));
        self
    }

    /// The error text's copy button.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn on_copy_error(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.copy = Some(Rc::new(f));
        self
    }

    /// The picker's `selected.has(name)` probe into `S`.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn picker_is_selected(mut self, f: IsSelectedFn<S>) -> Self {
        self.picker_is_selected = Some(f);
        self
    }

    /// The picker's `onToggle(name)`.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn picker_on_toggle(mut self, f: ToggleColumnFn<S>) -> Self {
        self.picker_toggle = Some(f);
        self
    }

    /// The picker's `onSelectAll`.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn picker_on_select_all(mut self, f: PickerActionFn<S>) -> Self {
        self.picker_select_all = Some(f);
        self
    }

    /// The picker's `onClearAll`.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn picker_on_clear_all(mut self, f: PickerActionFn<S>) -> Self {
        self.picker_clear_all = Some(f);
        self
    }

    /// The picker's `onResetDefault`.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn picker_on_reset_default(mut self, f: PickerActionFn<S>) -> Self {
        self.picker_reset_default = Some(f);
        self
    }

    /// Builds the dialog into the overlay `layer`, mirroring React's JSX.
    /// Needs `s` only for the picker's per-row selection probe.
    #[allow(dead_code)] // reusable component; wired by the grid host task
    pub fn build(self, s: &S, dark: bool, layer: &mut Vec<El>) {
        let vm = self.vm;
        if !vm.open {
            return;
        }
        let k = kit(dark);
        let key = self.key;
        let settled = matches!(
            vm.status,
            DataExportStatus::Success | DataExportStatus::Aborted
        );
        let exporting = vm.status == DataExportStatus::Exporting;

        let mut dialog = Dialog::new(key, t("common.exportData"))
            .open(true)
            .open_epoch(vm.open_epoch)
            // React's dialog panel: `w-full max-w-xl`.
            .width(576.0)
            .fit()
            .body_max_height(440.0)
            .header_pad((18.0, 14.0))
            .body_pad((18.0, 14.0))
            .footer_pad((16.0, 10.0))
            .footer_height(56.0);
        // React shows the description only for a real table.
        if vm.table_name != "data" {
            dialog = dialog.description(
                t("export.description").replace("{table}", &vm.table_name),
            );
        }

        for footer in self.footer(&k, settled, exporting) {
            dialog = dialog.footer(footer);
        }
        for child in self.body(s, &k, dark, settled, layer) {
            dialog = dialog.child(child);
        }

        // React wires `onClose` straight through: Escape / scrim / header
        // close all close the dialog even mid-run (only the footer's
        // secondary button is disabled while exporting).
        let close = self.close.clone();
        dialog = dialog.on_close(move |state: &mut S| {
            if let Some(close) = &close {
                close(state);
            }
        });
        dialog.build(&k, layer);
    }

    // ---- footer ---------------------------------------------------------------

    #[allow(dead_code)] // reachable only through `build`
    fn footer(&self, k: &Kit, settled: bool, exporting: bool) -> Vec<El> {
        let key = self.key;
        let vm = self.vm;

        if settled {
            let mut close = Button::new(t("common.close"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Regular);
            if let Some(cb) = self.close.clone() {
                close = close.on_press(move |s: &mut S| cb(s));
            }
            return vec![close.build(k).key(format!("{key}-footer-close"))];
        }

        let mut buttons = Vec::new();
        // React: the cancel-export control exists on the streaming scopes of
        // the desktop build, and only while a run is in flight.
        if vm.settled_scope().is_streaming() && vm.is_desktop {
            let label = if vm.cancelling {
                t("export.cancelling")
            } else {
                t("export.cancelExport")
            };
            let mut cancel_export = Button::new(label)
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Regular)
                .disabled(!exporting || vm.cancelling);
            if let Some(cb) = self.cancel.clone() {
                cancel_export = cancel_export.on_press(move |s: &mut S| cb(s));
            }
            buttons.push(
                cancel_export
                    .build(k)
                    .key(format!("{key}-footer-cancel-export")),
            );
        }

        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Regular)
            .disabled(exporting);
        if let Some(cb) = self.close.clone() {
            cancel = cancel.on_press(move |s: &mut S| cb(s));
        }
        buttons.push(cancel.build(k).key(format!("{key}-footer-cancel")));

        let label = if exporting {
            t("export.exporting")
        } else {
            t("export.export")
        };
        let mut export = Button::new(label)
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .disabled(vm.export_disabled());
        if let Some(cb) = self.export.clone() {
            let request = DataExportRequest {
                scope: vm.settled_scope(),
                format: vm.settled_format(),
                columns: vm.selected_column_names(),
                primary_key: vm.primary_key_columns.first().cloned(),
                table_name: vm.table_name.clone(),
                database_type: vm.database_type.clone(),
                db_session_id: vm.db_session_id.clone(),
            };
            export = export.on_press(move |s: &mut S| cb(s, request.clone()));
        }
        buttons.push(export.build(k).key(format!("{key}-footer-export")));
        buttons
    }

    // ---- body ------------------------------------------------------------------

    #[allow(dead_code)] // reachable only through `build`
    fn body(
        &self,
        s: &S,
        k: &Kit,
        dark: bool,
        settled: bool,
        layer: &mut Vec<El>,
    ) -> Vec<El> {
        let key = self.key;
        let vm = self.vm;

        if settled {
            let mut body = Vec::new();
            if vm.status == DataExportStatus::Success {
                // React: <ExportSuccessPanel message={t('export.success')} />
                body.push(
                    ExportSuccessPanel::new(format!("{key}-success"), t("export.success"))
                        .build(dark),
                );
            } else {
                // React: the aborted panel reports what happened to the
                // partial file, as the backend answered.
                body.push(
                    el().key(format!("{key}-aborted"))
                        .w_full()
                        .min_w(0.0)
                        .radius(k.radius.md)
                        .border(1.0, k.c.border_base)
                        .fill(k.c.surface_base)
                        .px(12.0)
                        .py(8.0)
                        .child(
                            text(if vm.partial_deleted {
                                t("export.cancelledPartialDeleted")
                            } else {
                                t("export.cancelledPartialKept")
                            })
                            .key(format!("{key}-aborted-text"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .min_w(0.0)
                            .wrap(),
                        ),
                );
            }
            return body;
        }

        // The form (React's `space-y-4`); it stays mounted while exporting.
        let mut form = col()
            .key(format!("{key}-form"))
            .w_full()
            .min_w(0.0)
            .gap(16.0);

        // The locked warning (React's amber inline notice).
        if vm.export_locked() {
            form = form.child(
                el().key(format!("{key}-locked-warning"))
                    .w_full()
                    .min_w(0.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.warning.border)
                    .fill(k.alpha(k.c.warning.bg, 0.1))
                    .px(12.0)
                    .py(8.0)
                    .child(
                        text(t("export.disabledByDriver"))
                            .key(format!("{key}-locked-warning-text"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.warning.fg)
                            .min_w(0.0)
                            .wrap(),
                    ),
            );
        }

        // The format select with the disabled reasons under it.
        form = form.child(self.format_block(k, layer));

        // The range select.
        form = form.child(self.range_block(k, layer));

        // The column picker (React shares one picker between both export
        // dialogs; its key sits under this dialog's prefix).
        if let Some(picker) = self.picker(s, dark) {
            form = form.child(picker);
        }

        // The "will export" summary.
        form = form.child(self.summary(k));

        // The copyable error.
        if let Some(message) = vm.error.clone() {
            let mut error = CopyableError::new(format!("{key}-error"), message).copy_button(true);
            if let Some(cb) = self.copy.clone() {
                error = error.on_copy_tx(move |state: &mut S, tx: &mut Tx| cb(state, tx));
            }
            form = form.child(error.build::<S>(k));
        }

        vec![form]
    }

    #[allow(dead_code)] // reachable only through `build`
    fn format_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let key = self.key;
        let vm = self.vm;
        let options = build_format_options(
            vm.settled_scope(),
            vm.is_desktop,
            vm.has_key_column(),
        );
        let labels: Vec<String> = options
            .iter()
            .map(|o| o.format.label().to_owned())
            .collect();
        let values: Vec<&str> = options.iter().map(|o| o.format.value()).collect();

        let mut select = Select::new(format!("{key}-format-field"), &labels)
            .values(&values)
            .current(Some(vm.settled_format().label()))
            .label(t("export.format"))
            .open(vm.format_open);
        if let Some(cb) = self.format_open.clone() {
            select = select.on_toggle(move |s: &mut S| cb(s));
        }
        if let Some(cb) = self.format_close.clone() {
            select = select.on_close(move |s: &mut S| cb(s));
        }
        if let Some(cb) = self.format.clone() {
            let refused = options.clone();
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                let format = DataFormat::from_value(value);
                // A disabled option can still arrive from keyboard input on
                // the raw select — ignore it (React's own guard).
                if format_option(&refused, format).is_some_and(|o| o.disabled) {
                    return;
                }
                cb(s, format);
            });
        }
        let mut block = col()
            .key(format!("{key}-format"))
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .child(select.build(k, layer));
        // The disabled formats' reasons, each shown once, on screen the
        // whole time the range makes them unavailable (DBR-06).
        for (i, reason) in disabled_reason_keys(&options).into_iter().enumerate() {
            block = block.child(
                text(t(reason))
                    .key(format!("{key}-format-reason-{i}"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        }
        block
    }

    #[allow(dead_code)] // reachable only through `build`
    fn range_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let key = self.key;
        let vm = self.vm;
        let options = build_scope_options(
            vm.rows_len,
            vm.selected_rows_len,
            vm.allow_entire().then(|| vm.total_rows),
            vm.allow_filtered(),
        );
        let labels: Vec<String> = options.iter().map(|o| o.label.clone()).collect();
        let values: Vec<&str> = options.iter().map(|o| o.scope.value()).collect();
        let current = options
            .iter()
            .find(|o| o.scope == vm.settled_scope())
            .map(|o| o.label.clone());

        let mut select = Select::new(format!("{key}-range-field"), &labels)
            .values(&values)
            .current(current.as_deref())
            .label(t("export.range"))
            .open(vm.scope_open);
        if let Some(cb) = self.scope_open.clone() {
            select = select.on_toggle(move |s: &mut S| cb(s));
        }
        if let Some(cb) = self.scope_close.clone() {
            select = select.on_close(move |s: &mut S| cb(s));
        }
        if let Some(cb) = self.scope.clone() {
            let refused = options.clone();
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                let scope = ExportScope::from_value(value);
                // The zero-selection "selected rows" option is refused, the
                // same way a disabled format is (see the module docs).
                if refused.iter().any(|o| o.scope == scope && o.disabled) {
                    return;
                }
                cb(s, scope);
            });
        }
        col()
            .key(format!("{key}-range"))
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .child(select.build(k, layer))
    }

    #[allow(dead_code)] // reachable only through `build`
    fn picker(&self, s: &S, dark: bool) -> Option<El> {
        let vm = self.vm;
        let is_selected = self.picker_is_selected?;
        let toggle = self.picker_toggle?;
        let select_all = self.picker_select_all?;
        let clear_all = self.picker_clear_all?;
        let reset_default = self.picker_reset_default?;

        let columns: Vec<ExportColumnPickerColumn> = vm
            .columns
            .iter()
            .map(|c| {
                let mut column = ExportColumnPickerColumn::new(c.name.clone());
                if let Some(col_type) = &c.col_type {
                    column = column.with_type(col_type.clone());
                }
                if vm.primary_key_columns.iter().any(|pk| pk == &c.name) {
                    column = column.primary_key();
                }
                column
            })
            .collect();
        Some(
            ExportColumnPicker::new(columns, is_selected, toggle, select_all, clear_all, reset_default)
                .key(format!("{}-columns", self.key))
                .build(s, dark),
        )
    }

    #[allow(dead_code)] // reachable only through `build`
    fn summary(&self, k: &Kit) -> El {
        let key = self.key;
        let vm = self.vm;
        let cols = vm.selected_column_names().len();
        let text_line = t("export.willExport")
            .replace("{rows}", &vm.row_count().to_string())
            .replace("{cols}", &cols.to_string());
        row()
            .key(format!("{key}-summary"))
            .w_full()
            .min_w(0.0)
            .items_center()
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base)
            .px(12.0)
            .py(8.0)
            .gap(6.0)
            .child(
                text(format!("{text_line}, {}", t("export.formatAs")))
                    .key(format!("{key}-summary-line"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .min_w(0.0)
                    .wrap(),
            )
            .child(
                text(vm.settled_format().label())
                    .key(format!("{key}-summary-format"))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base)
                    .min_w(0.0),
            )
    }
}
