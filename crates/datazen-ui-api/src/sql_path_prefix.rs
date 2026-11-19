//! SQL statement path-prefix parsing — ported from React
//! `packages/frontend/src/lib/sqlPathPrefix.ts` (tasklist T619, 130 lines).
//!
//! The query editor's completion path: which `db.schema.` prefix sits under
//! the cursor. [`parse_qualified_path_parents`] recovers the parent segments
//! of the (possibly still-being-typed) qualified token at the cursor,
//! [`extract_qualified_token`] is the token scanner behind it (also used
//! standalone by `lib/queryContextPath.ts`), [`is_inside_quotes`] keeps both
//! from reading inside a string literal, and [`resolve_ensure_segments`]
//! maps the parents onto the catalog fetch path — unqualified names
//! (`FROM users`) resolve to the current database so its tables/catalogs can
//! be completed without typing `db.`.
//!
//! ## Mirror rules applied
//!
//! - Positions are **character indices**. React's are UTF-16 code units,
//!   which coincide with char indices for BMP text, and every character
//!   this scanner special-cases (`'`, `"`, `.`, `[A-Za-z0-9_$]`) is ASCII
//!   anyway. Out-of-range positions clamp to `[0, len]` like the React
//!   `Math.max(0, Math.min(pos, text.length))`; `usize` cannot go negative.
//! - React's `IDENT = /[A-Za-z0-9_$]/` becomes the module-private
//!   [`is_ident_char`]. `$` is an identifier character but not a `\b` word
//!   character — that split never bites here because this file has no `\b`.
//! - `null` → `Option`; the token object → [`QualifiedToken`]; the inline
//!   options object → the borrowed [`ResolveEnsureOptions`]
//!   (`currentDatabase: string | null` → `Option<&str>`,
//!   `readonly string[]` → `&[&str]`), with the return borrowing back
//!   (`Vec<&str>`) so the `[current, ...parents]` prepend copies references,
//!   not strings — React returns a fresh array of the same string values.
//! - `Set.has` string membership → `HashSet<&str>` (byte equality, the
//!   SameValueZero of strings); JS `trim` → [`str::trim`] (the two
//!   whitespace vocabularies only differ on exotic code points).
//! - No serde: the outputs feed in-process completion logic
//!   (`lib/ensureNamespace.ts` and the sql-editor extensions), nothing that
//!   crosses a storage/IPC boundary.

use std::collections::HashSet;

/// React's `IDENT = /[A-Za-z0-9_$]/` — one identifier character.
fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

/// React's `isInsideQuotes`: whether position `pos` sits inside an
/// unterminated `'…'`/`"…"` literal. Doubled quotes escape, and an escape
/// that starts before `pos` keeps `pos` inside even when the pair straddles
/// it — the React loop checks `i + 1 < text.length` (the full length, not
/// the clamped end) before treating a quote as an escape.
pub fn is_inside_quotes(text: &str, pos: usize) -> bool {
    is_inside_quotes_chars(&text.chars().collect::<Vec<char>>(), pos)
}

/// `is_inside_quotes` over already-collected characters.
fn is_inside_quotes_chars(chars: &[char], pos: usize) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let end = pos.min(chars.len());
    let mut i = 0;
    while i < end {
        let ch = chars[i];
        if in_single {
            if ch == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    i += 1; // the escaped pair — the React loop's inner `i++`
                } else {
                    in_single = false;
                }
            }
        } else if in_double {
            if ch == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    i += 1;
                } else {
                    in_double = false;
                }
            }
        } else if ch == '\'' {
            in_single = true;
        } else if ch == '"' {
            in_double = true;
        }
        i += 1;
    }
    in_single || in_double
}

/// React's `extractQualifiedToken` return: the token text (which may carry a
/// trailing dot, e.g. `hive.snap.`) and whether it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualifiedToken {
    pub value: String,
    pub ends_with_dot: bool,
}

