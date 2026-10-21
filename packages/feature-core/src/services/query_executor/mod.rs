//! Query execution helpers and table browsing types.
//!
//! The filter rules live in [`filter_sql`]: this module only turns a
//! validated filter list into statements and hands the refusals back to the
//! caller.

use datazen_driver_api::{
    ColumnSchema, ConnectionHandle, DatabaseDriver, DriverError, LegacyCatalogReader,
    LegacySqlDialect, SqlDialect, SqlTarget, TableDataResult, Value,
};
use datazen_runtime::schema_cache::SchemaCache;
use std::sync::Arc;

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

        let data_sql = Self::build_select_sql(
            &cached.table_name,
            &cached.columns,
            filters.clone(),
            order_by,
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
                page,
                page_size,
            });
        }

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

        let (count_res, data_res) = tokio::try_join!(
            driver.query_at(handle, &count_sql, target),
            driver.query_at(handle, &data_sql, target),
        )?;

        let total_rows = count_res
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(|cell| cell.as_ref())
            .and_then(|v| match v {
                Value::Integer(n) => Some(*n),
                _ => None,
            });

        Ok(TableDataResult {
            columns: cached.columns,
            rows: data_res.rows,
            total_rows,
            page,
            page_size,
        })
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

    // Routing clause building through the SqlDialect contract added one
    // parameter past the clippy limit; the argument set is stable otherwise.
    #[allow(clippy::too_many_arguments)]
    pub fn build_select_sql(
        table_name: &str,
        columns: &[ColumnSchema],
        filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>,
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
        } else {
            let pk_cols: Vec<&str> = columns
                .iter()
                .filter(|c| c.is_primary_key)
                .map(|c| c.name.as_str())
                .collect();
            let order_cols = if pk_cols.is_empty() {
                columns
                    .first()
                    .map(|c| vec![c.name.as_str()])
                    .unwrap_or_default()
            } else {
                pk_cols
            };
            if !order_cols.is_empty() {
                let parts: Vec<String> = order_cols
                    .iter()
                    .map(|c| format!("{} ASC", qi(c)))
                    .collect();
                sql.push_str(&format!(" ORDER BY {}", parts.join(", ")));
                has_order_by = true;
            }
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
