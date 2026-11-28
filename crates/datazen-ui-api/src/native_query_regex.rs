//! Engine-neutral Native Query Find boundary. Only local, isolated providers
//! may advertise this capability. Ranges refer to the admitted LF UTF-8 draft.
use std::ops::Range;

pub mod projection;

pub const MAX_DOC: usize = 128 * 1024;
pub const MAX_PATTERN: usize = 1024;
pub const MAX_TEMPLATE: usize = 4096;
pub const MAX_RANGES: usize = 1000;
pub const MAX_EDITS: usize = 4096;
pub const MAX_OUTPUT: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation { Next, Previous, SelectAll, ReplaceNext, ReplaceAll }
impl Operation {
    pub fn replacing(self) -> bool { matches!(self, Self::ReplaceNext | Self::ReplaceAll) }
}

pub struct Request {
    pub id: u64,
    pub operation: Operation,
    pub document: String,
    pub selection: Range<usize>,
    pub pattern: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

pub struct Edit { pub range: Range<usize>, pub insert: String }
pub enum Outcome {
    Select(Vec<Range<usize>>),
    Edits { edits: Vec<Edit>, following: Option<Range<usize>> },
}
pub struct Delivery { pub id: u64, pub result: Result<Outcome, Error> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Empty, InvalidPattern, UnsupportedRange, Limit, Timeout, Unavailable, InvalidReply }

impl Request {
    pub fn check(&self) -> Result<(), Error> {
        if self.document.len() > MAX_DOC || self.pattern.len() > MAX_PATTERN || self.replacement.len() > MAX_TEMPLATE { return Err(Error::Limit); }
        if self.pattern.is_empty() { return Err(Error::Empty); }
        if self.document.contains('\r') || self.document.get(self.selection.clone()).is_none() {
            return Err(Error::UnsupportedRange);
        }
        if self.id == 0 || (!self.operation.replacing() && !self.replacement.is_empty()) { return Err(Error::InvalidReply); }
        Ok(())
    }
}

impl Outcome {
    /// Check even a nonstandard adapter's output before touching history,
    /// ranges, bracket provenance or document bytes. No partial edit is valid.
    pub fn check(&self, op: Operation, doc: &str, main: Range<usize>) -> Result<(), Error> {
        fn ranges<'a>(doc: &str, seq: impl IntoIterator<Item=&'a Range<usize>>) -> bool {
            let mut last: Option<&Range<usize>> = None;
            for r in seq {
                if doc.get(r.clone()).is_none() || last.is_some_and(|p| p.end > r.start || p.start == r.start) { return false; }
                last = Some(r);
            }
            true
        }
        match self {
            Self::Select(hits) => {
                if hits.len() > MAX_RANGES { return Err(Error::Limit); }
                if (!hits.is_empty() && op == Operation::ReplaceAll)
                    || (hits.len() > 1 && op != Operation::SelectAll) || !ranges(doc, hits) { return Err(Error::InvalidReply); }
            }
            Self::Edits { edits, following } => {
                if edits.len() > MAX_EDITS { return Err(Error::Limit); }
                if !op.replacing() || edits.is_empty() || !ranges(doc, edits.iter().map(|e| &e.range))
                    || (op == Operation::ReplaceNext && (edits.len() != 1 || edits[0].range != main))
                    || (op == Operation::ReplaceAll && following.is_some())
                    || following.as_ref().is_some_and(|r| doc.get(r.clone()).is_none()) { return Err(Error::InvalidReply); }
                let mut added = 0usize;
                let mut removed = 0usize;
                for e in edits {
                    if e.insert.contains('\r') { return Err(Error::InvalidReply); }
                    added = added.checked_add(e.insert.len()).filter(|n| *n <= MAX_OUTPUT).ok_or(Error::Limit)?;
                    removed += e.range.len();
                }
                if doc.len() - removed + added > MAX_OUTPUT { return Err(Error::Limit); }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_edit_with_navigation_or_partial_offsets() {
        let edit = |range| Outcome::Edits {
            edits: vec![Edit {range, insert: "z".into()}], following: None,
        };
        assert_eq!(edit(0..3).check(Operation::Next,"猫",0..3),Err(Error::InvalidReply));
        assert_eq!(edit(0..1).check(Operation::ReplaceAll,"猫",0..0),Err(Error::InvalidReply));
        assert_eq!(edit(0..3).check(Operation::ReplaceNext,"猫",0..0),Err(Error::InvalidReply));
        assert!(edit(0..3).check(Operation::ReplaceNext,"猫",0..3).is_ok());
    }

    #[test]
    fn select_all_and_replace_all_have_different_budgets() {
        let doc = "a".repeat(MAX_EDITS+1);
        let edits = |count| Outcome::Edits {
            edits:(0..count).map(|i|Edit {range:i..i+1,insert:String::new()}).collect(),
            following:None,
        };
        assert_eq!(edits(MAX_EDITS).check(Operation::ReplaceAll,&doc,0..0),Ok(()));
        assert_eq!(edits(MAX_EDITS+1).check(Operation::ReplaceAll,&doc,0..0),Err(Error::Limit));
        assert_eq!(Outcome::Select((0..=MAX_RANGES).map(|i|i..i+1).collect()).check(Operation::SelectAll,&doc,0..0),Err(Error::Limit));
    }

    #[test]
    fn rejects_overlap_trailing_bad_range_and_oversized_insertion() {
        let edits=|r, text:String| Outcome::Edits { edits:vec![
            Edit{range:0..1,insert:String::new()},Edit{range:r,insert:text}],following:None };
        for bad in [edits(0..1,"x".into()), edits(1..4,"".into()), edits(1..2,"\r".into())] {
            assert_eq!(bad.check(Operation::ReplaceAll,"ab",0..0),Err(Error::InvalidReply));
        }
        assert_eq!(edits(1..2,"x".repeat(MAX_OUTPUT+1)).check(Operation::ReplaceAll,"ab",0..0),Err(Error::Limit));
    }
}

/// Passive decoration for one measured viewport. This channel cannot return an
/// edit or selection, and it cannot evict an interactive Find command.
pub struct MarkRequest {
    pub id: u64,
    pub document: String,
    pub viewport: Range<usize>,
    pub pattern: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
}
impl MarkRequest {
    pub fn check(&self) -> Result<(), Error> {
        if self.document.len() > MAX_DOC || self.pattern.len() > MAX_PATTERN { return Err(Error::Limit); }
        if self.pattern.is_empty() { return Err(Error::Empty); }
        if self.id == 0 { return Err(Error::InvalidReply); }
        if self.document.contains('\r') || self.document.get(self.viewport.clone()).is_none() {
            return Err(Error::UnsupportedRange);
        }
        Ok(())
    }
}
pub struct MarkDelivery { pub id: u64, pub result: Result<Vec<Range<usize>>, Error> }

/// A CM highlight cursor can emit context (including whole-line lookahead)
/// beyond the viewport. Validate original ranges; clip only for painting.
pub fn check_marks(doc: &str, marks: &[Range<usize>]) -> Result<(), Error> {
    if marks.len() > MAX_RANGES { return Err(Error::Limit); }
    if marks.iter().any(|r| doc.get(r.clone()).is_none()) ||
        marks.windows(2).any(|pair| pair[0].end > pair[1].start || pair[0].start == pair[1].start) {
        return Err(Error::InvalidReply);
    }
    Ok(())
}
