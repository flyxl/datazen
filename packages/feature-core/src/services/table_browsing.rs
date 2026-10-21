//! The two decisions a table read makes before it runs a statement.
//!
//! * **How many rows does this table have?** — exactly, approximately, or not
//!   at all. The host used to know only "count or do not count"; the driver
//!   now declares which of the three it can serve.
//! * **In what order do the rows come back?** — when the caller did not ask
//!   for an order, the host used to pick one itself. Which columns it picks
//!   is a driver decision.
//!
//! Both decisions live here, as pure functions, so that every host (desktop
//! today, web later) applies the same rules and so that the rules can be
//! tested without a database.
//!
//! # The primary key has exactly one source
//!
//! The order of a table without an explicit sort is decided by the driver's
//! `default_order_columns`, and the primary keys it is given are the
//! **normalised** ones (`TableSchema::effective_primary_keys`): the
//! `primary_keys` field of the schema, or — when that field is empty — the
//! columns that carry the flag.
//!
//! The host used to re-derive the key from the column flags on its own, which
//! is a second source for the same fact, and the two can disagree: a driver
//! that fills `primary_keys` but forgets the flag on the column silently lost
//! its key and fell back to ordering by the first column. This module takes
//! the normalised list and never re-derives it; `pk_sources_disagree` exists
//! to make the remaining disagreement visible.

use datazen_driver_api::{ColumnSchema, CountStrategy, DatabaseDriver, Value};

use super::query_executor::filter_sql::filter_is_complete;
use super::query_executor::FilterCondition;

/// The row-count strategy a read actually uses.
///
/// * `skip_count` — the caller asked to reuse the cached total, so not a
///   single count statement may be sent. The answer is
///   [`CountStrategy::Unsupported`]; the caller keeps the total it already
///   has, which is why the *response* carries no kind at all in that case.
/// * `requested` — what the caller asked for. A driver that declares
///   [`CountStrategy::Unsupported`] is never overruled: "I do not count" is a
///   statement about the database, not a preference.
/// * otherwise the driver's own declaration, degraded when the driver cannot
///   keep its promise (an estimate it has no statement for).
/// * and finally the filters: an estimate is a *table* level statistic, so it
///   says nothing about the rows a `WHERE` clause selects. A read with
///   effective filters counts exactly.
#[allow(clippy::too_many_arguments)]
pub fn resolve_count_strategy(
    driver: &dyn DatabaseDriver,
    requested: Option<CountStrategy>,
    skip_count: bool,
    has_effective_filters: bool,
    database: &str,
    schema: Option<&str>,
    table: &str,
) -> CountStrategy {
    if skip_count {
        return CountStrategy::Unsupported;
    }

    let declared = driver.count_strategy();
    let strategy = match requested {
        Some(CountStrategy::Unsupported) => CountStrategy::Unsupported,
        Some(CountStrategy::Exact) => {
            if declared == CountStrategy::Unsupported {
                CountStrategy::Unsupported
            } else {
                CountStrategy::Exact
            }
        }
        Some(CountStrategy::Estimated) => {
            if declared == CountStrategy::Unsupported {
                CountStrategy::Unsupported
            } else if !driver_can_estimate(driver, database, schema, table) {
                CountStrategy::Exact
            } else {
                CountStrategy::Estimated
            }
        }
        None => {
            if declared == CountStrategy::Estimated
                && !driver_can_estimate(driver, database, schema, table)
            {
                CountStrategy::Exact
            } else {
                declared
            }
        }
    };

    if strategy == CountStrategy::Estimated && has_effective_filters {
        // The estimate is a catalog statistic of the whole table; it cannot
        // answer "how many rows match this filter". Counting exactly is
        // slower but it is the only honest number.
        return CountStrategy::Exact;
    }
    strategy
}

/// Whether the driver can produce an estimate statement for this table.
fn driver_can_estimate(
    driver: &dyn DatabaseDriver,
    database: &str,
    schema: Option<&str>,
    table: &str,
) -> bool {
    driver.count_estimate_sql(database, schema, table).is_some()
}

