//! Pure helpers carried for the useGridClipboard port (T294).
//!
//! Source of truth: `packages/frontend/src/lib/gridClipboard.ts` (the
//! clipboard parse/serialize kernel, DB-03) plus `lib/gridErrors.ts`'s
//! `PasteProblem` shape. Both files are **another segment's** ledger
//! entries (`lib/gridClipboard.ts` lands as T523, `lib/gridErrors.ts` as
//! T525); this module carries the functions the hook's port needs until
//! those tasks land and absorb them — the same `lib_helpers` pattern the
//! SqlEditor port (`components/sql_editor/lib_helpers.rs`) and the
//! useColumnStats port established. Kept in a sibling module so the hook
//! file itself stays under the 800-line limit.
//!
//! Not carried (no caller in this port): `toClipboardRange` (the hook
//! builds its ranges inline), the single-field faces `escapeCsvField` /
//! `escapeMarkdownField` (dataTableContextMenu's, DB-10), and the
//! `classifyGridError` prefix registry (back-end prose classification —
//! the paste path carries codes structurally).
//!
//! Known deltas (behaviour-preserving unless noted):
//!
//! - `utf8ByteLength`: JS counts UTF-16 code units and re-assembles
//!   surrogate pairs by hand; Rust's `str::len()` **is** the UTF-8 byte
//!   length, so the loop disappears.
//! - `scanBlock`/`sniffClipboardDelimiter` walk `char`s (Unicode scalar
//!   values), not UTF-16 code units. Every structural character
//!   (delimiter, `"`, CR, LF) is ASCII, so multi-byte content simply
//!   copies through — the JS loop's own invariant.
//! - `String(number)`: JS prints safe integers without a fraction and
//!   switches to exponent form at 1e21 (`"1e+21"`); Rust's `Display`
//!   keeps the plain digits (`"1000000000000000000000"`). Grid values
//!   that large do not survive a JSON round trip byte-identically in
//!   either stack, and no format test in the React suite pins it.
//! - A JSON `1.0` stays `1.0` through serde while JS collapses it to
//!   `1` (`JSON.parse` has no float shape). Only the `json` clipboard
//!   format can observe this, and only for values the wire itself
//!   spelled with a trailing zero.

use serde_json::Value;

use crate::table_data_connection_state::CellCoord;

// ── Limits (lib/gridClipboard.ts) ─────────────────────────────────────────────

/// Bytes of source text above which a paste is refused outright.
pub const GRID_PASTE_MAX_BYTES: usize = 10 * 1024 * 1024;
/// Cells above which a paste is refused outright.
pub const GRID_PASTE_MAX_CELLS: usize = 100_000;
/// Above this cell count the caller asks for confirmation; below it runs silently.
pub const GRID_PASTE_CONFIRM_CELLS: usize = 2_000;
/// Cells written per state patch; a larger paste is chunked so a frame stays under ~200ms.
pub const GRID_PASTE_CHUNK_CELLS: usize = 5_000;

// ── PasteProblem (lib/gridErrors.ts, carried until T525) ─────────────────────

/// The problem codes this port raises. Carried as `&'static str` — the
/// React side's closed string union, one table with the back end's
/// message prefixes (see `crates/datazen/src/table_editing`).
pub mod paste_code {
    pub const EMPTY: &str = "grid.paste.empty";
    pub const TOO_LARGE: &str = "grid.paste.tooLarge";
    pub const RAGGED_ROW: &str = "grid.paste.raggedRow";
    pub const UNCLOSED_QUOTE: &str = "grid.paste.unclosedQuote";
    pub const OUT_OF_BOUNDS: &str = "grid.paste.outOfBounds";
    pub const NO_WRITABLE_CELL: &str = "grid.paste.noWritableCell";
    pub const NOT_NULL_EMPTY: &str = "grid.paste.notNullEmpty";
    pub const IDENTITY_CHANGED: &str = "grid.paste.identityChanged";
    pub const CROSS_TABLE: &str = "grid.paste.crossTable";
    pub const COLUMN_MISMATCH: &str = "grid.paste.columnMismatch";
    pub const CLIPBOARD_WRITE_FAILED: &str = "grid.clipboard.writeFailed";
    pub const CLIPBOARD_READ_FAILED: &str = "grid.clipboard.readFailed";
    pub const CELL_READ_ONLY: &str = "grid.cell.readOnly";
    pub const PASTE_ROWS_NOT_ROW_DATA: &str = "grid.pasteRows.notRowData";
    pub const PASTE_ROWS_NO_WRITABLE_COLUMN: &str = "grid.pasteRows.noWritableColumn";
    pub const CELL_AMBIGUOUS_IDENTITY: &str = "grid.cell.ambiguousIdentity";
}

