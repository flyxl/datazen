//! Completion-context helpers carried for the
//! `snippetCompletionSource.ts` port (T359).
//!
//! Source of truth: `packages/frontend/src/lib/sqlCompletionContext.ts`
//! (T607 — another segment's ledger entry) and the
//! `@datazen/extension-points` `sql-editor/completionContext.ts`
//! helper `isTableContext` (the `extensions` family, segment 2). This
//! module carries the functions the snippet source needs until those
//! tasks land and absorb them — the `lib_helpers` /
//! `grid_clipboard_helpers` precedent. Only what the snippet source
//! reads is carried:
//!
//! * `SqlCompletionKind` / `infer_sql_completion_kind` (the three
//!   context kinds and the keyword classifier),
//! * `last_bare_sql_keyword_with_offset` (the string/comment-skipping
//!   backward keyword scan),
//! * `in_insert_column_list` (the `INSERT INTO t (` open-paren probe),
//! * `is_table_context` (the table-clause tail probe).
//!
//! Not carried (no caller in this port): `lastBareSqlKeyword` (the
//! offset-less face), `detectAliasDotContext` and the scope
//! machinery (the semantic model's own half, already ported as
//! `completion/semantic_model`), `extractQualifierParts` (the
//! qualifier parser — the snippet source only needs the dot
//! exclusion, which it implements itself).
//!
//! Known deltas (behaviour-preserving unless noted):
//!
//! - JS regexes walk UTF-16 code units; these scanners walk bytes
//!   over ASCII structural characters. Every structural character
//!   (`.`, `,`, `(`, quotes, `$`, `*`, `\w`) is ASCII, so multi-byte
//!   content simply falls out of the character classes — the JS
//!   loops' own invariant.
//! - JS `\s` is a fixed Unicode set; the byte scanners accept the
//!   ASCII whitespace members (` \t\n\r\x0b\x0c`). The pinned suites
//!   exercise ASCII SQL only.

/// `SqlCompletionKind`: the cursor context a completion source ranks
/// itself against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SqlCompletionKind {
    Column,
    Table,
    Any,
}

/// The keywords whose tail is a table position.
const TABLE_KWS: &[&str] = &["from", "join", "into", "update", "table", "using", "truncate"];

/// The keywords whose tail is a column position.
const COLUMN_KWS: &[&str] = &[
    "select", "where", "having", "set", "on", "and", "or", "not", "between", "like", "ilike",
    "in", "when", "returning", "group", "order", "by", "distinct", "values", "limit", "offset",
];

fn is_tracked_keyword(word: &str) -> bool {
    TABLE_KWS.contains(&word) || COLUMN_KWS.contains(&word)
}

/// A JS `\w` character (`[A-Za-z0-9_]`).
fn is_js_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The identifier-class characters SQL words may span on the wire
/// (`[\w$]`).
fn is_word_or_dollar(b: u8) -> bool {
    is_js_word(b) || b == b'$'
}