/// React's `extractQualifiedToken`: the qualified identifier around `pos`,
/// extended left across `ident.ident.` segments and right to the end of the
/// typed segment (a trailing dot included). `None` when `pos` is inside a
/// string literal, on a separator with no token character behind it, or at
/// a text edge without a preceding token character.
pub fn extract_qualified_token(text: &str, pos: usize) -> Option<QualifiedToken> {
    let chars: Vec<char> = text.chars().collect();
    if is_inside_quotes_chars(&chars, pos) {
        return None;
    }
    let len = chars.len();
    let mut cursor = pos.min(len);

    if cursor < len && !is_ident_char(chars[cursor]) && chars[cursor] != '.' {
        if cursor > 0 && (is_ident_char(chars[cursor - 1]) || chars[cursor - 1] == '.') {
            cursor -= 1;
        } else {
            return None;
        }
    } else if cursor == len {
        if cursor == 0 || (!is_ident_char(chars[cursor - 1]) && chars[cursor - 1] != '.') {
            return None;
        }
    }

    // Walk back to the start of the segment under the cursor.
    let mut start = cursor;
    while start > 0 && is_ident_char(chars[start - 1]) {
        start -= 1;
    }

    // Extend left over `ident.` segments: the segment before a dot joins the
    // token only when the dot itself has an identifier before it (`k` moved).
    let mut i = start;
    while i > 0 {
        if chars[i - 1] == '.' {
            let mut k = i - 1;
            while k > 0 && is_ident_char(chars[k - 1]) {
                k -= 1;
            }
            if k == i - 1 {
                break;
            }
            i = k;
            continue;
        }
        break;
    }
    start = i;

    // Extend right: identifier, optional dot. A trailing dot stays part of
    // the value (`endsWithDot`); a second dot or a non-identifier ends it.
    let mut end = start;
    while end < len {
        let ident_start = end;
        while end < len && is_ident_char(chars[end]) {
            end += 1;
        }
        if end == ident_start {
            break;
        }
        if end < len && chars[end] == '.' {
            end += 1;
            continue;
        }
        break;
    }

    let value: String = chars[start..end].iter().copied().collect();
    if value.is_empty() || !is_ident_char(chars[start]) {
        return None;
    }
    Some(QualifiedToken { ends_with_dot: value.ends_with('.'), value })
}

/// React's `parseQualifiedPathParents`: the parent segments of the qualified
/// token at `cursor`. A trailing dot keeps every segment (`hive.snap.` →
/// `hive, snap` — completion offers both levels); a partial last segment
/// (`hive.sn`) drops it; quoted text or a top-level identifier → `[]`.
pub fn parse_qualified_path_parents(text: &str, cursor: usize) -> Vec<String> {
    let pos = cursor.min(text.chars().count());
    if is_inside_quotes(text, pos) {
        return Vec::new();
    }
    let Some(token) = extract_qualified_token(text, pos) else {
        return Vec::new();
    };
    let segs: Vec<String> = token
        .value
        .split('.')
        .filter(|seg| !seg.is_empty())
        .map(str::to_owned)
        .collect();
    if token.ends_with_dot {
        return segs;
    }
    if segs.len() <= 1 {
        return Vec::new();
    }
    segs[..segs.len() - 1].to_vec()
}

/// React's `resolveEnsureSegments` options: the editor's current database
/// (`currentDatabase`), the roots that are already catalog-visible
/// (`knownRoots` — the databases plus path aliases in the React caller),
/// and whether an empty/`unknown` first segment should be prefixed with the
/// current database (`useCurrentDatabaseRoot`).
#[derive(Clone, Copy, Debug)]
pub struct ResolveEnsureOptions<'a> {
    /// Blank-after-trim counts as absent, mirroring the React
    /// `currentDatabase?.trim() || null`.
    pub current_database: Option<&'a str>,
    pub known_roots: &'a [&'a str],
    pub use_current_database_root: bool,
}

