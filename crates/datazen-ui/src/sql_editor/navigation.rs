//! User movement operates on Unicode graphemes; the rest of the editor keeps
//! UTF-8 byte ranges (SQL selection/execution and completion use exact ranges).
use std::ops::Range;

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use super::model::{boundary, Sel, Selection, SqlEditor};

#[derive(Clone, Copy)]
pub(super) enum Motion {
    Grapheme(bool),
    Word(bool),
    LineEdge(bool),
    DocumentEdge(bool),
    Vertical(bool),
}

fn step(doc: &str, at: usize, forward: bool) -> usize {
    let at = boundary(doc, at);
    let mut cursor = GraphemeCursor::new(at, doc.len(), true);
    let next = if forward {
        cursor.next_boundary(doc, 0)
    } else {
        cursor.prev_boundary(doc, 0)
    };
    next.ok().flatten().unwrap_or(if forward { doc.len() } else { 0 })
}

pub(super) fn grapheme_edge(doc: &str, at: usize, forward: bool) -> usize {
    let at = boundary(doc, at);
    let mut cursor = GraphemeCursor::new(at, doc.len(), true);
    if matches!(cursor.is_boundary(doc, 0), Ok(true)) { at } else { step(doc, at, forward) }
}

pub(super) fn edit_range(doc: &str, sel: Sel) -> (usize, usize) {
    let from = grapheme_edge(doc, sel.from(), false);
    let to = if sel.from() == sel.to() { from } else { grapheme_edge(doc, sel.to(), true) };
    (from, to)
}

/// Lines expose contents but not their LF/CRLF terminators. At a line break
/// the caret is still on the preceding line until it crosses the grapheme.
fn line_bounds(doc: &str, at: usize) -> Range<usize> {
    let at = boundary(doc, at);
    let start = doc[..at].rfind('\n').map_or(0, |i| i + 1);
    let next = doc[at..].find('\n').map(|i| at + i);
    let end = next.map_or(doc.len(), |i| {
        if i > start && doc.as_bytes()[i - 1] == b'\r' { i - 1 } else { i }
    });
    start..end
}

fn following_line(doc: &str, line: &Range<usize>, down: bool) -> Option<Range<usize>> {
    if down {
        doc[line.end..].find('\n').map(|i| line_bounds(doc, line.end + i + 1))
    } else {
        line.start.checked_sub(1).map(|i| line_bounds(doc, i))
    }
}

fn word_edge(doc: &str, at: usize, forward: bool) -> usize {
    let is_word = |s: &str| s.chars().any(|c| c.is_alphanumeric() || c == '_');
    let words = doc.split_word_bound_indices().filter(|(_, s)| is_word(s));
    if forward {
        words.map(|(i, s)| i + s.len()).find(|&end| end > at).unwrap_or(doc.len())
    } else {
        words.map(|(i, _)| i).take_while(|&start| start < at).last().unwrap_or(0)
    }
}

struct RangeGoal {
    sel: Sel,
    main: bool,
    column: usize,
}

/// Navigation can merge two ranges (e.g. two carets on short lines). Keep
/// the main identity and its direction; never leave duplicate edit ranges.
fn merge(mut ranges: Vec<RangeGoal>) -> (Selection, Vec<usize>) {
    ranges.sort_by_key(|range| (range.sel.from(), range.sel.to()));
    let mut merged: Vec<RangeGoal> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut().filter(|last| last.sel.to() >= range.sel.from()) {
            let from = last.sel.from();
            let to = last.sel.to().max(range.sel.to());
            let source = if range.main { &range } else { &*last };
            let reversed = source.sel.anchor > source.sel.head;
            last.column = source.column;
            last.main |= range.main;
            last.sel = if reversed { Sel { anchor: to, head: from } } else { Sel { anchor: from, head: to } };
        } else {
            merged.push(range);
        }
    }
    let main = merged.iter().position(|range| range.main).unwrap_or(0);
    let (ranges, columns) = merged.into_iter().map(|range| (range.sel, range.column)).unzip();
    (Selection { ranges, main }, columns)
}

impl SqlEditor {
    pub(super) fn move_caret(&mut self, motion: Motion, extend: bool) {
        self.completion_close();
        let doc = &self.doc;
        let vertical = matches!(motion, Motion::Vertical(_));
        let previous = std::mem::take(&mut self.preferred_columns);
        let ranges = self.selection.ranges().iter().enumerate().map(|(i, sel)| {
            let mut column = 0;
            let head = grapheme_edge(doc, sel.head, false);
            let to = match motion {
                Motion::Grapheme(right) | Motion::Word(right) if !extend && sel.from() != sel.to() =>
                    grapheme_edge(doc, if right { sel.to() } else { sel.from() }, right),
                Motion::Grapheme(right) => step(doc, sel.head, right),
                Motion::Word(right) => grapheme_edge(doc, word_edge(doc, head, right), right),
                Motion::LineEdge(end) => {
                    let line = line_bounds(doc, head);
                    if end { line.end } else { line.start }
                }
                Motion::DocumentEdge(end) => if end { doc.len() } else { 0 },
                Motion::Vertical(down) => {
                    let line = line_bounds(doc, head);
                    column = previous.get(i).copied().unwrap_or_else(|| doc[line.start..head].graphemes(true).count());
                    following_line(doc, &line, down).map_or(if down { doc.len() } else { 0 }, |next| {
                        doc[next.clone()].grapheme_indices(true).nth(column).map_or(next.end, |(offset, _)| next.start + offset)
                    })
                }
            };
            let anchor = if extend { grapheme_edge(doc, sel.anchor, sel.anchor > sel.head) } else { to };
            RangeGoal { sel: Sel { anchor, head: to }, main: i == self.selection.main_index(), column }
        }).collect();
        let (selection, columns) = merge(ranges);
        self.selection = selection;
        self.keep_bracket_closers_on_main_line();
        if vertical {
            self.preferred_columns = columns;
        }
    }

    pub(super) fn delete_grapheme(&mut self, forward: bool) {
        if self.composition.is_some() {
            return;
        }
        let doc = &self.doc;
        let ranges: Vec<_> = self.selection.ranges().iter().enumerate().map(|(i, sel)| {
            let (from, to) = if sel.from() != sel.to() {
                edit_range(doc, *sel)
            } else if forward {
                (grapheme_edge(doc, sel.head, false), step(doc, sel.head, true))
            } else {
                (step(doc, sel.head, false), grapheme_edge(doc, sel.head, true))
            };
            RangeGoal { sel: Sel { anchor: from, head: to }, main: i == self.selection.main_index(), column: 0 }
        }).collect();
        if ranges.iter().all(|range| range.sel.anchor == range.sel.head) {
            return;
        }
        self.push_history();
        self.selection = merge(ranges).0;
        self.change_by_range(|sel| (sel.from(), sel.to(), String::new()));
    }
}
