//! Query safety and the injectable execution port.
//!
//! The CLI owns policy (flags, production refusal); drivers own dialect and
//! execution. A fake executor in tests exercises the same path as a live
//! driver, so flag handling is never verified against a mock database.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use datazen::execution::domain::sql_guard::{
    check_sql, normalize_fullwidth, reject_null_bytes, strip_comments_for_tokenize, tokenize_sql,
};
use datazen_driver_api::{create_driver, ConnectionConfig, QueryResult, Value};
use serde::Serialize;

use crate::error::{
    CliError, E_DANGEROUS_SQL, E_DRIVER, E_PRODUCTION_WRITE, E_READ_ONLY, E_TIMEOUT, E_WRITE_FLAG,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct Safety {
    pub allow_writes: bool,
    pub allow_dangerous_sql: bool,
    /// Explicit operator assertion. Used in addition to group/name/option
    /// detection, never as a way to *disable* protection.
    pub production: bool,
}

#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub connection_id: String,
    pub sql: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOutput {
    pub connection: String,
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
    pub truncated: bool,
}

impl QueryOutput {
    fn from_result(connection: &str, mut result: QueryResult, limit: usize) -> Self {
        let truncated = result.rows.len() > limit;
        result.rows.truncate(limit);
        let columns = result
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let column_types = result
            .columns
            .iter()
            .map(|column| column.data_type.clone())
            .collect();
        let rows = result
            .rows
            .into_iter()
            .map(|row| row.into_iter().map(value_to_json).collect())
            .collect();
        Self {
            connection: connection.to_string(),
            columns,
            column_types,
            rows,
            rows_affected: result.rows_affected,
            execution_time_ms: result.execution_time_ms,
            truncated,
        }
    }

    pub fn table(&self) -> crate::output::Table {
        let mut table = crate::output::Table::new(self.columns.clone());
        for row in &self.rows {
            table.push(row.iter().map(json_cell).collect::<Vec<_>>());
        }
        table
    }
}

fn value_to_json(value: Option<Value>) -> serde_json::Value {
    match value {
        None | Some(Value::Null) => serde_json::Value::Null,
        Some(Value::Bool(value)) => serde_json::Value::Bool(value),
        Some(Value::Integer(value)) => serde_json::Value::Number(value.into()),
        Some(Value::Float(value)) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(Value::String(value)) | Some(Value::Timestamp(value)) => {
            serde_json::Value::String(value)
        }
        Some(Value::Bytes(value)) => serde_json::Value::Array(
            value
                .into_iter()
                .map(|byte| serde_json::Value::Number(byte.into()))
                .collect(),
        ),
        Some(Value::Json(value)) => value,
    }
}

fn json_cell(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

#[async_trait]
pub trait QueryExecutor: Send + Sync {
    async fn execute(&self, request: &QueryRequest) -> Result<QueryResult, CliError>;
}

/// Driver-backed executor used by the command layer. The config is decrypted
/// before it is placed here and is never serialized into command output.
pub struct DriverExecutor {
    pub config: ConnectionConfig,
}

#[async_trait]
impl QueryExecutor for DriverExecutor {
    async fn execute(&self, request: &QueryRequest) -> Result<QueryResult, CliError> {
        execute_driver_query(&self.config, &request.sql).await
    }
}

async fn execute_driver_query(
    config: &ConnectionConfig,
    sql: &str,
) -> Result<QueryResult, CliError> {
    let driver = create_driver(&config.database_type).ok_or_else(|| {
        CliError::new(
            E_DRIVER,
            format!(
                "driver '{}' is not compiled into this CLI build",
                config.database_type
            ),
        )
    })?;
    let handle = driver
        .connect(config)
        .await
        .map_err(|error| CliError::new(E_DRIVER, format!("connection failed: {error}")))?;
    let result = driver
        .query(&handle, sql)
        .await
        .map_err(|error| CliError::new(E_DRIVER, format!("query failed: {error}")));
    let _ = driver.disconnect(handle).await;
    result
}

pub fn is_production(config: &ConnectionConfig, explicit: bool) -> bool {
    if explicit {
        return true;
    }
    let option_production = config
        .options
        .as_ref()
        .and_then(|options| options.get("environment"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("production"));
    if option_production {
        return true;
    }
    [config.name.as_str(), config.group.as_deref().unwrap_or("")]
        .iter()
        .any(|label| {
            label
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .any(|part| {
                    part.eq_ignore_ascii_case("prod") || part.eq_ignore_ascii_case("production")
                })
        })
}

/// Enforce the default read-only policy and the two independent write flags.
/// The production check comes before flag handling: no flag combination may
/// turn a production write into a success.
pub fn guard_sql(sql: &str, config: &ConnectionConfig, safety: Safety) -> Result<(), CliError> {
    reject_null_bytes(sql).map_err(|message| CliError::new(E_READ_ONLY, message))?;
    let normalized = normalize_fullwidth(sql);
    let write = check_sql(&normalized, true, false).is_err();
    let tokens = tokenize_sql(&strip_comments_for_tokenize(&normalized));
    let dangerous = tokens.iter().any(|token| is_dangerous_keyword(token));

    if write && is_production(config, safety.production) {
        return Err(CliError::new(
            E_PRODUCTION_WRITE,
            "writes are refused on production connections",
        ));
    }
    if config.read_only && (write || dangerous) {
        return Err(CliError::new(
            E_READ_ONLY,
            format!(
                "connection '{}' is read-only; writes are not allowed",
                config.name
            ),
        ));
    }
    if dangerous {
        if !safety.allow_dangerous_sql {
            if !safety.allow_writes {
                return Err(CliError::new(
                    E_READ_ONLY,
                    "writes are disabled; pass --allow-writes to enable them",
                ));
            }
            return Err(CliError::new(
                E_DANGEROUS_SQL,
                "this statement needs --allow-dangerous-sql in addition to --allow-writes",
            ));
        }
        if !safety.allow_writes {
            return Err(CliError::new(
                E_WRITE_FLAG,
                "--allow-dangerous-sql does not imply --allow-writes",
            ));
        }
    } else if write && !safety.allow_writes {
        return Err(CliError::new(
            E_READ_ONLY,
            "writes are disabled; pass --allow-writes to enable them",
        ));
    }

    Ok(())
}

fn is_dangerous_keyword(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "CREATE"
            | "ALTER"
            | "DROP"
            | "TRUNCATE"
            | "GRANT"
            | "REVOKE"
            | "VACUUM"
            | "ATTACH"
            | "DETACH"
            | "PRAGMA"
            | "COPY"
            | "LOAD"
            | "CALL"
            | "EXEC"
            | "EXECUTE"
            | "DO"
            | "REINDEX"
            | "CLUSTER"
            | "COMMENT"
            | "RENAME"
    )
}

/// Execute with a hard wall-clock timeout around the injected executor.
pub async fn run_with_executor(
    executor: Arc<dyn QueryExecutor>,
    request: QueryRequest,
    safety: Safety,
    config: &ConnectionConfig,
    timeout: Duration,
) -> Result<QueryOutput, CliError> {
    guard_sql(&request.sql, config, safety)?;
    let connection = request.connection_id.clone();
    let limit = request.limit;
    let result = tokio::time::timeout(timeout, executor.execute(&request))
        .await
        .map_err(|_| {
            CliError::new(
                E_TIMEOUT,
                format!(
                    "query exceeded the {} second timeout",
                    timeout.as_secs_f64()
                ),
            )
        })??;
    Ok(QueryOutput::from_result(&connection, result, limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::{ColumnInfo, ConnectionConfig};
    use std::sync::Mutex;

    fn config(name: &str) -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "c1",
            "name": name,
            "databaseType": "sqlite",
            "host": null,
            "port": null,
            "database": "file:test",
            "schema": null,
            "username": null,
            "password": null,
            "sshTunnel": null,
            "httpProxyTunnel": null,
            "websocketTunnel": null,
            "readOnly": false
        }))
        .unwrap()
    }

    fn result(rows: usize) -> QueryResult {
        QueryResult {
            columns: vec![ColumnInfo {
                name: "id".into(),
                data_type: "INTEGER".into(),
                nullable: false,
            }],
            rows: (0..rows)
                .map(|index| vec![Some(Value::Integer(index as i64))])
                .collect(),
            rows_affected: None,
            execution_time_ms: 2,
        }
    }

    struct FakeExecutor {
        result: QueryResult,
        seen: Mutex<Vec<QueryRequest>>,
    }

    #[async_trait]
    impl QueryExecutor for FakeExecutor {
        async fn execute(&self, request: &QueryRequest) -> Result<QueryResult, CliError> {
            self.seen.lock().unwrap().push(request.clone());
            Ok(self.result.clone())
        }
    }

    #[test]
    fn default_blocks_writes_and_allow_writes_permits_dml() {
        let config = config("local");
        assert_eq!(
            guard_sql("UPDATE t SET x=1", &config, Safety::default())
                .unwrap_err()
                .code,
            E_READ_ONLY
        );
        assert!(guard_sql(
            "UPDATE t SET x=1",
            &config,
            Safety {
                allow_writes: true,
                ..Safety::default()
            }
        )
        .is_ok());
    }

    #[test]
    fn dangerous_sql_needs_its_own_flag() {
        let config = config("local");
        let write_only = Safety {
            allow_writes: true,
            ..Safety::default()
        };
        assert_eq!(
            guard_sql("DROP TABLE t", &config, write_only)
                .unwrap_err()
                .code,
            E_DANGEROUS_SQL
        );
        assert!(guard_sql(
            "DROP TABLE t",
            &config,
            Safety {
                allow_writes: true,
                allow_dangerous_sql: true,
                ..Safety::default()
            }
        )
        .is_ok());
    }

    #[test]
    fn dangerous_flag_alone_does_not_enable_writes() {
        let config = config("local");
        assert_eq!(
            guard_sql(
                "DROP TABLE t",
                &config,
                Safety {
                    allow_dangerous_sql: true,
                    ..Safety::default()
                }
            )
            .unwrap_err()
            .code,
            E_WRITE_FLAG
        );
    }

    #[test]
    fn read_verb_that_is_dangerous_still_needs_the_dangerous_flag() {
        let config = config("local");
        assert_eq!(
            guard_sql("PRAGMA journal_mode=DELETE", &config, Safety::default())
                .unwrap_err()
                .code,
            E_READ_ONLY
        );
        let both = Safety {
            allow_writes: true,
            allow_dangerous_sql: true,
            ..Safety::default()
        };
        assert!(guard_sql("PRAGMA journal_mode=DELETE", &config, both).is_ok());
    }

    #[test]
    fn production_refuses_writes_even_with_both_flags() {
        let mut config = config("prod-db");
        config.read_only = false;
        assert_eq!(
            guard_sql(
                "DELETE FROM t",
                &config,
                Safety {
                    allow_writes: true,
                    allow_dangerous_sql: true,
                    production: false,
                }
            )
            .unwrap_err()
            .code,
            E_PRODUCTION_WRITE
        );
    }

    #[test]
    fn stored_read_only_connection_refuses_writes() {
        let mut config = config("local");
        config.read_only = true;
        assert_eq!(
            guard_sql(
                "INSERT INTO t VALUES (1)",
                &config,
                Safety {
                    allow_writes: true,
                    ..Safety::default()
                }
            )
            .unwrap_err()
            .code,
            E_READ_ONLY
        );
    }

    #[tokio::test]
    async fn injected_executor_sees_limit_and_rows_are_truncated() {
        let executor = Arc::new(FakeExecutor {
            result: result(3),
            seen: Mutex::new(Vec::new()),
        });
        let output = run_with_executor(
            executor.clone(),
            QueryRequest {
                connection_id: "c1".into(),
                sql: "SELECT 1".into(),
                limit: 2,
            },
            Safety::default(),
            &config("local"),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(output.rows.len(), 2);
        assert!(output.truncated);
        assert_eq!(executor.seen.lock().unwrap()[0].limit, 2);
    }

    #[tokio::test]
    async fn slow_executor_returns_stable_timeout_code() {
        struct Slow;
        #[async_trait]
        impl QueryExecutor for Slow {
            async fn execute(&self, _request: &QueryRequest) -> Result<QueryResult, CliError> {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(result(1))
            }
        }
        let error = run_with_executor(
            Arc::new(Slow),
            QueryRequest {
                connection_id: "c1".into(),
                sql: "SELECT 1".into(),
                limit: 10,
            },
            Safety::default(),
            &config("local"),
            Duration::from_millis(10),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, E_TIMEOUT);
    }
}
