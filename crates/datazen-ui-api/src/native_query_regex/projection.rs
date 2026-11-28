//! Lossless boundary translation between a Native draft and the LF-only
//! regular-expression worker view. No edits are applied in this module.

use std::{borrow::Cow, ops::Range};

use super::{Edit, Error, MAX_DOC, Operation, Outcome};

struct Break {
    source: usize,
    normalized: usize,
}

/// Offsets in both coordinate spaces are UTF-8 scalar boundaries. The byte
/// edge inside a source CRLF is deliberately unrepresentable in the worker.
pub struct Projection<'a> {
    original: &'a str,
    normalized: Cow<'a, str>,
    breaks: Vec<Break>,
}

impl<'a> Projection<'a> {
    /// Accept mixed LF/CRLF but not bare CR. Check the source byte budget
    /// before scanning or allocating, even when CRLF would normalize smaller.
    pub fn new(original: &'a str) -> Result<Self, Error> {
        if original.len() > MAX_DOC {
            return Err(Error::Limit);
        }
        if !original.contains('\r') {
            return Ok(Self { original, normalized: Cow::Borrowed(original), breaks: Vec::new() });
        }

        let mut normalized = String::with_capacity(original.len() - 1);
        let mut breaks = Vec::new();
        let mut from = 0;
        for (at, _) in original.match_indices('\r') {
            if original.as_bytes().get(at + 1) != Some(&b'\n') {
                return Err(Error::UnsupportedRange);
            }
            normalized.push_str(&original[from..at]);
            breaks.push(Break { source: at, normalized: normalized.len() });
            // Skip only CR. The retained LF and all other original bytes
            // are copied by the next chunk or by the trailing append.
            from = at + 1;
        }
        normalized.push_str(&original[from..]);
        Ok(Self { original, normalized: Cow::Owned(normalized), breaks })
    }

    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    pub fn to_normalized(&self, original: Range<usize>) -> Result<Range<usize>, Error> {
        if self.original.get(original.clone()).is_none() {
            return Err(Error::UnsupportedRange);
        }
        Ok(self.normalize_edge(original.start)?..self.normalize_edge(original.end)?)
    }

    pub fn to_original(&self, normalized: Range<usize>) -> Result<Range<usize>, Error> {
        if self.normalized.get(normalized.clone()).is_none() {
            return Err(Error::UnsupportedRange);
        }
        Ok(self.source_edge(normalized.start)..self.source_edge(normalized.end))
    }

    fn normalize_edge(&self, original: usize) -> Result<usize, Error> {
        let preceding = self.breaks.partition_point(|edge| edge.source < original);
        if preceding != 0 && self.breaks[preceding - 1].source + 1 == original {
            return Err(Error::UnsupportedRange);
        }
        Ok(original - preceding)
    }

    fn source_edge(&self, normalized: usize) -> usize {
        normalized + self.breaks.partition_point(|edge| edge.normalized < normalized)
    }

