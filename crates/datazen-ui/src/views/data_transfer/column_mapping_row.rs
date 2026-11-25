//! One row of the data-transfer column-mapping table.
//!
//! Ported from `ColumnMappingRow` in
//! `packages/frontend/src/windows/data-transfer/ColumnMappingEditor.tsx`
//! (T023-S1, React lines 442–524). The row model
//! ([`TransferColumnMapping`]) and the editor's `targetOptions` helper
//! (React lines 96–108, [`target_options`]) live here too, because the row
//! consumes both and the editor shell (T023-S2) reuses them unchanged.
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim where the id is
//! already unique per row (it embeds the source column name):
//!
//! | part | key |
//! |---|---|
//! | root (caller-supplied; the probe passes `data-transfer-column-row`) | `<key>` |
//! | source column cell | `<key>-source` |
//! | source type cell | `<key>-source-type` |
//! | target column cell | `<key>-target` |
//! | target select wrapper | `data-transfer-target-select-{source}` |
//! | target select field (kit derives `-trigger`/`-content`/`-scrim`) | `data-transfer-target-select-{source}-field` |
//! | target column input (create-new) | `data-transfer-target-col-{source}` |
//! | target native-type input | `data-transfer-target-type-{source}` |
//! | skip cell | `<key>-skip-cell` |
//! | skip checkbox | `data-transfer-skip-{source}` |
//!
//! React repeats the row testid `data-transfer-column-row` on every row; Rune
//! keys must be unique among siblings, so the editor shell (T023-S2) passes a
//! per-row root key and the shared `data-transfer-column-row` id is the probe's
//! single-row key, not a contract the shell can keep.
//!
//! Open S2 decision (T023-S1 D3), resolved by T023-S2: `build` ends in
//! `rune_kit::root`, which forces the row to full-window size and adds its
//! own overlay layer — so the editor shell cannot stack rows as built.
//! The resolution is [`ColumnMappingRow::build_inner`]: the row without its
//! kit root, returning the row element plus the overlay layer its select
//! listbox needs. The shell calls `build_inner` once per row, collects the
//! layers, and calls `rune_kit::root` exactly once over the whole editor.
//! `build` is unchanged behaviour — it is `build_inner` plus the root — so
//! the S1 single-row probes still exercise the same layout. The inner row
//! also carries `.w_full()`: inside a kit root that was already forced by
//! `root`, and inside the shell's rows column it fills the table width.
//!
//! ## Documented deviations from React
//!
//! * **Text commits on submit, not per keystroke.** React's `Input onChange`
//!   commits `{ targetColumn, skip }` on every keystroke. Rune edits go
//!   through a `TextEdit` buffer owned by the host state; the row commits the
//!   buffer on submit (Enter), reading the edit's display text — the same
//!   on-edit-buffer / on-submit-commit shape as `AdminCreateDialog`. The
//!   editor shell (T023-S2) owns the `TextEdit`s and re-seeds them from the
//!   model, so a discarded edit cannot linger.
//! * **Disabled text inputs** go through the new `TextField::disabled`
//!   (kit `State::Disabled`, added with this slice — `ui/field.rs` had no
//!   disabled input). The row additionally guards every commit handler with
//!   `disabled`, mirroring React's `commit()` ("an inert editor is inert in
//!   the handler, not only in the DOM").
//! * **Source-type tooltip**: React's `title={sourceType}` becomes
//!   `El::help`, the Rune hover text.
//! * **Unmapped wash**: React `bg-amber-500/5` is `warning.fg` at 5% alpha,
//!   the same token shape the export panel uses for its green wash.
//! * **Input height**: React's `Input` defaults to `h-10` (40pt,
//!   `packages/ui/src/Input.tsx:13`), so both row inputs are 40pt tall —
//!   unlike the recordset editor's explicit `h-7` inputs (T023-S3).

// The production consumer is the editor shell (T023-S2); until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle, Tx};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

