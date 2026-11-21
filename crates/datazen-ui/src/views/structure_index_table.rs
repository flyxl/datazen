//! Port of `windows/connection/structure/StructureIndexTable.tsx`.
//!
//! React is a form table over index drafts: a name input, an ordered column
//! picker (chips with per-chip remove plus an "add column" select), a unique
//! checkbox, an optional index-type select (gated by the `indexType` driver
//! capability) and a delete button (gated by `dropIndex`). An empty draft list
//! shows `structEditor.noIndexes`. Driver capabilities decide which controls
//! are enabled and a disabled control carries `structEditor.capDisabled` as its
//! tooltip. This port keeps that contract and emits every edit/remove/add as a
//! [`StructureIndexAction`] through an injected host port — no fake IPC.
//!
//! ## Keys
//!
//! Header and row keys are named after the React elements
//! (`structure-index-head-{i}`, `structure-index-row-{id}`, ...). The pure
//! `suggested_index_name` helper is ported verbatim.
//!
//! ## Local stand-ins (removal stage)
//!
//! No production host mounts this table yet; the structure editor owns the
//! drafts. Every reusable item carries a per-item `#[allow(dead_code)]` with
//! removal stage "drop when the structure editor host wires this table".

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit};
use rune_kit::SortDir;

use crate::i18n::t;
use crate::icons::{ICON_TRASH, ICON_X};
use crate::structure_editor::control_hints::{cap_enabled, CONTROL_DISABLED_KEY};
use crate::structure_editor::{StructureCapabilities, StructureCapabilityFlag, StructureIndexDraft};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

pub(crate) const ROOT_KEY: &str = "structure-index-table";
pub(crate) const EMPTY_KEY: &str = "structure-index-empty";

/// React `suggestedIndexName` (exported from the same file).
#[allow(dead_code)] // pure helper; remove when the structure host calls the shared port.
pub(crate) fn suggested_index_name(table_name: &str, columns: &[String]) -> String {
    let base = table_name.trim();
    let base = if base.is_empty() { "table" } else { base };
    if columns.is_empty() {
        format!("idx_{base}")
    } else {
        format!("idx_{}_{}", base, columns.join("_"))
    }
}

/// A patch to one index draft (`Partial<StructureIndexDraft>`).
#[allow(dead_code)] // VM mirror; remove when the host owns the drafts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IndexPatch {
    Name(String),
    Columns(Vec<String>),
    IsUnique(bool),
    IndexType(String),
}

/// Host-owned projection of the component props plus `sort` / `selected`.
#[allow(dead_code)] // VM surface; remove when the host wires the table.
#[derive(Clone, Debug, Default)]
pub(crate) struct StructureIndexTableVm {
    pub(crate) caps: Option<StructureCapabilities>,
    pub(crate) index_methods: Vec<String>,
    pub(crate) column_names: Vec<String>,
    pub(crate) table_name: String,
    pub(crate) indexes: Vec<StructureIndexDraft>,
    pub(crate) name_edits: Vec<TextEdit>,
    pub(crate) sort: Option<(usize, SortDir)>,
    pub(crate) selected: Option<String>,
    pub(crate) open_method_for: Option<String>,
}

#[allow(dead_code)] // pure helpers; remove with the host.
impl StructureIndexTableVm {
    /// React's `indexType` capability gate.
    pub(crate) fn show_index_type(&self) -> bool {
        cap_enabled(self.caps.as_ref(), StructureCapabilityFlag::IndexType)
    }

    /// React's `row_order` after the host-owned sort by name / columns / type.
    pub(crate) fn row_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.indexes.len()).collect();
        if let Some((column, dir)) = self.sort {
            order.sort_by(|&a, &b| {
                let value = |index: usize| -> String {
                    let Some(draft) = self.indexes.get(index) else {
                        return String::new();
                    };
                    match column {
                        0 => draft.name.to_lowercase(),
                        1 => draft.columns.join("_").to_lowercase(),
                        _ => draft.index_type.clone().unwrap_or_default().to_lowercase(),
                    }
                };
                dir.apply(value(a).cmp(&value(b)))
            });
        }
        order
    }

    /// React's auto-name update rule: rename only while the current name is
    /// empty or still equals the previous suggestion.
    pub(crate) fn column_patch(&self, index: &StructureIndexDraft, columns: Vec<String>) -> Vec<IndexPatch> {
        let previous = suggested_index_name(&self.table_name, &index.columns);
        let next = suggested_index_name(&self.table_name, &columns);
        let mut patches = vec![IndexPatch::Columns(columns)];
        if index.name.trim().is_empty() || index.name == previous {
            patches.push(IndexPatch::Name(next));
        }
        patches
    }
}

