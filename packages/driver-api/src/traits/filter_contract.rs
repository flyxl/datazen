//! The filter-operator half of the driver contract.
//!
//! Filtering is split in two: the **shared** operators are rendered by the
//! host, because every ANSI-ish dialect can express them, and the
//! **dialect-specific** ones are rendered by the driver that declares them.
//! This module holds the default behaviour of both halves, so the trait
//! declarations in `traits.rs` stay one-liners and the reasoning — which
//! driver authors need most — lives in one place.
//!
//! The defaults are chosen so that a driver that writes nothing behaves
//! exactly as it did before this contract existed:
//!
//! * the whitelist is the shared set, so the host renders every operator a
//!   driver did not think about;
//! * the dialect renderer returns `None`, so an operator nobody implements is
//!   **rejected** before the statement reaches the database rather than
//!   silently dropped;
//! * `LIKE` escaping is off, so a `%` the user typed stays a wildcard.

use crate::filters::{FilterOperator, SHARED_FILTER_OPERATORS};

/// The operators a driver supports unless it says otherwise.
///
/// The default is the **shared** set — the ten operators that already shipped
/// plus the nine shared extensions — and not the ten operators that shipped
/// before the extension. Every shared operator is rendered by the host, so
/// claiming one costs the driver nothing; only the three dialect-specific
/// operators have to be declared explicitly.
///
/// A driver that cannot serve one of the shared operators overrides this with
/// a narrower list; the host then rejects the operator before it reaches the
/// database.
pub fn supported_filter_operators() -> &'static [FilterOperator] {
    SHARED_FILTER_OPERATORS
}

/// Render a dialect-specific operator, if this dialect has one.
///
/// `column_expr` is already quoted and `literal` is already rendered by the
/// driver's literal formatter, so a driver only has to supply the operator
/// syntax between them. The default is `None`: the host then rejects the
/// operator with `grid.filter.unsupportedOperator` instead of silently
/// dropping the condition.
pub fn filter_operator_sql(
    operator: FilterOperator,
    column_expr: &str,
    literal: &str,
) -> Option<String> {
    let _ = (operator, column_expr, literal);
    None
}

/// Whether `LIKE` is case-insensitive under this driver's default collation.
///
/// This only drives a hint in the UI; it never changes the rendered SQL. The
/// default is `false`, because "case-sensitive" is not a fact about the
/// column — a column-level `COLLATE` can still change it.
pub fn like_is_case_insensitive() -> bool {
    false
}

/// The escape character this dialect uses for `LIKE` patterns.
///
/// `None` means "do not escape at all", which is exactly what the host did
/// before this method existed: a `%` the user typed stayed a wildcard.
pub fn like_pattern_escape() -> Option<char> {
    None
}

/// The `ESCAPE '<char>'` clause this dialect accepts, if any.
///
/// This must be configured **together with** [`like_pattern_escape`]: an
/// escape character that is in force without the matching clause means the
/// dialect treats that character literally, so an escaped `%` degrades back
/// into a wildcard — worse than not escaping at all, because the query still
/// succeeds and returns the wrong rows.
pub fn like_escape_clause(escape_char: char) -> Option<&'static str> {
    let _ = escape_char;
    None
}
