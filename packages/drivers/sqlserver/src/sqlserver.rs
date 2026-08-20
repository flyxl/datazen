//! SQL Server driver backed by `tiberius`.

use async_trait::async_trait;
use datazen_driver_api::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, QueryItem};
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

    /// Map DataZen SSL mode → (tiberius encryption, trust server certificate).
    ///
    /// - `Disable`: plaintext TDS (no TLS)
    /// - `Prefer` / `Require`: encrypt, trust server cert (common for self-signed)
    /// - `VerifyCa` / `VerifyFull`: encrypt and verify the certificate chain
    fn ssl_settings(mode: &SslMode) -> (EncryptionLevel, bool) {
        match mode {
            SslMode::Disable => (EncryptionLevel::NotSupported, false),
            SslMode::Prefer => (EncryptionLevel::On, true),
            SslMode::Require => (EncryptionLevel::Required, true),
            SslMode::VerifyCa | SslMode::VerifyFull => (EncryptionLevel::Required, false),
        }
    }

    fn build_use_database_sql(database: &str) -> Result<String, DriverError> {
        let trimmed = database.trim();
        if trimmed.is_empty() {
            return Err(DriverError::InvalidConfig(
                "Database name must not be empty".into(),
            ));
        }
        if trimmed.contains('\0') {
            return Err(DriverError::InvalidConfig(
                "Database name contains invalid characters".into(),
            ));
        }
        // Bracket quoting; escape `]` by doubling.
        Ok(format!("USE [{}]", trimmed.replace(']', "]]")))
    }

    fn build_table_schema_sql(table: &str) -> String {
        let escaped = table.replace('\'', "''");
        format!(
            "SELECT c.name AS column_name, t.name AS data_type, c.is_nullable, c.is_identity, \
             dc.definition AS default_value, CAST(ep.value AS nvarchar(max)) AS comment, \
             CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk \
             FROM sys.columns c \
             JOIN sys.types t ON c.user_type_id = t.user_type_id \
             LEFT JOIN sys.default_constraints dc ON c.default_object_id = dc.object_id \
             LEFT JOIN sys.extended_properties ep ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
             LEFT JOIN ( \
               SELECT ic.object_id, ic.column_id \
               FROM sys.index_columns ic \
               INNER JOIN sys.indexes i ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
               WHERE i.is_primary_key = 1 \
             ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
             WHERE c.object_id = OBJECT_ID('{escaped}') ORDER BY c.column_id"
        )
    }

    fn bit_true(v: &Option<Value>) -> bool {
        matches!(v, Some(Value::Bool(true)) | Some(Value::Integer(1)))
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
        let (encryption, trust) = Self::ssl_settings(&config.ssl_mode);
        cfg.encryption(encryption);
        if trust {
            cfg.trust_cert();
        }
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
            .map_err(|e| {
                DriverError::ConnectionFailed(format!("SQL Server connect failed: {e}"))
            })?;
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
                Value::String(format!(
                    "0x{}",
                    b.iter().map(|x| format!("{x:02x}")).collect::<String>()
                ))
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

    fn columns_from_tiberius(cols: &[tiberius::Column]) -> Vec<ColumnInfo> {
        cols.iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: format!("{:?}", c.column_type()),
                nullable: true,
            })
            .collect()
    }

    async fn stream_one(
        client: &mut SqlClient,
        stmt: &str,
        limit: Option<u32>,
        index: usize,
        on_event: &QueryStreamCallback,
    ) -> Result<(), DriverError> {
        use futures_util::TryStreamExt;
        let (effective, applied) = apply_sqlserver_top(stmt, limit);
        let stmt_start = Instant::now();
        let mut stream = client
            .query(&effective, &[])
            .await
            .map_err(|e| DriverError::QueryFailed(format!("SQL Server query failed: {e}")))?;
        let mut batcher =
            QueryRowBatcher::new(Arc::clone(on_event), index, stmt.to_string(), applied);
        while let Some(item) = stream
            .try_next()
            .await
            .map_err(|e| DriverError::QueryFailed(format!("SQL Server row read failed: {e}")))?
        {
            match item {
                QueryItem::Metadata(meta) => {
                    batcher.start(Self::columns_from_tiberius(meta.columns()));
                }
                QueryItem::Row(row) => {
                    if !batcher.started() {
                        batcher.start(Self::columns_from_tiberius(row.columns()));
                    }
                    let vals: Vec<Option<Value>> = row
                        .cells()
                        .map(|(_, data)| Self::value_from_column(data))
                        .collect();
                    if !batcher.push(vals) {
                        break;
                    }
                }
            }
        }
        batcher.finish(stmt_start.elapsed().as_millis() as u64, None);
        Ok(())
    }
}

