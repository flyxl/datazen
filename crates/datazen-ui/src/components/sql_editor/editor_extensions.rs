//! Port of `packages/frontend/src/components/sql-editor/editorExtensions.ts`
//! (T139, split).
//!
//! The React file is the central CodeMirror extension assembly: it
//! composes the leaf factories (S4-A/B/C/D + S5-A) into compartment
//! groups. Rune has no CodeMirror — the engine under
//! [`crate::sql_editor`] is its replacement — so this port carries the
//! **assembly's decision logic**, the pure rules the file owns, and
//! documents the machinery as the engine seam (the same mapping T011's
//! table established):
//!
//! | React | Rune |
//! | --- | --- |
//! | `documentVersionField` (`docChanged \|\| BumpDocumentVersion` → +1) | [`document_version_bump`] (the engine's counter) |
//! | `currentThemeConfig` (settings → size/font/dark) | [`ThemeConfig`] + [`current_theme_config`] |
//! | `resolveEditorFontFamily` (driver-sdk) + `HOST_DEFAULT_EDITOR_FONT` | carried verbatim ([`resolve_editor_font_family`]) |
//! | the execute keymap's selection dispatch (263–277) | [`execute_key_dispatch`] |
//! | the model builder's ±200-line window (AC-19, 811–825) | [`window_large_document`] |
//! | `parentsEqual` + the update-listener dedup | [`qualified_path_step`] |
//! | the contextmenu SQL resolution (774–787) | [`context_menu_sql`] |
//! | the completion anchors / `validFor` classes / `prefixHint` strip (429–565) | [`function_anchor`], [`identifier_anchor`], [`prefix_hint`], [`admits_plain`], [`admits_qualified`] |
//!
//! ## Seams that stay out (their own tasks / segments)
//!
//! - `makeEditorTheme`'s CSS and `themeEditorColors`'s preset logic:
//!   the Rune view styles through `ui/palette` (segment 2's
//!   `lib/themeEditorColors.ts` owns the preset tables).
//! - `compartments` / the pro registry re-export: `proCompartments.ts`
//!   is T333 (this segment, later).
//! - `contextualKeywordCompletion` / `contextualSchemaCompletion`
//!   (`lib/sqlCompletionContext.ts`, segment 2), `sqlFunctionCompletions`
//!   (`lib/sqlCompletions.ts`, segment 2), the related-table boost cache
//!   (`lib/relationPrediction/relatedTables.ts`, segment 1 — the cache is
//!   memoization by snapshot identity, not semantics; the host holds the
//!   map keyed by epoch), `getActionShortcut`/`toCodeMirrorKeyFormat`
//!   (`lib/keymap.ts`, segment 2), and the SafeCompartmentWrapper EP
//!   machinery (extension-points, no engine counterpart).
//! - `snippetCompletionSource` (T357), `formatEditorDocument` (T218),
//!   `createPasteExtensions` (T261): their own tasks deepen the engine.
//!
//! Known deltas: none behavioural — every rule below is the React file's
//! verbatim.

// ---------------------------------------------------------------------------
// Theme config (currentThemeConfig + the driver-sdk font resolver)
// ---------------------------------------------------------------------------

/// driver-sdk's `HOST_DEFAULT_EDITOR_FONT` — the sentinel a user setting
/// must beat to count as explicit.
pub(crate) const HOST_DEFAULT_EDITOR_FONT: &str =
    "Menlo, Monaco, Consolas, \"Courier New\", monospace";

/// React `ThemeConfig`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThemeConfig {
    pub dark: bool,
    pub font_size: i64,
    pub font_family: String,
}

/// driver-sdk `resolveEditorFontFamily`: the explicit user setting wins
/// over the theme's `--font-editor` computed var, which wins over the
/// fallback (carried verbatim — the BaseTableSqlGenerator precedent).
pub(crate) fn resolve_editor_font_family(
    user_setting: &str,
    computed_editor_var: &str,
    fallback: &str,
) -> String {
    let trimmed_user = user_setting.trim();
    if !trimmed_user.is_empty() && trimmed_user != HOST_DEFAULT_EDITOR_FONT {
        return trimmed_user.to_owned();
    }
    let trimmed_theme = computed_editor_var.trim();
    if !trimmed_theme.is_empty() {
        return trimmed_theme.to_owned();
    }
    fallback.to_owned()
}

