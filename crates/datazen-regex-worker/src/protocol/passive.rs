//! A separately tagged, mark-only frame. Active command bytes have not
//! changed, and an active reply (even well-formed) has no authority here.

use super::{DOC, PATTERN, Reader, VERSION, Writer, check_range, error_tag, multiline_pattern, read_error_tag};
use crate::{Error, HIGHLIGHT_MAX_MARKS, HighlightRequest};
use std::ops::Range;

const HIGHLIGHT: u8 = 5;
const MARKS: u8 = 6;
const MARGIN_UTF16: usize = 250;
pub(crate) const MAX_REPLY: usize = 7 + HIGHLIGHT_MAX_MARKS * 8;

pub(crate) fn is_highlight(data: &[u8]) -> bool {
    data.starts_with(&[VERSION, HIGHLIGHT])
}

pub(crate) fn encode_request(
    request: &HighlightRequest<'_>,
) -> Result<(Vec<u8>, Range<usize>), Error> {
    let visible = window(request)?;
    let mut bytes = Writer::default();
    bytes.flag(VERSION);
    bytes.flag(HIGHLIGHT);
    bytes.flag(u8::from(request.case_sensitive) | (u8::from(request.whole_word) << 1));
    bytes.span(&request.viewport)?;
    bytes.text(request.document)?;
    bytes.text(request.pattern)?;
    Ok((bytes.0, reply_bounds(request, visible)))
}

pub(crate) fn decode_request(
    data: &[u8],
) -> Result<(HighlightRequest<'_>, Range<usize>), Error> {
    let mut r = Reader(data);
    if r.flag()? != VERSION || r.flag()? != HIGHLIGHT {
        return Err(Error::InvalidReply);
    }
    let flags = r.flag()?;
    if flags > 3 {
        return Err(Error::InvalidReply);
    }
    let request = HighlightRequest {
        case_sensitive: flags & 1 != 0,
        whole_word: flags & 2 != 0,
        viewport: r.span()?,
        document: r.text(DOC)?,
        pattern: r.text(PATTERN)?,
    };
    r.end()?;
    let window = window(&request)?;
    Ok((request, window))
}

pub(crate) fn encode_reply(outcome: Result<Vec<Range<usize>>, Error>) -> Result<Vec<u8>, Error> {
    let mut w = Writer::default();
    w.flag(VERSION);
    w.flag(HIGHLIGHT);
    match outcome {
        Ok(marks) if marks.len() <= HIGHLIGHT_MAX_MARKS => {
            w.flag(MARKS);
            w.number(marks.len())?;
            for mark in &marks {
                w.span(mark)?;
            }
        }
        Ok(_) => w.flag(error_tag(Error::BudgetExceeded)?),
        Err(error) => w.flag(error_tag(error)?),
    }
    Ok(w.0)
}

/// Validate the whole child frame before exposing a single visible mark.
pub(crate) fn decode_reply(
    document: &str,
    window: Range<usize>,
    bytes: &[u8],
) -> Result<Vec<Range<usize>>, Error> {
    let mut r = Reader(bytes);
    if r.flag()? != VERSION || r.flag()? != HIGHLIGHT || bytes.len() > MAX_REPLY {
        return Err(Error::InvalidReply);
    }
    let tag = r.flag()?;
    if tag != MARKS {
        r.end()?;
        return Err(read_error_tag(tag));
    }
    let count = r.number()?;
    if count > HIGHLIGHT_MAX_MARKS {
        return Err(Error::InvalidReply);
    }
    let mut marks = Vec::with_capacity(count);
    for _ in 0..count {
        let range = r.span()?;
        check_range(document, &range, marks.last())?;
        if window.start > range.start || window.end < range.end {
            return Err(Error::InvalidReply);
        }
        marks.push(range);
    }
    r.end()?;
    Ok(marks)
}

/// Most marks cannot escape the expanded window. A wide multiline cursor's
/// speculative chunk may cover the end of the line intersecting the upper
/// bound, before the final clipped retry. Check that documented overreach
/// here without relaxing the exact byte or count checks.
fn reply_bounds(request: &HighlightRequest<'_>, visible: Range<usize>) -> Range<usize> {
    let doc = request.document;
    let to = visible.end;
    if to == 0
        || to == doc.len()
        || doc.as_bytes()[to - 1] == b'\n'
        || !multiline_pattern(request.pattern)
        || doc[visible.clone()].encode_utf16().take(5001).count() <= 5000
    {
        return visible;
    }
    let line_end = doc[to..].find('\n').map_or(doc.len(), |i| to + i);
    visible.start..line_end
}

