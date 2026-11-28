use std::{
    io::{self, Read, Write},
    ops::Range,
};

use crate::{Edit, Error, Operation, Outcome, Request};

pub(crate) mod passive;

pub(crate) const READY: u8 = 0x95;
const VERSION: u8 = 1;
const DOC: usize = 128 * 1024;
const PATTERN: usize = 1024;
pub(crate) const TEMPLATE: usize = 4096;
pub(crate) const OUTPUT: usize = 512 * 1024;
pub(crate) const SELECT_COUNT: usize = 1000;
pub(crate) const REPLACE_COUNT: usize = 4096;
pub(crate) const MAX_REQUEST: usize = DOC + PATTERN + TEMPLATE + 32;
pub(crate) const MAX_REPLY: usize = OUTPUT + REPLACE_COUNT * 12 + 32;

pub(crate) fn validate(request: &Request<'_>) -> Result<(), Error> {
    if request.document.len() > DOC
        || request.pattern.len() > PATTERN
        || request.replacement.len() > TEMPLATE
    {
        return Err(Error::BudgetExceeded);
    }
    if request.document.contains('\r')
        || request.document.get(request.selection.clone()).is_none()
        || !request.operation.replaces() && !request.replacement.is_empty()
    {
        return Err(Error::UnsupportedRange);
    }
    if request.pattern.is_empty() {
        return Err(Error::Empty);
    }
    Ok(())
}

#[derive(Default)]
struct Writer(Vec<u8>);
impl Writer {
    fn flag(&mut self, n: u8) {
        self.0.push(n);
    }
    fn number(&mut self, n: usize) -> Result<(), Error> {
        self.0.extend_from_slice(
            &u32::try_from(n)
                .map_err(|_| Error::BudgetExceeded)?
                .to_le_bytes(),
        );
        Ok(())
    }
    fn span(&mut self, r: &Range<usize>) -> Result<(), Error> {
        self.number(r.start)?;
        self.number(r.end)
    }
    fn text(&mut self, s: &str) -> Result<(), Error> {
        self.number(s.len())?;
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let (value, rest) = self.0.split_at_checked(len).ok_or(Error::InvalidReply)?;
        self.0 = rest;
        Ok(value)
    }
    fn flag(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn number(&mut self) -> Result<usize, Error> {
        wire_size(self.take(4)?).ok_or(Error::InvalidReply)
    }
    fn span(&mut self) -> Result<Range<usize>, Error> {
        Ok(self.number()?..self.number()?)
    }
    fn text(&mut self, max: usize) -> Result<&'a str, Error> {
        let len = self.number()?;
        if len > max {
            return Err(Error::InvalidReply);
        }
        str::from_utf8(self.take(len)?).map_err(|_| Error::InvalidReply)
    }
    fn end(self) -> Result<(), Error> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidReply)
        }
    }
}

/// Literal upstream cursor heuristic, not a regex grammar decision.
pub(crate) fn multiline_pattern(pattern: &str) -> bool {
    let b = pattern.as_bytes();
    b.contains(&b'\n')
        || b.contains(&b'\r')
        || b.windows(2).any(|w| w == b"[^" || (w[0] == b'\\' && b"sWDnr".contains(&w[1])))
}

pub(crate) fn wire_size(header: &[u8]) -> Option<usize> {
    Some(u32::from_le_bytes(header.try_into().ok()?) as usize)
}

pub(crate) fn encode_request(request: &Request<'_>) -> Result<Vec<u8>, Error> {
    validate(request)?;
    let mut bytes = Writer::default();
    bytes.flag(VERSION);
    bytes.flag(request.operation as u8);
    bytes.flag(u8::from(request.case_sensitive) | (u8::from(request.whole_word) << 1));
    bytes.span(&request.selection)?;
    bytes.text(request.document)?;
    bytes.text(request.pattern)?;
    bytes.text(request.replacement)?;
    Ok(bytes.0)
}

pub(crate) fn decode_request(data: &[u8]) -> Result<Request<'_>, Error> {
    let mut r = Reader(data);
    if r.flag()? != VERSION {
        return Err(Error::InvalidReply);
    }
    let operation = match r.flag()? {
        0 => Operation::Next,
        1 => Operation::Previous,
        2 => Operation::SelectAll,
        3 => Operation::ReplaceNext,
        4 => Operation::ReplaceAll,
        _ => return Err(Error::InvalidReply),
    };
    let flags = r.flag()?;
    if flags > 3 {
        return Err(Error::InvalidReply);
    }
    let request = Request {
        operation,
        case_sensitive: flags & 1 != 0,
        whole_word: flags & 2 != 0,
        selection: r.span()?,
        document: r.text(DOC)?,
        pattern: r.text(PATTERN)?,
        replacement: r.text(TEMPLATE)?,
    };
    r.end()?;
    validate(&request)?;
    Ok(request)
}

pub(crate) fn encode_reply(
    operation: Operation,
    outcome: Result<Outcome, Error>,
) -> Result<Vec<u8>, Error> {
    let mut w = Writer::default();
    w.flag(VERSION);
    w.flag(operation as u8);
    match outcome {
        Ok(Outcome::Select(ranges)) => {
            w.flag(0);
            w.number(ranges.len())?;
            for r in &ranges {
                w.span(r)?;
            }
        }
        Ok(Outcome::Edits { edits, following }) => {
            w.flag(1);
            match following {
                Some(r) => {
                    w.flag(1);
                    w.span(&r)?;
                }
                None => w.flag(0),
            }
            w.number(edits.len())?;
            for Edit { range, insert } in &edits {
                w.span(range)?;
                w.text(insert)?;
            }
        }
        Err(error) => w.flag(error_tag(error)?),
    }
    if w.0.len() > MAX_REPLY {
        return Err(Error::BudgetExceeded);
    }
    Ok(w.0)
}

