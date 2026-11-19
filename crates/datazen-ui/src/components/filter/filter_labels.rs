//! Port of `packages/frontend/src/components/filter/filterLabels.ts`.
//!
//! One place for the "column operator value" text shared by the collapsed chip,
//! the expanded editor and the panel summary. The value parsers are the ones
//! T216 owns in [`super::filter_values`], so a comma-separated legacy draft
//! reads the same everywhere instead of being rendered as an empty value.
//!
//! React injects `t`; Rune asks [`crate::i18n::t`] directly with the exact
//! `filter.<operator>` key. No key is invented: an unknown operator falls back
//! to the key itself, which is React's key-style test contract too.

use crate::i18n::t;
use crate::support::formatters::format_cell;

use super::filter_condition_chip::{FilterCondition, FilterValue};
use super::filter_operators::{value_input_for, FilterValueInputKind};
use super::filter_values::{to_pair, to_tokens};

/// React `opLabel`: the translated label of an operator, e.g. `filter.eq`.
#[allow(dead_code)] // label contract; consumers are the filter chip and tests.
pub(crate) fn op_label(operator: &str) -> String {
    t(&format!("filter.{operator}")).to_owned()
}

/// React `conditionSummary`: the text of one condition, e.g. `name = alice`.
#[allow(dead_code)] // label contract; consumers are the filter chip and tests.
pub(crate) fn condition_summary(filter: &FilterCondition) -> String {
    let op = op_label(filter.operator);
    match value_input_for(filter.operator) {
        FilterValueInputKind::None => format!("{} {}", filter.column, op),
        FilterValueInputKind::Pair => {
            let (lower, upper) = to_pair(&filter.value);
            format!("{} {} {}, {}", filter.column, op, lower, upper)
        }
        FilterValueInputKind::Tokens => {
            format!("{} {} {}", filter.column, op, to_tokens(&filter.value).join(", "))
        }
        FilterValueInputKind::Single => {
            // React elides only a null / undefined / empty value. The typed
            // mirror has no null variant: `None` is the "no value" case and is
            // handled by the `none` operator kind above; `Single("")` is the
            // empty value React renders as an ellipsis.
            let shown = match &filter.value {
                FilterValue::None => "…".to_owned(),
                FilterValue::Single(value) if value.is_empty() => "…".to_owned(),
                // `formatCell` on a string returns the string unchanged; route
                // through the real formatter so the contract stays in one place.
                FilterValue::Single(value) => format_cell(&serde_json::Value::String(value.clone())),
                FilterValue::Pair(lower, upper) => {
                    format_cell(&serde_json::Value::String(format!("{lower},{upper}")))
                }
                FilterValue::Tokens(tokens) => {
                    format_cell(&serde_json::Value::String(tokens.join(",")))
                }
            };
            format!("{} {} {}", filter.column, op, shown)
        }
    }
}

// Leftover L1 (removal stage): when the filter panel view mounts the chip and
// editor directly, keep this module as the single label formatter and drop the
// blanket allows above; no other definition of `conditionSummary` should be
// added in the panel layer.
