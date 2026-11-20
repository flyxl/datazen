//! Port of `packages/frontend/src/components/sql-editor/snippets/
//! snippetCompletionSource.ts` (T359).
//!
//! Snippet completion source. Coexists with the keyword / schema /
//! function sources rather than replacing them: relevance is
//! expressed through `boost` (soft ordering) so snippets never starve
//! table or column suggestions. The single hard exclusion is a
//! dot-qualified position (`alias.`), where a snippet prefix is
//! syntactically impossible.
//!
//! ## What maps where
//!
//! - `buildSnippetCompletions` projects a library into the
//!   `Completion` face ([`SnippetCompletion`]: label = prefix,
//!   detail = the translated `descriptionKey`, type `"text"`, apply =
//!   the template) paired with its source item
//!   ([`SnippetBuildPair`]) — the React `{ item, completion }` shape
//!   the custom-snippet journey reads.
//! - `createSnippetCompletionSource` is the closure over a context.
//!   CodeMirror's context becomes two inputs — the text before the
//!   cursor (`state.sliceDoc(0, pos)`) and the explicit flag — and
//!   the source answers [`SnippetSourceResult`] (`from`, the boosted
//!   options, `validFor`) or `None` (the React `null`s).
//! - [`snippet_completion_valid_for`] is the `validFor` pattern
//!   `^[A-Za-z_][A-Za-z0-9_*]*$`: `*` must stay in it or typing
//!   `sel*` invalidates the list.
//! - `inferSqlCompletionKind` is carried in
//!   [`super::completion_context_helpers`] (its source file is
//!   another segment's ledger entry).
//!
//! The tabstop-walk journey (expand → select default → Tab →
//! Shift-Tab → deactivate) is the editor engine's snippet session,
//! not this source's; the template strings themselves are pinned by
//! the builtin library's audit.
//!
//! Known deltas: CodeMirror's `matchBefore` windows the line to 250
//! characters before the cursor; this port matches the whole line.
//! A word longer than 250 characters is outside every pinned suite
//! and outside SQL practice; the match itself is identical.

use crate::components::sql_editor::snippets::builtin::builtin_sql_snippets;
use crate::components::sql_editor::snippets::completion_context_helpers::{
    SqlCompletionKind, infer_sql_completion_kind,
};
use crate::components::sql_editor::snippets::types::SqlSnippetItem;

/// Statement-level snippets are the point of the feature; keep them
/// visible.
pub(crate) const BOOST_STATEMENT_CONTEXT: i32 = 5;
/// Inside a table or column position the user wants identifiers, not
/// templates.
pub(crate) const BOOST_IDENTIFIER_CONTEXT: i32 = -6;

/// `SnippetCompletionOptions`: the library (defaults to the builtins)
/// and the `descriptionKey` translator (defaults to echoing the key).
pub(crate) struct SnippetCompletionOptions<'a> {
    pub(crate) snippets: Option<&'a [SqlSnippetItem]>,
    /// Resolves `descriptionKey` to display text; defaults to echoing
    /// the key.
    pub(crate) translate: Option<&'a dyn Fn(&str) -> String>,
}

impl Default for SnippetCompletionOptions<'_> {
    fn default() -> Self {
        Self { snippets: None, translate: None }
    }
}

/// The `Completion` face of one snippet (label / detail / type /
/// apply-template); `boost` is applied per-context by the source, not
/// by the builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SnippetCompletion {
    pub(crate) label: String,
    pub(crate) detail: String,
    pub(crate) completion_type: &'static str,
    /// `snippetCompletion(template)`'s apply payload.
    pub(crate) template: String,
}

/// One `{ item, completion }` pair of `buildSnippetCompletions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SnippetBuildPair {
    pub(crate) item: SqlSnippetItem,
    pub(crate) completion: SnippetCompletion,
}

/// One offered option: the completion plus its context boost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SnippetSourceOption {
    pub(crate) completion: SnippetCompletion,
    pub(crate) boost: i32,
}

/// The source's answer (the CodeMirror `CompletionResult`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SnippetSourceResult {
    pub(crate) from: usize,
    pub(crate) options: Vec<SnippetSourceOption>,
}

/// Build the `Completion[]` projection of a snippet library.
pub(crate) fn build_snippet_completions(
    options: &SnippetCompletionOptions<'_>,
) -> Vec<SnippetBuildPair> {
    let snippets = options.snippets.unwrap_or(builtin_sql_snippets());
    let echo = |key: &str| key.to_owned();
    let translate = options.translate.unwrap_or(&echo);
    snippets
        .iter()
        .map(|item| SnippetBuildPair {
            item: item.clone(),
            completion: SnippetCompletion {
                label: item.prefix.clone(),
                detail: translate(&item.description_key),
                completion_type: "text",
                template: item.template.clone(),
            },
        })
        .collect()
}