/// React `currentThemeConfig`: `editorFontSize` defaults to **14** when
/// unset (`fs || 14`), the font resolves through the user setting → the
/// computed `--font-editor` var → the host default.
pub(crate) fn current_theme_config(
    editor_font_size: i64,
    editor_font_family: &str,
    computed_editor_var: &str,
    dark: bool,
) -> ThemeConfig {
    ThemeConfig {
        dark,
        font_size: if editor_font_size == 0 { 14 } else { editor_font_size },
        font_family: resolve_editor_font_family(
            editor_font_family,
            computed_editor_var,
            HOST_DEFAULT_EDITOR_FONT,
        ),
    }
}

// ---------------------------------------------------------------------------
// Document version (documentVersionField)
// ---------------------------------------------------------------------------

/// `documentVersionField.update`: the counter bumps on every document
/// change and on every explicit `BumpDocumentVersion` effect (the
/// external value replacement), and nothing else.
pub(crate) fn document_version_bump(version: u64, doc_changed: bool, explicit_bump: bool) -> u64 {
    if doc_changed || explicit_bump {
        version + 1
    } else {
        version
    }
}

// ---------------------------------------------------------------------------
// The execute keymap's dispatch (createBaseEditorExtensions, 263–277)
// ---------------------------------------------------------------------------

/// What the execute key does: a trimmed selection runs just that SQL,
/// anything else runs the whole document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExecuteDispatch {
    /// `onExecuteSelection(sel)` — the trimmed selection.
    Selection(String),
    /// `onExecute()`.
    All,
}

/// The execute key's rule: a selection that trims to nothing is no
/// selection — the whole document runs.
pub(crate) fn execute_key_dispatch(selected: &str) -> ExecuteDispatch {
    let trimmed = selected.trim();
    if trimmed.is_empty() {
        ExecuteDispatch::All
    } else {
        ExecuteDispatch::Selection(selected.to_owned())
    }
}

// ---------------------------------------------------------------------------
// The model builder's windowing (AC-19, 811–825)
// ---------------------------------------------------------------------------

/// Documents beyond this many lines window to ±[`WINDOW_LINES`] around
/// the cursor for semantic-model builds.
pub(crate) const LARGE_DOCUMENT_LINES: usize = 1_000;

/// The half-window (in lines) large documents take.
pub(crate) const WINDOW_LINES: usize = 200;

/// The windowing rule: documents of at most 1000 lines pass through
/// untouched; larger ones window to ±200 lines around the cursor's line
/// (clamped to the document), with the cursor re-based into the window.
/// `doc` is the full text, `cursor` a byte offset; returns the windowed
/// text and the effective cursor (byte offset into the window).
pub(crate) fn window_large_document(doc: &str, cursor: usize) -> (String, usize) {
    let line_starts: Vec<usize> = doc
        .match_indices('\n')
        .map(|(i, _)| i + 1)
        .collect();
    // Line count with CodeMirror's semantics: a trailing newline does
    // not start a new line ("a\n" is one line), an empty document has
    // none, anything else is newlines + 1.
    let lines = if doc.is_empty() {
        0
    } else if doc.ends_with('\n') {
        line_starts.len()
    } else {
        line_starts.len() + 1
    };
    if lines <= LARGE_DOCUMENT_LINES {
        return (doc.to_owned(), cursor);
    }
    // 1-based line number of the cursor (the line whose [start, next_start)
    // contains the cursor; the last line runs to the end).
    let line_of = |offset: usize| -> usize {
        let mut line = 1;
        for start in &line_starts {
            if *start > offset {
                break;
            }
            line += 1;
        }
        line
    };
    let current_line = line_of(cursor.min(doc.len()));
    let start_line = current_line.saturating_sub(WINDOW_LINES).max(1);
    let end_line = (current_line + WINDOW_LINES).min(lines);
    // Byte offsets: line N starts at line_starts[N-1] (line 1 at 0); line
    // N ends where line N+1 starts, or the document end.
    let from_pos = if start_line == 1 { 0 } else { line_starts[start_line - 2] };
    let to_pos = if end_line == lines { doc.len() } else { line_starts[end_line - 1] };
    let windowed = doc[from_pos..to_pos].to_owned();
    let effective_cursor = cursor.saturating_sub(from_pos);
    (windowed, effective_cursor)
}

