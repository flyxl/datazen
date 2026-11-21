//! T236: the row-detail drawer, ported from
//! `packages/frontend/src/components/DataTable/DetailPanel.tsx`.
//!
//! The aside that shows one row's fields beside the grid: a column search,
//! one field row per column, and the same value presentation the grid reads.
//! The drawer shows the **same columns** as the grid next to it (the driver's
//! `cellTypeHints` answer the same type question, and the presentation plans
//! are built from the same (columns, source, snapshot) triple), so grid and
//! aside agree cell for cell.
//!
//! | React piece | here |
//! |---|---|
//! | `DetailPanelProps` | [`DetailPanel`] builder + [`DetailPanelState`] |
//! | `filteredColumns` (`useMemo`) | [`filtered_columns`] |
//! | `multiRow` | [`multi_row`] |
//! | `FieldRow`'s type/value classification | [`FieldFacts`] / [`field_facts`] |
//! | the display `<pre>`'s content chain | [`displayed_text`] |
//! | the blob line's byte count | [`blob_byte_count`] |
//! | `InlineFieldEditor.commit` | [`commit_field_edit`] |
//! | `InlineFieldEditor.handleKeyDown` (the model) | [`editor_key_decision`] / [`EditorKeyDecision`] (Enter commits a short field; a long-text field's Return inserts the newline via `on_submit`, the `ai_context_settings` shape) |
//! | the search box's `TextEdit` | [`DetailPanelState::query`] |
//! | BUG-005's display/edit modes | [`DetailPanelState::enter_edit`] / [`exit_edit`] |
//! | `quickLookOpen` per field | [`DetailPanelState::quick_look`] |
//! | `presentationColumn` | [`DetailPanelState::presentation_column`] |
//! | `isBlobType` / `isJsonValue` / `prettyJson` imports | [`super::quick_look_dialog`] (T378's file, started early) |
//! | `ColumnPresentationDialog` mount | the ported dialog view, wired through [`DetailPanel::presentation_dialog`] |
//!
//! ## Deliberate boundaries
//!
//! * **QuickLookDialog awaits T378** (same segment): the drawer owns the
//!   state and the gate (`can_quick_look` + [`DetailPanelState::quick_look`])
//!   and renders the per-field button, but the dialog element itself is not
//!   mounted yet; T378 grows `quick_look_dialog.rs` into the viewer and adds
//!   the mount.
//! * The session-merged snapshot (`useSessionValuePresentationSnapshot`) is a
//!   host-side merge through the ported
//!   `hooks::use_column_presentation_session` hook, exactly as DataTable
//!   consumes it; the drawer receives the already-merged snapshot.
//! * The type label's Tailwind class (`dataTypeTextClass`) is the kit's
//!   semantic colour token here: [`crate::support::data_type_colors::data_type_color`].
//!
//! ## Deviations from the React source (deliberate)
//!
//! - `value.length` is UTF-16 code units in React; the byte-count line and
//!   the quick-look gate count Unicode code points
//!   ([`crate::support::to_edit_string::count_code_points`]), the same
//!   documented convention the rest of the crate pins.
//! - The hover-only reveal of the quick-look / presentation buttons
//!   (`opacity-0 group-hover:opacity-100`) stays visible: Rune rows have no
//!   group-hover cascade, and an always-visible affordance beats an
//!   invisible one (the buttons remain `title`-labelled).
//! - `Number(trimmed)` in the commit path accepts only the plain
//!   decimal/exponent forms `f64` parses; JS-only spellings (`0x10`,
//!   whitespace padding) degrade to the null commit instead of their JS
//!   values, and the wire's `NaN → null` equivalence is kept verbatim.
//! - `toLocaleString` is not involved here; the badge's `{count}` placeholder
//!   is formatted by the caller (`t("detail.editingRows")` keeps `{count}`).
//! - **Commits land on Return, not on blur** (the T023-S3 /
//!   `column_mapping_editor` shape): Rune exposes no field-level Escape or
//!   blur hook, so the short field's `on_submit` is the commit and the
//!   long-text field's `on_submit` inserts the newline React's textarea
//!   inserts. The runtime's field Escape blurs the caret and nothing else:
//!   the editing entry stays until a submit, and the next activation re-seeds
//!   the local from the value — which is React's Escape restore; React's
//!   blur-commit is the same deviation class. React's `autoFocus` on the
//!   editor is the field's own click (Rune has no programmatic focus).
//!
//! The element keys: the root is `detail-panel`; the field rows are
//! `detail-field-<index>` (Rune keys derive from position, React's list key
//! `col.id` is parity-only), and the presentation button keeps React's
//! `data-testid` shape `detail-field-presentation-<column name>` as its key.

// The drawer's host (the grid shell that mounts the aside beside it) lands
// with a later task; until then nothing in the crate constructs the view.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, icon, row, text, text_input, El, Key, KeyEvent, ScrollSpec, TextEdit, Tx};
use rune_kit::Kit;
use serde_json::Value;