/// The `validFor` pattern `^[A-Za-z_][A-Za-z0-9_*]*$`.
pub(crate) fn snippet_completion_valid_for(word: &str) -> bool {
    let b = word.as_bytes();
    match b.first() {
        Some(&c) if c.is_ascii_alphabetic() || c == b'_' => {}
        _ => return false,
    }
    b[1..].iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'*')
}

/// `matchBefore(SNIPPET_PREFIX)` — the leftmost `[A-Za-z_]` start
/// whose suffix to the cursor is all `[A-Za-z0-9_*]`, on the cursor's
/// line. Returns `(from, to)` as absolute offsets into the document
/// (the caller's `text_before` is `sliceDoc(0, pos)`).
fn match_snippet_prefix(text_before: &str) -> Option<(usize, usize)> {
    let line_start = text_before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = text_before.as_bytes();
    let n = line.len();
    // The maximal class-char run ending at the cursor.
    let mut start = n;
    while start > line_start && is_prefix_class(line[start - 1]) {
        start -= 1;
    }
    // The leftmost valid start inside the run must be `[A-Za-z_]`.
    while start < n && !(line[start].is_ascii_alphabetic() || line[start] == b'_') {
        start += 1;
    }
    if start >= n {
        return None;
    }
    Some((start, n))
}

fn is_prefix_class(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'*'
}

/// The hard exclusion `/\.\s*[\w$"'`]*$/`: a qualifier dot (optionally
/// followed by whitespace and a partial identifier or quote) — a
/// snippet prefix can never follow it.
fn after_qualifier_dot(text_before: &str) -> bool {
    let b = text_before.as_bytes();
    let mut i = b.len();
    while i > 0
        && (b[i - 1].is_ascii_alphanumeric()
            || b[i - 1] == b'_'
            || b[i - 1] == b'$'
            || b[i - 1] == b'"'
            || b[i - 1] == b'\''
            || b[i - 1] == b'`')
    {
        i -= 1;
    }
    while i > 0 && matches!(b[i - 1], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i -= 1;
    }
    i > 0 && b[i - 1] == b'.'
}

