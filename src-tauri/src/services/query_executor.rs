//! Query execution helpers and table browsing types.

use crate::cache::SchemaCache;
use crate::db::{
    ColumnSchema, ConnectionHandle, DatabaseDriver, DriverError, TableDataResult, Value,
};
use std::sync::Arc;

/// Single filter for table data APIs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    #[serde(default)]
    pub value: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    Like,
    In,
    IsNull,
    IsNotNull,
}

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
    ) -> Result<crate::db::QueryResult, DriverError> {
        driver.query(handle, sql).await
    }

    pub async fn get_table_data(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        connection_id: &str,
        database: &str,
        table: &str,
        page: u32,
        page_size: u32,
        filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>,
        skip_count: bool,
        filter_logic: Option<&str>,
    ) -> Result<TableDataResult, DriverError> {
        let cached = self
            .schema_cache
            .get_columns(connection_id, database, table, driver, handle)
            .await?;

        let qi = |name: &str| driver.quote_ident(name);
        let format_lit = |v: &Value| driver.format_sql_literal(&Some(v.clone()));

        let data_sql = Self::build_select_sql(
            &cached.table_name,
            &cached.columns,
            page,
            page_size,
            filters.clone(),
            order_by,
            &qi,
            &format_lit,
            driver.supports_offset(),
            filter_logic,
        );

        if skip_count {
            tracing::debug!(%table, "query_executor: skip_count, SELECT only");
            let result = driver.query(handle, &data_sql).await?;
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
            &qi,
            &format_lit,
            filter_logic,
        );
        tracing::info!(%table, %count_sql, "query_executor: count query");

        let (count_res, data_res) = tokio::try_join!(
            driver.query(handle, &count_sql),
            driver.query(handle, &data_sql),
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

    fn build_count_sql(
        table_name: &str,
        columns: &[ColumnSchema],
        filters: &Option<Vec<FilterCondition>>,
        qi: &dyn Fn(&str) -> String,
        format_lit: &dyn Fn(&Value) -> String,
        filter_logic: Option<&str>,
    ) -> String {
        let _ = columns;
        let mut sql = format!("SELECT COUNT(*) FROM {}", qi(table_name));

        if let Some(conditions) = filters {
            let parts: Vec<String> = conditions
                .iter()
                .map(|c| Self::format_condition(c, qi, format_lit))
                .filter(|s| !s.is_empty())
                .collect();
            if !parts.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&parts.join(filter_join(filter_logic)));
            }
        }

        sql
    }

    fn build_select_sql(
        table_name: &str,
        columns: &[ColumnSchema],
        page: u32,
        page_size: u32,
        filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>,
        qi: &dyn Fn(&str) -> String,
        format_lit: &dyn Fn(&Value) -> String,
        supports_offset: bool,
        filter_logic: Option<&str>,
    ) -> String {
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
            let parts: Vec<String> = conditions
                .iter()
                .map(|c| Self::format_condition(c, qi, format_lit))
                .collect();
            let parts: Vec<String> = parts.into_iter().filter(|s| !s.is_empty()).collect();
            if !parts.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&parts.join(filter_join(filter_logic)));
            }
        }

        if let Some(order) = order_by {
            sql.push_str(&format!(
                " ORDER BY {} {}",
                qi(&order.column),
                if order.descending { "DESC" } else { "ASC" }
            ));
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
            }
        }

        if supports_offset {
            let offset = page.saturating_mul(page_size);
            sql.push_str(&format!(" LIMIT {page_size} OFFSET {offset}"));
        } else {
            sql.push_str(&format!(" LIMIT {page_size}"));
        }
        sql
    }

    fn format_condition(
        condition: &FilterCondition,
        qi: &dyn Fn(&str) -> String,
        format_lit: &dyn Fn(&Value) -> String,
    ) -> String {
        let col = qi(&condition.column);
        match condition.operator {
            FilterOperator::Eq => format!("{col} = {}", format_lit(&condition.value)),
            FilterOperator::Ne => format!("{col} != {}", format_lit(&condition.value)),
            FilterOperator::Gt => format!("{col} > {}", format_lit(&condition.value)),
            FilterOperator::Lt => format!("{col} < {}", format_lit(&condition.value)),
            FilterOperator::Gte => format!("{col} >= {}", format_lit(&condition.value)),
            FilterOperator::Lte => format!("{col} <= {}", format_lit(&condition.value)),
            FilterOperator::Like => format!("{col} LIKE {}", format_lit(&condition.value)),
            FilterOperator::In => match &condition.value {
                Value::Json(serde_json::Value::Array(arr)) => {
                    let parts: Vec<String> = arr
                        .iter()
                        .map(|j| format_lit(&Value::Json(j.clone())))
                        .collect();
                    format!("{col} IN ({})", parts.join(", "))
                }
                _ => format!("{col} IN (NULL)"),
            },
            FilterOperator::IsNull => format!("{col} IS NULL"),
            FilterOperator::IsNotNull => format!("{col} IS NOT NULL"),
        }
    }
}