/// Never trust the child to validate its own result. Validate ranges against
/// exactly the original request (not a newer editor snapshot).
pub(crate) fn decode_reply(request: &Request<'_>, bytes: &[u8]) -> Result<Outcome, Error> {
    let mut r = Reader(bytes);
    if r.flag()? != VERSION || r.flag()? != request.operation as u8 {
        return Err(Error::InvalidReply);
    }
    let document = request.document;
    let outcome = match r.flag()? {
        0 => {
            let count = r.number()?;
            let max = match request.operation {
                Operation::SelectAll => SELECT_COUNT,
                Operation::ReplaceAll => 0,
                _ => 1,
            };
            if count > max {
                return Err(Error::InvalidReply);
            }
            let mut ranges = Vec::with_capacity(count);
            for _ in 0..count {
                let span = r.span()?;
                check_range(document, &span, ranges.last())?;
                ranges.push(span);
            }
            Outcome::Select(ranges)
        }
        1 if request.operation.replaces() => {
            let next = request.operation == Operation::ReplaceNext;
            let following = match r.flag()? {
                0 => None,
                1 if next => {
                    let span = r.span()?;
                    check_range(document, &span, None)?;
                    Some(span)
                }
                _ => return Err(Error::InvalidReply),
            };
            let count = r.number()?;
            if count == 0 || (next && count != 1) || count > REPLACE_COUNT {
                return Err(Error::InvalidReply);
            }
            let mut edits: Vec<Edit> = Vec::with_capacity(count);
            let (mut inserted, mut removed) = (0, 0);
            for _ in 0..count {
                let range = r.span()?;
                check_range(document, &range, edits.last().map(|edit| &edit.range))?;
                if next && range != request.selection {
                    return Err(Error::InvalidReply);
                }
                let insert = r.text(OUTPUT - inserted)?;
                if insert.contains('\r') {
                    return Err(Error::InvalidReply);
                }
                inserted += insert.len();
                removed += range.len();
                edits.push(Edit { range, insert: insert.to_owned() });
            }
            if document.len() - removed + inserted > OUTPUT {
                return Err(Error::InvalidReply);
            }
            Outcome::Edits { edits, following }
        }
        error => {
            r.end()?;
            return Err(read_error_tag(error));
        }
    };
    r.end()?;
    Ok(outcome)
}

fn error_tag(error: Error) -> Result<u8, Error> {
    Ok(match error {
        Error::Empty => 2,
        Error::InvalidPattern => 3,
        Error::UnsupportedRange => 4,
        Error::BudgetExceeded => 5,
        _ => return Err(Error::InvalidReply),
    })
}

fn read_error_tag(tag: u8) -> Error {
    match tag {
        2 => Error::Empty,
        3 => Error::InvalidPattern,
        4 => Error::UnsupportedRange,
        5 => Error::BudgetExceeded,
        _ => Error::InvalidReply,
    }
}

fn check_range(
    document: &str,
    range: &Range<usize>,
    previous: Option<&Range<usize>>,
) -> Result<(), Error> {
    if document.get(range.clone()).is_none()
        || previous.is_some_and(|p| range.start == p.start || range.start < p.end)
    {
        return Err(Error::InvalidReply);
    }
    Ok(())
}

pub(crate) fn write_frame(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let len = u32::try_from(bytes.len()).map_err(io::Error::other)?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(bytes)?;
    out.flush()
}

pub(crate) fn read_frame(input: &mut impl Read, max: usize) -> io::Result<Vec<u8>> {
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 || len > max {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut data = vec![0; len];
    input.read_exact(&mut data)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replace() -> Request<'static> {
        Request {
            operation: Operation::ReplaceNext,
            case_sensitive: true,
            whole_word: false,
            selection: 4..5,
            document: "😀xy",
            pattern: "x",
            replacement: "z",
        }
    }

    #[test]
    fn exact_request_round_trip_rejects_utf8_midpoint_and_tail() {
        let req = replace();
        let mut data = encode_request(&req).unwrap();
        let decoded = decode_request(&data).unwrap();
        assert_eq!(decoded.document, req.document);
        assert_eq!(decoded.selection, req.selection);
        data.push(0);
        assert!(matches!(decode_request(&data), Err(Error::InvalidReply)));
        assert_eq!(encode_request(&Request { selection: 1..1, ..req }), Err(Error::UnsupportedRange));
    }

    #[test]
    fn exact_reply_rejects_partial_trailing_wrong_operation_and_unsafe_edits() {
        let req = replace();
        let test = |range, insert: &str, following| {
            encode_reply(req.operation, Ok(Outcome::Edits {
                edits: vec![Edit { range, insert: insert.into() }],
                following,
            })).unwrap()
        };
        let valid = test(4..5, "x", Some(5..6));
        assert!(decode_reply(&req, &valid).is_ok());
        for bad in [
            test(1..5, "x", None),
            test(5..6, "x", None),
            test(4..5, "\r", None),
            test(4..5, "x", Some(0..1)),
            valid[..valid.len() - 1].to_vec(),
            [valid.clone(), vec![0]].concat(),
            encode_reply(Operation::Next, Ok(Outcome::Select(vec![4..5]))).unwrap(),
        ] {
            assert_eq!(decode_reply(&req, &bad), Err(Error::InvalidReply));
        }
    }
}
