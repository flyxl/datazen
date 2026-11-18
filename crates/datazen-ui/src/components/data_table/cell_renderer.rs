//! A single DataTable cell — a Rune port of
//! `packages/frontend/src/components/DataTable/CellRenderer.tsx`.
//!
//! React renders one cell from a value plus column metadata. The branches below
//! are kept in the source order, with the same guards:
//!
//! 1. editor kind is resolved **first** (even for a non-editing cell), from the
//!    normalised type — never from a substring test of the driver's type name;
//! 2. `isEditing` hands the cell to the editor host;
//! 3. `null`/`undefined` paints an italic `NULL` and stops — no plan, no colour;
//! 4. the colour reads the normalised type (the editor reads the same answer);
//! 5. a presentation plan's `text` result wins over the family branches, with
//!    the **raw** value in the tooltip and the editor still initialised raw;
//! 6. `bool` / `number` / `datetime` / `json` / fallback `text`, in that order.
//!
//! All formatting and type judgements come from `crate::support` (the ports of
//! `lib/formatters.ts`, `cellTypes.ts`, `dataTypeColors.ts`, `cellEditorKind.ts`
//! and `editors/toEditString.ts`); this module only decides which element to
//! build. Colours come from `rune-kit` tokens via [`cell_value_color`] — there
//! is no raw colour literal here — everything goes through the kit.
//!
//! # Deliberate boundaries
//!
//! - `ColumnPresentationPlan` / `PresentationValue` (issue-32 T05) live in a
//!   layer that is **not ported yet**. [`CellPresentationVm`] is a minimal
//!   stand-in with a `present` closure and the same two result kinds the
//!   extension contract exposes (`text` / `unchanged`). It is a VM, not a port;
//!   when the valuePresentation layer lands, replace this struct with the real
//!   plan and keep the `present()` call site unchanged. A cell with no plan
//!   renders exactly as before (default-off).
//! - React's `CellEditorHost` has no Rune port. The editing branch mounts a
//!   keyed, focusable `textbox` seeded with [`to_edit_string`] and reports the
//!   raw value through `on_commit`; the real editor host replaces that element
//!   once it lands. `panel_id`, `nullable`, `integer_only` and the resolved
//!   [`CellEditorKind`] stay on the VM so the handoff loses no information.
//! - The grid metadata flags (`is_primary_key`, `is_link`, `is_total`,
//!   `is_summary`, `read_only`) are **reserved**: React's `CellRenderer.tsx`
//!   has no branch for them. They are carried on the VM and only affect Rune
//!   affordances that do not change the visual branch — `is_link` gives a link
//!   role and pointer cursor, `read_only` withholds the press-to-edit hook.
//!   React's copy behaviour is DOM text selection handled by the grid; Rune
//!   keeps the text selectable and puts the full untruncated value in `help`.
//!
//! # Key convention
//!
//! Every cell receives a caller-unique `key` (for example `r3c7`); the single
//! element it mounts is `<key>-value`, or `<key>-editor` while editing. Two
//! cells rendered next to each other therefore never share an element key.

use std::rc::Rc;

use rune::{text, Align, Cursor, El};
use rune_kit::Kit;
use serde_json::Value;

use crate::support::cell_editor_kind::{
    resolve_cell_editor_kind, CellEditorKind, CellEditorKindInput,
};
use crate::support::cell_types::{
    cell_type_to_data_type_family, is_zoned_cell_type, resolve_cell_type, CellType,
};
use crate::support::data_type_colors::{cell_value_color, DataTypeFamily};
use crate::support::formatters::{format_cell, format_timestamp, number_text};
use crate::support::to_edit_string::{count_code_points, to_edit_string};
use crate::ui::kit::kit;

/// React's `PresentationResult` has exactly two kinds in the extension
/// contract; a VM keeps only those two so an unknown kind can never be
/// mistaken for renderable text.
#[allow(dead_code)] // Consumers: the presentation-plan caller lands with the grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PresentationResultVm {
    /// Render this text in the datetime branch's style.
    Text(String),
    /// Fall through to the normal family branches.
    Unchanged,
}

/// Minimal stand-in for `ColumnPresentationPlan` (issue-32 T05, deliberate
/// boundary). The plan answers one question for one value; the caller owns the
/// column context React keeps on the plan object.
#[allow(dead_code)] // Consumers: the valuePresentation layer is not ported yet.
#[derive(Clone)]
pub(crate) struct CellPresentationVm {
    /// The column the plan belongs to; kept for diagnostics/key parity.
    pub(crate) column_name: String,
    present: Rc<dyn Fn(&Value) -> PresentationResultVm>,
}