/// One structured problem of a paste plan (`PasteProblem`,
/// `lib/gridErrors.ts`). Produced entirely in the client: the code is
/// carried directly, never parsed back out of a message. `detail` never
/// carries a cell value — pasted content is user data and stays local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteProblem {
    /// The `grid.*` code (see [`paste_code`]).
    pub code: &'static str,
    /// 0-based clipboard-matrix row; `None` for a whole-batch problem.
    pub row: Option<usize>,
    /// Column name; `None` when the problem is not about one column.
    pub column: Option<String>,
    /// Extra explanation (a column-name list, a count).
    pub detail: Option<String>,
}

impl PasteProblem {
    /// `PasteProblem`'s object literal: row/column default to `null`.
    pub fn new(code: &'static str) -> Self {
        Self { code, row: None, column: None, detail: None }
    }
}

// ── The clipboard matrix (lib/gridClipboard.ts) ──────────────────────────────

/// Delimiters the parser understands. Tab wins because that is what
/// spreadsheets emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardDelimiter {
    Tab,
    Comma,
    Semicolon,
}

impl ClipboardDelimiter {
    fn as_char(self) -> char {
        match self {
            Self::Tab => '\t',
            Self::Comma => ',',
            Self::Semicolon => ';',
        }
    }
}

/// A rectangular landing area. Columns by name, rows by current-page index.
///
/// React `GridClipboardRange`; `GridClipboardAnchor` is React's local
/// `ClipboardCellCoord` — structurally the store's `CellCoord`, reused
/// here rather than mirrored a second time (the hook bridges the two
/// worlds; the lib layering that kept them apart in TS is a module
/// boundary, not a type boundary).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridClipboardRange {
    pub top_row: usize,
    pub bottom_row: usize,
    pub left_column: String,
    pub right_column: String,
}

/// The paste anchor: the cell a paste lands on (`GridClipboardAnchor`).
pub type GridClipboardAnchor = CellCoord;

/// A parsed two-dimensional block plus the per-cell quoting that carries
/// NULL semantics (`ClipboardMatrix`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardMatrix {
    /// Cell text: quotes stripped, `""` restored to `"`. At least one row,
    /// all rows equal width.
    pub cells: Vec<Vec<String>>,
    /// Same shape as `cells`; `true` = the source quoted this field, so
    /// `''` is an explicit empty string.
    pub quoted: Vec<Vec<bool>>,
    pub delimiter: ClipboardDelimiter,
    /// The source had at least one line break ("one cell" vs "N rows").
    pub multiline: bool,
    /// UTF-8 byte length of the source text.
    pub byte_length: usize,
}

/// `ClipboardParseResult`: the refusal is structured and total — an
/// empty, oversized or ragged block never yields partial output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardParseResult {
    Ok {
        matrix: ClipboardMatrix,
        problems: Vec<PasteProblem>,
    },
    Refused {
        /// `grid.paste.empty` / `grid.paste.tooLarge` / `grid.paste.raggedRow`.
        code: &'static str,
        cells: usize,
        byte_length: usize,
        /// First inconsistent matrix row (0-based) — ragged only.
        row: Option<usize>,
        /// Field count of the first row — ragged only.
        expected: Option<usize>,
        /// Field count of the offending row — ragged only.
        actual: Option<usize>,
    },
}

