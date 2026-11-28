use std::{io::{self, Read, Write}, ops::Range, process::ExitCode};

use crate::{Edit, Error, HIGHLIGHT_MAX_MARKS, HighlightRequest, Operation, Outcome, Request, isolation, protocol};

mod named_backref;
mod regex_cursor;
mod regex_replacement;
mod regex_whole_word;
mod whole_word_data;
use regex_cursor::{Hit, Policy, Window};

/// Trusted early child entry. Empty environment and descriptor restrictions
/// must hold before emitting the readiness byte or accepting any request.
pub fn worker_main() -> ExitCode {
    if std::env::vars_os().next().is_some() || isolation::confine().is_err() {
        return ExitCode::FAILURE;
    }
    // No unwinding, formatting, filesystem, thread startup or re-entrancy on
    // errors after sandboxing; the parent's structural checks reject any
    // nonzero exit code or truncated result.
    if serve().is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn serve() -> io::Result<()> {
    let mut output = io::stdout().lock();
    output.write_all(&[protocol::READY])?;
    output.flush()?;
    let mut input = io::stdin().lock();
    let bytes = protocol::read_frame(&mut input, protocol::MAX_REQUEST)?;
    if input.read(&mut [0])? != 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let reply = if protocol::passive::is_highlight(&bytes) {
        let (request, window) = protocol::passive::decode_request(&bytes)
            .map_err(|_| io::ErrorKind::InvalidData)?;
        protocol::passive::encode_reply(mark(&request, window))
    } else {
        let request = protocol::decode_request(&bytes).map_err(|_| io::ErrorKind::InvalidData)?;
        protocol::encode_reply(request.operation, execute(&request))
    }.map_err(|_| io::ErrorKind::InvalidData)?;
    protocol::write_frame(&mut output, &reply)
}

fn engine(pattern: &str, case_sensitive: bool) -> Result<regress::Regex, Error> {
    let compiled = regress::Regex::with_flags(
        pattern,
        regress::Flags { unicode: true, multiline: true, icase: !case_sensitive, ..Default::default() },
    ).map_err(|_| Error::InvalidPattern)?;
    named_backref::check(pattern)?;
    Ok(compiled)
}

fn mark(request: &HighlightRequest<'_>, window: Range<usize>) -> Result<Vec<Range<usize>>, Error> {
    let engine = engine(request.pattern, request.case_sensitive)?;
    let mut policy = Policy::new(request.document, request.pattern, request.whole_word, 32_768)
        .map_err(cursor_error)?;
    let mut conversion = WindowCache::default();
    let mut call = |window: Window<'_>| conversion.search(&engine, window, false);
    let hits = policy.highlight(window, HIGHLIGHT_MAX_MARKS, &mut call).map_err(cursor_error)?;
    Ok(hits.into_iter().map(|hit| hit.range).collect())
}

fn execute(request: &Request<'_>) -> Result<Outcome, Error> {
    let engine = engine(request.pattern, request.case_sensitive)?;
    let mut policy = Policy::new(
        request.document, request.pattern, request.whole_word, 32_768,
    ).map_err(cursor_error)?;
    let mut conversion = WindowCache::default();
    let mut call = |window: Window<'_>| conversion.search(&engine, window, request.operation.replaces());
    let selected = request.selection.clone();
    let (hits, following) = match request.operation {
        Operation::Next => return select(policy.next(selected.end, selected.end, &mut call)),
        Operation::Previous => return select(policy.previous(selected.start, &mut call)),
        Operation::SelectAll => {
            let hits = policy.all(protocol::SELECT_COUNT, &mut call).map_err(cursor_error)?;
            return Ok(Outcome::Select(hits.into_iter().map(|hit| hit.range).collect()));
        }
        Operation::ReplaceNext => {
            let hit = policy.next(selected.start, selected.start, &mut call).map_err(cursor_error)?;
            let Some(hit) = hit else { return Ok(Outcome::Select(vec![])); };
            if hit.range != selected {
                return Ok(Outcome::Select(vec![hit.range]));
            }
            let following = policy.next(selected.start, selected.end, &mut call)
                .map_err(cursor_error)?.map(|hit| hit.range);
            (vec![hit], following)
        }
        Operation::ReplaceAll => {
            let hits = policy.all(protocol::REPLACE_COUNT, &mut call).map_err(cursor_error)?;
            if hits.is_empty() { return Ok(Outcome::Select(vec![])); }
            (hits, None)
        }
    };
    let (mut inserted, mut removed) = (0, 0);
    let mut edits: Vec<Edit> = Vec::with_capacity(hits.len());
    for hit in hits {
        if edits.last().is_some_and(|last| hit.range.start == last.range.start || hit.range.start < last.range.end) {
            return Err(Error::UnsupportedRange);
        }
        let mut slots = Vec::with_capacity(1 + hit.captures.len());
        slots.push(Some(hit.range.clone()));
        slots.extend(hit.captures);
        let edit = regex_replacement::expand(request.document, hit.range, &slots, request.replacement,
            protocol::TEMPLATE, protocol::OUTPUT - inserted, regex_replacement::LineEndings::CodeMirrorLf,
        ).map_err(|error| match error {
            regex_replacement::Error::Limit => Error::BudgetExceeded,
            regex_replacement::Error::InvalidCapture => Error::UnsupportedRange,
        })?;
        inserted += edit.insert.len();
        removed += edit.range.len();
        edits.push(Edit { range: edit.range, insert: edit.insert });
    }
    if request.document.len() - removed + inserted > protocol::OUTPUT {
        return Err(Error::BudgetExceeded);
    }
    Ok(Outcome::Edits { edits, following })
}

fn select(result: Result<Option<Hit>, regex_cursor::Error>) -> Result<Outcome, Error> {
    Ok(Outcome::Select(result.map_err(cursor_error)?.into_iter().map(|hit| hit.range).collect()))
}

fn cursor_error(error: regex_cursor::Error) -> Error {
    match error {
        regex_cursor::Error::Bounds => Error::UnsupportedRange,
        regex_cursor::Error::Limit => Error::BudgetExceeded,
        regex_cursor::Error::Empty => Error::Empty,
    }
}

/// The same shrinking/per-line window and its offsets recur within a command.
/// Don't walk the whole UTF-8/UTF-16 prefix for every capture of every hit.
#[derive(Default)]
struct WindowCache {
    key: Option<(usize, usize)>,
    utf16: Vec<u16>,
    offsets: Vec<Option<usize>>,
}

impl WindowCache {
    fn search(&mut self, engine: &regress::Regex, window: Window<'_>, captures: bool) -> Result<Option<Hit>, regex_cursor::Error> {
        let input = window.input;
        let key = (input.as_ptr() as usize, input.len());
        if self.key != Some(key) {
            self.key = Some(key);
            self.utf16.clear();
            self.utf16.extend(input.encode_utf16());
            self.offsets.clear();
            for (byte, ch) in input.char_indices() {
                self.offsets.push(Some(byte));
                if ch.len_utf16() == 2 { self.offsets.push(None); }
            }
            self.offsets.push(Some(input.len()));
        }
        let local_range = |range: std::ops::Range<usize>| {
            let byte = |u| self.offsets.get(u).copied().flatten().ok_or(regex_cursor::Error::Bounds);
            Ok(byte(range.start)?..byte(range.end)?)
        };
        let mut start = window.last_index_utf16;
        if self.offsets.get(start) == Some(&None) { start -= 1; }
        let Some(hit) = engine.find_from_utf16(&self.utf16, start).next() else { return Ok(None); };
        let range = local_range(hit.range)?;
        let captures = if captures {
            // Bound stored capture slots across a worst-case 4096-hit reply.
            // Regress allocates its own match within the sandbox memory cap.
            if hit.captures.len() > 128 { return Err(regex_cursor::Error::Limit); }
            hit.captures.into_iter().map(|r| r.map(local_range).transpose()).collect::<Result<_, _>>()?
        } else {
            vec![]
        };
        Ok(Some(Hit { range, captures }))
    }
}