use super::column_presentation_dialog::ValuePresentationSnapshot;
use super::column_presentation_dialog_state::ColumnPresentationDialogState;
use super::column_presentation_dialog_view::ColumnPresentationDialog as PresentationDialogView;
use super::column_presentation_plan::{
    build_column_presentation_plan, ColumnSource, PresentationPlanColumn, PresentationResult,
    TimestampPresenter,
};
use super::column_presentation_rules::ValuePresentationRegistry;
use super::quick_look_dialog::{is_blob_type, is_json_value, pretty_json};
use super::table_header::Column;
use crate::i18n::t;
use crate::support::cell_types::CellTypeHints;
use crate::support::data_type_colors::data_type_color;
use crate::support::formatters::format_cell;
use crate::support::to_edit_string::{count_code_points, to_edit_string};
use crate::icons::{ICON_MAXIMIZE_2, ICON_SEARCH, ICON_X};
use crate::ui::kit::kit;

// Tailwind → pt constants (the codebase's px-to-pt convention).
const ASIDE_W: f32 = 288.0; // w-72
const HEADER_H: f32 = 40.0; // h-10
const HEADER_PAD_X: f32 = 12.0; // px-3
const HEADER_GAP: f32 = 8.0; // gap-2
const SEARCH_PAD_X: f32 = 12.0; // px-3
const SEARCH_PAD_Y: f32 = 8.0; // py-2
const SEARCH_ICON: f32 = 14.0; // h-3.5 w-3.5
const CLEAR_ICON: f32 = 12.0; // h-3 w-3
const FIELD_ICON: f32 = 12.0; // h-3 w-3 (Maximize2)
const INPUT_H: f32 = 28.0; // h-7
const INPUT_PAD_L: f32 = 32.0; // pl-8
const INPUT_PAD_R: f32 = 28.0; // pr-7
const INPUT_PAD_X: f32 = 8.0; // px-2 (the editor)
const INPUT_PAD_Y: f32 = 4.0; // py-1 (the editor)
const FIELD_PAD_X: f32 = 12.0; // px-3
const FIELD_PAD_Y: f32 = 10.0; // py-2.5
const FIELD_GAP: f32 = 6.0; // gap-1.5
const FIELD_LABEL_GAP: f32 = 4.0; // mb-1
const PRE_MAX_H: f32 = 128.0; // max-h-32
const BADGE_PAD_X: f32 = 8.0; // px-2
const BADGE_PAD_Y: f32 = 2.0; // py-0.5
const BADGE_RADIUS: f32 = 8.0; // rounded-full → the crate's pill radius
const BUTTON_PAD: f32 = 2.0; // p-0.5
const EDITOR_MIN_H: f32 = 28.0; // min-h-[28px] (the textarea form)
/// The byte budget a string must exceed before the quick-look gate opens
/// (React's `value.length > 120`).
const QUICK_LOOK_MIN_LENGTH: usize = 120;

// ---------------------------------------------------------------------------
// Pure derivations (the parity core; no Rune tree needed)
// ---------------------------------------------------------------------------

/// The columns the search box leaves visible: React's `filteredColumns`
/// `useMemo` — an empty or blank query keeps every column, otherwise a
/// case-insensitive substring test on the name.
pub(crate) fn filtered_columns<'a>(columns: &'a [Column], query: &str) -> Vec<&'a Column> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return columns.iter().collect();
    }
    columns
        .iter()
        .filter(|column| column.name.to_lowercase().contains(&needle))
        .collect()
}

/// React's `multiRow`: the bulk-edit badge shows when a row is selected and
/// more than one row is in the selection.
pub(crate) fn multi_row(row_index: Option<usize>, selected_rows: usize) -> bool {
    row_index.is_some() && selected_rows > 1
}

/// The badge text: `t('detail.editingRows', { count })` — the pack keeps the
/// `{count}` placeholder and the caller formats it (the i18n module's rule).
pub(crate) fn editing_rows_text(selected_rows: usize) -> String {
    t("detail.editingRows").replace("{count}", &selected_rows.to_string())
}

/// One field's classification, React's `FieldRow` locals — the same verdicts
/// the same inputs always produce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldFacts {
    /// `value === null || value === undefined` (`Value::Null` covers both).
    pub(crate) is_null: bool,
    /// `type.includes('json') || isJsonValue(value)`.
    pub(crate) is_json: bool,
    /// `isBlobType(type)` (the lowercased type name).
    pub(crate) is_blob: bool,
    /// `type.includes('text') || isJson || isBlob`.
    pub(crate) is_long_text: bool,
    /// `isJson || isBlob || (typeof value === 'string' && value.length > 120)`.
    pub(crate) can_quick_look: bool,
    /// `isJson ? prettyJson(value) : null`.
    pub(crate) pretty: Option<String>,
}

/// React's `FieldRow` classification. `ty` is the raw column type; the type
/// tests run on its lowercased form (`(column.type ?? '').toLowerCase()`).
pub(crate) fn field_facts(ty: Option<&str>, value: &Value) -> FieldFacts {
    let type_lower = ty.unwrap_or_default().to_lowercase();
    let is_json = type_lower.contains("json") || is_json_value(value);
    let is_blob = is_blob_type(&type_lower);
    let is_long_text = type_lower.contains("text") || is_json || is_blob;
    let can_quick_look = is_json
        || is_blob
        || matches!(value, Value::String(s) if count_code_points(s) > QUICK_LOOK_MIN_LENGTH);
    let pretty = if is_json { pretty_json(value) } else { None };
    FieldFacts {
        is_null: value.is_null(),
        is_json,
        is_blob,
        is_long_text,
        can_quick_look,
        pretty,
    }
}