/// Sniff the delimiter **outside quotes**: tab > comma > semicolon. When
/// none occurs the block is one column — deliberately **not** a comma,
/// because guessing one would shred a single-cell value containing one.
pub fn sniff_clipboard_delimiter(text: &str) -> ClipboardDelimiter {
    let (mut tabs, mut commas, mut semicolons) = (0usize, 0usize, 0usize);
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        continue;
                    }
                    in_quotes = false;
                }
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_quotes = true,
            '\t' => tabs += 1,
            ',' => commas += 1,
            ';' => semicolons += 1,
            _ => {}
        }
    }
    if tabs > 0 {
        ClipboardDelimiter::Tab
    } else if commas > 0 {
        ClipboardDelimiter::Comma
    } else if semicolons > 0 {
        ClipboardDelimiter::Semicolon
    } else {
        ClipboardDelimiter::Tab
    }
}

/// The scanner's accumulators; `end_field` / `end_row` are the JS
/// closures, as methods so the borrow checker can prove them disjoint.
struct BlockScanner {
    cells: Vec<Vec<String>>,
    quoted: Vec<Vec<bool>>,
    row: Vec<String>,
    row_quoted: Vec<bool>,
    field: String,
    field_started: bool,
    field_quoted: bool,
    in_quotes: bool,
    last_was_row_break: bool,
}

impl BlockScanner {
    fn end_field(&mut self) {
        self.row.push(std::mem::take(&mut self.field));
        self.row_quoted.push(self.field_quoted);
        self.field_started = false;
        self.field_quoted = false;
    }

    fn end_row(&mut self) {
        self.end_field();
        self.cells.push(std::mem::take(&mut self.row));
        self.quoted.push(std::mem::take(&mut self.row_quoted));
        self.last_was_row_break = true;
    }
}

/// Scan the block one character at a time (React's per-code-unit loop;
/// every structural character is ASCII, so multi-byte content copies
/// through untouched and surrogate pairs cannot tear).
fn scan_block(text: &str, delimiter: ClipboardDelimiter) -> (Vec<Vec<String>>, Vec<Vec<bool>>, bool) {
    let mut scanner = BlockScanner {
        cells: Vec::new(),
        quoted: Vec::new(),
        row: Vec::new(),
        row_quoted: Vec::new(),
        field: String::new(),
        field_started: false,
        field_quoted: false,
        in_quotes: false,
        last_was_row_break: false,
    };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if scanner.in_quotes {
            match ch {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        scanner.field.push('"');
                    } else {
                        scanner.in_quotes = false;
                    }
                }
                '\r' => {
                    // A CR inside quotes is a newline; a CRLF pair is one.
                    scanner.field.push('\n');
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                _ => {
                    scanner.field.push(ch);
                    scanner.last_was_row_break = false;
                }
            }
            continue;
        }
        if ch == '"' && !scanner.field_started {
            scanner.in_quotes = true;
            scanner.field_quoted = true;
            scanner.field_started = true;
            scanner.last_was_row_break = false;
            continue;
        }
        if ch == delimiter.as_char() {
            scanner.end_field();
            scanner.last_was_row_break = false;
            continue;
        }
        match ch {
            '\n' => scanner.end_row(),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                scanner.end_row();
            }
            _ => {
                scanner.field.push(ch);
                scanner.field_started = true;
                scanner.last_was_row_break = false;
            }
        }
    }
    let unclosed_quote = scanner.in_quotes;
    // A trailing line break must not fabricate an empty row: `'a\n'` is
    // one row, not two.
    if !scanner.last_was_row_break {
        scanner.end_row();
    }
    (scanner.cells, scanner.quoted, unclosed_quote)
}

