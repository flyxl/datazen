//! The live editor dock's Unicode-safe literal search and undoable edits.
use std::{borrow::Cow, ops::Range};

use super::{
    literal_cursor::{LiteralDocument, LiteralQuery, UnrepresentableRange},
    model::{Sel, Selection, SqlEditor},
};

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
    if low > high {
        // CM6 can assign an inverted pair of ordered bounds when Replace All
        // entirely covers an arbitrary selected fragment. Collapse to the
        // mapped head rather than selecting text neither endpoint requested.
        Sel::caret(if sel.anchor < sel.head { high } else { low })
    } else if sel.anchor < sel.head {
        Sel { anchor: low, head: high }
    } else {
        Sel { anchor: high, head: low }
    }
}

enum ReplacePlan {
    Seek(Range<usize>),
    Edit { found: Range<usize>, next: Option<Range<usize>> },
}

impl SqlEditor {
    /// Find-dock Select All Matches changes only the selection. The matches
    /// are fully checked before installing any Native byte range.
    pub(super) fn select_all_literal_matches_from_bar(&mut self) -> bool {
        if self.find_regex || self.composition().is_some() {
            return false;
        }
        const LIMIT: usize = 1_000;
        self.find_unsafe_range = false;
        let raw = decode_input(&self.find_input.text);
        let Some(query) = LiteralQuery::new(&raw, self.find_match_case, self.find_whole_word) else {
            return false;
        };
        // CodeMirror Text always normalizes document line boundaries. Reuse
        // the existing LF projection while keeping each original UTF-8
        // span, including any CRLF, unchanged in the Native buffer.
        let ranges = match query.all_up_to(&LiteralDocument::new(&self.doc, true), LIMIT) {
            Ok(Some(ranges)) if !ranges.is_empty() => ranges,
            Ok(_) => return false,
            Err(_) => {
                self.find_unsafe_range = true;
                return false;
            }
        };
        // The cursor returns ordered, non-overlapping matches. Fail closed
        // if this invariant ever changes: independent Native edit ranges
        // must not overlap. Adjacent nonempty ranges remain separate.
        if ranges.iter().any(Range::is_empty)
            || ranges.windows(2).any(|pair| pair[0].end > pair[1].start)
        {
            return false;
        }
        self.preferred_columns.clear();
        self.completion_close();
        self.selection = Selection {
            main: 0,
            ranges: ranges.into_iter().map(|range| Sel { anchor: range.start, head: range.end }).collect(),
        };
        self.keep_bracket_closers_on_main_line();
        true
    }

    fn search_plan(&self, raw: &str, forward: bool) -> Result<Option<Range<usize>>, UnrepresentableRange> {
        let Some(query) = LiteralQuery::new(raw, self.find_match_case, self.find_whole_word) else { return Ok(None) };
        let doc = LiteralDocument::new(&self.doc, self.find_whole_word);
        let main = self.selection.main();
        let from = doc.offset(main.from())?;
        let to = doc.offset(main.to())?;
        // CodeMirror's Find commands start at a collapsed directional edge.
        // Keep the ranged cursor used by Replace Next separate.
        let matched = if forward { query.next(&doc, to..to) } else { query.previous(&doc, from..from) };
        matched.map(|range| doc.bytes(range)).transpose()
    }

    pub(super) fn find_dock_literal(&mut self, raw: &str, forward: bool) -> bool {
        self.find_unsafe_range = false;
        match self.search_plan(raw, forward) {
            Ok(Some(found)) => {
                self.set_main_selection(found.start, found.end);
                true
            }
            Ok(None) => false,
            Err(_) => {
                self.find_unsafe_range = true;
                false
            }
        }
    }

    fn replace_plan(&self, raw: &str) -> Result<Option<ReplacePlan>, UnrepresentableRange> {
        let Some(query) = LiteralQuery::new(raw, self.find_match_case, self.find_whole_word) else { return Ok(None) };
        let doc = LiteralDocument::new(&self.doc, self.find_whole_word);
        let main = self.selection.main();
        let first = doc.offset(main.from())?;
        let Some(found) = query.next(&doc, first..first) else { return Ok(None) };
        let bytes = doc.bytes(found.clone())?;
        if (bytes.start, bytes.end) != (main.from(), main.to()) {
            return Ok(Some(ReplacePlan::Seek(bytes)));
        }
        // Validate the next selection BEFORE changing anything, even when
        // the currently selected match itself is representable.
        let next = query.next(&doc, found).map(|range| doc.bytes(range)).transpose()?;
        Ok(Some(ReplacePlan::Edit { found: bytes, next }))
    }

    pub(super) fn replace_next_literal(&mut self, raw: &str, replacement: &str) -> bool {
        self.find_unsafe_range = false;
        match self.replace_plan(raw) {
            Ok(Some(ReplacePlan::Seek(found))) => {
                self.set_main_selection(found.start, found.end);
                true
            }
            Ok(Some(ReplacePlan::Edit { found, next })) => {
                let mut after = next.map_or_else(|| self.selection.clone(), selection_for);
                for range in &mut after.ranges {
                    *range = map_selection(*range, &found, replacement.len());
                }
                self.push_history();
                self.apply_change(found.start, found.end, replacement);
                self.selection = after;
                self.keep_bracket_closers_on_main_line();
                true
            }
            Ok(None) => false,
            Err(_) => {
                self.find_unsafe_range = true;
                false
            }
        }
    }

    pub(super) fn replace_all_dock_literal(&mut self, raw: &str, replacement: &str) -> usize {
        self.find_unsafe_range = false;
        let Some(query) = LiteralQuery::new(raw, self.find_match_case, self.find_whole_word) else { return 0 };
        let ranges = match query.all(&LiteralDocument::new(&self.doc, self.find_whole_word)) {
            Ok(ranges) if ranges.is_empty() => return 0,
            Ok(ranges) => ranges,
            Err(_) => {
                // No partial replacement and no history on a rejected set.
                self.find_unsafe_range = true;
                return 0;
            }
        };
        self.push_history();
        let mut after = self.selection.clone();
        for found in ranges.iter().rev() {
            for selection in &mut after.ranges {
                *selection = map_selection(*selection, found, replacement.len());
            }
            self.apply_change(found.start, found.end, replacement);
        }
        self.selection = after;
        self.keep_bracket_closers_on_main_line();
        ranges.len()
    }
}
