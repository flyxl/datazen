//! The SQL text of a table read.
//!
//! Two statements are built here: the one that reads the rows of a page and
//! the one that counts them. Both are **text** — the execution, the session
//! and the row handling live in the module above.
//!
//! The page read has two shapes:
//!
//! * **by offset** — the historical one. `page × pageSize` rows are skipped
//!   and the statement says so.
//! * **by seek** — the page starts after a value boundary, so the offset is
//!   zero and the statement carries a condition instead. The condition is
//!   **not** built here: it is the driver's, because "can this dialect
//!   express this boundary" is dialect knowledge. This module only places the
//!   fragment the driver gave, with parentheses, and joins it with `AND`.
//!
//! The offset shape is byte-for-byte what it has always been. That is not
//! an accident to be checked by eye: the offset statement is the one every
//! table read produces, and a change in it is a change in every query the
//! product sends.

use datazen_driver_api::keyset::{KeysetUnavailableReason, OrderColumn, PagingMode};
use datazen_driver_api::{ColumnSchema, SqlDialect, Value};

use super::filter_sql::{self, FilterRejection};
use crate::services::table_paging::OrderSpec;

/// The statement of one page, and what it took to build it.
#[derive(Debug, Clone)]
pub struct PagedSelectSql {
    /// The statement.
    pub sql: String,
    /// The paging strategy the statement actually uses.
    pub mode: PagingMode,
    /// The order the statement is sorted by.
    pub order_columns: Vec<OrderColumn>,
    /// Why the page could not be read by seek, when it could not.
    pub unavailable_reason: Option<KeysetUnavailableReason>,
}

/// The statement that counts the rows of a table.
///
/// The column list is not part of it: a count does not name columns.
pub fn build_count_sql(
    table_name: &str,
    columns: &[ColumnSchema],
    filters: &Option<Vec<super::FilterCondition>>,
    dialect: &dyn SqlDialect,
    filter_logic: Option<&str>,
) -> Result<String, FilterRejection> {
    let _ = columns;
    let mut sql = format!("SELECT COUNT(*) FROM {}", dialect.quote_ident(table_name));

    if let Some(conditions) = filters {
        let parts = filter_sql::render_filter_parts(conditions, dialect)?;
        if !parts.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&filter_sql::join_filters(&parts, filter_logic));
        }
    }

    Ok(sql)
}

/// The statement that reads one page of a table, by offset.
///
/// `default_order` is the order the caller gets when it did not ask for
/// one; an empty list means "no order at all". The order used to be
/// derived here, from the column flags — the primary key was looked up by
/// filtering the columns, and a table without one fell back to its first
/// column. The columns are still the ones that carry the flags, but the
/// key itself now comes from the normalised list, because a driver that
/// fills `primary_keys` and not the flag used to lose its key here.
#[allow(clippy::too_many_arguments)]
pub fn build_select_sql(
    table_name: &str,
    columns: &[ColumnSchema],
    filters: Option<Vec<super::FilterCondition>>,
    order_by: Option<super::OrderBy>,
    default_order: &[String],
    dialect: &dyn SqlDialect,
    limit: u64,
    offset: u64,
    filter_logic: Option<&str>,
) -> Result<String, FilterRejection> {
    let order = super::order_spec_of(order_by.as_ref(), default_order);
    let paged = build_paged_select_sql(
        table_name,
        columns,
        filters,
        &order,
        PagingMode::Offset,
        None,
        dialect,
        limit,
        offset,
        filter_logic,
    )?;
    Ok(paged.sql)
}

