//! The Query editor's CodeMirror Mod+/ command, not the separate right-click
//! action (which replaces and reselects the main line range). All mapped
//! CodeMirror SQL dialects supply `--`; the space also keeps MySQL syntax valid.

use super::{model::boundary, Sel, Selection, SqlEditor};

const TOKEN: &str = "--";
const PREFIX: &str = "-- ";

#[derive(Clone, Copy)]
struct LogicalLine {
    from: usize,
    to: usize,
}

#[derive(Clone, Copy)]
struct LineTarget {
    line: LogicalLine,
    indent: usize,
    comment_at: Option<usize>,
    blank: bool,
    single: bool,
}

#[derive(Clone, Copy)]
struct Edit {
    from: usize,
    to: usize,
    insert: &'static str,
}

impl SqlEditor {
    pub(crate) fn toggle_query_line_comments(&mut self) -> bool {
        if self.composition().is_some() {
            return false;
        }
        let (edits, adding) = plan(&self.doc, self.selection());
        if edits.is_empty() {
            return false;
        }
        let selection = map_selection(self.selection(), &edits, adding);
        self.push_history();
        // One undo record and one original coordinate system for the batch.
        for edit in edits.iter().rev() {
            self.splice_doc(edit.from, edit.to, edit.insert);
        }
        self.selection = selection;
        self.keep_bracket_closers_on_main_line();
        self.preferred_columns.clear();
        self.completion_close();
        true
    }
}

fn logical_lines(doc: &str) -> Vec<LogicalLine> {
    let mut lines = Vec::new();
    let mut from = 0;
    let mut bytes = doc.bytes().enumerate().peekable();
    while let Some((to, byte)) = bytes.next() {
        if !matches!(byte, b'\r' | b'\n') {
            continue;
        }
        lines.push(LogicalLine { from, to });
        from = to + 1;
        if byte == b'\r' && bytes.peek().is_some_and(|(_, next)| *next == b'\n') {
            bytes.next();
            from += 1;
        }
    }
    lines.push(LogicalLine { from, to: doc.len() });
    lines
}

// CodeMirror tests JavaScript's /\s/ for indentation (all matches are BMP).
// Converting the measured character count back to a byte boundary avoids
// slicing an ideographic space or other non-ASCII indentation in half.
fn indent_char(value: char) -> bool {
    matches!(value, '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn indent_byte_at(line: &str, columns: usize) -> usize {
    line.char_indices().nth(columns).map_or(line.len(), |(at, _)| at)
}

fn plan(doc: &str, selection: &Selection) -> (Vec<Edit>, bool) {
    let lines = logical_lines(doc);
    let mut spans = selection.ranges().iter()
        .map(|range| (boundary(doc, range.from()), boundary(doc, range.to())))
        .collect::<Vec<_>>();
    spans.sort_by_key(|&(from, _)| from);
    let mut targets: Vec<LineTarget> = Vec::new();

    for (from, to) in spans {
        let first = lines.partition_point(|line| line.from <= from).saturating_sub(1);
        let after = if from == to { first + 1 } else { lines.partition_point(|line| line.from < to) };
        let own_targets_start = targets.len();
        let mut least_indent = usize::MAX;
        for &line in &lines[first..after] {
            if targets.last().is_some_and(|previous| previous.line.from >= line.from) {
                continue;
            }
            let text = &doc[line.from..line.to];
            let indent = text.chars().take_while(|ch| indent_char(*ch)).count();
            let indent_byte = indent_byte_at(text, indent);
            let blank = indent_byte == text.len();
            if !blank {
                least_indent = least_indent.min(indent);
            }
            let comment_at = text[indent_byte..].starts_with(TOKEN).then_some(line.from + indent_byte);
            targets.push(LineTarget { line, indent, comment_at, blank, single: false });
        }
        let single = targets.len() == own_targets_start + 1;
        for target in &mut targets[own_targets_start..] {
            target.single = single;
            if !target.blank {
                target.indent = least_indent;
            }
        }
    }

    let adding = targets.iter().any(|line| line.comment_at.is_none() && (!line.blank || line.single));
    let edits = if adding {
        targets.into_iter().filter(|line| !line.blank || line.single).map(|target| {
            let text = &doc[target.line.from..target.line.to];
            let at = target.line.from + indent_byte_at(text, target.indent);
            Edit { from: at, to: at, insert: PREFIX }
        }).collect()
    } else {
        targets.into_iter().filter_map(|target| {
            let from = target.comment_at?;
            let end = from + TOKEN.len();
            let to = end + usize::from(doc.as_bytes().get(end) == Some(&b' '));
            Some(Edit { from, to, insert: "" })
        }).collect()
    };
    (edits, adding)
}

fn map_position(position: usize, after_insert: bool, edits: &[Edit]) -> usize {
    let mut delta = 0isize;
    for edit in edits {
        if position < edit.from {
            break;
        }
        let start = edit.from.saturating_add_signed(delta);
        if edit.from == edit.to && position == edit.from {
            return start + if after_insert { edit.insert.len() } else { 0 };
        }
        if edit.from != edit.to && position <= edit.to {
            return start;
        }
        delta += edit.insert.len() as isize - (edit.to - edit.from) as isize;
    }
    position.saturating_add_signed(delta)
}

fn map_selection(selection: &Selection, edits: &[Edit], adding: bool) -> Selection {
    let mut mapped = selection.ranges().iter().enumerate().map(|(at, range)| {
        let from = map_position(range.from(), adding, edits);
        let to = map_position(range.to(), false, edits);
        // Existing non-empty endpoints associate inward. For deletion the
        // association does not change the location, so only carets need the
        // +1 association of the comment-insert transaction.
        let mapped = if range.from() == range.to() {
            Sel::caret(from)
        } else if range.anchor > range.head {
            Sel { anchor: to, head: from }
        } else {
            Sel { anchor: from, head: to }
        };
        (at == selection.main_index(), mapped)
    }).collect::<Vec<_>>();
    mapped.sort_by_key(|(_, range)| range.from());
    let mut joined: Vec<(bool, Sel)> = Vec::new();
    for (main, next) in mapped {
        if let Some((main_seen, last)) = joined.last_mut() {
            let overlaps = next.from() < last.to()
                || (next.anchor == next.head && next.from() == last.to());
            if overlaps {
                let from = last.from();
                let to = next.to().max(last.to());
                *last = if next.anchor > next.head { Sel { anchor: to, head: from } }
                    else { Sel { anchor: from, head: to } };
                *main_seen |= main;
                continue;
            }
        }
        joined.push((main, next));
    }
    let main = joined.iter().position(|(is_main, _)| *is_main).unwrap_or(0);
    Selection { main, ranges: joined.into_iter().map(|(_, range)| range).collect() }
}

#[cfg(test)]
mod tests;