fn apply_sqlserver_top(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };
    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("SELECT") {
        return (stmt.to_string(), None);
    }
    let after_select = trimmed[6..].trim_start();
    let after_upper = after_select.to_ascii_uppercase();
    if after_upper.starts_with("DISTINCT") {
        let after_distinct = after_select[8..].trim_start();
        if after_distinct.to_ascii_uppercase().starts_with("TOP") {
            return (stmt.to_string(), Some(lim));
        }
        return (
            format!("SELECT DISTINCT TOP {} {after_distinct}", lim + 1),
            Some(lim),
        );
    }
    if after_upper.starts_with("TOP") {
        return (stmt.to_string(), Some(lim));
    }
    (format!("SELECT TOP {} {after_select}", lim + 1), Some(lim))
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
            .map(|v| datazen_driver_http_support::value_display(&v))
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
            .map(|v| datazen_driver_http_support::value_display(&v))
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
                    .map(|v| datazen_driver_http_support::value_display(&v))?;
                let kind = r
                    .get(1)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))
                    .unwrap_or_default();
                Some(TableInfo {
                    name,
                    schema: None,
                    table_type: if kind == "VIEW" {
                        TableType::View
                    } else {
                        TableType::Table
                    },
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
        let sql = Self::build_table_schema_sql(table);
        let result = Self::run(client, &sql).await?;
        let columns: Vec<ColumnSchema> = result
            .rows
            .into_iter()
            .filter_map(|r| {
                let is_pk = Self::bit_true(&r.get(6).cloned().flatten());
                Some(ColumnSchema {
                    name: r
                        .get(0)
                        .cloned()
                        .flatten()
                        .map(|v| datazen_driver_http_support::value_display(&v))?,
                    data_type: r
                        .get(1)
                        .cloned()
                        .flatten()
                        .map(|v| datazen_driver_http_support::value_display(&v))
                        .unwrap_or_default(),
                    nullable: r
                        .get(2)
                        .cloned()
                        .flatten()
                        .map(|v| Self::bit_true(&Some(v)))
                        .unwrap_or(true),
                    default_value: r
                        .get(4)
                        .cloned()
                        .flatten()
                        .map(|v| datazen_driver_http_support::value_display(&v)),
                    comment: r
                        .get(5)
                        .cloned()
                        .flatten()
                        .map(|v| datazen_driver_http_support::value_display(&v)),
                    is_primary_key: is_pk,
                    is_auto_increment: Self::bit_true(&r.get(3).cloned().flatten()),
                })
            })
            .collect();
        let primary_keys: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
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

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let statements: Vec<String> = sql
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if statements.is_empty() {
            on_event(QueryStreamEvent::Done { total_time_ms: 0 });
            return Ok(());
        }
        let total_start = Instant::now();
        for (index, stmt) in statements.iter().enumerate() {
            Self::stream_one(client, stmt, limit, index, &on_event).await?;
        }
        on_event(QueryStreamEvent::Done {
            total_time_ms: total_start.elapsed().as_millis() as u64,
        });
        Ok(())
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

    fn supports_explain(&self) -> bool {
        true
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        // SHOWPLAN_TEXT returns the plan without executing; always clear session flag.
        let enable = Self::run(client, "SET SHOWPLAN_TEXT ON").await;
        if let Err(e) = enable {
            let _ = Self::run(client, "SET SHOWPLAN_TEXT OFF").await;
            return Err(e);
        }
        let plan = Self::run(client, sql).await;
        let disable = Self::run(client, "SET SHOWPLAN_TEXT OFF").await;
        let result = plan?;
        disable?;
        Ok(datazen_driver_http_support::explain_result_from_query(
            result,
        ))
    }

    async fn use_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        let sql = Self::build_use_database_sql(database)?;
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        Self::run(client, &sql).await.map(|_| ())
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        crate::admin_commands::sqlserver_admin_command_definitions()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Err(DriverError::Unsupported(_)) => {}
            other => return other,
        }
        let sql = crate::admin_commands::build_admin_sql(command, &input)?;
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        Self::run(client, &sql).await?;
        Ok(CommandResult {
            data: serde_json::json!({ "ok": true }),
        })
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(crate::structure::sqlserver_capabilities(
            &self.driver_type(),
        ))
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = self.structure_capabilities(handle).await?;
        crate::structure::plan_structure_changes(&caps, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiberius::EncryptionLevel;

    #[test]
    fn ssl_disable_is_plaintext() {
        assert_eq!(
            SqlServerDriver::ssl_settings(&SslMode::Disable),
            (EncryptionLevel::NotSupported, false)
        );
    }

    #[test]
    fn ssl_require_trusts_cert() {
        assert_eq!(
            SqlServerDriver::ssl_settings(&SslMode::Require),
            (EncryptionLevel::Required, true)
        );
    }

    #[test]
    fn ssl_verify_full_requires_encryption_without_trust() {
        assert_eq!(
            SqlServerDriver::ssl_settings(&SslMode::VerifyFull),
            (EncryptionLevel::Required, false)
        );
    }

    #[test]
    fn build_use_database_sql_brackets_and_escapes() {
        assert_eq!(
            SqlServerDriver::build_use_database_sql(" sales ").unwrap(),
            "USE [sales]"
        );
        assert_eq!(
            SqlServerDriver::build_use_database_sql("a]b").unwrap(),
            "USE [a]]b]"
        );
        assert!(SqlServerDriver::build_use_database_sql("  ").is_err());
        assert!(SqlServerDriver::build_use_database_sql("bad\0name").is_err());
    }

    #[test]
    fn build_table_schema_sql_includes_primary_key_join() {
        let sql = SqlServerDriver::build_table_schema_sql("dbo.users");
        assert!(sql.contains("is_primary_key = 1"));
        assert!(sql.contains("OBJECT_ID('dbo.users')"));
        assert!(sql.contains("is_pk"));
    }

    #[test]
    fn apply_sqlserver_top_inserts_plus_one() {
        assert_eq!(
            apply_sqlserver_top("SELECT * FROM t", None),
            ("SELECT * FROM t".into(), None)
        );
        assert_eq!(
            apply_sqlserver_top("SELECT * FROM t", Some(10)),
            ("SELECT TOP 11 * FROM t".into(), Some(10))
        );
        assert_eq!(
            apply_sqlserver_top("SELECT TOP 5 * FROM t", Some(10)),
            ("SELECT TOP 5 * FROM t".into(), Some(10))
        );
        assert_eq!(
            apply_sqlserver_top("SELECT DISTINCT name FROM t", Some(3)),
            ("SELECT DISTINCT TOP 4 name FROM t".into(), Some(3))
        );
        assert_eq!(
            apply_sqlserver_top("INSERT INTO t VALUES (1)", Some(10)),
            ("INSERT INTO t VALUES (1)".into(), None)
        );
        assert_eq!(
            apply_sqlserver_top("select id from t", Some(1)),
            ("SELECT TOP 2 id from t".into(), Some(1))
        );
    }
}