/// The columns a read orders by when the caller did not ask for an order.
///
/// `override_columns` is the caller's explicit wish: an empty list means "do
/// not order at all". Without an override the driver decides — and the
/// "may a table without a primary key fall back to its first column" switch
/// is applied **here**, so it holds for a driver that overrode
/// `default_order_columns` as well.
pub fn resolve_default_order(
    driver: &dyn DatabaseDriver,
    columns: &[ColumnSchema],
    primary_keys: &[String],
    override_columns: Option<&[String]>,
) -> Vec<String> {
    if let Some(override_columns) = override_columns {
        return override_columns.to_vec();
    }
    let order = driver.default_order_columns(columns, primary_keys);
    if !order.is_empty() && primary_keys.is_empty() && !driver.allow_first_column_fallback_order() {
        return Vec::new();
    }
    order
}

/// Whether the column flags and the normalised primary keys name different
/// columns.
///
/// The two are the same fact read twice, and a driver that fills one and not
/// the other is the reason the host stopped re-deriving the key from the
/// flags. The answer is not an error — it is a fact worth reporting, because
/// it is the set of tables whose result order changed.
pub fn pk_sources_disagree(columns: &[ColumnSchema], primary_keys: &[String]) -> bool {
    let from_flags: Vec<&str> = columns
        .iter()
        .filter(|column| column.is_primary_key)
        .map(|column| column.name.as_str())
        .collect();
    let from_keys: Vec<&str> = primary_keys.iter().map(|key| key.as_str()).collect();
    from_flags != from_keys
}

/// Report the disagreement between the two sources of the primary key.
///
/// The level is `debug` on purpose: a driver that fills one source and not
/// the other is a normal difference, not a fault, and a `warn` would fill the
/// log of every user of that driver. The message is what a user report about
/// "the order changed" is matched against.
pub fn report_pk_source_disagreement(
    table: &str,
    columns: &[ColumnSchema],
    primary_keys: &[String],
) {
    if !pk_sources_disagree(columns, primary_keys) {
        return;
    }
    let from_flags: Vec<&str> = columns
        .iter()
        .filter(|column| column.is_primary_key)
        .map(|column| column.name.as_str())
        .collect();
    tracing::debug!(
        %table,
        column_flags = ?from_flags,
        primary_keys = ?primary_keys,
        "table_browsing: the column flags and the normalised primary keys disagree; \
         the read orders by the normalised primary keys"
    );
}

/// Whether the filters of a read will end up in a `WHERE` clause.
///
/// The judgement is the one the statement builder uses — an incomplete
/// condition is refused before the statement is built, so a list that would
/// contribute nothing never reaches the database. A second rule here would
/// drift from the SQL and make the estimate disagree with the rows.
pub fn has_effective_filters(filters: Option<&[FilterCondition]>) -> bool {
    match filters {
        None => false,
        Some(conditions) => !conditions.is_empty() && conditions.iter().all(filter_is_complete),
    }
}

/// The number an exact count statement reported.
///
/// Only an integer is a count. A string, a `NULL` or a value that does not
/// fit says "the count did not come back" — and the answer is then "the rows
/// are there, the number is not", not a number that was guessed out of the
/// text.
pub fn exact_count_cell(cell: &Value) -> Option<i64> {
    match cell {
        Value::Integer(count) => Some(*count),
        _ => None,
    }
}

