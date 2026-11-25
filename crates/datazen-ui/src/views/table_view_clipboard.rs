//! `windows/connection/useTableViewClipboard.ts` — the TableView → clipboard
//! adapter and the decisions it delegates to `useGridClipboard` / `gridClipboard`,
//! ported as pure logic for T630. React's hook is a wiring shell (empty fallbacks,
//! panel-bound actions); the ported decisions are: cell-vs-row copy intent and
//! the paste gate (read-only → parse refusal → no writes → confirm → stage);
//! the single TSV/CSV/JSON/plain/HTML/Markdown serializer and clipboard parser;
//! selection/row → matrix and matrix → draft mapping; and reported problems,
//! paste summaries and the optimistic "copied" host-clock deadline. No timer,
//! IPC or promise lives here — the host reads the clipboard and applies actions.
//!
//! [`PasteProblem`] / [`PasteSummary`] are reused from the status-line port.
//! ## Consumer / removal stage
//! No production table host mounts this yet. Every reusable item carries a
//! per-item `#[allow(dead_code)]` with this stage as its removal note; the
//! attributes are dropped together when the host lands. All labels come from
//! the exact React keys via `t()`; no colour or inline icon is introduced.

use std::collections::BTreeMap;

use crate::i18n::t;
use crate::views::table_clipboard_status::{PasteProblem, PasteSummary};

pub mod state;

// ── Limits (React `gridClipboard.ts` / `useGridClipboard.ts`) ────────────────

/// React `GRID_PASTE_MAX_BYTES` — 10 MiB of source text.
#[allow(dead_code)] // limit mirror; consumed by the parser and tests.
pub const GRID_PASTE_MAX_BYTES: usize = 10 * 1024 * 1024;
/// React `GRID_PASTE_MAX_CELLS`.
#[allow(dead_code)] // limit mirror; consumed by the parser and tests.
pub const GRID_PASTE_MAX_CELLS: usize = 100_000;
/// React `GRID_PASTE_CONFIRM_CELLS` — above this a paste asks first.
#[allow(dead_code)] // limit mirror; consumed by the gate and tests.
pub const GRID_PASTE_CONFIRM_CELLS: usize = 2_000;
/// React `GRID_PASTE_CHUNK_CELLS` — cells per staging patch.
#[allow(dead_code)] // limit mirror; consumed by the gate and tests.
pub const GRID_PASTE_CHUNK_CELLS: usize = 5_000;
/// React `COPY_FEEDBACK_MS` — the grid's own "copied" window.
#[allow(dead_code)] // limit mirror; consumed by the feedback state.
pub const COPY_FEEDBACK_MS: f64 = 1_200.0;
/// React `MAX_REPORTED_PROBLEMS` — a big block must not flood the status line.
#[allow(dead_code)] // limit mirror; consumed by report_problem.
pub const MAX_REPORTED_PROBLEMS: usize = 100;

// ── Value / target mirrors ─────────────────────────────────────────────────

/// One cell value, the Rune spelling of React's `unknown` cell.
#[allow(dead_code)] // VM mirror; supplied by the host row feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipValue {
    Null,
    Text(String),
    Number(String),
    Bool(bool),
    Json(String),
    Blob(Vec<u8>),
}

impl ClipValue {
    #[allow(dead_code)] // pure helper; consumed by the serializer and tests.
    fn as_text(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Text(text) | Self::Number(text) | Self::Json(text) => text.clone(),
            Self::Bool(value) => value.to_string(),
            Self::Blob(bytes) => bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        }
    }
}

/// The formats React's one serializer speaks.
#[allow(dead_code)] // VM mirror; consumed by the serializer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardFormat {
    Tsv,
    Csv,
    Json,
    Plain,
    Html,
    Markdown,
}

/// A target column's writable metadata (React `ColumnSchema`'s copy-relevant half).
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipboardColumn {
    pub name: String,
    pub data_type: String,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
}

/// One row record, keyed by column name (React `Record<string, unknown>`).
#[allow(dead_code)] // VM mirror; supplied by the host row feed.
pub type ClipboardRow = BTreeMap<String, ClipValue>;

/// A normalized rectangular selection (React's `CellRange`).
#[allow(dead_code)] // VM mirror; supplied by the grid selection slice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellRange {
    pub top_row: usize,
    pub bottom_row: usize,
    pub left_column: String,
    pub right_column: String,
}

