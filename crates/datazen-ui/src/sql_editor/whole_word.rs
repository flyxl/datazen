//! Literal CM6 word edges on the cursor's actual UTF-16 line buffer.
//! The pinned SQL dialect has no extra wordChars. Its cluster edge behavior
//! is based on @marijn/find-cluster-break 1.0.4 (MIT, notice in testdata),
//! not generic UAX #29 and not ECMAScript's \b assertion.
use std::ops::Range;

use super::whole_word_data::{EXTEND, WORD};

const LF: u16 = b'\n' as u16;
const ZWJ: u32 = 0x200d;

fn has(table: &[(u32, u32)], code: u32) -> bool {
    let i = table.partition_point(|&(_, end)| end <= code);
    table.get(i).is_some_and(|&(start, _)| start <= code)
}

fn high(u: u16) -> bool { (0xd800..0xdc00).contains(&u) }
fn low(u: u16) -> bool { (0xdc00..0xe000).contains(&u) }

fn scalar(input: &[u16], at: usize) -> (u32, usize) {
    let first = input[at];
    if let Some(&second) = input.get(at+1).filter(|&&u| high(first) && low(u)) {
        (0x1_0000 + ((u32::from(first)-0xd800) << 10) + u32::from(second)-0xdc00, 2)
    } else {
        (u32::from(first), 1)
    }
}

fn flag(code: u32) -> bool { (0x1f1e6..=0x1f1ff).contains(&code) }

struct ClusterWindow {
    extent: Range<usize>,
    /// Relative, strictly ordered boundaries, including 0 and window end.
    breaks: Vec<usize>,
    words: Vec<Range<usize>>,
}

impl ClusterWindow {
    fn build(input: &[u16], extent: Range<usize>) -> Self {
        let chunk = &input[extent.clone()];
        let (mut at, mut prior, mut odd_flags) = (0, 0, false);
        let mut breaks = vec![0];
        let mut words = Vec::new();
        while at < chunk.len() {
            let (cp, width) = scalar(chunk, at);
            // The pinned algorithm consults the preceding run of flags
            // even when a cursor starts inside it. Any consumed flag is
            // non-ZWJ, so retaining the previous non-ZWJ after pairing
            // has the same effect as advancing it to that flag.
            if at > 0 && prior != ZWJ && cp != ZWJ && !has(EXTEND, cp)
                && !(flag(cp) && odd_flags)
            {
                breaks.push(at);
            }
            if has(WORD, cp) { words.push(at..at+width); }
            odd_flags = flag(cp) && (!flag(prior) || !odd_flags);
            prior = cp;
            at += width;
        }
        if at > 0 { breaks.push(at); }
        Self { extent, breaks, words }
    }

    fn has_word(&self, a: usize, b: usize) -> bool {
        let i = self.words.partition_point(|c| c.start < a);
        self.words.get(i).is_some_and(|c| c.end <= b)
    }

    fn connected(&self, edge: usize) -> bool {
        let middle = self.breaks.partition_point(|&at| at < edge);
        let before = middle.checked_sub(1).map_or(0, |i| self.breaks[i]);
        let after = self.breaks.get(self.breaks.partition_point(|&at| at <= edge))
            .copied().unwrap_or(edge);
        self.has_word(before, edge) && self.has_word(edge, after)
    }
}

#[derive(Default)]
pub(super) struct Filter {
    line: Option<ClusterWindow>,
}

/// A CM Text.iterRange exposes one partial logical line or one literal LF.
/// It only adds up to two units of full-document context for a match spanning
/// StringCursor's current buffer. At a clipped scan edge, the neighbor can
/// legitimately be absent even if it exists in the underlying document.
impl Filter {
    pub(super) fn accepts(
        &mut self, input: &[u16], newlines: &[usize], scan: Range<usize>,
        current: usize, matched: Range<usize>,
    ) -> bool {
        let buffer = if input[current] == LF {
            current..current+1
        } else {
            let n = newlines.partition_point(|&newline| newline < current);
            let start = n.checked_sub(1).map_or(0, |p| newlines[p]+1);
            let end = newlines.get(n).copied().unwrap_or(input.len());
            start.max(scan.start)..end.min(scan.end)
        };
        let temporary;
        let context = if buffer.start > matched.start || buffer.end < matched.end {
            // CodeMirror's multiline fallback uses document context rather
            // than the clipped cursor, including only two units per side.
            let surround = matched.start.saturating_sub(2)..input.len().min(matched.end.saturating_add(2));
            temporary = ClusterWindow::build(input, surround);
            &temporary
        } else {
            if self.line.as_ref().is_none_or(|line| line.extent != buffer) {
                self.line = Some(ClusterWindow::build(input, buffer));
            }
            self.line.as_ref().expect("the clipped cursor line was built")
        };
        let lo = matched.start-context.extent.start;
        let hi = matched.end-context.extent.start;
        !context.connected(lo) && !context.connected(hi)
    }
}
