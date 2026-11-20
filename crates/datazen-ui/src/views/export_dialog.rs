//! Connection-table export dialog, ported from
//! `packages/frontend/src/windows/connection/ExportDialog.tsx` and its shared
//! `lib/exportDialogShared.ts` option builders.
//!
//! React is a form with format/scope selects, a shared column picker and a
//! four-state status machine (form / exporting / success / aborted). The Rune
//! port keeps that structure as a host-owned [`ExportVm`] and emits
//! [`ExportAction`]s; the host executes the export and feeds [`ExportResult`]s
//! back. No fake IPC, no threads.
//!
//! ## Reuse / stand-ins (removal stages)
//!
//! `DataFormat` and `DataExportCapability` are reused from
//! `components::connection::batch_export_dialog`. `ExportColumnPicker` and the
//! provided-data fetcher / Rust exporter have no Rune port: the picker is
//! rendered locally with `ui::toggle::Checkbox` and the host owns the export
//! execution. Remove those stand-ins when their ports land.
//!
//! ## Keys
//!
//! T223's pinned slot keys: `content-view-export-dialog` with `-body`,
//! `-context`, `-confirm`, `-cancel`, `-close`. Added: `-format`, `-scope`,
//! `-format-disabled-reason-<i>`, `-columns`, `-column-<name>`, `-select-all`,
//! `-clear-all`, `-reset-default`, `-summary`, `-error`, `-success`, `-aborted`,
//! `-cancel-export`.

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::components::connection::batch_export_dialog::{DataExportCapability, DataFormat};
use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

const ROOT: &str = "content-view-export-dialog";

/// One column offered by the picker.
#[allow(dead_code)] // VM mirror; remove when the picker port lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportColumn {
    pub name: String,
    pub data_type: String,
    pub is_primary_key: bool,
}

/// React `ExportScope`.
#[allow(dead_code)] // VM surface; consumed by the host and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportScope {
    #[default]
    CurrentPage,
    Selected,
    EntireTable,
}

impl ExportScope {
    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::CurrentPage => "current_page",
            Self::Selected => "selected",
            Self::EntireTable => "entire_table",
        }
    }

    pub(crate) fn from_value(value: &str) -> Self {
        match value {
            "selected" => Self::Selected,
            "entire_table" => Self::EntireTable,
            _ => Self::CurrentPage,
        }
    }

    fn is_streaming(self) -> bool {
        self == Self::EntireTable
    }
}

/// React `status` state.
#[allow(dead_code)] // VM surface; consumed by the result feed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportStatus {
    #[default]
    Form,
    Exporting,
    Success,
    Aborted,
}

/// One format option with its availability and i18n reason.
#[allow(dead_code)] // option mirror; remove when exportDialogShared lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FormatOption {
    pub format: DataFormat,
    pub disabled: bool,
    pub reason_key: Option<&'static str>,
}

/// The full view snapshot the host fills.
#[allow(dead_code)] // VM surface; owned by the host until the panel store lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExportVm {
    pub table_name: String,
    pub columns: Vec<ExportColumn>,
    pub rows: Vec<Vec<String>>,
    pub selected_rows: Vec<usize>,
    pub database_type: Option<String>,
    pub db_session_id: Option<String>,
    pub database: Option<String>,
    pub total_rows: Option<u64>,
    pub default_scope: Option<ExportScope>,
    pub capability: DataExportCapability,
    pub is_desktop: bool,

    pub format: DataFormat,
    pub scope: ExportScope,
    /// Rune's Select needs its open state from the caller; React keeps it inside.
    pub format_select_open: bool,
    pub scope_select_open: bool,
    pub selected_cols: Vec<String>,
    pub status: ExportStatus,
    pub exporting: bool,
    pub cancelling: bool,
    pub partial_deleted: bool,
    pub error: String,
}