/// React `GridCopyTarget`.
#[allow(dead_code)] // decision mirror; consumed by the host and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyTarget {
    Cell {
        top_row: usize,
        bottom_row: usize,
        left_column: String,
        right_column: String,
    },
    Row {
        row_indices: Vec<usize>,
    },
}

/// Matrix plus the column names it spans, before serialization.
#[allow(dead_code)] // decision mirror; consumed by the host and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyMatrix {
    pub matrix: Vec<Vec<ClipValue>>,
    pub column_names: Vec<String>,
}

/// A cell coordinate in the selection model (column **name**, not index).
#[allow(dead_code)] // VM mirror; supplied by the grid selection slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellCoord {
    pub row_index: usize,
    pub column_name: String,
}

/// A recorded DataZen row copy (React `RowCopyPayload`).
#[allow(dead_code)] // VM mirror; recorded by a row copy, read by paste-rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowCopy {
    pub table_key: String,
    pub column_names: Vec<String>,
    pub rows: Vec<Vec<ClipValue>>,
}

// ── Clipboard text parsing ─────────────────────────────────────────────────

/// React `ClipboardParseResult` (the refusal half carries counts for the
/// problem text; the success half carries the matrix and its quoting bits).
#[allow(dead_code)] // deps mirror; consumed by the paste gate and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseResult {
    Empty { byte_length: usize },
    TooLarge { cells: usize, byte_length: usize },
    Ragged { row: usize, expected: usize, actual: usize },
    Ok {
        cells: Vec<Vec<String>>,
        quoted: Vec<Vec<bool>>,
        unclosed_quote: bool,
        multiline: bool,
        byte_length: usize,
    },
}

#[allow(dead_code)] // pure helper; remove with the host.
fn utf8_byte_length(text: &str) -> usize {
    text.chars().map(char::len_utf8).sum()
}

#[allow(dead_code)] // pure helper; remove with the host.
fn sniff_delimiter(text: &str) -> u8 {
    let (mut tabs, mut commas, mut semis) = (0usize, 0usize, 0usize);
    let mut in_quotes = false;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' if in_quotes && bytes.get(index + 1) == Some(&b'"') => index += 2,
            b'"' => {
                in_quotes = !in_quotes;
                index += 1;
            }
            b'\t' if !in_quotes => {
                tabs += 1;
                index += 1;
            }
            b',' if !in_quotes => {
                commas += 1;
                index += 1;
            }
            b';' if !in_quotes => {
                semis += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    if tabs > 0 {
        b'\t'
    } else if commas > 0 {
        b','
    } else if semis > 0 {
        b';'
    } else {
        b'\t'
    }
}

/// React `scanBlock` + `parseClipboardMatrix` in one total function.
#[allow(dead_code)] // pure helper; consumed by the paste gate and tests.
pub fn parse_clipboard_text(text: &str) -> ParseResult {
    let byte_length = utf8_byte_length(text);
    let has_delimiter = text.chars().any(|c| matches!(c, '\t' | ',' | ';' | '"'));
    if !has_delimiter && text.trim().is_empty() {
        return ParseResult::Empty { byte_length };
    }
    if byte_length > GRID_PASTE_MAX_BYTES {
        return ParseResult::TooLarge { cells: 0, byte_length };
    }
    let delimiter = sniff_delimiter(text);
    let mut cells: Vec<Vec<String>> = Vec::new();
    let mut quoted: Vec<Vec<bool>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut row_quoted: Vec<bool> = Vec::new();
    let mut field = String::new();
    let mut field_started = false;
    let mut field_quoted = false;
    let mut in_quotes = false;
    let mut last_was_break = false;
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_quotes {
            if ch == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    field.push('"');
                    index += 2;
                    continue;
                }
                in_quotes = false;
                index += 1;
                continue;
            }
            if ch == '\r' {
                field.push('\n');
                index += if chars.get(index + 1) == Some(&'\n') { 2 } else { 1 };
                continue;
            }
            field.push(ch);
            last_was_break = false;
            index += 1;
            continue;
        }
        if ch == '"' && !field_started {
            in_quotes = true;
            field_quoted = true;
            field_started = true;
            last_was_break = false;
            index += 1;
            continue;
        }
        if (delimiter == b'\t' && ch == '\t') || (delimiter == b',' && ch == ',') || (delimiter == b';' && ch == ';') {
            row.push(std::mem::take(&mut field));
            row_quoted.push(field_quoted);
            field_quoted = false;
            field_started = false;
            last_was_break = false;
            index += 1;
            continue;
        }
        if ch == '\n' || ch == '\r' {
            row.push(std::mem::take(&mut field));
            row_quoted.push(field_quoted);
            field_quoted = false;
            field_started = false;
            cells.push(std::mem::take(&mut row));
            quoted.push(std::mem::take(&mut row_quoted));
            last_was_break = true;
            index += if ch == '\r' && chars.get(index + 1) == Some(&'\n') { 2 } else { 1 };
            continue;
        }
        field.push(ch);
        field_started = true;
        last_was_break = false;
        index += 1;
    }
    if !last_was_break {
        row.push(field);
        row_quoted.push(field_quoted);
        cells.push(row);
        quoted.push(row_quoted);
    }
    let width = cells.first().map_or(0, Vec::len);
    let cell_count = cells.len() * width;
    if let Some((ragged, sample)) = cells.iter().enumerate().find(|(_, row)| row.len() != width) {
        return ParseResult::Ragged {
            row: ragged,
            expected: width,
            actual: sample.len(),
        };
    }
    if cell_count > GRID_PASTE_MAX_CELLS {
        return ParseResult::TooLarge { cells: cell_count, byte_length };
    }
    ParseResult::Ok {
        cells,
        quoted,
        unclosed_quote: in_quotes,
        multiline: text.contains('\n') || text.contains('\r'),
        byte_length,
    }
}

