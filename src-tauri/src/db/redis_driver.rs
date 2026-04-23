//! Redis driver — exposes Redis as a key-value browser through the DatabaseDriver trait.
//!
//! Unlike SQL databases, Redis doesn't have tables/schemas. We map the concepts:
//! - "databases" → Redis logical databases (0-15)
//! - "tables" → key type groupings (string, list, set, zset, hash, stream)
//! - "query" → raw Redis command execution (e.g. `GET key`, `HGETALL key`)
//! - "get_tables" → SCAN-based key listing with type classification

use super::*;
use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client, Cmd};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

struct RedisConn {
    #[allow(dead_code)]
    client: Client,
    connection: MultiplexedConnection,
}

pub struct RedisDriver {
    connections: RwLock<HashMap<String, RedisConn>>,
}

impl RedisDriver {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }

    fn get_conn<'a>(
        conns: &'a mut HashMap<String, RedisConn>,
        handle: &ConnectionHandle,
    ) -> Result<&'a mut RedisConn, DriverError> {
        conns
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Redis connection not found".into()))
    }

    /// Redis only accepts a numeric db index in the URL path; non-numeric values default to `0`.
    fn redis_db_index(raw: Option<&str>) -> String {
        let s = raw.map(str::trim).unwrap_or("");
        if s.is_empty() {
            return "0".into();
        }
        if s.chars().all(|c| c.is_ascii_digit()) {
            s.to_string()
        } else {
            "0".into()
        }
    }

    fn build_url(config: &ConnectionConfig) -> String {
        let host = config.host.as_deref().unwrap_or("127.0.0.1");
        let port = config.port.unwrap_or(6379);
        let db = Self::redis_db_index(config.database.as_deref());
        let password = config
            .password
            .as_deref()
            .map(|p| urlencoding::encode(p));
        let username = config
            .username
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|u| urlencoding::encode(u));

        match (username, password) {
            (Some(user), Some(pass)) => {
                format!("redis://{}:{}@{}:{}/{}", user, pass, host, port, db)
            }
            (None, Some(pass)) => format!("redis://:{}@{}:{}/{}", pass, host, port, db),
            (Some(user), None) => format!("redis://{}@{}:{}/{}", user, host, port, db),
            (None, None) => format!("redis://{}:{}/{}", host, port, db),
        }
    }
}