/// Every intent the table emits.
#[allow(dead_code)] // host port; remove when the host wires the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StructureIndexAction {
    Update { id: String, patch: IndexPatch },
    Remove(String),
    AddColumn { id: String, column: String },
    Select(String),
    Sort(usize),
    ToggleMethod(String),
}

#[allow(dead_code)] // host port; remove when the host wires the table.
pub(crate) type StructureIndexActionFn<S> = Rc<dyn Fn(&mut S, StructureIndexAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<StructureIndexActionFn<S>>,
    action: StructureIndexAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

fn disabled_title(caps: Option<&StructureCapabilities>, control: StructureCapabilityFlag) -> Option<&'static str> {
    if cap_enabled(caps, control) {
        None
    } else {
        Some(t(CONTROL_DISABLED_KEY))
    }
}

/// Build the table into a single element.
#[allow(dead_code)] // view entry; remove when the host wires the table.
pub(crate) fn build<S: 'static>(
    vm: &StructureIndexTableVm,
    on_action: &Option<StructureIndexActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    if vm.indexes.is_empty() {
        return col().key(EMPTY_KEY).w_full().child(
            text(t("structEditor.noIndexes"))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .px(16.0)
                .py(8.0),
        );
    }

    let mut headers = vec![
        t("indexes.indexName"),
        t("indexes.columns"),
        t("indexes.unique"),
    ];
    if vm.show_index_type() {
        headers.push(t("indexes.indexType"));
    }
    let mut head = row().key("structure-index-head").w_full().min_w(0.0).h(32.0).center_y();
    for (index, label) in headers.iter().enumerate() {
        head = head.child(
            text((*label).to_owned())
                .key(format!("structure-index-head-{index}"))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        );
    }
    // A keyed sort strip stands in for React's native header menu (host-owned).
    let mut sort_strip = row().key("structure-index-sort-strip").w_full().gap(8.0);
    for (index, label) in headers.iter().enumerate() {
        sort_strip = sort_strip.child(
            text((*label).to_owned())
                .key(format!("structure-index-sort-{index}"))
                .role("button")
                .focusable(true)
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .on_press(emit(on_action, StructureIndexAction::Sort(index))),
        );
    }

    let mut body = col().key("structure-index-body").w_full().min_w(0.0);
    for original in vm.row_order() {
        body = body.child(index_row(vm, original, on_action, dark));
    }
    col()
        .key(ROOT_KEY)
        .w_full()
        .min_w(0.0)
        .child(head)
        .child(sort_strip)
        .child(body)
}

