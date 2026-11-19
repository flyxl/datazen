//! The recordset editor — T023-S3.
//!
//! Ported from `RecordsetEditor` in
//! `packages/frontend/src/windows/data-transfer/ColumnMappingEditor.tsx`
//! (React lines 195–440): the enable switch, the single-bound editor
//! (order column + start/end bounds + limit), and the multi-primary-key
//! tuple editor. The editor shell (T023-S2) mounts this in the
//! `data-transfer-recordset` slot it left empty.
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim:
//!
//! | part | key |
//! |---|---|
//! | root | `data-transfer-recordset` |
//! | header row | `data-transfer-recordset-header` (internal) |
//! | enable checkbox | `data-transfer-recordset-enable` |
//! | disabled hint | `data-transfer-recordset-empty` (internal) |
//! | editor body | `data-transfer-recordset-editor` |
//! | tuple editor | `data-transfer-recordset-tuple-editor` |
//! | tuple order line | `data-transfer-recordset-tuple-order` (internal) |
//! | tuple collation hint | `data-transfer-recordset-tuple-collation-hint` |
//! | tuple bound box | `data-transfer-recordset-tuple-{start,end}-box` (internal) |
//! | tuple cell input | `data-transfer-recordset-tuple-{start,end}-{index}` |
//! | tuple inclusive | `data-transfer-recordset-tuple-{start,end}-inclusive` |
//! | order select (trigger key) | `data-transfer-recordset-order-trigger` |
//! | order error | `data-transfer-recordset-order-error` |
//! | single bound input | `data-transfer-recordset-{start,end}` |
//! | single inclusive | `data-transfer-recordset-{start,end}-inclusive` |
//! | limit input | `data-transfer-recordset-limit` |
//!
//! ## Mounting
//!
//! Like the S1 row, [`RecordsetEditor::build_inner`] returns the editor
//! element plus the overlay layer its order select's listbox needs, and
//! [`RecordsetEditor::build`] is `build_inner` plus the kit root — the
//! shell calls `build_inner` once and roots the whole editor once, so
//! the `data-transfer-recordset-order` listbox never nests a second
//! `kit-root` inside the editor.
//!
//! ## Documented deviations from React
//!
//! * **Text commits on submit, not per keystroke** (inherited from S1):
//!   every bound / limit input commits its `TextEdit` buffer on submit
//!   (Enter), like the row inputs. React commits on every keystroke.
//! * **No `sm:grid-cols-2`.** React lays the two bound boxes side by
//!   side only at the `sm` breakpoint and stacks them below it; Rune
//!   rows do not wrap or reflow, so the two boxes always sit side by
//!   side. The gate's 960pt minimum still fits both.
//! * **Limit is stored as `f64`.** React stores `Number(value)` —
//!   including `NaN` for a non-numeric edit — so the model keeps the
//!   same type; [`parse_limit`] mirrors the conversion exactly.
//! * **The order select closes on choose.** React's `Select` closes
//!   its own listbox; the kit select leaves closing to the app (see
//!   S2), so the builder closes it from the `on_choose` handler, and
//!   the shell writes the close back to the host-owned open flag.
//! * **Disabled is inert in the handler, not only in the DOM.** Every
//!   commit closure returns early when `disabled`, mirroring React's
//!   `commit()` — the kit controls are additionally built disabled.
//! * **Unwired renders read-only.** Like the shell, a builder without
//!   edit buffers shows the model values as plain text under the
//!   inputs' keys instead of focusable fields; no callback exists to
//!   drive, so nothing can commit. (A `TextField` without `on_edit`
//!   cannot take focus, so there is no read-only field to build.)

// The production consumer is the editor shell (T023-S2); until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle, Tx};

use super::column_mapping_row::TargetOption;
use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

/// React `TransferRecordsetBound` (`commands/transfer.ts:48-52`).
///
/// `inclusive` is optional in TypeScript; here it is a plain bool that
/// defaults to `true`, the value React substitutes with `?? true`
/// everywhere it reads the bound.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferRecordsetBound {
    pub value: String,
    pub inclusive: bool,
}

impl Default for TransferRecordsetBound {
    fn default() -> Self {
        Self {
            value: String::new(),
            inclusive: true,
        }
    }
}

/// React `TransferRecordsetTupleBound` (`commands/transfer.ts:70-73`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferRecordsetTupleBound {
    pub values: Vec<String>,
    pub inclusive: bool,
}

impl Default for TransferRecordsetTupleBound {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            inclusive: true,
        }
    }
}

/// React `TransferRecordsetTupleRange` (`commands/transfer.ts:64-68`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferRecordsetTupleRange {
    pub columns: Vec<String>,
    pub start: Option<TransferRecordsetTupleBound>,
    pub end: Option<TransferRecordsetTupleBound>,
}

/// React `TransferRecordset` (`commands/transfer.ts:54-62`).
///
/// `limit` is `f64` because React stores `Number(value)` — including
/// `NaN` for a non-numeric edit (see [`parse_limit`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferRecordset {
    pub order_by: Option<String>,
    pub start: Option<TransferRecordsetBound>,
    pub end: Option<TransferRecordsetBound>,
    pub tuple_range: Option<TransferRecordsetTupleRange>,
    pub limit: Option<f64>,
}