#[allow(dead_code)] // Consumers: the valuePresentation layer is not ported yet.
impl CellPresentationVm {
    /// A plan that answers `present` for each value.
    pub(crate) fn new(
        column_name: impl Into<String>,
        present: impl Fn(&Value) -> PresentationResultVm + 'static,
    ) -> Self {
        Self {
            column_name: column_name.into(),
            present: Rc::new(present),
        }
    }

    /// A plan that always presents the same text — the common case in tests
    /// and the shape a timestamp contribution produces for an instant column.
    pub(crate) fn text(column_name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self::new(column_name, move |_| {
            PresentationResultVm::Text(text.clone())
        })
    }

    /// The same call site as React's `presentationPlan?.present(value)`.
    pub(crate) fn present(&self, value: &Value) -> PresentationResultVm {
        (self.present)(value)
    }
}

/// The props of one cell: React's `CellRendererProps` plus the grid metadata
/// the caller already knows. This is a data VM — callbacks live on
/// [`CellRenderer`] so the data can be inspected without a runtime.
#[allow(dead_code)] // Consumers: GridCell lands in a later task.
#[derive(Clone)]
pub(crate) struct CellRendererVm {
    /// Caller-unique element key prefix (`<key>-value` / `<key>-editor`).
    pub(crate) key: String,
    /// `columnName` — only used when a plan asks for candidates.
    pub(crate) column_name: String,
    /// The driver's raw type name, e.g. `bigint` (DB-05).
    pub(crate) data_type: Option<String>,
    /// The value: `Value::Null` covers React's `null` and `undefined`.
    pub(crate) value: Value,
    /// Whether the cell is being edited.
    pub(crate) is_editing: bool,
    /// The normalised type computed once by the grid; `None` re-resolves.
    pub(crate) cell_type: Option<CellType>,
    /// Whether the column accepts SQL `NULL` (handed to the editor).
    pub(crate) nullable: bool,
    /// Whether the column holds whole numbers (handed to the editor).
    pub(crate) integer_only: bool,
    /// The panel the cell belongs to (handed to the editor).
    pub(crate) panel_id: Option<String>,
    /// DB-20 override, e.g. a pickable FK edits as a candidates dropdown.
    pub(crate) editor_kind_override: Option<CellEditorKind>,
    /// The column's presentation plan (issue-32 T05); `None` is default-off.
    pub(crate) presentation_plan: Option<CellPresentationVm>,
    /// Reserved: a primary-key cell. No visual branch in the React source.
    pub(crate) is_primary_key: bool,
    /// Reserved: a cell that navigates elsewhere. Adds a Rune link affordance.
    pub(crate) is_link: bool,
    /// Reserved: an aggregate total row.
    pub(crate) is_total: bool,
    /// Reserved: an aggregate summary row.
    pub(crate) is_summary: bool,
    /// Reserved: a cell the driver marked read-only (no press-to-edit).
    pub(crate) read_only: bool,
}

#[allow(dead_code)] // Consumers: GridCell lands in a later task.
impl CellRendererVm {
    /// A cell with React's defaults: nullable, not editing, no plan.
    pub(crate) fn new(
        key: impl Into<String>,
        column_name: impl Into<String>,
        value: Value,
    ) -> Self {
        Self {
            key: key.into(),
            column_name: column_name.into(),
            data_type: None,
            value,
            is_editing: false,
            cell_type: None,
            nullable: true,
            integer_only: false,
            panel_id: None,
            editor_kind_override: None,
            presentation_plan: None,
            is_primary_key: false,
            is_link: false,
            is_total: false,
            is_summary: false,
            read_only: false,
        }
    }

    pub(crate) fn data_type(mut self, data_type: impl Into<String>) -> Self {
        self.data_type = Some(data_type.into());
        self
    }

    pub(crate) fn cell_type(mut self, cell_type: CellType) -> Self {
        self.cell_type = Some(cell_type);
        self
    }

    pub(crate) fn editing(mut self, editing: bool) -> Self {
        self.is_editing = editing;
        self
    }

    pub(crate) fn presentation_plan(mut self, plan: CellPresentationVm) -> Self {
        self.presentation_plan = Some(plan);
        self
    }

    pub(crate) fn primary_key(mut self, primary_key: bool) -> Self {
        self.is_primary_key = primary_key;
        self
    }

    pub(crate) fn link(mut self, link: bool) -> Self {
        self.is_link = link;
        self
    }

