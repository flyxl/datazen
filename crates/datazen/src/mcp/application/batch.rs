//! `execute_batch` application use case.
//!
//! Statement framing and the default read-only rejection live in
//! [`crate::mcp::domain::batch`]; this module owns execution. It connects the
//! profile's real driver, starts an explicit transaction only when requested,
//! executes the already-guarded statements in order, and returns one result per
//! attempted statement. The report is deliberately structured instead of
//! pretending a partial batch succeeded: callers see `ok`, `stoppedAt`,
//! `rolledBack`, and the per-statement outcome.
//!
//! Direct driver execution keeps the transaction on one connection. Going
//! through independent Runtime commands would put each statement on a different
//! pooled connection, which cannot honor a rollback promise.

use serde_json::{json, Value};
use std::time::Duration;

use crate::execution::domain::permission::{self, McpPermissionMode};
use crate::mcp::application::tools::ToolError;
use crate::mcp::domain::batch;
use crate::mcp::domain::policy::is_read_only_sql;
use crate::workflow::application::resource_execution::ResourceExecutionHost;
use datazen_driver_api::DatabaseDriver;
use std::sync::Arc;

pub const DEFAULT_LIMIT: u32 = 100;
pub const MAX_LIMIT: u32 = 5_000;
pub const MAX_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

pub struct BatchRequest<'a> {
    pub connection_id: &'a str,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub sql: &'a str,
    pub transaction: bool,
    pub allow_writes: bool,
    pub limit: Option<u32>,
    pub timeout_ms: Option<u32>,
    pub mode: McpPermissionMode,
}

pub async fn execute_batch(
    host: &dyn ResourceExecutionHost,
    request: BatchRequest<'_>,
) -> Result<String, ToolError> {
    let statements = batch::parse_sql_batch(request.sql).map_err(ToolError::InvalidParams)?;
    if !request.allow_writes {
        batch::ensure_default_read_only(&statements).map_err(ToolError::InvalidParams)?;
    }
    for (index, statement) in statements.iter().enumerate() {
        let allowed = if request.mode == McpPermissionMode::ReadOnly {
            // The read-only mode must still allow a read-only batch; writes are
            // rejected above unless allow_writes was explicitly set, in which
            // case the mode guard rejects them here.
            is_read_only_sql(statement)
        } else {
            permission::check_sql_allowed(statement, request.mode).is_ok()
        };
        if !allowed {
            return Err(ToolError::InvalidParams(format!(
                "execute_batch statement {} is not allowed in the current MCP permission mode",
                index + 1
            )));
        }
    }

    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let timeout_ms =
        u64::from(request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS as u32)).min(MAX_TIMEOUT_MS);
    let (config, _, enabled) = host
        .profile(request.connection_id)
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?;
    if !enabled {
        return Err(ToolError::Internal("Connection profile is disabled".into()));
    }
    let driver = host
        .driver(&config)
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?;
    let database = request
        .database
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| config.database.clone());
    if driver.has_multi_database() && database.is_none() {
        return Err(ToolError::InvalidParams(
            "execute_batch has no database: set database on the call or a default database on the connection".into(),
        ));
    }
    let mut connect_config = config;
    if let Some(database) = database.clone() {
        connect_config.database = Some(database);
    }
    if let Some(schema) = request
        .schema
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        connect_config.schema = Some(schema.to_owned());
    }

    let handle = driver
        .connect(&connect_config)
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?;
    let execution = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        run_batch(&*driver, &handle, &statements, request.transaction, limit),
    )
    .await;
    let _ = driver.disconnect(handle).await;

    match execution {
        Err(_) => Err(ToolError::Internal("execute_batch timed out".into())),
        Ok(Ok(report)) => serde_json::to_string_pretty(&report)
            .map_err(|error| ToolError::Internal(error.to_string())),
        Ok(Err(error)) => Err(error),
    }
}