/// Which end of a range a patch addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundName {
    Start,
    End,
}

/// React `Partial<TransferRecordsetBound>` as the bound editors report it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RecordsetBoundPatch {
    pub value: Option<String>,
    pub inclusive: Option<bool>,
}

/// React `Partial<TransferRecordsetTupleBound>` as the tuple editors
/// report it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RecordsetTupleBoundPatch {
    pub values: Option<Vec<String>>,
    pub inclusive: Option<bool>,
}

/// React `Partial<TransferRecordset>` as the editor reports it.
///
/// Each field is `Option<Option<T>>` because React distinguishes
/// "leave it" (`None`) from "clear it" (`Some(None)`, e.g. choosing the
/// empty order option writes `orderBy: undefined`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RecordsetPatch {
    pub order_by: Option<Option<String>>,
    pub start: Option<Option<TransferRecordsetBound>>,
    pub end: Option<Option<TransferRecordsetBound>>,
    pub tuple_range: Option<Option<TransferRecordsetTupleRange>>,
    pub limit: Option<Option<f64>>,
}

impl RecordsetPatch {
    /// Apply the patch to a recordset, exactly as React's `update`
    /// spreads `{ ...(recordset ?? {}), ...patch }` over it.
    pub(crate) fn apply_to(&self, recordset: &mut TransferRecordset) {
        if let Some(order_by) = &self.order_by {
            recordset.order_by = order_by.clone();
        }
        if let Some(start) = &self.start {
            recordset.start = start.clone();
        }
        if let Some(end) = &self.end {
            recordset.end = end.clone();
        }
        if let Some(tuple_range) = &self.tuple_range {
            recordset.tuple_range = tuple_range.clone();
        }
        if let Some(limit) = &self.limit {
            recordset.limit = *limit;
        }
    }
}

/// React `defaultOrder` (line 213): the single primary key, if the table
/// has exactly one; otherwise there is no automatic order.
pub(crate) fn default_order(primary_keys: &[String]) -> Option<String> {
    if primary_keys.len() == 1 {
        primary_keys.first().cloned()
    } else {
        None
    }
}

/// React `orderBy` (line 216): the stored order, the single-PK default,
/// or `''` when neither exists.
pub(crate) fn effective_order(recordset: &TransferRecordset, primary_keys: &[String]) -> String {
    recordset
        .order_by
        .clone()
        .or_else(|| default_order(primary_keys))
        .unwrap_or_default()
}

/// React `orderOptions` (lines 217–226): the empty "order required"
/// option first, then one option per source column, labelled
/// `column (TYPE)` when the column type is known, plain `column`
/// otherwise. `required_label` is the resolved
/// `transfer.mapping.recordsetOrderRequired` text; the caller owns the
/// key, as in `ExportSuccessPanel`.
pub(crate) fn order_options(
    columns: &[String],
    source_column_types: &[(String, String)],
    required_label: &str,
) -> Vec<TargetOption> {
    let mut options = vec![TargetOption::new(String::new(), required_label)];
    for column in columns {
        let label = match source_column_types.iter().find(|(name, _)| name == column) {
            Some((_, ty)) => format!("{column} ({ty})"),
            None => column.clone(),
        };
        options.push(TargetOption::new(column.clone(), label));
    }
    options
}

/// React's enable toggle (lines 287–297): a multi-key table starts in
/// tuple mode with the key columns and no bounds; anything else starts
/// with the single-PK default order (or no order when there is no
/// single key).
pub(crate) fn enable_recordset(primary_keys: &[String]) -> TransferRecordset {
    if primary_keys.len() > 1 {
        TransferRecordset {
            tuple_range: Some(TransferRecordsetTupleRange {
                columns: primary_keys.to_vec(),
                start: None,
                end: None,
            }),
            ..TransferRecordset::default()
        }
    } else {
        TransferRecordset {
            order_by: default_order(primary_keys),
            ..TransferRecordset::default()
        }
    }
}

/// React `updateBound` (lines 233–246): merge the patch over the stored
/// bound (defaulting to `{ value: '', inclusive: true }`), and drop the
/// bound entirely when the merged value is blank.
pub(crate) fn apply_bound_update(
    recordset: &TransferRecordset,
    name: BoundName,
    patch: RecordsetBoundPatch,
) -> TransferRecordset {
    let current = match name {
        BoundName::Start => &recordset.start,
        BoundName::End => &recordset.end,
    };
    let mut next = current.clone().unwrap_or_default();
    if let Some(value) = patch.value {
        next.value = value;
    }
    if let Some(inclusive) = patch.inclusive {
        next.inclusive = inclusive;
    }
    let mut out = recordset.clone();
    let cleared = next.value.trim().is_empty();
    match name {
        BoundName::Start => out.start = if cleared { None } else { Some(next) },
        BoundName::End => out.end = if cleared { None } else { Some(next) },
    }
    out
}

