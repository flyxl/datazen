//! CodeMirror string-search coordinates with a Unicode-safe UTF-8 boundary.
//! The matching stream is UTF-16, not normalized byte positions in the source.
use std::ops::Range;

use unicode_normalization::UnicodeNormalization;

use super::whole_word;

const INSIDE_SCALAR: usize = usize::MAX;
const PREVIOUS_CHUNK: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct UnrepresentableRange;

pub(super) struct LiteralDocument<'a> {
    raw: &'a str,
    units: Vec<u16>,
    /// UTF-16 boundary -> UTF-8 boundary; the middle of a surrogate pair
    /// does not correspond to *any* valid byte offset.
    byte_boundary: Vec<usize>,
    newlines: Vec<usize>,
    projected_breaks: bool,
}

impl<'a> LiteralDocument<'a> {
    pub(super) fn new(raw: &'a str, whole_word: bool) -> Self {
        let mut units = Vec::new();
        let mut byte_boundary = Vec::new();
        let mut newlines = Vec::new();
        let mut projected_breaks = false;
        let mut input = raw.char_indices().peekable();
        while let Some((byte, mut scalar)) = input.next() {
            byte_boundary.push(byte);
            if whole_word && scalar == '\r' {
                // CM's Text has LF line breaks. Map the entire original
                // CR/CRLF to one search unit without rewriting stored SQL.
                scalar = '\n';
                projected_breaks = true;
                if input.peek().is_some_and(|&(_, c)| c == '\n') {
                    input.next();
                }
            }
            if whole_word && scalar == '\n' { newlines.push(units.len()); }
            let mut encoded = [0; 2];
            let utf16 = scalar.encode_utf16(&mut encoded);
            units.extend_from_slice(utf16);
            if utf16.len() == 2 {
                byte_boundary.push(INSIDE_SCALAR);
            }
        }
        byte_boundary.push(raw.len());
        Self { raw, units, byte_boundary, newlines, projected_breaks }
    }

    pub(super) fn offset(&self, byte: usize) -> Result<usize, UnrepresentableRange> {
        // The editor selection is given as UTF-8 bytes, but CodeMirror's
        // wrapping and its 10k backward chunks are in original UTF-16 units.
        if self.projected_breaks {
            if !self.raw.is_char_boundary(byte) {
                return Err(UnrepresentableRange);
            }
            self.byte_boundary.iter().position(|&b| b == byte).ok_or(UnrepresentableRange)
        } else {
            self.raw.get(..byte)
                .map(|s| s.encode_utf16().count())
                .ok_or(UnrepresentableRange)
        }
    }

    pub(super) fn bytes(&self, range: Range<usize>) -> Result<Range<usize>, UnrepresentableRange> {
        let at = |index: usize| {
            self.byte_boundary.get(index).copied()
                .filter(|offset| *offset != INSIDE_SCALAR)
                .ok_or(UnrepresentableRange)
        };
        Ok(at(range.start)?..at(range.end)?)
    }

    fn len(&self) -> usize {
        self.units.len()
    }
}

pub(super) struct LiteralQuery {
    target: Vec<u16>,
    prefix_fallback: Vec<usize>,
    raw_len: usize,
    match_case: bool,
    whole_word: bool,
}

fn normalize(value: &str, match_case: bool) -> Vec<u16> {
    let normalized: String = value.nfkd().collect();
    if match_case {
        normalized.encode_utf16().collect()
    } else {
        normalized.to_lowercase().encode_utf16().collect()
    }
}

impl LiteralQuery {
    pub(super) fn new(raw: &str, match_case: bool, whole_word: bool) -> Option<Self> {
        // Normalize the entire query, but the cursor below normalizes one
        // *source scalar* at a time. Whole-string lowercase on the document
        // would disagree on contextual Greek final sigma, for example.
        let target = normalize(raw, match_case);
        if target.is_empty() {
            return None;
        }
        let mut prefix_fallback = vec![0; target.len()];
        let mut matched = 0;
        for i in 1..target.len() {
            while matched > 0 && target[matched] != target[i] {
                matched = prefix_fallback[matched - 1];
            }
            if target[matched] == target[i] {
                matched += 1;
            }
            prefix_fallback[i] = matched;
        }
        Some(Self { target, prefix_fallback, raw_len: raw.encode_utf16().count(), match_case, whole_word })
    }

