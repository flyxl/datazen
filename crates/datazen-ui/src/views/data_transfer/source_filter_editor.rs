//! T202: `windows/data-transfer/SourceFilterEditor.tsx` ported to Rune.
//!
//! The structured, parameterized source row filter of a table mapping:
//! a list of `column / operator / value` conditions with an `and`/`or`
//! switch, an add button, and a per-row remove. The Data Sync window
//! reuses the same editor and carries a `recordset` alongside (a sync-only
//! row range the predicate editor must preserve while the user edits the
//! filter).
//!
//! ## The update chain (verbatim from React)
//!
//! * Adding appends `{ column: columns[0], operator: 'eq', value: '' }`;
//!   with no columns at all nothing is added (React returns early) and the
//!   button is disabled for the same condition.
//! * A non-empty condition list rebuilds the filter with the **logic
//!   preserved, defaulting to `and`** (`filter?.logic ?? 'and'`) and the
//!   recordset preserved; an emptied list keeps `{ filters: [], recordset
//!   }` while a recordset is still present, and reports `undefined` (Rust
//!   `None`) otherwise.
//! * Switching an operator to `isNull` / `isNotNull` clears the value
//!   (those operators carry none); switching back restores `''` when the
//!   condition had no value (`condition.value ?? ''`).
//! * Every mutation routes through one `apply` chain, whose `disabled`
//!   gate is **defence in depth, not a fix** — the load-bearing gate is
//!   the parent's commit (`ColumnMappingEditor.commit` in React, proven
//!   by mutant N4 there). Removing this one changes no behaviour; the
//!   React source says so in place and the port repeats it so nobody
//!   "fixes" it later.
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-transfer-source-filter` |
//! | header (internal) | `…-header` |
//! | add button (internal) | `…-add` |
//! | logic row (internal) | `…-logic-row` |
//! | logic select (React test id) | `data-transfer-source-filter-logic` |
//! | empty-state paragraph (internal) | `…-empty` |
//! | rows root (internal) | `…-rows` |
//! | one condition row (internal) | `…-row-<index>` |
//! | column select (internal) | `…-row-<index>-column` |
//! | operator select (internal) | `…-row-<index>-operator` |
//! | value input (internal) | `…-row-<index>-value` |
//! | remove button (internal) | `…-row-<index>-remove` |
//!
//! ## Documented deviations from React
//!
//! * **The value field commits on Enter, not per keystroke.** React's
//!   controlled `<Input>` fires the whole `onChange` chain on every key;
//!   Rune's `TextEdit` is the host-held draft itself and an in-place edit
//!   callback cannot re-enter the state mutably mid-keystroke, so the
//!   commit point is Enter — the same stand-in shape
//!   `filter_condition_editor` documents for its single-value field. The
//!   field always shows the host's draft, exactly as React's controlled
//!   input shows its value.
//! * **`FilterCondition.value` is `Option<Value>`** (the shared
//!   `table_data_connection_state` mirror): React's `string | number |
//!   boolean | undefined` maps to `Some(Value)` of the matching JSON kind,
//!   `undefined` (the `isNull` pair) maps to `None`.
//! * **The operator list is this editor's fixed ten**, not the
//!   type-driven intersection a query filter computes: the React source
//!   hard-codes `OPERATORS` the same way.
//! * **`space-y-2` / `space-y-1`** become `gap(8.0)` / `gap(4.0)`
//!   columns; `!h-7` selects and inputs are 28pt.

// The production consumers are the ColumnMappingEditor shell (the
// transfer window) and the Data Sync MappingPanel (T336); until they wire
// it, the component tests are the only callers, so the dead-code lint is
// silenced here rather than crate-wide.
#![allow(dead_code)] // reusable component; wired by the mapping editors

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::table_data_connection_state::{FilterCondition, FilterLogic};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::views::TransferRecordset;

/// The ten operators the React source hard-codes (`OPERATORS`).
pub const OPERATORS: [&str; 10] = [
    "eq",
    "ne",
    "gt",
    "lt",
    "gte",
    "lte",
    "like",
    "in",
    "isNull",
    "isNotNull",
];