// ── The single serializer ──────────────────────────────────────────────────

#[allow(dead_code)] // pure helper; remove with the host.
fn escape_delimited(text: &str, delimiter: char) -> String {
    if text.contains(delimiter) || text.contains('"') || text.contains('\n') || text.contains('\r') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

#[allow(dead_code)] // pure helper; remove with the host.
fn escape_html_cell(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[allow(dead_code)] // pure helper; remove with the host.
fn escape_markdown_cell(text: &str) -> String {
    text.replace('\\', "\\\\").replace('|', "\\|")
        .replace("\r\n", "\n").replace('\r', "\n").replace('\n', "<br>")
}

#[allow(dead_code)] // pure helper; remove with the host.
fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"), '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// React `serializeCells` — the one TSV/CSV/JSON/plain/HTML/Markdown kernel.
#[allow(dead_code)] // pure helper; consumed by the copy intents and tests.
pub fn serialize_cells(
    rows: &[Vec<ClipValue>],
    format: ClipboardFormat,
    column_names: &[String],
    include_header: Option<bool>,
    null_literal: &str,
    json_objects: bool,
) -> String {
    let width = if column_names.is_empty() {
        rows.iter().map(Vec::len).max().unwrap_or(0)
    } else {
        column_names.len()
    };
    let cell_text = |row: &[ClipValue], index: usize| -> String {
        match row.get(index) {
            None | Some(ClipValue::Null) => null_literal.to_owned(),
            Some(value) => value.as_text(),
        }
    };
    // React's JSON branch uses the RAW values: nullLiteral is a delimited-text
    // concern only. rowToNamedRecord writes `row[index] ?? null` (and skips
    // empty names); positional JSON is `[...row]` verbatim, width included.
    let json_value = |value: Option<&ClipValue>| -> String {
        match value {
            None | Some(ClipValue::Null) => "null".to_owned(),
            Some(value) => json_string(&value.as_text()),
        }
    };
    let named = |row: &[ClipValue]| -> String {
        let fields: Vec<String> = column_names
            .iter()
            .enumerate()
            .filter(|(_, name)| !name.is_empty())
            .map(|(i, name)| format!("{}:{}", json_string(name), json_value(row.get(i))))
            .collect();
        format!("{{{}}}", fields.join(","))
    };
    let positional = |row: &[ClipValue]| -> String {
        let fields: Vec<String> = row.iter().map(|value| json_value(Some(value))).collect();
        format!("[{}]", fields.join(","))
    };

    match format {
        ClipboardFormat::Json => {
            let payload: Vec<String> = rows
                .iter()
                .map(|row| if json_objects { named(row) } else { positional(row) })
                .collect();
            format!("[{}]", payload.join(","))
        }
        ClipboardFormat::Plain => {
            let mut lines: Vec<String> = Vec::new();
            if include_header == Some(true) && !column_names.is_empty() {
                lines.push(column_names.join("\t"));
            }
            lines.extend(rows.iter().map(|row| {
                (0..width).map(|i| cell_text(row, i)).collect::<Vec<_>>().join("\t")
            }));
            lines.join("\n")
        }
        ClipboardFormat::Html => {
            let mut lines = vec!["<table>".to_owned()];
            if !column_names.is_empty() {
                let head: String = column_names
                    .iter()
                    .map(|name| format!("<th>{}</th>", escape_html_cell(name)))
                    .collect();
                lines.push(format!("<thead><tr>{head}</tr></thead>"));
            }
            if !rows.is_empty() {
                lines.push("<tbody>".to_owned());
                for row in rows {
                    let cells: String = (0..width)
                        .map(|i| format!("<td>{}</td>", escape_html_cell(&cell_text(row, i))))
                        .collect();
                    lines.push(format!("<tr>{cells}</tr>"));
                }
                lines.push("</tbody>".to_owned());
            }
            lines.push("</table>".to_owned());
            lines.join("\n")
        }
        ClipboardFormat::Markdown => {
            let head: Vec<String> = if column_names.is_empty() {
                (0..width).map(|_| String::new()).collect()
            } else {
                column_names.to_vec()
            };
            let head_cells: Vec<String> = head.iter().map(|name| escape_markdown_cell(name)).collect();
            let separator: Vec<&str> = head.iter().map(|_| "---").collect();
            let mut lines = vec![
                format!("| {} |", head_cells.join(" | ")),
                format!("| {} |", separator.join(" | ")),
            ];
            for row in rows {
                let cells: Vec<String> = (0..width)
                    .map(|i| escape_markdown_cell(&cell_text(row, i)))
                    .collect();
                lines.push(format!("| {} |", cells.join(" | ")));
            }
            lines.join("\n")
        }
        ClipboardFormat::Tsv | ClipboardFormat::Csv => {
            let delimiter = if format == ClipboardFormat::Csv { ',' } else { '\t' };
            let header = include_header.unwrap_or(format == ClipboardFormat::Csv);
            let mut lines: Vec<String> = Vec::new();
            if header {
                let cells: Vec<String> = (0..width)
                    .map(|i| escape_delimited(column_names.get(i).map_or("", String::as_str), delimiter))
                    .collect();
                lines.push(cells.join(&delimiter.to_string()));
            }
            for row in rows {
                let cells: Vec<String> = (0..width)
                    .map(|i| escape_delimited(&cell_text(row, i), delimiter))
                    .collect();
                lines.push(cells.join(&delimiter.to_string()));
            }
            lines.join("\n")
        }
    }
}

// ── Row / value mapping ────────────────────────────────────────────────────

/// React `buildSelectionMatrix`: coordinates in row-major order → a matrix,
/// reading each value from the row record by column **name**.
#[allow(dead_code)] // pure helper; consumed by copy and tests.
pub fn build_selection_matrix(rows: &[ClipboardRow], coords: &[CellCoord]) -> Vec<Vec<ClipValue>> {
    let mut out: Vec<Vec<ClipValue>> = Vec::new();
    let mut current_row = None;
    for coord in coords {
        if current_row != Some(coord.row_index) {
            out.push(Vec::new());
            current_row = Some(coord.row_index);
        }
        let value = rows
            .get(coord.row_index)
            .and_then(|row| row.get(&coord.column_name))
            .cloned()
            .unwrap_or(ClipValue::Null);
        if let Some(last) = out.last_mut() {
            last.push(value);
        }
    }
    out
}

/// React `buildRowsMatrix`: row indices → a matrix over `column_order`,
/// dropping invalid indices and sorting ascending.
#[allow(dead_code)] // pure helper; consumed by copy and tests.
pub fn build_rows_matrix(
    rows: &[ClipboardRow],
    column_order: &[String],
    row_indices: &[usize],
) -> Vec<Vec<ClipValue>> {
    let mut ordered: Vec<usize> = row_indices.iter().copied().collect();
    ordered.sort_unstable();
    ordered
        .into_iter()
        .map(|index| {
            column_order
                .iter()
                .map(|name| {
                    rows.get(index)
                        .and_then(|row| row.get(name))
                        .cloned()
                        .unwrap_or(ClipValue::Null)
                })
                .collect()
        })
        .collect()
}

/// React `rowsToDraftValues` (DBR-02): a copied matrix → draft values. Primary
/// keys, auto-increment and unknown columns are never copied; an explicit null
/// stays a null and only present keys are written.
#[allow(dead_code)] // pure helper; consumed by paste-rows and tests.
pub fn rows_to_draft_values(
    column_names: &[String],
    matrix: &[Vec<ClipValue>],
    columns: &[ClipboardColumn],
) -> Vec<BTreeMap<String, ClipValue>> {
    let writable: Vec<&str> = columns
        .iter()
        .filter(|column| !column.is_primary_key && !column.is_auto_increment)
        .map(|column| column.name.as_str())
        .collect();
    matrix
        .iter()
        .map(|row| {
            let mut values = BTreeMap::new();
            for (index, name) in column_names.iter().enumerate() {
                if !writable.iter().any(|candidate| candidate == name) {
                    continue;
                }
                if let Some(value) = row.get(index) {
                    values.insert(name.clone(), value.clone());
                }
            }
            values
        })
        .collect()
}

// ── Copy / paste intent ───────────────────────────────────────────────────

/// React `copySelectedCellsAs`'s target choice: a normalized cell selection
/// wins; otherwise the covered rows; otherwise there is nothing to copy.
#[allow(dead_code)] // decision; remove with the host.
pub fn copy_target(selection: Option<&CellRange>, covered: &[usize]) -> Option<CopyTarget> {
    if let Some(range) = selection {
        return Some(CopyTarget::Cell {
            top_row: range.top_row,
            bottom_row: range.bottom_row,
            left_column: range.left_column.clone(),
            right_column: range.right_column.clone(),
        });
    }
    if covered.is_empty() {
        return None;
    }
    let mut rows = covered.to_vec();
    rows.sort_unstable();
    Some(CopyTarget::Row { row_indices: rows })
}

#[allow(dead_code)] // gate; remove with the host.
pub fn can_copy(selection: Option<&CellRange>, covered: &[usize]) -> bool {
    selection.is_some() || !covered.is_empty()
}

#[allow(dead_code)] // gate; remove with the host.
pub fn can_paste(is_editable: bool, selection: Option<&CellRange>) -> bool {
    is_editable && selection.is_some()
}

#[allow(dead_code)] // gate; remove with the host.
pub fn can_use_row_copy(is_editable: bool, has_add_drafts: bool) -> bool {
    is_editable && has_add_drafts
}

/// React `plan.bounds` / the dropped-column and cross-table fields the paste
/// gate and confirmation read. The planner itself stays a host concern.
#[allow(dead_code)] // VM mirror; supplied by the paste planner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PastePlanInput {
    pub writes: usize,
    pub skipped: usize,
    pub overwritten_pending: usize,
    pub bounds: Option<CellRange>,
    pub matched_columns: usize,
    pub total_columns: usize,
    /// Displayed columns in the landing rectangle (React: right-left index + 1).
    pub columns: usize,
    pub dropped_columns: Vec<String>,
    pub requires_confirmation: bool,
    /// `None` = unknown source (React `crossTable === null`).
    pub cross_table: Option<bool>,
    pub source_key: String,
}

fn fmt(key: &str, pairs: &[(&str, String)]) -> String {
    let mut text = t(key).to_owned();
    for (name, value) in pairs {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// React `confirmOptionsFor` (large paste / cross-table headings, exact keys).
#[allow(dead_code)] // pure helper; remove with the host.
pub fn confirm_texts(plan: &PastePlanInput, table_label: &str) -> ConfirmTexts {
    let rows_written = plan
        .bounds
        .as_ref()
        .map_or(0, |b| b.bottom_row.saturating_sub(b.top_row) + 1);
    let columns_written = if plan.dropped_columns.is_empty() {
        plan.total_columns
    } else {
        plan.matched_columns
    };
    let body = match plan.cross_table {
        None => fmt(
            "dataTable.pasteUnknownSourceBody",
            &[("total", columns_written.to_string()), ("target", table_label.to_owned())],
        ),
        Some(_) => fmt(
            "dataTable.pasteCrossTableBody",
            &[
                ("source", plan.source_key.clone()),
                ("target", table_label.to_owned()),
                ("matched", plan.matched_columns.to_string()),
                ("total", plan.total_columns.to_string()),
            ],
        ),
    };
    let large = plan.writes > GRID_PASTE_CONFIRM_CELLS;
    let title = if large {
        fmt("dataTable.pasteConfirmTitle", &[("cells", plan.writes.to_string())])
    } else {
        t("dataTable.pasteCrossTableTitle").to_owned()
    };
    let message = if large {
        fmt(
            "dataTable.pasteConfirmMessage",
            &[
                ("rows", rows_written.to_string()),
                ("cols", columns_written.to_string()),
                ("overwritten", plan.overwritten_pending.to_string()),
            ],
        )
    } else {
        body
    };
    let description = if plan.dropped_columns.is_empty() {
        None
    } else {
        Some(fmt(
            "dataTable.pasteDroppedColumns",
            &[
                ("count", plan.dropped_columns.len().to_string()),
                ("names", plan.dropped_columns.iter().take(5).cloned().collect::<Vec<_>>().join(", ")),
            ],
        ))
    };
    ConfirmTexts {
        title,
        message,
        description,
        confirm_label: t("dataTable.pasteToSelectedCells").to_owned(),
        cancel_label: t("common.cancel").to_owned(),
    }
}

/// React's confirm dialog copy. `warning` is always the kind.
#[allow(dead_code)] // VM mirror; consumed by the host dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmTexts {
    pub title: String,
    pub message: String,
    pub description: Option<String>,
    pub confirm_label: String,
    pub cancel_label: String,
}

/// React `pasteText`'s ordered gate: read-only → parse refusal → no writes →
/// confirmation → staged chunks. Read-only still consumes the gesture.
#[allow(dead_code)] // decision; remove with the host.
pub fn decide_paste(
    is_editable: bool,
    parse: &ParseResult,
    plan: &PastePlanInput,
) -> PasteDecision {
    if !is_editable {
        return PasteDecision::ReadOnly;
    }
    if let ParseResult::Empty { .. }
    | ParseResult::TooLarge { .. }
    | ParseResult::Ragged { .. } = parse
    {
        return PasteDecision::ParseRefused;
    }
    if plan.writes == 0 {
        let mut empty = zero_shape(summary(plan, 0, plan.skipped, false));
        empty.overwritten_pending = 0; // React pins this to zero on a no-write
        return PasteDecision::NoWrites(empty);
    }
    if plan.requires_confirmation || plan.writes > GRID_PASTE_CONFIRM_CELLS {
        return PasteDecision::NeedConfirm;
    }
    PasteDecision::Stage {
        chunks: plan.writes.div_ceil(GRID_PASTE_CHUNK_CELLS),
    }
}

#[allow(dead_code)] // decision; remove with the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteDecision {
    ReadOnly,
    ParseRefused,
    NoWrites(PasteSummary),
    NeedConfirm,
    Stage { chunks: usize },
}

/// React's no-write and cancelled summaries both report zero rows/columns even
/// when a landing rectangle is known; only the staged path carries bounds.
fn zero_shape(mut result: PasteSummary) -> PasteSummary {
    result.rows = 0;
    result.columns = 0;
    result
}

fn summary(plan: &PastePlanInput, staged: usize, skipped: usize, cancelled: bool) -> PasteSummary {
    let rows = plan.bounds.as_ref().map_or(0, |b| b.bottom_row.saturating_sub(b.top_row) + 1);
    PasteSummary {
        cells: plan.writes,
        staged,
        skipped,
        overwritten_pending: plan.overwritten_pending,
        rows,
        columns: if plan.bounds.is_some() { plan.columns } else { 0 },
        cancelled,
    }
}

#[allow(dead_code)] // pure helper; remove with the host.
pub fn cancelled_summary(plan: &PastePlanInput) -> PasteSummary {
    zero_shape(summary(plan, 0, 0, true))
}

#[allow(dead_code)] // pure helper; remove with the host.
pub fn staged_summary(plan: &PastePlanInput, staged: usize, skipped: usize) -> PasteSummary {
    summary(plan, staged, skipped, false)
}

/// React `parseClipboardMatrix` failure → the problem the status line reports.
#[allow(dead_code)] // pure helper; remove with the host.
pub fn parse_problem(parse: &ParseResult) -> Option<PasteProblem> {
    let (code, detail) = match parse {
        ParseResult::Empty { .. } => ("grid.paste.empty", None),
        ParseResult::TooLarge { cells, .. } => ("grid.paste.tooLarge", Some(cells.to_string())),
        ParseResult::Ragged { row, expected, actual } => (
            "grid.paste.raggedRow",
            Some(fmt(
                "dataTable.pasteRaggedRow",
                &[
                    ("row", (row + 1).to_string()),
                    ("count", actual.to_string()),
                    ("expected", expected.to_string()),
                ],
            )),
        ),
        ParseResult::Ok { unclosed_quote: true, .. } => ("grid.paste.unclosedQuote", None),
        ParseResult::Ok { .. } => return None,
    };
    Some(PasteProblem {
        code: code.to_owned(),
        detail,
        ..PasteProblem::default()
    })
}
