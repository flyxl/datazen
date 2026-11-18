//! Batch export dialog — port of
//! `packages/frontend/src/windows/connection/BatchExportDialog.tsx`.
//!
//! A modal shell ([`Dialog`]) for exporting a set of selected tables. The React
//! file's host dependencies (`requireTablesExporter`, `runBatchExportJob`,
//! `loadTableExportData`, the `onExport` override and `IS_DESKTOP_BUILD`) are not
//! Rune concerns: they are projected into [`BatchExportVm`] fields and the
//! dialog's callbacks. No exporter is reimplemented here — a press on **Export**
//! asks the host via [`BatchExportDialog::on_export`] with a complete
//! [`ExportRequest`], and the host writes the resulting status/progress/error
//! back into the VM. That mirrors React, where the same `status`/`progress`/
//! `error` state is owned by the component and driven by the async job.
//!
//! Structure follows React's JSX top to bottom: the four body states
//! (`success`, `aborted`, `exporting`, `form`) and the matching footer. The form
//! is split into internal sub-structures — [`TableSelection`], [`ModeOptions`],
//! [`FormatSelector`], [`OutputOptions`] — each a small builder that paints one
//! block, so the modal keeps React's order without one giant paint function.
//! The format block is omitted in `structure_only` mode, exactly like React's
//! `{!formatDisabled && …}` guard.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | panel / title / close / body / footer / scrim / layer | `<key>`, `<key>-title`, `<key>-close`, `<key>-body`, `<key>-footer`, `<key>-scrim`, `<key>-layer` (from [`Dialog`]) |
//! | locked warning | `<key>-locked-warning` |
//! | table section | `<key>-tables-label`, `<key>-select-all`, `<key>-clear-all`, `<key>-table-list`, `<key>-table-{name}`, `<key>-tables-empty` |
//! | mode | `<key>-mode-group`, `<key>-mode-group-item-{i}` |
//! | format | `<key>-format-label`, `<key>-format-field` (kit trigger `<key>-format-field-trigger`) |
//! | output | `<key>-output-group`, `<key>-output-group-item-{i}` |
//! | error | `<key>-error`, `<key>-error-text`, `<key>-error-copy` (from [`CopyableError`]) |
//! | success | `<key>-success`, `<key>-success-icon`, `<key>-success-text` |
//! | aborted | `<key>-aborted`, `<key>-aborted-text` (React's `data-testid="export-aborted-panel"`) |
//! | exporting | `<key>-spinner`, `<key>-exporting-text`, `<key>-progress-track`, `<key>-progress-fill` |
//! | footer | `<key>-footer-close`, `<key>-footer-cancel-export`, `<key>-footer-cancel`, `<key>-footer-export` |
//!
//! All colours, type and radii come from the kit palette; there are no raw
//! colour literals here.

use std::rc::Rc;

use rune::{col, el, icon, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::components::connection::batch_export_parts::{
    FormatSelector, ModeOptions, OutputOptions, TableSelection,
};
use crate::i18n::t;
use crate::icons::ICON_CHECK;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone, SpinnerVariant};

/// React's `status` union: form / exporting / success / aborted.
#[allow(dead_code)] // VM surface; wired by the host/view track that owns export jobs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BatchExportStatus {
    #[default]
    Form,
    Exporting,
    Success,
    Aborted,
}

/// React's `BatchExportMode` union.
#[allow(dead_code)] // VM surface
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BatchExportMode {
    StructureOnly,
    DataOnly,
    #[default]
    DataAndStructure,
}

#[allow(dead_code)] // VM surface
impl BatchExportMode {
    /// The i18n key React uses for this mode's radio label.
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::StructureOnly => "common.structureOnly",
            Self::DataOnly => "common.dataOnly",
            Self::DataAndStructure => "batchExport.modeDataAndStructure",
        }
    }
}

/// React's `BatchExportDataFormat` union, in the order of `DATA_FORMAT_OPTIONS`.
#[allow(dead_code)] // VM surface
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum DataFormat {
    #[default]
    Csv,
    Tsv,
    Json,
    Markdown,
    Xlsx,
    Parquet,
    SqlInsert,
    SqlUpdate,
}

