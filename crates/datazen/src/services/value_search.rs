//! The value search of one table (DB-19): the statement side of the
//! full-database search.
//!
//! The **scan** is not here. The front end drives it — one table at a time,
//! one command per table — because that is what makes pause and resume real:
//! pausing is simply not issuing the next table's command. This module owns
//! the one thing the front end must never do: turning a search value into a
//! statement.
//!
//! The rules that matter:
//!
//! * The columns to search are the **request's** columns. Choosing them is a
//!   type-classification question the front end already answers (the same
//!   normalised type table the grid renders with); this module never guesses
//!   which columns are searchable.
//! * The value is **bound** wherever the dialect owns a bound-parameter
//!   vocabulary, exactly like the foreign-key probe: the value never reaches
//!   the SQL text unless the driver has no placeholders at all.
//! * Wildcard characters in the value are escaped only when the dialect
//!   declares an escape character **and** its `ESCAPE` clause — the pair must
//!   exist together, or an escaped `%` degrades back into a wildcard and the
//!   answer lies.

use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, DriverError, LegacySqlDialect, SqlDialect, SqlTarget, Value,
};
use serde::{Deserialize, Serialize};

/// How the search value must match a column's value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// The value appears anywhere in the column (`%value%`).
    #[default]
    Contains,
    /// The column equals the value.
    Exact,
    /// The column starts with the value (`value%`).
    Prefix,
    /// The column ends with the value (`%value`).
    Suffix,
}

/// The request: one table, the columns to search, the value and its mode.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchTableValuesRequest {
    /// The runtime session; never persisted.
    pub db_session_id: String,
    /// The table to scan.
    pub table: String,
    /// The columns to search, in the order the statement ORs them.
    pub columns: Vec<String>,
    /// The value to look for.
    pub value: String,
    /// How the value must match; `contains` when absent.
    #[serde(default)]
    pub mode: SearchMode,
    /// The database of the table; the host command resolves the fallback.
    #[serde(default)]
    pub database: Option<String>,
    /// The namespace of the table; the host command resolves the fallback.
    #[serde(default)]
    pub schema: Option<String>,
}

/// The answer: how many rows of the table match, and what was searched.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchTableValuesResult {
    /// The table, as the driver normalized it.
    pub table: String,
    /// The columns the statement actually searched.
    pub searched_columns: Vec<String>,
    /// The rows matching at least one column.
    pub matches: u64,
}

/// Why a value-search request cannot be served.
#[derive(Debug, Clone)]
pub enum ValueSearchError {
    /// The request cannot describe a search at all.
    Shape(String),
    /// The statement or the query failed; the text carries the error's kind
    /// only, never the driver's own words (a server error can quote a value).
    Query(String),
}

impl std::fmt::Display for ValueSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape(message) => write!(f, "grid.valueSearch.shapeMismatch: {message}"),
            Self::Query(message) => write!(f, "grid.valueSearch.queryFailed: {message}"),
        }
    }
}

impl std::error::Error for ValueSearchError {}

/// Build the LIKE pattern for a mode, escaping the wildcards **in the value**
/// when the dialect owns an escape vocabulary.
fn like_pattern(dialect: &dyn SqlDialect, value: &str, mode: SearchMode) -> String {
    let escape = dialect.like_pattern_escape();
    let escaped = match escape {
        Some(escape_char) => value
            .chars()
            .flat_map(|character| {
                if character == escape_char || character == '%' || character == '_' {
                    vec![escape_char, character]
                } else {
                    vec![character]
                }
            })
            .collect::<String>(),
        None => value.to_string(),
    };
    match mode {
        SearchMode::Contains => format!("%{escaped}%"),
        SearchMode::Prefix => format!("{escaped}%"),
        SearchMode::Suffix => format!("%{escaped}"),
        SearchMode::Exact => escaped,
    }
}

/// The statement for one table: one `SELECT COUNT(*)`, ORed over the columns.
///
/// The statement comes back in one of the two shapes every value-carrying
/// statement of the shared layer uses: **bound** when the dialect owns
/// placeholders, **inlined** (the value in the text) when it does not.
pub(crate) enum SearchStatement {
    Bound { sql: String, params: Vec<Value> },
    Inlined { sql: String },
}

impl SearchStatement {
    /// The statement text; the tests read it back.
    #[cfg(test)]
    pub(crate) fn sql(&self) -> &str {
        match self {
            Self::Bound { sql, .. } | Self::Inlined { sql } => sql,
        }
    }
}

