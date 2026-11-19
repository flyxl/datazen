//! Extract the schema objects removed by DDL executed in the query editor —
//! ported from React
//! `packages/frontend/src/lib/sqlSchemaDropTargets.ts` (tasklist T619,
//! 90 lines). A T179 `lib/aiMemoryCorrection.ts` dependency:
//! `markStaleForSchemaDrops` feeds these targets field by field to the
//! host's memory-mark-stale command.
//!
//! This is deliberately a small lexical scanner rather than a full SQL
//! parser: it only needs the table/column identifiers that T14 marks
//! stale, and it must never invent a target when the statement is
//! ambiguous.
//!
//! ## The regex mirror (this port's only substitution decision)
//!
//! React compiles three `RegExp`s over one identifier grammar and scans
//! the noise-stripped statement; the crate's dependency budget has no
//! regex engine (and none may be added), so the same grammar is
//! hand-rolled as a deterministic scanner over `Vec<char>`:
//!
//! - **IDENTIFIER** — `"(?:[^"]|"")*" | ``(?:[^`]|``)*`` |
//!   `\[[^\]]+\]` | `[A-Za-z_][A-Za-z0-9_$]*`. The opening character
//!   decides the alternative, so at most one can match at a position and
//!   no cross-alternative backtracking exists. The quoted forms close at
//!   the first *singleton* quote (one not followed by another), or — when
//!   the text ends inside the literal — at the first quote of the last
//!   consumed doubled-quote pair, which is the only productive backtrack
//!   of `Q(?:[^Q]|QQ)*Q` ([`match_quoted`]).
//! - **QUALIFIED** = IDENTIFIER(`\s*\.\s*` IDENTIFIER)`{0,2}` is maximal
//!   munch on the part count; each `\s*\.\s*` hop is deterministic.
//! - `\s` stands in as [`char::is_whitespace`] (the JS and Rust Unicode
//!   whitespace vocabularies differ only on exotic code points — U+0085
//!   is not a JS `\s`, U+FEFF is not a Rust whitespace); `\b` word
//!   characters are ASCII `[A-Za-z0-9_]` exactly as in JS — `$` is not
//!   one.
//! - `matchAll` semantics: positions are tried left to right and scanning
//!   resumes at each match's end. Backtracking is observable in exactly
//!   two places, both reproduced: the DROP TABLE capture `([^;]+`)
//!   **accepts whitespace**, so the optional `IF\s+EXISTS\s+` group's
//!   trailing whitespace run unwinds k = n..1 before the group is
//!   abandoned ([`try_drop_table_at`]); in the ALTER tail every optional
//!   is all-or-nothing (a keyword or identifier tail cannot match
//!   whitespace, so an optional's internal `\s+` never unwinds) and the
//!   QUALIFIED part count unwinds 3 → 2 → 1 before the leading
//!   `IF EXISTS`/`ONLY` optionals give up — most recent choice point
//!   first, the regex engine's order ([`try_alter_at`]).
//! - The noise passes run in React's replace order — line comments, then
//!   block comments, then `'…'` literals — because each pass feeds the
//!   next (a `--` inside a string literal is stripped by the earlier pass
//!   either way).
//! - No serde: the output is an in-process list the staleness loop reads
//!   field by field, never a stored/IPC shape.
//!
//! Faithful quirks (traced through the regex engine, kept because the
//! contract is the React behavior): a DROP TABLE whose body reduces to
//! `IF EXISTS` yields `{table: "IF"}` — the backtracked capture starts
//! where the abandoned group did; an `ALTER TABLE a.b.c.d …` (four parts)
//! matches nothing because QUALIFIED caps at three parts with nothing
//! behind to backtrack into; a three-part (catalog.schema.table) name
//! keeps only the last two parts.

use std::collections::HashSet;

/// React's `SqlSchemaDropTarget`: the schema object a statement removed.
/// Fields are optional exactly like the `?:` interface — `column` is only
/// ever set by the ALTER path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlSchemaDropTarget {
    pub schema: Option<String>,
    pub table: Option<String>,
    pub column: Option<String>,
}

// --- shared character vocabulary ----------------------------------------

/// JS `\s` — stands in as [`char::is_whitespace`] (module docs).
fn is_js_space(ch: char) -> bool {
    ch.is_whitespace()
}