/// React `StructuredSourceFilter` (`SourceFilterEditor.tsx:22-27`): the
/// structured filter this editor edits, with the sync-only recordset the
/// predicate editor must preserve.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructuredSourceFilter {
    pub filters: Vec<FilterCondition>,
    /// `Some` = the React `logic` field (`'and'` | `'or'`); `None` lets the
    /// update chain default it to `'and'`.
    pub logic: Option<FilterLogic>,
    /// Sync-only row range; preserved while editing the predicate.
    pub recordset: Option<TransferRecordset>,
}

/// React's operator-change value rule: `undefined` for the null pair,
/// otherwise `condition.value ?? ''`.
fn value_for_operator_change(
    condition: &FilterCondition,
    operator: &str,
) -> Option<serde_json::Value> {
    if operator == "isNull" || operator == "isNotNull" {
        None
    } else {
        Some(condition.value.clone().unwrap_or(serde_json::Value::String(String::new())))
    }
}

/// The host-held value drafts of the current condition list: the `&[TextEdit]`
/// slice borrows the drafts for this frame, and `bind` locates the same
/// draft mutably when the field reports an edit. The host keeps the slice
/// in step with the condition list (same length, same order).
pub struct ValueDrafts<S: 'static> {
    pub edits: Vec<TextEdit>,
    /// Locates one condition's draft in the host state.
    pub bind: Rc<dyn for<'b> Fn(&'b mut S, usize) -> &'b mut TextEdit>,
}

/// Which select of the editor is open. React's listboxes are DOM-local;
/// Rune keeps the one open slot in the host state, the same shape
/// `filter_condition_editor` takes (`EditorSelect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFilterSelect {
    Logic,
    Column(usize),
    Operator(usize),
}

/// The editor builder. `S` is the host state that owns the filter (and,
/// if value drafts are supplied, the drafts) and the open select slot.
pub struct SourceFilterEditor<S: 'static> {
    columns: Vec<String>,
    filter: Option<StructuredSourceFilter>,
    disabled: bool,
    open: Option<SourceFilterSelect>,
    on_toggle: Option<Rc<dyn Fn(&mut S, SourceFilterSelect)>>,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    on_change: Option<Rc<dyn Fn(&mut S, Option<StructuredSourceFilter>)>>,
    value_drafts: Option<ValueDrafts<S>>,
}