/// Parse clipboard text into a rectangular block (RFC 4180 style
/// quoting). An unclosed quote is **not** a refusal: everything after it
/// becomes one cell, and the caller is told so.
pub fn parse_clipboard_matrix(text: &str) -> ClipboardParseResult {
    let byte_length = text.len();
    // Empty means "nothing here", not "no visible character": a block of
    // unquoted empty fields (`'\t'`, `';'`) writes NULLs and is a good
    // paste by the NULL/empty rules the planner implements.
    let has_delimiter =
        text.contains('\t') || text.contains(',') || text.contains(';') || text.contains('"');
    if !has_delimiter && text.trim().is_empty() {
        return ClipboardParseResult::Refused {
            code: paste_code::EMPTY,
            cells: 0,
            byte_length,
            row: None,
            expected: None,
            actual: None,
        };
    }
    if byte_length > GRID_PASTE_MAX_BYTES {
        return ClipboardParseResult::Refused {
            code: paste_code::TOO_LARGE,
            cells: 0,
            byte_length,
            row: None,
            expected: None,
            actual: None,
        };
    }

    let delimiter = sniff_clipboard_delimiter(text);
    let (cells, quoted, unclosed_quote) = scan_block(text, delimiter);
    let width = cells.first().map_or(0, Vec::len);
    let cell_count = cells.len() * width;
    let ragged_row = cells.iter().position(|row| row.len() != width);
    if let Some(row) = ragged_row {
        return ClipboardParseResult::Refused {
            code: paste_code::RAGGED_ROW,
            cells: cell_count,
            byte_length,
            row: Some(row),
            expected: Some(width),
            actual: Some(cells.get(row).map_or(0, Vec::len)),
        };
    }
    if cell_count > GRID_PASTE_MAX_CELLS {
        return ClipboardParseResult::Refused {
            code: paste_code::TOO_LARGE,
            cells: cell_count,
            byte_length,
            row: None,
            expected: None,
            actual: None,
        };
    }

    let mut problems = Vec::new();
    if unclosed_quote {
        problems.push(PasteProblem::new(paste_code::UNCLOSED_QUOTE));
    }
    ClipboardParseResult::Ok {
        matrix: ClipboardMatrix {
            cells,
            quoted,
            delimiter,
            multiline: text.contains('\n') || text.contains('\r'),
            byte_length,
        },
        problems,
    }
}

// ── Selection → matrix ───────────────────────────────────────────────────────

/// Coordinates (produced by the region expansion) → positional matrix.
/// The coordinates arrive in row-major order; the function groups them
/// into rows and never walks a rectangle itself. A missing cell reads
/// as `Value::Null` (React's `?? null`).
pub fn build_selection_matrix(
    rows: &[std::collections::HashMap<String, Value>],
    coords: &[CellCoord],
) -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = Vec::new();
    let mut current_row_index = None;
    for coord in coords {
        if current_row_index != Some(coord.row_index) {
            out.push(Vec::new());
            current_row_index = Some(coord.row_index);
        }
        let cell = rows
            .get(coord.row_index)
            .and_then(|row| row.get(&coord.column_name))
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(current) = out.last_mut() {
            current.push(cell);
        }
    }
    out
}

/// Row indices → positional matrix over `column_order` (whole-row copy).
/// `row_indices` must come from `rows_covered_by_selection`; a caller
/// recomputing a row set here would create the second source of truth
/// the contracts forbid.
pub fn build_rows_matrix(
    rows: &[std::collections::HashMap<String, Value>],
    column_order: &[String],
    row_indices: &[usize],
) -> Vec<Vec<Value>> {
    let mut ordered: Vec<usize> = row_indices.to_vec();
    ordered.sort_unstable();
    ordered
        .into_iter()
        .map(|row_index| {
            column_order
                .iter()
                .map(|column_name| {
                    rows.get(row_index)
                        .and_then(|row| row.get(column_name))
                        .cloned()
                        .unwrap_or(Value::Null)
                })
                .collect()
        })
        .collect()
}