/// React `TransferColumnMapping` (`commands/transfer.ts:21-27`).
///
/// `skip` is optional in TypeScript; here it is a plain bool (`false` when a
/// mapping row is created without one — `normalizeColumnMappings` sets an
/// explicit `skip: true` only for rows it synthesises, see T307).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferColumnMapping {
    pub source_column: String,
    pub target_column: String,
    pub skip: bool,
    /// Native target DDL type override (cross-dialect create-new).
    pub target_native_type: Option<String>,
}

/// React `Partial<TransferColumnMapping>` as the row reports it.
///
/// `target_native_type` is `Option<Option<String>>` because React
/// distinguishes "leave it" (`None`) from "clear it" (`Some(None)`, an
/// all-whitespace edit).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnMappingPatch {
    pub target_column: Option<String>,
    pub skip: Option<bool>,
    pub target_native_type: Option<Option<String>>,
}

impl ColumnMappingPatch {
    /// Apply the patch to a row, exactly as the editor's `updateMappings`
    /// spreads `{ ...m, ...patch }` over the matching row.
    pub fn apply_to(&self, row: &mut TransferColumnMapping) {
        if let Some(target_column) = &self.target_column {
            row.target_column = target_column.clone();
        }
        if let Some(skip) = self.skip {
            row.skip = skip;
        }
        if let Some(target_native_type) = &self.target_native_type {
            row.target_native_type = target_native_type.clone();
        }
    }
}

/// One `{ value, label }` React select option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetOption {
    pub value: String,
    pub label: String,
}

impl TargetOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// React `ColumnMappingEditor.targetOptions` (lines 96–108): the target
/// columns as options, the current value prepended when it is not among them,
/// and the empty "pick a column" option first unless one is already there.
///
/// `pick_label` is the resolved `transfer.mapping.pickTargetColumn` text; the
/// caller owns the key, as in `ExportSuccessPanel`.
pub fn target_options(
    target_columns: &[String],
    current: &str,
    pick_label: &str,
) -> Vec<TargetOption> {
    let mut options: Vec<TargetOption> = target_columns
        .iter()
        .map(|col| TargetOption::new(col.clone(), col.clone()))
        .collect();
    if !current.is_empty() && !target_columns.iter().any(|col| col == current) {
        options.insert(0, TargetOption::new(current, current));
    }
    if !options.iter().any(|o| o.value.is_empty()) {
        options.insert(0, TargetOption::new(String::new(), pick_label));
    }
    options
}

/// React `ColumnMappingRow`'s `unmapped`: a live row with no target column.
/// A skipped row is not unmapped — it is deliberately excluded.
pub fn is_unmapped(row: &TransferColumnMapping) -> bool {
    !row.skip && row.target_column.trim().is_empty()
}

/// The controlled state of one row. Fields mirror `ColumnMappingRowProps`;
/// `target_select_open` is the kit-`Select` open flag the host owns, as in
/// `MigrationEndpointsBar`.
#[derive(Clone, Debug)]
pub struct ColumnMappingRowVm {
    pub row: TransferColumnMapping,
    pub create_new: bool,
    pub show_target_type: bool,
    pub source_type: Option<String>,
    pub target_options: Vec<TargetOption>,
    pub target_select_open: bool,
    pub disabled: bool,
}

impl Default for ColumnMappingRowVm {
    fn default() -> Self {
        Self {
            row: TransferColumnMapping::default(),
            create_new: false,
            show_target_type: false,
            source_type: None,
            target_options: Vec::new(),
            target_select_open: false,
            disabled: false,
        }
    }
}

type Change<S> = Rc<dyn Fn(&mut S, ColumnMappingPatch)>;
type EditFn<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut TextEdit>;
type Action<S> = Rc<dyn Fn(&mut S)>;

