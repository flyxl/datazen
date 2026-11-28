//! Compatibility gate for regress 0.12.0. It lowers a named reference to
//! duplicate captures into a left-first alternation of numeric references.
//! Unmatched numeric captures match empty, so this can silently choose an
//! empty branch rather than the participating capture (ECMAScript semantics).
//! This is not a second regexp parser. Compile first with the real /u engine.
//! Walk only syntax that can introduce names, and canonicalize their escapes.

use crate::Error;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check(pattern: &str) -> Result<(), Error> {
    // No syntactic reference can occur without this exact source sequence.
    if !pattern.contains("\\k<") {
        return Ok(());
    }
    let mut rest = pattern;
    let mut class = false;
    let mut captures = BTreeMap::new();
    let mut references = BTreeSet::new();
    while let Some(ch) = pop(&mut rest) {
        match ch {
            '\\' => {
                // Outside a /u class, k<...> is a named reference. In a
                // class or on an escaped backslash it cannot introduce one.
                if pop(&mut rest) == Some('k') && !class && take(&mut rest, "<") {
                    references.insert(name(&mut rest)?);
                }
            }
            '[' if !class => class = true,
            ']' if class => class = false,
            '(' if !class && take(&mut rest, "?<") => {
                if !rest.starts_with(['=', '!']) {
                    let count = captures.entry(name(&mut rest)?).or_insert(0usize);
                    *count += 1;
                }
            }
            _ => {}
        }
    }
    if references.iter().any(|value| captures.get(value).is_some_and(|count| *count > 1)) {
        return Err(Error::UnsupportedPattern);
    }
    Ok(())
}

fn pop(rest: &mut &str) -> Option<char> {
    let c = rest.chars().next()?;
    *rest = &rest[c.len_utf8()..];
    Some(c)
}

fn take(rest: &mut &str, prefix: &str) -> bool {
    if let Some(tail) = rest.strip_prefix(prefix) {
        *rest = tail;
        true
    } else {
        false
    }
}

fn name(rest: &mut &str) -> Result<String, Error> {
    let mut name = String::new();
    loop {
        match pop(rest) {
            Some('>') if !name.is_empty() => return Ok(name),
            Some('\\') if take(rest, "u") => {
                let value = unicode_escape(rest).ok_or(Error::InvalidPattern)?;
                // Only a literal > terminates an IdentifierName. In
                // particular, don't trust an engine treating \u003e as >.
                if value == '>' {
                    return Err(Error::InvalidPattern);
                }
                name.push(value);
            }
            Some(c) if !matches!(c, '\\' | '>' | '[' | ']' | '(' | ')') => name.push(c),
            _ => return Err(Error::InvalidPattern),
        }
    }
}

fn unicode_escape(rest: &mut &str) -> Option<char> {
    if take(rest, "{") {
        let (digits, tail) = rest.split_once('}')?;
        let code = hex(digits)?;
        let value = char::from_u32(code)?;
        *rest = tail;
        return Some(value);
    }
    let head = short_escape(rest)?;
    if (0xd800..=0xdbff).contains(&head) {
        if !take(rest, "\\u") {
            return None;
        }
        return char::decode_utf16([head, short_escape(rest)?]).next()?.ok();
    }
    char::from_u32(u32::from(head))
}

fn short_escape(rest: &mut &str) -> Option<u16> {
    let value = hex(rest.get(..4)?)?;
    *rest = &rest[4..];
    u16::try_from(value).ok()
}

fn hex(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(text, 16).ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::engine;

    #[test]
    fn refused_only_for_reference_to_duplicate_canonical_names() {
        for raw in [
            r"(?:(?<k>a)|(?<k>b))\k<k>",
            r"\k<\u006b>(?:(?<k>a)|(?<\u{00006b}>b))",
            r"(?:(?<𐐀>a)|(?<\uD801\uDC00>b))\k<\u{10400}>",
            r"(?:(?<$a>a)|(?<\u0024a>b))\k<$a>",
            r"(?:(?<字>a)|(?<\u5b57>b))(?<=b)\k<字>",
        ] {
            assert!(matches!(engine(raw, true), Err(Error::UnsupportedPattern)), "{raw}");
        }
    }

    #[test]
    fn real_names_classes_and_escape_boundaries_do_not_overreject() {
        for valid in [
            r"(?<k>b)\k<k>",
            r"(?<k>b)(?<q>a)\k<q>",
            r"(?:(?<k>a)|(?<k>b))",
            r"(?:(?<k>a)|(?<k>b))\2",
            r"(?:(?<k>a)|(?<k>b))(?<x>x)\k<x>",
            r"(?:(?<k>a)|(?<k>b))\\k<k>",
            r"(?:(?<k>a)|(?<k>b))[\\k<k>]",
            r"(?<k>a)[\][(<?k>]+\k<k>",
            r"\(\?<k>b(?<k>a)\k<k>",
            r"(?<=a)(?<!b)(?<x>c)\k<x>",
            r"(?<字>a)(?<\u5b58>b)\k<\u5b57>",
            r"(?<a>a)(?<A>b)\k<A>",
        ] {
            assert!(engine(valid, false).is_ok(), "{valid}");
        }
        assert!(matches!(engine(r"(?<x>a)(?<x>b)\k<x>",true),Err(Error::InvalidPattern)));
        assert!(matches!(engine(r"(?<x>a)\k<y>",true),Err(Error::InvalidPattern)));
    }
}
