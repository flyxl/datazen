//! The value shapes a filter condition carries, ported 1:1 from
//! `packages/frontend/src/components/filter/filterValues.ts`.
//!
//! A condition's value is one shape per operator, but older drafts (and the
//! back end's comma separated compatibility form) carry the same information in
//! a string. The panel reads both forms, so the parsing lives in **one** place:
//! the editor and the summary must never disagree about what a value says.
//!
//! ## Home and removal stage
//!
//! The React file is `components/filter/filterValues.ts`, owned by T216
//! (`components/filter/types.ts` + value shapes). It lives next to the editor
//! until then because T056 owns `components/filter/` and
//! [`super::filter_condition_editor::FilterConditionEditor`] is its only
//! consumer. T216 moves the two functions into its own module and deletes this
//! copy.

// Dead-code lint is silenced per item below: T216 owns this file's permanent
// home and deletes these two local copies when its module lands.

use super::filter_condition_chip::FilterValue;

use super::filter_operators::split_tokens;

/// The value of a set operator (`in` / `notIn`) as a list of strings.
#[allow(dead_code)] // T216 owns this helper; delete this copy when its module lands.
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
#[allow(dead_code)] // T216 owns this helper; delete this copy when its module lands.
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
