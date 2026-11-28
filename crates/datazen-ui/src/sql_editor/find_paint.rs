//! Passive literal marks in the mounted Native Query viewport. The search
//! cursor matches CM6's highlight rules for the supplied window; the window
//! is Rune's laid-out, unwrapped rows, not CM6's virtualized soft-wrap DOM.
use std::{collections::HashSet, ops::Range};

use rune::{Color, TextHighlight, memo};
use crate::query_editor_tabs::QueryEditorViewport;

use super::{
    literal_cursor::{LiteralDocument, LiteralQuery},
    model::SqlEditor,
    plain_find::decode_input,
};

const DOCUMENT_LIMIT: usize = 128 * 1024;
const QUERY_LIMIT: usize = 1024;
const VISIBLE_MARK_LIMIT: usize = 8192;

struct Hit {
    range: Range<usize>,
    fill: Color,
}

pub(super) struct FindPaint(Vec<Hit>);

impl FindPaint {
    pub(super) fn mounted(
        editor: &SqlEditor, viewport: Option<&QueryEditorViewport>, lines: &[(usize, &str)],
        row_height: f32, padding: f32, popup: Option<(f32, f32)>, dark: bool,
    ) -> Option<Self> {
        let viewport = viewport?;
        if !within_budget(editor) {
            return None;
        }
        // Window-relative reads are tracked by Rune for scroll and resize.
        // Only changed row bounds invalidate this read, not every pixel of a
        // scroll. Use the current tab's actual mounted document anchor.
        let visible = memo::window_layout_with("sql-editor-scroll", |r| (r[1], r[3]))?;
        let count = lines.len();
        let rows = memo::window_layout_with(&viewport.anchor, move |anchor| {
            visible_rows(visible, anchor[1], count, row_height, padding, popup)
        })??;
        let (first, _) = *lines.get(rows.start)?;
        let (last, text) = *lines.get(rows.end.checked_sub(1)?)?;
        let last_byte = last + text.strip_suffix('\r').unwrap_or(text).len();
        Self::for_window(editor, first..last_byte, dark)
    }

    fn for_window(editor: &SqlEditor, window: Range<usize>, dark: bool) -> Option<Self> {
        if !can_paint(editor) {
            return None;
        }
        let raw = decode_input(&editor.find_input.text);
        let query = LiteralQuery::new(&raw, editor.find_match_case, editor.find_whole_word)?;
        // CM Text normalizes LF/CRLF. The Native row model does not support
        // standalone CR as a laid-out line break, so only project real CRLF.
        let doc = LiteralDocument::new(editor.doc(), true);
        let visible = query.visible_matches(&doc, window, VISIBLE_MARK_LIMIT)?;
        let selected: HashSet<_> = editor.selection().ranges().iter()
            .map(|sel| sel.from()..sel.to()).collect();
        let (ordinary, current) = if dark {
            ("#00ffff8a", "#ff00ff8a")
        } else {
            ("#ffff0054", "#ff6a0054")
        };
        let ordinary = rune::color::hex(ordinary)?;
        let current = rune::color::hex(current)?;
        Some(Self(visible.into_iter().map(|range| {
            let fill = if selected.contains(&range) { current } else { ordinary };
            Hit { range, fill }
        }).collect()))
    }

    pub(super) fn fragment(&self, fragment: Range<usize>) -> impl Iterator<Item = TextHighlight> + '_ {
        let Range { start, end } = fragment;
        let first = self.0.partition_point(|hit| hit.range.end <= start);
        self.0[first..].iter().take_while(move |hit| hit.range.start < end)
            .filter_map(move |hit| {
                let from = hit.range.start.max(start);
                let to = hit.range.end.min(end);
                (from < to).then(|| TextHighlight::new(
                    from - start..to - start, hit.fill,
                ).pad(0.0).key(format!("sql-find-hit-{}-{}-{from}", hit.range.start, hit.range.end)))
            })
    }
}

fn within_budget(editor: &SqlEditor) -> bool {
    // Go to Line has presentation priority over Find. Do not paint hidden
    // Find state during a transition between the two scoped panels.
    !editor.find_regex && editor.find_open && editor.goto_lease.is_none() && !editor.find_input.text.is_empty()
        && editor.find_input.text.len() <= QUERY_LIMIT && editor.doc().len() <= DOCUMENT_LIMIT
}

fn can_paint(editor: &SqlEditor) -> bool {
    let bytes = editor.doc().as_bytes();
    within_budget(editor) && bytes.iter().enumerate().all(|(i, &b)| {
        b != b'\r' || bytes.get(i + 1) == Some(&b'\n')
    })
}

/// Undo just the completion popup's extra flow height to identify logical
/// rows in the measured scroll viewport; no guessed vertical offsets.
fn visible_rows(
    viewport: (f32, f32), anchor_y: f32, count: usize,
    row_height: f32, padding: f32, popup: Option<(f32, f32)>,
) -> Option<Range<usize>> {
    let project = |y: f32| {
        let logical = y - anchor_y - padding;
        let gap = popup.map_or(0.0, |(after, height)| (logical - after).clamp(0.0, height));
        logical - gap
    };
    let (top, height) = viewport;
    let (start, end) = (project(top), project(top + height));
    if row_height <= 0.0 || !start.is_finite() || !end.is_finite() || end <= start {
        return None;
    }
    let from = ((start / row_height).floor() as usize).min(count);
    let to = ((end / row_height).ceil() as usize).min(count);
    (from < to).then_some(from..to)
}

#[cfg(test)]
#[path = "find_paint_tests.rs"]
mod tests;