    /// Translate a checked worker result without applying it. No result is
    /// returned for a different original byte snapshot, including one with
    /// identical normalized text but changed line endings. The editor must
    /// still check tab, document revision (including identical-byte ABA),
    /// all current selection state and command ownership, then apply edits
    /// against the exact original source in one transaction.
    ///
    /// Replacement inserts remain LF as returned by the worker. An unchanged
    /// source span never acquires newly normalized line endings.
    pub fn checked_outcome(
        &self,
        current_original: &str,
        operation: Operation,
        original_main: Range<usize>,
        outcome: Outcome,
    ) -> Result<Option<Outcome>, Error> {
        if self.original != current_original {
            return Ok(None);
        }
        let normalized_main = self.to_normalized(original_main.clone())?;
        outcome.check(operation, self.normalized(), normalized_main)?;

        let lift = |r| self.to_original(r).map_err(|_| Error::InvalidReply);
        let mapped = match outcome {
            Outcome::Select(hits) => {
                Outcome::Select(hits.into_iter().map(lift).collect::<Result<_, _>>()?)
            }
            Outcome::Edits { edits, following } => {
                let edits = edits.into_iter().map(|Edit { range, insert }| {
                    Ok(Edit { range: lift(range)?, insert })
                }).collect::<Result<_, Error>>()?;
                Outcome::Edits { edits, following: following.map(lift).transpose()? }
            }
        };
        // A normal-sized LF result could exceed the original output limit
        // once untouched CRLF bytes are counted. Re-check after projection.
        mapped.check(operation, current_original, original_main)?;
        Ok(Some(mapped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_query_regex::MAX_OUTPUT;

    #[test]
    fn mixed_line_endings_and_astral_boundaries_round_trip() {
        let source = "猫\r\n😀\nX\r\n";
        let projection = Projection::new(source).unwrap();
        assert_eq!(projection.normalized(), "猫\n😀\nX\n");

        let edges = [(0, 0), (3, 3), (5, 4), (9, 8), (10, 9), (11, 10), (13, 11)];
        for (original, normalized) in edges {
            assert_eq!(projection.to_normalized(original..original), Ok(normalized..normalized));
            assert_eq!(projection.to_original(normalized..normalized), Ok(original..original));
        }
        // Either newline can be matched, including the pair at EOF.
        assert_eq!(projection.to_original(3..4), Ok(3..5));
        assert_eq!(projection.to_original(8..9), Ok(9..10));
        assert_eq!(projection.to_original(10..11), Ok(11..13));

        for interior in [1, 2, 4, 6, 7, 8, 12, 14] {
            assert_eq!(projection.to_normalized(interior..interior), Err(Error::UnsupportedRange));
        }
        for interior in [1, 2, 5, 6, 7, 12] {
            assert_eq!(projection.to_original(interior..interior), Err(Error::UnsupportedRange));
        }
        assert_eq!(projection.to_normalized(13..3), Err(Error::UnsupportedRange));
        assert_eq!(projection.to_original(11..3), Err(Error::UnsupportedRange));
    }

    #[test]
    fn rejects_bare_cr_and_checks_original_budget_before_normalizing() {
        for bare in ["\r", "abc\rdef", "x\r\ny\r", "\r\r\n"] {
            assert!(matches!(Projection::new(bare), Err(Error::UnsupportedRange)));
        }

        let max = "\r\n".repeat(MAX_DOC / 2);
        let projection = Projection::new(&max).unwrap();
        assert_eq!(projection.to_original(0..MAX_DOC / 2), Ok(0..MAX_DOC));
        assert!(matches!(Projection::new(&(max + "a")), Err(Error::Limit)));
        assert_eq!(Projection::new("").unwrap().to_original(0..0), Ok(0..0));

        let lf = "x\n😀";
        let unchanged = Projection::new(lf).unwrap();
        assert_eq!(unchanged.normalized().as_ptr(), lf.as_ptr());
        assert_eq!(unchanged.to_original(0..lf.len()), Ok(0..lf.len()));
    }

    #[test]
    fn stale_reply_cannot_edit_and_only_matched_source_bytes_change() {
        fn replace_next() -> Outcome {
            Outcome::Edits {
                edits: vec![Edit { range: 4..7, insert: "dog\n".into() }],
                following: Some(8..11),
            }
        }

        let original = "猫\r\ncat\r\ncat\n!";
        let projection = Projection::new(original).unwrap();
        let normalized_equal = "猫\ncat\ncat\n!";
        assert_eq!(projection.normalized(), normalized_equal);
        for stale in [normalized_equal, "猫\r\nCAT\r\ncat\n!"] {
            assert!(matches!(
                projection.checked_outcome(stale, Operation::ReplaceNext, 5..8, replace_next()),
                Ok(None)
            ));
        }
        assert!(matches!(
            projection.checked_outcome(original, Operation::ReplaceNext, 4..8, replace_next()),
            Err(Error::UnsupportedRange)
        ));
        assert!(matches!(
            projection.checked_outcome(original, Operation::ReplaceNext, 5..6, replace_next()),
            Err(Error::InvalidReply)
        ));

        let mapped = projection.checked_outcome(
            original, Operation::ReplaceNext, 5..8, replace_next(),
        ).unwrap();
        let Some(Outcome::Edits { edits, following }) = mapped else {
            panic!("expected a checked edit");
        };
        assert_eq!(following, Some(10..13));
        assert_eq!(edits[0].range, 5..8);
        let mut edited = original.to_owned();
        edited.replace_range(edits[0].range.clone(), &edits[0].insert);
        assert_eq!(edited, "猫\r\ndog\n\r\ncat\n!");
    }

    #[test]
    fn byte_equality_is_not_a_revision_lease() {
        let original = "a\r\nfoo";
        let lease = Projection::new(original).unwrap();
        let at = |revision_matches: bool, current: &str| {
            // This is the caller's revision guard, before the byte check.
            if !revision_matches {
                return Ok(None);
            }
            lease.checked_outcome(
                current, Operation::Next, 0..0, Outcome::Select(vec![2..5]),
            )
        };

        // Different bytes are stale even if a caller reports the same revision.
        assert!(matches!(at(true, "a\r\nbar"), Ok(None)));
        // Restoring identical bytes after an intervening edit (ABA) is stale
        // by revision, but cannot be detected by the projection byte check.
        let identical_new_value = original.to_owned();
        assert!(matches!(at(false, &identical_new_value), Ok(None)));
        let Some(Outcome::Select(hits)) = at(true, &identical_new_value).unwrap() else {
            panic!("the projection only has evidence about the current bytes");
        };
        assert_eq!(hits, [3..6]);
    }

    #[test]
    fn maps_selection_and_consumes_a_matched_crlf_pair_atomically() {
        let original = "a\r\n😀\r\nx";
        let projection = Projection::new(original).unwrap();
        let outcome = projection.checked_outcome(
            original, Operation::SelectAll, 0..0, Outcome::Select(vec![1..2, 6..7]),
        ).unwrap();
        let Some(Outcome::Select(hits)) = outcome else {
            panic!("expected checked hits");
        };
        assert_eq!(hits, [1..3, 7..9]);

        let outcome = projection.checked_outcome(
            original,
            Operation::ReplaceAll,
            0..0,
            Outcome::Edits {
                edits: vec![Edit { range: 1..2, insert: String::new() }],
                following: None,
            },
        ).unwrap();
        let Some(Outcome::Edits { edits, .. }) = outcome else {
            panic!("expected the matched CRLF deletion");
        };
        let mut edited = original.to_owned();
        edited.replace_range(edits[0].range.clone(), &edits[0].insert);
        assert_eq!(edited, "a😀\r\nx");
    }

    #[test]
    fn rejects_original_overflow_even_if_normalized_reply_fits_exactly() {
        let original = "\r\n".repeat(MAX_DOC / 2);
        let projection = Projection::new(&original).unwrap();
        let outcome = Outcome::Edits {
            edits: vec![Edit {
                range: 0..0,
                insert: "a".repeat(MAX_OUTPUT - projection.normalized().len()),
            }],
            following: None,
        };
        assert_eq!(outcome.check(Operation::ReplaceAll, projection.normalized(), 0..0), Ok(()));
        assert!(matches!(
            projection.checked_outcome(&original, Operation::ReplaceAll, 0..0, outcome),
            Err(Error::Limit)
        ));
    }
}