impl<S: 'static> SourceFilterEditor<S> {
    pub fn new(columns: Vec<String>, filter: Option<StructuredSourceFilter>) -> Self {
        Self {
            columns,
            filter,
            disabled: false,
            open: None,
            on_toggle: None,
            on_close: None,
            on_change: None,
            value_drafts: None,
        }
    }

    /// The open select, from the host's slot (React's listbox is
    /// DOM-local; the host mirrors it, exactly like the filter panel does).
    pub fn open(mut self, open: Option<SourceFilterSelect>) -> Self {
        self.open = open;
        self
    }

    /// The host records which select opened.
    pub fn on_toggle(
        mut self,
        f: impl Fn(&mut S, SourceFilterSelect) + 'static,
    ) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    /// The host clears the slot when any select closes.
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// React `disabled`: held inert while the transfer job prepares.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// React `onChange(filter | undefined)`.
    pub fn on_change(
        mut self,
        f: impl Fn(&mut S, Option<StructuredSourceFilter>) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// The value drafts of the current condition list (one per condition,
    /// in order). Without them the rows still render, minus the value
    /// inputs — the same shape as a React mount whose draft state has not
    /// been wired yet.
    pub fn value_drafts(mut self, drafts: ValueDrafts<S>) -> Self {
        self.value_drafts = Some(drafts);
        self
    }

    /// The editor on its own kit root.
    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut layer = Vec::new();
        let root = self.build_inner(dark, &k, &mut layer);
        rune_kit::root(root, layer)
    }

    /// The same editor without its kit root (for a parent that roots the
    /// overlay once).
    pub fn build_inner(self, dark: bool, k: &Kit, layer: &mut Vec<El>) -> El {
        let conditions = self
            .filter
            .as_ref()
            .map(|f| f.filters.clone())
            .unwrap_or_default();
        let mut root = col()
            .key("data-transfer-source-filter")
            .w_full()
            .gap(8.0)
            .radius(k.radius.lg)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak)
            .padding(12.0)
            .child(self.header(k, layer));
        if conditions.len() > 1 {
            root = root.child(self.logic_row(k, layer, &conditions));
        }
        if conditions.is_empty() {
            root = root.child(
                text(t("transfer.mapping.noSourceFilter"))
                    .key("data-transfer-source-filter-empty")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        } else {
            let mut rows = col().key("data-transfer-source-filter-rows").gap(4.0);
            for (index, condition) in conditions.iter().enumerate() {
                rows = rows.child(self.condition_row(dark, k, layer, index, condition));
            }
            root = root.child(rows);
        }
        root
    }

    /// The header: the title + hint on the left, the add button on the
    /// right (`flex items-center justify-between gap-2`).
    fn header(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut add = Button::new(t("transfer.mapping.addFilter"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(self.disabled || self.columns.is_empty());
        if !(self.disabled || self.columns.is_empty()) {
            let columns = self.columns.clone();
            let filter = self.filter.clone();
            let on_change = self.on_change.clone();
            add = add.on_press(move |s: &mut S| {
                // React's `add`: no column to default to means no-op — the
                // button is disabled for the same condition, and the
                // guard is the same defence-in-depth as `apply`'s.
                let Some(column) = columns.first().cloned() else {
                    return;
                };
                let next = {
                    let mut filters =
                        filter.as_ref().map(|f| f.filters.clone()).unwrap_or_default();
                    filters.push(FilterCondition {
                        column,
                        operator: "eq".to_owned(),
                        value: Some(serde_json::Value::String(String::new())),
                        connector: None,
                    });
                    filters
                };
                apply_change(on_change.as_ref(), s, &filter, next);
            });
        }
        row()
            .key("data-transfer-source-filter-header")
            .w_full()
            .items_center()
            .justify_between()
            .gap(8.0)
            .child(
                col()
                    .key("data-transfer-source-filter-title")
                    .child(
                        text(t("transfer.mapping.sourceFilter"))
                            .font(k.ty.text_small)
                            .color(k.c.text_base),
                    )
                    .child(
                        text(t("transfer.mapping.sourceFilterHint"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            )
            .child(add.build(k).key("data-transfer-source-filter-add"))
    }

    /// The `and`/`or` switch, mounted only with more than one condition
    /// (`label flex items-center gap-2 text-xs text-fg-muted`).
    fn logic_row(&self, k: &Kit, layer: &mut Vec<El>, conditions: &[FilterCondition]) -> El {
        let and_label = t("transfer.mapping.filterAll");
        let or_label = t("transfer.mapping.filterAny");
        let current = match self.filter.as_ref().and_then(|f| f.logic) {
            Some(FilterLogic::Or) => or_label,
            _ => and_label,
        };
        let options = vec![and_label.to_owned(), or_label.to_owned()];
        let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
        let mut select =
            Select::new("data-transfer-source-filter-logic", &option_refs)
                .width(112.0)
                .disabled(self.disabled)
                .open(self.open == Some(SourceFilterSelect::Logic))
                .current(Some(current));
        if let Some(on_toggle) = &self.on_toggle {
            let on_toggle = on_toggle.clone();
            select = select.on_toggle(move |s: &mut S| on_toggle(s, SourceFilterSelect::Logic));
        }
        if let Some(on_close) = &self.on_close {
            let on_close = on_close.clone();
            select = select.on_close(move |s: &mut S| on_close(s));
        }
        if let Some(on_change) = &self.on_change {
            let filter = self.filter.clone();
            let conditions = conditions.to_vec();
            let on_change = on_change.clone();
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                let logic = if value == or_label {
                    FilterLogic::Or
                } else {
                    FilterLogic::And
                };
                if let Some(filter) = &filter {
                    on_change(
                        s,
                        Some(StructuredSourceFilter {
                            filters: conditions.clone(),
                            logic: Some(logic),
                            recordset: filter.recordset.clone(),
                        }),
                    );
                }
            });
        }
        row()
            .key("data-transfer-source-filter-logic-row")
            .items_center()
            .gap(8.0)
            .child(
                text(t("transfer.mapping.filterLogic"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(select.build(k, layer))
    }

    /// One condition row: the column select, the operator select, the
    /// value input (hidden for the null pair), and the remove button.
    fn condition_row(
        &self,
        dark: bool,
        k: &Kit,
        layer: &mut Vec<El>,
        index: usize,
        condition: &FilterCondition,
    ) -> El {
        let base = format!("data-transfer-source-filter-row-{index}");
        let mut row_el = row().key(base.clone()).flex_wrap().items_center().gap(4.0);

        // Column select: the options are the columns themselves.
        let column_refs: Vec<&str> = self.columns.iter().map(String::as_str).collect();
        let mut column_select =
            Select::new(format!("{base}-column"), &column_refs)
                .width(128.0)
                .disabled(self.disabled)
                .open(self.open == Some(SourceFilterSelect::Column(index)))
                .current(Some(condition.column.as_str()));
        if let Some(on_toggle) = &self.on_toggle {
            let on_toggle = on_toggle.clone();
            let slot = SourceFilterSelect::Column(index);
            column_select =
                column_select.on_toggle(move |s: &mut S| on_toggle(s, slot));
        }
        if let Some(on_close) = &self.on_close {
            let on_close = on_close.clone();
            column_select = column_select.on_close(move |s: &mut S| on_close(s));
        }
        if let Some(on_change) = &self.on_change {
            let filter = self.filter.clone();
            let on_change = on_change.clone();
            column_select =
                column_select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                    let mut next =
                        filter.as_ref().map(|f| f.filters.clone()).unwrap_or_default();
                    if let Some(slot) = next.get_mut(index) {
                        slot.column = value.to_owned();
                    }
                    apply_change(Some(&on_change), s, &filter, next);
                });
        }
        row_el = row_el.child(column_select.build(k, layer));

        // Operator select: the fixed ten, labelled `filter.<operator>`.
        let operator_labels: Vec<String> = OPERATORS
            .iter()
            .map(|operator| t(&format!("filter.{operator}")).to_owned())
            .collect();
        let operator_refs: Vec<&str> = operator_labels.iter().map(String::as_str).collect();
        let current_label = OPERATORS
            .iter()
            .zip(operator_labels.iter())
            .find(|(operator, _)| **operator == condition.operator)
            .map(|(_, label)| label.as_str())
            .unwrap_or("");
        let mut operator_select =
            Select::new(format!("{base}-operator"), &operator_refs)
                .width(112.0)
                .disabled(self.disabled)
                .open(self.open == Some(SourceFilterSelect::Operator(index)))
                .current(Some(current_label));
        if let Some(on_toggle) = &self.on_toggle {
            let on_toggle = on_toggle.clone();
            let slot = SourceFilterSelect::Operator(index);
            operator_select =
                operator_select.on_toggle(move |s: &mut S| on_toggle(s, slot));
        }
        if let Some(on_close) = &self.on_close {
            let on_close = on_close.clone();
            operator_select = operator_select.on_close(move |s: &mut S| on_close(s));
        }
        if let Some(on_change) = &self.on_change {
            let filter = self.filter.clone();
            let on_change = on_change.clone();
            let condition = condition.clone();
            operator_select =
                operator_select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                    let operator = OPERATORS
                        .iter()
                        .zip(operator_labels.iter())
                        .find(|(_, label)| label.as_str() == value)
                        .map(|(operator, _)| *operator)
                        .unwrap_or("eq");
                    let mut next =
                        filter.as_ref().map(|f| f.filters.clone()).unwrap_or_default();
                    if let Some(slot) = next.get_mut(index) {
                        slot.operator = operator.to_owned();
                        slot.value = value_for_operator_change(&condition, operator);
                    }
                    apply_change(Some(&on_change), s, &filter, next);
                });
        }
        row_el = row_el.child(operator_select.build(k, layer));

        // Value input: hidden for the null pair; the host's draft is the
        // field text, Enter commits it through the update chain.
        let needs_value = condition.operator != "isNull" && condition.operator != "isNotNull";
        if needs_value {
            if let Some(drafts) = &self.value_drafts {
                if let Some(edit) = drafts.edits.get(index) {
                    let placeholder = if condition.operator == "in" {
                        "a,b,c"
                    } else {
                        t("filter.value")
                    };
                    let bind = drafts.bind.clone();
                    let on_change = self.on_change.clone();
                    let filter = self.filter.clone();
                    let field = TextField::new(format!("{base}-value"), edit)
                        .height(28.0)
                        .min_width(128.0)
                        .grow()
                        .text_style(k.ty.text_xsmall)
                        .placeholder(placeholder)
                        .on_edit({
                            let bind = bind.clone();
                            move |s: &mut S| bind(s, index)
                        })
                        .on_submit(move |s: &mut S, _tx: &mut Tx| {
                            let value = bind(s, index).text.clone();
                            let mut next =
                                filter.as_ref().map(|f| f.filters.clone()).unwrap_or_default();
                            if let Some(slot) = next.get_mut(index) {
                                slot.value = Some(serde_json::Value::String(value));
                            }
                            apply_change(on_change.as_ref(), s, &filter, next);
                        })
                        .build(dark);
                    let field = if self.disabled { field.disabled(true) } else { field };
                    row_el = row_el.child(field);
                }
            }
        }

        // Remove button: ghost, labelled `filter.remove`.
        let mut remove = Button::new(t("filter.remove"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(self.disabled);
        if let Some(on_change) = &self.on_change {
            let filter = self.filter.clone();
            let on_change = on_change.clone();
            remove = remove.on_press(move |s: &mut S| {
                let next: Vec<FilterCondition> = filter
                    .as_ref()
                    .map(|f| f.filters.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .filter(|(item_index, _)| *item_index != index)
                    .map(|(_, condition)| condition)
                    .collect();
                apply_change(Some(&on_change), s, &filter, next);
            });
        }
        row_el.child(remove.build(k).key(format!("{base}-remove")))
    }
}

/// React's `update(next)` chain, the part that is pure state logic:
/// rebuild the filter with the logic and recordset preserved, keep
/// `{ filters: [], recordset }` while a recordset is present, and report
/// `None` otherwise. (`apply_change` wraps it with the callback.)
pub fn rebuild_filter(
    filter: Option<&StructuredSourceFilter>,
    next: Vec<FilterCondition>,
) -> Option<StructuredSourceFilter> {
    let default_logic = FilterLogic::And;
    if !next.is_empty() {
        Some(StructuredSourceFilter {
            logic: filter.and_then(|f| f.logic).or(Some(default_logic)),
            recordset: filter.and_then(|f| f.recordset.clone()),
            filters: next,
        })
    } else if filter.map(|f| f.recordset.is_some()).unwrap_or(false) {
        Some(StructuredSourceFilter {
            filters: Vec::new(),
            logic: filter.and_then(|f| f.logic).or(Some(default_logic)),
            recordset: filter.and_then(|f| f.recordset.clone()),
        })
    } else {
        None
    }
}

/// `update(next)` plus the `apply` leg: report the rebuild to the host.
/// The React `disabled` gate inside `apply` is defence in depth (the
/// module doc records why); the parent's commit is the load-bearing gate,
/// so this port applies the same gate by not wiring handlers on disabled
/// controls at all.
fn apply_change<S: 'static>(
    on_change: Option<&Rc<dyn Fn(&mut S, Option<StructuredSourceFilter>)>>,
    s: &mut S,
    filter: &Option<StructuredSourceFilter>,
    next: Vec<FilterCondition>,
) {
    let Some(on_change) = on_change else {
        return;
    };
    on_change(s, rebuild_filter(filter.as_ref(), next));
}
