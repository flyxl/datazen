//! Engine-neutral CodeMirror Search 6.6.0 window/iteration policy.
//! No regexp compilation, matching, UI, original bytes mutation or worker.
use std::ops::Range;
use super::regex_whole_word::WordBudget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error { Bounds, Empty, Limit }

/// Both global progress and Unicode/multiline flags are fixed by the oracle.
/// The engine executes one search from lastIndex against this precise text;
/// slicing off the prefix instead would break lookbehind and line anchors.
pub(super) struct Window<'a> {
    pub(super) input: &'a str,
    pub(super) last_index_utf16: usize,
}

/// Engine coordinates are byte offsets inside Window::input; capture groups
/// may be unmatched or lie outside the total hit owing to lookaround.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Hit {
    pub(super) range: Range<usize>,
    pub(super) captures: Vec<Option<Range<usize>>>,
}

#[derive(Debug)]
struct Document<'a> {
    text: &'a str,
    units: Vec<u16>,
    unit_to_byte: Vec<Option<usize>>,
    edges: Vec<(usize, usize)>,
    lines: Vec<Range<usize>>,
}
impl<'a> Document<'a> {
    fn new(text: &'a str) -> Result<Self, Error> {
        // The host supplies the immutable LF projection; don't normalize the
        // original Rust editor here and lose byte origins.
        if text.contains('\r') || text.len() > 128 * 1024 { return Err(Error::Bounds); }
        let mut unit_to_byte = Vec::with_capacity(text.len() + 1);
        let mut edges = Vec::new();
        let mut lines = Vec::new();
        let mut start = 0;
        for (byte, ch) in text.char_indices() {
            let unit = unit_to_byte.len();
            if ch == '\n' {
                lines.push(start..unit);
                start = unit + 1;
            }
            edges.push((byte, unit));
            unit_to_byte.push(Some(byte));
            if ch.len_utf16() == 2 { unit_to_byte.push(None); }
        }
        lines.push(start..unit_to_byte.len());
        edges.push((text.len(), unit_to_byte.len()));
        unit_to_byte.push(Some(text.len()));
        Ok(Self { text, units: text.encode_utf16().collect(), unit_to_byte, edges, lines })
    }
    fn len(&self) -> usize { self.unit_to_byte.len() - 1 }
    fn byte(&self, unit: usize) -> Result<usize, Error> {
        self.unit_to_byte.get(unit).copied().flatten().ok_or(Error::Bounds)
    }
    fn unit(&self, byte: usize) -> Result<usize, Error> {
        self.edges.binary_search_by_key(&byte, |p| p.0).map(|i| self.edges[i].1)
            .map_err(|_| Error::Bounds)
    }
    fn line_at(&self, at: usize) -> usize {
        self.lines.partition_point(|line| line.start <= at) - 1
    }
    fn char_end(&self, at: usize) -> usize {
        // CM toCharEnd advances over a trailing surrogate after +1.
        if self.unit_to_byte.get(at) == Some(&None) { at + 1 } else { at }
    }
    fn chunk_end(&self, at: usize, end: usize) -> usize {
        if at >= end { end } else { self.lines[self.line_at(at)].end }
    }
}

/// Stateful *call-count* budget shared across wrap and previous windows. It
/// does not interrupt any single engine call; production isolation is distinct.
pub(super) struct Policy<'a> {
    doc: Document<'a>,
    multiline: bool,
    calls: usize,
    words: Option<WordBudget>,
}

impl<'a> Policy<'a> {
    pub(super) fn new(
        document: &'a str, pattern: &str, whole_word: bool, calls: usize,
    ) -> Result<Self, Error> {
        if pattern.is_empty() { return Err(Error::Empty); }
        Ok(Self {
            doc: Document::new(document)?, multiline: crate::protocol::multiline_pattern(pattern), calls,
            words: whole_word.then_some(WordBudget(2_000_000)),
        })
    }