/// The statement that reads one page of a table, by offset or by seek.
///
/// * `order` is the order the page is sorted by. It is the **only**
///   description of the order: the statement does not compute an order of
///   its own.
/// * `mode` is the paging strategy the caller decided on. In
///   [`PagingMode::Keyset`] the offset is zero — a seek reads the rows after
///   the boundary, so there is nothing to skip.
/// * `predicate_sql` is the driver's seek condition, or `None`. It is `None`
///   on the **first** page of a seek, which has no boundary yet.
///
/// The condition is placed **after** the user's filters and joined with
/// `AND`, inside parentheses. It is never joined with `OR`: a page boundary
/// is not one of the alternatives the user asked for, it is the frame the
/// alternatives live in.
#[allow(clippy::too_many_arguments)]
pub fn build_paged_select_sql(
    table_name: &str,
    columns: &[ColumnSchema],
    filters: Option<Vec<super::FilterCondition>>,
    order: &OrderSpec,
    mode: PagingMode,
    predicate_sql: Option<&str>,
    dialect: &dyn SqlDialect,
    limit: u64,
    offset: u64,
    filter_logic: Option<&str>,
) -> Result<PagedSelectSql, FilterRejection> {
    let qi = |name: &str| dialect.quote_ident(name);
    let by_seek = mode == PagingMode::Keyset;
    // The filters are only grouped when a condition is placed next to them:
    // without one there is nothing to bind, and the statement stays the
    // statement the table read always sent.
    let with_predicate = predicate_sql.is_some();
    // A seek reads the rows after the boundary: there is nothing to skip.
    let pagination = dialect.pagination_syntax(limit, if by_seek { 0 } else { offset });

    let mut sql = String::new();
    sql.push_str("SELECT ");
    if columns.is_empty() {
        sql.push('*');
    } else {
        sql.push_str(
            &columns
                .iter()
                .map(|c| qi(&c.name))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }

    sql.push_str(&format!(" FROM {}", qi(table_name)));

    let mut has_filters = false;
    if let Some(conditions) = filters {
        let parts = filter_sql::render_filter_parts(&conditions, dialect)?;
        if !parts.is_empty() {
            has_filters = true;
            sql.push_str(" WHERE ");
            if with_predicate {
                // The filters become one group, so that the `AND` below binds
                // the whole group and not only its last condition.
                sql.push('(');
                sql.push_str(&filter_sql::join_filters(&parts, filter_logic));
                sql.push(')');
            } else {
                sql.push_str(&filter_sql::join_filters(&parts, filter_logic));
            }
        }
    }

    if let Some(predicate) = predicate_sql {
        if has_filters {
            sql.push_str(" AND ");
        } else {
            sql.push_str(" WHERE ");
        }
        sql.push('(');
        sql.push_str(predicate);
        sql.push(')');
    }

    let mut has_order_by = false;
    if !order.columns.is_empty() {
        let parts: Vec<String> = order
            .columns
            .iter()
            .map(|column| {
                format!(
                    "{} {}",
                    qi(&column.column),
                    if column.descending { "DESC" } else { "ASC" }
                )
            })
            .collect();
        sql.push_str(&format!(" ORDER BY {}", parts.join(", ")));
        has_order_by = true;
    }

    // Dialects whose pagination clause is only legal after `ORDER BY`
    // (T-SQL `OFFSET … FETCH`) supply their own neutral ordering. Gated on
    // `requires_order_by` so a dialect that merely offers a fallback is not
    // forced into an ordering it did not ask for.
    if !has_order_by && pagination.requires_order_by {
        if let Some(fallback) = pagination.order_by_fallback {
            sql.push_str(&format!(" ORDER BY {fallback}"));
        }
    }

    // The clause belongs to the dialect: `LIMIT … OFFSET …` on PostgreSQL,
    // `OFFSET … ROWS FETCH NEXT … ROWS ONLY` on SQL Server.
    if !pagination.clause.is_empty() {
        sql.push(' ');
        sql.push_str(&pagination.clause);
    }

    Ok(PagedSelectSql {
        sql,
        mode,
        order_columns: order.columns.clone(),
        unavailable_reason: None,
    })
}

/// The boundary of a page, as the driver contract speaks it.
///
/// The values come from a row that was really read, in the order of the
/// order columns, and they are the values the **driver** compares against —
/// so the boundary and the condition are built from the same list.
pub fn boundary_of(
    order_columns: &[OrderColumn],
    values: &[Value],
) -> Option<Vec<(String, Value)>> {
    if order_columns.len() != values.len() {
        return None;
    }
    Some(
        order_columns
            .iter()
            .zip(values.iter())
            .map(|(column, value)| (column.column.clone(), value.clone()))
            .collect(),
    )
}