/// The display `<pre>`'s content chain, React's nested ternary in both the
/// editable-display and read-only branches:
///
/// 1. `NULL` — decided before anything else (`isNull`);
/// 2. a presentation plan's `text` result (the T05 read path — consulted
///    only here, never by the editor);
/// 3. `pretty ?? formatCell(value)` (pretty JSON, else the shared formatter).
pub(crate) fn displayed_text(
    facts: &FieldFacts,
    value: &Value,
    plan: Option<&super::column_presentation_plan::ColumnPresentationPlan>,
) -> String {
    if facts.is_null {
        return "NULL".to_owned();
    }
    if let Some(plan) = plan {
        if let PresentationResult::Text { text, .. } = plan.present(value) {
            return text;
        }
    }
    facts.pretty.clone().unwrap_or_else(|| format_cell(value))
}

/// The blob line's count: a string counts its length, anything else its
/// formatted form (`typeof value === 'string' ? value.length :
/// formatCell(value).length`). Code points, per the crate convention.
pub(crate) fn blob_byte_count(value: &Value) -> usize {
    match value {
        Value::String(text) => count_code_points(text),
        other => count_code_points(&format_cell(other)),
    }
}

/// The blob line: `t('detail.blobData') · <n> t('detail.bytes')`.
pub(crate) fn blob_line(value: &Value) -> String {
    format!("{} · {} {}", t("detail.blobData"), blob_byte_count(value), t("detail.bytes"))
}

// ---------------------------------------------------------------------------
// The inline editor's commit semantics (InlineFieldEditor)
// ---------------------------------------------------------------------------

/// What the editor's commit does, React's `commit` callbacks in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FieldCommit {
    /// `trimmed === toEditString(value)` or `trimmed === '' && isNull` —
    /// nothing to commit.
    NoChange,
    /// `trimmed === ''` → `onCommit(null)`.
    Null,
    /// The typed value to commit.
    Value(Value),
}

/// React's `InlineFieldEditor.commit`, as a pure function over the editor's
/// text. `display` is `toEditString(value)` (the equality guard); the type
/// tests run on the lowercased column type, exactly as the editor receives
/// it.
///
/// The typed arms:
///
/// * int/serial/bigint and float/double/numeric/decimal/real →
///   `Number(trimmed)`; an unparseable text is JS's `NaN`, which the wire
///   carries as `null` — the null commit is that equivalence, kept verbatim;
/// * bool → `trimmed === 'true'`;
/// * json → `JSON.parse(trimmed)`, the raw string when it does not parse;
/// * otherwise the string itself.
pub(crate) fn commit_field_edit(ty: &str, trimmed: &str, display: &str, value_is_null: bool) -> FieldCommit {
    if trimmed == display {
        return FieldCommit::NoChange;
    }
    if trimmed.is_empty() && value_is_null {
        return FieldCommit::NoChange;
    }
    if trimmed.is_empty() {
        return FieldCommit::Null;
    }
    let number = || match trimmed.parse::<f64>() {
        Ok(number) => {
            // JS's `Number("3")` is the JSON integer `3`, and the crate's
            // display paths (`number_text`, `float_whole_to_i64`) already
            // keep whole floats integral — the commit does too.
            if number.is_finite() && number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0 {
                FieldCommit::Value(serde_json::json!(number as i64))
            } else {
                FieldCommit::Value(serde_json::json!(number))
            }
        }
        // JS commits NaN; JSON.stringify(NaN) is null, so the wire value is
        // null — the faithful commit.
        Err(_) => FieldCommit::Null,
    };
    if ty.contains("int") || ty.contains("serial") || ty.contains("bigint") {
        return number();
    }
    if ty.contains("bool") {
        return FieldCommit::Value(serde_json::json!(trimmed == "true"));
    }
    if ty.contains("float")
        || ty.contains("double")
        || ty.contains("numeric")
        || ty.contains("decimal")
        || ty.contains("real")
    {
        return number();
    }
    if ty.contains("json") {
        return match serde_json::from_str::<Value>(trimmed) {
            Ok(parsed) => FieldCommit::Value(parsed),
            // React's catch → onCommit(trimmed): the raw string.
            Err(_) => FieldCommit::Value(Value::String(trimmed.to_owned())),
        };
    }
    FieldCommit::Value(Value::String(trimmed.to_owned()))
}

/// What the editor does with a key, React's `handleKeyDown`:
/// Escape restores the value and cancels (BUG-005: the caller returns to the
/// display state), Enter commits when the field is not long text (React
/// `preventDefault`s it so the textarea keeps no newline), anything else is
/// the editor's own typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorKeyDecision {
    /// Escape: restore + cancel.
    Cancel,
    /// Enter on a single-line field: commit.
    Commit,
    /// Anything else (including Enter on a long-text field).
    None,
}

/// The decision for one key. Keys arrive as the host runtime reports them
/// (the same `Key` enum the `on_key` handler set sees).
pub(crate) fn editor_key_decision(key: &Key, is_long_text: bool) -> EditorKeyDecision {
    match key {
        Key::Escape => EditorKeyDecision::Cancel,
        Key::Enter if !is_long_text => EditorKeyDecision::Commit,
        _ => EditorKeyDecision::None,
    }
}

// ---------------------------------------------------------------------------
// The drawer's state (the host owns one; React's hooks land here)
// ---------------------------------------------------------------------------