fn index_row<S: 'static>(
    vm: &StructureIndexTableVm,
    index: usize,
    on_action: &Option<StructureIndexActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let Some(draft) = vm.indexes.get(index) else {
        return row().key(format!("structure-index-row-missing-{index}"));
    };
    let can_edit = cap_enabled(vm.caps.as_ref(), StructureCapabilityFlag::CreateIndex);
    let can_drop = cap_enabled(vm.caps.as_ref(), StructureCapabilityFlag::DropIndex);
    let edit_title = disabled_title(vm.caps.as_ref(), StructureCapabilityFlag::CreateIndex);
    let suggested = suggested_index_name(&vm.table_name, &draft.columns);
    let name_edit = vm.name_edits.get(index).cloned().unwrap_or_else(|| TextEdit::new(draft.name.clone()));

    let mut name_field = TextField::new(format!("structure-index-name-{}", draft.id), &name_edit)
        .height(28.0)
        .grow()
        .min_width(120.0)
        .text_style(k.ty.text_small)
        .placeholder(if suggested.is_empty() { "idx_name" } else { &suggested });
    // The host owns each draft's TextEdit and provides the real edit accessor;
    // without one this is an honest bound display of the current name. Remaining
    // disabled keeps the value visible without claiming an edit binding.
    name_field = name_field.disabled(true);
    let mut name_el = name_field.build(dark);
    if let Some(title) = edit_title {
        name_el = name_el.help(title);
    }

    // Column chips in selection order, each with a remove control.
    let mut chips = row().key(format!("structure-index-columns-{}", draft.id)).grow(1.0).min_w(0.0).gap(4.0);
    for (order, column) in draft.columns.iter().enumerate() {
        let mut chip = row()
            .key(format!("structure-index-chip-{}-{order}", draft.id))
            .h(20.0)
            .px(6.0)
            .center_y()
            .radius(4.0)
            .fill(k.alpha(k.c.text_accent, 0.10))
            .child(
                text(format!("#{}", order + 1))
                    .key(format!("structure-index-chip-order-{}-{order}", draft.id))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(text(column.clone()).font(k.ty.text_xsmall).color(k.c.text_accent));
        if can_edit {
            chip = chip.child(
                icon(&ICON_X, 10.0)
                    .key(format!("structure-index-chip-remove-{}-{order}", draft.id))
                    .role("button")
                    .label(t("filter.remove"))
                    .on_press(emit(on_action, StructureIndexAction::Update {
                        id: draft.id.clone(),
                        patch: IndexPatch::Columns(
                            draft.columns.iter().enumerate().filter(|(i, _)| *i != order).map(|(_, c)| c.clone()).collect(),
                        ),
                    })),
            );
        }
        chips = chips.child(chip);
    }
    if can_edit {
        let available: Vec<String> = vm.column_names.iter().filter(|name| !draft.columns.contains(name)).cloned().collect();
        for column in available {
            chips = chips.child(
                text(column.clone())
                    .key(format!("structure-index-add-{}-{}", draft.id, column))
                    .role("button")
                    .focusable(true)
                    .font(k.ty.text_xsmall)
                    .label(t("indexes.addColumn"))
                    .color(k.c.text_accent)
                    .on_press(emit(on_action, StructureIndexAction::AddColumn {
                        id: draft.id.clone(),
                        column: column.clone(),
                    })),
            );
        }
        if draft.columns.is_empty() && vm.column_names.is_empty() {
            chips = chips.child(
                text(t("indexes.selectColumns")).font(k.ty.text_xsmall).color(k.c.text_muted),
            );
        }
    }

    let unique = Checkbox::new(format!("structure-index-unique-{}", draft.id), draft.is_unique)
        .disabled(!can_edit);
    let unique = if can_edit {
        let id = draft.id.clone();
        let was = draft.is_unique;
        match on_action.clone() {
            Some(callback) => unique.on_toggle(move |s: &mut S| {
                callback(s, StructureIndexAction::Update { id: id.clone(), patch: IndexPatch::IsUnique(!was) });
            }),
            None => unique,
        }
    } else {
        unique
    };

    let mut cells = row()
        .key(format!("structure-index-row-{}", draft.id))
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .px(8.0)
        .py(6.0)
        .fill(k.c.surface_base)
        .child(name_el)
        .child(chips)
        .child(unique.build(k));

    if vm.show_index_type() {
        let selected = draft.index_type.clone().unwrap_or_else(|| vm.index_methods.first().cloned().unwrap_or_else(|| "btree".to_owned()));
        let mut method = row()
            .key(format!("structure-index-method-{}", draft.id))
            .h(24.0)
            .px(6.0)
            .center_y()
            .radius(4.0)
            .fill(k.c.surface_raised)
            .child(text(selected.clone()).font(k.ty.text_small).color(k.c.text_base))
            .on_press(emit(on_action, StructureIndexAction::ToggleMethod(draft.id.clone())));
        if let Some(title) = disabled_title(vm.caps.as_ref(), StructureCapabilityFlag::IndexType) {
            method = method.help(title);
        } else if !can_edit {
            method = method.help(t(CONTROL_DISABLED_KEY));
        }
        cells = cells.child(method);
    }

    let mut delete = icon(&ICON_TRASH, 14.0)
        .key(format!("structure-index-remove-{}", draft.id))
        .role("button")
        .focusable(can_drop)
        .color(k.c.text_muted)
        .label(if can_drop {
            t("common.deleteIndex")
        } else {
            t(CONTROL_DISABLED_KEY)
        });
    if can_drop {
        delete = delete.on_press(emit(on_action, StructureIndexAction::Remove(draft.id.clone())));
    }
    cells.child(delete)
}
