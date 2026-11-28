//! Query's directly typed asymmetric SQL brackets: (), [] and {}.
//! Quotes are separate and need the live SQL syntax context.
use super::{model::boundary, Sel, Selection, SqlEditor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BracketCloser {
    id: u64,
    byte: usize,
    closer: u8,
}

#[derive(Debug, Default)]
pub(super) struct BracketClosers {
    next_id: u64,
    marks: Vec<BracketCloser>,
}

impl BracketClosers {
    fn add(&mut self, byte: usize, closer: u8) {
        self.next_id = self.next_id.wrapping_add(1);
        self.marks.push(BracketCloser { id: self.next_id, byte, closer });
    }

    pub(super) fn snapshot(&self) -> Vec<BracketCloser> {
        self.marks.clone()
    }

    /// History may move surviving marks, never certify newly redone text or
    /// resurrect a mark that a later edit removed.
    pub(super) fn restore_survivors(&mut self, mut before: Vec<BracketCloser>) {
        before.retain(|old| self.marks.iter().any(|now| now.id == old.id));
        self.marks = before;
    }

    pub(super) fn map_edit(&mut self, from: usize, to: usize, inserted: usize) {
        self.marks.retain_mut(|mark| {
            if (from..to).contains(&mark.byte) {
                return false;
            }
            if mark.byte >= to {
                mark.byte = mark.byte - (to - from) + inserted;
            }
            true
        });
    }

    pub(super) fn map_batch(&mut self, mut map: impl FnMut(usize) -> Option<usize>) {
        self.marks.retain_mut(|mark| match map(mark.byte) {
            Some(at) => { mark.byte = at; true }
            None => false,
        });
    }

    fn contains(&self, byte: usize, closer: u8) -> bool {
        self.marks.iter().any(|mark| mark.byte == byte && mark.closer == closer)
    }
}

/// Exact CodeMirror SQL asymmetric opener lookahead. The language has no
/// SQL-specific override of the default close-before list; comma is not one.
fn can_pair_here(doc: &str, at: usize) -> bool {
    doc[at..].chars().next().is_none_or(|ch| {
        matches!(ch, ')' | ']' | '}' | ':' | ';' | '>'
            | '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}'
            | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
            | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

impl SqlEditor {
    /// CodeMirror drops old markers away from the main selection's line on
    /// selection changes. Generated markers for a fresh multi-range edit
    /// are registered after this step, so all newly wrapped ranges work.
    pub(super) fn keep_bracket_closers_on_main_line(&mut self) {
        if self.bracket_closers.marks.is_empty() {
            return;
        }
        let at = boundary(&self.doc, self.selection.main().head);
        let from = self.doc[..at].rfind(['\r', '\n']).map_or(0, |br| br + 1);
        let to = self.doc[at..].find(['\r', '\n']).map_or(self.doc.len(), |br| at + br);
        self.bracket_closers.marks.retain(|mark| {
            (from..to).contains(&mark.byte)
                && self.doc.as_bytes().get(mark.byte) == Some(&mark.closer)
        });
    }

    /// False delegates to the unchanged native text path. This is used only
    /// for a single input key in the focused Query canvas, not paste/IME.
    pub(crate) fn type_query_bracket(&mut self, key: &str) -> bool {
        if self.composition().is_some() {
            return false;
        }
        // These are the three asymmetric, one-byte pairs in the React SQL
        // dialect. Context-sensitive quotes/backticks deliberately fall through.
        let (open, close, pair) = match key {
            "(" => ("(", ")", "()"),
            "[" => ("[", "]", "[]"),
            "{" => ("{", "}", "{}"),
            ")" => return self.step_over_bracket_closers(b')'),
            "]" => return self.step_over_bracket_closers(b']'),
            "}" => return self.step_over_bracket_closers(b'}'),
            _ => return false,
        };
        self.open_bracket_pair(open, close, pair)
    }

    fn open_bracket_pair(&mut self, open: &str, close: &str, pair: &str) -> bool {
        let main = self.selection.main_index();
        let mut ranges = self.selection.ranges().iter().copied().enumerate().collect::<Vec<_>>();
        ranges.sort_by_key(|(_, range)| (range.from(), range.to()));
        if ranges.is_empty() || ranges.iter().any(|(_, range)| {
            range.to() > self.doc.len()
                || !self.doc.is_char_boundary(range.from()) || !self.doc.is_char_boundary(range.to())
                || (range.from() == range.to() && !can_pair_here(&self.doc, range.from()))
        }) || ranges.windows(2).any(|pair| {
            let (left, right) = (pair[0].1, pair[1].1);
            left.to() > right.from() || (left.from() == right.from() && left.to() == right.to())
        }) {
            return false;
        }

        // Construct ranges in original coordinates, then apply non-overlapping
        // insertions back-to-front. We do not replace the selected text, so
        // any generated marks inside a wrapped selection still map correctly.
        let mut selected = self.selection.clone();
        let mut fresh_closers = Vec::with_capacity(ranges.len());
        for (order, (index, range)) in ranges.iter().copied().enumerate() {
            let shift = 2 * order + 1;
            selected.ranges[index] = Sel { anchor: range.anchor + shift, head: range.head + shift };
            fresh_closers.push(range.to() + shift);
        }
        selected.main = main;
        self.push_history();
        for (_, range) in ranges.into_iter().rev() {
            let (from, to) = (range.from(), range.to());
            if from == to {
                self.splice_doc(from, to, pair);
            } else {
                self.splice_doc(to, to, close);
                self.splice_doc(from, from, open);
            }
        }
        self.selection = selected;
        self.preferred_columns.clear();
        self.completion_close();
        self.keep_bracket_closers_on_main_line();
        for byte in fresh_closers {
            self.bracket_closers.add(byte, close.as_bytes()[0]);
        }
        true
    }

    fn step_over_bracket_closers(&mut self, closer: u8) -> bool {
        let ranges = self.selection.ranges();
        // More conservative than CodeMirror for mixed provenance across
        // multiple carets: not even another caret may skip a literal closer.
        if ranges.is_empty() || ranges.iter().any(|range| {
            range.anchor != range.head
                || self.doc.as_bytes().get(range.head) != Some(&closer)
                || !self.bracket_closers.contains(range.head, closer)
        }) {
            return false;
        }
        let old = self.selection.clone();
        self.bracket_closers.marks.retain(|mark| {
            mark.closer != closer || !old.ranges.iter().any(|range| range.head == mark.byte)
        });
        self.selection = Selection {
            main: old.main,
            ranges: old.ranges.into_iter().map(|range| Sel::caret(range.head + 1)).collect(),
        };
        // A caret move only: no new text or undo event for a skipped closer.
        self.preferred_columns.clear();
        self.completion_close();
        self.keep_bracket_closers_on_main_line();
        true
    }
}

#[cfg(test)]
mod tests;
