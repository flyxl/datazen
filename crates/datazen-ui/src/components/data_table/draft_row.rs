//! T257: the new-row draft row, ported from
//! `packages/frontend/src/components/DataTable/DraftRow.tsx` (moved out of
//! `VirtualBody.tsx` when DB-03 hit its size ceiling — the body is moved,
//! not rewritten: same markup, same attributes, same conditions).
//!
//! One draft (DB-04) is deliberately **outside** the cell-selection
//! contract: a draft has no identity and is not a row of the table yet,
//! so it must not answer the grid's selection queries (04 §3.7 R-6). It
//! carries its own `data-row-kind="draft"` marker (the root key here) and
//! its own `draft-row-<N>` testid, and the grid renders it after the
//! real rows, above the table's own bottom edge.
//!
//! | React piece | here |
//! |---|---|
//! | `DraftRowProps` | the [`DraftRow`] builder |
//! | `draft` (`DraftRowViewModel`) | [`PendingInsert`] (`draft_id` + the written `values`; a missing key is the unwritten third state, `Value::Null` an explicit NULL) |
//! | `vRow.start` / `style.top` | [`DraftRow::row_start`] through [`El::absolute`] |
//! | the badge `NEW` / the `DEFAULT` placeholder / the unsaved / the discard labels | `tableData.draftRowBadge` / `insertDefault` / `draftRowUnsaved` / `removeDraftRow`, straight from the packs |
//! | `written` third state (React 109–112) | [`draft_state`] — `Default`/`Null`/`Value`, the cell's own content switch |
//! | the written cell (`CellRenderer` + NOOP commits) | [`CellRenderer`] with no callbacks, exactly React's `NOOP`s |
//! | `onDoubleClick` per cell (React 124–127) | [`El::on_double_press`] — and [`El::on_press`] with the row's own activate, because React's single click bubbles to the row while Rune hands a press to the deepest handler: the cell carries both so a click activates and a double-click edits |
//! | the remove `×` button's `stopPropagation` (React 100–103) | nothing to stop: the button is the deepest handler under the point, so the row's press never sees it |
//!
//! ## Key convention
//!
//! | part | key |
//! |---|---|
//! | root (`data-testid="draft-row-N"` + `data-draft-id`) | `<key>` |
//! | badge (`draft-row-badge`) | `<key>-badge` |
//! | remove (`draft-row-remove`) | `<key>-remove` |
//! | one cell (`draft-cell` + `data-draft-col`) | `<key>-cell-<col.id>` |
//! | the DEFAULT placeholder (`draft-cell-default`) | `<key>-cell-<col.id>-default` |
//! | the editing stand-in | `<key>-cell-<col.id>-editing` |
//!
//! ## Deliberate boundaries
//!
//! * **`EditableCell` awaits T278** (same segment): while a cell is being
//!   edited the port mounts a stand-in — the raw value's seed text, in the
//!   cell's own typography, keyed `-editing` — and the commit seam
//!   ([`DraftRow::on_draft_cell_edit`], the `DraftCellEdit` three-state)
//!   stays the public API. T278's port replaces the stand-in with the real
//!   per-type editor and threads its commit into that seam; the cancel
//!   seam ([`DraftRow::on_draft_cell_edit_cancel`]) is likewise wired then.
//! * `hover:text-danger` on the remove button is a text colour Rune's
//!   hover does not carry ([`El::hover`] is a fill); the affordance is the
//!   `surface-raised` fill. Documented rather than approximated.
//! * `tabIndex={-1}` (React 95) is `El::focusable(false)`: the discard
//!   button stays off the tab ring while keeping its press.
//! * React's `col.id` cell keys become the `-cell-<id>` suffix; the
//!   `data-draft-col` name stays implicit in the column list the caller
//!   owns.

use std::collections::HashMap;
use std::rc::Rc;

use rune::{row, text, El, TextStyle};

