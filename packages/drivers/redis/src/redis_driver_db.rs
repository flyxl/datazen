//! DatabaseDriver trait implementation for RedisDriver.

use async_trait::async_trait;
use datazen_driver_api::*;
use redis::AsyncCommands;
use std::time::{Duration, Instant};

use crate::connect::{build_connection_plan, open_live_conn};
use crate::redis_driver::{RedisConn, RedisDriver, TEST_CONNECTION_TLS_GRACE};
use crate::redis_driver_on::{get_tables_on, info_server_on, query_cmd_on};
use crate::redis_value::{parse_redis_command_args, redis_value_to_rows};
use crate::with_redis_conn;

fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).into_owned(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Array(items) => items
            .iter()
            .map(value_to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[async_trait]
impl DatabaseDriver for RedisDriver {
    fn driver_type(&self) -> DatabaseType {
        "redis".to_string()
    }

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::KeyValue
    }

    fn quote_char(&self) -> char {
        '\0'
    }

    fn quote_ident(&self, name: &str) -> String {
        name.to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let timeout = Duration::from_secs(config.connection_timeout.max(1) as u64)
            .saturating_add(TEST_CONNECTION_TLS_GRACE);
        tokio::time::timeout(timeout, self.test_connection_inner(config))
            .await
            .map_err(|_| {
                DriverError::ConnectionFailed(format!(
                    "Redis test connection timed out after {timeout:?}"
                ))
            })?
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let plan = build_connection_plan(config)?;
        let pool_id = format!("redis_{}", uuid::Uuid::new_v4());
        let live = open_live_conn(&plan).await?;

        let mut conns = self.connections.write().await;
        conns.insert(pool_id.clone(), RedisConn { plan, live });
        drop(conns);

        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        let mut conns = self.connections.write().await;
        conns.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        let db_count: u32 = match with_redis_conn!(&mut rc.live, |conn| {
            redis::cmd("CONFIG")
                .arg("GET")
                .arg("databases")
                .query_async::<redis::Value>(conn)
                .await
        }) {
            Ok(val) => value_to_string(&val)
                .lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .next()
                .unwrap_or(16),
            Err(_) => 16,
        };

        Ok((0..db_count).map(|i| format!("db{i}")).collect())
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let db_index = Self::parse_db_name(database)?;
        // Dedicated connection so enumerating several DBs does not flip the
        // shared session's selected database.
        let plan = {
            let mut conns = self.connections.write().await;
            let rc = Self::get_conn(&mut conns, handle)?;
            rc.plan.clone()
        };
        let mut live = open_live_conn(&plan).await?;
        Self::select_db(&mut live, db_index)
            .await
            .map_err(DriverError::QueryFailed)?;
        with_redis_conn!(&mut live, |conn| get_tables_on(conn, db_index, 500).await)
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        let table_name = table.to_string();
        let key_type: String = with_redis_conn!(&mut rc.live, |conn| {
            conn.key_type(&table_name)
                .await
                .map_err(|e| e.to_string())
        })
        .map_err(DriverError::QueryFailed)?;

        let columns = match key_type.as_str() {
            "hash" => vec![
                ColumnSchema {
                    name: "field".into(),
                    data_type: "string".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                    comment: None,
                },
                ColumnSchema {
                    name: "value".into(),
                    data_type: "string".into(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                },
            ],
            "list" | "set" | "zset" => {
                let mut cols = vec![ColumnSchema {
                    name: "value".into(),
                    data_type: "string".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                }];
                if key_type == "zset" {
                    cols.push(ColumnSchema {
                        name: "score".into(),
                        data_type: "float".into(),
                        nullable: false,
                        default_value: None,
                        is_primary_key: false,
                        is_auto_increment: false,
                        comment: None,
                    });
                }
                cols
            }
            _ => vec![ColumnSchema {
                name: "value".into(),
                data_type: key_type.clone(),
                nullable: true,
                default_value: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
        };

        Ok(TableSchema {
            table_name: table_name.clone(),
            columns,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        })
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let start = Instant::now();
        let parts = parse_redis_command_args(sql)?;

        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        let cmd_name = parts[0].clone();
        let cmd_args: Vec<String> = parts[1..].to_vec();

        let result: redis::Value = with_redis_conn!(&mut rc.live, |conn| {
            query_cmd_on(conn, &cmd_name, &cmd_args).await
        })
        .map_err(DriverError::QueryFailed)?;

        let (columns, rows) = redis_value_to_rows(&result);

        Ok(QueryResult {
            columns,
            rows,
            rows_affected: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let total_start = Instant::now();
        let commands: Vec<&str> = sql
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        let mut results = Vec::new();

        for cmd_str in commands {
            let start = Instant::now();
            match self.query(handle, cmd_str).await {
                Ok(qr) => {
                    results.push(StatementResult {
                        sql: cmd_str.to_string(),
                        columns: qr.columns,
                        rows: qr.rows,
                        rows_affected: qr.rows_affected,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        truncated: false,
                    });
                }
                Err(e) => {
                    tracing::warn!(cmd = cmd_str, error = %e, "redis query_multi command failed");
                    results.push(StatementResult {
                        sql: cmd_str.to_string(),
                        columns: vec![],
                        rows: vec![],
                        rows_affected: None,
                        execution_time_ms: start.elapsed().as_millis() as u64,
                        truncated: false,
                    });
                }
            }
        }

        Ok(MultiQueryResult {
            results,
            total_time_ms: total_start.elapsed().as_millis() as u64,
        })
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        let result = self.query(handle, sql).await?;
        Ok(result.rows_affected.unwrap_or(0))
    }

    fn command_definitions(&self) -> Vec<datazen_driver_api::DriverCommandDefinition> {
        crate::commands::redis_command_definitions()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command_id: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        crate::commands_exec::execute_redis_command(self, handle, command_id, input).await
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        let mut conns = self.connections.write().await;
        let redis_conn = Self::get_conn(&mut conns, handle)?;
        let info: String = with_redis_conn!(&mut redis_conn.live, |conn| info_server_on(conn).await)
            .map_err(DriverError::QueryFailed)?;
        let version = info
            .lines()
            .find(|l| l.starts_with("redis_version:"))
            .map(|l| l.trim_start_matches("redis_version:").trim().to_string())
            .unwrap_or_else(|| "unknown".into());
        Ok(ServerInfo {
            server_version: version,
            server_type: "Redis".to_string(),
        })
    }

    async fn dump_database_with_progress(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _opts: &BackupDumpOptions,
        _on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        Err(DriverError::NotSupported(
            "Redis does not use SQL dump; export keys via driver commands".into(),
        ))
    }

    async fn restore_sql_with_progress(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _opts: Option<&BackupRestoreOptions>,
        _on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        Err(DriverError::NotSupported(
            "Redis does not restore SQL files; import via driver commands".into(),
        ))
    }
}
