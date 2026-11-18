//! Internal sub-structures of [`super::batch_export_dialog::BatchExportDialog`].
//!
//! React's batch-export form is four independent blocks — table selection,
//! mode, format and output. Each is a small builder here so the modal keeps
//! React's order without one giant paint function. They are siblings of the
//! dialog module (not children) so neither file alone crosses the 800-line
//! limit; the dialog owns the data model and passes its VM plus callbacks in.

use rune::{col, row, text, El, ScrollSpec, Tx};
use rune_kit::Kit;

use super::batch_export_dialog::{
    BatchExportMode, BatchExportVm, CloseFn, DataFormat, FormatFn, ModeFn, OutputFn, OutputMode,
    TableFn,
};
use crate::i18n::t;
use crate::ui::select::Select;
use crate::ui::toggle::{Checkbox, RadioGroup, RadioItem};

/// The table-selection block: label + select-all/clear-all + scrolling checkbox
/// list (React's first `<div>` in the form).
pub(crate) struct TableSelection<'a, S: 'static> {
    pub(crate) key: &'a str,
    pub(crate) vm: &'a BatchExportVm,
    pub(crate) toggle_table: Option<TableFn<S>>,
    pub(crate) select_all: Option<CloseFn<S>>,
    pub(crate) clear_all: Option<CloseFn<S>>,
}

