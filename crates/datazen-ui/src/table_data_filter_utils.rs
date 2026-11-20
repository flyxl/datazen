//! Filter-draft helpers, ported from
//! `packages/frontend/src/stores/tableData/filterUtils.ts` (71 lines, T185
//! on this segment's ledger).
//!
//! The completeness predicate mirrors the back end's `filter_is_complete`
//! (`crates/datazen/src/table_browsing/executor/filter_sql`): the
//! operators that carry no value are complete on their own, the set
//! operators need at least one value, and the range operators need
//! exactly two bounds. A condition that is not complete is never dropped
//! silently — the editor marks it and blocks Apply, and the back end
//! refuses it with `grid.filter.incomplete`.
//!
//! ## Input boundary
//!
//! React interprets the condition's JSON value through
//! `valueInputFor(operator)` from `lib/filterOperators.ts` (T513,
//! segment-2). The mapping is DATA — four groups of operator ids — so
//! this port carries the minimal table
//! ([`value_input_for`], with React's unknown-operator degradation to
//! `single`); T513's full metadata port (operators-by-cell-type, the
//! driver whitelists) re-homes it beside its table when that task lands.

// The production consumer is the filter panel view (T106's slices); until
// then the component tests are the only callers.
#![allow(dead_code)]

use crate::components::filter::{FilterCondition, FilterConnector, FilterValue};

/// Which control a condition's value is edited with
/// (`lib/filterOperators.ts:30`): none, one literal, a pair
/// (`between`), or a token list (`in`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilterValueInputKind {
    None,
    Single,
    Pair,
    Tokens,
}

/// The operator rows of `FILTER_OPERATOR_META` (`filterOperators.ts:209`)
/// that the completeness predicate reads: which operators carry no value,
/// which take a pair, and which take a token list. Everything else is a
/// single literal.
pub(crate) fn value_input_for(operator: &str) -> FilterValueInputKind {
    match operator {
        "isNull" | "isNotNull" | "isEmpty" | "isNotEmpty" => FilterValueInputKind::None,
        "between" | "notBetween" => FilterValueInputKind::Pair,
        "in" | "notIn" => FilterValueInputKind::Tokens,
        // A persisted operator this build does not know degrades to a
        // single value field (React's `meta?.valueInput ?? 'single'`).
        _ => FilterValueInputKind::Single,
    }
}

/// `isCompleteFilter` (`filterUtils.ts:22`): whether a condition carries
/// everything it needs to be sent to the database.
pub(crate) fn is_complete_filter(filter: &FilterCondition) -> bool {
    if filter.column.is_empty() {
        return false;
    }
    // The no-value operators are complete before the value is even read.
    if value_input_for(filter.operator) == FilterValueInputKind::None {
        return true;
    }
    match &filter.value {
        // A missing value is incomplete (React's null/undefined).
        FilterValue::None => false,
        // A single literal: only the empty string counts as missing — a
        // number, a boolean or a JSON value is a value the database can
        // compare, and the port's typed value carries the same rule.
        FilterValue::Single(value) => match value_input_for(filter.operator) {
            FilterValueInputKind::Pair => complete_pair_str(value),
            FilterValueInputKind::Tokens => complete_token_str(value),
            _ => !value.is_empty(),
        },
        FilterValue::Pair(lower, upper) => {
            is_filled(lower.as_str()) && is_filled(upper.as_str())
        }
        FilterValue::Tokens(tokens) => match value_input_for(filter.operator) {
            // A pair operator fed a token list: React's array rule —
            // exactly two filled bounds.
            FilterValueInputKind::Pair => {
                tokens.len() == 2 && tokens.iter().all(|t| is_filled(t.as_str()))
            }
            _ => !tokens.is_empty(),
        },
    }
}

/// `isCompletePair`'s string form (`filterUtils.ts:39`): two comma parts,
/// both filled.
fn complete_pair_str(value: &str) -> bool {
    let parts: Vec<&str> = value.split(',').collect();
    parts.len() == 2 && parts.iter().all(|part| !part.trim().is_empty())
}

/// `isCompleteTokenList`'s string form (`filterUtils.ts:51`): any
/// non-empty comma part.
fn complete_token_str(value: &str) -> bool {
    value.split(',').any(|part| !part.trim().is_empty())
}

/// `isFilled` (`filterUtils.ts:57`): null/undefined and blank strings are
/// missing; everything else is a value.
fn is_filled(value: &str) -> bool {
    !value.trim().is_empty()
}

/// `cloneFilters` (`filterUtils.ts:47`).
pub(crate) fn clone_filters(filters: &[FilterCondition]) -> Vec<FilterCondition> {
    filters.to_vec()
}

/// `filterDraftEqualsApplied` (`filterUtils.ts:59`): the draft equals the
/// applied list — same wide logic and the same conditions in the same
/// order (React's `JSON.stringify` comparison is order-sensitive; the
/// typed conditions compare field by field with the same meaning).
pub(crate) fn filter_draft_equals_applied(
    draft_filters: &[FilterCondition],
    draft_logic: FilterConnector,
    applied_filters: &[FilterCondition],
    applied_logic: FilterConnector,
) -> bool {
    draft_logic == applied_logic && draft_filters == applied_filters
}
