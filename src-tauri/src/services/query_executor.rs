//! Query execution helpers and table browsing types.

use crate::cache::SchemaCache;
use crate::db::{
    ColumnSchema, ConnectionHandle, DatabaseDriver, DriverError, TableDataResult,
    Value,
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

    pub async fn execute_query(
        &self,
        driver: &Arc<dyn DatabaseDriver>,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<crate::db::QueryResult, DriverError> {
        driver.query(handle, sql).await
    }

    fn format_json_scalar(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Null => "NULL".into(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            _ => "NULL".into(),
        }
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
    ) -> Result<TableDataResult, DriverError> {
        let cached = self
            .schema_cache
            .get_columns(connection_id, database, table, driver, handle)
            .await?;

        let qi = |name: &str| driver.quote_ident(name);

        let count_sql = Self::build_count_sql(&cached.table_name, &cached.columns, &filters, &qi);
        let total_rows = match driver.query(handle, &count_sql).await {
            Ok(count_result) => {
                count_result.rows.first()
                    .and_then(|row| row.first())
                    .and_then(|cell| cell.as_ref())
                    .and_then(|v| match v {
                        Value::Integer(n) => Some(*n),
                        _ => None,
                    })
            }
            Err(_) => None,
        };

        let sql = Self::build_select_sql(&cached.table_name, &cached.columns, page, page_size, filters, order_by, &qi);
        let result = driver.query(handle, &sql).await?;

        Ok(TableDataResult {
            columns: cached.columns,
            rows: result.rows,
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
    ) -> String {
        let _ = columns;
        let mut sql = format!("SELECT COUNT(*) FROM {}", qi(table_name));

        if let Some(conditions) = filters {
            let parts: Vec<String> = conditions
                .iter()
                .map(|c| Self::format_condition(c, qi))
                .filter(|s| !s.is_empty())
                .collect();
            if !parts.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&parts.join(" AND "));
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
            let parts: Vec<String> = conditions.iter().map(|c| Self::format_condition(c, qi)).collect();
            let parts: Vec<String> = parts.into_iter().filter(|s| !s.is_empty()).collect();
            if !parts.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&parts.join(" AND "));
            }
        }

        if let Some(order) = order_by {
            sql.push_str(&format!(
                " ORDER BY {} {}",
                qi(&order.column),
                if order.descending { "DESC" } else { "ASC" }
            ));
        }

        let offset = page.saturating_mul(page_size);
        sql.push_str(&format!(" LIMIT {page_size} OFFSET {offset}"));
        sql
    }

    fn format_condition(condition: &FilterCondition, qi: &dyn Fn(&str) -> String) -> String {
        let col = qi(&condition.column);
        match condition.operator {
            FilterOperator::Eq => format!("{col} = {}", Self::format_value(&condition.value)),
            FilterOperator::Ne => format!("{col} != {}", Self::format_value(&condition.value)),
            FilterOperator::Gt => format!("{col} > {}", Self::format_value(&condition.value)),
            FilterOperator::Lt => format!("{col} < {}", Self::format_value(&condition.value)),
            FilterOperator::Gte => format!("{col} >= {}", Self::format_value(&condition.value)),
            FilterOperator::Lte => format!("{col} <= {}", Self::format_value(&condition.value)),
            FilterOperator::Like => format!("{col} LIKE {}", Self::format_value(&condition.value)),
            FilterOperator::In => match &condition.value {
                Value::Json(serde_json::Value::Array(arr)) => {
                    let parts: Vec<String> = arr.iter().map(Self::format_json_scalar).collect();
                    format!("{col} IN ({})", parts.join(", "))
                }
                _ => format!("{col} IN (NULL)"),
            },
            FilterOperator::IsNull => format!("{col} IS NULL"),
            FilterOperator::IsNotNull => format!("{col} IS NOT NULL"),
        }
    }

    fn format_value(value: &Value) -> String {
        match value {
            Value::Null => "NULL".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            Value::Bytes(_) => "NULL".to_string(),
            Value::Timestamp(s) => format!("'{}'", s.replace('\'', "''")),
            Value::Json(v) => format!("'{}'", v.to_string().replace('\'', "''")),
        }
    }
}
