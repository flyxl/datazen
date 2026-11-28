//! Compatibility gates for the pinned regress 0.12.0 /u parser and VM.
//!
//! The parser's duplicate check compares branch numbers at equal *depth*,
//! which cannot prove that captures in concatenated sibling groups are
//! disjoint. Reject declarations that share a name but not an enclosing
//! disjunction with different branches (ECMAScript MightBothParticipate).
//! Even valid disjoint names with a reference must still be refused: the
//! current VM can compare an unmatched group's empty value instead of the
//! participating capture. Compile first and walk only structural tokens in
//! the validated /u syntax. This does not extend to /v nested classes.

use crate::Error;
use std::collections::{BTreeMap, BTreeSet};

type Branch = (usize, usize); // Unique disjunction identity and branch ordinal.

fn disjoint(a: &[Branch], b: &[Branch]) -> bool {
    a.iter().zip(b).any(|(&(a_id,a_branch), &(b_id,b_branch))|
        a_id == b_id && a_branch != b_branch)
}

pub(super) fn check(pattern: &str) -> Result<(), Error> {
    // Both the ordinary parser and this guard require a literal opener.
    if !pattern.contains("(?<") {
        return Ok(());
    }
    let mut rest = pattern;
    let mut class = false;
    let mut captures: BTreeMap<String, Vec<Vec<Branch>>> = BTreeMap::new();
    let mut references = BTreeSet::new();
    let mut path = vec![(0,0)]; // The top-level disjunction.
    let mut next_disjunction = 1;
    // The wire caps pattern bytes at 1024 prior to compilation. Each group
    // enters one path, each capture stores its current prefix, and each
    // same-name comparison is bounded by that prefix length.
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
            '(' if !class => {
                if take(&mut rest, "?<") && !rest.starts_with(['=', '!']) {
                    let previous = captures.entry(name(&mut rest)?).or_default();
                    // Depth alone is not proof. Disjunction identities must
                    // be the same and each capture must be in its own arm.
                    if previous.iter().any(|other| !disjoint(other,&path)) {
                        return Err(Error::InvalidPattern);
                    }
                    previous.push(path.clone());
                }
                path.push((next_disjunction,0));
                next_disjunction += 1;
            }
            ')' if !class => {
                if path.len() == 1 { return Err(Error::InvalidPattern); }
                path.pop();
            }
            '|' if !class => path.last_mut().ok_or(Error::InvalidPattern)?.1 += 1,
            _ => {}
        }
    }
    if class || path.len() != 1 { return Err(Error::InvalidPattern); }
    if references.iter().any(|value| captures.get(value).is_some_and(|paths| paths.len() > 1)) {
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
    fn sibling_branch_numbers_and_lookaround_do_not_prove_exclusivity() {
        for raw in [
            r"(?:(?<k>a)|x)(?:y|(?<k>b))",
            r"(?:(?<k>a)|x)(?:y|(?<k>b))\k<k>",
            r"(?:(?<k>a)|x)(?:y|(?<\u006b>b))",
            r"(?:(?<𐐀>a)|x)(?:y|(?<\uD801\uDC00>b))",
            r"(?:(?<k>a)|x)(?:y|(?=(?<k>b))b)",
            r"(?<k>a)(?<k>b)",
        ] {
            assert!(matches!(engine(raw,true), Err(Error::InvalidPattern)), "{raw}");
        }
    }

    #[test]
    fn actual_disjoint_branches_may_share_names_if_not_referenced() {
        for raw in [
            r"(?<k>a)|(?<k>b)",
            r"(?:(?:(?<k>a))x|y(?:(?<k>b)))",
            r"(?:(?<𐐀>a)[|]|\|(?<\u{10400}>b))",
            r"(?:(?=(?<k>a))a|(?=(?<k>b))b)",
            r"(?:(?<k>a)|(?<k>b))(?<other>c)\k<other>",
        ] {
            assert!(engine(raw,false).is_ok(), "{raw}");
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