/// The drawer's own UI state — everything React kept in `useState` inside
/// `DetailPanel` and its two children. The row, the columns and the props
/// are per-frame inputs ([`DetailPanel`]); only the editable state lives
/// here.
#[derive(Clone, Debug, Default)]
pub(crate) struct DetailPanelState {
    /// The search box's text (`query`).
    pub(crate) query: TextEdit,
    /// BUG-005's per-field edit state: a column name → its editor text. A
    /// field with no entry is in the display state (the default); entering
    /// the editor seeds it with `toEditString(value)` and leaving drops it,
    /// which is React's `useEffect` reset in the ownership world.
    pub(crate) editing: BTreeMap<String, TextEdit>,
    /// Which column's Quick Look is open (`quickLookOpen`, per field);
    /// the dialog element itself awaits T378.
    pub(crate) quick_look: Option<String>,
    /// Which column's presentation dialog is open (`presentationColumn`).
    pub(crate) presentation_column: Option<String>,
}

impl DetailPanelState {
    /// The empty drawer: no query, every field displaying, nothing open.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// BUG-005's "enter the editor": seed the field's local text with the
    /// value's edit form (React's `useState(display)` on mount — the local
    /// starts from the raw value, never the presented one).
    pub(crate) fn enter_edit(&mut self, column: &str, value: &Value) {
        self.editing.insert(column.to_owned(), TextEdit::new(to_edit_string(value)));
    }

    /// BUG-005's "return to display": drop the field's local text. The next
    /// entry re-seeds it, which is the reset half of React's `useEffect`.
    pub(crate) fn exit_edit(&mut self, column: &str) {
        self.editing.remove(column);
    }

    /// The field's editor text, if the field is editing.
    pub(crate) fn field_editor(&self, column: &str) -> Option<&TextEdit> {
        self.editing.get(column)
    }

    /// The search text as the filter reads it (the raw `TextEdit` string).
    pub(crate) fn query_text(&self) -> &str {
        &self.query.text
    }
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// The field row's rendering inputs, resolved once per frame.
struct FieldRowInputs<'a> {
    column: &'a Column,
    value: Value,
    facts: FieldFacts,
    /// The column's presentation plan (`presentationPlanByColumn.get(name)`).
    plan: Option<super::column_presentation_plan::ColumnPresentationPlan>,
    /// `editable && onFieldEdit && rowIndex !== null` — the editor arms.
    on_commit: bool,
}

/// The drawer, one per grid. Callers wire the per-frame data (row, columns,
/// selection) plus the host callbacks; [`build`](DetailPanel::build) paints
/// the aside and, when the presentation dialog is open, its layer element.
pub(crate) struct DetailPanel<'a, S> {
    state: &'a DetailPanelState,
    open: bool,
    columns: &'a [Column],
    row: Option<&'a Value>,
    row_index: Option<usize>,
    editable: bool,
    selected_rows: usize,
    cell_type_hints: Option<&'a CellTypeHints>,
    presentation_snapshot: Option<&'a ValuePresentationSnapshot>,
    presentation_source: Option<&'a ColumnSource>,
    presentation_registry: Option<&'a ValuePresentationRegistry>,
    presentation_presenter: Option<&'a TimestampPresenter>,
    presentation_dialog: Option<&'a ColumnPresentationDialogState>,
    presentation_dialog_host: Option<Rc<dyn Fn(&mut S) -> &mut ColumnPresentationDialogState>>,
    on_field_edit: Option<Rc<dyn Fn(&mut S, usize, &str, Value)>>,
    host: Option<Rc<dyn Fn(&mut S) -> &mut DetailPanelState>>,
    on_open_column_presentation: Option<Rc<dyn Fn(&mut S, &str)>>,
}

