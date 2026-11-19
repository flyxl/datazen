//! Row cap weaving — the React `lib/sqlDialects/rowLimit.ts` port (T123).
//!
//! The cap is a dialect fact, not a host decision: `LIMIT 200` is a
//! syntax error on SQL Server (`TOP 200` in the select list) and on
//! Oracle 12c+ (`FETCH FIRST 200 ROWS ONLY` at the end). The dialect
//! declares its own shape — see [`RowLimitShape`][super::strategy::RowLimitShape]
//! — and this module weaves that shape into the statement. The host
//! never spells `TOP` or `FETCH FIRST` itself, and never branches on a
//! driver name.

use super::strategy::{RowLimitPlacement, SqlDialectStrategy};

/// The standard cap: `LIMIT n` at the end of the statement.
pub(crate) const LIMIT_ROW_LIMIT: super::strategy::RowLimitShape =
    super::strategy::RowLimitShape {
        placement: RowLimitPlacement::Trailing,
        clause: limit_clause,
};

/// `LIMIT_ROW_LIMIT.clause`.
fn limit_clause(limit: usize) -> String {
    format!("LIMIT {limit}")
}

/// The parts of a statement the row cap is woven into.
pub(crate) struct RowLimitedSelectParts<'a> {
    /// Whether the statement selects distinct values.
    pub distinct: bool,
    /// The already-quoted column list.
    pub columns: &'a [String],
    /// The already-quoted table reference.
    pub table: &'a str,
    /// The number of rows the statement is capped at.
    pub limit: usize,
}

/// Build a `SELECT` whose row count is capped the way the dialect caps
/// it. `strategy` is the dialect's own strategy object; `None` means
/// "no dialect known", which falls back to the standard `LIMIT n` —
/// the shape every dialect that has not declared a cap of its own
/// accepts.
pub(crate) fn build_row_limited_select(
    strategy: Option<&SqlDialectStrategy>,
    parts: &RowLimitedSelectParts<'_>,
) -> String {
    let shape = strategy
        .map(|s| s.row_limit_shape())
        .unwrap_or(LIMIT_ROW_LIMIT);
    let head = if parts.distinct { "SELECT DISTINCT" } else { "SELECT" };
    let columns = parts.columns.join(", ");
    if shape.placement == RowLimitPlacement::SelectList {
        format!("{} {} {} FROM {}", head, (shape.clause)(parts.limit), columns, parts.table)
    } else {
        format!("{} {} FROM {} {}", head, columns, parts.table, (shape.clause)(parts.limit))
    }
}
