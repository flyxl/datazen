//! Stock Query Primary+D: first select word(s), then add one literal hit
//! while preserving the main range. This is not Find or Select All.
use super::{
    literal_cursor::{LiteralDocument, LiteralQuery},
    model::{Sel, Selection, SqlEditor},
    whole_word::WordAt,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SelectionAdvance {
    ExpandedWords,
    Added(String),
}

pub(super) fn secondary_caret_key(at: usize) -> String {
    format!("sql-secondary-caret-{at}")
}

/// CM EditorSelection.create: stable sort by start, merging overlaps
/// (but not adjacent nonempty ranges), with the later range's direction.
fn normalized(mut ranges: Vec<(Sel, bool)>) -> Option<Selection> {
    ranges.sort_by_key(|(sel, _)| sel.from());
    let mut ordered: Vec<(Sel, bool)> = Vec::with_capacity(ranges.len());
    for (sel, main) in ranges {
        if let Some((prior, old_main)) = ordered.last_mut() {
            if sel.from() < prior.to() || (sel.from() == sel.to() && sel.from() == prior.to()) {
                let (lo, hi) = (prior.from(), prior.to().max(sel.to()));
                *prior = if sel.anchor > sel.head {
                    Sel { anchor: hi, head: lo }
                } else {
                    Sel { anchor: lo, head: hi }
                };
                *old_main |= main;
                continue;
            }
        }
        ordered.push((sel, main));
    }
    let main = ordered.iter().position(|(_, main)| *main)?;
    Some(Selection { ranges: ordered.into_iter().map(|(sel, _)| sel).collect(), main })
}

impl SqlEditor {
    pub(crate) fn select_next_query_occurrence(&mut self) -> Option<SelectionAdvance> {
        if self.composition().is_some() {
            return None;
        }

        let doc = LiteralDocument::new(self.doc(), true);
        let mut input = Vec::with_capacity(self.selection.len());
        for (index, range) in self.selection.ranges().iter().enumerate() {
            // Fail atomically on any edge splitting a Rust scalar or a
            // projected CRLF; never silently move it to a different word.
            let anchor = doc.occurrence_offset(range.anchor)?;
            let head = doc.occurrence_offset(range.head)?;
            input.push((Sel { anchor, head }, index == self.selection.main_index()));
        }
        let current = normalized(input)?;
        let mut words = WordAt::default();

        let (selection, added_end) = if current.ranges().iter().any(|r| r.anchor == r.head) {
            // If *any* range is empty, the first command selects all their
            // head words (including the heads of already expanded ranges).
            let expanded = current.ranges().iter().enumerate().map(|(index, range)| {
                let selected = doc.word_at(&mut words, range.head).map_or_else(
                    || Sel::caret(range.head),
                    |word| Sel { anchor: word.start, head: word.end },
                );
                (selected, index == current.main_index())
            }).collect();
            let selection = normalized(expanded)?;
            if selection == current { return None; }
            (selection, None)
        } else {
            // Equality is exact after CodeMirror's LF document projection,
            // *before* NFKD search normalization. Do not use Find flags.
            let first = current.ranges().first()?;
            let text = doc.selected_units(first.from()..first.to())?;
            for range in current.ranges().iter().skip(1) {
                if doc.selected_units(range.from()..range.to())? != text {
                    return None;
                }
            }
            let query = LiteralQuery::new(&String::from_utf16(text).ok()?, true, false)?;
            let main = current.main();
            let last = current.ranges().last()?;
            let hit = query.next_occurrence(
                &doc, &mut words, main.from()..main.to(), main.head, last.from()..last.to(),
                |start| current.ranges().binary_search_by_key(&start, Sel::from).is_ok(),
            )?;
            // Check the hit itself, even if normalizing overlaps could hide
            // a JS UTF-16 position that Native cannot represent.
            let reveal = doc.bytes(hit.clone()).ok()?.end;
            let added = (Sel { anchor: hit.start, head: hit.end }, false);
            let selection = normalized(std::iter::once(added).chain(
                current.ranges().iter().copied().enumerate()
                    .map(|(index, range)| (range, index == current.main_index())),
            ).collect())?;
            (selection, Some(reveal))
        };

        let mut ranges = Vec::with_capacity(selection.len());
        for range in selection.ranges() {
            let anchor = doc.bytes(range.anchor..range.anchor).ok()?.start;
            let head = doc.bytes(range.head..range.head).ok()?.start;
            ranges.push(Sel { anchor, head });
        }
        let next = Selection { ranges, main: selection.main_index() };
        let advance = added_end.map_or(SelectionAdvance::ExpandedWords, |end| {
            let target = if next.main().head == end {
                "sql-caret".to_owned()
            } else if next.ranges().iter().any(|range| range.head == end) {
                secondary_caret_key(end)
            } else {
                // An overlapping wrapped hit may merge away its independent
                // endpoint. Reveal that logical row without changing main.
                let row = self.doc.as_bytes()[..end].iter().filter(|&&b| b == b'\n').count();
                format!("sql-line-{row}")
            };
            SelectionAdvance::Added(target)
        });
        self.preferred_columns.clear();
        self.completion_close();
        self.selection = next;
        self.keep_bracket_closers_on_main_line();
        Some(advance)
    }
}

#[cfg(test)]
mod tests;
