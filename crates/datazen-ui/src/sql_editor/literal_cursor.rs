//! CodeMirror string-search coordinates with a Unicode-safe UTF-8 boundary.
//! The matching stream is UTF-16, not normalized byte positions in the source.
use std::ops::Range;

use unicode_normalization::UnicodeNormalization;

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
}

impl<'a> LiteralDocument<'a> {
    pub(super) fn new(raw: &'a str) -> Self {
        let mut units = Vec::new();
        let mut byte_boundary = Vec::new();
        for (byte, scalar) in raw.char_indices() {
            byte_boundary.push(byte);
            let mut encoded = [0; 2];
            let utf16 = scalar.encode_utf16(&mut encoded);
            units.extend_from_slice(utf16);
            if utf16.len() == 2 {
                byte_boundary.push(INSIDE_SCALAR);
            }
        }
        byte_boundary.push(raw.len());
        Self { raw, units, byte_boundary }
    }

    pub(super) fn offset(&self, byte: usize) -> Result<usize, UnrepresentableRange> {
        // The editor selection is given as UTF-8 bytes, but CodeMirror's
        // wrapping and its 10k backward chunks are in original UTF-16 units.
        self.raw.get(..byte)
            .map(|s| s.encode_utf16().count())
            .ok_or(UnrepresentableRange)
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
    pub(super) fn new(raw: &str, match_case: bool) -> Option<Self> {
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
        Some(Self { target, prefix_fallback, raw_len: raw.encode_utf16().count(), match_case })
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
                    if i + 1 == projection.len() {
                        if visit(starts[written]..end) {
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