/// CodeMirror subtracts/adds 250 *UTF-16 code units*, not 250 UTF-8 bytes.
/// Traverse at most that many scalars in either direction. A sliced
/// half-surrogate would need a non-UTF-8 view, so refuse it fail-closed.
fn window(request: &HighlightRequest<'_>) -> Result<Range<usize>, Error> {
    let doc = request.document;
    if doc.len() > DOC || request.pattern.len() > PATTERN {
        return Err(Error::BudgetExceeded);
    }
    if doc.contains('\r') || doc.get(request.viewport.clone()).is_none() {
        return Err(Error::UnsupportedRange);
    }
    if request.pattern.is_empty() {
        return Err(Error::Empty);
    }

    let mut from = request.viewport.start;
    let mut to = request.viewport.end;
    let mut left = MARGIN_UTF16;
    for (at, ch) in doc[..from].char_indices().rev() {
        if left == 0 {
            break;
        }
        left = left.checked_sub(ch.len_utf16()).ok_or(Error::UnsupportedRange)?;
        from = at;
    }
    let mut right = MARGIN_UTF16;
    for ch in doc[to..].chars() {
        if right == 0 {
            break;
        }
        right = right.checked_sub(ch.len_utf16()).ok_or(Error::UnsupportedRange)?;
        to += ch.len_utf8();
    }
    Ok(from..to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_complete_sorted_unique_in_scope_marks_can_pass() {
        let valid = encode_reply(Ok(vec![1..1, 5..6])).unwrap();
        let doc = "a😀x?";
        let check = |bytes: &[u8]| decode_reply(doc, 1..6, bytes);
        assert_eq!(check(&valid), Ok(vec![1..1, 5..6]));
        for bad in [vec![0..1], vec![1..2], vec![5..7], vec![5..6, 1..1], vec![5..5, 5..6]] {
            let encoded = encode_reply(Ok(bad)).unwrap();
            assert_eq!(check(&encoded), Err(Error::InvalidReply));
        }
        assert_eq!(check(&valid[..valid.len() - 1]), Err(Error::InvalidReply));
        let mut trailing = valid;
        trailing.push(0);
        assert_eq!(check(&trailing), Err(Error::InvalidReply));
    }

    #[test]
    fn active_edits_and_passive_marks_cannot_be_confused() {
        use crate::{Edit, Operation, Outcome, Request};
        let active = super::super::encode_reply(
            Operation::ReplaceAll,
            Ok(Outcome::Edits {
                edits: vec![Edit { range: 0..1, insert: "b".into() }],
                following: None,
            }),
        ).unwrap();
        assert_eq!(decode_reply("a", 0..1, &active), Err(Error::InvalidReply));

        let active_request = Request {
            operation: Operation::ReplaceAll,
            case_sensitive: true,
            whole_word: false,
            selection: 0..0,
            document: "a",
            pattern: "a",
            replacement: "b",
        };
        let active_bytes = super::super::encode_request(&active_request).unwrap();
        // Frozen V1 command shape (not the passive request layout).
        assert_eq!(active_bytes, [
            1, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0,
            1, 0, 0, 0, b'a', 1, 0, 0, 0, b'a', 1, 0, 0, 0, b'b',
        ]);
        assert!(matches!(decode_request(&active_bytes), Err(Error::InvalidReply)));

        let marks = encode_reply(Ok(vec![0..1])).unwrap();
        assert_eq!(
            super::super::decode_reply(&active_request, &marks),
            Err(Error::InvalidReply),
        );
    }

    #[test]
    fn oversized_count_never_allocates_a_partial_result() {
        let document = "x".repeat(HIGHLIGHT_MAX_MARKS + 1);
        let mut frame = Writer(vec![VERSION, HIGHLIGHT, MARKS]);
        frame.number(HIGHLIGHT_MAX_MARKS + 1).unwrap();
        assert_eq!(decode_reply(&document, 0..document.len(), &frame.0), Err(Error::InvalidReply));
        let too_many = (0..document.len()).map(|i| i..i + 1).collect();
        let refused = encode_reply(Ok(too_many)).unwrap();
        assert_eq!(
            decode_reply(&document, 0..document.len(), &refused),
            Err(Error::BudgetExceeded),
        );
    }
}
