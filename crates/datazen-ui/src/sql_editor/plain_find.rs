//! The existing exact-string Find's Replace button/field semantics.
use std::{borrow::Cow, ops::Range};

use super::model::{Sel, Selection, SqlEditor};

/// CodeMirror's plain Find input unquotes just these four single-pass
/// escapes. An unknown escape or a trailing slash stays literal.
pub(super) fn decode_input(input: &str) -> Cow<'_, str> {
    if !input.contains('\\') {
        return Cow::Borrowed(input);
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let replacement = match chars.peek() {
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('\\') => '\\',
            _ => {
                out.push(ch);
                continue;
            }
        };
        chars.next();
        out.push(replacement);
    }
    Cow::Owned(out)
}

/// The replacement transaction's text, like CodeMirror's default Text,
/// has LF line endings. This does not change any unrelated document input.
pub(super) fn decode_replacement(input: &str) -> Cow<'_, str> {
    let decoded = decode_input(input);
    if decoded.contains('\r') {
        Cow::Owned(decoded.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        decoded
    }
}

/// Search after the full selection, then wrap up to its first character.
/// All matching is still on the existing exact literal UTF-8 substring.
/// This is not a regexp, case folding, or normalization implementation.
fn next_match(doc: &str, needle: &str, from: usize, to: usize) -> Option<Range<usize>> {
    if needle.is_empty() || !doc.is_char_boundary(from) {
        return None;
    }
    let start = doc.get(to..)?.find(needle).map(|at| to + at)
        .or_else(|| doc.find(needle).filter(|at| *at <= from))?;
    let end = start + needle.len();
    ((start, end) != (from, to)).then_some(start..end)
}

fn selection_for(found: Range<usize>) -> Selection {
    Selection { ranges: vec![Sel { anchor: found.start, head: found.end }], main: 0 }
}

/// Map a result chosen in the *old* document through this replacement.
/// Match edges that touch the replaced range follow the inserted text; an
/// edge strictly inside it follows its association, not a newly found match.
fn map_selection(sel: Sel, replaced: &Range<usize>, inserted: usize) -> Sel {
    let map = |pos: usize, after: bool| if pos <= replaced.start {
        pos
    } else if pos >= replaced.end {
        pos - (replaced.end - replaced.start) + inserted
    } else if after {
        replaced.start + inserted
    } else {
        replaced.start
    };
    if sel.anchor == sel.head {
        return Sel::caret(map(sel.head, false));
    }
    let low = map(sel.from(), true);
    let high = map(sel.to(), false);
    if sel.anchor < sel.head {
        Sel { anchor: low, head: high }
    } else {
        Sel { anchor: high, head: low }
    }
}

impl SqlEditor {
    /// The live dock's Replace first selects a candidate. A second activation
    /// on the same candidate edits the document in one undoable transaction.
    /// The next selection comes from the pre-edit document, even if the
    /// replacement text itself contains the search string.
    pub(super) fn replace_next_literal(&mut self, needle: &str, replacement: &str) -> bool {
        let main = self.selection.main();
        let Some(found) = next_match(&self.doc, needle, main.from(), main.from()) else {
            return false;
        };
        if (found.start, found.end) != (main.from(), main.to()) {
            self.preferred_columns.clear();
            self.selection = selection_for(found);
            return true;
        }

        let mut after = next_match(&self.doc, needle, found.start, found.end)
            .map_or_else(|| self.selection.clone(), selection_for);
        for range in &mut after.ranges {
            *range = map_selection(*range, &found, replacement.len());
        }
        self.push_history();
        self.apply_change(found.start, found.end, replacement);
        self.selection = after;
        true
    }
}
