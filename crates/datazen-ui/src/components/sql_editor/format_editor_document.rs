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
//!   fallback when the statement count changes. The formatting-only
//!   anchor projection follows the React statement scanner/ranges; it
//!   is not the Native driver's query-execution splitter.

use std::borrow::Cow;

use datazen_ui_api::native_sql_format::trim_like_react;

mod statement_anchors;
use self::statement_anchors::map_cursor_by_statement;

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
    let original = if sel_from == sel_to { doc } else { doc.get(sel_from..sel_to)? };
    if trim_like_react(original).is_empty() {
        return None;
    }
    let resolved = SqlFormatOptionsVm::resolve(options);
    let formatted = format_sql(original, database_type, &resolved).ok()?;
    format_editor_output(doc, sel_from, sel_to, cursor, formatted, usize::MAX)
}

pub(crate) fn project_lf(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// Same edit calculation for a synchronous caller and a completed
/// asynchronous job. The worker does not get caret, indent, or editor state.
pub(crate) fn format_editor_output(
    doc: &str, from: usize, to: usize, cursor: usize, formatted: String, limit: usize,
) -> Option<FormatEdit> {
    if formatted.len() > limit {
        return None;
    }
    if from == to {
        let original = project_lf(doc);
        if formatted == original.as_ref() { return None; }
        let cursor = doc.floor_char_boundary(cursor.min(doc.len()));
        let cursor = project_lf(&doc[..cursor]).len();
        let anchor = map_cursor_by_statement(&original, &formatted, cursor);
        return Some(FormatEdit { from: 0, to: doc.len(), insert: formatted, anchor, head: anchor });
    }

    let original = project_lf(doc.get(from..to)?);
    if formatted == original.as_ref() { return None; }
    let indent = leading_indent_of(doc, from);
    let insert = if indent.is_empty() {
        formatted
    } else {
        reindent_checked(&formatted, indent, limit)?
    };
    if insert == original.as_ref() { return None; }
    Some(FormatEdit { from, to, anchor: from, head: from.checked_add(insert.len())?, insert })
}

/// `leadingIndentOf`: the line's start-to-selection prefix, but only
/// when it is all whitespace — a mid-line selection owns no indent.
fn leading_indent_of(doc: &str, from: usize) -> &str {
    let line_start = doc[..from.min(doc.len())].rfind(['\r', '\n']).map_or(0, |i| i + 1);
    let prefix = &doc[line_start..from.min(doc.len())];
    if !prefix.is_empty() && prefix.chars().all(|c| c == ' ' || c == '\t') {
        prefix
    } else {
        ""
    }
}

/// `reindent`: the first line and empty lines keep their depth; every
/// other line gains the indent.
fn reindent_checked(text: &str, indent: &str, limit: usize) -> Option<String> {
    // Raw job output alone is not the upper bound: restoring an imported
    // document's long indentation could amplify a multiline result.
    let nested = text.split('\n').skip(1).filter(|line| !line.is_empty()).count();
    let bytes = nested.checked_mul(indent.len())?.checked_add(text.len())?;
    if bytes > limit { return None; }
    let mut output = String::with_capacity(bytes);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            output.push('\n');
            if !line.is_empty() { output.push_str(indent); }
        }
        output.push_str(line);
    }
    Some(output)
}