/// JS `\b` word character: ASCII `[A-Za-z0-9_]` (`$` is not one).
fn is_word(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// Case-insensitive ASCII literal match of `word` at `at` — JS `/i` over
/// these all-ASCII keywords.
fn eq_ci(chars: &[char], at: usize, word: &str) -> bool {
    word.chars().enumerate().all(|(k, letter)| {
        chars.get(at + k).is_some_and(|ch| ch.eq_ignore_ascii_case(&letter))
    })
}

/// `\b` before a keyword at `at` — no word character to the left (the
/// keyword's first letter is the word character on the right).
fn has_word_boundary(chars: &[char], at: usize) -> bool {
    at == 0 || !is_word(chars[at - 1])
}

/// `\b` after a keyword ending at `end` — no word character on the right.
fn has_trailing_boundary(chars: &[char], end: usize) -> bool {
    end >= chars.len() || !is_word(chars[end])
}

/// Greedy `\s*` skip from `i`.
fn skip_ws(chars: &[char], i: usize) -> usize {
    let mut i = i;
    while i < chars.len() && is_js_space(chars[i]) {
        i += 1;
    }
    i
}

/// Greedy `\s+` — `None` when there is no whitespace at all.
fn skip_ws_at_least_one(chars: &[char], i: usize) -> Option<usize> {
    let after = skip_ws(chars, i);
    (after > i).then_some(after)
}

/// The length of the whitespace run starting at `i`.
fn ws_run_len(chars: &[char], i: usize) -> usize {
    skip_ws(chars, i) - i
}

// --- the identifier grammar ----------------------------------------------

/// `Q(?:[^Q]|QQ)*Q` anchored at `at` (`Q = quote`): closes at the first
/// singleton quote, or — when the text ends inside the literal — at the
/// first quote of the last consumed `QQ` pair, the engine's only productive
/// backtrack (module docs). Returns the end index (exclusive).
fn match_quoted(chars: &[char], at: usize, quote: char) -> Option<usize> {
    if chars.get(at) != Some(&quote) {
        return None;
    }
    let mut i = at + 1;
    let mut last_pair = None;
    while let Some(&ch) = chars.get(i) {
        if ch == quote {
            if chars.get(i + 1) == Some(&quote) {
                last_pair = Some(i);
                i += 2;
            } else {
                return Some(i + 1);
            }
        } else {
            i += 1;
        }
    }
    last_pair.map(|pair| pair + 1)
}

/// Bare `[A-Za-z_][A-Za-z0-9_$]*` — maximal munch (nothing after it can
/// force it to give characters back).
fn bare_ident_end(chars: &[char], at: usize) -> usize {
    let mut end = at + 1;
    while end < chars.len() && is_bare_ident_char(chars[end]) {
        end += 1;
    }
    end
}

fn is_bare_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_bare_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

/// `\[[^\]]+\]` — from `[` to the first `]`, at least one inner character.
fn match_bracketed(chars: &[char], at: usize) -> Option<usize> {
    let inner_start = at + 1;
    let mut i = inner_start;
    while i < chars.len() && chars[i] != ']' {
        i += 1;
    }
    (i > inner_start && i < chars.len()).then_some(i + 1)
}

/// IDENTIFIER at `at`, in React's alternative order. The opening character
/// decides the alternative (`"`, `` ` ``, `[`, letter or `_`), so no
/// cross-alternative backtracking exists. Returns the end (exclusive).
fn match_identifier(chars: &[char], at: usize) -> Option<usize> {
    match chars.get(at) {
        Some('"') => match_quoted(chars, at, '"'),
        Some('`') => match_quoted(chars, at, '`'),
        Some('[') => match_bracketed(chars, at),
        Some(&ch) if is_bare_ident_start(ch) => Some(bare_ident_end(chars, at)),
        _ => None,
    }
}

/// QUALIFIED at `at` with at most `limit` extra dotted parts (the `{0,2}`
/// quantifier): IDENTIFIER, then up to `limit` × `\s*\.\s*` IDENTIFIER.
/// The parts are prefix-monotonic, so re-matching with a smaller `limit`
/// is exactly the engine's part-count backtracking.
fn match_qualified_at_most(chars: &[char], at: usize, limit: usize) -> Option<usize> {
    let mut end = match_identifier(chars, at)?;
    let mut taken = 0;
    while taken < limit {
        let dot = skip_ws(chars, end);
        if chars.get(dot) != Some(&'.') {
            break;
        }
        let after_dot = skip_ws(chars, dot + 1);
        match match_identifier(chars, after_dot) {
            Some(next_end) => {
                end = next_end;
                taken += 1;
            }
            // The iteration fails; the quantifier keeps the count so far.
            None => break,
        }
    }
    Some(end)
}

/// `(?:IF\s+EXISTS\s+)?` probed at `at`: IF, whitespace, EXISTS, then the
/// whitespace run its own trailing `\s+` greedily consumes. Returns where
/// `EXISTS` ends and how long that run is.
fn try_if_exists_prefix(chars: &[char], at: usize) -> Option<(usize, usize)> {
    if !eq_ci(chars, at, "IF") {
        return None;
    }
    let after_if = skip_ws_at_least_one(chars, at + 2)?;
    if !eq_ci(chars, after_if, "EXISTS") {
        return None;
    }
    let run_start = after_if + 6;
    Some((run_start, ws_run_len(chars, run_start)))
}

/// `(?:IF\s+EXISTS\s+)?` where the tail cannot match whitespace (every
/// tail after it here is a keyword or identifier): all-or-nothing, greedy.
/// Returns the position after the group.
fn try_if_exists_group(chars: &[char], at: usize) -> Option<usize> {
    let (run_start, run) = try_if_exists_prefix(chars, at)?;
    (run >= 1).then_some(run_start + run)
}

/// `(?:ONLY\s+)?` / `(?:COLUMN\s+)?` — keyword plus mandatory whitespace,
/// all-or-nothing for the same reason as [`try_if_exists_group`].
fn try_word_group(chars: &[char], at: usize, word: &str) -> Option<usize> {
    if !eq_ci(chars, at, word) {
        return None;
    }
    skip_ws_at_least_one(chars, at + word.len())
}

// --- the noise passes (React's replace order) ---------------------------

/// `--[^\n]*` → `' '` (global): line comments, each replaced by one space
/// (the newline itself survives).
fn strip_line_comments(sql: &str) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' && chars.get(i + 1) == Some(&'-') {
            out.push(' ');
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `/\*[\s\S]*?\*\//` → `' '` (global, non-greedy): each block comment is
/// the span from `/*` to the NEAREST following `*/`, one space each; an
/// unclosed `/*` is ordinary text.
fn strip_block_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            let mut j = i + 2;
            while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '/') {
                j += 1;
            }
            if j + 1 < chars.len() {
                out.push(' ');
                i = j + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `'(?:''|[^'])*'` → `' '` (global): single-quoted literals, `''`
/// escaping, one space each.
fn strip_string_literals(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            if let Some(end) = match_quoted(&chars, i, '\'') {
                out.push(' ');
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

// --- the two statement scanners ------------------------------------------

/// `([^;]+)` — at least one non-semicolon character, greedy to the next
/// `;` or the end of the text. Returns the end (exclusive).
fn capture_to_semicolon(chars: &[char], start: usize) -> Option<usize> {
    if start >= chars.len() || chars[start] == ';' {
        return None;
    }
    let mut end = start + 1;
    while end < chars.len() && chars[end] != ';' {
        end += 1;
    }
    Some(end)
}

/// `\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?([^;]+)` at `p` (`/gi`): the
/// capture span when the statement matches.
///
/// The optional group's trailing whitespace run unwinds k = n..1 *before*
/// the group is abandoned, because the capture accepts whitespace: with
/// `;` directly after a two-space run the k = n-1 capture starts inside it
/// (body `" "`), and only when the run cannot shrink (n = 1, or the group
/// never matched) does the engine fall back to group-absent, whose capture
/// starts at `IF EXISTS`. Module docs.
fn try_drop_table_at(chars: &[char], p: usize) -> Option<(usize, usize)> {
    if !has_word_boundary(chars, p) || !eq_ci(chars, p, "DROP") {
        return None;
    }
    let after_drop = skip_ws_at_least_one(chars, p + 4)?;
    if !eq_ci(chars, after_drop, "TABLE") {
        return None;
    }
    let after_table = skip_ws_at_least_one(chars, after_drop + 5)?;
    if let Some((run_start, run)) = try_if_exists_prefix(chars, after_table) {
        if run >= 1 {
            for k in (1..=run).rev() {
                let capture_start = run_start + k;
                if let Some(end) = capture_to_semicolon(chars, capture_start) {
                    return Some((capture_start, end));
                }
            }
        }
    }
    // Group absent (or exhausted): the capture starts right after TABLE's
    // whitespace.
    let end = capture_to_semicolon(chars, after_table)?;
    Some((after_table, end))
}

/// All `matchAll` capture spans of
/// `\bDROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?([^;]+)` (`/gi`); scanning resumes
/// at each match's end.
fn drop_table_bodies(chars: &[char]) -> Vec<(usize, usize)> {
    let mut bodies = Vec::new();
    let mut p = 0;
    while p < chars.len() {
        match try_drop_table_at(chars, p) {
            Some(span) => {
                bodies.push(span);
                p = span.1;
            }
            None => p += 1,
        }
    }
    bodies
}

/// `\bALTER\s+TABLE\s+(?:IF\s+EXISTS\s+)?(?:ONLY\s+)?(QUALIFIED)\s+DROP\s+
/// (?:COLUMN\s+)?(?:IF\s+EXISTS\s+)?(IDENTIFIER)` at `p` (`/gi`): the
/// table QUALIFIED span and the column IDENTIFIER span.
///
/// The retry order is the regex engine's — most recent choice point first:
/// the trailing `COLUMN`/`IF EXISTS` optionals vary fastest (neither can
/// match whitespace, so each is all-or-nothing), then the QUALIFIED part
/// count 3 → 2 → 1, then the leading `IF EXISTS`/`ONLY` optionals. Module
/// docs.
fn try_alter_at(chars: &[char], p: usize) -> Option<(usize, usize, usize, usize)> {
    if !has_word_boundary(chars, p) || !eq_ci(chars, p, "ALTER") {
        return None;
    }
    let after_alter = skip_ws_at_least_one(chars, p + 5)?;
    if !eq_ci(chars, after_alter, "TABLE") {
        return None;
    }
    let after_table = skip_ws_at_least_one(chars, after_alter + 5)?;
    for leading_if_exists in [true, false] {
        let after_if_exists = match try_if_exists_group(chars, after_table) {
            Some(at) if leading_if_exists => at,
            _ if !leading_if_exists => after_table,
            // The group did not match; only its absence works here.
            _ => continue,
        };
        for only in [true, false] {
            let after_only = match try_word_group(chars, after_if_exists, "ONLY") {
                Some(at) if only => at,
                _ if !only => after_if_exists,
                _ => continue,
            };
            for extra_parts in (0..=2).rev() {
                let Some(qual_end) = match_qualified_at_most(chars, after_only, extra_parts)
                else {
                    // The first identifier failed — fewer parts cannot help.
                    break;
                };
                let Some(after_qual) = skip_ws_at_least_one(chars, qual_end) else {
                    continue;
                };
                if !eq_ci(chars, after_qual, "DROP") {
                    continue;
                }
                let Some(after_drop) = skip_ws_at_least_one(chars, after_qual + 4) else {
                    continue;
                };
                // `(?:COLUMN\s+)?(?:IF\s+EXISTS\s+)?(IDENTIFIER)` — the
                // trailing optionals vary first.
                for column in [true, false] {
                    let after_column = match try_word_group(chars, after_drop, "COLUMN") {
                        Some(at) if column => at,
                        _ if !column => after_drop,
                        _ => continue,
                    };
                    for trailing_if_exists in [true, false] {
                        let after_trailing = match try_if_exists_group(chars, after_column) {
                            Some(at) if trailing_if_exists => at,
                            _ if !trailing_if_exists => after_column,
                            _ => continue,
                        };
                        if let Some(column_end) = match_identifier(chars, after_trailing) {
                            return Some((
                                after_only,
                                qual_end,
                                after_trailing,
                                column_end,
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

/// All `matchAll` hits of the ALTER DROP COLUMN regex (`/gi`): the table
/// QUALIFIED span and the column IDENTIFIER span, scanning left to right
/// and resuming at each match's end.
fn alter_drop_columns(chars: &[char]) -> Vec<(usize, usize, usize, usize)> {
    let mut hits = Vec::new();
    let mut p = 0;
    while p < chars.len() {
        match try_alter_at(chars, p) {
            Some(hit) => {
                hits.push(hit);
                p = hit.3;
            }
            None => p += 1,
        }
    }
    hits
}

// --- the target vocabulary -----------------------------------------------

/// The slice between the first and last character: empty for
/// one-character values (a lone quote unquotes to `""` — React's
/// startsWith/endsWith agree on it, and its `slice(1, -1)` is empty).
fn inner_range(chars: &[char]) -> std::ops::Range<usize> {
    let len = chars.len();
    if len <= 1 {
        0..0
    } else {
        1..len - 1
    }
}

/// JS `replace(/QQ/g, 'Q')` — non-overlapping pairs, left to right.
fn collapse_quote_pairs(inner: &[char], quote: char) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == quote && inner.get(i + 1) == Some(&quote) {
            out.push(quote);
            i += 2;
        } else {
            out.push(inner[i]);
            i += 1;
        }
    }
    out
}

/// React's `unquote`: strip `"…"`/`` `…` ``/`[…]` quoting, collapsing the
/// doubled-quote escapes inside the first two; anything else returns the
/// trimmed value.
fn unquote(identifier: &str) -> String {
    let value = identifier.trim();
    let chars: Vec<char> = value.chars().collect();
    if chars.first() == Some(&'"') && chars.last() == Some(&'"') {
        return collapse_quote_pairs(&chars[inner_range(&chars)], '"');
    }
    if chars.first() == Some(&'`') && chars.last() == Some(&'`') {
        return collapse_quote_pairs(&chars[inner_range(&chars)], '`');
    }
    if chars.first() == Some(&'[') && chars.last() == Some(&']') {
        return chars[inner_range(&chars)].iter().copied().collect();
    }
    value.to_owned()
}

/// React's `targetForQualified`: split on `.` (naively — a quoted part that
/// contains a dot splits too, the React behavior), unquote each part, drop
/// the empty ones. One part is the table; two or more take the last two as
/// schema and table (a catalog part in front is dropped).
fn target_for_qualified(qualified: &str) -> SqlSchemaDropTarget {
    let mut parts: Vec<String> = qualified
        .split('.')
        .map(unquote)
        .filter(|part| !part.is_empty())
        .collect();
    let table = parts.pop();
    let schema = parts.pop();
    SqlSchemaDropTarget { schema, table, column: None }
}

/// React's `push` closure: skip targetless rows, dedupe on the lowercased
/// `schema.table.column` key, then append (the first occurrence, original
/// casing).
fn push_target(
    targets: &mut Vec<SqlSchemaDropTarget>,
    seen: &mut HashSet<String>,
    target: SqlSchemaDropTarget,
) {
    let Some(table) = target.table.as_deref().filter(|table| !table.is_empty()) else {
        return;
    };
    let key = format!(
        "{}.{}.{}",
        target.schema.as_deref().unwrap_or(""),
        table,
        target.column.as_deref().unwrap_or("")
    )
    .to_lowercase();
    if seen.insert(key) {
        targets.push(target);
    }
}

/// The `match[1].split(/\bCASCADE\b|\bRESTRICT\b/i)[0]` cut: the span
/// before the first whole-word CASCADE/RESTRICT. Returns the cut index.
fn cut_before_cascade(chars: &[char]) -> usize {
    for i in 0..chars.len() {
        for word in ["CASCADE", "RESTRICT"] {
            if has_word_boundary(chars, i)
                && eq_ci(chars, i, word)
                && has_trailing_boundary(chars, i + word.len())
            {
                return i;
            }
        }
    }
    chars.len()
}

/// The leftmost start of a `\b(?:CASCADE|RESTRICT)\s*$` tail — React's
/// non-global replace removes one such word. Returns the cut index.
fn trailing_cascade_cut(chars: &[char]) -> usize {
    for i in 0..chars.len() {
        for word in ["CASCADE", "RESTRICT"] {
            let end = i + word.len();
            if has_word_boundary(chars, i)
                && eq_ci(chars, i, word)
                && chars.get(end..).is_some_and(|tail| tail.iter().all(|ch| is_js_space(*ch)))
            {
                return i;
            }
        }
    }
    chars.len()
}

/// React's `stripLeadingNoise`: drop a trailing
/// `\b(?:CASCADE|RESTRICT)\s*$`, collapse whitespace runs to single
/// spaces, then trim.
fn strip_leading_noise(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let cut = trailing_cascade_cut(&chars);
    let mut collapsed = String::new();
    let mut i = 0;
    while i < cut {
        if is_js_space(chars[i]) {
            collapsed.push(' ');
            i = skip_ws(&chars, i);
        } else {
            collapsed.push(chars[i]);
            i += 1;
        }
    }
    collapsed.trim().to_owned()
}

/// `cleaned.match(QUALIFIED_PART_RE)?.[0]` — the leftmost QUALIFIED in the
/// cleaned part, maximal munch.
fn first_qualified_match(cleaned: &str) -> Option<String> {
    let chars: Vec<char> = cleaned.chars().collect();
    for i in 0..chars.len() {
        if let Some(end) = match_qualified_at_most(&chars, i, 2) {
            return Some(chars[i..end].iter().copied().collect());
        }
    }
    None
}

/// React's `parseSchemaDropTargets`: the schema objects (tables, or tables
/// with a dropped column) that the DDL in `sql` removes, deduplicated on
/// `schema.table.column` (lowercased), in statement order.
///
/// The statement is noise-stripped first — line comments, block comments,
/// then string literals, in React's replace order — so a commented-out or
/// quoted `DROP TABLE` is never read. Statements that do not remove schema
/// objects simply produce no targets.
pub fn parse_schema_drop_targets(sql: &str) -> Vec<SqlSchemaDropTarget> {
    let ddl_only = strip_string_literals(&strip_block_comments(&strip_line_comments(sql)));
    let chars: Vec<char> = ddl_only.chars().collect();

    let mut targets: Vec<SqlSchemaDropTarget> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (body_start, body_end) in drop_table_bodies(&chars) {
        let body: Vec<char> = chars[body_start..body_end].to_vec();
        let prefix_end = cut_before_cascade(&body);
        let prefix: String = body[..prefix_end].iter().copied().collect();
        for part in prefix.split(',') {
            let cleaned = strip_leading_noise(part);
            if cleaned.is_empty() {
                continue;
            }
            if let Some(qualified) = first_qualified_match(&cleaned) {
                push_target(&mut targets, &mut seen, target_for_qualified(&qualified));
            }
        }
    }

    for (qual_start, qual_end, column_start, column_end) in alter_drop_columns(&chars) {
        let qualified: String = chars[qual_start..qual_end].iter().copied().collect();
        let column: String = chars[column_start..column_end].iter().copied().collect();
        let table = target_for_qualified(&qualified);
        let column = unquote(&column);
        // React: `if (table.table && column) push({ ...table, column })` —
        // the push guard re-checks the table.
        if !column.is_empty() {
            push_target(
                &mut targets,
                &mut seen,
                SqlSchemaDropTarget { column: Some(column), ..table },
            );
        }
    }

    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    // The React suite (`lib/__tests__/sqlSchemaDropTargets.test.ts`) is
    // mirrored it() by it(). The [tester] pins below go beyond it: the
    // hand-rolled regex semantics (identifier quoting forms, matchAll
    // resumption, the two backtracking paths, the noise passes) are exactly
    // what a future edit could silently break.

    fn target(schema: Option<&str>, table: &str, column: Option<&str>) -> SqlSchemaDropTarget {
        SqlSchemaDropTarget {
            schema: schema.map(str::to_owned),
            table: Some(table.to_owned()),
            column: column.map(str::to_owned),
        }
    }

    #[test]
    fn finds_dropped_tables_including_qualified_and_multiple_targets() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE IF EXISTS \"sales\".\"orders\", customers CASCADE;"),
            vec![
                target(Some("sales"), "orders", None),
                target(None, "customers", None),
            ]
        );
    }

    #[test]
    fn finds_a_dropped_column_on_a_qualified_table() {
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE sales.orders DROP COLUMN IF EXISTS legacy_id;"),
            vec![target(Some("sales"), "orders", Some("legacy_id"))]
        );
    }

    #[test]
    fn ignores_statements_that_do_not_delete_schema_objects() {
        assert_eq!(parse_schema_drop_targets("SELECT * FROM orders"), Vec::new());
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE orders ADD COLUMN note TEXT"),
            Vec::new()
        );
        assert_eq!(
            parse_schema_drop_targets("SELECT 'DROP TABLE orders' AS note"),
            Vec::new()
        );
        assert_eq!(parse_schema_drop_targets("-- DROP TABLE orders\nSELECT 1"), Vec::new());
    }

    // [tester] The identifier quoting forms and the whitespace-tolerant
    // dots of QUALIFIED, none of which the React suite reaches.

    #[test]
    fn unquotes_backtick_and_bracket_identifiers_and_spaced_dots() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE `sales`.`orders`"),
            vec![target(Some("sales"), "orders", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE [dbo].orders"),
            vec![target(Some("dbo"), "orders", None)]
        );
        // `\s*\.\s*` lets dots carry whitespace; unquote trims each part.
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE sales . orders DROP COLUMN note"),
            vec![target(Some("sales"), "orders", Some("note"))]
        );
        // Doubled-quote escapes unquote back to one quote.
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE t DROP COLUMN \"note\"\"x\""),
            vec![target(None, "t", Some("note\"x"))]
        );
    }

    // [tester] matchAll resumption across statements and the
    // schema.table.column dedupe key.

    #[test]
    fn scans_multiple_statements_and_dedupes_repeated_targets() {
        assert_eq!(
            parse_schema_drop_targets(
                "DROP TABLE a.x; DROP TABLE a.x; ALTER TABLE a.y DROP COLUMN c; \
                 ALTER TABLE a.y DROP COLUMN c"
            ),
            vec![target(Some("a"), "x", None), target(Some("a"), "y", Some("c"))]
        );
    }

    // [tester] The DROP TABLE `IF EXISTS` backtracking consequences — the
    // one place the capture's whitespace tolerance makes the engine's
    // unwinding observable.

    #[test]
    fn pins_the_drop_table_if_exists_backtracking_consequences() {
        // One space before the `;`: the group's trailing `\s+` cannot
        // unwind below one character, so the engine abandons the group and
        // the capture starts at `IF EXISTS` — whose leftmost QUALIFIED is
        // `IF`. The React behavior, faithfully.
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE IF EXISTS ;"),
            vec![target(None, "IF", None)]
        );
        // Two spaces: the capture starts inside the run (k = n-1), the
        // body trims to nothing, and no target is invented.
        assert_eq!(parse_schema_drop_targets("DROP TABLE IF EXISTS  ;"), Vec::new());
        // `IF EXISTS` at the end of the text: the group matches, the
        // capture is the two keywords, leftmost QUALIFIED `IF`.
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE IF EXISTS"),
            vec![target(None, "IF", None)]
        );
    }

    // [tester] The noise passes in React's order.

    #[test]
    fn strips_block_comments_and_escaped_string_literals() {
        assert_eq!(parse_schema_drop_targets("/* DROP TABLE ghost */ SELECT 1"), Vec::new());
        assert_eq!(
            parse_schema_drop_targets("SELECT 'DROP TABLE ''orders''' AS x"),
            Vec::new()
        );
    }

    // [tester] The ALTER tail's optionals and the QUALIFIED part cap.

    #[test]
    fn pins_alter_only_and_the_three_part_cap() {
        assert_eq!(
            parse_schema_drop_targets(
                "ALTER TABLE ONLY sales.orders DROP COLUMN IF EXISTS note"
            ),
            vec![target(Some("sales"), "orders", Some("note"))]
        );
        // A four-part name never matches: QUALIFIED caps at three parts
        // with nothing behind to backtrack into.
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE a.b.c.d DROP COLUMN note"),
            Vec::new()
        );
        // A three-part name takes the last two parts (the catalog in front
        // is dropped, as in React's targetForQualified).
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE cat.sales.orders DROP COLUMN note"),
            vec![target(Some("sales"), "orders", Some("note"))]
        );
    }

    // [tester] The `\b` around the keywords and the `/i` flag.

    #[test]
    fn requires_word_boundaries_around_the_keywords() {
        // `XDROP` has no `\b` before DROP.
        assert_eq!(parse_schema_drop_targets("XDROP TABLE y"), Vec::new());
        // An identifier named `drop_table` is not a statement either.
        assert_eq!(parse_schema_drop_targets("drop_table(y)"), Vec::new());
        // Lowercase keywords match (`/i`).
        assert_eq!(
            parse_schema_drop_targets("drop table if exists Orders"),
            vec![target(None, "Orders", None)]
        );
    }

    // [tester] Audit round — the identifier charset. `$` is an identifier
    // character (`[A-Za-z0-9_$]*`), while a DIGIT is not a start: in `9abc`
    // no identifier begins at `9`, so the leftmost QUALIFIED starts at `a`
    // and the digits are lost — React's `match` skips the position too.

    #[test]
    fn pins_the_identifier_charset_and_the_digit_start_rule() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE order$s"),
            vec![target(None, "order$s", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE 9abc"),
            vec![target(None, "abc", None)]
        );
    }

    // [tester] Audit round — the noise passes run in React's replace
    // order (line comments, then block comments, then string literals),
    // and the order is observable: a `--` inside a string literal is a
    // line comment that swallows the rest of the LINE — including a later
    // DROP TABLE — before the string pass ever sees the literal; and a
    // `--` inside a block comment swallows the `*/`, leaving the comment
    // unclosed so the DROP after it is comment text.

    #[test]
    fn pins_the_noise_pass_order() {
        assert_eq!(parse_schema_drop_targets("SELECT 'x--y'; DROP TABLE b"), Vec::new());
        assert_eq!(parse_schema_drop_targets("/* -- */ DROP TABLE x"), Vec::new());
    }

    // [tester] Audit round — `matchAll` resumes at each match's END. The
    // DROP capture `[^;]+` is greedy, so an unsemicoloned second DROP in
    // the same body is swallowed by the first capture and never
    // re-scanned; a quoted ALTER column can literally contain a whole
    // second ALTER statement, and resuming past the match end skips it.

    #[test]
    fn pins_matchall_resumption_at_each_match_end() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE a DROP TABLE b"),
            vec![target(None, "a", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE t DROP COLUMN \"ALTER TABLE y DROP COLUMN z\""),
            vec![target(None, "t", Some("ALTER TABLE y DROP COLUMN z"))]
        );
    }

    // [tester] Audit round — unquote's doubled-quote escape applies to
    // the backtick form too (React's `replace(/``/g, '`')`), and the
    // QUALIFIED part cap `{0,2}` holds in the DROP body scan as well: a
    // three-part name keeps the last two parts.

    #[test]
    fn pins_backtick_escapes_and_the_drop_body_part_cap() {
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE t DROP COLUMN `note``x`"),
            vec![target(None, "t", Some("note`x"))]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE cat.sch.tbl"),
            vec![target(Some("sch"), "tbl", None)]
        );
    }

    // [tester] Audit round — the `\b` AFTER the cut words
    // (`split(/\bCASCADE\b|\bRESTRICT\b/i)`): `RESTRICTED` is not the
    // whole word RESTRICT, so the body survives with the identifier as
    // the table; and the dedupe key is LOWERCASED while the pushed target
    // keeps the first occurrence's original casing.

    #[test]
    fn pins_the_cut_word_boundaries_and_the_lowercase_dedupe_key() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE RESTRICTED t;"),
            vec![target(None, "RESTRICTED", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE A; drop table a"),
            vec![target(None, "A", None)]
        );
    }

    // [tester] Audit round — stripLeadingNoise's `\s+` → ` ` collapse
    // runs before unquoting, so a bracketed identifier's inner double
    // space becomes one; and the bracketed form closes at the FIRST `]`
    // (`\[[^\]]+\]`), so `[a]b]` unquotes to `a` and the `b]` tail is
    // noise.

    #[test]
    fn pins_the_whitespace_collapse_and_the_first_bracket_close() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE [a  b]"),
            vec![target(None, "a b", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE [a]b]"),
            vec![target(None, "a", None)]
        );
    }

    // [tester] Audit round — the ALTER leading `(?:IF\s+EXISTS\s+)?` is
    // consumed by the group (with ONLY behind it), not read as the table;
    // and the quoted identifier's end-inside-literal backtrack: `"a b""`
    // (text ends inside the literal) closes at the first quote of the
    // last doubled-quote pair — the engine's only productive backtrack —
    // so it unquotes to `a b`.

    #[test]
    fn pins_the_alter_leading_if_exists_and_the_unterminated_quote_backtrack() {
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE IF EXISTS ONLY t DROP COLUMN c"),
            vec![target(None, "t", Some("c"))]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE \"a b\"\""),
            vec![target(None, "a b", None)]
        );
    }

    // [tester] Audit round 2 — the CASCADE/RESTRICT cut happens at the
    // FIRST whole word in the whole DROP body, BEFORE the comma-split: a
    // mid-list `a CASCADE, b` yields only `a` — `b` sits past the cut.

    #[test]
    fn cuts_the_body_at_a_mid_list_cascade_keyword() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE a CASCADE, b"),
            vec![target(None, "a", None)]
        );
    }

    // [tester] Audit round 2 — the string-literal noise pass replaces each
    // CLOSED literal with one space, not nothing: `x'y'z` stays two words
    // (an empty replacement would weld them into `xz`). An UNTERMINATED
    // literal is not noise at all — `'(?:''|[^'])*'` needs its closing
    // quote — so `'unterminated` stays and the body scan reads the bare
    // identifier behind the quote.

    #[test]
    fn string_literals_strip_to_a_separator_and_unterminated_ones_stay() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE x'y'z"),
            vec![target(None, "x", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE 'unterminated"),
            vec![target(None, "unterminated", None)]
        );
    }

    // [tester] Audit round 2 — a line comment starts at exactly `--`: a
    // single `-` is an ordinary character, so the statement behind it
    // survives; and `--[^\n]*` stops BEFORE the newline, which the
    // replacement keeps — the next line's statement is still scanned once
    // the first body closed at its `;`.

    #[test]
    fn a_single_dash_is_not_a_comment_and_a_comment_stops_before_the_newline() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE a; - DROP TABLE b"),
            vec![target(None, "a", None), target(None, "b", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE x; --c\nDROP TABLE y"),
            vec![target(None, "x", None), target(None, "y", None)]
        );
    }

    // [tester] Audit round 2 — an unclosed `/*` is ordinary text: the
    // non-greedy `[\s\S]*?\*/` needs its `*/`, so `DROP TABLE /*x` keeps
    // its body and the scan skips past `/*` to the bare identifier `x`.

    #[test]
    fn an_unclosed_block_comment_is_ordinary_text() {
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE /*x"),
            vec![target(None, "x", None)]
        );
    }

    // [tester] Audit round 2 — the block pass runs BEFORE the string pass
    // (React's replace order): `/* ' */ … ' */` closes the block comment
    // first, leaving one lone quote the string pass cannot pair, so the
    // DROP survives — string-first would weld `/* ' … ' */` into one noise
    // span and swallow it.

    #[test]
    fn block_comments_strip_before_string_literals() {
        assert_eq!(
            parse_schema_drop_targets("/* ' */ DROP TABLE t ; ' */"),
            vec![target(None, "t", None)]
        );
    }

    // [tester] Audit round 2 — `(?:COLUMN\s+)?` is optional: `DROP c`
    // without the keyword still names the column; and the bare identifier
    // HEAD is `[A-Za-z_]` — `_t` is a whole identifier (a letter-only head
    // would read it as `t`), and `_s`/`_c` ride the same rule through the
    // qualified ALTER path.

    #[test]
    fn accepts_a_drop_without_the_column_keyword_and_underscore_heads() {
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE t DROP c"),
            vec![target(None, "t", Some("c"))]
        );
        assert_eq!(
            parse_schema_drop_targets("DROP TABLE _t"),
            vec![target(None, "_t", None)]
        );
        assert_eq!(
            parse_schema_drop_targets("ALTER TABLE _s.t DROP COLUMN _c"),
            vec![target(Some("_s"), "t", Some("_c"))]
        );
    }
}