impl ExportVm {
    /// React `allowEntire = !locked && supportsFullTableExport && dbSessionId`.
    pub(crate) fn allow_entire(&self) -> bool {
        self.capability != DataExportCapability::None
            && self.capability.supports_full_table()
            && self.db_session_id.as_ref().is_some_and(|id| !id.is_empty())
    }

    /// React's default-scope correction: a scope the driver cannot serve falls
    /// back to the page scope.
    pub(crate) fn effective_scope(&self) -> ExportScope {
        let requested = self.default_scope.unwrap_or(self.scope);
        if requested == ExportScope::EntireTable && !self.allow_entire() {
            ExportScope::CurrentPage
        } else {
            requested
        }
    }

    fn has_key_column(&self) -> bool {
        self.columns.iter().any(|column| column.is_primary_key)
    }

    /// React `buildExportFormatOptions` in the OBJECT option order. Disabled
    /// options stay visible with their reason (DBR-06).
    pub(crate) fn format_options(&self) -> Vec<FormatOption> {
        let formats = [
            DataFormat::Csv,
            DataFormat::Json,
            DataFormat::Markdown,
            DataFormat::Xlsx,
            DataFormat::Parquet,
            DataFormat::SqlInsert,
            DataFormat::SqlUpdate,
        ];
        let streaming = self.effective_scope().is_streaming();
        formats
            .into_iter()
            .map(|format| {
                if format == DataFormat::SqlUpdate && !self.has_key_column() {
                    FormatOption { format, disabled: true, reason_key: Some("export.formatDisabledNoKey") }
                } else if format == DataFormat::Parquet && (!self.is_desktop || !streaming) {
                    FormatOption {
                        format,
                        disabled: true,
                        reason_key: Some("export.formatDisabledParquetRustOnly"),
                    }
                } else if !self.is_desktop && streaming && format != DataFormat::Csv {
                    FormatOption {
                        format,
                        disabled: true,
                        reason_key: Some("export.formatDisabledWebSqlScope"),
                    }
                } else {
                    FormatOption { format, disabled: false, reason_key: None }
                }
            })
            .collect()
    }

    /// React `disabledFormatReasons`: each reason once, in option order.
    pub(crate) fn disabled_format_reasons(&self) -> Vec<&'static str> {
        let mut reasons = Vec::new();
        for option in self.format_options() {
            if let Some(reason) = option.reason_key {
                if option.disabled && !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
        }
        reasons
    }

    /// React `buildExportScopeOptions` labels.
    pub(crate) fn scope_labels(&self) -> Vec<(ExportScope, String, bool)> {
        let rows = self.rows.len().to_string();
        let selected = self.selected_rows.len().to_string();
        let rows_key = t("common.rows");
        let mut options = vec![
            (
                ExportScope::CurrentPage,
                format!("{} ({} {})", t("export.currentPage"), rows, rows_key),
                false,
            ),
            (
                ExportScope::Selected,
                format!("{} ({} {})", t("export.selectedRows"), selected, rows_key),
                self.selected_rows.is_empty(),
            ),
        ];
        if self.allow_entire() {
            let label = match self.total_rows {
                Some(total) => format!("{} ({} {})", t("export.entireTable"), total, rows_key),
                None => t("export.entireTable").to_owned(),
            };
            options.push((ExportScope::EntireTable, label, false));
        }
        options
    }

    /// React `rowCount`.
    pub(crate) fn row_count(&self) -> u64 {
        match self.effective_scope() {
            ExportScope::EntireTable => self.total_rows.unwrap_or(self.rows.len() as u64),
            ExportScope::Selected if !self.selected_rows.is_empty() => self.selected_rows.len() as u64,
            _ => self.rows.len() as u64,
        }
    }

    /// React `exportLocked` and confirm disabled expression.
    pub(crate) fn export_locked(&self) -> bool {
        self.capability == DataExportCapability::None
    }

    pub(crate) fn export_disabled(&self) -> bool {
        self.exporting
            || self.export_locked()
            || (!self.columns.is_empty() && self.selected_cols.is_empty())
    }

