//! Pure helpers carried for the useGridClipboard port (T294).
//!
//! Sources of truth, all **other segments'** ledger entries carried here
//! until they land and absorb them (the `lib_helpers` pattern):
//!
//! - `lib/gridPastePlan.ts` (T529, segment 2): the paste planner and the
//!   copy-source / row-copy registries.
//! - `lib/tableChanges.ts` (T629, segment 2): `CellPasteWrite` and the
//!   paste column gate (`columnWritability`).
//! - `stores/tableData/gridSelection.ts` (segment 3): the four geometry
//!   helpers the hook needs.
//! - `stores/tableData/pendingEditActions.ts` (segment 3): the
//!   `StagePasteWritesResult` shape (the staging action itself is
//!   host-injected, exactly as the React hook receives it via options).
//!
//! The registries are `thread_local` on purpose: React keeps them as
//! module state because they describe the **system clipboard**, not one
//! panel's table state — a paste in panel B must see what panel A
//! copied. Tests run on their own threads (the `views/navigation.rs`
//! precedent), so sessions cannot mix.
//!
//! Known deltas: none behavioural. `JSON.stringify` of the table key
//! parts is `serde_json` over a 4-string array — byte-identical for
//! every string JS can produce here (the parts are trimmed and folded
//! first, exactly as `tableChangeKey` does).

use std::cell::RefCell;

use serde_json::{json, Value};

use crate::support::cell_types::{cell_type_to_data_type_family, resolve_cell_type};
use crate::support::data_type_colors::DataTypeFamily;
use crate::table_data_connection_state::{
    CellCoord, CellRange, ColumnSchema, GridSelection, TableChangeContext,
};

use super::grid_clipboard_helpers::{
    paste_code, PasteProblem, ClipboardMatrix, GridClipboardRange, GRID_PASTE_MAX_BYTES,
    GRID_PASTE_MAX_CELLS,
};

// ── Selection geometry (stores/tableData/gridSelection.ts, carried) ──────────

/// Column name → index in render order; `None` when not displayed.
fn column_index_from_coord(coord: &CellCoord, column_order: &[String]) -> Option<usize> {
    column_order.iter().position(|name| name == &coord.column_name)
}

/// `normalizeCellRange`: rows grow downwards, columns grow in render
/// order — never lexicographic (`id` sorts before `name` while the grid
/// may show the opposite). An unknown column has no position to compare,
/// so the range is "not found".
pub(crate) fn normalize_cell_range(
    range: &CellRange,
    column_order: &[String],
) -> Option<CellRange> {
    let start_column = column_index_from_coord(&range.start, column_order)?;
    let end_column = column_index_from_coord(&range.end, column_order)?;
    let top_row = range.start.row_index.min(range.end.row_index);
    let bottom_row = range.start.row_index.max(range.end.row_index);
    let (left, right) = if start_column <= end_column {
        (&range.start, &range.end)
    } else {
        (&range.end, &range.start)
    };
    Some(CellRange {
        start: CellCoord { row_index: top_row, column_name: left.column_name.clone() },
        end: CellCoord { row_index: bottom_row, column_name: right.column_name.clone() },
    })
}

/// Normalized anchor→focus rectangle; `None` unless both corners exist
/// and are displayable.
pub(crate) fn normalize_selection_range(
    selection: &GridSelection,
    column_order: &[String],
) -> Option<CellRange> {
    let anchor = selection.anchor.as_ref()?;
    let focus = selection.focus.as_ref()?;
    normalize_cell_range(&CellRange { start: anchor.clone(), end: focus.clone() }, column_order)
}