/// Builder for one mapping row. `S` is the host state that owns the row's
/// `TextEdit` buffers and the select's open flag.
pub struct ColumnMappingRow<'a, S: 'static> {
    key: String,
    vm: ColumnMappingRowVm,
    dark: bool,
    target_column_edit: Option<&'a TextEdit>,
    target_native_type_edit: Option<&'a TextEdit>,
    on_change: Option<Change<S>>,
    on_target_column_edit: Option<EditFn<S>>,
    on_target_native_type_edit: Option<EditFn<S>>,
    on_toggle_select: Option<Action<S>>,
    on_close_select: Option<Action<S>>,
}

impl<'a, S: 'static> ColumnMappingRow<'a, S> {
    pub fn new(key: impl Into<String>, vm: ColumnMappingRowVm) -> Self {
        Self {
            key: key.into(),
            vm,
            dark: true,
            target_column_edit: None,
            target_native_type_edit: None,
            on_change: None,
            on_target_column_edit: None,
            on_target_native_type_edit: None,
            on_toggle_select: None,
            on_close_select: None,
        }
    }

    /// React `onChange`: receives the patch for this row.
    pub fn on_change(mut self, f: impl Fn(&mut S, ColumnMappingPatch) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// The `TextEdit` the create-new target-column input displays. The
    /// editor shell (T023-S2) seeds it from `row.target_column`.
    pub fn target_column_edit(mut self, edit: &'a TextEdit) -> Self {
        self.target_column_edit = Some(edit);
        self
    }

    /// The `TextEdit` the target-native-type input displays, seeded from
    /// `row.target_native_type`.
    pub fn target_native_type_edit(mut self, edit: &'a TextEdit) -> Self {
        self.target_native_type_edit = Some(edit);
        self
    }

    /// Where the create-new target-column input writes its keystrokes.
    pub fn on_target_column_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        let f: EditFn<S> = Rc::new(f);
        self.on_target_column_edit = Some(f);
        self
    }

    /// Where the target-native-type input writes its keystrokes.
    pub fn on_target_native_type_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        let f: EditFn<S> = Rc::new(f);
        self.on_target_native_type_edit = Some(f);
        self
    }

    /// Route the target select's open/close to host state. Without it the
    /// select builds but never opens (see `ui/select.rs`).
    pub fn on_toggle_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_select = Some(Rc::new(f));
        self
    }

    pub fn on_close_select(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_select = Some(Rc::new(f));
        self
    }

    /// React's row: `flex items-center gap-3 border-t border-edge px-3 py-1.5
    /// text-sm`, with the amber wash while the row is unmapped.
    ///
    /// A single row on its own, exactly as the S1 probes mount it.
    pub fn build(self, dark: bool) -> El {
        let (root, layer) = self.build_inner(dark);
        rune_kit::root(root, layer)
    }

    /// The same row without its kit root: the row element plus the overlay
    /// layer (the target select's listbox) the caller must mount alongside
    /// it. The editor shell (T023-S2) calls this once per row and roots the
    /// whole editor once — stacking `build` outputs would nest N full-window
    /// `kit-root`s, each forcing its row to window size.
    pub fn build_inner(mut self, dark: bool) -> (El, Vec<El>) {
        self.dark = dark;
        let k = kit(dark);
        let vm = &self.vm;
        let source = vm.row.source_column.clone();
        let key = self.key.clone();
        let mut layer = Vec::new();

        let mut root = row()
            .key(key.clone())
            .items_center()
            .gap(12.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
            .px(12.0)
            .py(6.0)
            .w_full()
            .min_w(0.0);
        if is_unmapped(&vm.row) {
            root = root.fill(k.alpha(k.c.warning.fg, 0.05));
        }

        // `min-w-0 flex-1 truncate font-mono text-xs`.
        root = root.child(
            text(source.clone())
                .key(format!("{key}-source"))
                .font(k.ty.mono)
                .color(k.c.text_base)
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );

        if vm.show_target_type {
            // `w-28 shrink-0 truncate font-mono text-[11px] text-fg-muted`,
            // `title={sourceType}`.
            let mut cell = text(vm.source_type.clone().unwrap_or_else(|| "—".to_owned()))
                .key(format!("{key}-source-type"))
                .font(TextStyle {
                    size: 11.0,
                    ..k.ty.mono
                })
                .color(k.c.text_muted)
                .truncate()
                .w(112.0);
            if let Some(source_type) = &vm.source_type {
                cell = cell.help(source_type.clone());
            }
            root = root.child(cell);
        }

        root = root.child(self.build_target_cell(k, &key, &source, &mut layer));
        if vm.show_target_type {
            root = root.child(self.build_target_type_cell(k, &key, &source));
        }

        // `flex w-16 shrink-0 justify-center`.
        let skip = vm.row.skip;
        let disabled = vm.disabled;
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let mut skip_cell = row().key(format!("{key}-skip-cell")).w(64.0).center();
        let mut checkbox =
            Checkbox::new(format!("data-transfer-skip-{source}"), skip).disabled(disabled);
        checkbox = checkbox.on_toggle(move |s: &mut S| {
            if disabled {
                return;
            }
            change(
                s,
                ColumnMappingPatch {
                    skip: Some(!skip),
                    ..ColumnMappingPatch::default()
                },
            );
        });
        skip_cell = skip_cell.child(checkbox.build(k));
        root = root.child(skip_cell);

        (root, layer)
    }

    /// The `min-w-0 flex-1` target cell: a text input when the target table is
    /// being created, a select otherwise.
    fn build_target_cell(
        &self,
        k: &'static Kit,
        key: &str,
        source: &str,
        layer: &mut Vec<El>,
    ) -> El {
        let mut cell = col().key(format!("{key}-target")).grow(1.0).min_w(0.0);
        if self.vm.create_new {
            cell = cell.child(self.build_target_input(k, source));
        } else {
            cell = cell.child(self.build_target_select(k, source, layer));
        }
        cell
    }

    /// React's create-new branch: `<Input value={row.targetColumn}
    /// onChange={(e) => onChange({ targetColumn: e.target.value, skip:
    /// !e.target.value.trim() })} />`. The commit happens on submit (see the
    /// module docs); without an edit buffer the value renders as plain text.
    fn build_target_input(&self, k: &'static Kit, source: &str) -> El {
        let Some(edit) = self.target_column_edit else {
            debug_assert!(
                false,
                "a create-new target input needs target_column_edit + on_target_column_edit"
            );
            return text(self.vm.row.target_column.clone())
                .key(format!("data-transfer-target-col-{source}"))
                .font(TextStyle {
                    size: 13.0,
                    ..k.ty.mono
                })
                .truncate();
        };
        self.build_text_input(
            format!("data-transfer-target-col-{source}"),
            edit,
            self.on_target_column_edit.clone(),
            None,
            TextStyle {
                size: 13.0,
                ..k.ty.mono
            },
            true,
        )
    }

    /// React's select branch: `<div
    /// data-testid={data-transfer-target-select-{source}}><Select
    /// value={row.targetColumn} options={targetOptions} onChange={(value) =>
    /// onChange({ targetColumn: value, skip: !value.trim() })} /></div>`.
    /// The select shows the option label for the current value and the
    /// `transfer.mapping.pickTargetColumn` text while nothing is chosen.
    fn build_target_select(&self, k: &'static Kit, source: &str, layer: &mut Vec<El>) -> El {
        let vm = &self.vm;
        let labels: Vec<&str> = vm.target_options.iter().map(|o| o.label.as_str()).collect();
        let values: Vec<&str> = vm.target_options.iter().map(|o| o.value.as_str()).collect();
        let current_label = vm
            .target_options
            .iter()
            .find(|o| o.value == vm.row.target_column)
            .map(|o| o.label.as_str());
        let mut select = Select::new(
            format!("data-transfer-target-select-{source}-field"),
            &labels,
        )
        .values(&values)
        .current(current_label)
        .placeholder(t("transfer.mapping.pickTargetColumn"))
        .label(t("transfer.mapping.targetColumn"))
        .disabled(vm.disabled)
        .open(vm.target_select_open);
        let toggle = self
            .on_toggle_select
            .clone()
            .unwrap_or_else(|| Rc::new(|_: &mut S| {}));
        select = select.on_toggle(move |s: &mut S| toggle(s));
        let close = self
            .on_close_select
            .clone()
            .unwrap_or_else(|| Rc::new(|_: &mut S| {}));
        select = select.on_close(move |s: &mut S| close(s));
        let disabled = vm.disabled;
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            if disabled {
                return;
            }
            change(
                s,
                ColumnMappingPatch {
                    target_column: Some(value.to_owned()),
                    skip: Some(value.trim().is_empty()),
                    ..ColumnMappingPatch::default()
                },
            );
        });
        col()
            .key(format!("data-transfer-target-select-{source}"))
            .grow(1.0)
            .min_w(0.0)
            .child(select.build(k, layer))
    }

    /// React's `w-36 shrink-0` native-type cell: `<Input
    /// placeholder="VARCHAR(255)" value={row.targetNativeType ?? ''}
    /// onChange={(e) => onChange({ targetNativeType: e.target.value.trim() ?
    /// e.target.value : undefined })} />`.
    fn build_target_type_cell(&self, k: &'static Kit, key: &str, source: &str) -> El {
        let mut cell = col().key(format!("{key}-target-type")).w(144.0);
        let Some(edit) = self.target_native_type_edit else {
            debug_assert!(
                false,
                "a target-type input needs target_native_type_edit + on_target_native_type_edit"
            );
            return cell.child(
                text(self.vm.row.target_native_type.clone().unwrap_or_default())
                    .key(format!("data-transfer-target-type-{source}"))
                    .font(TextStyle {
                        size: 11.0,
                        ..k.ty.mono
                    })
                    .truncate(),
            );
        };
        let field = self.build_text_input(
            format!("data-transfer-target-type-{source}"),
            edit,
            self.on_target_native_type_edit.clone(),
            Some("VARCHAR(255)"),
            TextStyle {
                size: 11.0,
                ..k.ty.mono
            },
            false,
        );
        cell = cell.child(field);
        cell
    }

    /// One row text input. `is_target_column` selects the submit patch:
    /// `{ targetColumn, skip }` for the target column,
    /// `{ targetNativeType }` (cleared on blank) for the native type.
    /// The field renders in the theme `build` was called with.
    ///
    /// No `.grow()` here, deliberately: the field sits inside a vertical
    /// `col()` cell (`build_target_cell` / `build_target_type_cell`), so
    /// `grow` would grow on the vertical axis and collapse the explicit
    /// `height(40.0)` to the text line height (~15–18pt, T023-S1 D1). The
    /// field still fills its cell width — the kit `TextInput` fills its
    /// parent by default, and the `col()` cross axis stretches it.
    fn build_text_input(
        &self,
        key: String,
        edit: &TextEdit,
        on_edit: Option<EditFn<S>>,
        placeholder: Option<&str>,
        style: TextStyle,
        is_target_column: bool,
    ) -> El {
        let disabled = self.vm.disabled;
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let mut field = TextField::new(key, edit)
            .text_style(style)
            .height(40.0)
            .disabled(disabled);
        if let Some(placeholder) = placeholder {
            field = field.placeholder(placeholder.to_owned());
        }
        if let Some(edit_fn) = on_edit.clone() {
            field = field.on_edit(move |s: &mut S| edit_fn(s));
        }
        if let Some(edit_fn) = on_edit {
            field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
                if disabled {
                    return;
                }
                let value = edit_fn(s).display().0.to_string();
                if is_target_column {
                    change(
                        s,
                        ColumnMappingPatch {
                            target_column: Some(value.clone()),
                            skip: Some(value.trim().is_empty()),
                            ..ColumnMappingPatch::default()
                        },
                    );
                } else {
                    change(
                        s,
                        ColumnMappingPatch {
                            target_native_type: Some(if value.trim().is_empty() {
                                None
                            } else {
                                Some(value)
                            }),
                            ..ColumnMappingPatch::default()
                        },
                    );
                }
            });
        }
        field.build(self.dark)
    }
}