fn filter_join(logic: Option<&str>) -> &'static str {
    match logic {
        Some(s) if s.eq_ignore_ascii_case("or") => " OR ",
        _ => " AND ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_qi(name: &str) -> String {
        format!("\"{}\"", name)
    }

    fn simple_lit(value: &Value) -> String {
        match value {
            Value::Null => "NULL".to_string(),
            Value::Bool(b) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Value::Integer(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            Value::Bytes(_) => "NULL".to_string(),
            Value::Timestamp(s) => format!("'{}'", s.replace('\'', "''")),
            Value::Json(v) => format!("'{}'", v.to_string().replace('\'', "''")),
        }
    }

    fn make_column(name: &str, is_pk: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: is_pk,
            is_auto_increment: false,
        }
    }

    #[test]
    fn no_explicit_sort_uses_primary_key_order() {
        let columns = vec![
            make_column("id", true),
            make_column("name", false),
            make_column("email", false),
        ];
        let sql = QueryExecutor::build_select_sql(
            "orders",
            &columns,
            0,
            50,
            None,
            None,
            &simple_qi,
            &simple_lit,
            true,
            None,
        );
        assert!(
            sql.contains("ORDER BY \"id\" ASC"),
            "Expected default ORDER BY primary key, got: {sql}"
        );
    }

    #[test]
    fn no_explicit_sort_composite_pk_uses_all_pk_columns() {
        let columns = vec![
            make_column("order_id", true),
            make_column("product_id", true),
            make_column("quantity", false),
        ];
        let sql = QueryExecutor::build_select_sql(
            "order_items",
            &columns,
            0,
            50,
            None,
            None,
            &simple_qi,
            &simple_lit,
            true,
            None,
        );
        assert!(
            sql.contains("ORDER BY \"order_id\" ASC, \"product_id\" ASC"),
            "Expected composite PK ordering, got: {sql}"
        );
    }

    #[test]
    fn explicit_sort_overrides_default_pk_order() {
        let columns = vec![make_column("id", true), make_column("name", false)];
        let order = OrderBy {
            column: "name".to_string(),
            descending: true,
        };
        let sql = QueryExecutor::build_select_sql(
            "users",
            &columns,
            0,
            50,
            None,
            Some(order),
            &simple_qi,
            &simple_lit,
            true,
            None,
        );
        assert!(
            sql.contains("ORDER BY \"name\" DESC"),
            "Expected explicit ORDER BY, got: {sql}"
        );
        assert!(
            !sql.contains("\"id\" ASC"),
            "Should not contain default PK ordering when explicit sort is given, got: {sql}"
        );
    }

    #[test]
    fn no_pk_no_explicit_sort_still_has_order_by_first_column() {
        let columns = vec![make_column("col_a", false), make_column("col_b", false)];
        let sql = QueryExecutor::build_select_sql(
            "no_pk_table",
            &columns,
            0,
            50,
            None,
            None,
            &simple_qi,
            &simple_lit,
            true,
            None,
        );
        assert!(
            sql.contains("ORDER BY \"col_a\" ASC"),
            "Expected fallback ORDER BY first column, got: {sql}"
        );
    }

    #[test]
    fn filter_literals_use_formatter() {
        let columns = vec![make_column("flag", false)];
        let filters = vec![FilterCondition {
            column: "flag".into(),
            operator: FilterOperator::Eq,
            value: Value::Bool(true),
        }];
        let sql = QueryExecutor::build_select_sql(
            "t",
            &columns,
            0,
            10,
            Some(filters),
            None,
            &simple_qi,
            &simple_lit,
            true,
            None,
        );
        assert!(
            sql.contains("WHERE \"flag\" = TRUE"),
            "Expected driver-style TRUE literal, got: {sql}"
        );
    }

    #[test]
    fn build_count_sql_with_eq_filter() {
        let columns = vec![make_column("status", false)];
        let filters = vec![FilterCondition {
            column: "status".into(),
            operator: FilterOperator::Eq,
            value: Value::String("active".into()),
        }];
        let sql = QueryExecutor::build_count_sql(
            "users",
            &columns,
            &Some(filters),
            &simple_qi,
            &simple_lit,
            None,
        );
        assert!(sql.contains("WHERE \"status\" = 'active'"));
    }

    #[test]
    fn build_select_sql_joins_filters_with_or() {
        let columns = vec![make_column("status", false), make_column("role", false)];
        let filters = vec![
            FilterCondition {
                column: "status".into(),
                operator: FilterOperator::Eq,
                value: Value::String("active".into()),
            },
            FilterCondition {
                column: "role".into(),
                operator: FilterOperator::Eq,
                value: Value::String("admin".into()),
            },
        ];
        let sql = QueryExecutor::build_select_sql(
            "users",
            &columns,
            0,
            10,
            Some(filters),
            None,
            &simple_qi,
            &simple_lit,
            true,
            Some("or"),
        );
        assert!(
            sql.contains("WHERE \"status\" = 'active' OR \"role\" = 'admin'"),
            "got: {sql}"
        );
    }

    #[test]
    fn build_count_sql_joins_filters_with_or() {
        let columns = vec![make_column("status", false), make_column("role", false)];
        let filters = vec![
            FilterCondition {
                column: "status".into(),
                operator: FilterOperator::Eq,
                value: Value::String("active".into()),
            },
            FilterCondition {
                column: "role".into(),
                operator: FilterOperator::Eq,
                value: Value::String("admin".into()),
            },
        ];
        let sql = QueryExecutor::build_count_sql(
            "users",
            &columns,
            &Some(filters),
            &simple_qi,
            &simple_lit,
            Some("OR"),
        );
        assert!(
            sql.contains("WHERE \"status\" = 'active' OR \"role\" = 'admin'"),
            "got: {sql}"
        );
    }

    #[test]
    fn filter_join_defaults_to_and() {
        assert_eq!(filter_join(None), " AND ");
        assert_eq!(filter_join(Some("and")), " AND ");
        assert_eq!(filter_join(Some("or")), " OR ");
    }

    #[test]
    fn build_select_sql_without_offset_when_unsupported() {
        let columns = vec![make_column("id", true)];
        let sql = QueryExecutor::build_select_sql(
            "users",
            &columns,
            2,
            25,
            None,
            None,
            &simple_qi,
            &simple_lit,
            false,
            None,
        );
        assert!(sql.contains("LIMIT 25"));
        assert!(!sql.contains("OFFSET"));
    }

    #[test]
    fn format_condition_in_and_is_null() {
        let in_filter = FilterCondition {
            column: "id".into(),
            operator: FilterOperator::In,
            value: Value::Json(serde_json::json!([1, 2, 3])),
        };
        let null_filter = FilterCondition {
            column: "deleted_at".into(),
            operator: FilterOperator::IsNull,
            value: Value::Null,
        };
        assert_eq!(
            QueryExecutor::format_condition(&in_filter, &simple_qi, &simple_lit),
            "\"id\" IN ('1', '2', '3')"
        );
        assert_eq!(
            QueryExecutor::format_condition(&null_filter, &simple_qi, &simple_lit),
            "\"deleted_at\" IS NULL"
        );
    }

    #[tokio::test]
    async fn get_table_data_uses_schema_cache_and_returns_total() {
        use crate::cache::SchemaCache;
        use crate::db::registry::DriverRegistry;
        use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

        let registry = Arc::new(DriverRegistry::new());
        let mock = MockDriver::new(
            "postgres",
            MockDriverOptions {
                count_total: 99,
                query_rows: vec![vec![
                    Some(Value::Integer(1)),
                    Some(Value::String("alice".into())),
                ]],
                ..Default::default()
            },
        );
        registry
            .register_test_driver("postgres", mock.clone())
            .await;
        let cache = Arc::new(SchemaCache::new(registry));
        let executor = QueryExecutor::new(cache);
        let driver = mock.clone() as Arc<dyn crate::db::DatabaseDriver>;
        let handle = ConnectionHandle {
            id: "h1".into(),
            pool_id: "p1".into(),
        };

        let result = executor
            .get_table_data(
                &driver, &handle, "conn1", "db1", "users", 0, 50, None, None, false, None,
            )
            .await
            .unwrap();

        assert_eq!(result.total_rows, Some(99));
        assert_eq!(result.rows.len(), 1);
        assert_eq!(mock.query_calls(), 2, "count + data queries");
    }
}