/// Row indices covered by the selection (unordered, de-duplicated). The
/// only source for row-level batch actions in `cell` mode; it is never
/// written back into `selected_rows` (00-contracts §4.2, rule 3).
pub(crate) fn rows_covered_by_selection(
    selection: &GridSelection,
    selected_rows: &std::collections::HashSet<usize>,
) -> std::collections::HashSet<usize> {
    use crate::table_data_connection_state::GridSelectionMode;
    if selection.mode == GridSelectionMode::Row {
        return selected_rows.clone();
    }
    if selection.mode != GridSelectionMode::Cell {
        return std::collections::HashSet::new();
    }
    let (Some(anchor), Some(focus)) = (selection.anchor.as_ref(), selection.focus.as_ref()) else {
        return std::collections::HashSet::new();
    };
    // Row coverage never depends on column order, so the rectangle is
    // rebuilt directly from the two corners.
    let mut rows = std::collections::HashSet::new();
    let add_range_rows =
        |start: &CellCoord, end: &CellCoord, rows: &mut std::collections::HashSet<usize>| {
            let top = start.row_index.min(end.row_index);
            let bottom = start.row_index.max(end.row_index);
            for row_index in top..=bottom {
                rows.insert(row_index);
            }
        };
    add_range_rows(anchor, focus, &mut rows);
    for extra in &selection.extra_ranges {
        add_range_rows(&extra.start, &extra.end, &mut rows);
    }
    rows
}

/// Every coordinate of a normalized rectangle, row-major. The **one**
/// place a region is expanded into coordinates (00-contracts §4.1).
pub(crate) fn cell_coords_in_range(range: &CellRange, column_order: &[String]) -> Vec<CellCoord> {
    let Some(normalized) = normalize_cell_range(range, column_order) else {
        return Vec::new();
    };
    let Some(left) = column_index_from_coord(&normalized.start, column_order) else {
        return Vec::new();
    };
    let Some(right) = column_index_from_coord(&normalized.end, column_order) else {
        return Vec::new();
    };
    let mut coords = Vec::new();
    for row_index in normalized.start.row_index..=normalized.end.row_index {
        for column in left..=right {
            if let Some(column_name) = column_order.get(column) {
                coords.push(CellCoord { row_index, column_name: column_name.clone() });
            }
        }
    }
    coords
}

// ── CellPasteWrite (lib/tableChanges.ts, carried until T629) ─────────────────

/// One cell a paste wants to write (`CellPasteWrite`). "Do not write"
/// cells are simply absent from a plan's list; `value: None` always
/// means "write SQL NULL" and nothing else.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CellPasteWrite {
    /// 0-based row index inside the current page. Render-only: never
    /// used to build SQL.
    pub row_index: usize,
    /// **Column name**, not index: indices drift when columns are hidden
    /// or reordered.
    pub column_name: String,
    /// The value to write; `None` means SQL `NULL`.
    pub value: Option<Value>,
}

/// Whether a column may be written by a **paste** (`ColumnWritability`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ColumnWritability {
    pub writable: bool,
    /// Why not, when not writable. `None` when writable.
    pub reason: Option<&'static str>,
}

/// Column-level gate for pasting. Deliberately **not** wired into the
/// single-cell editing path: a double-click edit is an explicit
/// per-column action; a paste is a bulk action the user did not aim, so
/// an auto-increment column is skipped wholesale. `ColumnSchema` has no
/// `isGenerated`, so a generated column cannot be told apart from an
/// ordinary one with a default — using the default instead would mark
/// perfectly writable columns read-only.
pub(crate) fn column_writability(column: Option<&ColumnSchema>) -> ColumnWritability {
    match column {
        None => ColumnWritability { writable: false, reason: Some("unknown-column") },
        Some(column) if column.is_auto_increment => {
            ColumnWritability { writable: false, reason: Some("auto-increment") }
        }
        Some(_) => ColumnWritability { writable: true, reason: None },
    }
}

/// What one paste-chunk staging did, for the caller's summary line
/// (`StagePasteWritesResult`, `stores/tableData/pendingEditActions.ts`).
/// The staging action itself is host-injected — the panel's action
/// table supplies it, exactly as the React hook receives it via
/// options.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StagePasteWritesResult {
    /// Cells that really entered `pendingChanges`.
    pub staged: usize,
    /// Cells that were skipped (the reason is in `problems`).
    pub skipped: usize,
    pub problems: Vec<PasteCellProblem>,
}

/// One staging refusal (`PasteCellProblem`): which cell, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PasteCellProblem {
    pub row_index: usize,
    pub column_name: String,
    pub code: &'static str,
    pub detail: Option<String>,
}