/// The source body: `createSnippetCompletionSource(options)` applied
/// to a context. `text_before` is `state.sliceDoc(0, pos)`;
/// `explicit` is the CodeMirror explicit-request flag.
pub(crate) fn snippet_completion_source(
    options: &SnippetCompletionOptions<'_>,
    text_before: &str,
    explicit: bool,
) -> Option<SnippetSourceResult> {
    // A snippet prefix can never follow a qualifier dot — the only
    // 100% mutually exclusive case, so hard-exclude it here.
    if after_qualifier_dot(text_before) {
        return None;
    }

    let matched = match_snippet_prefix(text_before);
    if matched.is_none() && !explicit {
        return None;
    }

    let kind = infer_sql_completion_kind(text_before);
    let boost =
        if kind == SqlCompletionKind::Any { BOOST_STATEMENT_CONTEXT } else { BOOST_IDENTIFIER_CONTEXT };

    Some(SnippetSourceResult {
        from: matched.map(|(from, _)| from).unwrap_or(text_before.len()),
        options: build_snippet_completions(options)
            .into_iter()
            .map(|pair| SnippetSourceOption { completion: pair.completion, boost })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(result: &SnippetSourceResult) -> Vec<&str> {
        result.options.iter().map(|o| o.completion.label.as_str()).collect()
    }

    fn first_boost(result: &SnippetSourceResult) -> i32 {
        result.options[0].boost
    }

    /// The journey suite's `completeAt`: the default source at the
    /// end of `doc`.
    fn complete_at(doc: &str, explicit: bool) -> Option<SnippetSourceResult> {
        snippet_completion_source(&SnippetCompletionOptions::default(), doc, explicit)
    }

    #[test]
    fn offers_snippets_at_statement_start_with_a_positive_boost() {
        let result = complete_at("sel", false).expect("answers");
        assert!(labels(&result).contains(&"sel*"));
        assert!(first_boost(&result) > 0);
    }

    #[test]
    fn captures_the_trailing_star_so_the_prefix_anchors() {
        let result = complete_at("sel*", false).expect("answers");
        // `from` must cover the whole typed prefix, else expansion
        // duplicates text.
        assert_eq!(result.from, 0);
        assert!(labels(&result).contains(&"sel*"));
    }

    #[test]
    fn match_before_is_scoped_to_the_cursor_line_and_offsets_are_absolute() {
        let result = complete_at("SELECT\nsel", false).expect("answers");
        assert_eq!(result.from, "SELECT\n".len());
        let result = complete_at("SELECT sel", false).expect("answers");
        assert_eq!(result.from, "SELECT ".len());
    }

    #[test]
    fn downweights_but_never_removes_snippets_in_a_table_position() {
        let result = complete_at("SELECT * FROM cou", false).expect("answers");
        // Soft ordering, not a hard filter: the list survives, ranked
        // below tables.
        assert!(labels(&result).contains(&"count"));
        assert!(first_boost(&result) < 0);
    }

    #[test]
    fn downweights_snippets_in_a_column_position() {
        let result = complete_at("SELECT co", false).expect("answers");
        assert!(first_boost(&result) < 0);
    }

    #[test]
    fn hard_excludes_qualified_positions_where_a_prefix_is_impossible() {
        assert!(complete_at("SELECT u.", false).is_none());
        assert!(complete_at("SELECT u.na", false).is_none());
        assert!(complete_at("SELECT * FROM public.", false).is_none());
    }

    #[test]
    fn stays_silent_on_an_implicit_keystroke_with_no_word_prefix() {
        assert!(complete_at("SELECT * FROM ", false).is_none());
    }

    #[test]
    fn still_answers_an_explicit_request_with_no_prefix() {
        let result = complete_at("", true).expect("answers");
        assert_eq!(result.options.len(), builtin_sql_snippets().len());
        assert_eq!(result.from, 0);
    }

    #[test]
    fn valid_for_keeps_the_star_or_typing_sel_star_invalidates_the_list() {
        assert!(snippet_completion_valid_for("sel*"));
        assert!(snippet_completion_valid_for("a"));
        assert!(snippet_completion_valid_for("_a1"));
        assert!(!snippet_completion_valid_for(""));
        assert!(!snippet_completion_valid_for("1a"));
        assert!(!snippet_completion_valid_for("sel x"));
        assert!(!snippet_completion_valid_for("*sel"));
        assert!(!snippet_completion_valid_for("sel;"));
    }

    #[test]
    fn build_projects_the_library_with_label_detail_and_type() {
        let built = build_snippet_completions(&SnippetCompletionOptions::default());
        assert_eq!(built.len(), builtin_sql_snippets().len());
        let sel = built.iter().find(|p| p.item.prefix == "sel*").expect("sel* builtin");
        assert_eq!(sel.completion.label, "sel*");
        assert_eq!(sel.completion.detail, "query.editor.snippet.selectAll");
        assert_eq!(sel.completion.completion_type, "text");
        assert_eq!(sel.completion.template, sel.item.template);
        // No boost on the builder face: boost is a per-context
        // decision of the source.
        // (the field does not exist on SnippetCompletion by design)
    }

    #[test]
    fn the_translator_defaults_to_echoing_the_key_and_can_be_overridden() {
        let echo = build_snippet_completions(&SnippetCompletionOptions::default());
        assert_eq!(echo[0].completion.detail, echo[0].item.description_key);

        let shout = |key: &str| format!("[{key}]");
        let translated =
            build_snippet_completions(&SnippetCompletionOptions { translate: Some(&shout), ..Default::default() });
        assert_eq!(translated[0].completion.detail, format!("[{}]", echo[0].item.description_key));
    }

    mod user_custom_snippets {
        use super::*;

        fn custom_library() -> Vec<SqlSnippetItem> {
            let mut all = builtin_sql_snippets().to_vec();
            all.push(SqlSnippetItem {
                id: "custom-foo".to_owned(),
                prefix: "custom_foo".to_owned(),
                description_key: "Custom foo template".to_owned(),
                template: "SELECT foo FROM ${1:tbl};${2}".to_owned(),
            });
            all
        }

        #[test]
        fn includes_user_custom_snippets_when_provided_in_options() {
            let library = custom_library();
            let completions = build_snippet_completions(&SnippetCompletionOptions {
                snippets: Some(&library),
                ..Default::default()
            });
            assert!(completions.iter().any(|c| c.item.prefix == "custom_foo"));
        }

        #[test]
        fn surfaces_custom_snippets_in_the_source() {
            let library = custom_library();
            let doc = "custom_foo";
            let result = snippet_completion_source(
                &SnippetCompletionOptions { snippets: Some(&library), ..Default::default() },
                doc,
                false,
            )
            .expect("answers");
            assert!(labels(&result).contains(&"custom_foo"));
            assert_eq!(result.from, 0);
        }

        #[test]
        fn typing_a_shared_prefix_offers_both_the_builtin_and_the_custom() {
            let mut library = builtin_sql_snippets().to_vec();
            library.push(SqlSnippetItem {
                id: "custom-insert".to_owned(),
                prefix: "inss".to_owned(),
                description_key: "Custom insert into audit".to_owned(),
                template: "INSERT INTO audit_log (${1:action}, ${2:actor})\nVALUES (${3:val1}, ${4:val2});${5}".to_owned(),
            });
            let result = snippet_completion_source(
                &SnippetCompletionOptions { snippets: Some(&library), ..Default::default() },
                "ins",
                false,
            )
            .expect("answers");
            let labels = labels(&result);
            assert!(labels.contains(&"ins"));
            assert!(labels.contains(&"inss"));
        }
    }
}
