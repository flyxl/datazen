//! The probe statement: the one SQL text the foreign-key resolver sends.
//!
//! It is built here, away from the rules, because it is the only place where
//! a **value** could reach a statement — and the place where the statement
//! has to stay valid SQL for every dialect.

use datazen_driver_api::{ColumnSchema, DriverError, SqlDialect, Value};

/// The probe statement, in one of the two shapes a driver can execute.
pub(super) enum ProbeStatement {
    /// The values are **bound**: the statement text carries placeholders
    /// only, so a key value never reaches the SQL text.
    ///
    /// PostgreSQL, MySQL, SQLite and SQL Server own a bound-parameter
    /// vocabulary and take this path.
    Bound { sql: String, params: Vec<Value> },
    /// The driver has no bound-parameter vocabulary, so the values are
    /// inlined as literals: **the value does reach the SQL text**, and with
    /// it the server's statement log.
    ///
    /// The SQL drivers in that group are ClickHouse, DuckDB, rqlite and
    /// Turso (the non-SQL ones — Redis, MongoDB, Elasticsearch, InfluxDB,
    /// Vector, VictoriaMetrics, HBase — have no foreign keys at all). The IPC
    /// message stays value-free on this path too: only the error's *kind*
    /// travels back.
    Inlined { sql: String },
}

impl ProbeStatement {
    /// The statement text, whichever shape it has.
    pub(super) fn sql(&self) -> &str {
        match self {
            Self::Bound { sql, .. } | Self::Inlined { sql } => sql,
        }
    }
}

/// Build the existence probe: one `SELECT … LIMIT limit + 1` and no
/// `COUNT(*)`, because the existence check must not cost a full-table count.
///
/// The statement selects the referenced columns only, in the constraint's
/// order, so a candidate row is already the target's key.
pub(super) fn build_probe_statement(
    dialect: &dyn SqlDialect,
    target_table: &str,
    referenced_columns: &[ColumnSchema],
    referenced_names: &[String],
    values: &[Value],
    limit: u32,
) -> Result<ProbeStatement, DriverError> {
    let quote = |name: &str| dialect.quote_ident(name);
    let columns = referenced_columns
        .iter()
        .map(|column| quote(&column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!("SELECT {columns} FROM {}", quote(target_table));

    // The bound-parameter vocabulary is the driver's own; a driver without one
    // keeps the values inline.
    let placeholders = referenced_columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            dialect.parameter_placeholder(index + 1, Some(column.data_type.as_str()))
        })
        .collect::<Result<Vec<_>, _>>()
        .ok();

    let mut conditions: Vec<String> = Vec::with_capacity(values.len());
    let mut params = Vec::with_capacity(values.len());
    for (index, name) in referenced_names.iter().enumerate() {
        let value = values.get(index).cloned().unwrap_or(Value::Null);
        let condition = match &placeholders {
            Some(placeholders) => {
                params.push(value);
                format!(
                    "{} = {}",
                    quote(name),
                    placeholders.get(index).cloned().unwrap_or_default()
                )
            }
            None => {
                let literal = dialect.try_format_sql_literal(&Some(value))?;
                format!("{} = {}", quote(name), literal)
            }
        };
        conditions.push(condition);
    }
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    // An ordering is added **only** when the dialect's pagination clause is
    // illegal without one (T-SQL's `OFFSET … FETCH NEXT …`). The command has
    // no ordering parameter (07 U-8), so an ordering of its own would
    // silently reorder the target rows — usually by the very key that was
    // looked up, which reads as "the jump sorted my table".
    let pagination = dialect.pagination_syntax(u64::from(limit) + 1, 0);
    if pagination.requires_order_by {
        let order = pagination
            .order_by_fallback
            .map(str::to_string)
            .unwrap_or_else(|| {
                referenced_columns
                    .iter()
                    .map(|column| format!("{} ASC", quote(&column.name)))
                    .collect::<Vec<_>>()
                    .join(", ")
            });
        if !order.is_empty() {
            sql.push_str(&format!(" ORDER BY {order}"));
        }
    }
    if !pagination.clause.is_empty() {
        sql.push(' ');
        sql.push_str(&pagination.clause);
    }

    Ok(match placeholders {
        Some(_) => ProbeStatement::Bound { sql, params },
        None => ProbeStatement::Inlined { sql },
    })
}