// ── Copy source tracking (cross-table guard, L1) ─────────────────────────────

/// Where the last in-app copy came from. `column_names[index]` belongs
/// to matrix column `index`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClipboardSource {
    /// Table identity, from [`table_change_key`].
    pub table_key: String,
    /// Source column names, in matrix order. A TSV copy carries no
    /// header, but this does.
    pub column_names: Vec<String>,
    /// Source column type families (DB-05 will offer the finer
    /// `CellType`; the family is what exists in commit order).
    pub column_types: Vec<DataTypeFamily>,
}

/// The pair React keeps as two module-level `let`s: the source, and the
/// exact text that proves it. On its own the source is **not** bound to
/// what is on the clipboard right now — copying in Excel leaves it
/// untouched — so it is only ever read through
/// [`source_for_clipboard_text`], never trusted bare.
#[derive(Default)]
struct CopySourceRecord {
    source: Option<ClipboardSource>,
    /// The normalized text this application put on the clipboard;
    /// `None` when the recording call could not say.
    text: Option<String>,
}

thread_local! {
    static COPY_SOURCE: RefCell<CopySourceRecord> = const { RefCell::new(CopySourceRecord {
        source: None,
        text: None,
    }) };
}

/// `\r\n` (and a lone `\r`) → `\n`. Normalising CRLF is not cosmetic:
/// Windows rewrites `\n` to `\r\n` when text goes through the clipboard,
/// so a byte comparison would report "changed" on every Windows round
/// trip and put a confirmation in front of every same-table paste.
fn normalize_clipboard_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            _ => out.push(ch),
        }
    }
    out
}

/// `recordClipboardSource`: remember the source together with the text
/// that proves it. A caller that cannot say what it copied leaves the
/// text unknown, so the source stops being usable for paste decisions —
/// the failure direction is a confirmation, not a write.
pub(crate) fn record_clipboard_source(source: ClipboardSource, text: Option<&str>) {
    COPY_SOURCE.with(|record| {
        let mut record = record.borrow_mut();
        record.source = Some(source);
        record.text = text.map(normalize_clipboard_text);
    });
}

/// Read-only probe; tests may also use it.
pub(crate) fn peek_clipboard_source() -> Option<ClipboardSource> {
    COPY_SOURCE.with(|record| record.borrow().source.clone())
}

/// The recorded source **if and only if** the clipboard still holds
/// what we copied. This is the function a paste must use;
/// `peek_clipboard_source` answers a different (wrong) question.
pub(crate) fn source_for_clipboard_text(text: &str) -> Option<ClipboardSource> {
    COPY_SOURCE.with(|record| {
        let record = record.borrow();
        let source = record.source.as_ref()?;
        let recorded = record.text.as_ref()?;
        (normalize_clipboard_text(text) == *recorded).then(|| source.clone())
    })
}

/// `clearClipboardSource` (the tests' reset).
pub(crate) fn clear_clipboard_source() {
    COPY_SOURCE.with(|record| *record.borrow_mut() = CopySourceRecord::default());
}

// ── Row copy payload (DBR-02: copy existing rows → paste as new rows) ────────

/// What a **row-mode** copy actually copied: the values, not just the
/// names. TSV cannot round-trip a row faithfully — `null` and `''` both
/// serialize to an empty cell — so "paste as new rows" needs the matrix
/// itself. Like the source, this payload is only a claim about the past
/// until it is verified against the clipboard text at paste time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowCopyPayload {
    /// Table identity, from [`table_change_key`].
    pub table_key: String,
    /// Source column names, in matrix column order.
    pub column_names: Vec<String>,
    /// One entry per copied row, aligned with `column_names`;
    /// `Value::Null` = SQL NULL.
    pub rows: Vec<Vec<Value>>,
}

thread_local! {
    static ROW_COPY: RefCell<(Option<RowCopyPayload>, Option<String>)> =
        const { RefCell::new((None, None)) };
}