// ── Serialization (the single kernel) ────────────────────────────────────────

/// The formats the one serializer speaks (DB-03's three + DB-10's three).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardFormat {
    Tsv,
    Csv,
    Json,
    Plain,
    Html,
    Markdown,
}

/// `SerializeCellsOptions`. `include_header`: default `false` for tsv and
/// plain (Excel alignment), `true` for csv; html/markdown always write
/// one. `null_literal`: default `''`. `json_objects` / `json_indent`
/// shape the `json` flavor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SerializeCellsOptions<'a> {
    pub format: Option<ClipboardFormat>,
    /// Column names, in the order of the selection.
    pub column_names: &'a [String],
    pub include_header: Option<bool>,
    pub null_literal: Option<&'a str>,
    pub json_objects: Option<bool>,
    pub json_indent: Option<usize>,
}

/// One cell value as clipboard text. Objects become JSON, never
/// `[object Object]`.
fn cell_to_text(value: &Value, null_literal: &str) -> String {
    match value {
        Value::Null => null_literal.to_owned(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => {
            // Integers print exactly; floats take Rust's shortest form,
            // which matches JS's `String(number)` through the safe range.
            if number.is_u64() || number.is_i64() {
                number.to_string()
            } else {
                let as_f64 = number.as_f64().unwrap_or(f64::NAN);
                format!("{as_f64}")
            }
        }
        other @ (Value::Object(_) | Value::Array(_)) => other.to_string(),
    }
}

/// Quote one field. A field is quoted when it contains the delimiter, a
/// quote or a line break; inner quotes are doubled. This is what fixes
/// "a tab inside a value silently adds a column" (defect G2).
fn escape_delimited(text: &str, delimiter: char) -> String {
    if text.contains(delimiter) || text.contains('"') || text.contains('\n') || text.contains('\r')
    {
        let mut quoted = String::with_capacity(text.len() + 2);
        quoted.push('"');
        for ch in text.chars() {
            if ch == '"' {
                quoted.push('"');
            }
            quoted.push(ch);
        }
        quoted.push('"');
        quoted
    } else {
        text.to_owned()
    }
}

/// Positional row + column names → named record (`null` for a missing
/// cell).
pub fn row_to_named_record(
    column_names: &[String],
    row: &[Value],
) -> serde_json::Map<String, Value> {
    let mut record = serde_json::Map::new();
    for (index, name) in column_names.iter().enumerate() {
        record.insert(
            name.clone(),
            row.get(index).cloned().unwrap_or(Value::Null),
        );
    }
    record
}

/// Escape one HTML text node: `&`, `<` and `>` (DB-10). Quotes are not
/// escaped — the value never lands in an attribute.
pub fn escape_html_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Escape one Markdown table cell (DB-10): a pipe would end the cell, a
/// line break would end the **row**. `\|` and `<br>` are what GitHub and
/// Notion both render back into the original characters.
pub fn escape_markdown_cell_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                out.push_str("\\\\");
                if chars.peek() == Some(&'|') {
                    // `\\|` would read as "escaped backslash, then pipe":
                    // the pipe still needs its own escape.
                    chars.next();
                    out.push_str("\\|");
                }
            }
            '|' => out.push_str("\\|"),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str("<br>");
            }
            '\n' => out.push_str("<br>"),
            _ => out.push(ch),
        }
    }
    out
}

/// The width a serialized table spans: the column list, or the widest row.
fn table_width(rows: &[Vec<Value>], column_names: &[String]) -> usize {
    if !column_names.is_empty() {
        return column_names.len();
    }
    rows.iter().map(Vec::len).max().unwrap_or(0)
}