use crate::components::data_table::cell_renderer::{
    js_string, CellPresentationVm, CellRenderer, CellRendererVm,
};
use crate::components::data_table::table_header::Column;
use crate::i18n::t;
use crate::table_data_connection_state::PendingInsert;
use crate::ui::kit::kit;

/// React `editing: { draftId: string; column: string } | null` — the
/// draft cell the editor is open on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftCellEditing {
    pub(crate) draft_id: String,
    pub(crate) column: String,
}

/// React `onDraftCellEdit`'s value argument: `None` is the `undefined`
/// that **clears** the column (the editor's empty commit — React 138's
/// `committed === null` translation, "do not write it"); `Some(Value::Null)`
/// is an explicit NULL write; anything else writes the value.
pub(crate) type DraftCellEdit = Option<serde_json::Value>;

/// The three states of one cell's write intent (React 121:
/// `data-draft-state`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftCellState {
    /// The column is not written at all — the cell shows the DEFAULT
    /// placeholder.
    Default,
    /// The column is written, with an explicit NULL.
    Null,
    /// The column is written, with a value.
    Value,
}

/// The cell's state from the draft's write intent (React 109–112).
fn draft_state(written: Option<&serde_json::Value>) -> DraftCellState {
    match written {
        None => DraftCellState::Default,
        Some(serde_json::Value::Null) => DraftCellState::Null,
        Some(_) => DraftCellState::Value,
    }
}

/// One new-row draft row (DB-04). `S` is the host state the callbacks
/// run against.
#[allow(dead_code)] // ported base; remove when the VirtualBody call site lands
pub(crate) struct DraftRow<S: 'static> {
    key: String,
    draft: PendingInsert,
    columns: Vec<Column>,
    row_start: f32,
    row_height: f32,
    column_widths: Option<Vec<f32>>,
    editing: Option<DraftCellEditing>,
    cell_selection_enabled: bool,
    presentation_plans: HashMap<String, CellPresentationVm>,
    on_draft_cell_edit: Option<Rc<dyn Fn(&mut S, &str, &str, DraftCellEdit)>>,
    on_draft_cell_edit_start: Option<Rc<dyn Fn(&mut S, &str, &str)>>,
    on_draft_cell_edit_cancel: Option<Rc<dyn Fn(&mut S)>>,
    on_draft_row_remove: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_draft_row_activate: Option<Rc<dyn Fn(&mut S, &str)>>,
}