/// Record a row copy together with the clipboard text that proves it.
pub(crate) fn record_row_copy(payload: RowCopyPayload, text: &str) {
    ROW_COPY.with(|record| {
        *record.borrow_mut() = (Some(payload), Some(normalize_clipboard_text(text)));
    });
}

/// Read-only probe; tests may also use it.
pub(crate) fn peek_row_copy() -> Option<RowCopyPayload> {
    ROW_COPY.with(|record| record.borrow().0.clone())
}

/// The recorded row payload **if and only if** the clipboard still holds
/// exactly what the row copy put there. An Excel copy in between leaves
/// a payload that describes data the clipboard no longer carries — the
/// paste must not trust it.
pub(crate) fn row_copy_for_clipboard_text(text: &str) -> Option<RowCopyPayload> {
    ROW_COPY.with(|record| {
        let record = record.borrow();
        let payload = record.0.as_ref()?;
        let recorded = record.1.as_ref()?;
        (normalize_clipboard_text(text) == *recorded).then(|| payload.clone())
    })
}

/// `clearRowCopy` (the tests' reset).
pub(crate) fn clear_row_copy() {
    ROW_COPY.with(|record| *record.borrow_mut() = (None, None));
}

/// Stable identity of "the table" for cross-table detection. Three
/// traps, all load-bearing: never `dbSessionId` (it changes on every
/// reconnect); lower-case everything (schemas are case-sensitive in
/// PostgreSQL and not in MySQL); `null` and `''` are the same table
/// (`database` is null for SQLite).
pub(crate) fn table_change_key(context: Option<&TableChangeContext>) -> String {
    let Some(context) = context else { return String::new() };
    let normalize = |value: Option<&str>| -> String {
        value.unwrap_or_default().trim().to_lowercase()
    };
    let parts = [
        normalize(context.connection_id.as_deref()),
        normalize(context.database.as_deref()),
        normalize(context.schema.as_deref()),
        normalize(Some(context.table.as_str())),
    ];
    // JSON.stringify of the 4-string array, byte for byte.
    Value::Array(parts.iter().map(|part| json!(part)).collect()).to_string()
}

/// `(rowIndex, columnName)` → stable key, shared with
/// `pending_cell_keys` and the tests.
pub(crate) fn pending_cell_key(row_index: usize, column_name: &str) -> String {
    format!("{row_index}:{column_name}")
}

// ── Paste planning ───────────────────────────────────────────────────────────

/// `PasteOverflow`: `Expand` grows the region (the PRD behaviour),
/// `Clip` keeps the paste inside the selected rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PasteOverflow {
    #[default]
    Expand,
    Clip,
}

/// `PastePlanOptions` (the defaults resolved: `overflow` = Expand,
/// `null_literal` = "NULL").
pub(crate) struct PastePlanOptions<'a> {
    pub matrix: &'a ClipboardMatrix,
    /// Landing cell — always the **top-left** of the normalized region,
    /// never the raw anchor.
    pub anchor: &'a CellCoord,
    /// Displayed columns, in render order.
    pub column_order: &'a [String],
    /// The page's column metadata (`TableState.columns`).
    pub columns: &'a [ColumnSchema],
    /// The original region when pasting into a region; `None` for a
    /// single cell.
    pub region: Option<GridClipboardRange>,
    /// Rows on the current page: both the expansion bound and the
    /// truncation bound.
    pub page_row_count: usize,
    pub overflow: PasteOverflow,
    /// Case-insensitive NULL literal (default `'NULL'`); `\N` is always
    /// recognized as well.
    pub null_literal: &'a str,
    /// `(rowIndex, columnName)` keys already staged, to count what the
    /// paste will overwrite.
    pub pending_cell_keys: Option<&'a std::collections::HashSet<String>>,
    /// Where the clipboard content came from, when this application
    /// copied it (L1).
    pub source: Option<&'a ClipboardSource>,
    /// The target table's context, used only for the same-table test.
    pub context: Option<&'a TableChangeContext>,
}

/// How the block was aligned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PasteAlignment {
    ByName,
    ByPosition,
    /// The block never reached planning (empty/oversized).
    #[default]
    Unknown,
}