/// React `updateTupleBound` (lines 248–282): merge the patch over the
/// stored tuple bound (defaulting to blank values for every key,
/// `inclusive: true`).
///
/// * All-blank values drop the edited bound and keep the other one —
///   the scalar `orderBy` / `start` / `end` are untouched.
/// * Any non-blank value switches the recordset to tuple mode: the
///   scalar order and bounds are cleared and the tuple range carries
///   the edited bound plus the other bound, if it exists.
pub(crate) fn apply_tuple_bound_update(
    recordset: &TransferRecordset,
    primary_keys: &[String],
    name: BoundName,
    patch: RecordsetTupleBoundPatch,
) -> TransferRecordset {
    let tuple = recordset
        .tuple_range
        .clone()
        .unwrap_or(TransferRecordsetTupleRange {
            columns: primary_keys.to_vec(),
            start: None,
            end: None,
        });
    let current = match name {
        BoundName::Start => &tuple.start,
        BoundName::End => &tuple.end,
    };
    let mut next = current.clone().unwrap_or(TransferRecordsetTupleBound {
        values: primary_keys.iter().map(|_| String::new()).collect(),
        inclusive: true,
    });
    if let Some(values) = patch.values {
        next.values = values;
    }
    if let Some(inclusive) = patch.inclusive {
        next.inclusive = inclusive;
    }
    let mut out = recordset.clone();
    if next.values.iter().all(|value| value.trim().is_empty()) {
        out.tuple_range = Some(TransferRecordsetTupleRange {
            columns: primary_keys.to_vec(),
            start: match name {
                BoundName::Start => None,
                BoundName::End => tuple.start.clone(),
            },
            end: match name {
                BoundName::Start => tuple.end.clone(),
                BoundName::End => None,
            },
        });
    } else {
        out.order_by = None;
        out.start = None;
        out.end = None;
        out.tuple_range = Some(TransferRecordsetTupleRange {
            columns: primary_keys.to_vec(),
            start: match name {
                BoundName::Start => Some(next.clone()),
                BoundName::End => tuple.start.clone(),
            },
            end: match name {
                BoundName::Start => tuple.end.clone(),
                BoundName::End => Some(next),
            },
        });
    }
    out
}

/// React's limit commit (lines 417–421): a blank edit clears the limit,
/// anything else is `Number(value)` — including `NaN` for a
/// non-numeric edit.
pub(crate) fn parse_limit(raw: &str) -> Option<f64> {
    let value = raw.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.parse::<f64>().unwrap_or(f64::NAN))
    }
}

/// The limit buffer text for a stored limit: integers render without a
/// fraction (`100`, not `100`), `NaN` renders as `NaN`, and no limit
/// renders empty.
pub(crate) fn format_limit(limit: Option<f64>) -> String {
    match limit {
        None => String::new(),
        Some(value) if value.is_finite() && value.fract() == 0.0 => {
            format!("{}", value as i64)
        }
        Some(value) => format!("{value}"),
    }
}

/// The controlled state of the editor. Fields mirror
/// `RecordsetEditor`'s props (`table.sourceColumns`,
/// `table.sourceColumnTypes`, `table.sourcePrimaryKeys`,
/// `table.recordset`, `disabled`); `order_select_open` is the kit
/// `Select` open flag the host owns, as in `ColumnMappingRowVm`.
#[derive(Clone, Debug)]
pub(crate) struct RecordsetEditorVm {
    pub columns: Vec<String>,
    pub source_column_types: Vec<(String, String)>,
    pub primary_keys: Vec<String>,
    pub recordset: Option<TransferRecordset>,
    pub order_select_open: bool,
    pub disabled: bool,
}

impl RecordsetEditorVm {
    pub(crate) fn source_type(&self, column: &str) -> Option<&str> {
        self.source_column_types
            .iter()
            .find(|(name, _)| name == column)
            .map(|(_, ty)| ty.as_str())
    }

    /// Whether the tuple editor renders: React renders it whenever
    /// `recordset.tupleRange` is present.
    pub(crate) fn is_tuple(&self) -> bool {
        self.recordset
            .as_ref()
            .is_some_and(|recordset| recordset.tuple_range.is_some())
    }
}

/// The host-owned edit buffers for one recordset editor, mirroring
/// `ColumnMappingRowHost`: the shell owns one and re-seeds it from the
/// model whenever a patch lands (see [`RecordsetHost::resync`]).
#[derive(Clone, Debug)]
pub(crate) struct RecordsetHost {
    pub order_open: bool,
    pub start_edit: TextEdit,
    pub end_edit: TextEdit,
    pub tuple_start_edits: Vec<TextEdit>,
    pub tuple_end_edits: Vec<TextEdit>,
    pub limit_edit: TextEdit,
}

impl Default for RecordsetHost {
    fn default() -> Self {
        Self {
            order_open: false,
            start_edit: TextEdit::new(String::new()),
            end_edit: TextEdit::new(String::new()),
            tuple_start_edits: Vec::new(),
            tuple_end_edits: Vec::new(),
            limit_edit: TextEdit::new(String::new()),
        }
    }
}

