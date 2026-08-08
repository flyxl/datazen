//! SQL Server driver backed by `tiberius`.

use super::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tiberius::{AuthMethod, Client, ColumnData, Config, QueryItem};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

type SqlClient = Client<Compat<TcpStream>>;

pub struct SqlServerDriver {
    clients: RwLock<HashMap<String, SqlClient>>,
}

impl SqlServerDriver {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    fn build_config(config: &ConnectionConfig) -> Result<Config, DriverError> {
        let mut cfg = Config::new();
        cfg.host(
            config
                .host
                .clone()
                .ok_or_else(|| DriverError::InvalidConfig("host is required".into()))?,
        );
        if let Some(port) = config.port {
            cfg.port(port);
        }
        let user = config
            .username
            .clone()
            .ok_or_else(|| DriverError::InvalidConfig("username is required".into()))?;
        let pass = config.password.clone().unwrap_or_default();
        cfg.authentication(AuthMethod::sql_server(user, pass));
        if let Some(db) = &config.database {
            if !db.is_empty() {
                cfg.database(db);
            }
        }
        cfg.trust_cert();
        Ok(cfg)
    }

    async fn connect_client(config: &ConnectionConfig) -> Result<SqlClient, DriverError> {
        let cfg = Self::build_config(config)?;
        let host = config
            .host
            .clone()
            .ok_or_else(|| DriverError::InvalidConfig("host is required".into()))?;
        let port = config.port.unwrap_or(1433);
        let addr = format!("{host}:{port}");
        let timeout = Duration::from_secs(config.connection_timeout as u64);
        let tcp = tokio::time::timeout(timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| DriverError::ConnectionFailed("SQL Server connect timed out".into()))?
            .map_err(|e| DriverError::ConnectionFailed(format!("SQL Server connect failed: {e}")))?;
        tcp.set_nodelay(true).ok();
        tokio::time::timeout(timeout, Client::connect(cfg, tcp.compat_write()))
            .await
            .map_err(|_| DriverError::ConnectionFailed("SQL Server login timed out".into()))?
            .map_err(|e| DriverError::ConnectionFailed(format!("SQL Server login failed: {e}")))
    }

