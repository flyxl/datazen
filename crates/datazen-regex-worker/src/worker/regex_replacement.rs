//! Engine-neutral expansion of the locked CodeMirror Search 6.6.0 regex
//! replacement grammar. No regex matching, IO, transactions or UI here.
//! All spans use UTF-8 byte boundaries in one immutable *search* snapshot.
//! A later host owns projection back to the original editor bytes.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error { InvalidCapture, Limit }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LineEndings { #[cfg(test)] Raw, CodeMirrorLf }

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Replacement {
    pub(super) range: Range<usize>,
    pub(super) insert: String,
}

fn append(out: &mut String, text: &str, limit: usize) -> Result<(), Error> {
    if text.len() > limit.saturating_sub(out.len()) {
        return Err(Error::Limit);
    }
    out.push_str(text);
    Ok(())
}

fn unquote(template: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let replacement = match chars.peek() {
                Some('n') => Some('\n'),
                Some('r') => Some('\r'),
                Some('t') => Some('\t'),
                Some('\\') => Some('\\'),
                _ => None,
            };
            if let Some(decoded) = replacement {
                chars.next();
                out.push(decoded);
                continue;
            }
        }
        out.push(ch);
    }
    out
}

/// Use capture slots in engine-number order: 0 is the exact whole hit,
/// None is a valid unmatched group. Lookaround may capture outside slot 0.
/// Reject every malformed supplied range before expansion, even unused ones.
pub(super) fn expand(
    source: &str, whole: Range<usize>, slots: &[Option<Range<usize>>], template: &str,
    template_limit: usize, output_limit: usize, endings: LineEndings,
) -> Result<Replacement, Error> {
    if slots.first().and_then(Option::as_ref) != Some(&whole)
        || slots.iter().flatten().any(|range| source.get(range.clone()).is_none())
    {
        return Err(Error::InvalidCapture);
    }
    if template.len() > template_limit {
        return Err(Error::Limit);
    }
    let template = unquote(template);
    let input = template.as_bytes();
    let mut out = String::new();
    let mut start = 0;
    let mut at = 0;
    while at < input.len() {
        if input[at] != b'$' {
            at += 1;
            continue;
        }
        let next = input.get(at + 1).copied();
        let (end, emitted, suffix) = match next {
            Some(b'$') => (at + 2, "$", ""),
            Some(b'&') => (at + 2, &source[whole.clone()], ""),
            Some(b'0'..=b'9') => {
                let digits = at + 1;
                let mut end = digits;
                let mut value = 0usize;
                let mut best = None;
                while input.get(end).is_some_and(u8::is_ascii_digit) {
                    let digit = usize::from(input[end] - b'0');
                    // CodeMirror tries the longest valid positive decimal
                    // prefix. Once a prefix is outside the groups, further
                    // digits cannot make it valid; leading zeroes remain valid.
                    value = value.saturating_mul(10).saturating_add(digit).min(slots.len());
                    end += 1;
                    if value != 0 && value < slots.len() {
                        best = Some((value, end));
                    }
                }
                if let Some((index, split)) = best {
                    let resolved = slots[index].as_ref()
                        .map_or("undefined", |range| &source[range.clone()]);
                    (end, resolved, &template[split..end])
                } else {
                    (end, &template[at..end], "")
                }
            }
            _ => {
                // In particular, CodeMirror leaves $<name> literal.
                at += 1;
                continue;
            }
        };
        append(&mut out, &template[start..at], output_limit)?;
        append(&mut out, emitted, output_limit)?;
        append(&mut out, suffix, output_limit)?;
        start = end;
        at = end;
    }
    append(&mut out, &template[start..], output_limit)?;
    // CodeMirror's editor change construction normalizes CR and CRLF to LF.
    // Do this only after expansion so adjacent fragments share the same rule.
    if endings == LineEndings::CodeMirrorLf && out.contains('\r') {
        out = out.replace("\r\n", "\n").replace('\r', "\n");
    }
    Ok(Replacement { range: whole, insert: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_prefix_lookaround_and_raw_or_normalized() {
        let source = "🦀cat😀";
        let slots = [Some(4..7), Some(0..4), None, Some(7..11)];
        let template = r"$&|$$|$1|$2|$12|$03|$00001|$0|$<a>|\r\n\t";
        let raw = expand(source, 4..7, &slots, template, 100, 100, LineEndings::Raw).unwrap();
        assert_eq!(raw.range, 4..7);
        assert_eq!(raw.insert, "cat|$|🦀|undefined|🦀2|😀|🦀|$0|$<a>|\r\n\t");
        let edit = expand(source, 4..7, &slots, template, 100, 100, LineEndings::CodeMirrorLf).unwrap();
        assert_eq!(edit.insert, raw.insert.replace("\r\n", "\n"));
    }

    #[test]
    fn invalid_and_oversized_inputs_produce_no_edit() {
        let source = "a🐈z";
        assert_eq!(expand(source, 1..5, &[Some(1..5), Some(2..5)], "$&", 100, 100, LineEndings::Raw),
                   Err(Error::InvalidCapture));
        assert_eq!(expand(source, 0..1, &[Some(1..5)], "$&", 100, 100, LineEndings::Raw),
                   Err(Error::InvalidCapture));
        assert_eq!(expand(source, 1..5, &[Some(1..5)], "$&$&", 100, 7, LineEndings::Raw),
                   Err(Error::Limit));
        assert_eq!(expand(source, 1..5, &[Some(1..5)], "$&$&", 3, 100, LineEndings::Raw),
                   Err(Error::Limit));
    }
}
