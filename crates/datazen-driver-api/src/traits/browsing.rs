//! The browsing half of the driver contract: row counts and default ordering.
//!
//! Two questions the host asks every driver before it reads a table:
//!
//! * **How many rows does this table have?** — a count that is exact, a
//!   catalog estimate, or nothing at all. The answer used to be a boolean
//!   (`skip_count_query`), which cannot tell "I do not count" apart from "I
//!   can only estimate"; the UI has to render the two differently.
//! * **In what order do the rows come back?** — when the user did not ask
//!   for an order, the host used to pick one itself. Which columns it picks
//!   is a driver decision, and the driver may also say "do not order at all".
//!
//! Every default here is chosen so that a driver that writes nothing behaves
//! exactly as it did before this contract existed:
//!
//! * `count_strategy` is derived from the pre-existing `skip_count_query`, so
//!   a driver that skips the count keeps skipping it and every other driver
//!   keeps counting exactly;
//! * `count_estimate_sql` returns `None`, so "I cannot estimate" never
//!   becomes a guess;
//! * `default_order_columns` reproduces the host's old three-step rule
//!   (primary key → first column → nothing);
//! * `allow_first_column_fallback_order` is `true`, so a table without a
//!   primary key is still ordered by its first column.

use crate::types::{ColumnSchema, CountStrategy};

/// The row-count strategy that follows from "this driver skips the count".
///
/// A driver that answers `true` to `skip_count_query` has no count to give,
/// so the strategy is [`CountStrategy::Unsupported`]; every other driver
/// counts exactly, which is what it has always done.
pub fn count_strategy(skip_count_query: bool) -> CountStrategy {
    if skip_count_query {
        CountStrategy::Unsupported
    } else {
        CountStrategy::Exact
    }
}

/// The statement that estimates the row count of a table, if there is one.
///
/// The default is `None`: a driver without an estimate statement has no
/// estimate, and the host then counts exactly rather than guess. The
/// arguments are the **unquoted** table name plus the explicit
/// database/schema, the same convention `get_columns` and
/// `get_table_schema` use — quoting is the driver's own business.
pub fn count_estimate_sql(_database: &str, _schema: Option<&str>, _table: &str) -> Option<String> {
    None
}

/// The columns to order by when the caller did not ask for an order.
///
/// The primary keys win; a table without a primary key falls back to its
/// first column; a table without columns gets no order at all. The primary
/// keys arrive as the **normalised** list (`TableSchema::effective_primary_keys`),
/// not as something re-derived from the column flags — the two can disagree,
/// and the normalised list is the one that names the real key.
pub fn default_order_columns(columns: &[ColumnSchema], primary_keys: &[String]) -> Vec<String> {
    if !primary_keys.is_empty() {
        return primary_keys.to_vec();
    }
    columns
        .first()
        .map(|c| c.name.clone())
        .into_iter()
        .collect()
}

/// Whether a table without a primary key may be ordered by its first column.
///
/// The default is `true`, which is what the host did before this method
/// existed. A driver that returns `false` gets an unordered read instead —
/// the host still adds whatever neutral ordering its pagination clause
/// needs, so the statement stays legal.
pub fn allow_first_column_fallback_order() -> bool {
    true
}