    pub(crate) fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

/// Which JSX branch a cell takes, in React's order.
#[allow(dead_code)] // Consumers: GridCell lands in a later task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellBranch {
    Editing,
    Null,
    Presented,
    Bool,
    Number,
    Datetime,
    Json,
    Text,
}

/// The pure projection behind the element: text, tooltip, type style, colour.
/// Keeping it separate lets tests pin the mono/right/italic decisions that the
/// accessibility tree cannot expose, while still probing the real element.
#[allow(dead_code)] // Consumers: GridCell lands in a later task.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CellPlan {
    pub(crate) branch: CellBranch,
    pub(crate) text: String,
    /// React's `title` becomes Rune's `help` (also the copy source).
    pub(crate) help: Option<String>,
    /// `font-mono` → kit's mono type.
    pub(crate) mono: bool,
    /// `text-xs` → kit's `text_xsmall`; otherwise `text-sm` → `text_small`.
    pub(crate) xsmall: bool,
    /// `text-right` → `Align::End`.
    pub(crate) right: bool,
    /// `italic` on the NULL span.
    pub(crate) italic: bool,
    pub(crate) color: rune::Color,
    /// Resolved even for a non-editing cell, exactly like React's `useMemo`.
    pub(crate) editor_kind: CellEditorKind,
}

/// Resolve the editor kind the same way React's `useMemo` does: an override
/// wins, otherwise the normalised type and the raw edit text decide.
fn resolve_editor_kind(vm: &CellRendererVm, resolved: CellType, edit_text: &str) -> CellEditorKind {
    if let Some(kind) = vm.editor_kind_override {
        return kind;
    }
    resolve_cell_editor_kind(CellEditorKindInput {
        cell_type: resolved,
        zoned: vm.data_type.as_deref().is_some_and(is_zoned_cell_type),
        has_candidates: false,
        value_code_points: count_code_points(edit_text),
        has_line_break: edit_text.contains('\n') || edit_text.contains('\r'),
    })
}

/// Compute a cell's render plan without building a Rune element.
#[allow(dead_code)] // Consumers: GridCell lands in a later task.
pub(crate) fn plan(vm: &CellRendererVm, dark: bool) -> CellPlan {
    let resolved = vm
        .cell_type
        .unwrap_or_else(|| resolve_cell_type(vm.data_type.as_deref(), None));
    // React computes this before every branch, including the NULL branch; the
    // editor host always initialises from the raw value.
    let edit_text = to_edit_string(&vm.value);
    let editor_kind = resolve_editor_kind(vm, resolved, &edit_text);
    let color = cell_value_color(resolved, &vm.value, dark);

    // React's early return: the editor host owns the cell while editing.
    if vm.is_editing {
        return CellPlan {
            branch: CellBranch::Editing,
            text: edit_text,
            help: None,
            mono: false,
            xsmall: false,
            right: false,
            italic: false,
            color,
            editor_kind,
        };
    }

    // `null` / `undefined` short-circuit before the colour and the plan.
    if vm.value.is_null() {
        return CellPlan {
            branch: CellBranch::Null,
            text: "NULL".to_owned(),
            help: None,
            mono: false,
            xsmall: false,
            right: false,
            italic: true,
            color,
            editor_kind,
        };
    }

    // A `text` result wins over the family branches, but is consulted only
    // after editing and NULL. `unchanged` falls through (default-off).
    if let Some(present) = &vm.presentation_plan {
        if let PresentationResultVm::Text(text) = present.present(&vm.value) {
            return CellPlan {
                branch: CellBranch::Presented,
                text,
                help: Some(js_string(&vm.value)),
                mono: true,
                xsmall: true,
                right: false,
                italic: false,
                color,
                editor_kind,
            };
        }
    }

    match cell_type_to_data_type_family(resolved) {
        DataTypeFamily::Bool => CellPlan {
            branch: CellBranch::Bool,
            text: js_string(&vm.value),
            help: None,
            mono: true,
            xsmall: false,
            right: false,
            italic: false,
            color,
            editor_kind,
        },
        DataTypeFamily::Number => CellPlan {
            branch: CellBranch::Number,
            text: js_string(&vm.value),
            help: None,
            mono: true,
            xsmall: false,
            right: true,
            italic: false,
            color,
            editor_kind,
        },
        DataTypeFamily::Datetime => CellPlan {
            branch: CellBranch::Datetime,
            text: format_timestamp(&vm.value),
            help: Some(js_string(&vm.value)),
            mono: true,
            xsmall: true,
            right: false,
            italic: false,
            color,
            editor_kind,
        },
        DataTypeFamily::Json => {
            let raw = format_cell(&vm.value);
            CellPlan {
                branch: CellBranch::Json,
                text: truncate_120(&raw),
                help: Some(raw),
                mono: true,
                xsmall: true,
                right: false,
                italic: false,
                color,
                editor_kind,
            }
        }
        // React's fallback branch: objects are `JSON.stringify`d, scalars use
        // `String(value)`. `format_cell` is that projection for serde values.
        DataTypeFamily::Text | DataTypeFamily::Binary | DataTypeFamily::Null => {
            let raw = format_cell(&vm.value);
            CellPlan {
                branch: CellBranch::Text,
                text: truncate_120(&raw),
                help: Some(raw),
                mono: false,
                xsmall: false,
                right: false,
                italic: false,
                color,
                editor_kind,
            }
        }
    }
}