    fn execute(
        &mut self, start: usize, end: usize, last: usize,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Option<(Hit, usize, usize)>, Error> {
        if last > end - start { return Ok(None); }
        self.calls = self.calls.checked_sub(1).ok_or(Error::Limit)?;
        let base = self.doc.byte(start)?;
        let stop = self.doc.byte(end)?;
        let input = &self.doc.text[base..stop];
        let Some(mut hit) = engine(Window {
            input, last_index_utf16: last,
        })? else { return Ok(None) };
        if input.get(hit.range.clone()).is_none()
            || hit.captures.iter().flatten().any(|r| input.get(r.clone()).is_none())
        {
            return Err(Error::Bounds);
        }
        // ECMAScript /u starts at the scalar containing lastIndex if a
        // multiline empty-match retry deliberately lands on its low surrogate.
        let min = if self.doc.unit_to_byte.get(start + last) == Some(&None) {
            start + last - 1
        } else { start + last };
        hit.range = base + hit.range.start..base + hit.range.end;
        let from = self.doc.unit(hit.range.start)?;
        let to = self.doc.unit(hit.range.end)?;
        if from < min { return Err(Error::Bounds); }
        for r in hit.captures.iter_mut().flatten() {
            *r = base + r.start..base + r.end;
        }
        Ok(Some((hit, from, to)))
    }

    fn word_accepts(
        &mut self, window_start: usize, window_end: usize, from: usize, to: usize,
    ) -> Result<bool, Error> {
        let Some(words) = self.words.as_mut() else { return Ok(true) };
        words.accepts(&self.doc.units[window_start..window_end], from-window_start..to-window_start)
    }

    fn visit(
        &mut self, lo: usize, hi: usize, passive: bool,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
        mut accept: impl FnMut(Hit) -> Result<bool, Error>,
    ) -> Result<(), Error> {
        if lo > hi { return Err(Error::Bounds); }
        self.doc.byte(lo)?;
        self.doc.byte(hi)?;
        let mut pos = self.doc.char_end(lo);
        if self.multiline {
            let mut chunk = self.doc.chunk_end(lo + 5000, hi);
            loop {
                let off = pos - lo;
                let mut hit = self.execute(lo, chunk, off, engine)?;
                if hit.as_ref().is_some_and(|(_,from,to)| from == to && *from == pos) {
                    hit = self.execute(lo, chunk, off + 1, engine)?;
                }
                if let Some((hit, from, to)) = hit {
                    if (chunk >= hi || to.saturating_add(10) <= chunk)
                        && self.word_accepts(lo, chunk, from, to)? {
                        // CM's line-ended speculative buffer can extend beyond
                        // hi. Preserve this only for passive marks, which cannot
                        // select or edit and are clipped again by the renderer.
                        if !passive && to > hi { return Err(Error::Bounds); }
                        if accept(hit)? { return Ok(()); }
                        pos = self.doc.char_end(to + usize::from(from == to));
                        continue;
                    }
                }
                if chunk == hi || (!passive && chunk > hi) { return Ok(()); }
                let next = self.doc.chunk_end(lo + (chunk - lo) * 2, hi);
                // When a speculative line ended beyond hi, pinned CM retries
                // in the clipped final buffer before it considers itself done.
                if next <= chunk && !(passive && chunk > hi && next == hi) {
                    return Err(Error::Bounds);
                }
                chunk = next;
            }
        }

        let mut i = self.doc.line_at(lo);
        let mut last = None;
        while let Some(line) = self.doc.lines.get(i).cloned() {
            if line.start > hi || pos > hi { break; }
            let end = line.end.min(hi);
            let Some((hit, from, to)) = self.execute(
                line.start, end, pos.saturating_sub(line.start), engine,
            )? else {
                if end == hi { break; }
                i += 1;
                pos = self.doc.lines[i].start;
                continue;
            };
            pos = self.doc.char_end(to + usize::from(to == from));
            if from == end { i += 1; }
            if (from < to || last.is_none_or(|prior| from > prior))
                && self.word_accepts(line.start, end, from, to)? {
                if accept(hit)? { return Ok(()); }
                last = Some(to);
            }
        }
        Ok(())
    }

    fn first(
        &mut self, lo: usize, hi: usize,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Option<Hit>, Error> {
        let mut value = None;
        self.visit(lo, hi, false, engine, |hit| { value = Some(hit); Ok(true) })?;
        Ok(value)
    }

    /// Native caller supplies the main selection's physical upper byte edge.
    pub(super) fn next(
        &mut self, from_byte: usize, to_byte: usize,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Option<Hit>, Error> {
        let to = self.doc.unit(to_byte)?;
        if let Some(hit) = self.first(to, self.doc.len(), engine)? { return Ok(Some(hit)); }
        self.first(0, self.doc.unit(from_byte)?, engine)
    }

    fn last(
        &mut self, lo: usize, hi: usize,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Option<Hit>, Error> {
        let mut width = 10000;
        loop {
            let start = hi.saturating_sub(width).max(lo);
            // CM's private 10k seed may cut a trailing surrogate. Per-line
            // scans advance lastIndex in the intact line. For a multiline
            // sliced buffer, fail rather than invent a lone surrogate in UTF-8.
            let cursor_start = if self.multiline { start } else { self.doc.char_end(start) };
            let mut value = None;
            self.visit(cursor_start, hi, false, engine, |hit| { value = Some(hit); Ok(false) })?;
            if let Some(hit) = value {
                if start == lo || self.doc.unit(hit.range.start)? > start + 10 { return Ok(Some(hit)); }
            }
            if start == lo { return Ok(None); }
            width += 10000;
        }
    }

    /// Native caller supplies the main selection's physical lower byte edge.
    pub(super) fn previous(
        &mut self, from_byte: usize,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Option<Hit>, Error> {
        let from = self.doc.unit(from_byte)?;
        if let Some(hit) = self.last(0, from, engine)? { return Ok(Some(hit)); }
        self.last(from, self.doc.len(), engine)
    }

    pub(super) fn all(
        &mut self, max: usize,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Vec<Hit>, Error> {
        self.collect(0..self.doc.text.len(), max, false, engine)
    }

    pub(super) fn highlight(
        &mut self, bytes: Range<usize>, max: usize,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Vec<Hit>, Error> {
        self.collect(bytes, max, true, engine)
    }

    fn collect(
        &mut self, bytes: Range<usize>, max: usize, passive: bool,
        engine: &mut impl FnMut(Window<'_>) -> Result<Option<Hit>, Error>,
    ) -> Result<Vec<Hit>, Error> {
        let mut hits = Vec::new();
        let lo = self.doc.unit(bytes.start)?;
        let hi = self.doc.unit(bytes.end)?;
        self.visit(lo, hi, passive, engine, |hit| {
            if hits.len() == max { return Err(Error::Limit); }
            hits.push(hit);
            Ok(false)
        })?;
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_wrapped_tail_and_retained_lookbehind_prefix() {
        let mut windows = Vec::new();
        let mut p = Policy::new("catZ", "cat$", false, 4).unwrap();
        let hit = p.next(3, 3, &mut |w| {
            windows.push((w.input.to_owned(), w.last_index_utf16));
            Ok((w.input == "cat").then_some(Hit { range: 0..3, captures: vec![] }))
        }).unwrap().unwrap();
        assert_eq!(windows, vec![("catZ".into(), 3), ("cat".into(), 0)]);
        assert_eq!(hit.range, 0..3);
        let mut p = Policy::new("x cat x cat", "(?<=x )cat", false, 4).unwrap();
        let hit = p.next(2, 2, &mut |w| {
            assert_eq!((w.input,w.last_index_utf16),("x cat x cat",2));
            Ok(Some(Hit { range: 2..5, captures: vec![Some(0..1), None] }))
        }).unwrap().unwrap();
        assert_eq!(hit.captures, vec![Some(0..1),None]);
    }

    #[test]
    fn speculative_multiline_window_expands_and_zero_width_advances_utf16() {
        let line = "x".repeat(5100);
        let d = format!("{line}\nend tail");
        let mut p = Policy::new(&d, r"\send", false, 6).unwrap();
        let mut calls = 0;
        let result = p.next(0, 0, &mut |w| {
            calls += 1;
            if calls == 1 {
                assert_eq!(w.input, line);
                assert_eq!(w.last_index_utf16, 0);
                Ok(None)
            } else {
                assert_eq!(w.input, d);
                Ok(Some(Hit{range:5100..5104,captures:vec![]}))
            }
        }).unwrap().unwrap();
        assert_eq!(calls,2);
        assert_eq!(result.range,5100..5104);

        let mut p = Policy::new("😀cat", "(?=.)", false, 10).unwrap();
        let mut positions = Vec::new();
        let found = p.all(5, &mut |w| {
            positions.push(w.last_index_utf16);
            let range = match w.last_index_utf16 {
                0=>0..0, 2=>4..4, 3=>5..5, 4=>6..6, _=>return Ok(None),
            };
            Ok(Some(Hit { range, captures:vec![] }))
        }).unwrap();
        assert_eq!(positions, vec![0,2,3,4,5]);
        assert_eq!(found.into_iter().map(|h|h.range).collect::<Vec<_>>(),vec![0..0,4..4,5..5,6..6]);
    }

    #[test]
    fn refuses_unsafe_ranges_and_partial_budget() {
        let mut p = Policy::new("😃cat", ".", false, 3).unwrap();
        assert_eq!(p.next(1, 1,&mut |_| panic!("invalid byte edge must never call engine")),Err(Error::Bounds));
        assert_eq!(p.next(0, 0,&mut |_| Ok(Some(Hit{range:1..3,captures:vec![]}))),Err(Error::Bounds));
        let mut p = Policy::new("a a","a",false,10).unwrap();
        assert_eq!(p.all(1,&mut |w| Ok(Some(Hit {
            range: (if w.last_index_utf16==0 {0..1}else{2..3}), captures:vec![],
        }))),Err(Error::Limit));
    }
}