    #[allow(dead_code)] // consumer: the export-dialog host wiring (not landed); remove when wired.
    pub(crate) fn toggle_column(&mut self, name: &str) {
        if let Some(index) = self.selected_cols.iter().position(|item| item == name) {
            self.selected_cols.remove(index);
        } else {
            self.selected_cols.push(name.to_owned());
        }
    }

    #[allow(dead_code)] // consumer: the export-dialog host wiring (not landed); remove when wired.
    pub(crate) fn select_all(&mut self) {
        self.selected_cols = self.columns.iter().map(|column| column.name.clone()).collect();
    }

    #[allow(dead_code)] // consumer: the export-dialog host wiring (not landed); remove when wired.
    pub(crate) fn clear_all(&mut self) {
        self.selected_cols.clear();
    }

    #[allow(dead_code)] // consumer: the export-dialog host wiring (not landed); remove when wired.
    pub(crate) fn reset_default(&mut self) {
        self.select_all();
    }

    /// React's open effect: reset the form state, keeping the supplied columns.
    #[allow(dead_code)] // consumer: the export-dialog host wiring (not landed); remove when wired.
    pub(crate) fn reset_on_open(&mut self) {
        self.status = ExportStatus::Form;
        self.error.clear();
        self.cancelling = false;
        self.partial_deleted = false;
        self.exporting = false;
        self.select_all();
    }

    /// React `handleExport` state transition before the host runs the command.
    #[allow(dead_code)] // consumer: the export-dialog host wiring (not landed); remove when wired.
    pub(crate) fn begin_export(&mut self) {
        self.error.clear();
        self.exporting = true;
        self.status = ExportStatus::Exporting;
        self.cancelling = false;
        self.partial_deleted = false;
    }

    /// React's export answer branches.
    #[allow(dead_code)] // consumer: the export-dialog host result feed (not landed); remove when wired.
    pub(crate) fn apply_result(&mut self, result: ExportResult) {
        self.exporting = false;
        match result {
            ExportResult::Success => self.status = ExportStatus::Success,
            ExportResult::Cancelled => self.status = ExportStatus::Form,
            ExportResult::Aborted { partial_deleted } => {
                self.status = ExportStatus::Aborted;
                self.cancelling = false;
                self.partial_deleted = partial_deleted;
            }
            ExportResult::Failed(message) => {
                self.status = ExportStatus::Form;
                self.error = message;
            }
        }
    }
}

/// Host answer to a requested export.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExportResult {
    Success,
    Cancelled,
    Aborted { partial_deleted: bool },
    Failed(String),
}

/// Host calls the dialog emits.
#[allow(dead_code)] // host port; remove when the panel-store view track wires it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExportAction {
    Close,
    SetFormat(DataFormat),
    SetScope(ExportScope),
    ToggleColumn(String),
    SelectAll,
    ClearAll,
    ResetDefault,
    Export,
    CancelExport,
    /// Rune-only: the host owns each Select's open state.
    FormatSelectOpen(bool),
    ScopeSelectOpen(bool),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, ExportAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: ExportAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Render the export dialog into `layer`.
