//! Port of `components/sql-editor/format/formatEditorDocument.ts` (T220,
//! split) — the selection-aware SQL beautifier command.
//!
//! The decision logic is this module's; the formatter itself
//! (`lib/sqlFormat.ts`) is T615 (segment 2) and stays injected — the
//! seam [`FormatSql`] takes the dialect and the resolved options, and a
//! rejection (sql-formatter throws on syntax it cannot parse) leaves
//! the buffer untouched: mid-edit SQL is routinely unparseable, and
//! corrupting it would be far worse than doing nothing.
//!
//! ## What maps where
//!
//! - `EditorView` collapses into the caller's editor: the command takes
//!   the document, the selection and the cursor, and returns one
//!   [`FormatEdit`] (the `view.dispatch({changes, selection})`) the
//!   caller applies.
//! - `{ ...DEFAULT_SQL_FORMAT_OPTIONS, ...options }` becomes
//!   [`SqlFormatOptionsVm::resolve`] (the defaults, overlaid).
//! - The selection branch re-applies the block's own leading indentation
//!   (`leadingIndentOf` — only when the selection starts its line) so a
//!   formatted block keeps sitting at the depth the user selected it
//!   at, and keeps the block highlighted (`anchor = from`, `head =
//!   from + len`) — the user's next action is usually to re-run it.
//! - The whole-document branch anchors the cursor to the statement it
//!   was in ([`map_cursor_by_statement`]): reformatting rewrites every
//!   offset, so a raw offset would drop the cursor in an unrelated
//!   statement and make the viewport jump; a clamped offset is the
//!   fallback when the statement count changes. `contentFrom` is the
//!   statement's first non-whitespace byte (the range the engine's
//!   `statements()` splits at semicolons, content-trimmed).
//! - `buildStatementRanges`/`findStatementAtCursor`
//!   (extension-points) are the statement-splitting primitives —
//!   carried here as the `sql_editor` model's semantics (`statements()`
//!   + the at-cursor rule), which those extension-points mirror.
//!
//! Known deltas: none behavioural — every rule below is the React
//! file's verbatim.

/// React `SqlFormatOptions` (the four configurable knobs;
/// `SettingsSqlFormatOptions` on the wire).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlFormatOptionsVm {
    /// `'upper' | 'lower' | 'preserve'`.
    pub keyword_case: KeywordCase,
    /// `'2spaces' | '4spaces' | 'tab'`.
    pub indent_style: IndentStyle,
    pub break_before_boolean_operators: bool,
    pub lines_between_queries: u32,
}

/// `keywordCase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeywordCase {
    Upper,
    Lower,
    Preserve,
}

/// `indentStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndentStyle {
    TwoSpaces,
    FourSpaces,
    Tab,
}

/// React `DEFAULT_SQL_FORMAT_OPTIONS` (`Object.freeze`).
pub const DEFAULT_SQL_FORMAT_OPTIONS: SqlFormatOptionsVm = SqlFormatOptionsVm {
    keyword_case: KeywordCase::Upper,
    indent_style: IndentStyle::TwoSpaces,
    break_before_boolean_operators: true,
    lines_between_queries: 1,
};

/// The `options?` partial overlay (React `{...defaults, ...options}`;
/// `None` keeps the default).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartialSqlFormatOptions {
    pub keyword_case: Option<KeywordCase>,
    pub indent_style: Option<IndentStyle>,
    pub break_before_boolean_operators: Option<bool>,
    pub lines_between_queries: Option<u32>,
}

impl SqlFormatOptionsVm {
    /// The defaults overlaid with the caller's partial.
    pub fn resolve(partial: PartialSqlFormatOptions) -> Self {
        Self {
            keyword_case: partial.keyword_case.unwrap_or(DEFAULT_SQL_FORMAT_OPTIONS.keyword_case),
            indent_style: partial.indent_style.unwrap_or(DEFAULT_SQL_FORMAT_OPTIONS.indent_style),
            break_before_boolean_operators: partial
                .break_before_boolean_operators
                .unwrap_or(DEFAULT_SQL_FORMAT_OPTIONS.break_before_boolean_operators),
            lines_between_queries: partial
                .lines_between_queries
                .unwrap_or(DEFAULT_SQL_FORMAT_OPTIONS.lines_between_queries),
        }
    }
}

/// The injected formatter seam (T615's `formatSql`): reject the input
/// and the buffer stays untouched.
pub type FormatSql<'a> =
    &'a dyn Fn(&str, Option<&str>, &SqlFormatOptionsVm) -> Result<String, FormatRejection>;

/// sql-formatter's throw, as a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatRejection;

/// The one `view.dispatch` the command decided on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatEdit {
    /// The replaced range.
    pub from: usize,
    pub to: usize,
    /// The inserted text.
    pub insert: String,
    /// `selection.anchor`.
    pub anchor: usize,
    /// `selection.head`.
    pub head: usize,
}

/// `formatEditorDocument`: format the selection when one exists,
/// otherwise the whole document. `None` when there is nothing to format
/// or the formatter rejects or changes nothing.
pub fn format_editor_document(
    doc: &str,
    sel_from: usize,
    sel_to: usize,
    cursor: usize,
    database_type: Option<&str>,
    options: PartialSqlFormatOptions,
    format_sql: FormatSql<'_>,
) -> Option<FormatEdit> {
    let resolved = SqlFormatOptionsVm::resolve(options);
    if sel_from == sel_to {
        format_whole_document(doc, cursor, database_type, &resolved, format_sql)
    } else {
        format_selection(doc, sel_from, sel_to, database_type, &resolved, format_sql)
    }
}