impl RecordsetHost {
    /// Re-seed every buffer from the model, mirroring React's controlled
    /// inputs (which re-render from `table.recordset` after every
    /// commit). The tuple buffers are re-sized to the primary keys, so a
    /// table whose keys change cannot leave a stale buffer behind.
    pub(crate) fn resync(
        &mut self,
        recordset: &Option<TransferRecordset>,
        primary_keys: &[String],
    ) {
        let bound_value = |name: BoundName| {
            recordset
                .as_ref()
                .and_then(|recordset| match name {
                    BoundName::Start => recordset.start.as_ref(),
                    BoundName::End => recordset.end.as_ref(),
                })
                .map(|bound| bound.value.clone())
                .unwrap_or_default()
        };
        self.start_edit.replace_text(bound_value(BoundName::Start));
        self.end_edit.replace_text(bound_value(BoundName::End));
        let tuple_values = |name: BoundName| -> Vec<String> {
            let bound = recordset.as_ref().and_then(|recordset| {
                recordset.tuple_range.as_ref().and_then(|range| match name {
                    BoundName::Start => range.start.as_ref(),
                    BoundName::End => range.end.as_ref(),
                })
            });
            primary_keys
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    bound
                        .and_then(|bound| bound.values.get(index))
                        .cloned()
                        .unwrap_or_default()
                })
                .collect()
        };
        self.tuple_start_edits = tuple_values(BoundName::Start)
            .into_iter()
            .map(TextEdit::new)
            .collect();
        self.tuple_end_edits = tuple_values(BoundName::End)
            .into_iter()
            .map(TextEdit::new)
            .collect();
        self.limit_edit.replace_text(format_limit(
            recordset.as_ref().and_then(|recordset| recordset.limit),
        ));
    }
}

type Change<S> = Rc<dyn Fn(&mut S, Option<TransferRecordset>)>;
type EditFn<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut TextEdit>;
type IndexedEditFn<S> = Rc<dyn Fn(&mut S, usize) -> &mut TextEdit>;
type Action<S> = Rc<dyn Fn(&mut S)>;

/// Builder for the recordset editor. `S` is the host state that owns
/// the `TextEdit` buffers and the order select's open flag.
pub(crate) struct RecordsetEditor<'a, S: 'static> {
    vm: RecordsetEditorVm,
    dark: bool,
    start_edit: Option<&'a TextEdit>,
    end_edit: Option<&'a TextEdit>,
    tuple_start_edits: Option<&'a [TextEdit]>,
    tuple_end_edits: Option<&'a [TextEdit]>,
    limit_edit: Option<&'a TextEdit>,
    on_change: Option<Change<S>>,
    on_start_edit: Option<EditFn<S>>,
    on_end_edit: Option<EditFn<S>>,
    on_tuple_start_edit: Option<IndexedEditFn<S>>,
    on_tuple_end_edit: Option<IndexedEditFn<S>>,
    on_limit_edit: Option<EditFn<S>>,
    on_toggle_order: Option<Action<S>>,
    on_close_order: Option<Action<S>>,
}