/// React's `String(value)` for a JSON value: arrays join their elements with
/// commas (`Array.prototype.toString`), objects become `[object Object]`.
fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number_text(number),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

/// React's `text.length > 120 ? text.slice(0, 120) + '…' : text`, on Unicode
/// code points rather than UTF-16 units (the same count the editor uses).
fn truncate_120(text: &str) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

/// The cell element builder. The VM owns data; callbacks are optional so a
/// read-only probe can render the same element without any state wiring.
pub(crate) struct CellRenderer<S: 'static> {
    vm: CellRendererVm,
    on_edit: Option<Rc<dyn Fn(&mut S)>>,
    on_commit: Option<Rc<dyn Fn(&mut S, Option<Value>)>>,
    #[allow(dead_code)] // The editor host owns cancellation until it lands.
    on_cancel: Option<Rc<dyn Fn(&mut S)>>,
}

#[allow(dead_code)] // Consumers: GridCell lands in a later task.
impl<S: 'static> CellRenderer<S> {
    pub(crate) fn new(vm: CellRendererVm) -> Self {
        Self {
            vm,
            on_edit: None,
            on_commit: None,
            on_cancel: None,
        }
    }

    /// Pressing a non-editing, writable cell asks the grid to open the editor.
    pub(crate) fn on_edit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_edit = Some(Rc::new(f));
        self
    }

    /// The editing handoff reports the raw value (React's `onCellValue ?? onCommit`).
    pub(crate) fn on_commit(mut self, f: impl Fn(&mut S, Option<Value>) + 'static) -> Self {
        self.on_commit = Some(Rc::new(f));
        self
    }

    /// Escape while editing cancels; kept for parity with React's `onCancel`.
    #[allow(dead_code)] // The editor host owns cancellation until it lands.
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// The data VM, for callers that need to inspect the cell without a runtime.
    pub(crate) fn vm(&self) -> &CellRendererVm {
        &self.vm
    }

    /// Paint the single element for this cell into its parent.
    pub(crate) fn build(self, dark: bool) -> El {
        let plan = plan(&self.vm, dark);
        let k: &Kit = kit(dark);
        let editing = plan.branch == CellBranch::Editing;

        let mut style = if plan.xsmall {
            k.ty.text_xsmall
        } else {
            k.ty.text_small
        };
        if plan.mono {
            style = style.mono();
        }
        if plan.italic {
            style = style.italic();
        }

        let key = &self.vm.key;
        let mut element = text(plan.text)
            .key(if editing {
                format!("{key}-editor")
            } else {
                format!("{key}-value")
            })
            .font(style)
            .color(plan.color)
            .w_full()
            .min_w(0.0)
            .truncate();

        if editing {
            // Minimal editor handoff: a focusable textbox seeded from the raw
            // value, labelled with the resolved editor kind.
            element = element.role("textbox").label(plan.editor_kind.as_str());
        } else if self.vm.is_link {
            // Rune-only affordance; React owns navigation in the grid, not here.
            element = element.role("link").cursor(Cursor::Pointer);
        }
        if plan.right {
            element = element.align(Align::End);
        }
        if let Some(help) = plan.help {
            element = element.help(help);
        }

        if editing {
            if let Some(commit) = self.on_commit {
                let value = self.vm.value.clone();
                element = element.on_press(move |state: &mut S| commit(state, Some(value.clone())));
            }
        } else if !self.vm.read_only {
            if let Some(on_edit) = self.on_edit {
                element = element.on_press(move |state: &mut S| on_edit(state));
            }
        }
        element
    }
}