#[allow(dead_code)] // ported base; remove when the VirtualBody call site lands
impl<S: 'static> DraftRow<S> {
    /// The draft row `draft-row-<N>` — `key` is React's whole
    /// `data-testid` (the `data-draft-id` rides the draft itself).
    /// `row_start` is React's `vRow.start` (the `style.top`); `vRow.key`
    /// and `vRow.index` never reach the markup and stay with the
    /// virtualizer.
    pub(crate) fn new(
        key: impl Into<String>,
        draft: PendingInsert,
        row_start: f32,
        row_height: f32,
    ) -> Self {
        Self {
            key: key.into(),
            draft,
            columns: Vec::new(),
            row_start,
            row_height,
            column_widths: None,
            editing: None,
            cell_selection_enabled: false,
            presentation_plans: HashMap::new(),
            on_draft_cell_edit: None,
            on_draft_cell_edit_start: None,
            on_draft_cell_edit_cancel: None,
            on_draft_row_remove: None,
            on_draft_row_activate: None,
        }
    }

    /// React `columns` — the grid's own column list, in order.
    pub(crate) fn columns(mut self, columns: Vec<Column>) -> Self {
        self.columns = columns;
        self
    }

    /// React `columnWidths` — one width per column; a missing entry (or
    /// the whole list) falls back to React's 160.
    pub(crate) fn column_widths(mut self, widths: Option<Vec<f32>>) -> Self {
        self.column_widths = widths;
        self
    }

    /// React `editing` — the draft cell the editor is open on.
    pub(crate) fn editing(mut self, editing: Option<DraftCellEditing>) -> Self {
        self.editing = editing;
        self
    }

    /// React `cellSelectionEnabled` — while on, the row and its lead
    /// cell carry the ARIA grid roles; while off, no role at all.
    pub(crate) fn cell_selection_enabled(mut self, enabled: bool) -> Self {
        self.cell_selection_enabled = enabled;
        self
    }

    /// React `presentationPlanByColumn` — only a **written** cell consults
    /// its plan (the DEFAULT placeholder and the editing stand-in never
    /// see one; React 30–35).
    pub(crate) fn presentation_plans(mut self, plans: HashMap<String, CellPresentationVm>) -> Self {
        self.presentation_plans = plans;
        self
    }

    /// React `onDraftCellEdit(draftId, column, value)` — the commit seam
    /// T278's `EditableCell` lands on.
    pub(crate) fn on_draft_cell_edit(
        mut self,
        f: impl Fn(&mut S, &str, &str, DraftCellEdit) + 'static,
    ) -> Self {
        self.on_draft_cell_edit = Some(Rc::new(f));
        self
    }

    /// React `onDraftCellEditStart(draftId, column)` — the cell's
    /// double-press.
    pub(crate) fn on_draft_cell_edit_start(
        mut self,
        f: impl Fn(&mut S, &str, &str) + 'static,
    ) -> Self {
        self.on_draft_cell_edit_start = Some(Rc::new(f));
        self
    }

    /// React `onDraftCellEditCancel` — the editor's escape; wired with
    /// T278's editor.
    pub(crate) fn on_draft_cell_edit_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_draft_cell_edit_cancel = Some(Rc::new(f));
        self
    }

    /// React `onDraftRowRemove(draftId)` — the `×`.
    pub(crate) fn on_draft_row_remove(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_draft_row_remove = Some(Rc::new(f));
        self
    }

    /// React `onDraftRowActivate(draftId)` — any press that reaches the
    /// row itself (React: the click that bubbles out of the cells).
    pub(crate) fn on_draft_row_activate(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_draft_row_activate = Some(Rc::new(f));
        self
    }

    /// Paint the row: the lead cell (badge + discard), then one cell per
    /// column in the grid's order.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let Self {
            key,
            draft,
            columns,
            row_start,
            row_height,
            column_widths,
            editing,
            cell_selection_enabled,
            presentation_plans,
            on_draft_cell_edit: _,
            on_draft_cell_edit_start,
            on_draft_cell_edit_cancel: _,
            on_draft_row_remove,
            on_draft_row_activate,
        } = self;

        // React 69–73: the four labels, straight from the packs.
        let badge_label = t("tableData.draftRowBadge");
        let default_label = t("tableData.insertDefault");
        let unsaved_label = t("tableData.draftRowUnsaved");
        let remove_label = t("tableData.removeDraftRow");

        // React 90: the badge's 10-point semibold.
        let f10_semibold = TextStyle {
            size: 10.0,
            weight: 600.0,
            line_height: 13.0,
            ..k.ty.text_xsmall
        };

        // ---- the lead cell (React 86–107): badge + discard button ----
        let mut lead = row()
            .key(format!("{key}-lead"))
            .w(40.0)
            .items_center()
            .justify_center()
            .gap(2.0)
            .border_sides([0.0, 1.0, 0.0, 0.0], k.alpha(k.c.border_base, 0.3))
            .child(
                text(badge_label)
                    .key(format!("{key}-badge"))
                    .font(f10_semibold)
                    .color(k.c.text_accent),
            );
        if let Some(on_draft_row_remove) = on_draft_row_remove.clone() {
            let draft_id = draft.draft_id.clone();
            // React 93–106: the `×`; its press never reaches the row
            // (stopPropagation there, deepest-handler here).
            lead = lead.child(
                row()
                    .key(format!("{key}-remove"))
                    .role("button")
                    .label(remove_label)
                    .w(14.0)
                    .h(14.0)
                    .items_center()
                    .justify_center()
                    .radius(4.0)
                    .focusable(false)
                    .hover(k.c.surface_raised)
                    .child(
                        text("×")
                            .key(format!("{key}-remove-glyph"))
                            .font(f10_semibold)
                            .color(k.c.text_muted),
                    )
                    .on_press(move |state: &mut S| on_draft_row_remove(state, &draft_id)),
            );
        }
        if cell_selection_enabled {
            lead = lead.role("rowheader");
        }

        // ---- the cells (React 108–163) ----
        let mut cells = Vec::with_capacity(columns.len());
        for (col_idx, col) in columns.iter().enumerate() {
            // React 109–112: the third state of the write intent.
            let written = draft.values.get(&col.name);
            let state = draft_state(written);
            let is_editing = editing
                .as_ref()
                .is_some_and(|e| e.draft_id == draft.draft_id && e.column == col.name);
            // React 115: `columnWidths?.[colIdx] ?? 160`.
            let col_w = column_widths
                .as_ref()
                .and_then(|w| w.get(col_idx).copied())
                .unwrap_or(160.0);
            let cell_key = format!("{key}-cell-{}", col.id);

            // The press pair: a click activates the draft (React's bubbling
            // onClick) and a double-press opens the editor.
            let mut cell = row()
                .key(cell_key.clone())
                .w(col_w)
                .items_center()
                .border_sides([0.0, 1.0, 0.0, 0.0], k.alpha(k.c.border_base, 0.3))
                .px(8.0);
            if let Some(on_activate) = on_draft_row_activate.clone() {
                let draft_id = draft.draft_id.clone();
                cell = cell.on_press(move |state: &mut S| {
                    on_activate(state, &draft_id)
                });
            }
            if let Some(on_start) = on_draft_cell_edit_start.clone() {
                let draft_id = draft.draft_id.clone();
                let column = col.name.clone();
                cell = cell.on_double_press(move |state: &mut S| {
                    on_start(state, &draft_id, &column)
                });
            }

            // React 129–160: the three contents.
            let content = if is_editing {
                // The stand-in until T278: the editor's seed — the raw
                // value's text, empty for the unwritten and NULL.
                let seed = match written {
                    Some(serde_json::Value::Null) | None => String::new(),
                    Some(value) => js_string(value),
                };
                text(seed)
                    .key(format!("{cell_key}-editing"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .truncate()
            } else if let Some(value) = written {
                // The written cell: the same renderer the grid reads, with
                // React's NOOP commits (no callbacks here at all).
                let mut vm = CellRendererVm::new(cell_key.clone(), col.name.clone(), value.clone());
                if let Some(ty) = col.ty.clone() {
                    vm = vm.data_type(ty);
                }
                if let Some(plan) = presentation_plans.get(&col.name) {
                    vm = vm.presentation_plan(plan.clone());
                }
                CellRenderer::<S>::new(vm).build(dark)
            } else {
                // The DEFAULT placeholder: the italic muted 12-point hint.
                text(default_label)
                    .key(format!("{cell_key}-default"))
                    .font(k.ty.text_xsmall.italic())
                    .color(k.c.text_muted)
                    .truncate()
            };
            cells.push(cell.child(content));
        }

        // ---- the row (React 76–85) ----
        let mut root = row()
            .key(key)
            .absolute(0.0, row_start)
            .w_full()
            .h(row_height)
            .fill(k.alpha(k.c.text_accent, 0.05))
            .border_sides([0.0, 0.0, 1.0, 0.0], k.alpha(k.c.text_accent, 0.40))
            .border_dash(4.0, 4.0)
            .label(unsaved_label)
            .child(lead);
        if let Some(on_activate) = on_draft_row_activate.clone() {
            let draft_id = draft.draft_id.clone();
            root = root.on_press(move |state: &mut S| {
                on_activate(state, &draft_id)
            });
        }
        if cell_selection_enabled {
            root = root.role("row");
        }
        for cell in cells {
            root = root.child(cell);
        }
        root
    }
}