/// The last SQL keyword before the cursor, with its end offset,
/// ignoring `'…'` strings (with `''` escapes), `--` line comments and
/// `/* … */` block comments — the React scanner verbatim.
pub(crate) fn last_bare_sql_keyword_with_offset(sql: &str) -> Option<(&'static str, usize)> {
    let b = sql.as_bytes();
    let n = b.len();
    let mut last_kw: Option<&'static str> = None;
    let mut last_end = 0usize;
    let mut i = 0usize;
    while i < n {
        let ch = b[i];
        if ch == b'\'' {
            i += 1;
            while i < n {
                if b[i] == b'\'' && i + 1 < n && b[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                if b[i] == b'\'' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if ch == b'-' && i + 1 < n && b[i + 1] == b'-' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if ch == b'/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i < n && !(b[i] == b'*' && i + 1 < n && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == b'_' {
            let start = i;
            let mut j = i + 1;
            while j < n && is_word_or_dollar(b[j]) {
                j += 1;
            }
            let word = &sql[start..j];
            if is_tracked_keyword(&word.to_ascii_lowercase()) {
                last_kw = Some(match word.to_ascii_lowercase().as_str() {
                    "from" => "from",
                    "join" => "join",
                    "into" => "into",
                    "update" => "update",
                    "table" => "table",
                    "using" => "using",
                    "truncate" => "truncate",
                    "select" => "select",
                    "where" => "where",
                    "having" => "having",
                    "set" => "set",
                    "on" => "on",
                    "and" => "and",
                    "or" => "or",
                    "not" => "not",
                    "between" => "between",
                    "like" => "like",
                    "ilike" => "ilike",
                    "in" => "in",
                    "when" => "when",
                    "returning" => "returning",
                    "group" => "group",
                    "order" => "order",
                    "by" => "by",
                    "distinct" => "distinct",
                    "values" => "values",
                    "limit" => "limit",
                    "offset" => "offset",
                    _ => unreachable!("tracked keyword spelling"),
                });
                last_end = j;
            }
            i = j;
            continue;
        }
        i += 1;
    }
    last_kw.map(|kw| (kw, last_end))
}

/// `inInsertColumnList(stmt)`: an `INSERT INTO t (` column list — the
/// statement ends inside an unclosed `(` whose content is free of a
/// closing paren and of dangling quotes, with the word `into`
/// (case-insensitive) somewhere before it.
pub(crate) fn in_insert_column_list(stmt: &str) -> bool {
    // The positions of `(` whose tail can satisfy
    // `(?:[^)'"`]|'[^']*'|"[^"]*"|`[^`]*`)*$`.
    let b = stmt.as_bytes();
    let n = b.len();
    let valid_open = |p: usize| -> bool {
        let mut i = p + 1;
        while i < n {
            match b[i] {
                b')' => return false,
                b'\'' | b'"' | b'`' => {
                    let quote = b[i];
                    let mut j = i + 1;
                    while j < n && b[j] != quote {
                        j += 1;
                    }
                    if j == n {
                        // An unterminated quote: the class alternatives
                        // cannot consume it — the tail does not match.
                        return false;
                    }
                    i = j + 1;
                }
                _ => i += 1,
            }
        }
        true
    };
    // An `into` word before the first valid open paren suffices (the
    // regex's `[\s\S]*` spans anything between).
    for p in (0..n).filter(|&p| b[p] == b'(' && valid_open(p)) {
        if has_word_before(stmt, p, "into") {
            return true;
        }
    }
    false
}

/// Whether `word` (lowercase) appears before byte offset `end` as a
/// `\b`-delimited word, case-insensitively.
fn has_word_before(text: &str, end: usize, word: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0usize;
    while i < end {
        if is_js_word(b[i]) {
            let start = i;
            while i < end && is_js_word(b[i]) {
                i += 1;
            }
            if text[start..i].eq_ignore_ascii_case(word) {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// The identifier-run scanner behind `isTableContext`: the
/// `["`\[][^"`\]]+["`\]]|\b[A-Za-z0-9_$]+\b` (with `\s*.\s*`
/// continuations) matches of `segment`, leftmost and non-overlapping.
fn scan_identifier_runs(segment: &str) -> Vec<(usize, usize)> {
    let b = segment.as_bytes();
    let n = b.len();
    let mut out: Vec<(usize, usize)> = Vec::new();

    fn is_quote_open(c: u8) -> bool {
        c == b'"' || c == b'`' || c == b'['
    }
    fn is_quote_close(c: u8) -> bool {
        c == b'"' || c == b'`' || c == b']'
    }
    fn is_inner(c: u8) -> bool {
        // `[^"`\]]`
        !(c == b'"' || c == b'`' || c == b']')
    }
    fn is_class(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
    }
    fn is_space(c: u8) -> bool {
        matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
    }

    /// One identifier at `i` (quoted or bare with `\b`), or None.
    fn ident_at(b: &[u8], n: usize, i: usize) -> Option<usize> {
        let c = *b.get(i)?;
        if is_quote_open(c) {
            let mut j = i + 1;
            while j < n && is_inner(b[j]) {
                j += 1;
            }
            if j > i + 1 && j < n && is_quote_close(b[j]) {
                return Some(j + 1);
            }
            return None;
        }
        if is_class(c) && (i == 0 || !is_js_word(b[i - 1])) {
            let mut j = i + 1;
            while j < n && is_class(b[j]) {
                j += 1;
            }
            // The closing `\b`: trim trailing non-word class chars
            // (only `$` can be one).
            while j > i + 1 && !is_js_word(b[j - 1]) {
                j -= 1;
            }
            if j > i + 1 || (j == i + 1 && is_js_word(b[i])) {
                return Some(j);
            }
        }
        None
    }

    let mut i = 0usize;
    while i < n {
        if let Some(mut end) = ident_at(b, n, i) {
            loop {
                let mut j = end;
                while j < n && is_space(b[j]) {
                    j += 1;
                }
                if j < n && b[j] == b'.' {
                    let mut k = j + 1;
                    while k < n && is_space(b[k]) {
                        k += 1;
                    }
                    if let Some(id_end) = ident_at(b, n, k) {
                        end = id_end;
                        continue;
                    }
                }
                break;
            }
            out.push((i, end));
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// `isTableContext(tail)` (extension-points `completionContext.ts`):
/// whether the text tail following a table-clause keyword is still in
/// the process of typing a table name, vs. already completed it.
pub(crate) fn is_table_context(tail: &str) -> bool {
    let comma_idx = tail.rfind(',');
    let segment = match comma_idx {
        Some(i) => &tail[i + 1..],
        None => tail,
    };
    let trimmed = segment.trim_start();
    if trimmed.is_empty() {
        return true;
    }
    let matches = scan_identifier_runs(segment);
    if matches.is_empty() {
        return true;
    }
    if matches.len() == 1 {
        let after_first = &segment[matches[0].1..];
        // `/^\s+/`: any leading whitespace means the table name is done.
        return !after_first.starts_with(|c: char| c.is_ascii_whitespace());
    }
    false
}

/// `inferSqlCompletionKind(textBeforeCursor)`: `FROM`/`JOIN` →
/// tables; `WHERE`/`SELECT`/`HAVING` → columns; after `ident.` the
/// schema path already picked the level — do not filter.
pub(crate) fn infer_sql_completion_kind(text_before_cursor: &str) -> SqlCompletionKind {
    // `/\.\s*(?:["`][\w$]*|[\w$]*)$/` — a qualifier dot plus a
    // partial (possibly quoted, possibly empty) identifier.
    if dot_partial_identifier(text_before_cursor) {
        return SqlCompletionKind::Any;
    }
    let stmt_start = text_before_cursor.rfind(';').map(|i| i + 1).unwrap_or(0);
    let stmt = &text_before_cursor[stmt_start..];
    if in_insert_column_list(stmt) {
        return SqlCompletionKind::Column;
    }
    let Some((kw, end)) = last_bare_sql_keyword_with_offset(stmt) else {
        return SqlCompletionKind::Any;
    };
    if TABLE_KWS.contains(&kw) {
        let tail = &stmt[end..];
        return if is_table_context(tail) {
            SqlCompletionKind::Table
        } else {
            SqlCompletionKind::Any
        };
    }
    if COLUMN_KWS.contains(&kw) {
        return SqlCompletionKind::Column;
    }
    SqlCompletionKind::Any
}

/// The `/\.\s*(?:["`][\w$]*|[\w$]*)$/` probe: the text ends with a
/// dot, optional whitespace, and either nothing, bare `[\w$]*`, or an
/// opening quote plus `[\w$]*`.
fn dot_partial_identifier(text: &str) -> bool {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = n;
    // Trailing `[\w$]*`.
    while i > 0 && is_word_or_dollar(b[i - 1]) {
        i -= 1;
    }
    // The quoted alternative: the whole tail from the quote is one
    // opening quote plus `[\w$]*`.
    if i > 0 && (b[i - 1] == b'"' || b[i - 1] == b'`') {
        i -= 1;
    }
    // `\s*`.
    while i > 0 && matches!(b[i - 1], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i -= 1;
    }
    i > 0 && b[i - 1] == b'.'
}