// ---------------------------------------------------------------------------
// The update listener's qualified-path dedup (738–756, 842–844)
// ---------------------------------------------------------------------------

/// `parentsEqual`: segment-by-segment equality.
pub(crate) fn parents_equal(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}

/// The update-listener dedup: `onQualifiedPath` fires only when the
/// parents actually changed — [`Some`] carries the new parents to store
/// and report, `None` means "same as last time, stay quiet".
pub(crate) fn qualified_path_step(last: &[String], next: Vec<String>) -> Option<Vec<String>> {
    if parents_equal(last, &next) {
        None
    } else {
        Some(next)
    }
}

// ---------------------------------------------------------------------------
// The contextmenu SQL resolution (774–787)
// ---------------------------------------------------------------------------

/// The contextmenu's SQL: the selection when there is one; otherwise the
/// statement at the cursor, or the trimmed whole document when even that
/// is unavailable.
pub(crate) fn context_menu_sql(
    selected: Option<&str>,
    statement_at_cursor: Option<&str>,
    doc: &str,
) -> String {
    match selected {
        Some(sel) if !sel.is_empty() => sel.to_owned(),
        _ => statement_at_cursor.map(str::to_owned).unwrap_or_else(|| doc.trim().to_owned()),
    }
}

// ---------------------------------------------------------------------------
// The completion anchors and admission classes (429–565)
// ---------------------------------------------------------------------------

/// `[\w$]` — the characters the function-source anchor walks back over
/// (`while from > 0 && /[\w$]/.test(doc.charAt(from - 1)) from -= 1`).
fn is_word_or_dollar(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '$'
}

/// `[\w$"']` — the identifier characters the schema-aware token-start
/// walk admits (quoted identifiers included).
fn is_identifier_quote_admitted(ch: char) -> bool {
    is_word_or_dollar(ch) || ch == '"' || ch == '\''
}

/// The function completion source's stable anchor: the start of the
/// word token containing `from` (byte offsets; walks back over `\w$`
/// characters, so a match that started mid-word anchors at the word
/// start and CodeMirror can reuse the option list across keystrokes).
pub(crate) fn function_anchor(doc: &str, from: usize) -> usize {
    token_anchor(doc, from, is_word_or_dollar)
}

/// The schema-aware source's token-start anchor: walks back over
/// `[\w$"']` characters (`match.from` may sit mid-token).
pub(crate) fn identifier_anchor(doc: &str, from: usize) -> usize {
    token_anchor(doc, from, is_identifier_quote_admitted)
}

/// The shared walk: the start of the maximal run of `admits` characters
/// that ends at `from`.
fn token_anchor(doc: &str, from: usize, admits: impl Fn(char) -> bool) -> usize {
    let byte_from = from.min(doc.len());
    // Byte offset of the first character of the contiguous run ending at
    // byte_from: scan backwards in char steps.
    let mut boundary = byte_from;
    let mut idx = byte_from;
    for (i, ch) in doc.char_indices().rev() {
        if i >= byte_from {
            continue;
        }
        // Only step back while the character directly before the current
        // run start is admitted.
        if i + ch.len_utf8() == idx && admits(ch) {
            idx = i;
            boundary = i;
        } else {
            break;
        }
    }
    boundary
}

/// `match?.text.replace(/^["'`\[]|["'`\]]$/g, '').toLowerCase()`: the
/// short-prefix hint strips the identifier's surrounding quote pair
/// (one leading and one trailing quote character) before lowercasing.
pub(crate) fn prefix_hint(match_text: &str) -> String {
    let mut text = match_text;
    if text.starts_with(['"', '\'', '`', '[']) {
        text = &text[1..];
    }
    if text.ends_with(['"', '\'', '`', ']']) && !text.is_empty() {
        text = &text[..text.len() - 1];
    }
    text.to_lowercase()
}

/// The column-context `validFor` class `/^[\w$"']*$/`: identifier
/// characters only, so backspace keeps reusing the cached result.
pub(crate) fn admits_plain(ch: char) -> bool {
    is_identifier_quote_admitted(ch)
}