/// Positional matrix → clipboard text. **The one serializer** (DB-03 §5
/// Step 9): every copy/export path delegates here so no second escaping
/// strategy can drift from it.
pub fn serialize_cells(rows: &[Vec<Value>], options: &SerializeCellsOptions) -> String {
    let format = options.format.unwrap_or(ClipboardFormat::Tsv);
    let column_names = options.column_names;
    let null_literal = options.null_literal.unwrap_or("");

    if format == ClipboardFormat::Json {
        let payload: Vec<Value> = if options.json_objects == Some(true) {
            rows.iter()
                .map(|row| Value::Object(row_to_named_record(column_names, row)))
                .collect()
        } else {
            rows.iter().map(|row| Value::Array(row.clone())).collect()
        };
        return match options.json_indent {
            Some(indent) if indent > 0 => serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| Value::Array(payload.clone()).to_string()),
            _ => Value::Array(payload).to_string(),
        };
    }

    let width = table_width(rows, column_names);
    let cells_of = |row: &[Value]| -> Vec<String> {
        (0..width).map(|index| cell_to_text(row.get(index).unwrap_or(&Value::Null), null_literal)).collect()
    };

    match format {
        ClipboardFormat::Plain => {
            // No quoting at all (DB-10): the tab and the newline are the
            // only structure.
            let mut lines: Vec<String> = Vec::new();
            if options.include_header == Some(true) && !column_names.is_empty() {
                lines.push(column_names.join("\t"));
            }
            for row in rows {
                lines.push(cells_of(row).join("\t"));
            }
            lines.join("\n")
        }
        ClipboardFormat::Html => {
            // A `<table>` for the `text/html` clipboard flavor (DB-10).
            // The header is not optional here: a caller asking for a
            // *table* asked for the column names too.
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
                    let tds: String = cells_of(row)
                        .iter()
                        .map(|cell| format!("<td>{}</td>", escape_html_cell(cell)))
                        .collect();
                    lines.push(format!("<tr>{tds}</tr>"));
                }
                lines.push("</tbody>".to_owned());
            }
            lines.push("</table>".to_owned());
            lines.join("\n")
        }
        ClipboardFormat::Markdown => {
            // A GitHub-style pipe table (DB-10). The header row and its
            // separator are the syntax — without them GitHub renders
            // prose, not a table.
            let head: Vec<&str> = if !column_names.is_empty() {
                column_names.iter().map(String::as_str).collect()
            } else {
                vec![""; width]
            };
            let head_cells: Vec<String> =
                head.iter().map(|name| escape_markdown_cell_text(name)).collect();
            let separator: Vec<&str> = head.iter().map(|_| "---").collect();
            let mut lines = vec![
                format!("| {} |", head_cells.join(" | ")),
                format!("| {} |", separator.join(" | ")),
            ];
            for row in rows {
                let cells: Vec<String> =
                    cells_of(row).iter().map(|cell| escape_markdown_cell_text(cell)).collect();
                lines.push(format!("| {} |", cells.join(" | ")));
            }
            lines.join("\n")
        }
        ClipboardFormat::Tsv | ClipboardFormat::Csv => {
            let delimiter = if format == ClipboardFormat::Csv { ',' } else { '\t' };
            let include_header = options.include_header.unwrap_or(format == ClipboardFormat::Csv);
            // A caller with no column list (whole-row copy over
            // positional arrays) still needs the widest row as the width;
            // an empty header must not collapse every row to a blank line.
            let mut lines: Vec<String> = Vec::new();
            if include_header {
                let header: Vec<String> = (0..width)
                    .map(|index| {
                        escape_delimited(
                            column_names.get(index).map(String::as_str).unwrap_or(""),
                            delimiter,
                        )
                    })
                    .collect();
                lines.push(header.join(&delimiter.to_string()));
            }
            for row in rows {
                let cells: Vec<String> = cells_of(row)
                    .iter()
                    .map(|cell| escape_delimited(cell, delimiter))
                    .collect();
                lines.push(cells.join(&delimiter.to_string()));
            }
            lines.join("\n")
        }
        ClipboardFormat::Json => String::new(),
    }
}

// ── Selection geometry and the column gate live in
// `grid_paste_plan_helpers.rs` (also carried; see that file's header).