    fn value_from_column(data: &ColumnData<'_>) -> Option<Value> {
        match data {
            ColumnData::U8(v) => v.map(|x| Value::Integer(x as i64)),
            ColumnData::I16(v) => v.map(|x| Value::Integer(x as i64)),
            ColumnData::I32(v) => v.map(|x| Value::Integer(x as i64)),
            ColumnData::I64(v) => v.map(Value::Integer),
            ColumnData::F32(v) => v.map(|x| Value::Float(x as f64)),
            ColumnData::F64(v) => v.map(Value::Float),
            ColumnData::Bit(v) => v.map(Value::Bool),
            ColumnData::String(v) => v.as_ref().map(|s| Value::String(s.to_string())),
            ColumnData::Guid(v) => v.map(|g| Value::String(g.to_string())),
            ColumnData::Binary(v) => v.as_ref().map(|b| {
                Value::String(format!("0x{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>()))
            }),
            ColumnData::Numeric(v) => v.map(|n| Value::String(n.to_string())),
            ColumnData::Xml(v) => v.as_ref().map(|x| Value::String(x.to_string())),
            ColumnData::DateTime(v) => v.map(|d| Value::String(format!("{d:?}"))),
            ColumnData::SmallDateTime(v) => v.map(|d| Value::String(format!("{d:?}"))),
            ColumnData::Time(v) => v.map(|d| Value::String(format!("{d:?}"))),
            ColumnData::Date(v) => v.map(|d| Value::String(format!("{d:?}"))),
            ColumnData::DateTime2(v) => v.map(|d| Value::String(format!("{d:?}"))),
            ColumnData::DateTimeOffset(v) => v.map(|d| Value::String(format!("{d:?}"))),
        }
    }

    async fn run(client: &mut SqlClient, sql: &str) -> Result<QueryResult, DriverError> {
        use futures_util::TryStreamExt;
        let start = Instant::now();
        let mut stream = client
            .query(sql, &[])
            .await
            .map_err(|e| DriverError::QueryFailed(format!("SQL Server query failed: {e}")))?;
        let mut columns: Vec<ColumnInfo> = Vec::new();
        let mut result_rows: Vec<Vec<Option<Value>>> = Vec::new();
        while let Some(item) = stream
            .try_next()
            .await
            .map_err(|e| DriverError::QueryFailed(format!("SQL Server row read failed: {e}")))?
        {
            match item {
                QueryItem::Metadata(meta) => {
                    columns = meta
                        .columns()
                        .iter()
                        .map(|c| ColumnInfo {
                            name: c.name().to_string(),
                            data_type: format!("{:?}", c.column_type()),
                            nullable: true,
                        })
                        .collect();
                }
                QueryItem::Row(row) => {
                    if columns.is_empty() {
                        columns = row
                            .columns()
                            .iter()
                            .map(|c| ColumnInfo {
                                name: c.name().to_string(),
                                data_type: format!("{:?}", c.column_type()),
                                nullable: true,
                            })
                            .collect();
                    }
                    let row_values: Vec<Option<Value>> = row
                        .cells()
                        .map(|(_, data)| Self::value_from_column(data))
                        .collect();
                    result_rows.push(row_values);
                }
            }
        }
        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: None,
            execution_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

#[async_trait]
impl DatabaseDriver for SqlServerDriver {
    fn driver_type(&self) -> DatabaseType {
        "sqlserver".to_string()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let mut client = Self::connect_client(config).await?;
        let result = Self::run(&mut client, "SELECT @@VERSION AS version").await?;
        let version = result
            .rows
            .first()
            .and_then(|r| r.first())
            .cloned()
            .flatten()
            .map(|v| super::http_support::value_display(&v))
            .unwrap_or_default();
        Ok(ServerInfo {
            server_version: version,
            server_type: "sqlserver".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = Self::connect_client(config).await?;
        let pool_id = format!("sqlserver_{}", uuid::Uuid::new_v4());
        self.clients.write().await.insert(pool_id.clone(), client);
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.clients.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let result = Self::run(client, "SELECT name FROM sys.databases ORDER BY name").await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| r.into_iter().next().flatten())
            .map(|v| super::http_support::value_display(&v))
            .collect())
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let result = Self::run(
            client,
            "SELECT name, 'TABLE' AS kind FROM sys.tables UNION ALL SELECT name, 'VIEW' FROM sys.views ORDER BY name",
        )
        .await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| {
                let name = r
                    .get(0)
                    .cloned()
                    .flatten()
                    .map(|v| super::http_support::value_display(&v))?;
                let kind = r
                    .get(1)
                    .cloned()
                    .flatten()
                    .map(|v| super::http_support::value_display(&v))
                    .unwrap_or_default();
                Some(TableInfo {
                    name,
                    schema: None,
                    table_type: if kind == "VIEW" { TableType::View } else { TableType::Table },
                    row_count: None,
                })
            })
            .collect())
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let escaped = table.replace('\'', "''");
        let sql = format!(
            "SELECT c.name AS column_name, t.name AS data_type, c.is_nullable, c.is_identity, \
             dc.definition AS default_value, CAST(ep.value AS nvarchar(max)) AS comment \
             FROM sys.columns c \
             JOIN sys.types t ON c.user_type_id = t.user_type_id \
             LEFT JOIN sys.default_constraints dc ON c.default_object_id = dc.object_id \
             LEFT JOIN sys.extended_properties ep ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
             WHERE c.object_id = OBJECT_ID('{escaped}') ORDER BY c.column_id"
        );
        let result = Self::run(client, &sql).await?;
        let columns = result
            .rows
            .into_iter()
            .filter_map(|r| {
                Some(ColumnSchema {
                    name: r
                        .get(0)
                        .cloned()
                        .flatten()
                        .map(|v| super::http_support::value_display(&v))?,
                    data_type: r
                        .get(1)
                        .cloned()
                        .flatten()
                        .map(|v| super::http_support::value_display(&v))
                        .unwrap_or_default(),
                    nullable: r
                        .get(2)
                        .cloned()
                        .flatten()
                        .map(|v| matches!(v, Value::Bool(true)))
                        .unwrap_or(true),
                    default_value: r
                        .get(4)
                        .cloned()
                        .flatten()
                        .map(|v| super::http_support::value_display(&v)),
                    comment: r
                        .get(5)
                        .cloned()
                        .flatten()
                        .map(|v| super::http_support::value_display(&v)),
                    is_primary_key: false,
                    is_auto_increment: r
                        .get(3)
                        .cloned()
                        .flatten()
                        .map(|v| matches!(v, Value::Bool(true)))
                        .unwrap_or(false),
                })
            })
            .collect();
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        Self::run(client, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let total_start = Instant::now();
        let statements: Vec<String> = sql
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let mut results = Vec::new();
        for stmt in statements {
            let start = Instant::now();
            let limited = if let Some(lim) = limit {
                let upper = stmt.to_uppercase();
                if upper.starts_with("SELECT") && !upper.contains("TOP") {
                    let inner = stmt.trim_start();
                    format!("SELECT TOP {lim} {}", &inner["SELECT".len()..])
                } else {
                    stmt.clone()
                }
            } else {
                stmt.clone()
            };
            let r = Self::run(client, &limited).await?;
            results.push(StatementResult {
                sql: stmt,
                columns: r.columns,
                rows: r.rows,
                rows_affected: r.rows_affected,
                execution_time_ms: r.execution_time_ms,
                truncated: false,
            });
            let _ = start;
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
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        client
            .execute(sql, &[])
            .await
            .map(|r| r.total())
            .map_err(|e| DriverError::QueryFailed(format!("SQL Server execute failed: {e}")))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}