impl<'a, S: 'static> RecordsetEditor<'a, S> {
    pub(crate) fn new(vm: RecordsetEditorVm) -> Self {
        Self {
            vm,
            dark: true,
            start_edit: None,
            end_edit: None,
            tuple_start_edits: None,
            tuple_end_edits: None,
            limit_edit: None,
            on_change: None,
            on_start_edit: None,
            on_end_edit: None,
            on_tuple_start_edit: None,
            on_tuple_end_edit: None,
            on_limit_edit: None,
            on_toggle_order: None,
            on_close_order: None,
        }
    }

    /// React `onChange`: receives the next recordset, or `None` when
    /// the enable toggle switches the recordset off.
    pub(crate) fn on_change(
        mut self,
        f: impl Fn(&mut S, Option<TransferRecordset>) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// The `TextEdit` the start-bound input displays, seeded from
    /// `recordset.start.value`.
    pub(crate) fn start_edit(mut self, edit: &'a TextEdit) -> Self {
        self.start_edit = Some(edit);
        self
    }

    /// The `TextEdit` the end-bound input displays.
    pub(crate) fn end_edit(mut self, edit: &'a TextEdit) -> Self {
        self.end_edit = Some(edit);
        self
    }

    /// The per-key `TextEdit`s the tuple start-bound inputs display,
    /// one per primary key, in key order.
    pub(crate) fn tuple_start_edits(mut self, edits: &'a [TextEdit]) -> Self {
        self.tuple_start_edits = Some(edits);
        self
    }

    /// The per-key `TextEdit`s the tuple end-bound inputs display.
    pub(crate) fn tuple_end_edits(mut self, edits: &'a [TextEdit]) -> Self {
        self.tuple_end_edits = Some(edits);
        self
    }

    /// The `TextEdit` the limit input displays, seeded from
    /// `recordset.limit`.
    pub(crate) fn limit_edit(mut self, edit: &'a TextEdit) -> Self {
        self.limit_edit = Some(edit);
        self
    }

    /// Where the start-bound input writes its keystrokes.
    pub(crate) fn on_start_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_start_edit = Some(Rc::new(f));
        self
    }

    /// Where the end-bound input writes its keystrokes.
    pub(crate) fn on_end_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_end_edit = Some(Rc::new(f));
        self
    }

    /// Where the tuple start-bound cell `index` writes its keystrokes.
    pub(crate) fn on_tuple_start_edit(
        mut self,
        f: impl Fn(&mut S, usize) -> &mut TextEdit + 'static,
    ) -> Self {
        self.on_tuple_start_edit = Some(Rc::new(f));
        self
    }

    /// Where the tuple end-bound cell `index` writes its keystrokes.
    pub(crate) fn on_tuple_end_edit(
        mut self,
        f: impl Fn(&mut S, usize) -> &mut TextEdit + 'static,
    ) -> Self {
        self.on_tuple_end_edit = Some(Rc::new(f));
        self
    }

    /// Where the limit input writes its keystrokes.
    pub(crate) fn on_limit_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_limit_edit = Some(Rc::new(f));
        self
    }

    /// Route the order select's open/close to host state. Without it
    /// the select builds but never opens (see `ui/select.rs`).
    pub(crate) fn on_toggle_order(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_order = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_close_order(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_order = Some(Rc::new(f));
        self
    }

    /// The recordset editor on its own, exactly as the probes mount it.
    pub(crate) fn build(self, dark: bool) -> El {
        let (root, layer) = self.build_inner(dark);
        rune_kit::root(root, layer)
    }

    /// The same editor without its kit root: the editor element plus
    /// the overlay layer (the order select's listbox) the caller must
    /// mount alongside it. The editor shell (T023-S2) calls this once
    /// and roots the whole editor once.
    pub(crate) fn build_inner(mut self, dark: bool) -> (El, Vec<El>) {
        self.dark = dark;
        let k = kit(dark);
        let vm = &self.vm;
        let recordset = vm.recordset.clone();
        let mut layer = Vec::new();

        // React root: `space-y-2 rounded-lg border border-edge
        // bg-surface-alt p-3`.
        let mut root = col()
            .key("data-transfer-recordset")
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .px(12.0)
            .py(12.0)
            .fill(k.c.surface_weak)
            .border(1.0, k.c.border_base)
            .radius(8.0);
        root = root.child(self.header_row(k, recordset.is_some()));

        match &recordset {
            None => {
                root = root.child(
                    text(t("transfer.mapping.noRecordset"))
                        .key("data-transfer-recordset-empty")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted)
                        .wrap(),
                );
            }
            Some(recordset) => {
                let mut body = col()
                    .key("data-transfer-recordset-editor")
                    .w_full()
                    .min_w(0.0)
                    .gap(8.0);
                if vm.is_tuple() {
                    body = body.child(self.tuple_editor(k, recordset));
                } else {
                    body = body.child(self.single_editor(k, recordset, &mut layer));
                }
                body = body.child(self.limit_row(k, recordset));
                root = root.child(body);
            }
        }

        (root, layer)
    }

    /// React 272–303: the title + hint on the left, the enable
    /// checkbox on the right.
    fn header_row(&self, k: &'static rune_kit::Kit, enabled: bool) -> El {
        let vm = &self.vm;
        let disabled = vm.disabled;
        let primary_keys = vm.primary_keys.clone();
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let mut header = row()
            .key("data-transfer-recordset-header")
            .items_start()
            .gap(8.0)
            .w_full()
            .min_w(0.0);
        let mut title_col = col().grow(1.0).min_w(0.0).gap(2.0);
        title_col = title_col.child(
            text(t("transfer.mapping.recordset"))
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        );
        title_col = title_col.child(
            text(t("transfer.mapping.recordsetHint"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap(),
        );
        header = header.child(title_col);
        let mut checkbox =
            Checkbox::new("data-transfer-recordset-enable", enabled).disabled(disabled);
        checkbox = checkbox.on_toggle(move |s: &mut S| {
            if disabled {
                return;
            }
            change(
                s,
                if enabled {
                    None
                } else {
                    Some(enable_recordset(&primary_keys))
                },
            );
        });
        header = header.child(checkbox.build(k));
        header
    }

    /// React 317–368: the tuple editor — the key order line, the
    /// collation hint, and the start/end bound boxes with one input per
    /// primary key.
    fn tuple_editor(&self, k: &'static rune_kit::Kit, recordset: &TransferRecordset) -> El {
        let vm = &self.vm;
        let tuple_range = recordset
            .tuple_range
            .clone()
            .unwrap_or(TransferRecordsetTupleRange {
                columns: vm.primary_keys.clone(),
                start: None,
                end: None,
            });
        let mut editor = col()
            .key("data-transfer-recordset-tuple-editor")
            .w_full()
            .min_w(0.0)
            .gap(8.0);
        editor = editor.child(
            text(format!(
                "{}: {}",
                t("transfer.mapping.recordsetOrder"),
                vm.primary_keys.join(" → ")
            ))
            .key("data-transfer-recordset-tuple-order")
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted)
            .wrap(),
        );
        editor = editor.child(
            text(t("transfer.mapping.recordsetTextCollationHint"))
                .key("data-transfer-recordset-tuple-collation-hint")
                .font(k.ty.text_xsmall)
                .color(k.c.warning.fg)
                .wrap(),
        );
        let mut bounds = row().w_full().min_w(0.0).gap(8.0);
        for name in [BoundName::Start, BoundName::End] {
            let bound = match name {
                BoundName::Start => tuple_range.start.clone(),
                BoundName::End => tuple_range.end.clone(),
            };
            bounds = bounds.child(self.tuple_bound_box(k, recordset, name, bound));
        }
        editor = editor.child(bounds);
        editor
    }

    /// One tuple bound box: the start/end label, one labelled input per
    /// primary key, and the inclusive checkbox once the bound exists.
    fn tuple_bound_box(
        &self,
        k: &'static rune_kit::Kit,
        recordset: &TransferRecordset,
        name: BoundName,
        bound: Option<TransferRecordsetTupleBound>,
    ) -> El {
        let vm = &self.vm;
        let suffix = match name {
            BoundName::Start => "start",
            BoundName::End => "end",
        };
        let mut box_el = col()
            .key(format!("data-transfer-recordset-tuple-{suffix}-box"))
            .grow(1.0)
            .min_w(0.0)
            .gap(4.0)
            .px(8.0)
            .py(8.0)
            .border(1.0, k.c.border_base)
            .radius(8.0);
        box_el = box_el.child(
            text(match name {
                BoundName::Start => t("transfer.mapping.recordsetStart"),
                BoundName::End => t("transfer.mapping.recordsetEnd"),
            })
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted),
        );
        for (index, column) in vm.primary_keys.iter().enumerate() {
            let mut cell = row().items_center().gap(8.0).w_full().min_w(0.0);
            cell = cell.child(
                text(column.clone())
                    .font(TextStyle {
                        size: 12.0,
                        ..k.ty.mono
                    })
                    .color(k.c.text_base)
                    .truncate()
                    .w(96.0),
            );
            let value = bound
                .as_ref()
                .and_then(|bound| bound.values.get(index))
                .cloned()
                .unwrap_or_default();
            cell = cell.child(self.tuple_cell_field(k, recordset, name, index, value));
            box_el = box_el.child(cell);
        }
        if let Some(bound) = &bound {
            box_el = box_el.child(self.inclusive_checkbox(
                k,
                format!("data-transfer-recordset-tuple-{suffix}-inclusive"),
                bound.inclusive,
                InclusiveTarget::Tuple(name),
                recordset,
            ));
        }
        box_el
    }

    /// One tuple cell input. React reads the edited cell from the event
    /// and the rest of the values from the stored bound, so the submit
    /// patch carries the stored values with only this cell replaced —
    /// the other cells' uncommitted drafts are never read.
    fn tuple_cell_field(
        &self,
        k: &'static rune_kit::Kit,
        recordset: &TransferRecordset,
        name: BoundName,
        index: usize,
        value: String,
    ) -> El {
        let vm = &self.vm;
        let suffix = match name {
            BoundName::Start => "start",
            BoundName::End => "end",
        };
        let key = format!("data-transfer-recordset-tuple-{suffix}-{index}");
        let edits = match name {
            BoundName::Start => self.tuple_start_edits,
            BoundName::End => self.tuple_end_edits,
        };
        let Some(edit) = edits.and_then(|edits| edits.get(index)) else {
            return col().grow(1.0).min_w(0.0).child(
                text(value)
                    .key(key)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .truncate(),
            );
        };
        let edit_fn = match name {
            BoundName::Start => self.on_tuple_start_edit.clone(),
            BoundName::End => self.on_tuple_end_edit.clone(),
        };
        let Some(edit_fn) = edit_fn else {
            return col().grow(1.0).min_w(0.0).child(
                text(value)
                    .key(key)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .truncate(),
            );
        };
        let disabled = vm.disabled;
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let recordset = recordset.clone();
        let primary_keys = vm.primary_keys.clone();
        let stored_values: Vec<String> = recordset
            .tuple_range
            .as_ref()
            .and_then(|range| match name {
                BoundName::Start => range.start.as_ref(),
                BoundName::End => range.end.as_ref(),
            })
            .map(|bound| bound.values.clone())
            .unwrap_or_else(|| primary_keys.iter().map(|_| String::new()).collect());
        let mut field = TextField::new(key, edit)
            .text_style(k.ty.text_xsmall)
            .height(28.0)
            .disabled(disabled)
            .placeholder(t("transfer.mapping.recordsetUnbounded"));
        let edit_for_key = Rc::clone(&edit_fn);
        field = field.on_edit(move |s: &mut S| edit_for_key(s, index));
        field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
            if disabled {
                return;
            }
            let mut values = stored_values.clone();
            if index < values.len() {
                values[index] = edit_fn(s, index).display().0.to_string();
            } else {
                values.push(edit_fn(s, index).display().0.to_string());
            }
            change(
                s,
                Some(apply_tuple_bound_update(
                    &recordset,
                    &primary_keys,
                    name,
                    RecordsetTupleBoundPatch {
                        values: Some(values),
                        inclusive: None,
                    },
                )),
            );
        });
        col().grow(1.0).min_w(0.0).child(field.build(self.dark))
    }

    /// React 370–413: the single-bound editor — the order select, the
    /// order-required error, and the start/end bound boxes.
    fn single_editor(
        &self,
        k: &'static rune_kit::Kit,
        recordset: &TransferRecordset,
        layer: &mut Vec<El>,
    ) -> El {
        let vm = &self.vm;
        let mut editor = col().w_full().min_w(0.0).gap(8.0);
        editor = editor.child(self.order_row(k, recordset, layer));
        let order_by = effective_order(recordset, &vm.primary_keys);
        if order_by.is_empty() {
            editor = editor.child(
                text(t("transfer.mapping.recordsetOrderRequired"))
                    .key("data-transfer-recordset-order-error")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg)
                    .wrap(),
            );
        }
        let mut bounds = row().w_full().min_w(0.0).gap(8.0);
        for name in [BoundName::Start, BoundName::End] {
            bounds = bounds.child(self.single_bound_box(k, recordset, name));
        }
        editor = editor.child(bounds);
        editor
    }

    /// React 371–383: the order label + select. The select's current
    /// value is the effective order; choosing the empty option clears
    /// `orderBy`.
    fn order_row(
        &self,
        k: &'static rune_kit::Kit,
        recordset: &TransferRecordset,
        layer: &mut Vec<El>,
    ) -> El {
        let vm = &self.vm;
        let disabled = vm.disabled;
        let order_by = effective_order(recordset, &vm.primary_keys);
        let options = order_options(
            &vm.columns,
            &vm.source_column_types,
            &t("transfer.mapping.recordsetOrderRequired"),
        );
        let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
        let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
        let current_label = options
            .iter()
            .find(|o| o.value == order_by)
            .map(|o| o.label.as_str());
        let mut select = Select::new("data-transfer-recordset-order", &labels)
            .values(&values)
            .current(current_label)
            .label(t("transfer.mapping.recordsetOrder"))
            .disabled(disabled)
            .open(vm.order_select_open)
            .width(192.0);
        let toggle = self
            .on_toggle_order
            .clone()
            .unwrap_or_else(|| Rc::new(|_: &mut S| {}));
        select = select.on_toggle(move |s: &mut S| toggle(s));
        let close = self
            .on_close_order
            .clone()
            .unwrap_or_else(|| Rc::new(|_: &mut S| {}));
        select = select.on_close(move |s: &mut S| close(s));
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let close_on_choose = self
            .on_close_order
            .clone()
            .unwrap_or_else(|| Rc::new(|_: &mut S| {}));
        let recordset = recordset.clone();
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            if disabled {
                return;
            }
            let mut next = recordset.clone();
            RecordsetPatch {
                order_by: Some(if value.is_empty() {
                    None
                } else {
                    Some(value.to_owned())
                }),
                ..RecordsetPatch::default()
            }
            .apply_to(&mut next);
            change(s, Some(next));
            close_on_choose(s);
        });
        let mut row_el = row()
            .key("data-transfer-recordset-order-row")
            .items_center()
            .gap(8.0)
            .w_full()
            .min_w(0.0);
        row_el = row_el.child(
            text(t("transfer.mapping.recordsetOrder"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
        row_el = row_el.child(select.build(k, layer));
        row_el
    }

    /// One single bound box: the start/end label and the input +
    /// inclusive checkbox row.
    fn single_bound_box(
        &self,
        k: &'static rune_kit::Kit,
        recordset: &TransferRecordset,
        name: BoundName,
    ) -> El {
        let suffix = match name {
            BoundName::Start => "start",
            BoundName::End => "end",
        };
        let bound = match name {
            BoundName::Start => recordset.start.clone(),
            BoundName::End => recordset.end.clone(),
        };
        let mut box_el = col().grow(1.0).min_w(0.0).gap(4.0);
        box_el = box_el.child(
            text(match name {
                BoundName::Start => t("transfer.mapping.recordsetStart"),
                BoundName::End => t("transfer.mapping.recordsetEnd"),
            })
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted),
        );
        let mut input_row = row().items_center().gap(8.0).w_full().min_w(0.0);
        input_row = input_row.child(self.single_bound_field(k, recordset, name));
        if let Some(bound) = &bound {
            input_row = input_row.child(self.inclusive_checkbox(
                k,
                format!("data-transfer-recordset-{suffix}-inclusive"),
                bound.inclusive,
                InclusiveTarget::Single(name),
                recordset,
            ));
        }
        box_el = box_el.child(input_row);
        box_el
    }

    /// One single bound input. Without an edit buffer the model value
    /// renders as plain text, the row's no-edit fallback shape.
    fn single_bound_field(
        &self,
        k: &'static rune_kit::Kit,
        recordset: &TransferRecordset,
        name: BoundName,
    ) -> El {
        let vm = &self.vm;
        let suffix = match name {
            BoundName::Start => "start",
            BoundName::End => "end",
        };
        let key = format!("data-transfer-recordset-{suffix}");
        let value = match name {
            BoundName::Start => recordset
                .start
                .as_ref()
                .map(|bound| bound.value.clone())
                .unwrap_or_default(),
            BoundName::End => recordset
                .end
                .as_ref()
                .map(|bound| bound.value.clone())
                .unwrap_or_default(),
        };
        let edit = match name {
            BoundName::Start => self.start_edit,
            BoundName::End => self.end_edit,
        };
        let edit_fn = match name {
            BoundName::Start => self.on_start_edit.clone(),
            BoundName::End => self.on_end_edit.clone(),
        };
        let (Some(edit), Some(edit_fn)) = (edit, edit_fn) else {
            return col().grow(1.0).min_w(0.0).child(
                text(value)
                    .key(key)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .truncate(),
            );
        };
        let disabled = vm.disabled;
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let recordset = recordset.clone();
        let mut field = TextField::new(key, edit)
            .text_style(k.ty.text_xsmall)
            .height(28.0)
            .disabled(disabled)
            .placeholder(t("transfer.mapping.recordsetUnbounded"));
        let edit_for_key = Rc::clone(&edit_fn);
        field = field.on_edit(move |s: &mut S| edit_for_key(s));
        field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
            if disabled {
                return;
            }
            let value = edit_fn(s).display().0.to_string();
            change(
                s,
                Some(apply_bound_update(
                    &recordset,
                    name,
                    RecordsetBoundPatch {
                        value: Some(value),
                        inclusive: None,
                    },
                )),
            );
        });
        col().grow(1.0).min_w(0.0).child(field.build(self.dark))
    }

    /// The inclusive checkbox for one bound. It only renders once the
    /// bound exists (React renders it under `{bound && ...}`), and
    /// toggling it re-commits the bound with the flipped flag — the
    /// blank-value rule in `apply_bound_update` cannot clear it here,
    /// because a bound with a value stays non-blank.
    fn inclusive_checkbox(
        &self,
        k: &'static rune_kit::Kit,
        key: String,
        checked: bool,
        target: InclusiveTarget,
        recordset: &TransferRecordset,
    ) -> El {
        let vm = &self.vm;
        let disabled = vm.disabled;
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let recordset = recordset.clone();
        let primary_keys = vm.primary_keys.clone();
        let mut checkbox = Checkbox::new(key, checked)
            .label(t("transfer.mapping.recordsetInclusive"))
            .disabled(disabled);
        checkbox = checkbox.on_toggle(move |s: &mut S| {
            if disabled {
                return;
            }
            let next = match target {
                InclusiveTarget::Single(name) => Some(apply_bound_update(
                    &recordset,
                    name,
                    RecordsetBoundPatch {
                        value: None,
                        inclusive: Some(!checked),
                    },
                )),
                InclusiveTarget::Tuple(name) => Some(apply_tuple_bound_update(
                    &recordset,
                    &primary_keys,
                    name,
                    RecordsetTupleBoundPatch {
                        values: None,
                        inclusive: Some(!checked),
                    },
                )),
            };
            change(s, next);
        });
        checkbox.build(k)
    }

    /// React 415–433: the limit label + number input.
    fn limit_row(&self, k: &'static rune_kit::Kit, recordset: &TransferRecordset) -> El {
        let vm = &self.vm;
        let disabled = vm.disabled;
        let mut row_el = row()
            .key("data-transfer-recordset-limit-row")
            .items_center()
            .gap(8.0)
            .w_full()
            .min_w(0.0);
        row_el = row_el.child(
            text(t("transfer.mapping.recordsetLimit"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
        let value = format_limit(recordset.limit);
        let (Some(edit), Some(edit_fn)) = (self.limit_edit, self.on_limit_edit.clone()) else {
            row_el = row_el.child(
                col().w(128.0).child(
                    text(value)
                        .key("data-transfer-recordset-limit")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_base)
                        .truncate(),
                ),
            );
            return row_el;
        };
        let change = self.on_change.clone().unwrap_or_else(|| Rc::new(|_, _| {}));
        let recordset = recordset.clone();
        let mut field = TextField::new("data-transfer-recordset-limit", edit)
            .text_style(k.ty.text_xsmall)
            .height(28.0)
            .disabled(disabled);
        let edit_for_key = Rc::clone(&edit_fn);
        field = field.on_edit(move |s: &mut S| edit_for_key(s));
        field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
            if disabled {
                return;
            }
            let raw = edit_fn(s).display().0.to_string();
            let mut next = recordset.clone();
            RecordsetPatch {
                limit: Some(parse_limit(&raw)),
                ..RecordsetPatch::default()
            }
            .apply_to(&mut next);
            change(s, Some(next));
        });
        row_el = row_el.child(col().w(128.0).child(field.build(self.dark)));
        row_el
    }
}

/// Which bound an inclusive checkbox belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InclusiveTarget {
    Single(BoundName),
    Tuple(BoundName),
}
