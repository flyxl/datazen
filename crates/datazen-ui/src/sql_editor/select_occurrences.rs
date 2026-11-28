//! CodeMirror's editor-only Select Selection Matches. This is not the Find
//! dock's All button: it uses the current text and a different match limit.
use std::{borrow::Cow, ops::Range};

use super::{
    literal_cursor::{LiteralDocument, LiteralQuery},
    model::{Sel, Selection, SqlEditor},
};

impl SqlEditor {
    pub(crate) fn select_current_occurrences(&mut self) -> bool {
        let [main] = self.selection.ranges() else { return false };
        if self.composition().is_some() || main.from() == main.to() {
            return false;
        }
        let from = main.from();
        let to = main.to();
        let Some(needle) = self.doc().get(from..to) else { return false };
        let doc = LiteralDocument::new(self.doc(), true);
        // CodeMirror's Text projects CRLF to LF. Reject an existing selection
        // that splits the projected line terminator; preserve the source bytes.
        if doc.offset(from).is_err() || doc.offset(to).is_err() {
            return false;
        }
        let needle = if needle.contains('\r') {
            Cow::Owned(needle.replace("\r\n", "\n").replace('\r', "\n"))
        } else {
            Cow::Borrowed(needle)
        };
        // Use the selected bytes themselves with case-sensitive NFKD matching
        // and no word filter or Find input unescaping. The pinned command
        // checks "ranges.length > 1000" BEFORE pushing a match.
        const LIMIT: usize = 1_001;
        let Some(query) = LiteralQuery::new(&needle, true, false) else { return false };
        let Ok(Some(ranges)) = query.all_up_to(&doc, LIMIT) else { return false };
        if ranges.is_empty() || ranges.iter().any(Range::is_empty)
            || ranges.windows(2).any(|pair| pair[0].end > pair[1].start)
        {
            return false;
        }

        // All ranges are forward, and the original starting position identifies
        // the main one unless an earlier non-overlapping match suppresses it.
        let main = ranges.iter().position(|range| range.start == from).unwrap_or(0);
        self.preferred_columns.clear();
        self.completion_close();
        self.selection = Selection {
            main,
            ranges: ranges.into_iter().map(|r| Sel { anchor: r.start, head: r.end }).collect(),
        };
        self.keep_bracket_closers_on_main_line();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_occurrences_keep_the_original_start_as_forward_main_without_history() {
        let text = "cat CAT cat xcat";
        let mut editor = SqlEditor::from_doc(text);
        // The Find options/escapes are unrelated to this command.
        editor.find_whole_word = true;
        editor.find_match_case = false;
        editor.set_main_selection(11, 8);
        assert!(editor.select_current_occurrences());
        let selected = editor.selection().clone();
        assert_eq!(selected.main_index(), 1);
        assert_eq!(selected.ranges(), [
            Sel { anchor: 0, head: 3 }, Sel { anchor: 8, head: 11 }, Sel { anchor: 13, head: 16 },
        ]);
        assert!(!editor.undo());
        assert_eq!(editor.doc(), text);
        assert_eq!(editor.selected_text(), "cat\ncat\ncat");

        assert!(!editor.select_current_occurrences(), "an existing multi-range selection stays put");
        assert_eq!(editor.selection(), &selected);
        editor.insert("z");
        assert_eq!(editor.doc(), "z CAT z xz");
        assert!(editor.undo());
        assert_eq!(editor.doc(), text);
        assert_eq!(editor.selection(), &selected);
        assert!(!editor.undo());
    }

    #[test]
    fn current_occurrences_are_nonoverlapping_nfkd_literal_and_preserve_crlf_bytes() {
        for (text, from, to, expected) in [
            ("banana", 3, 6, vec![1..4]),
            ("É E\u{0301} é e\u{0301}", 0, 2, vec![0..2, 3..6]),
            ("x\\n y\nz\\n", 1, 3, vec![1..3, 7..9]),
        ] {
            let mut editor = SqlEditor::from_doc(text);
            editor.set_main_selection(to, from);
            assert!(editor.select_current_occurrences(), "{text:?}");
            let actual = editor.selection().ranges().iter().map(|r| r.from()..r.to()).collect::<Vec<_>>();
            assert_eq!(actual, expected);
            assert_eq!(editor.doc(), text);
        }

        let mut multiline = SqlEditor::from_doc("hi\r\nthere hi\r\nthere");
        multiline.set_main_selection(0, 9);
        assert!(multiline.select_current_occurrences());
        assert_eq!(multiline.selection().ranges(), [Sel { anchor: 0, head: 9 }, Sel { anchor: 10, head: 19 }]);
        assert_eq!(multiline.doc(), "hi\r\nthere hi\r\nthere");
    }

    #[test]
    fn current_occurrences_refuse_over_cap_empty_ime_and_bad_unicode_atomically() {
        for count in [1_001, 1_002] {
            let text = "x".repeat(count);
            let mut editor = SqlEditor::from_doc(&text);
            editor.set_main_selection(count, count - 1);
            let before = editor.selection().clone();
            if count == 1_001 {
                assert!(editor.select_current_occurrences());
                assert_eq!(editor.selection().len(), 1_001);
                assert_eq!(editor.selection().main_index(), 1_000);
            } else {
                assert!(!editor.select_current_occurrences());
                assert_eq!(editor.selection(), &before);
            }
            assert_eq!(editor.doc(), text);
            assert!(!editor.undo());
        }

        let mut editor = SqlEditor::from_doc("cat cat");
        assert!(!editor.select_current_occurrences());
        editor.set_main_selection(7, 4);
        editor.preedit("输入");
        let before = editor.selection().clone();
        assert!(!editor.select_current_occurrences());
        assert_eq!(editor.selection(), &before);

        for (text, from, to) in [("a\r\nb", 2, 3), ("x\u{1d15e}y \u{1d165}", 7, 11)] {
            let mut invalid = SqlEditor::from_doc(text);
            invalid.set_main_selection(to, from);
            let before = invalid.selection().clone();
            assert!(!invalid.select_current_occurrences(), "{text:?}");
            assert_eq!(invalid.selection(), &before);
            assert_eq!(invalid.doc(), text);
            assert!(!invalid.undo());
        }
    }
}