#[allow(dead_code)] // VM surface
impl DataFormat {
    /// The stored value behind the option (React's `option.value`).
    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Json => "json",
            Self::Markdown => "markdown",
            Self::Xlsx => "xlsx",
            Self::Parquet => "parquet",
            Self::SqlInsert => "sql_insert",
            Self::SqlUpdate => "sql_update",
        }
    }

    /// The visible label behind the option (React's `option.label`).
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
            Self::Xlsx => "Excel (XLSX)",
            Self::Parquet => "Parquet",
            Self::SqlInsert => "SQL INSERT",
            Self::SqlUpdate => "SQL UPDATE",
        }
    }

    pub(crate) fn from_value(value: &str) -> Self {
        match value {
            "tsv" => Self::Tsv,
            "json" => Self::Json,
            "markdown" => Self::Markdown,
            "xlsx" => Self::Xlsx,
            "parquet" => Self::Parquet,
            "sql_insert" => Self::SqlInsert,
            "sql_update" => Self::SqlUpdate,
            _ => Self::Csv,
        }
    }
}

/// React's `BatchExportOutputMode` union.
#[allow(dead_code)] // VM surface
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum OutputMode {
    #[default]
    Single,
    Zip,
}

/// React's `DataExportCapability`: batch export is only allowed for `full_table`.
#[allow(dead_code)] // VM surface
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum DataExportCapability {
    None,
    LoadedOnly,
    #[default]
    FullTable,
}

#[allow(dead_code)] // VM surface
impl DataExportCapability {
    /// React's `supportsFullTableExport(capability)`.
    pub(crate) fn supports_full_table(self) -> bool {
        matches!(self, Self::FullTable)
    }
}

/// React's `progress` state: a writer-progress snapshot from the bound exporter.
#[allow(dead_code)] // VM surface
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExportProgress {
    pub(crate) current: usize,
    pub(crate) total: usize,
    pub(crate) table_name: Option<String>,
    pub(crate) rows_written: Option<u64>,
}

/// The complete argument React's `onExport` override receives.
#[allow(dead_code)] // VM surface; handed to the host by `on_export`
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportRequest {
    pub(crate) selected_tables: Vec<String>,
    pub(crate) mode: BatchExportMode,
    pub(crate) data_format: DataFormat,
    pub(crate) output_mode: OutputMode,
}

/// Everything the dialog renders. The host keeps this in the app state `S` and
/// projects it here; every interaction is a callback that mutates `S`.
#[allow(dead_code)] // VM surface
#[derive(Clone, Debug)]
pub(crate) struct BatchExportVm {
    pub(crate) open: bool,
    /// Bumped on each open so the body's scroll viewport resets (from [`Dialog`]).
    pub(crate) open_epoch: u64,
    pub(crate) db_session_id: String,
    pub(crate) database_type: Option<String>,
    pub(crate) database: Option<String>,
    /// Fallback schema for the exported tables (PostgreSQL family).
    pub(crate) schema: Option<String>,
    /// Available table names (from the schema store).
    pub(crate) tables: Vec<String>,
    /// Optional pre-selected table names; the host applies this on open (React's effect).
    pub(crate) initial_selected: Vec<String>,
    pub(crate) selected: Vec<String>,
    pub(crate) mode: BatchExportMode,
    pub(crate) data_format: DataFormat,
    pub(crate) output_mode: OutputMode,
    pub(crate) status: BatchExportStatus,
    /// The cancel was asked for and the run is winding down (DBR-06).
    pub(crate) cancelling: bool,
    /// Whether the aborted run's partial file was deleted (reported honestly).
    pub(crate) partial_deleted: bool,
    pub(crate) progress: Option<ExportProgress>,
    pub(crate) error: Option<String>,
    pub(crate) capability: DataExportCapability,
    /// React's `IS_DESKTOP_BUILD`: controls zip output and the cancel-export button.
    pub(crate) is_desktop: bool,
    /// React's `onExport` override: when present the cancel-export button is hidden.
    pub(crate) has_export_override: bool,
    /// Whether the kit format listbox is open (React's `Select` owns this internally).
    pub(crate) format_open: bool,
}

#[allow(dead_code)] // VM surface
impl Default for BatchExportVm {
    fn default() -> Self {
        Self {
            open: false,
            open_epoch: 0,
            db_session_id: String::new(),
            database_type: None,
            database: None,
            schema: None,
            tables: Vec::new(),
            initial_selected: Vec::new(),
            selected: Vec::new(),
            mode: BatchExportMode::default(),
            data_format: DataFormat::default(),
            // React: `IS_DESKTOP_BUILD ? 'zip' : 'single'` (desktop is the default).
            output_mode: OutputMode::Zip,
            status: BatchExportStatus::default(),
            cancelling: false,
            partial_deleted: false,
            progress: None,
            error: None,
            capability: DataExportCapability::default(),
            is_desktop: true,
            has_export_override: false,
            format_open: false,
        }
    }
}

