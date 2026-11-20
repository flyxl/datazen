//! The value shapes a filter condition carries, ported 1:1 from
//! `packages/frontend/src/components/filter/filterValues.ts`.
//!
//! A condition's value is one shape per operator, but older drafts (and the
//! back end's comma separated compatibility form) carry the same information in
//! a string. The panel reads both forms, so the parsing lives in **one** place:
//! the editor and the summary must never disagree about what a value says.
//!
//! ## Home
//!
//! This is T216's permanent home for the React `filterValues.ts` contract. The
//! typed [`FilterValue`] mirror maps onto React's `Value` forms as follows:
//!
//! | React `Value`            | Rune `FilterValue`     |
//! |--------------------------|------------------------|
//! | `null` / `undefined`     | `None`                 |
//! | scalar (`string`/number)  | `Single(text)`         |
//! | array                     | `Tokens(..)` / `Pair(..)` |
//!
//! The two functions below preserve React's exact boundary behaviour: an array
//! keeps every entry (including empty strings), a comma string is split,
//! trimmed and has its empty parts dropped, and `null` is always empty.

use super::filter_condition_chip::FilterValue;

use super::filter_operators::split_tokens;

/// The value of a set operator (`in` / `notIn`) as a list of strings.
pub(crate) fn to_tokens(value: &FilterValue) -> Vec<String> {
    match value {
        FilterValue::Tokens(tokens) => tokens.clone(),
        // React treats an array as the list itself, whatever produced it.
        FilterValue::Pair(lower, upper) => vec![lower.clone(), upper.clone()],
        FilterValue::Single(text) => split_tokens(text),
        FilterValue::None => Vec::new(),
    }
}

/// The value of a range operator (`between` / `notBetween`) as its two bounds.
pub(crate) fn to_pair(value: &FilterValue) -> (String, String) {
    match value {
        FilterValue::Pair(lower, upper) => (lower.clone(), upper.clone()),
        FilterValue::Tokens(tokens) => (
            tokens.first().cloned().unwrap_or_default(),
            tokens.get(1).cloned().unwrap_or_default(),
        ),
        FilterValue::Single(text) => match text.split_once(',') {
            Some((lower, upper)) => (lower.trim().to_owned(), upper.trim().to_owned()),
            None => (text.clone(), String::new()),
        },
        FilterValue::None => (String::new(), String::new()),
    }
}