/// `PastePlan`: only the cells to write; "do not write" cells are
/// absent from `writes`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PastePlan {
    pub writes: Vec<CellPasteWrite>,
    /// Area actually written; `None` when nothing is written.
    pub bounds: Option<GridClipboardRange>,
    pub problems: Vec<PasteProblem>,
    /// Cells skipped (read-only column, NOT NULL conflict, out of
    /// bounds, off-grid).
    pub skipped: usize,
    /// Staged changes the paste will overwrite.
    pub overwritten_pending: usize,
    /// Columns skipped wholesale because they are not writable
    /// (deduplicated, in order).
    pub skipped_columns: Vec<String>,
    /// `None` = the source is unknown (it cannot be judged), which is
    /// **not** the same as `Some(false)`.
    pub cross_table: Option<bool>,
    pub alignment: PasteAlignment,
    /// Source columns dropped by name alignment. **Not one of their
    /// cells is written.**
    pub dropped_columns: Vec<String>,
    /// The caller **must** confirm before staging. Always `true` unless
    /// the source proves same-table.
    pub requires_confirmation: bool,
    /// Source columns that found a same-named target column (0 when
    /// aligning by position).
    pub matched_columns: usize,
    /// Matrix width, for the confirmation text.
    pub total_columns: usize,
}

/// No matrix reached planning: nothing is aligned, nothing is written.
fn empty_plan(width: usize) -> PastePlan {
    PastePlan {
        writes: Vec::new(),
        bounds: None,
        problems: Vec::new(),
        skipped: 0,
        overwritten_pending: 0,
        skipped_columns: Vec::new(),
        cross_table: None,
        alignment: PasteAlignment::Unknown,
        dropped_columns: Vec::new(),
        requires_confirmation: true,
        matched_columns: 0,
        total_columns: width,
    }
}

fn is_null_text(text: &str, null_literal: &str) -> bool {
    text == "\\N" || text.to_lowercase() == null_literal.to_lowercase()
}