#[allow(dead_code)] // VM surface
impl BatchExportVm {
    /// React's `useEffect` on open: restore the pre-selection and clear all run state.
    /// The host calls this when it flips `open` back on.
    pub(crate) fn reset_on_open(&mut self) {
        self.selected = self.initial_selected.clone();
        self.mode = BatchExportMode::DataAndStructure;
        self.data_format = DataFormat::Csv;
        self.output_mode = if self.is_desktop {
            OutputMode::Zip
        } else {
            OutputMode::Single
        };
        self.status = BatchExportStatus::Form;
        self.progress = None;
        self.error = None;
        self.cancelling = false;
        self.partial_deleted = false;
        self.format_open = false;
    }

    /// React's `allSelected`: a non-empty table list with every table selected.
    pub(crate) fn all_selected(&self) -> bool {
        !self.tables.is_empty() && self.selected.len() == self.tables.len()
    }

    /// React's `formatDisabled`: structure-only exports have no data format.
    pub(crate) fn format_disabled(&self) -> bool {
        self.mode == BatchExportMode::StructureOnly
    }

    /// React's `batchExportLocked`: the driver cannot export whole tables.
    pub(crate) fn locked(&self) -> bool {
        !self.capability.supports_full_table()
    }
}

pub(crate) type CloseFn<S> = Rc<dyn Fn(&mut S) + 'static>;
pub(crate) type TableFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;
pub(crate) type ModeFn<S> = Rc<dyn Fn(&mut S, BatchExportMode) + 'static>;
pub(crate) type FormatFn<S> = Rc<dyn Fn(&mut S, DataFormat) + 'static>;
pub(crate) type OutputFn<S> = Rc<dyn Fn(&mut S, OutputMode) + 'static>;
pub(crate) type ExportFn<S> = Rc<dyn Fn(&mut S, ExportRequest) + 'static>;
pub(crate) type CopyFn<S> = Rc<dyn Fn(&mut S, &mut Tx) + 'static>;

/// Builder for the dialog, generic over the app state `S` that owns the VM.
#[allow(dead_code)] // reusable component; wired by the export host/view track
pub(crate) struct BatchExportDialog<'a, S: 'static> {
    key: &'a str,
    vm: &'a BatchExportVm,
    close: Option<CloseFn<S>>,
    toggle_table: Option<TableFn<S>>,
    select_all: Option<CloseFn<S>>,
    clear_all: Option<CloseFn<S>>,
    mode: Option<ModeFn<S>>,
    format: Option<FormatFn<S>>,
    format_open: Option<CloseFn<S>>,
    format_close: Option<CloseFn<S>>,
    output: Option<OutputFn<S>>,
    export: Option<ExportFn<S>>,
    cancel: Option<CloseFn<S>>,
    copy: Option<CopyFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by the export host/view track
impl<'a, S: 'static> BatchExportDialog<'a, S> {
    pub(crate) fn new(key: &'a str, vm: &'a BatchExportVm) -> Self {
        Self {
            key,
            vm,
            close: None,
            toggle_table: None,
            select_all: None,
            clear_all: None,
            mode: None,
            format: None,
            format_open: None,
            format_close: None,
            output: None,
            export: None,
            cancel: None,
            copy: None,
        }
    }