/// The number an estimate statement reported.
///
/// An estimate has one more rule than an exact count: a negative number is
/// not an estimate of anything, it is the sign of a failed lookup. Zero is a
/// legal estimate — an empty table really has no rows.
pub fn estimated_count_cell(cell: &Value) -> Option<i64> {
    match cell {
        Value::Integer(count) if *count >= 0 => Some(*count),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::query_executor::FilterCondition;
    use datazen_driver_api::filters::FilterOperator;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};

    fn mock(options: MockDriverOptions) -> std::sync::Arc<MockDriver> {
        MockDriver::new("sqlite", options)
    }

    fn column(name: &str, is_primary_key: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key,
            is_auto_increment: false,
        }
    }

    fn complete_filter() -> FilterCondition {
        FilterCondition {
            column: "id".into(),
            operator: FilterOperator::Eq,
            value: Value::Integer(1),
            connector: None,
        }
    }

    #[test]
    fn skip_count_means_no_count_at_all() {
        let driver = mock(MockDriverOptions::default());
        assert_eq!(
            resolve_count_strategy(driver.as_ref(), None, true, false, "app", None, "orders"),
            CountStrategy::Unsupported
        );
    }

    #[test]
    fn a_driver_that_declares_unsupported_is_never_overruled() {
        let driver = mock(MockDriverOptions {
            skip_count_query: true,
            ..Default::default()
        });
        assert_eq!(
            resolve_count_strategy(
                driver.as_ref(),
                Some(CountStrategy::Exact),
                false,
                false,
                "app",
                None,
                "orders"
            ),
            CountStrategy::Unsupported
        );
        assert_eq!(
            resolve_count_strategy(
                driver.as_ref(),
                Some(CountStrategy::Estimated),
                false,
                false,
                "app",
                None,
                "orders"
            ),
            CountStrategy::Unsupported
        );
    }

    #[test]
    fn an_estimate_without_a_statement_degrades_to_exact() {
        let driver = mock(MockDriverOptions {
            count_strategy: Some(CountStrategy::Estimated),
            ..Default::default()
        });
        assert_eq!(
            resolve_count_strategy(
                driver.as_ref(),
                Some(CountStrategy::Estimated),
                false,
                false,
                "app",
                None,
                "orders"
            ),
            CountStrategy::Exact
        );
        assert_eq!(
            resolve_count_strategy(driver.as_ref(), None, false, false, "app", None, "orders"),
            CountStrategy::Exact
        );
    }

    #[test]
    fn an_estimate_with_a_statement_stays_an_estimate() {
        let driver = mock(MockDriverOptions {
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(10),
            ..Default::default()
        });
        assert_eq!(
            resolve_count_strategy(driver.as_ref(), None, false, false, "app", None, "orders"),
            CountStrategy::Estimated
        );
    }

    #[test]
    fn effective_filters_force_an_exact_count() {
        let driver = mock(MockDriverOptions {
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(10),
            ..Default::default()
        });
        assert_eq!(
            resolve_count_strategy(driver.as_ref(), None, false, true, "app", None, "orders"),
            CountStrategy::Exact
        );
    }

    #[test]
    fn the_first_column_fallback_is_gated_in_the_host() {
        let columns = vec![column("col_a", false)];
        let driver = mock(MockDriverOptions::default());
        assert_eq!(
            resolve_default_order(driver.as_ref(), &columns, &[], None),
            vec!["col_a".to_string()]
        );
        assert!(resolve_default_order(driver.as_ref(), &columns, &[], Some(&[])).is_empty());
    }

    #[test]
    fn effective_filters_follow_the_statement_builder() {
        assert!(!has_effective_filters(None));
        assert!(!has_effective_filters(Some(&[])));
        assert!(has_effective_filters(Some(&[complete_filter()])));
    }

    #[test]
    fn only_an_integer_is_a_count() {
        assert_eq!(exact_count_cell(&Value::Integer(7)), Some(7));
        assert_eq!(exact_count_cell(&Value::String("7".into())), None);
        assert_eq!(exact_count_cell(&Value::Null), None);
        assert_eq!(exact_count_cell(&Value::Float(7.0)), None);
    }

    #[test]
    fn an_estimate_may_not_be_negative() {
        assert_eq!(estimated_count_cell(&Value::Integer(0)), Some(0));
        assert_eq!(estimated_count_cell(&Value::Integer(7)), Some(7));
        assert_eq!(estimated_count_cell(&Value::Integer(-1)), None);
        assert_eq!(estimated_count_cell(&Value::Null), None);
    }
}