    /// Visit matches without allocating a normalized copy of the document.
    /// KMP recognizes overlapping matches; its ring of source positions lets
    /// a folded/decomposed match still address its original scalar boundaries.
    fn scan(
        &self,
        doc: &LiteralDocument<'_>,
        from: usize,
        to: usize,
        overlapping: bool,
        mut visit: impl FnMut(Range<usize>) -> bool,
    ) {
        let mut cursor = from;
        let size = self.target.len();
        let mut starts = vec![0; size];
        let mut words = whole_word::Filter::default();
        let (mut written, mut matched) = (0, 0);
        while cursor < to {
            let original = doc.units[cursor];
            let scalar_units =
                if (0xd800..=0xdbff).contains(&original) && cursor + 1 < to
                    && (0xdc00..=0xdfff).contains(&doc.units[cursor + 1])
                {
                    2
                } else {
                    1
                };
            let end = cursor + scalar_units;
            let source = &doc.units[cursor..end];
            let ascii;
            let normalized;
            let projection: &[u16] = if original <= 0x7f {
                let unit = original as u8;
                ascii = [u16::from(if self.match_case { unit } else { unit.to_ascii_lowercase() })];
                &ascii
            } else if let Ok(scalar) = String::from_utf16(source) {
                normalized = normalize(&scalar, self.match_case);
                &normalized
            } else {
                // A wrap/chunk bound can itself split a surrogate pair.
                // Preserve the unit in the comparison stream, not in the
                // original document. Real edits are checked at both edges.
                source
            };
            let mut pos = cursor;
            for (i, unit) in projection.iter().copied().enumerate() {
                starts[written] = pos;
                written = (written + 1) % size;
                while matched > 0 && self.target[matched] != unit {
                    matched = self.prefix_fallback[matched - 1];
                }
                if self.target[matched] == unit {
                    matched += 1;
                }
                if matched == size {
                    matched = self.prefix_fallback[matched - 1];
                    // The CM cursor publishes only a match ending on the
                    // last projected unit of this source scalar.
                    let candidate = starts[written]..end;
                    if i + 1 == projection.len()
                        && (!self.whole_word || words.accepts(
                            &doc.units, &doc.newlines, from..to, cursor, candidate.clone(),
                        ))
                    {
                        if visit(candidate) {
                            return;
                        }
                        if !overlapping {
                            matched = 0;
                        }
                    }
                }
                if pos == cursor && source.get(i) == Some(&unit) {
                    pos += 1;
                }
            }
            cursor = end;
        }
    }

    fn first(&self, doc: &LiteralDocument<'_>, from: usize, to: usize) -> Option<Range<usize>> {
        let mut found = None;
        self.scan(doc, from, to, true, |range| {
            found = Some(range);
            true
        });
        found
    }

    fn latest(&self, doc: &LiteralDocument<'_>, from: usize, mut to: usize) -> Option<Range<usize>> {
        loop {
            let start = from.max(to.saturating_sub(PREVIOUS_CHUNK.saturating_add(self.raw_len)));
            let mut found = None;
            self.scan(doc, start, to, true, |range| {
                found = Some(range);
                false
            });
            if found.is_some() || start == from {
                return found;
            }
            to -= PREVIOUS_CHUNK;
        }
    }

    pub(super) fn next(&self, doc: &LiteralDocument<'_>, cursor: Range<usize>) -> Option<Range<usize>> {
        self.first(doc, cursor.end, doc.len())
            .or_else(|| self.first(doc, 0, cursor.start.saturating_add(self.raw_len).min(doc.len())))
            .filter(|range| range != &cursor)
    }

    pub(super) fn previous(&self, doc: &LiteralDocument<'_>, cursor: Range<usize>) -> Option<Range<usize>> {
        self.latest(doc, 0, cursor.start)
            .or_else(|| self.latest(doc, cursor.end.saturating_sub(self.raw_len), doc.len()))
            .filter(|range| range != &cursor)
    }

    /// CodeMirror StringQuery.highlight: non-overlapping from the start
    /// of this viewport's search-length margin, not from document zero.
    /// Keep paint-only safe hits even when another range splits a surrogate.
    pub(super) fn visible_matches(
        &self, doc: &LiteralDocument<'_>, window: Range<usize>, limit: usize,
    ) -> Option<Vec<Range<usize>>> {
        let from = doc.offset(window.start).ok()?;
        let to = doc.offset(window.end).ok()?;
        if from >= to {
            return Some(Vec::new());
        }
        let mut out = Some(Vec::new());
        self.scan(
            doc, from.saturating_sub(self.raw_len), to.saturating_add(self.raw_len).min(doc.len()),
            false, |hit| {
                if hit.end <= from || hit.start >= to {
                    return false;
                }
                if let Ok(bytes) = doc.bytes(hit) {
                    let Some(ranges) = &mut out else { return true };
                    if ranges.len() == limit {
                        out = None;
                        return true;
                    }
                    ranges.push(bytes);
                }
                false
            },
        );
        out
    }

    /// Non-overlapping matches for an explicit multi-selection. Exactly
    /// limit is allowed. The next match aborts rather than truncating, as
    /// CodeMirror's StringQuery.matchAll does for Select All Matches.
    pub(super) fn all_up_to(
        &self, doc: &LiteralDocument<'_>, limit: usize,
    ) -> Result<Option<Vec<Range<usize>>>, UnrepresentableRange> {
        let mut result = Ok(Some(Vec::new()));
        self.scan(doc, 0, doc.len(), false, |range| {
            let Ok(Some(ranges)) = &mut result else { return true };
            if ranges.len() == limit {
                result = Ok(None);
                return true;
            }
            match doc.bytes(range) {
                Ok(bytes) => {
                    ranges.push(bytes);
                    false
                }
                Err(error) => {
                    result = Err(error);
                    true
                }
            }
        });
        result
    }

    pub(super) fn all(&self, doc: &LiteralDocument<'_>) -> Result<Vec<Range<usize>>, UnrepresentableRange> {
        let mut ranges = Vec::new();
        let mut safe = true;
        self.scan(doc, 0, doc.len(), false, |range| match doc.bytes(range) {
            Ok(bytes) => {
                ranges.push(bytes);
                false
            }
            Err(_) => {
                safe = false;
                true
            }
        });
        if safe { Ok(ranges) } else { Err(UnrepresentableRange) }
    }
}