impl<S: 'static> TableSelection<'_, S> {
    pub(crate) fn build(self, k: &Kit) -> El {
        let key = self.key;
        let vm = self.vm;

        let mut select_all = row()
            .key(format!("{key}-select-all"))
            .role("button")
            .focusable(!vm.tables.is_empty() && !vm.all_selected())
            .center_y()
            .child(
                text(t("common.selectAll"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            )
            .disabled(vm.tables.is_empty() || vm.all_selected());
        if let Some(cb) = self.select_all {
            select_all = select_all.on_press(move |s: &mut S| cb(s));
        }

        let mut clear_all = row()
            .key(format!("{key}-clear-all"))
            .role("button")
            .focusable(!vm.selected.is_empty())
            .center_y()
            .child(
                text(t("batchExport.clearAll"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            )
            .disabled(vm.selected.is_empty());
        if let Some(cb) = self.clear_all {
            clear_all = clear_all.on_press(move |s: &mut S| cb(s));
        }

        let header = row()
            .key(format!("{key}-tables-header"))
            .w_full()
            .min_w(0.0)
            .items_center()
            .justify_between()
            .gap(8.0)
            .mb(4.0)
            .child(
                text(t("batchExport.selectTables"))
                    .key(format!("{key}-tables-label"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(row().gap(8.0).shrink().child(select_all).child(clear_all));

        let mut list = col()
            .key(format!("{key}-table-list"))
            .w_full()
            .min_w(0.0)
            .max_h(160.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base)
            .padding(8.0);

        if vm.tables.is_empty() {
            list = list.child(
                text(t("batchExport.noTablesSelected"))
                    .key(format!("{key}-tables-empty"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            );
        } else {
            for name in &vm.tables {
                let mut checkbox = Checkbox::new(
                    format!("{key}-table-{name}"),
                    vm.selected.iter().any(|s| s == name),
                )
                .label(name.clone());
                if let Some(cb) = self.toggle_table.clone() {
                    let name = name.clone();
                    checkbox = checkbox.on_toggle(move |s: &mut S| cb(s, name.clone()));
                }
                list = list.child(checkbox.build(k));
            }
        }

        col()
            .key(format!("{key}-table-section"))
            .w_full()
            .min_w(0.0)
            .child(header)
            .child(list)
    }
}

/// The mode radio fieldset (React's `<fieldset>` with three radios).
pub(crate) struct ModeOptions<'a, S: 'static> {
    pub(crate) key: &'a str,
    pub(crate) vm: &'a BatchExportVm,
    pub(crate) mode: Option<ModeFn<S>>,
}

impl<S: 'static> ModeOptions<'_, S> {
    pub(crate) fn build(self, k: &Kit) -> El {
        let key = self.key;
        let modes = [
            BatchExportMode::StructureOnly,
            BatchExportMode::DataOnly,
            BatchExportMode::DataAndStructure,
        ];
        let mut group = RadioGroup::new(format!("{key}-mode-group")).label(t("batchExport.mode"));
        for mode in modes {
            let mut item = RadioItem::new(t(mode.label_key()), mode == self.vm.mode);
            if let Some(cb) = self.mode.clone() {
                item = item.on_select(move |s: &mut S| cb(s, mode));
            }
            group = group.item(item);
        }
        group.build(k)
    }
}

/// The data-format block: label + kit `Select` (React's `DATA_FORMAT_OPTIONS`).
pub(crate) struct FormatSelector<'a, S: 'static> {
    pub(crate) key: &'a str,
    pub(crate) vm: &'a BatchExportVm,
    pub(crate) format: Option<FormatFn<S>>,
    pub(crate) format_open: Option<CloseFn<S>>,
    pub(crate) format_close: Option<CloseFn<S>>,
}

#[allow(dead_code)] // internal sub-structure; built from `BatchExportDialog::form`
impl<S: 'static> FormatSelector<'_, S> {
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let key = self.key;
        let vm = self.vm;

        // Web only offers CSV (React's `IS_DESKTOP_BUILD ? all : filter(csv)`).
        let formats = if vm.is_desktop {
            vec![
                DataFormat::Csv,
                DataFormat::Tsv,
                DataFormat::Json,
                DataFormat::Markdown,
                DataFormat::Xlsx,
                DataFormat::Parquet,
                DataFormat::SqlInsert,
                DataFormat::SqlUpdate,
            ]
        } else {
            vec![DataFormat::Csv]
        };
        let labels: Vec<&str> = formats.iter().map(|f| f.label()).collect();
        let values: Vec<&str> = formats.iter().map(|f| f.value()).collect();

        let mut select = Select::new(format!("{key}-format-field"), &labels)
            .values(&values)
            .current(Some(vm.data_format.value()))
            .label(t("batchExport.format"))
            .open(vm.format_open)
            .disabled(vm.format_disabled());
        if let Some(cb) = self.format_open.clone() {
            select = select.on_toggle(move |s: &mut S| cb(s));
        }
        if let Some(cb) = self.format_close.clone() {
            select = select.on_close(move |s: &mut S| cb(s));
        }
        if let Some(cb) = self.format.clone() {
            // The kit does not close its listbox on a choice; React's Select
            // does, so selecting a format also runs the close callback.
            let close = self.format_close.clone();
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                cb(s, DataFormat::from_value(value));
                if let Some(close) = &close {
                    close(s);
                }
            });
        }

        col()
            .key(format!("{key}-format"))
            .w_full()
            .min_w(0.0)
            .gap(4.0)
            .child(
                text(t("batchExport.format"))
                    .key(format!("{key}-format-label"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(select.build(k, layer))
    }
}

/// The output radio fieldset (React's `<fieldset>` with single/zip radios).
pub(crate) struct OutputOptions<'a, S: 'static> {
    pub(crate) key: &'a str,
    pub(crate) vm: &'a BatchExportVm,
    pub(crate) output: Option<OutputFn<S>>,
}

impl<S: 'static> OutputOptions<'_, S> {
    pub(crate) fn build(self, k: &Kit) -> El {
        let key = self.key;
        let mut single =
            RadioItem::new(t("batchExport.outputSingle"), self.vm.output_mode == OutputMode::Single);
        let mut zip = RadioItem::new(t("batchExport.outputZip"), self.vm.output_mode == OutputMode::Zip)
            .disabled(!self.vm.is_desktop);
        if let Some(cb) = self.output.clone() {
            let single_cb = cb.clone();
            single = single.on_select(move |s: &mut S| single_cb(s, OutputMode::Single));
            zip = zip.on_select(move |s: &mut S| cb(s, OutputMode::Zip));
        }
        RadioGroup::new(format!("{key}-output-group"))
            .label(t("batchExport.output"))
            .item(single)
            .item(zip)
            .build(k)
    }
}
