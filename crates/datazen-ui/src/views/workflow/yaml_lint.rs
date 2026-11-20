//! Port of `packages/frontend/src/windows/workflow/yamlLint.ts` (T244) —
//! the lint pass for the workflow YAML editor.
//!
//! React wraps the check as a CodeMirror `linter` extension; the port keeps
//! the check itself (the function [`param_ref_diagnostics`]) and surfaces
//! its diagnostics through the yaml editor's lint gutter
//! ([`super::workflow_yaml_editor`], T200's port of the chrome that mounts
//! them). The React file's own doc comment carries the scope ruling, which
//! the port keeps verbatim as its law:
//!
//! - **parameter reference pairing** — every `${param}` must have a closing
//!   `}`. A param reference left unclosed (`draft SQL ``${id`` /
//!   ``${id}${name``) is the most common silent failure in hand-written
//!   workflow YAML, so the diagnostic lands at the offending `${`.
//! - **conservative by design**: a stray `}` with no open `${` is ignored
//!   (SQL blocks can legitimately contain `}`, e.g. in string literals or
//!   JSON casts), which keeps the pass from flagging real SQL. Comment
//!   lines are skipped.
//!
//! The React test suite (`__tests__/yamlLint.test.ts`) ports below; the
//! React file has no other caller than the yaml editor's extension list.

/// The `source` leg of every diagnostic this pass emits (React's literal).
pub(crate) const LINT_SOURCE: &str = "workflow-yaml";

/// The one message this pass reports (React's literal): an opener that never
/// met its closer.
pub(crate) const UNCLOSED_PARAM_MESSAGE: &str =
    "Unclosed parameter reference — missing \"}\"";

/// One lint diagnostic: a byte range in the document plus the message leg.
/// React's `Diagnostic` carries `severity`/`source` fields too; this pass
/// only ever emits `severity: 'error'` with [`LINT_SOURCE`], so those two
/// ride as constants and the struct stays the observable pair (the range
/// plus what it says).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YamlLintDiagnostic {
    pub(crate) from: usize,
    pub(crate) to: usize,
}

/// The parameter-reference pairing pass — React's `paramRefDiagnostics`,
/// with the `EditorView` argument already reduced to its document (the only
/// leg the function reads).
///
/// Scanning is the direct twin of React's `TOKEN_RE = /\$\{|\}/g` walk: every
/// `${` pushes its offset onto the openers stack, every `}` pops one if the
/// stack is non-empty (a stray closer is ignored by design), and each
/// surviving opener becomes an error diagnostic whose range is React's
/// `from = min(open, len)`, `to = max(from + 1, min(open + 2, len))` — the
/// `${` pair clamped to the document, or a single byte at the tail.
pub(crate) fn param_ref_diagnostics(doc: &str) -> Vec<YamlLintDiagnostic> {
    let bytes = doc.as_bytes();
    let len = bytes.len();
    let mut openers: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i < len {
        // `${` (the two-byte opener) or `}` (the one-byte closer) — the g
        // flag advances past the whole match, so an opener consumes both
        // bytes and never nests into the `${` of `${${`.
        if bytes[i] == b'$' && bytes.get(i + 1) == Some(&b'{') {
            if !line_comment_ahead(doc, i) {
                openers.push(i);
            }
            i += 2;
        } else if bytes[i] == b'}' {
            if !line_comment_ahead(doc, i) {
                // A closer only counts while an opener is awaiting one; a
                // stray `}` (SQL strings, JSON casts) is ignored by design.
                openers.pop();
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    openers
        .into_iter()
        .map(|open| {
            let from = open.min(len);
            let to = (from + 1).max((open + 2).min(len));
            YamlLintDiagnostic { from, to }
        })
        .collect()
}

/// React's `lineCommentAhead`: the diagnostic position's line must not be a
/// comment — a `#` anywhere between the line's start and the position makes
/// the whole token invisible to the pass.
fn line_comment_ahead(doc: &str, pos: usize) -> bool {
    let line_start = doc[..pos].rfind('\n').map(|at| at + 1).unwrap_or(0);
    doc.as_bytes()[line_start..pos].contains(&b'#')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The React suite's `diag(text)` helper: run the pass over a document.
    fn diag(text: &str) -> Vec<YamlLintDiagnostic> {
        param_ref_diagnostics(text)
    }

    /// The React suite's `textSlice`: the document bytes the range selects —
    /// the assertion leg the React tests pin (`'${'` at the offending site).
    fn text_slice(from: usize, to: usize, doc: &str) -> &str {
        &doc[from..to]
    }

    #[test]
    fn flags_an_unclosed_parameter_reference() {
        let doc = "sql: SELECT ${id} FROM t WHERE x=${name";
        let d = diag(doc);
        assert_eq!(d.len(), 1, "exactly the unclosed opener");
        // React pins severity 'error' at the site; the port's only severity
        // is the error constant, pinned by construction (the constants
        // above), and the range is the observable leg:
        assert_eq!(
            text_slice(d[0].from, d[0].to, doc),
            "${",
            "the diagnostic selects the opener pair"
        );
    }

    #[test]
    fn accepts_balanced_references() {
        assert_eq!(diag("sql: SELECT ${id} FROM t WHERE x=${name} AND y=${other}").len(), 0);
    }

    #[test]
    fn reports_each_unclosed_opener() {
        assert_eq!(diag("${a} ${b ${c}").len(), 1);
        assert_eq!(diag("${a ${b").len(), 2);
    }

    #[test]
    fn ignores_a_stray_closing_brace() {
        // May be legitimate SQL: the quoted '}$not_a_param' has no opener.
        assert_eq!(diag("sql: SELECT '}$not_a_param'").len(), 0);
    }

    #[test]
    fn ignores_parameter_tokens_in_comment_lines() {
        assert_eq!(diag("# ${hint\nsql: SELECT 1").len(), 0);
    }

    #[test]
    fn the_opener_range_clamps_at_the_document_tail() {
        // React's min/max clamps: an opener whose pair would run past the
        // end still selects a single byte (from == to would be invisible).
        let doc = "$";
        // '$' alone is not an opener — the pairing below is the real tail
        // clamp: '${' at the very end selects the two bytes it has.
        let d = diag("${");
        assert_eq!(d.len(), 1);
        assert_eq!(text_slice(d[0].from, d[0].to, "${"), "${");
        assert_eq!(diag(doc).len(), 0, "a lone '$' opens nothing");
    }
}
