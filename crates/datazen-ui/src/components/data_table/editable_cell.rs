//! The compatibility shell of the old editor, ported from React
//! `packages/frontend/src/components/DataTable/EditableCell.tsx`
//! (T278).
//!
//! The editor itself moved to `editors/`: one file per control family
//! and one place that decides **when** a value is written
//! (`editors/CellEditorHost.tsx`). What is left here is the entry
//! point the new-row drafts still use, plus the re-export of
//! `toEditString`, whose import path other code depends on.
//!
//! The old editor had three defects that the new one does not have: it
//! committed an IME candidate word when the user pressed `Enter`, it
//! wrote the raw text into a JSON column when the text did not parse,
//! and it turned every empty input into a `NULL` on a `NOT NULL`
//! column.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `export { toEditString }` | the re-export through this module's docs — callers import the support module directly |
//! | `resolveCellType(type)` | [`crate::support::cell_types::resolve_cell_type`] |
//! | `toEditString(value)` / `countCodePoints(text)` | [`crate::support::to_edit_string`]'s pair |
//! | `resolveCellEditorKind({…})` (React 39-45) | [`EditableCellKind`] — the kind derivation, real logic |
//! | `<CellEditorHost …>` (React 48-59) | the T297 seam — the host mounts the per-type controls; the two families this task ports (binary, boolean) land in `editors/`, the other six with their owning tasks |
//! | `onCommit` / `onCancel` (React 56-57) | held here, forwarded when the host lands |
//! | `panelId` (the navigation guard, 05 §4.4) | the host's registration (T297); held here |
//!
//! ## The kind derivation (React 39-45)
//!
//! [`EditableCellKind::of`] is the shell's whole computation: the cell
//! type, the edit string, and the kind over `hasCandidates: false`
//! (the shell has no candidates), the code-point count, and the line
//! break. It is the real logic — the seam only mounts.
//!
//! ## Documented deviations from React
//!
//! * **The host is a seam.** `CellEditorHost` is T297 (this segment,
//!   pending): the port renders a labelled placeholder naming it,
//!   carrying the kind derivation's result on its label so the wiring
//!   task can assert what the host will be asked to mount. The commit
//!   and cancel channels stay the public API (the DraftRow seam's
//!   contract, `on_draft_cell_edit` / `on_draft_cell_edit_cancel`).
//! * **The draft-row stand-in replacement completes at T297**: the
//!   stand-in keeps its contract until the host can mount per-type
//!   controls, because that is when the shell's delegation becomes
//!   real. The stand-in's own docs say so.

use std::rc::Rc;

use rune::{col, text, El};
use rune_kit::Kit;
use serde_json::Value;

use crate::support::cell_editor_kind::{resolve_cell_editor_kind, CellEditorKind};
use crate::support::cell_types::{is_zoned_cell_type, resolve_cell_type, CellType};
use crate::support::to_edit_string::{count_code_points, to_edit_string};

/// The shell's kind derivation (React 37-45): the cell type, the edit
/// string, and the resolved editor kind over the shell's fixed
/// `hasCandidates: false`.
#[derive(Clone, Debug)]
pub(crate) struct EditableCellKind {
    pub(crate) cell_type: CellType,
    /// `toEditString(value)` — the seed text the host's control starts
    /// from.
    pub(crate) text: String,
    pub(crate) kind: CellEditorKind,
}

impl EditableCellKind {
    /// React 37-45. `data_type` is the column's type string; `value`
    /// the cell's current value.
    pub(crate) fn of(value: &Value, data_type: &str) -> Self {
        let cell_type = resolve_cell_type(Some(data_type), None);
        let text = to_edit_string(value);
        let kind = resolve_cell_editor_kind(crate::support::cell_editor_kind::CellEditorKindInput {
            cell_type,
            zoned: is_zoned_cell_type(data_type),
            has_candidates: false,
            value_code_points: count_code_points(&text),
            has_line_break: text.contains('\n') || text.contains('\r'),
        });
        Self {
            cell_type,
            text,
            kind,
        }
    }
}

/// `EditableCell`: the compatibility shell — the kind derivation plus
/// the T297 host seam.
pub(crate) struct EditableCell<S: 'static> {
    value: Value,
    data_type: String,
    panel_id: Option<String>,
    on_commit: Option<Rc<dyn Fn(&mut S, Value)>>,
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> EditableCell<S> {
    /// The shell over its props (`value`, `type`).
    pub(crate) fn new(value: Value, data_type: impl Into<String>) -> Self {
        Self {
            value,
            data_type: data_type.into(),
            panel_id: None,
            on_commit: None,
            on_cancel: None,
        }
    }

    /// React `panelId` (optional): the panel the cell belongs to — the
    /// editor registers itself with the navigation guard under this
    /// id (05 §4.4) — without it, a re-read would take the draft with
    /// it. The host (T297) owns the registration; held here.
    pub(crate) fn panel_id(mut self, panel_id: impl Into<String>) -> Self {
        self.panel_id = Some(panel_id.into());
        self
    }

    /// React `onCommit(next)` — the written value (the host's commit
    /// path; the DraftRow seam's `on_draft_cell_edit` lands here).
    pub(crate) fn on_commit(mut self, f: impl Fn(&mut S, Value) + 'static) -> Self {
        self.on_commit = Some(Rc::new(f));
        self
    }

    /// React `onCancel()` — the editor's escape (the DraftRow seam's
    /// `on_draft_cell_edit_cancel`).
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// The shell: the kind derivation plus the T297 host seam (see
    /// the module docs — the host mounts the per-type controls).
    pub(crate) fn build(self, k: &Kit) -> El {
        let derived = EditableCellKind::of(&self.value, &self.data_type);
        // React 48-59 mounts the host with `columnName: ""`, `nullable`
        // always true, and `originalValue: value`. The seam carries the
        // derivation's result for the wiring task to assert.
        col()
            .key("editable-cell")
            .w_full()
            .h(28.0)
            .center_y()
            .label(format!(
                "T297|kind={}|cellType={:?}|zoned={}|text={:?}|nullable=true|panel={}",
                derived.kind.as_str(),
                derived.cell_type,
                is_zoned_cell_type(&self.data_type),
                derived.text,
                self.panel_id.as_deref().unwrap_or("")
            ))
            .child(
                text(derived.text)
                    .key("editable-cell-seam-text")
                    .font(k.ty.small)
                    .color(k.c.text_muted)
                    .truncate(),
            )
    }
}