/// The relation/join-target `validFor` class `/^[\w$"'.]*$/`:
/// additionally admits the dot, for qualified typing.
pub(crate) fn admits_qualified(ch: char) -> bool {
    admits_plain(ch) || ch == '.'
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // ---- theme ----------------------------------------------------------------

    #[test]
    fn theme_config_defaults_the_size_and_resolves_the_font() {
        // `fs || 14`: zero (the unset setting) is 14.
        let config = current_theme_config(0, "", "  ", true);
        assert_eq!(config.font_size, 14);
        assert!(config.dark);
        // The user setting wins when it is not the host-default sentinel.
        let explicit = current_theme_config(16, "JetBrains Mono", "", false);
        assert_eq!(explicit.font_size, 16);
        assert_eq!(explicit.font_family, "JetBrains Mono");
        // The sentinel falls through to the theme var, then the fallback.
        let via_theme = current_theme_config(16, HOST_DEFAULT_EDITOR_FONT, " --font-x ", false);
        assert_eq!(via_theme.font_family, "--font-x");
        let via_fallback = current_theme_config(16, "  ", "", false);
        assert_eq!(via_fallback.font_family, HOST_DEFAULT_EDITOR_FONT);
        // A trimmed user setting is used trimmed.
        assert_eq!(
            resolve_editor_font_family("  Fira Code  ", "", "fallback"),
            "Fira Code"
        );
    }

    // ---- document version --------------------------------------------------------

    #[test]
    fn the_version_bumps_on_change_or_effect_only() {
        assert_eq!(document_version_bump(0, false, false), 0);
        assert_eq!(document_version_bump(0, true, false), 1);
        assert_eq!(document_version_bump(0, false, true), 1, "the explicit bump effect counts");
        assert_eq!(document_version_bump(7, true, true), 8, "not twice");
        // A selection-only update does not bump.
        assert_eq!(document_version_bump(7, false, false), 7);
    }

    // ---- execute dispatch ---------------------------------------------------------

    #[test]
    fn the_execute_key_runs_a_selection_only_when_it_trims_nonempty() {
        assert_eq!(execute_key_dispatch("SELECT 1"), ExecuteDispatch::Selection("SELECT 1".to_owned()));
        assert_eq!(
            execute_key_dispatch("  SELECT 1  "),
            ExecuteDispatch::Selection("  SELECT 1  ".to_owned())
        );
        assert_eq!(execute_key_dispatch(""), ExecuteDispatch::All);
        assert_eq!(execute_key_dispatch("   \n\t "), ExecuteDispatch::All);
    }

    // ---- the windowing --------------------------------------------------------------

    fn line(n: usize) -> String {
        format!("line{n}\n")
    }

    #[test]
    fn small_documents_pass_through_untouched() {
        let doc = "SELECT 1;\nSELECT 2;\n";
        assert_eq!(window_large_document(doc, 4), (doc.to_owned(), 4));
        // Exactly 1000 lines is still small; 1001 windows.
        let mut thousand = String::new();
        for n in 1..=1_000 {
            thousand.push_str(&line(n));
        }
        let (text, cursor) = window_large_document(&thousand, 0);
        assert_eq!(text.len(), thousand.len());
        assert_eq!(cursor, 0);
    }

    #[test]
    fn large_documents_window_to_200_lines_around_the_cursor() {
        let mut doc = String::new();
        for n in 1..=1_400 {
            doc.push_str(&line(n));
        }
        // Cursor at the start of line 700: lines 500..=900 survive.
        let line700_start = doc.find("line700\n").expect("line 700 exists");
        let (text, cursor) = window_large_document(&doc, line700_start);
        assert!(text.starts_with("line500\n"), "window starts 200 lines back: {}", &text[..12]);
        assert!(text.ends_with("line900\n"), "window ends 200 lines ahead");
        assert_eq!(text.lines().count(), 401, "500..=900 inclusive");
        assert_eq!(&text[cursor..cursor + 7], "line700", "the cursor re-bases into the window");

        // A cursor near the top clamps at line 1.
        let (text, cursor) = window_large_document(&doc, 0);
        assert!(text.starts_with("line1\n"));
        assert!(text.ends_with("line201\n"));
        assert_eq!(cursor, 0);

        // A cursor near the end clamps at the last line.
        let end = doc.len() - 1;
        let (text, _cursor) = window_large_document(&doc, end);
        assert!(text.starts_with("line1200\n"), "clamped 200 back from 1400");
        assert!(text.ends_with("line1400\n"));
    }

    // ---- qualified-path dedup ---------------------------------------------------------

    #[test]
    fn qualified_path_fires_only_on_change() {
        let last = vec!["public".to_string(), "app".to_string()];
        assert_eq!(qualified_path_step(&last, vec!["public".into(), "app".into()]), None);
        assert_eq!(
            qualified_path_step(&last, vec!["public".into(), "other".into()]),
            Some(vec!["public".to_string(), "other".to_string()])
        );
        // A prefix is a change (length differs).
        assert_eq!(
            qualified_path_step(&last, vec!["public".into()]),
            Some(vec!["public".to_string()])
        );
        assert_eq!(
            qualified_path_step(&[], vec!["public".into()]),
            Some(vec!["public".to_string()]),
            "the first report always fires"
        );
        assert!(parents_equal(&last, &last));
        assert!(!parents_equal(&last, &[]));
    }

    // ---- contextmenu sql ----------------------------------------------------------------

    #[test]
    fn the_context_menu_takes_the_selection_else_the_statement_else_the_doc() {
        assert_eq!(context_menu_sql(Some("SELECT 1"), None, "doc"), "SELECT 1");
        assert_eq!(
            context_menu_sql(None, Some("SELECT 2;\n"), "whole doc"),
            "SELECT 2;\n"
        );
        assert_eq!(context_menu_sql(None, None, "  whole doc  "), "whole doc");
        // An empty selection behaves like none.
        assert_eq!(context_menu_sql(Some(""), Some("stmt", ), " doc "), "stmt");
    }

    // ---- anchors and admission -------------------------------------------------------------

    #[test]
    fn anchors_walk_back_to_the_token_start() {
        // The function anchor walks over word characters only.
        let doc = "SELECT us";
        assert_eq!(function_anchor(doc, doc.len()), 7);
        // The identifier anchor also walks over quotes — but a space is
        // still a boundary: the anchor is the last identifier run's start.
        let doc = "FROM \"app users";
        assert_eq!(identifier_anchor(doc, doc.len()), 10, "the space stops the walk");
        // A quoted run without spaces anchors at the opening quote.
        let doc = "FROM \"app_users";
        assert_eq!(identifier_anchor(doc, doc.len()), 5);
        // Mid-token match.from anchors at the token start.
        let doc = "users";
        assert_eq!(identifier_anchor(doc, 3), 0);
        // Anchors at 0 stay 0; anchors after a separator start fresh.
        assert_eq!(function_anchor("a.bc", 4), 2, "the dot is not a word char");
    }

    #[test]
    fn prefix_hint_strips_one_quote_pair_and_lowercases() {
        assert_eq!(prefix_hint("\"Users\""), "users");
        assert_eq!(prefix_hint("`Orders"), "orders");
        assert_eq!(prefix_hint("[Tbl"), "tbl");
        assert_eq!(prefix_hint("plain"), "plain");
        assert_eq!(prefix_hint("MixedCase"), "mixedcase");
        // Only the outermost characters: inner quotes survive.
        assert_eq!(prefix_hint("\"a\"b\""), "a\"b");
        // And only ONE pair: doubled quotes are not fully stripped.
        assert_eq!(prefix_hint("\"\"a\"\""), "\"a\"");
    }

    #[test]
    fn valid_for_admission_matches_the_regex_classes() {
        assert!(admits_plain('a'));
        assert!(admits_plain('7'));
        assert!(admits_plain('_'));
        assert!(admits_plain('$'));
        assert!(admits_plain('"'));
        assert!(admits_plain('\''));
        assert!(!admits_plain('.'));
        assert!(!admits_plain(' '));
        // Qualified admits everything plain does plus the dot.
        assert!(admits_qualified('.'));
        assert!(admits_qualified('a'));
        assert!(!admits_qualified(' '));
    }
}