async fn run_batch(
    driver: &dyn DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    statements: &[String],
    transaction: bool,
    limit: u32,
) -> Result<Value, ToolError> {
    let transactional = transaction;
    let mut outcomes = Vec::with_capacity(statements.len());
    let mut total_rows: u64 = 0;
    let mut stopped_at: Option<usize> = None;
    let mut rolled_back = false;
    let mut transaction_error: Option<String> = None;
    let mut transaction = if transactional {
        Some(
            driver
                .begin_transaction(handle)
                .await
                .map_err(|error| ToolError::Internal(error.to_string()))?,
        )
    } else {
        None
    };

    for (index, statement) in statements.iter().enumerate() {
        match driver.query_multi(handle, statement, Some(limit)).await {
            Ok(result) => {
                let mut statement_rows = 0;
                let mut first = true;
                for row in result.results {
                    let row_count = row
                        .rows_affected
                        .map(|count| count as u64)
                        .unwrap_or(row.rows.len() as u64);
                    statement_rows += row_count;
                    let entry = json!({
                        "index": index,
                        "ok": true,
                        "rowCount": row_count,
                        "rowsAffected": row.rows_affected,
                        "columns": row.columns,
                        "rows": row.rows,
                        "truncated": row.truncated,
                        "executionTimeMs": row.execution_time_ms,
                    });
                    if first {
                        outcomes.push(entry);
                        first = false;
                    } else {
                        // A driver may split one submitted statement into
                        // several protocol frames; keep each frame visible.
                        outcomes.push(entry);
                    }
                }
                if first {
                    outcomes.push(json!({
                        "index": index,
                        "ok": true,
                        "rowCount": 0,
                        "rows": [],
                        "columns": [],
                    }));
                }
                total_rows += statement_rows;
            }
            Err(error) => {
                stopped_at = Some(index);
                outcomes.push(json!({
                    "index": index,
                    "ok": false,
                    "rowCount": 0,
                    "error": error.to_string(),
                }));
                if let Some(tx) = transaction.take() {
                    match driver.rollback(tx).await {
                        Ok(()) => rolled_back = true,
                        Err(rollback_error) => {
                            transaction_error = Some(format!(
                                "statement {} failed and rollback also failed: {rollback_error}",
                                index + 1
                            ));
                        }
                    }
                }
                break;
            }
        }
    }

    if let Some(tx) = transaction.take() {
        if let Err(error) = driver.commit(tx).await {
            transaction_error = Some(error.to_string());
        }
    }

    let ok = stopped_at.is_none() && transaction_error.is_none();
    let mut report = json!({
        "transaction": transactional,
        "ok": ok,
        "rowCount": total_rows,
        "statements": outcomes,
        "stoppedAt": stopped_at,
        "rolledBack": rolled_back,
    });
    if let Some(error) = transaction_error {
        report["error"] = json!(error);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::domain::error::WorkflowError;
    use crate::workflow::resource_execution::PreparedExecution;
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::ConnectionConfig;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Host {
        driver: Arc<MockDriver>,
        config: ConnectionConfig,
        prepare_reached: AtomicBool,
    }

    #[async_trait::async_trait]
    impl ResourceExecutionHost for Host {
        async fn profile(&self, _id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
            Ok((self.config.clone(), 1, true))
        }
        async fn driver(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
            Ok(self.driver.clone())
        }
        async fn safe_mode(&self) -> bool {
            false
        }
        async fn prepare(
            &self,
            _connection_id: &str,
            _revision: u64,
            _command: &str,
            _target: crate::execution::domain::target::ExecutionTarget,
        ) -> Result<PreparedExecution, WorkflowError> {
            self.prepare_reached.store(true, Ordering::SeqCst);
            Err(WorkflowError::Validation("prepare not used".into()))
        }
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "batch",
            "name": "batch",
            "databaseType": "sqlite",
            "host": null,
            "port": null,
            "database": "app",
            "schema": null,
            "username": null,
            "password": null,
            "connectionTimeout": 30,
            "maxPoolSize": 10,
            "readOnly": false,
            "pinned": false
        }))
        .expect("config")
    }

    fn host(options: MockDriverOptions) -> Host {
        Host {
            driver: MockDriver::new("sqlite", options),
            config: config(),
            prepare_reached: AtomicBool::new(false),
        }
    }

    #[tokio::test]
    async fn default_write_is_rejected_before_the_driver_is_used() {
        let host = host(MockDriverOptions::default());
        let error = execute_batch(
            &host,
            BatchRequest {
                connection_id: "batch",
                database: Some("app".into()),
                schema: None,
                sql: "SELECT 1; UPDATE users SET name = name",
                transaction: false,
                allow_writes: false,
                limit: None,
                timeout_ms: None,
                mode: McpPermissionMode::SafeWrite,
            },
        )
        .await
        .expect_err("write batch must be rejected");
        assert!(error.into_message().contains("statement 2"));
        assert!(!host.prepare_reached.load(Ordering::SeqCst));
        assert_eq!(host.driver.query_calls(), 0);
    }

    #[tokio::test]
    async fn transaction_failure_rolls_back_and_never_commits() {
        let options = MockDriverOptions {
            query_error_on_call: Some(2),
            ..Default::default()
        };
        let host = host(options);
        let output = execute_batch(
            &host,
            BatchRequest {
                connection_id: "batch",
                database: Some("app".into()),
                schema: None,
                sql: "SELECT 1; SELECT 2",
                transaction: true,
                allow_writes: false,
                limit: None,
                timeout_ms: None,
                mode: McpPermissionMode::SafeWrite,
            },
        )
        .await
        .expect("structured partial result");
        let value: Value = serde_json::from_str(&output).expect("json");
        assert_eq!(value["ok"], false);
        assert_eq!(value["rolledBack"], true);
        assert_eq!(value["stoppedAt"], 1);
        assert_eq!(value["statements"][0]["ok"], true);
        assert_eq!(value["statements"][1]["ok"], false);
        assert_eq!(host.driver.commit_calls(), 0);
        assert_eq!(host.driver.open_transaction_count(), 0);
    }

    #[tokio::test]
    async fn partial_failure_without_transaction_keeps_successful_statement_and_stops() {
        let options = MockDriverOptions {
            query_error_on_call: Some(2),
            ..Default::default()
        };
        let host = host(options);
        let output = execute_batch(
            &host,
            BatchRequest {
                connection_id: "batch",
                database: Some("app".into()),
                schema: None,
                sql: "SELECT 1; SELECT 2; SELECT 3",
                transaction: false,
                allow_writes: false,
                limit: None,
                timeout_ms: None,
                mode: McpPermissionMode::SafeWrite,
            },
        )
        .await
        .expect("structured partial result");
        let value: Value = serde_json::from_str(&output).expect("json");
        assert_eq!(value["ok"], false);
        assert_eq!(value["stoppedAt"], 1);
        assert_eq!(value["rolledBack"], false);
        assert_eq!(value["statements"].as_array().map(Vec::len), Some(2));
        assert_eq!(value["statements"][0]["ok"], true);
        assert_eq!(value["statements"][1]["ok"], false);
        assert_eq!(host.driver.query_calls(), 2);
    }

    #[tokio::test]
    async fn read_only_batch_executes_without_approval_or_transaction() {
        let host = host(MockDriverOptions::default());
        let output = execute_batch(
            &host,
            BatchRequest {
                connection_id: "batch",
                database: Some("app".into()),
                schema: None,
                sql: "SELECT 1; SELECT 2",
                transaction: false,
                allow_writes: false,
                limit: None,
                timeout_ms: None,
                mode: McpPermissionMode::SafeWrite,
            },
        )
        .await
        .expect("read batch");
        let value: Value = serde_json::from_str(&output).expect("json");
        assert_eq!(value["ok"], true);
        assert_eq!(value["statements"].as_array().map(Vec::len), Some(2));
        assert_eq!(host.driver.query_calls(), 2);
    }
}