#[allow(dead_code)] // consumer: content_view_dialogs slot (wired by T391).
pub(crate) fn build<S: 'static>(
    vm: &ExportVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let description = t("export.description").replace("{table}", &vm.table_name);
    // Build the body first: Select listboxes go to the top-level layer so their
    // option rows are not clipped by the dialog's scrolling body.
    let body_el = build_body(vm, &on_action, dark, layer);
    let mut dialog = Dialog::new(ROOT, t("common.exportData"))
        .open(true)
        .fit()
        .width(480.0)
        .body_max_height(320.0)
        .description(description)
        .child(row().key(format!("{ROOT}-body")).w_full().min_w(0.0).child(body_el));

    if matches!(vm.status, ExportStatus::Success | ExportStatus::Aborted) {
        dialog = dialog.footer(
            Button::new(t("common.close"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .on_press(emit(&on_action, ExportAction::Close))
                .build(k)
                .key(format!("{ROOT}-confirm")),
        );
    } else {
        if vm.effective_scope() == ExportScope::EntireTable && vm.is_desktop {
            dialog = dialog.footer(
                Button::new(if vm.cancelling { t("export.cancelling") } else { t("export.cancelExport") })
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::ExtraLarge)
                    .disabled(!vm.exporting || vm.cancelling)
                    .on_press(emit(&on_action, ExportAction::CancelExport))
                    .build(k)
                    .key(format!("{ROOT}-cancel-export")),
            );
        }
        dialog = dialog.footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::ExtraLarge)
                .disabled(vm.exporting)
                .on_press(emit(&on_action, ExportAction::Close))
                .build(k)
                .key(format!("{ROOT}-cancel")),
        );
        dialog = dialog.footer(
            Button::new(if vm.exporting { t("export.exporting") } else { t("export.export") })
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .disabled(vm.export_disabled())
                .on_press(emit(&on_action, ExportAction::Export))
                .build(k)
                .key(format!("{ROOT}-confirm")),
        );
    }
    dialog.on_close::<S>(emit(&on_action, ExportAction::Close)).build(k, layer);
}

fn build_body<S: 'static>(
    vm: &ExportVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let k = kit(dark);
    let mut body = col().key(format!("{ROOT}-form")).w_full().min_w(0.0).gap(12.0);
    match vm.status {
        ExportStatus::Success => {
            return body.child(
                row()
                    .key(format!("{ROOT}-success"))
                    .w_full()
                    .child(text(t("export.success")).wrap()),
            );
        }
        ExportStatus::Aborted => {
            let message = if vm.partial_deleted {
                t("export.cancelledPartialDeleted")
            } else {
                t("export.cancelledPartialKept")
            };
            return body.child(
                row()
                    .key(format!("{ROOT}-aborted"))
                    .w_full()
                    .child(text(message).key(format!("{ROOT}-aborted-text")).wrap()),
            );
        }
        _ => {}
    }
    if vm.export_locked() {
        body = body.child(text(t("export.disabledByDriver")).key(format!("{ROOT}-locked").to_owned()).wrap());
    }

    // Format select + disabled reasons.
    let options = vm.format_options();
    let labels: Vec<String> = options.iter().map(|option| option.format.label().to_owned()).collect();
    let values: Vec<String> = options.iter().map(|option| option.format.value().to_owned()).collect();
    // The kit matches `current` against option labels, not wire values.
    let current_label = options
        .iter()
        .find(|option| option.format == vm.format)
        .map(|option| option.format.label().to_owned())
        .unwrap_or_else(|| DataFormat::Csv.label().to_owned());
    let mut select = Select::new(format!("{ROOT}-format"), &labels)
        .values(&values)
        .current(Some(current_label.as_str()))
        .label(t("export.format"))
        .open(vm.format_select_open)
        .on_toggle(move |_s: &mut S| {});
    if let Some(callback) = on_action.clone() {
        let open = callback.clone();
        select = select.on_toggle(move |s: &mut S| open(s, ExportAction::FormatSelectOpen(true)));
        let close = callback.clone();
        select = select.on_close(move |s: &mut S| close(s, ExportAction::FormatSelectOpen(false)));
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
            let format = DataFormat::from_value(value);
            let disabled = options.iter().any(|option| option.format == format && option.disabled);
            if !disabled {
                callback(s, ExportAction::SetFormat(format));
            }
            callback(s, ExportAction::FormatSelectOpen(false));
        });
    }
    body = body
        .child(text(t("export.format")).key(format!("{ROOT}-format-label").to_owned()).wrap())
        .child(select.build(&k, layer));
    for (index, reason) in vm.disabled_format_reasons().into_iter().enumerate() {
        body = body.child(
            text(t(reason))
                .key(format!("{ROOT}-format-disabled-reason-{index}"))
                .wrap(),
        );
    }

    // Scope select.
    let scopes = vm.scope_labels();
    let scope_labels: Vec<String> = scopes.iter().map(|(_, label, _)| label.clone()).collect();
    let scope_values: Vec<String> = scopes.iter().map(|(scope, _, _)| scope.value().to_owned()).collect();
    let current_scope = vm.effective_scope();
    let current_scope_label = scopes
        .iter()
        .find(|(scope, _, _)| *scope == current_scope)
        .map(|(_, label, _)| label.clone())
        .unwrap_or_else(|| t("export.currentPage").to_owned());
    let mut scope_sel = Select::new(format!("{ROOT}-scope"), &scope_labels)
        .values(&scope_values)
        .current(Some(current_scope_label.as_str()))
        .label(t("export.range"))
        .open(vm.scope_select_open)
        .on_toggle(move |_s: &mut S| {});
    if let Some(callback) = on_action.clone() {
        let open = callback.clone();
        scope_sel = scope_sel.on_toggle(move |s: &mut S| open(s, ExportAction::ScopeSelectOpen(true)));
        let close = callback.clone();
        scope_sel = scope_sel.on_close(move |s: &mut S| close(s, ExportAction::ScopeSelectOpen(false)));
        scope_sel = scope_sel.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
            callback(s, ExportAction::SetScope(ExportScope::from_value(value)));
            callback(s, ExportAction::ScopeSelectOpen(false));
        });
    }
    body = body
        .child(text(t("export.range")).key(format!("{ROOT}-scope-label").to_owned()).wrap())
        .child(scope_sel.build(&k, layer));

    // Column picker.
    body = body.child(build_column_picker(vm, on_action, dark));

    // Summary.
    let summary = format!(
        "{}, {} {}",
        t("export.willExport")
            .replace("{rows}", &vm.row_count().to_string())
            .replace("{cols}", &vm.selected_cols.len().to_string()),
        t("export.formatAs"),
        vm.format.label()
    );
    body = body.child(text(summary).key(format!("{ROOT}-summary").to_owned()).wrap());
    if !vm.error.is_empty() {
        body = body.child(CopyableError::new(format!("{ROOT}-error"), vm.error.clone()).build::<S>(k));
    }
    body
}