impl<'a, S: 'static> DetailPanel<'a, S> {
    /// The drawer for `state`, closed until [`open`](DetailPanel::open).
    pub(crate) fn new(state: &'a DetailPanelState) -> Self {
        Self {
            state,
            open: false,
            columns: &[],
            row: None,
            row_index: None,
            editable: false,
            selected_rows: 0,
            cell_type_hints: None,
            presentation_snapshot: None,
            presentation_source: None,
            presentation_registry: None,
            presentation_presenter: None,
            presentation_dialog: None,
            presentation_dialog_host: None,
            on_field_edit: None,
            host: None,
            on_open_column_presentation: None,
        }
    }

    /// React `open`: `false` mounts nothing.
    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// The grid's columns — the drawer shows the same columns, so the type
    /// labels answer the same question the grid's do.
    pub(crate) fn columns(mut self, columns: &'a [Column]) -> Self {
        self.columns = columns;
        self
    }

    /// The selected row (`row[col.name]` reads each field; `None` paints the
    /// no-selection line).
    pub(crate) fn row(mut self, row: Option<&'a Value>) -> Self {
        self.row = row;
        self
    }

    /// React `rowIndex` — the row the edits commit to.
    pub(crate) fn row_index(mut self, row_index: Option<usize>) -> Self {
        self.row_index = row_index;
        self
    }

    /// React `editable`: whether the fields offer the editor at all.
    pub(crate) fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    /// The size of React's `selectedRows` set (the badge only needs the
    /// count).
    pub(crate) fn selected_rows(mut self, count: usize) -> Self {
        self.selected_rows = count;
        self
    }

    /// React `cellTypeHints`.
    pub(crate) fn cell_type_hints(mut self, hints: &'a CellTypeHints) -> Self {
        self.cell_type_hints = Some(hints);
        self
    }

    /// The session-merged presentation snapshot (the host merges through the
    /// ported session hook, the same merge DataTable consumes).
    pub(crate) fn presentation_snapshot(mut self, snapshot: &'a ValuePresentationSnapshot) -> Self {
        self.presentation_snapshot = Some(snapshot);
        self
    }

    /// React `presentationSource` (the table identity; `None` for
    /// query-result columns, where overrides are unavailable).
    pub(crate) fn presentation_source(mut self, source: &'a ColumnSource) -> Self {
        self.presentation_source = Some(source);
        self
    }

    /// React `presentationRegistry` (the default is the global one when the
    /// caller wires none).
    pub(crate) fn presentation_registry(mut self, registry: &'a ValuePresentationRegistry) -> Self {
        self.presentation_registry = Some(registry);
        self
    }

    /// The timestamp engine's presenter, the same one the dialog previews
    /// with; `None` leaves `ready` plans on the `plan-error` fallback.
    pub(crate) fn presentation_presenter(mut self, presenter: &'a TimestampPresenter) -> Self {
        self.presentation_presenter = Some(presenter);
        self
    }

    /// The column-presentation dialog's state and the accessor that drives
    /// it (T09's aside entry). The drawer mounts the dialog when the state
    /// is open; the open/close bridging is the host's press wiring.
    pub(crate) fn presentation_dialog(
        mut self,
        state: &'a ColumnPresentationDialogState,
        host: impl Fn(&mut S) -> &mut ColumnPresentationDialogState + 'static,
    ) -> Self {
        self.presentation_dialog = Some(state);
        self.presentation_dialog_host = Some(Rc::new(host));
        self
    }

    /// React `onFieldEdit(row, col, value)` — the drawer's edit commits.
    /// Without it the fields never offer the editor (`editable &&
    /// onCommit` in React).
    pub(crate) fn on_field_edit(
        mut self,
        f: impl Fn(&mut S, usize, &str, Value) + 'static,
    ) -> Self {
        self.on_field_edit = Some(Rc::new(f));
        self
    }

    /// The drawer's own state accessor — the search box, the per-field edit
    /// state and the quick-look / presentation records live behind it.
    pub(crate) fn host(mut self, f: impl Fn(&mut S) -> &mut DetailPanelState + 'static) -> Self {
        self.host = Some(Rc::new(f));
        self
    }

    /// React `onOpenColumnPresentation` (T09): the aside field-menu entry.
    /// The button mounts only when the callback exists, exactly as React's
    /// `{onOpenColumnPresentation && (<button …>)}`; the host bridges the
    /// drawer's `presentationColumn` and the dialog state's config/open.
    pub(crate) fn on_open_column_presentation(
        mut self,
        f: impl Fn(&mut S, &str) + 'static,
    ) -> Self {
        self.on_open_column_presentation = Some(Rc::new(f));
        self
    }

    /// Build the aside (and the presentation dialog's layer element while it
    /// is open). `None` — a closed drawer — mounts nothing, as React's
    /// `if (!open) return null`.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> Option<El> {
        if !self.open {
            return None;
        }
        let k = kit(dark);
        let multi = multi_row(self.row_index, self.selected_rows);

        let mut aside = col()
            .key("detail-panel")
            .w(ASIDE_W)
            .h_full()
            .shrink()
            .fill(k.c.layer_01)
            .border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_base)
            .child(self.header(&k, multi));

        let body = match self.row {
            // React's null row: the centered no-selection line, and nothing
            // else (no search box either — it lives inside the non-null arm).
            None => col().key("detail-panel-empty-row").grow(1.0).items_center().child(
                text(t("detail.noSelection").to_owned())
                    .key("detail-panel-no-selection")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            ),
            Some(row) => {
                let mut body = col().key("detail-panel-body").grow(1.0);
                body = body.child(self.search_row(&k));
                body = body.child(self.field_list(&k, row, dark));
                body
            }
        };
        aside = aside.child(body);

        // The column-presentation dialog's mount (T09): the same dialog the
        // grid header menu opens, wired with the same snapshot / registry /
        // presenter the drawer reads. The mount condition is the dialog
        // state's own open flag plus a column that still exists.
        let presentation_dialog_host = self.presentation_dialog_host.clone();
        if let (Some(dialog_state), Some(snapshot), Some(registry), Some(host)) = (
            self.presentation_dialog,
            self.presentation_snapshot,
            self.presentation_registry,
            presentation_dialog_host,
        ) {
            let column_exists = self
                .columns
                .iter()
                .any(|column| column.name == dialog_state.config().column_name);
            if dialog_state.is_open() && column_exists {
                let mut dialog = PresentationDialogView::new(dialog_state)
                    .snapshot(snapshot)
                    .registry(registry)
                    .host_mut(move |s: &mut S| host(s));
                if let Some(presenter) = self.presentation_presenter {
                    dialog = dialog.presenter(Rc::clone(presenter));
                }
                dialog.build(dark, layer);
            }
        }

        Some(aside)
    }

    /// The title row: `DETAIL` on the left, the bulk-edit badge when
    /// `multiRow`.
    fn header(&self, k: &Kit, multi: bool) -> El {
        let mut header = row()
            .key("detail-panel-header")
            .h(HEADER_H)
            .shrink()
            .items_center()
            .justify_between()
            .gap(HEADER_GAP)
            .px(HEADER_PAD_X)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
        header = header.child(
            text(t("detail.title").to_owned())
                .key("detail-panel-title")
                .font(k.ty.text_xsmall_medium)
                .color(k.c.text_muted)
                .text_case(rune::TextCase::Uppercase),
        );
        if multi {
            header = header.child(
                text(editing_rows_text(self.selected_rows))
                    .key("detail-panel-editing-rows")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent)
                    .px(BADGE_PAD_X)
                    .py(BADGE_PAD_Y)
                    .radius(BADGE_RADIUS)
                    .fill(k.alpha(k.c.text_accent, 0.15)),
            );
        }
        header
    }

    /// The search row: the leading search glyph, the mono query input and,
    /// while a query is typed, the trailing clear button.
    fn search_row(&self, k: &Kit) -> El {
        let state = self.state;
        let mut search = row()
            .key("detail-panel-search")
            .shrink()
            .items_center()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.alpha(k.c.border_base, 0.5))
            .px(SEARCH_PAD_X)
            .py(SEARCH_PAD_Y)
            .child(icon(&ICON_SEARCH, SEARCH_ICON).key("detail-panel-search-icon").color(k.c.text_muted));

        // React's input: `h-7 w-full rounded-sm border border-edge/50
        // bg-surface-raised/30 pl-8 pr-7 font-mono text-xs`. The leading
        // glyph and trailing clear button are siblings in the row here (React
        // absolutely positions them inside the input's own padding), so the
        // field grows instead of taking the full width — the clear button
        // keeps its space at the row's end instead of being squeezed out.
        let mut input = text_input(&state.query, k.ty.mono)
            .key("detail-panel-search-input")
            .grow(1.0)
            .min_w(0.0)
            .min_h(INPUT_H)
            .px(INPUT_PAD_X)
            .radius(k.radius.sm)
            .fill(k.alpha(k.c.surface_raised, 0.3))
            .border(1.0, k.alpha(k.c.border_base, 0.5))
            .placeholder(t("detail.searchColumns"), k.alpha(k.c.text_faint, 0.7));
        if let Some(host) = &self.host {
            let host = Rc::clone(host);
            input = input.on_edit(rune::edit_access(move |s: &mut S| &mut host(s).query));
        }
        search = search.child(input);

        // The clear button mounts only while a query is typed
        // (`{query && (<button …>)}`).
        if !state.query.text.is_empty() {
            let mut clear = row()
                .key("detail-panel-search-clear")
                .shrink()
                .items_center()
                .radius(k.radius.sm)
                .py(BUTTON_PAD)
                .focusable(true)
                .focus_ring(2.0, 2.0, k.c.border_focus)
                .hover(k.c.surface_raised)
                .help(t("detail.clearSearch").to_owned())
                .child(icon(&ICON_X, CLEAR_ICON).key("detail-panel-search-clear-icon").color(k.c.text_muted));
            if let Some(host) = &self.host {
                let host = Rc::clone(host);
                clear = clear.on_press(move |s: &mut S| {
                    // React's `setQuery('')`: the whole text, not just the
                    // selection, clears.
                    host(s).query = TextEdit::new(String::new());
                });
            }
            search = search.child(clear);
        }
        search
    }

    /// The scrollable field list: the no-columns-found line, or one field
    /// row per visible column.
    fn field_list(&self, k: &Kit, row: &Value, dark: bool) -> El {
        let visible = filtered_columns(self.columns, self.state.query_text());
        if visible.is_empty() {
            return col()
                .key("detail-panel-no-columns")
                .grow(1.0)
                .items_center()
                .child(
                    text(t("detail.noColumnsFound").to_owned())
                        .key("detail-panel-no-columns-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                );
        }

        let mut list = col()
            .key("detail-panel-fields")
            .grow(1.0)
            .scroll(ScrollSpec { stick_bottom: false });
        for (index, column) in visible.into_iter().enumerate() {
            // `row[col.name]` — a missing field is `undefined`, which the
            // null verdicts cover (`Value::Null`).
            let value = row.get(&column.name).cloned().unwrap_or(Value::Null);
            let plan = self.plan_for_column(&column.name);
            let facts = field_facts(column.ty.as_deref(), &value);
            let on_commit = self.editable
                && self.on_field_edit.is_some()
                && self.row_index.is_some();
            let inputs = FieldRowInputs {
                column,
                value,
                facts,
                plan,
                on_commit,
            };
            list = list.child(self.field_row(&inputs, index, dark));
        }
        list
    }

    /// The column's presentation plan, built from the same (columns, source,
    /// snapshot) triple the grid builds from (issue-32 T05: the same input
    /// yields the same plan, so the two read identically). `None` while no
    /// snapshot or registry is wired.
    fn plan_for_column(&self, name: &str) -> Option<super::column_presentation_plan::ColumnPresentationPlan> {
        let snapshot = self.presentation_snapshot?;
        let registry = self.presentation_registry?;
        let column = self
            .columns
            .iter()
            .find(|column| column.name == name)?;
        let plan_column = PresentationPlanColumn {
            name: column.name.clone(),
            data_type: column.ty.clone(),
            hints: self.cell_type_hints.cloned(),
        };
        Some(build_column_presentation_plan(
            &plan_column,
            self.presentation_source,
            snapshot,
            registry,
            self.presentation_presenter,
        ))
    }

    /// One field row: the label line (name, type label, quick-look and
    /// presentation entries), then the value arm.
    fn field_row(&self, inputs: &FieldRowInputs<'_>, index: usize, dark: bool) -> El {
        let k = kit(dark);
        let column = inputs.column;
        let facts = &inputs.facts;
        let key = format!("detail-field-{index}");

        let mut field = col()
            .key(key.clone())
            .border_sides([0.0, 0.0, 1.0, 0.0], k.alpha(k.c.border_base, 0.5))
            .px(FIELD_PAD_X)
            .py(FIELD_PAD_Y)
            .child(self.label_row(inputs, index, dark));

        // The value arm, React's order: the editable display (click / Enter
        // / Space enters the editor), the editor itself, the blob line, the
        // read-only pre.
        let editing = self.state.field_editor(&column.name).is_some();
        if inputs.on_commit && !editing {
            field = field.child(self.display_container(inputs, index, dark));
        } else if inputs.on_commit && editing {
            field = field.child(self.inline_editor(inputs, dark));
        } else if facts.is_blob && !facts.is_null {
            field = field.child(self.blob_text(inputs, &k));
        } else {
            field = field.child(self.value_pre(inputs, &k));
        }

        field
    }

    /// The label row: the column name, the type label, the quick-look entry
    /// and (when the host wired it) the presentation entry.
    fn label_row(&self, inputs: &FieldRowInputs<'_>, index: usize, dark: bool) -> El {
        let k = kit(dark);
        let column = inputs.column;
        let facts = &inputs.facts;

        let mut label = row()
            .key(format!("detail-field-{index}-label"))
            .items_center()
            .justify_between()
            .gap(HEADER_GAP)
            .pb(FIELD_LABEL_GAP)
            .child(
                text(column.name.clone())
                    .key(format!("detail-field-{index}-name"))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_muted)
                    .truncate()
                    .help_when_truncated(column.name.clone()),
            );

        // The trailing cluster: the type label, then the two entries.
        let mut cluster = row()
            .key(format!("detail-field-{index}-meta"))
            .shrink()
            .items_center()
            .gap(FIELD_GAP);
        if let Some(ty) = &column.ty {
            if !ty.is_empty() {
                cluster = cluster.child(
                    text(ty.clone())
                        .key(format!("detail-field-{index}-type"))
                        .font(k.ty.text_xsmall.mono())
                        .color(data_type_color(Some(ty), self.cell_type_hints, dark))
                        .truncate()
                        .help_when_truncated(ty.clone()),
                );
            }
        }
        // The quick-look entry (DBR-05's drawer half): state + gate land
        // here, the dialog itself with T378.
        if facts.can_quick_look {
            let mut quick = row()
                .key(format!("detail-field-{index}-quick-look"))
                .shrink()
                .items_center()
                .radius(k.radius.sm)
                .py(BUTTON_PAD)
                .focusable(true)
                .focus_ring(2.0, 2.0, k.c.border_focus)
                .hover(k.c.surface_raised)
                .help(t("detail.quickLook").to_owned())
                .child(
                    icon(&ICON_MAXIMIZE_2, FIELD_ICON)
                        .key(format!("detail-field-{index}-quick-look-icon"))
                        .color(k.c.text_muted),
                );
            if let Some(host) = &self.host {
                let host = Rc::clone(host);
                let name = column.name.clone();
                quick = quick.on_press(move |s: &mut S| {
                    host(s).quick_look = Some(name.clone());
                });
            }
            cluster = cluster.child(quick);
        }
        // The presentation entry (T09): React's `data-testid` shape is the
        // element key here. It mounts only when the callback exists.
        if let Some(on_open) = &self.on_open_column_presentation {
            let mut presentation = row()
                .key(format!("detail-field-presentation-{}", column.name))
                .shrink()
                .items_center()
                .radius(k.radius.sm)
                .py(BUTTON_PAD)
                .focusable(true)
                .focus_ring(2.0, 2.0, k.c.border_focus)
                .hover(k.c.surface_raised)
                .help(t("dataTable.columnPresentation").to_owned())
                .child(
                    text(t("dataTable.columnPresentation").to_owned())
                        .key(format!("detail-field-{index}-presentation-label"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                );
            let on_open = Rc::clone(on_open);
            let name = column.name.clone();
            presentation = presentation.on_press(move |s: &mut S| on_open(s, &name));
            cluster = cluster.child(presentation);
        }
        label = label.child(cluster);
        label
    }

    /// BUG-005's display container: `role="button"`, press / Enter / Space
    /// enters the editor. The content is the same pre/blob pair the
    /// read-only arm paints.
    fn display_container(&self, inputs: &FieldRowInputs<'_>, index: usize, dark: bool) -> El {
        let k = kit(dark);
        let facts = &inputs.facts;
        let mut container = col()
            .key(format!("detail-field-{index}-display"))
            .role("button")
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus);
        let content = if facts.is_blob && !facts.is_null {
            self.blob_text(inputs, &k)
        } else {
            self.value_pre(inputs, &k)
        };
        container = container.child(content);
        if let Some(host) = &self.host {
            let host_press = Rc::clone(host);
            let name_press = inputs.column.name.clone();
            let value_press = inputs.value.clone();
            let host_key = Rc::clone(host);
            let name_key = inputs.column.name.clone();
            let value_key = inputs.value.clone();
            container = container
                .on_press(move |s: &mut S| host_press(s).enter_edit(&name_press, &value_press))
                .on_key(move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                    // React's Enter / Space activation on the display
                    // container. Rune has no `Key::Space` — a plain space
                    // arrives as `Text(" ")`.
                    let space = matches!(&event.key, Key::Text(text) if text == " ");
                    if matches!(event.key, Key::Enter) || space {
                        host_key(s).enter_edit(&name_key, &value_key);
                        true
                    } else {
                        false
                    }
                });
        }
        container
    }

    /// The read-only `<pre>`: `NULL` italic muted, a presentation's `text`
    /// result, pretty JSON, or the shared formatter — mono, wrapped
    /// (`whitespace-pre-wrap break-all`), capped (`max-h-32`).
    fn value_pre(&self, inputs: &FieldRowInputs<'_>, k: &Kit) -> El {
        let facts = &inputs.facts;
        let text_content = displayed_text(facts, &inputs.value, inputs.plan.as_ref());
        let mut pre = text(text_content)
            .key(format!("detail-field-{}-value", inputs.column.name))
            .font(k.ty.mono)
            .max_h(PRE_MAX_H)
            .wrap()
            .wrap_anywhere();
        if facts.is_null {
            // React's italic `NULL` in the muted ink.
            pre = pre.color(k.c.text_muted).font(k.ty.mono.italic());
        } else {
            pre = pre.color(k.c.text_base);
        }
        pre
    }

    /// The blob line: `BLOB 数据 · <n> 字节`, mono, italic, muted.
    fn blob_text(&self, inputs: &FieldRowInputs<'_>, k: &Kit) -> El {
        text(blob_line(&inputs.value))
            .key(format!("detail-field-{}-blob", inputs.column.name))
            .font(k.ty.mono.italic())
            .color(k.c.text_muted)
    }

    /// The inline editor (BUG-005): a single-line field for short values, a
    /// multiline one for long text. Return submits (the runtime's
    /// `on_submit`): a short field's submit runs the commit chain and
    /// returns the field to display; a long-text field's submit inserts the
    /// newline React's editor inserts (the `ai_context_settings` shape).
    ///
    /// Escape is the runtime's field Escape — the caret blurs and nothing
    /// else happens: Rune exposes no field-level Escape or blur hook (the
    /// `column_mapping_editor` / T023-S3 precedent), so the editing entry
    /// stays until a submit; the next activation re-seeds the local from
    /// the value, which is React's restore. React's blur-commit is the same
    /// documented deviation class: edits land on Enter only.
    fn inline_editor(&self, inputs: &FieldRowInputs<'_>, dark: bool) -> El {
        let k = kit(dark);
        let column = inputs.column;
        let facts = &inputs.facts;
        let Some(host) = &self.host else {
            return self.value_pre(inputs, &k);
        };
        let Some(edit) = self.state.field_editor(&column.name) else {
            return self.value_pre(inputs, &k);
        };
        let mut field = text_input(edit, k.ty.mono)
            .key(format!("detail-field-{}-editor", column.name))
            .w_full()
            .min_w(0.0)
            .min_h(if facts.is_long_text { EDITOR_MIN_H } else { INPUT_H })
            .px(INPUT_PAD_X)
            .py(INPUT_PAD_Y)
            .radius(k.radius.sm)
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base);
        if !facts.is_long_text {
            field = field.single_line();
        }
        // The editor's accessor: the field's TextEdit inside the drawer's
        // own state.
        let host_edit = Rc::clone(host);
        let name = column.name.clone();
        field = field.on_edit(rune::edit_access(move |s: &mut S| {
            host_edit(s)
                .editing
                .get_mut(&name)
                .expect("the editing field mounts only while its edit state exists")
        }));

        // Return's two meanings. A long-text field takes the newline
        // (React's Enter inside `wrap="off"` textareas); a short field's
        // submit is the commit: the pure chain decides, `onFieldEdit`
        // delivers, and the field returns to display (BUG-005's
        // commit-then-setEditing(false)).
        if facts.is_long_text {
            let host_newline = Rc::clone(host);
            let name = column.name.clone();
            field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
                host_newline(s)
                    .editing
                    .get_mut(&name)
                    .expect("the editing field mounts only while its edit state exists")
                    .insert("\n");
            });
        } else if let Some(on_field_edit) = &self.on_field_edit {
            let host_submit = Rc::clone(host);
            let on_field_edit = Rc::clone(on_field_edit);
            let name = column.name.clone();
            let ty = column.ty.clone().unwrap_or_default().to_lowercase();
            let display = to_edit_string(&inputs.value);
            let value_is_null = facts.is_null;
            let Some(row_index) = self.row_index else {
                return field;
            };
            field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
                let trimmed = host_submit(s)
                    .field_editor(&name)
                    .map(|edit| edit.text.clone())
                    .unwrap_or_default();
                match commit_field_edit(&ty, &trimmed, &display, value_is_null) {
                    FieldCommit::NoChange => {}
                    FieldCommit::Null => on_field_edit(s, row_index, &name, Value::Null),
                    FieldCommit::Value(value) => on_field_edit(s, row_index, &name, value),
                }
                // React's commit handler: onCommit then setEditing(false).
                host_submit(s).exit_edit(&name);
            });
        }
        field
    }
}