/// Map the editor's qualified-path parents onto a fetch path. Unqualified
/// names (`FROM users`) resolve to the current database so its
/// tables/catalogs can be completed without typing `db.`.
///
/// - empty parents + `use_current_database_root` → `[current]`;
/// - a first segment that is already a known root (or the current database
///   itself — it joins the root set) stays as-is;
/// - otherwise, and only when `use_current_database_root` is set, `current`
///   is prepended; a database is never invented otherwise.
pub fn resolve_ensure_segments<'a>(
    parents: &[&'a str],
    opts: &ResolveEnsureOptions<'a>,
) -> Vec<&'a str> {
    // React: `opts.currentDatabase?.trim() || null`.
    let current = opts
        .current_database
        .map(str::trim)
        .filter(|current| !current.is_empty());
    let mut roots: HashSet<&str> = opts.known_roots.iter().copied().collect();
    if let Some(current) = current {
        roots.insert(current);
    }
    let use_current = opts.use_current_database_root && current.is_some();
    if parents.is_empty() {
        return if use_current { current.into_iter().collect() } else { Vec::new() };
    }
    if roots.contains(parents[0]) {
        return parents.to_vec();
    }
    if use_current {
        return current.into_iter().chain(parents.iter().copied()).collect();
    }
    parents.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The React suite (`lib/__tests__/sqlPathPrefix.test.ts`) is mirrored
    // it() by it(): the four under `describe('parseQualifiedPathParents')`
    // and the four under `describe('resolveEnsureSegments')`. The [tester]
    // pins at the tail hold the exports that suite never pins —
    // `isInsideQuotes`, `extractQualifiedToken`, and `resolveEnsureSegments`'s
    // trim/blank handling — which are exactly the paths a hand port could
    // drift on. The fixtures are ASCII, so `sql.len()` is simultaneously the
    // char count (this port's position space) and React's `sql.length`.

    #[test]
    fn returns_parents_after_a_trailing_dot() {
        let sql = "SELECT * FROM hive.snap.";
        assert_eq!(parse_qualified_path_parents(sql, sql.len()), vec!["hive", "snap"]);
    }

    #[test]
    fn returns_parents_for_partial_last_segment() {
        let sql = "SELECT * FROM hive.sn";
        assert_eq!(parse_qualified_path_parents(sql, sql.len()), vec!["hive"]);
    }

    #[test]
    fn returns_empty_at_top_level_identifier() {
        let sql = "SELECT * FROM hi";
        assert_eq!(parse_qualified_path_parents(sql, sql.len()), Vec::<String>::new());
    }

    #[test]
    fn ignores_dots_inside_strings() {
        let sql = "SELECT 'a.b.' FROM hive.";
        assert_eq!(parse_qualified_path_parents(sql, sql.len()), vec!["hive"]);
    }

    #[test]
    fn uses_current_database_for_unqualified_names() {
        let opts = ResolveEnsureOptions {
            current_database: Some("app"),
            known_roots: &["app", "other"],
            use_current_database_root: true,
        };
        assert_eq!(resolve_ensure_segments(&[], &opts), vec!["app"]);
    }

    #[test]
    fn keeps_an_explicit_database_prefix() {
        let opts = ResolveEnsureOptions {
            current_database: Some("app"),
            known_roots: &["app", "other"],
            use_current_database_root: true,
        };
        assert_eq!(resolve_ensure_segments(&["other"], &opts), vec!["other"]);
    }

    #[test]
    fn prepends_current_database_when_first_segment_is_not_a_known_root() {
        let opts = ResolveEnsureOptions {
            current_database: Some("presto"),
            known_roots: &["presto"],
            use_current_database_root: true,
        };
        assert_eq!(resolve_ensure_segments(&["hive", "snap"], &opts), vec!["presto", "hive", "snap"]);
    }

    #[test]
    fn does_not_invent_a_database_when_use_current_database_root_is_false() {
        let opts = ResolveEnsureOptions {
            current_database: Some("app"),
            known_roots: &["app"],
            use_current_database_root: false,
        };
        assert_eq!(resolve_ensure_segments(&[], &opts), Vec::<&str>::new());
    }

    // [tester] The React suite leaves `isInsideQuotes` unpinned; these hold
    // the literal/escape rules the two scanners above depend on.

    #[test]
    fn is_inside_quotes_pins_the_literal_and_escape_rules() {
        assert!(is_inside_quotes("'a.b'", 2));
        assert!(is_inside_quotes("\"a.b\"", 2));
        // Outside once the literal has closed.
        assert!(!is_inside_quotes("'a' b", 4));
        // A doubled quote is an escape: the position between the pair is
        // still inside the literal the first quote opened...
        assert!(is_inside_quotes("'a''b'", 2));
        // ...and a position right after the pair stays inside too (the
        // escape straddles the cursor in React's full-length check).
        assert!(is_inside_quotes("'a''b'", 5));
        // The literal only closes at a singleton quote.
        assert!(!is_inside_quotes("'a'''", 6));
    }

    // [tester] `extractQualifiedToken` is likewise unpinned in React.

    #[test]
    fn extract_qualified_token_pins_separator_and_trailing_dot_cursors() {
        // Cursor mid-segment: the whole qualified token, no trailing dot.
        let token = extract_qualified_token("SELECT hive.sn", 13);
        assert_eq!(
            token,
            Some(QualifiedToken { value: "hive.sn".to_owned(), ends_with_dot: false })
        );
        // Cursor on the separator right after a token steps back into it.
        let token = extract_qualified_token("FROM hive , x", 9);
        assert_eq!(
            token,
            Some(QualifiedToken { value: "hive".to_owned(), ends_with_dot: false })
        );
        // A trailing dot stays part of the value.
        let token = extract_qualified_token("FROM hive.", 10);
        assert_eq!(
            token,
            Some(QualifiedToken { value: "hive.".to_owned(), ends_with_dot: true })
        );
        // Inside a string literal there is no token.
        assert_eq!(extract_qualified_token("'hive.'", 2), None);
        // A separator with no token character behind it has nothing to read.
        assert_eq!(extract_qualified_token("FROM ,", 5), None);
    }

    // [tester] `resolveEnsureSegments`'s own normalization: the current
    // database is trimmed before use, and one that is blank afterwards
    // counts as absent (React's `currentDatabase?.trim() || null`).

    #[test]
    fn resolve_ensure_segments_trims_the_current_database_and_blanks_go_absent() {
        let opts = ResolveEnsureOptions {
            current_database: Some("  app  "),
            known_roots: &["other"],
            use_current_database_root: true,
        };
        assert_eq!(resolve_ensure_segments(&[], &opts), vec!["app"]);
        // Blank after trimming: nothing to invent even when the flag is set.
        let opts = ResolveEnsureOptions {
            current_database: Some("   "),
            known_roots: &[],
            use_current_database_root: true,
        };
        assert_eq!(resolve_ensure_segments(&[], &opts), Vec::<&str>::new());
        // The current database joins the root set, so it is never doubled
        // in front of itself.
        let opts = ResolveEnsureOptions {
            current_database: Some("app"),
            known_roots: &["other"],
            use_current_database_root: true,
        };
        assert_eq!(resolve_ensure_segments(&["app", "snap"], &opts), vec!["app", "snap"]);
    }

    // [tester] Audit round — the identifier charset. React's `IDENT` is
    // exactly ASCII `[A-Za-z0-9_$]`: `_` and `$` are identifier characters
    // (a `\w` port would lose `$`, a trimmed set would lose `_`), while a
    // non-ASCII letter is NOT one — the cursor at the end of `a.é` has no
    // token character behind it, so no token exists there (React's
    // `/[A-Za-z0-9_$]/.test('é')` is false).

    #[test]
    fn pins_the_ascii_ident_charset() {
        // `_` and `$` extend the token across the dot hop.
        let token = extract_qualified_token("a$._b", 5);
        assert_eq!(
            token,
            Some(QualifiedToken { value: "a$._b".to_owned(), ends_with_dot: false })
        );
        // `é` ends no token: the text ends in a non-identifier character.
        let sql = "SELECT * FROM a.é";
        let cursor = sql.chars().count();
        assert_eq!(parse_qualified_path_parents(sql, cursor), Vec::<String>::new());
    }

    // [tester] Audit round — the doubled-quote escape straddling the
    // cursor. React checks `i + 1 < text.length` — the FULL length, not
    // the clamped scan end — so a `''`/`""` pair that starts before the
    // cursor keeps the cursor inside the literal even when the pair's
    // second quote sits past the cursor's clamp. Position 3 — the second
    // quote of the pair — is the straddling case (the pin above sits at 2
    // and 5, on either side of it).

    #[test]
    fn is_inside_quotes_pins_the_straddling_escape_pair() {
        assert!(is_inside_quotes("'a''b'", 3));
        assert!(is_inside_quotes("\"a\"\"b\"", 3));
    }

    // [tester] Audit round — `useCurrentDatabaseRoot` gates the prepend
    // for NON-EMPTY parents too: with the flag off, unknown first segments
    // pass through untouched, and with no current database at all nothing
    // can be prepended (a database is never invented either way).

    #[test]
    fn resolve_ensure_segments_gates_the_prepend_for_non_empty_parents() {
        let opts = ResolveEnsureOptions {
            current_database: Some("app"),
            known_roots: &["app"],
            use_current_database_root: false,
        };
        assert_eq!(resolve_ensure_segments(&["hive", "snap"], &opts), vec!["hive", "snap"]);
        let opts = ResolveEnsureOptions {
            current_database: None,
            known_roots: &[],
            use_current_database_root: true,
        };
        assert_eq!(resolve_ensure_segments(&["hive"], &opts), vec!["hive"]);
    }

    // [tester] Audit round — the cursor step-back accepts a preceding DOT
    // as well as an identifier character: a cursor on the separator right
    // after `hive.` steps back onto the trailing dot and reads the
    // `hive.` token (React's `text[cursor - 1] === '.'`).

    #[test]
    fn extract_qualified_token_steps_back_onto_a_trailing_dot() {
        assert_eq!(
            extract_qualified_token("hive. , x", 5),
            Some(QualifiedToken { value: "hive.".to_owned(), ends_with_dot: true })
        );
    }

    // [tester] Audit round 2 — a doubled dot does not extend the token
    // across both dots. The left extension requires the dot to have an
    // identifier before IT (`k` moved); `a..b`'s second dot sits behind
    // the first dot, so the token is just `b` — and one lone segment has
    // no parents.

    #[test]
    fn a_double_dot_does_not_join_the_left_segment() {
        assert_eq!(
            extract_qualified_token("a..b", 4),
            Some(QualifiedToken { value: "b".to_owned(), ends_with_dot: false })
        );
        assert_eq!(parse_qualified_path_parents("a..b", 4), Vec::<String>::new());
    }
}