    /// Header close / Escape / footer cancel. Suppressed while exporting, like
    /// React's `onClose={exporting ? () => undefined : onClose}`.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.close = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_toggle_table(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.toggle_table = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_select_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.select_all = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_clear_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.clear_all = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_mode(mut self, f: impl Fn(&mut S, BatchExportMode) + 'static) -> Self {
        self.mode = Some(Rc::new(f));
        self
    }

    /// A chosen data format, as the stored value (React casts to `BatchExportDataFormat`).
    pub(crate) fn on_data_format(mut self, f: impl Fn(&mut S, DataFormat) + 'static) -> Self {
        self.format = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_format_open(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.format_open = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_format_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.format_close = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_output_mode(mut self, f: impl Fn(&mut S, OutputMode) + 'static) -> Self {
        self.output = Some(Rc::new(f));
        self
    }

    /// The export action. This is the only place a batch export is requested;
    /// the host runs the job and writes `status`/`progress`/`error` back to the VM.
    pub(crate) fn on_export(mut self, f: impl Fn(&mut S, ExportRequest) + 'static) -> Self {
        self.export = Some(Rc::new(f));
        self
    }

    /// The cancel-export action (desktop, no override path only).
    pub(crate) fn on_cancel_export(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.cancel = Some(Rc::new(f));
        self
    }

    /// Recorded after [`CopyableError`] has copied the error text.
    pub(crate) fn on_copy(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.copy = Some(Rc::new(f));
        self
    }

    /// Paints the modal into `layer`. Format listbox overlays are appended by
    /// [`FormatSelector`] so they sit above the scrim and panel.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        if !self.vm.open {
            return;
        }
        let k = kit(dark);
        let key = self.key;
        let vm = self.vm;
        let exporting = vm.status == BatchExportStatus::Exporting;
        let settled = matches!(
            vm.status,
            BatchExportStatus::Success | BatchExportStatus::Aborted
        );

        let mut dialog = Dialog::new(key, t("batchExport.title"))
            .open(true)
            .open_epoch(vm.open_epoch)
            .width(512.0)
            .fit()
            .body_max_height(408.0)
            .header_pad((18.0, 14.0))
            .body_pad((18.0, 14.0))
            .footer_pad((16.0, 10.0))
            .footer_height(56.0);

        for footer in self.footer(k, settled, exporting) {
            dialog = dialog.footer(footer);
        }
        for child in self.body(k, dark, settled, exporting, layer) {
            dialog = dialog.child(child);
        }

        // While exporting, Escape/press-outside must not dismiss (React's guard).
        let close = self.close.clone();
        dialog = dialog.on_close(move |s: &mut S| {
            if !exporting {
                if let Some(close) = &close {
                    close(s);
                }
            }
        });
        // `Dialog` also uses the same handler for its header close button, so
        // the guard covers React's three dismissal paths at once.
        dialog.build(k, layer);
    }

    // ---- footer ---------------------------------------------------------------

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
        // React: `IS_DESKTOP_BUILD && !onExport` shows the cancel-export control.
        if vm.is_desktop && !vm.has_export_override {
            let label = if vm.cancelling {
                t("export.cancelling")
            } else {
                t("export.cancelExport")
            };
            let mut cancel = Button::new(label)
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Regular)
                .disabled(!exporting || vm.cancelling);
            if let Some(cb) = self.cancel.clone() {
                cancel = cancel.on_press(move |s: &mut S| cb(s));
            }
            buttons.push(cancel.build(k).key(format!("{key}-footer-cancel-export")));
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
            t("batchExport.exporting")
        } else {
            t("batchExport.export")
        };
        let mut export = Button::new(label)
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .disabled(exporting || vm.locked() || vm.selected.is_empty());
        if let Some(cb) = self.export.clone() {
            let request = ExportRequest {
                selected_tables: vm
                    .tables
                    .iter()
                    .filter(|name| vm.selected.iter().any(|s| s == *name))
                    .cloned()
                    .collect(),
                mode: vm.mode,
                data_format: vm.data_format,
                output_mode: vm.output_mode,
            };
            export = export.on_press(move |s: &mut S| cb(s, request.clone()));
        }
        buttons.push(export.build(k).key(format!("{key}-footer-export")));
        buttons
    }

    // ---- body: the four React states -----------------------------------------

    fn body(
        &self,
        k: &Kit,
        dark: bool,
        settled: bool,
        exporting: bool,
        layer: &mut Vec<El>,
    ) -> Vec<El> {
        if !settled && !exporting {
            return self.form(k, layer);
        }
        let key = self.key;
        let vm = self.vm;
        match vm.status {
            BatchExportStatus::Success => vec![
                col()
                    .key(format!("{key}-success"))
                    .w_full()
                    .py(24.0)
                    .items_center()
                    .gap(8.0)
                    .child(
                        el()
                            .key(format!("{key}-success-icon"))
                            .size(48.0, 48.0)
                            .radius(24.0)
                            .center()
                            .fill(k.alpha(k.c.success.fg, 0.15))
                            .child(
                                icon(&ICON_CHECK, 24.0)
                                    .color(k.c.success.fg)
                                    .a11y_hidden(),
                            ),
                    )
                    .child(
                        text(t("batchExport.success"))
                            .key(format!("{key}-success-text"))
                            .font(k.ty.text_small)
                            .color(k.c.text_base),
                    ),
            ],
            BatchExportStatus::Aborted => {
                let message = if vm.partial_deleted {
                    t("export.cancelledPartialDeleted")
                } else {
                    t("export.cancelledPartialKept")
                };
                vec![
                    row()
                        .key(format!("{key}-aborted"))
                        .w_full()
                        .radius(k.radius.md)
                        .border(1.0, k.c.border_base)
                        .fill(k.c.surface_base)
                        .px(12.0)
                        .py(8.0)
                        .child(
                            text(message)
                                .key(format!("{key}-aborted-text"))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_muted)
                                .min_w(0.0)
                                .wrap(),
                        ),
                ]
            }
            BatchExportStatus::Exporting => {
                let mut status = text(t("batchExport.exporting"));
                let mut track = None;
                if let Some(progress) = vm.progress.as_ref().filter(|p| p.total > 0) {
                    let mut label = format!(
                        "{} {}/{}",
                        t("batchExport.exporting"),
                        progress.current,
                        progress.total
                    );
                    if let Some(table) = progress.table_name.as_deref() {
                        label.push_str(" · ");
                        label.push_str(table);
                    }
                    status = text(label);
                    let ratio = (progress.current as f32 / progress.total as f32).clamp(0.0, 1.0);
                    track = Some(
                        row()
                            .key(format!("{key}-progress-track"))
                            .w(224.0)
                            .h(6.0)
                            .radius(3.0)
                            .fill(k.c.surface_raised)
                            .child(
                                el()
                                    .key(format!("{key}-progress-fill"))
                                    .h(6.0)
                                    .w(224.0 * ratio)
                                    .radius(3.0)
                                    .fill(k.c.bg_accent),
                            ),
                    );
                }
                let mut col = col()
                    .key(format!("{key}-exporting"))
                    .w_full()
                    .py(24.0)
                    .items_center()
                    .gap(12.0)
                    .child(
                        Spinner::new(format!("{key}-spinner"))
                            .size(SpinnerSize::Lg)
                            .tone(SpinnerTone::Accent)
                            .variant(SpinnerVariant::Ring)
                            .build(dark),
                    )
                    .child(
                        status
                            .key(format!("{key}-exporting-text"))
                            .font(k.ty.text_small)
                            .color(k.c.text_base),
                    );
                if let Some(track) = track {
                    col = col.child(track);
                }
                vec![col]
            }
            BatchExportStatus::Form => Vec::new(),
        }
    }

    // ---- body: the form and its sub-structures --------------------------------

    fn form(&self, k: &Kit, layer: &mut Vec<El>) -> Vec<El> {
        let key = self.key;
        let vm = self.vm;
        let mut form = col().key(format!("{key}-form")).w_full().min_w(0.0).gap(16.0);

        if vm.locked() {
            form = form.child(
                row()
                    .key(format!("{key}-locked-warning"))
                    .w_full()
                    .min_w(0.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.warning.border)
                    .fill(k.alpha(k.c.warning.bg, 0.1))
                    .px(12.0)
                    .py(8.0)
                    .child(
                        text(t("batchExport.disabledByDriver"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.warning.fg)
                            .min_w(0.0)
                            .wrap(),
                    ),
            );
        }

        form = form.child(
            TableSelection {
                key,
                vm,
                toggle_table: self.toggle_table.clone(),
                select_all: self.select_all.clone(),
                clear_all: self.clear_all.clone(),
            }
            .build(k),
        );
        form = form.child(
            ModeOptions {
                key,
                vm,
                mode: self.mode.clone(),
            }
            .build(k),
        );
        // React: `{!formatDisabled && (<div>…<Select/></div>)}`.
        if !vm.format_disabled() {
            form = form.child(
                FormatSelector {
                    key,
                    vm,
                    format: self.format.clone(),
                    format_open: self.format_open.clone(),
                    format_close: self.format_close.clone(),
                }
                .build(k, layer),
            );
        }
        form = form.child(
            OutputOptions {
                key,
                vm,
                output: self.output.clone(),
            }
            .build(k),
        );

        if let Some(message) = vm.error.as_deref() {
            let mut error = CopyableError::new(format!("{key}-error"), message).copy_button(true);
            if let Some(cb) = self.copy.clone() {
                error = error.on_copy_tx(move |s: &mut S, tx: &mut Tx| cb(s, tx));
            }
            form = form.child(error.build::<S>(k));
        }

        vec![form]
    }
}