/// Build the count statement of one value search.
pub(crate) fn build_search_statement(
    dialect: &dyn SqlDialect,
    table: &str,
    columns: &[String],
    value: &str,
    mode: SearchMode,
) -> Result<SearchStatement, DriverError> {
    let quote = |name: &str| dialect.quote_ident(name);

    // The bound-parameter vocabulary is the driver's own; a driver without one
    // keeps the value inline — the same ladder the foreign-key probe climbs.
    let placeholders = dialect.parameter_placeholder(1, Some("text")).ok();

    let escape_clause = match (
        dialect.like_pattern_escape(),
        dialect.like_escape_clause(dialect.like_pattern_escape().unwrap_or('\\')),
    ) {
        (Some(_), Some(clause)) => Some(clause.to_string()),
        _ => None,
    };

    let mut conditions: Vec<String> = Vec::with_capacity(columns.len());
    let mut params: Vec<Value> = Vec::with_capacity(columns.len());
    for column in columns {
        let name = quote(column);
        let condition = match (mode, placeholders.as_ref()) {
            (SearchMode::Exact, Some(placeholder)) => {
                params.push(Value::String(value.to_string()));
                format!("{name} = {placeholder}")
            }
            (SearchMode::Exact, None) => {
                let literal =
                    dialect.try_format_sql_literal(&Some(Value::String(value.to_string())))?;
                format!("{name} = {literal}")
            }
            (mode, Some(placeholder)) => {
                let pattern = like_pattern(dialect, value, mode);
                params.push(Value::String(pattern));
                match &escape_clause {
                    Some(clause) => format!("{name} LIKE {placeholder} {clause}"),
                    None => format!("{name} LIKE {placeholder}"),
                }
            }
            (mode, None) => {
                let pattern = like_pattern(dialect, value, mode);
                let literal = dialect.try_format_sql_literal(&Some(Value::String(pattern)))?;
                match &escape_clause {
                    Some(clause) => format!("{name} LIKE {literal} {clause}"),
                    None => format!("{name} LIKE {literal}"),
                }
            }
        };
        conditions.push(condition);
    }

    let sql = format!(
        "SELECT COUNT(*) FROM {} WHERE {}",
        quote(table),
        conditions.join(" OR ")
    );
    Ok(match placeholders {
        Some(_) => SearchStatement::Bound { sql, params },
        None => SearchStatement::Inlined { sql },
    })
}

/// Run one table's value search.
pub async fn search_table_values(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    database: &str,
    schema: Option<&str>,
    request: &SearchTableValuesRequest,
) -> Result<SearchTableValuesResult, ValueSearchError> {
    if request.columns.is_empty() {
        return Err(ValueSearchError::Shape(
            "a value search needs at least one column".to_string(),
        ));
    }
    if request.value.is_empty() {
        return Err(ValueSearchError::Shape(
            "the search value is empty".to_string(),
        ));
    }

    let dialect = LegacySqlDialect::new(driver);
    let statement = build_search_statement(
        &dialect,
        &request.table,
        &request.columns,
        &request.value,
        request.mode,
    )
    .map_err(|error| {
        tracing::warn!(
            table = %request.table,
            "value search statement could not be built"
        );
        ValueSearchError::Query(format!(
            "the search statement cannot be expressed ({})",
            driver_error_kind(&error)
        ))
    })?;

    let target = SqlTarget::new(Some(database), schema);
    let result = match statement {
        SearchStatement::Bound { sql, params } => {
            driver
                .query_with_params_at(handle, &sql, &params, target)
                .await
        }
        SearchStatement::Inlined { sql } => driver.query_at(handle, &sql, target).await,
    }
    .map_err(|error| {
        tracing::warn!(table = %request.table, "value search query failed");
        ValueSearchError::Query(format!(
            "searching '{table}' failed ({kind})",
            table = request.table,
            kind = driver_error_kind(&error)
        ))
    })?;

    let matches = result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|cell| cell.as_ref())
        .and_then(count_of)
        .unwrap_or(0);
    Ok(SearchTableValuesResult {
        table: request.table.clone(),
        searched_columns: request.columns.clone(),
        matches,
    })
}

/// The count cell, as any numeric spelling the drivers report it.
fn count_of(value: &Value) -> Option<u64> {
    match value {
        Value::Integer(number) if *number >= 0 => Some(*number as u64),
        Value::Float(number) if *number >= 0.0 => Some(*number as u64),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

/// The kind of a driver error, without its content — the same rule the
/// foreign-key probe follows: the text can quote a user value, the kind cannot.
fn driver_error_kind(error: &DriverError) -> &'static str {
    match error {
        DriverError::SessionLost => "sessionLost",
        DriverError::ConnectionFailed(_) => "connectionFailed",
        DriverError::QueryFailed(_) => "queryFailed",
        _ => "driverError",
    }
}

#[cfg(test)]
mod tests;