fn build_column_picker<S: 'static>(
    vm: &ExportVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let mut picker = col().key(format!("{ROOT}-columns")).w_full().min_w(0.0).gap(4.0);
    let selected_all = !vm.columns.is_empty() && vm.selected_cols.len() == vm.columns.len();
    let action = if selected_all { "common.deselectAll" } else { "common.selectAll" };
    picker = picker.child(
        row()
            .key(format!("{ROOT}-columns-actions"))
            .w_full()
            .gap(8.0)
            .child(text(t("export.selectColumns")).wrap())
            .child(
                text(t(action))
                    .key(format!("{ROOT}-{}", if selected_all { "clear-all" } else { "select-all" }))
                    .role("button")
                    .focusable(true)
                    .on_press(emit(on_action, if selected_all { ExportAction::ClearAll } else { ExportAction::SelectAll })),
            ),
    );
    for column in &vm.columns {
        let checked = vm.selected_cols.contains(&column.name);
        let mut checkbox = Checkbox::new(format!("{ROOT}-column-{}", column.name), checked)
            .label(column.name.clone());
        if let Some(callback) = on_action.clone() {
            let name = column.name.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| callback(s, ExportAction::ToggleColumn(name.clone())));
        }
        picker = picker.child(checkbox.build(&kit(dark)));
    }
    picker.child(
        text(t("export.resetColumns"))
            .key(format!("{ROOT}-reset-default"))
            .role("button")
            .focusable(true)
            .on_press(emit(on_action, ExportAction::ResetDefault)),
    )
}