/// Plan one paste: which cells to write, with which values, and what to
/// tell the user.
///
/// The three-state rule is honoured by **omission**: a cell the block
/// does not cover, a column that cannot be written, and a cell beyond
/// the page are not present in `writes` at all. Aligning by **column
/// name** whenever the source is known (L2) keeps a paste from silently
/// writing `email` into the `amount` column when two tables order their
/// columns differently; falling back to positions plus a mandatory
/// confirmation (L3) is the last line of defence when the source is
/// unknown (an unknown source is treated as "possibly another table",
/// never as "probably the same one").
pub(crate) fn plan_paste(options: &PastePlanOptions) -> PastePlan {
    let matrix = options.matrix;
    let height = matrix.cells.len();
    let width = matrix.cells.first().map_or(0, Vec::len);
    let mut plan = empty_plan(width);

    if height == 0 || width == 0 {
        plan.problems.push(PasteProblem::new(paste_code::EMPTY));
        return plan;
    }
    let cell_count = height * width;
    if cell_count > GRID_PASTE_MAX_CELLS || matrix.byte_length > GRID_PASTE_MAX_BYTES {
        plan.problems.push(PasteProblem {
            code: paste_code::TOO_LARGE,
            row: None,
            column: None,
            detail: Some(cell_count.to_string()),
        });
        return plan;
    }

    // ── L2: column-name alignment when the source is known ─────────────
    let source_names = options.source.map(|source| source.column_names.as_slice());
    let mut alignment = PasteAlignment::ByPosition;
    let mut mapping: Vec<Option<String>> = Vec::new();
    let mut dropped_columns: Vec<String> = Vec::new();
    let mut matched_columns = 0usize;
    if let Some(source_names) = source_names.filter(|names| names.len() == width) {
        let by_name: Vec<Option<&str>> = source_names
            .iter()
            .map(|name| {
                options.column_order.iter().any(|target| target == name).then(|| name.as_str())
            })
            .collect();
        let matched = by_name.iter().filter(|name| name.is_some()).count();
        if matched > 0 {
            alignment = PasteAlignment::ByName;
            mapping = by_name.into_iter().map(|name| name.map(str::to_owned)).collect();
            matched_columns = matched;
            let mut dropped = std::collections::HashSet::new();
            for name in source_names {
                if !options.column_order.iter().any(|target| target == name) {
                    dropped.insert(name.clone());
                }
            }
            dropped_columns = dropped.into_iter().collect();
        }
    }
    if alignment == PasteAlignment::ByPosition {
        let landing = options
            .region
            .as_ref()
            .map(|region| region.left_column.as_str())
            .unwrap_or(options.anchor.column_name.as_str());
        let Some(left_index) =
            options.column_order.iter().position(|name| name == landing)
        else {
            plan.problems.push(PasteProblem::new(paste_code::NO_WRITABLE_CELL));
            return plan;
        };
        mapping = (0..width)
            .map(|column| options.column_order.get(left_index + column).cloned())
            .collect();
        matched_columns = 0;
        dropped_columns = Vec::new();
    }

    // ── L1/L3: same-table proof and the confirmation gate ──────────────
    let source_key = options.source.map(|source| source.table_key.clone());
    let target_key = options.context.map(|context| table_change_key(Some(context)));
    let cross_table = match (source_key.as_deref(), target_key.as_deref()) {
        (Some(source_key), Some(target_key)) => Some(source_key != target_key),
        _ => None,
    };
    let same_table = match (source_key.as_deref(), target_key.as_deref()) {
        (Some(source_key), Some(target_key)) => source_key == target_key,
        _ => false,
    };
    let requires_confirmation = !same_table;

    plan.alignment = alignment;
    plan.cross_table = cross_table;
    plan.requires_confirmation = requires_confirmation;
    plan.matched_columns = matched_columns;
    plan.total_columns = width;
    plan.dropped_columns = dropped_columns;

    let row_start = options.anchor.row_index;
    if options.page_row_count == 0 {
        plan.problems.push(PasteProblem::new(paste_code::OUT_OF_BOUNDS));
        return plan;
    }

    let column_by_name: std::collections::HashMap<&str, &ColumnSchema> =
        options.columns.iter().map(|column| (column.name.as_str(), column)).collect();
    // `clip`'s right edge: the region's right column in render order.
    // `None` is React's `indexOf(...) === -1` — no displayed column
    // qualifies, so every position is "beyond" and nothing lands.
    let clip_right_edge: Option<usize> = options.region.as_ref().and_then(|region| {
        options.column_order.iter().position(|name| *name == region.right_column)
    });
    let bottom_edge = if options.overflow == PasteOverflow::Clip {
        options
            .region
            .as_ref()
            .map(|region| region.bottom_row.min(options.page_row_count - 1))
            .unwrap_or(options.page_row_count - 1)
    } else {
        options.page_row_count - 1
    };

    let mut writes: Vec<CellPasteWrite> = Vec::new();
    let mut problems: Vec<PasteProblem> = Vec::new();
    let mut unwritable_columns: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut skipped_column_order: Vec<String> = Vec::new();
    let mut reported_columns: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut skipped = 0usize;
    let mut clipped = 0usize;
    let mut overwritten_pending = 0usize;
    let mut min_row = usize::MAX;
    let mut max_row = usize::MIN;
    let mut min_column = usize::MAX;
    let mut max_column = usize::MIN;

    for matrix_row in 0..height {
        let row_index = row_start + matrix_row;
        if row_index > bottom_edge {
            clipped += width;
            continue;
        }
        for matrix_column in 0..width {
            let Some(column_name) = mapping.get(matrix_column).and_then(Option::as_deref) else {
                skipped += 1;
                continue;
            };
            let Some(column_position) =
                options.column_order.iter().position(|name| name == column_name)
            else {
                skipped += 1;
                continue;
            };
            // `Clip` keeps a paste inside the rectangle **whichever way
            // it was aligned**: a name-aligned block can map to a column
            // outside the region, and writing there would contradict the
            // promise that `clip` never leaves the rectangle. Position
            // alignment simply runs out of displayed columns instead.
            if options.overflow == PasteOverflow::Clip
                && clip_right_edge.is_none_or(|edge| column_position > edge)
            {
                skipped += 1;
                continue;
            }
            let column = column_by_name.get(column_name).copied();
            let writability: ColumnWritability = column_writability(column);
            if !writability.writable {
                skipped += 1;
                unwritable_columns.insert(column_name);
                if writability.reason == Some("auto-increment")
                    && !skipped_column_order.iter().any(|name| name == column_name)
                {
                    skipped_column_order.push(column_name.to_owned());
                }
                if reported_columns.insert(column_name) {
                    problems.push(PasteProblem {
                        code: paste_code::CELL_READ_ONLY,
                        row: None,
                        column: Some(column_name.to_owned()),
                        detail: writability.reason.map(str::to_owned),
                    });
                }
                continue;
            }
            let text = matrix
                .cells
                .get(matrix_row)
                .and_then(|row| row.get(matrix_column))
                .map(String::as_str)
                .unwrap_or("");
            let quoted = matrix
                .quoted
                .get(matrix_row)
                .and_then(|row| row.get(matrix_column))
                .is_some_and(|flag| *flag);
            let value: Option<Value> = if quoted {
                // A quoted field is an explicit string, even when it is
                // empty or literally "NULL".
                Some(Value::String(text.to_owned()))
            } else if text.is_empty() || is_null_text(text, options.null_literal) {
                if column.is_some_and(|column| !column.nullable) {
                    skipped += 1;
                    problems.push(PasteProblem {
                        code: paste_code::NOT_NULL_EMPTY,
                        row: Some(matrix_row),
                        column: Some(column_name.to_owned()),
                        detail: None,
                    });
                    continue;
                }
                None
            } else {
                Some(Value::String(text.to_owned()))
            };
            if options
                .pending_cell_keys
                .is_some_and(|keys| keys.contains(&pending_cell_key(row_index, column_name)))
            {
                overwritten_pending += 1;
            }
            writes.push(CellPasteWrite {
                row_index,
                column_name: column_name.to_owned(),
                value,
            });
            min_row = min_row.min(row_index);
            max_row = max_row.max(row_index);
            min_column = min_column.min(column_position);
            max_column = max_column.max(column_position);
        }
    }

    if alignment == PasteAlignment::ByName && !plan.dropped_columns.is_empty() {
        problems.push(PasteProblem {
            code: paste_code::COLUMN_MISMATCH,
            row: None,
            column: None,
            detail: Some(plan.dropped_columns.join(", ")),
        });
    }
    if clipped > 0 {
        problems.push(PasteProblem {
            code: paste_code::OUT_OF_BOUNDS,
            row: None,
            column: None,
            detail: Some(clipped.to_string()),
        });
    }
    if writes.is_empty() && !unwritable_columns.is_empty() {
        let target_columns: std::collections::HashSet<&str> =
            mapping.iter().filter_map(|name| name.as_deref()).collect();
        if !target_columns.is_empty() && unwritable_columns.len() >= target_columns.len() {
            problems.push(PasteProblem::new(paste_code::NO_WRITABLE_CELL));
        }
    }

    plan.writes = writes;
    plan.problems = problems;
    plan.skipped = skipped + clipped;
    plan.overwritten_pending = overwritten_pending;
    plan.skipped_columns = skipped_column_order;
    plan.bounds = (!plan.writes.is_empty()).then(|| GridClipboardRange {
        top_row: min_row,
        bottom_row: max_row,
        left_column: options
            .column_order
            .get(min_column)
            .cloned()
            .unwrap_or_else(|| options.anchor.column_name.clone()),
        right_column: options
            .column_order
            .get(max_column)
            .cloned()
            .unwrap_or_else(|| options.anchor.column_name.clone()),
    });
    plan
}

/// `recordClipboardSource`'s `columnTypes`: the family each source
/// column classifies into (`classifyDataType` with no driver hints —
/// the hint-less call is exactly the behaviour a caller that has no
/// `DatabaseTypeMeta` at hand gets, 00-contracts R4).
pub(crate) fn classify_data_type_family(data_type: Option<&str>) -> DataTypeFamily {
    cell_type_to_data_type_family(resolve_cell_type(data_type, None))
}