/// `formatSelection`.
fn format_selection(
    doc: &str,
    from: usize,
    to: usize,
    database_type: Option<&str>,
    options: &SqlFormatOptionsVm,
    format_sql: FormatSql<'_>,
) -> Option<FormatEdit> {
    let original = &doc[from.min(doc.len())..to.min(doc.len())];
    if original.trim().is_empty() {
        return None;
    }

    let formatted = safe_format(original, database_type, options, format_sql)?;
    if formatted == original {
        return None;
    }

    // Re-apply the selection's own leading indentation so a formatted
    // block keeps sitting at the depth the user selected it at.
    let indent = leading_indent_of(doc, from);
    let reindented = if indent.is_empty() { formatted } else { reindent(&formatted, &indent) };

    Some(FormatEdit {
        from,
        to,
        // Keep the block highlighted — the user's next action is usually
        // to re-run it.
        anchor: from,
        head: from + reindented.len(),
        insert: reindented,
    })
}

/// `formatWholeDocument`.
fn format_whole_document(
    doc: &str,
    cursor: usize,
    database_type: Option<&str>,
    options: &SqlFormatOptionsVm,
    format_sql: FormatSql<'_>,
) -> Option<FormatEdit> {
    if doc.trim().is_empty() {
        return None;
    }

    let formatted = safe_format(doc, database_type, options, format_sql)?;
    if formatted == original_eq(doc) {
        return None;
    }

    let anchor = map_cursor_by_statement(doc, &formatted, cursor);

    Some(FormatEdit {
        from: 0,
        to: doc.len(),
        insert: formatted.clone(),
        anchor,
        head: anchor,
    })
}

fn original_eq(doc: &str) -> &str {
    doc
}

/// `mapCursorByStatement`: reformatting rewrites every offset, so a raw
/// offset would drop the cursor in an unrelated statement. Anchor on
/// the statement the cursor was in, and fall back to a clamped offset
/// when the statement count changes.
fn map_cursor_by_statement(original: &str, formatted: &str, cursor: usize) -> usize {
    let Some(before) = statement_at_cursor(original, cursor) else {
        return cursor.min(formatted.len());
    };
    let Some(after) = statement_with_index(formatted, before.index) else {
        return cursor.min(formatted.len());
    };
    after.content_from.min(formatted.len())
}


/// One split statement: its ordinal and its content start (the
/// non-whitespace start — `SqlStatementRange.index`/`contentFrom`).
#[derive(Clone, Debug)]
struct StatementAnchor {
    index: usize,
    content_from: usize,
}

/// `buildStatementRanges`'s split (the engine `statements()` semantics,
/// carried): segments at semicolons, empty ones skipped; `contentFrom`
/// skips the segment's leading whitespace.
fn statement_ranges(doc: &str) -> Vec<StatementAnchor> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, byte) in doc.bytes().enumerate() {
        if byte == b';' {
            push_range(doc, start, i, &mut out);
            start = i + 1;
        }
    }
    push_range(doc, start, doc.len(), &mut out);
    out
}

fn push_range(doc: &str, start: usize, end: usize, out: &mut Vec<StatementAnchor>) {
    let segment = &doc[start..end];
    if segment.trim().is_empty() {
        return;
    }
    let content_from = start + (segment.len() - segment.trim_start().len());
    out.push(StatementAnchor { index: out.len(), content_from });
}

/// `findStatementAtCursor`: the statement the cursor is in — the first
/// ending at or after the cursor, the last one past the end.
fn statement_at_cursor(doc: &str, cursor: usize) -> Option<StatementAnchor> {
    let ranges = statement_ranges(doc);
    // Segment ends: each range's segment ends at its terminating
    // semicolon (or the document's end for the last).
    let ends = segment_ends(doc);
    let mut picked = None;
    for (range, end) in ranges.iter().zip(ends) {
        if cursor <= end {
            picked = Some(range.clone());
            break;
        }
    }
    picked.or_else(|| ranges.last().cloned())
}

fn segment_ends(doc: &str) -> Vec<usize> {
    let mut ends = Vec::new();
    for (i, byte) in doc.bytes().enumerate() {
        if byte == b';' {
            ends.push(i);
        }
    }
    ends.push(doc.len());
    ends
}

/// The statement with `index` in the formatted document (the same
/// ordinal — `find((r) => r.index === before.index)`).
fn statement_with_index(doc: &str, index: usize) -> Option<StatementAnchor> {
    statement_ranges(doc).into_iter().find(|range| range.index == index)
}

/// `safeFormat`: a rejection reads as `None` (the buffer stays
/// untouched).
fn safe_format(
    sql: &str,
    database_type: Option<&str>,
    options: &SqlFormatOptionsVm,
    format_sql: FormatSql<'_>,
) -> Option<String> {
    format_sql(sql, database_type, options).ok()
}

/// `leadingIndentOf`: the line's start-to-selection prefix, but only
/// when it is all whitespace — a mid-line selection owns no indent.
fn leading_indent_of(doc: &str, from: usize) -> String {
    let line_start = doc[..from.min(doc.len())].rfind('\n').map_or(0, |i| i + 1);
    let prefix = &doc[line_start..from.min(doc.len())];
    if !prefix.is_empty() && prefix.chars().all(|c| c == ' ' || c == '\t') {
        prefix.to_owned()
    } else {
        String::new()
    }
}

/// `reindent`: the first line and empty lines keep their depth; every
/// other line gains the indent.
fn reindent(text: &str, indent: &str) -> String {
    text.split('\n')
        .enumerate()
        .map(|(i, line)| if i == 0 || line.is_empty() { line.to_owned() } else { format!("{indent}{line}") })
        .collect::<Vec<_>>()
        .join("\n")
}
