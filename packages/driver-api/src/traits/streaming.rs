//! Streaming and column-resolution defaults.
//!
//! The trait promises every driver that a streamed read needs no host-side
//! special casing, and that a single table can report columns and primary keys
//! from whatever `get_table_schema` already returns. Both bodies are thin
//! adapters over other trait methods, so they are grouped with the plumbing they
//! adapt rather than left inside the trait declaration.

use super::DatabaseDriver;
use crate::query_stream::{emit_multi_query_as_stream, QueryStreamCallback};
use crate::types::{
    ColumnSchema, ConnectionHandle, DriverError, MultiQueryResult, StatementResult, Value,
};

pub(crate) async fn get_columns<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
    let table_schema = driver
        .get_table_schema(handle, table, database, schema)
        .await?;
    let pks = table_schema.effective_primary_keys();
    Ok((table_schema.columns, pks))
}

pub(crate) async fn query_stream_with_params<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    sql: &str,
    params: &[Value],
    limit: Option<u32>,
    on_event: QueryStreamCallback,
) -> Result<(), DriverError> {
    if params.is_empty() {
        return driver.query_stream(handle, sql, limit, on_event).await;
    }
    let result = driver.query_with_params(handle, sql, params).await?;
    let mut rows = result.rows;
    let truncated = limit.is_some_and(|cap| rows.len() > cap as usize);
    if let Some(cap) = limit {
        rows.truncate(cap as usize);
    }
    emit_multi_query_as_stream(
        MultiQueryResult {
            results: vec![StatementResult {
                sql: sql.to_string(),
                columns: result.columns,
                rows,
                rows_affected: result.rows_affected,
                execution_time_ms: result.execution_time_ms,
                truncated,
            }],
            total_time_ms: result.execution_time_ms,
        },
        &on_event,
    );
    Ok(())
}
