//! Query execution helpers and table browsing types.
//!
//! The filter rules live in [`filter_sql`]: this module only turns a
//! validated filter list into statements and hands the refusals back to the
//! caller.

use datazen_driver_api::{
    ColumnSchema, ConnectionHandle, CountStrategy, DatabaseDriver, DriverError,
    LegacyCatalogReader, LegacySqlDialect, SqlDialect, SqlTarget, TableDataResult,
};
use datazen_runtime::schema_cache::SchemaCache;
use std::sync::Arc;

use crate::services::table_browsing;

pub mod filter_sql;

#[cfg(test)]
mod tests;

pub use datazen_driver_api::filters::{FilterCondition, FilterConnector, FilterOperator};
pub use filter_sql::{FilterRejection, MAX_REGEX_PATTERN_LENGTH};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortCondition {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

#[derive(Debug, Clone)]
pub struct OrderBy {
    pub column: String,
    pub descending: bool,
}

/// Executes queries with optional schema cache integration.
pub struct QueryExecutor {
    pub schema_cache: Arc<SchemaCache>,
}

impl QueryExecutor {
    pub fn new(schema_cache: Arc<SchemaCache>) -> Self {
        Self { schema_cache }
    }

    #[allow(dead_code)]
    pub async fn execute_query(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Result<datazen_driver_api::QueryResult, DriverError> {
        driver
            .query_at(handle, sql, SqlTarget::new(database, schema))
            .await
    }

    /// Read one page of a table.
    ///
    /// `count_strategy` and `default_order_columns` are the caller's wishes;
    /// `None` for either means "the driver decides". `skip_count` is a
    /// different thing entirely: it says "reuse the total you already have",
    /// so not a single count statement is sent and the answer carries no
    /// count kind at all.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_table_data(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        db_session_id: &str,
        database: &str,
        schema: Option<&str>,
        table: &str,
        page: u32,
        page_size: u32,
        filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>,
        skip_count: bool,
        filter_logic: Option<&str>,
        count_strategy: Option<CountStrategy>,
        default_order_columns: Option<Vec<String>>,
    ) -> Result<TableDataResult, DriverError> {
        let catalog = LegacyCatalogReader::new(driver.as_ref());
        let cached = self
            .schema_cache
            .get_columns(db_session_id, database, schema, table, &catalog, handle)
            .await?;

        let dialect = LegacySqlDialect::new(driver.as_ref());

        // The service layer checks the filters too, not only the command
        // layer: callers that reach this method directly (MCP, workflows)
        // never pass the host's pre-check, and a filter that may not run must
        // not run through either door.
        filter_sql::validate_filters(
            driver.supported_filter_operators(),
            filters.as_deref().unwrap_or_default(),
        )
        .map_err(|rejection| DriverError::NotSupported(rejection.contract_message()))?;

        // The primary key has one source: the normalised list the cache
        // holds. The column flags are a second, possibly stale, copy of the
        // same fact, and a driver that fills one and not the other used to
        // lose its key here and fall back to the first column.
        table_browsing::report_pk_source_disagreement(
            &cached.table_name,
            &cached.columns,
            &cached.primary_keys,
        );
        let default_order = table_browsing::resolve_default_order(
            driver.as_ref(),
            &cached.columns,
            &cached.primary_keys,
            default_order_columns.as_deref(),
        );

        let data_sql = Self::build_select_sql(
            &cached.table_name,
            &cached.columns,
            filters.clone(),
            order_by,
            &default_order,
            &dialect,
            page_size as u64,
            (page as u64).saturating_mul(page_size as u64),
            filter_logic,
        )
        .map_err(|rejection| DriverError::NotSupported(rejection.contract_message()))?;

        let target = SqlTarget::new(Some(database), schema);

        if skip_count {
            tracing::debug!(%table, "query_executor: skip_count, SELECT only");
            let result = driver.query_at(handle, &data_sql, target).await?;
            return Ok(TableDataResult {
                columns: cached.columns,
                rows: result.rows,
                total_rows: None,
                // The caller asked to reuse the total it already has, so this
                // response says nothing about the count — not "there is no
                // count", which is what `Unsupported` would mean.
                total_rows_kind: None,
                page,
                page_size,
                paging: None,
            });
        }

        let strategy = table_browsing::resolve_count_strategy(
            driver.as_ref(),
            count_strategy,
            false,
            table_browsing::has_effective_filters(filters.as_deref()),
            database,
            schema,
            table,
        );

        match strategy {
            CountStrategy::Unsupported => {
                tracing::debug!(%table, "query_executor: no row count, SELECT only");
                let result = driver.query_at(handle, &data_sql, target).await?;
                Ok(TableDataResult {
                    columns: cached.columns,
                    rows: result.rows,
                    total_rows: None,
                    total_rows_kind: Some(CountStrategy::Unsupported),
                    page,
                    page_size,
                    paging: None,
                })
            }
            CountStrategy::Exact => {
                let count_sql = Self::build_count_sql(
                    &cached.table_name,
                    &cached.columns,
                    &filters,
                    &dialect,
                    filter_logic,
                )
                .map_err(|rejection| DriverError::NotSupported(rejection.contract_message()))?;
                tracing::info!(%table, "query_executor: count query");
                tracing::debug!(
                    %table,
                    count_sql = %datazen_platform_infra::redaction::sql_preview_for_log(&count_sql),
                    "query_executor: count sql"
                );

                // The count and the rows go out together and fail together,
                // which is what a read has always done: a read that cannot
                // count is a failed read, not a read without a number.
                let (count_res, data_res) = tokio::try_join!(
                    driver.query_at(handle, &count_sql, target),
                    driver.query_at(handle, &data_sql, target),
                )?;

                let total_rows = count_res
                    .rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(|cell| cell.as_ref())
                    .and_then(table_browsing::exact_count_cell);

                Ok(TableDataResult {
                    columns: cached.columns,
                    rows: data_res.rows,
                    total_rows,
                    total_rows_kind: Some(CountStrategy::Exact),
                    page,
                    page_size,
                    paging: None,
                })
            }
            CountStrategy::Estimated => {
                let estimate_sql = driver
                    .count_estimate_sql(database, schema, table)
                    .ok_or_else(|| {
                        DriverError::Unsupported(
                            "the driver declared an estimated row count but produced no statement"
                                .into(),
                        )
                    })?;
                tracing::info!(%table, "query_executor: estimate query");
                tracing::debug!(
                    %table,
                    estimate_sql = %datazen_platform_infra::redaction::sql_preview_for_log(&estimate_sql),
                    "query_executor: estimate sql"
                );

                // The estimate and the rows go out together, but they are
                // judged separately: a failed estimate must not take the
                // rows down with it, and it must not be retried as a count.
                let (estimate_res, data_res) = tokio::join!(
                    driver.query_at(handle, &estimate_sql, target),
                    driver.query_at(handle, &data_sql, target),
                );
                let data_res = data_res?;

                let (total_rows, kind) = match estimate_res {
                    Ok(result) => {
                        let cell = result
                            .rows
                            .first()
                            .and_then(|row| row.first())
                            .and_then(|cell| cell.as_ref());
                        match cell.and_then(table_browsing::estimated_count_cell) {
                            Some(count) => (Some(count), CountStrategy::Estimated),
                            None => {
                                tracing::warn!(
                                    %table,
                                    "query_executor: the estimate is not a usable row count"
                                );
                                (None, CountStrategy::Unsupported)
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            %table,
                            %error,
                            "query_executor: the estimate failed; the rows are unaffected"
                        );
                        (None, CountStrategy::Unsupported)
                    }
                };

                Ok(TableDataResult {
                    columns: cached.columns,
                    rows: data_res.rows,
                    total_rows,
                    total_rows_kind: Some(kind),
                    page,
                    page_size,
                    paging: None,
                })
            }
        }
    }

    /// The estimated number of rows in a table.
    ///
    /// `Ok(None)` means "this driver cannot estimate this table" — not an
    /// error, and not a hidden exact count. The caller decides what to do
    /// with a missing estimate.
    pub async fn estimate_row_count(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Option<i64>, DriverError> {
        let Some(sql) = driver.count_estimate_sql(database, schema, table) else {
            return Ok(None);
        };
        tracing::debug!(%table, "query_executor: estimate query");
        let target = SqlTarget::new(Some(database), schema);
        match driver.query_at(handle, &sql, target).await {
            Ok(result) => {
                let cell = result
                    .rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(|cell| cell.as_ref());
                Ok(cell.and_then(table_browsing::estimated_count_cell))
            }
            Err(error) => {
                // "Cannot estimate" is an answer, not a failure: the caller
                // is asking for a number it may not get.
                tracing::warn!(%table, %error, "query_executor: the estimate failed");
                Ok(None)
            }
        }
    }

    pub fn build_count_sql(
        table_name: &str,
        columns: &[ColumnSchema],
        filters: &Option<Vec<FilterCondition>>,
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

    /// Build the statement that reads one page of a table.
    ///
    /// `default_order` is the order the caller gets when it did not ask for
    /// one; an empty list means "no order at all". The order used to be
    /// derived here, from the column flags — the primary key was looked up by
    /// filtering the columns, and a table without one fell back to its first
    /// column. The columns are still the ones that carry the flags, but the
    /// key itself now comes from the normalised list, because a driver that
    /// fills `primary_keys` and not the flag used to lose its key here.
    // Routing clause building through the SqlDialect contract added one
    // parameter past the clippy limit; the argument set is stable otherwise.
    #[allow(clippy::too_many_arguments)]
    pub fn build_select_sql(
        table_name: &str,
        columns: &[ColumnSchema],
        filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>,
        default_order: &[String],
        dialect: &dyn SqlDialect,
        limit: u64,
        offset: u64,
        filter_logic: Option<&str>,
    ) -> Result<String, FilterRejection> {
        let qi = |name: &str| dialect.quote_ident(name);
        let pagination = dialect.pagination_syntax(limit, offset);

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

        if let Some(conditions) = filters {
            let parts = filter_sql::render_filter_parts(&conditions, dialect)?;
            if !parts.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&filter_sql::join_filters(&parts, filter_logic));
            }
        }

        let mut has_order_by = false;
        if let Some(order) = order_by {
            sql.push_str(&format!(
                " ORDER BY {} {}",
                qi(&order.column),
                if order.descending { "DESC" } else { "ASC" }
            ));
            has_order_by = true;
        } else if !default_order.is_empty() {
            let parts: Vec<String> = default_order
                .iter()
                .map(|column| format!("{} ASC", qi(column)))
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
        Ok(sql)
    }

    /// Whether a filter condition carries everything it needs.
    ///
    /// The predicate lives in [`filter_sql`]; this forwarder keeps the
    /// `QueryExecutor::filter_is_complete` path alive for the callers that
    /// already use it, so there is exactly one predicate in the codebase.
    pub fn filter_is_complete(condition: &FilterCondition) -> bool {
        filter_sql::filter_is_complete(condition)
    }
}