#[async_trait]
impl DatabaseDriver for RedisDriver {
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::Redis
    }

    fn quote_char(&self) -> char {
        '\0' // Redis doesn't quote identifiers
    }

    fn quote_ident(&self, name: &str) -> String {
        name.to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let url = Self::build_url(config);
        let client = Client::open(url)
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let info: String = redis::cmd("INFO")
            .arg("server")
            .query_async(&mut conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let version = info
            .lines()
            .find(|l| l.starts_with("redis_version:"))
            .map(|l| l.trim_start_matches("redis_version:").trim().to_string())
            .unwrap_or_else(|| "unknown".into());

        Ok(ServerInfo {
            server_version: version,
            server_type: "Redis".into(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let url = Self::build_url(config);
        let pool_id = format!("redis_{}", uuid::Uuid::new_v4());

        let client = Client::open(url)
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let mut conns = self.connections.write().await;
        conns.insert(pool_id.clone(), RedisConn { client, connection });

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

        let info: String = redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query_async(&mut rc.connection)
            .await
            .unwrap_or_else(|_| "databases\n16".into());

        let db_count: usize = info
            .lines()
            .last()
            .and_then(|l| l.trim().parse().ok())
            .unwrap_or(16);

        Ok((0..db_count).map(|i| i.to_string()).collect())
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        // SCAN to get all keys (limited to first 1000 for performance)
        let mut keys: Vec<String> = Vec::new();
        let mut cursor: u64 = 0;
        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("COUNT")
                .arg(200)
                .query_async(&mut rc.connection)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

            keys.extend(batch);
            cursor = next_cursor;
            if cursor == 0 || keys.len() >= 1000 {
                break;
            }
        }

        keys.sort();

        let tables: Vec<TableInfo> = keys
            .into_iter()
            .map(|key| TableInfo {
                name: key,
                schema: None,
                table_type: TableType::Table,
                row_count: None,
            })
            .collect();

        Ok(tables)
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        let key_type: String = rc.connection.key_type(table).await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

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
            table_name: table.to_string(),
            columns,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        })
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let start = Instant::now();
        let parts: Vec<&str> = sql.split_whitespace().collect();
        if parts.is_empty() {
            return Err(DriverError::QueryFailed("Empty command".into()));
        }

        let mut conns = self.connections.write().await;
        let rc = Self::get_conn(&mut conns, handle)?;

        let mut cmd = Cmd::new();
        for part in &parts {
            cmd.arg(*part);
        }

        let result: redis::Value = cmd
            .query_async(&mut rc.connection)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

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
        let commands: Vec<&str> = sql.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
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

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError("Transactions are not supported for Redis".into()))
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError("Transactions are not supported for Redis".into()))
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError("Transactions are not supported for Redis".into()))
    }

    async fn explain(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<ExplainResult, DriverError> {
        Err(DriverError::QueryFailed("EXPLAIN is not available for Redis".into()))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

/// Convert a Redis value into tabular format for the UI.
fn redis_value_to_rows(value: &redis::Value) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
    match value {
        redis::Value::Nil => (
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: true }],
            vec![vec![Some(Value::Null)]],
        ),
        redis::Value::Int(n) => (
            vec![ColumnInfo { name: "result".into(), data_type: "integer".into(), nullable: false }],
            vec![vec![Some(Value::Integer(*n))]],
        ),
        redis::Value::BulkString(bytes) => {
            let s = String::from_utf8_lossy(bytes).to_string();
            (
                vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
                vec![vec![Some(Value::String(s))]],
            )
        }
        redis::Value::Array(items) => {
            // Check if it's key-value pairs (even number of items → hash)
            if items.len() >= 2 && items.len() % 2 == 0 && looks_like_hash(items) {
                let columns = vec![
                    ColumnInfo { name: "field".into(), data_type: "string".into(), nullable: false },
                    ColumnInfo { name: "value".into(), data_type: "string".into(), nullable: true },
                ];
                let rows: Vec<Vec<Option<Value>>> = items
                    .chunks(2)
                    .map(|pair| {
                        vec![
                            Some(redis_to_value(&pair[0])),
                            Some(redis_to_value(&pair[1])),
                        ]
                    })
                    .collect();
                (columns, rows)
            } else {
                let columns = vec![
                    ColumnInfo { name: "index".into(), data_type: "integer".into(), nullable: false },
                    ColumnInfo { name: "value".into(), data_type: "string".into(), nullable: true },
                ];
                let rows: Vec<Vec<Option<Value>>> = items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        vec![
                            Some(Value::Integer(i as i64)),
                            Some(redis_to_value(v)),
                        ]
                    })
                    .collect();
                (columns, rows)
            }
        }
        redis::Value::SimpleString(s) => (
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
            vec![vec![Some(Value::String(s.clone()))]],
        ),
        redis::Value::Okay => (
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
            vec![vec![Some(Value::String("OK".into()))]],
        ),
        _ => (
            vec![ColumnInfo { name: "result".into(), data_type: "string".into(), nullable: false }],
            vec![vec![Some(Value::String(format!("{:?}", value)))]],
        ),
    }
}

fn redis_to_value(v: &redis::Value) -> Value {
    match v {
        redis::Value::Nil => Value::Null,
        redis::Value::Int(n) => Value::Integer(*n),
        redis::Value::BulkString(bytes) => Value::String(String::from_utf8_lossy(bytes).to_string()),
        redis::Value::SimpleString(s) => Value::String(s.clone()),
        redis::Value::Okay => Value::String("OK".into()),
        redis::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(|i| format!("{:?}", i)).collect();
            Value::String(format!("[{}]", parts.join(", ")))
        }
        _ => Value::String(format!("{:?}", v)),
    }
}

fn looks_like_hash(items: &[redis::Value]) -> bool {
    items.chunks(2).all(|pair| {
        matches!(&pair[0], redis::Value::BulkString(_) | redis::Value::SimpleString(_))
    })
}
