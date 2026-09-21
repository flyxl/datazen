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

    /// `[database].` prefix for a catalog view, or an empty string when the
    /// caller targets the connection's current database. SQL Server accepts
    /// three-part names, so a metadata read of another database never needs a
    /// session-level `USE`.
    fn catalog_prefix(database: &str) -> String {
        let trimmed = database.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            // Bracket quoting; escape `]` by doubling.
            format!("[{}].", trimmed.replace(']', "]]"))
        }
    }

    /// List tables and views together with the schema that owns them.
    ///
    /// The schema column is mandatory: SQL Server has a real schema level, and
    /// the backup/dump path feeds `TableInfo::schema` straight back into
    /// `get_table_schema`, which the contract validator rejects when it is
    /// missing. A schema filter is applied only when the caller pinned one;
    /// `None` lists every schema in the database, which is the set the
    /// connection tree groups by.
    fn build_tables_sql(database: &str, schema: Option<&str>) -> String {
        let catalog = Self::catalog_prefix(database);
        let filter = match schema.map(str::trim).filter(|s| !s.is_empty()) {
            Some(schema) => format!(" WHERE s.name = '{}'", schema.replace('\'', "''")),
            None => String::new(),
        };
        format!(
            "SELECT s.name AS schema_name, t.name AS table_name, 'TABLE' AS kind \
             FROM {catalog}sys.tables t JOIN {catalog}sys.schemas s ON t.schema_id = s.schema_id{filter} \
             UNION ALL \
             SELECT s.name, v.name, 'VIEW' \
             FROM {catalog}sys.views v JOIN {catalog}sys.schemas s ON v.schema_id = s.schema_id{filter} \
             ORDER BY schema_name, table_name"
        )
    }

    /// Columns of one table or view, filtered by the explicit `(schema, table)`.
    ///
    /// `database` is inlined as a catalog prefix when non-empty, so reading
    /// another database never needs a session `USE`. `INFORMATION_SCHEMA.COLUMNS`
    /// is the column source; the catalog-qualified `sys.*` views add the
    /// identity / default / primary-key / comment metadata it does not expose.
    /// The object lookup is a derived table keyed on `(schema, name)` so a
    /// same-named relation in another schema can never duplicate or steal rows.
    fn build_table_schema_sql(database: &str, schema: &str, table: &str) -> String {
        let catalog = Self::catalog_prefix(database);
        let schema = schema.replace('\'', "''");
        let table = table.replace('\'', "''");
        format!(
            "SELECT c.COLUMN_NAME AS column_name, c.DATA_TYPE AS data_type, \
             CAST(CASE WHEN c.IS_NULLABLE = 'YES' THEN 1 ELSE 0 END AS bit) AS is_nullable, \
             CAST(CASE WHEN sc.is_identity = 1 THEN 1 ELSE 0 END AS bit) AS is_identity, \
             dc.definition AS default_value, CAST(ep.value AS nvarchar(max)) AS comment, \
             CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk \
             FROM {catalog}INFORMATION_SCHEMA.COLUMNS c \
             LEFT JOIN ( \
               SELECT o.object_id, o.name AS object_name, s.name AS schema_name \
               FROM {catalog}sys.objects o \
               JOIN {catalog}sys.schemas s ON s.schema_id = o.schema_id \
             ) obj ON obj.object_name = c.TABLE_NAME AND obj.schema_name = c.TABLE_SCHEMA \
             LEFT JOIN {catalog}sys.columns sc ON sc.object_id = obj.object_id AND sc.name = c.COLUMN_NAME \
             LEFT JOIN {catalog}sys.default_constraints dc ON dc.parent_object_id = obj.object_id AND dc.parent_column_id = sc.column_id \
             LEFT JOIN {catalog}sys.extended_properties ep ON ep.major_id = obj.object_id AND ep.minor_id = sc.column_id AND ep.name = 'MS_Description' \
             LEFT JOIN ( \
               SELECT ic.object_id, ic.column_id \
               FROM {catalog}sys.index_columns ic \
               INNER JOIN {catalog}sys.indexes i ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
               WHERE i.is_primary_key = 1 \
             ) pk ON pk.object_id = obj.object_id AND pk.column_id = sc.column_id \
             WHERE c.TABLE_SCHEMA = '{schema}' AND c.TABLE_NAME = '{table}' \
             ORDER BY c.ORDINAL_POSITION"
        )
    }

    /// Batch columns for every table/view in `database` (optionally narrowed to
    /// one `schema`). `database` is inlined as a catalog prefix, so a batch read
    /// of another database needs no session `USE`.
    fn build_all_columns_sql(database: &str, schema: Option<&str>) -> String {
        let catalog = Self::catalog_prefix(database);
        let filter = match schema.map(str::trim).filter(|s| !s.is_empty()) {
            Some(schema) => format!(" AND s.name = '{}'", schema.replace('\'', "''")),
            None => String::new(),
        };
        format!(
            "SELECT s.name AS schema_name, o.name AS table_name, c.name AS column_name, \
             tp.name AS data_type, c.is_nullable, c.is_identity, dc.definition AS default_value, \
             CAST(ep.value AS nvarchar(max)) AS comment, \
             CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk \
             FROM {catalog}sys.columns c \
             JOIN {catalog}sys.objects o ON c.object_id = o.object_id \
             JOIN {catalog}sys.schemas s ON o.schema_id = s.schema_id \
             JOIN {catalog}sys.types tp ON c.user_type_id = tp.user_type_id \
             LEFT JOIN {catalog}sys.default_constraints dc ON c.default_object_id = dc.object_id \
             LEFT JOIN {catalog}sys.extended_properties ep ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
             LEFT JOIN ( \
               SELECT ic.object_id, ic.column_id \
               FROM {catalog}sys.index_columns ic \
               INNER JOIN {catalog}sys.indexes i ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
               WHERE i.is_primary_key = 1 \
             ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
             WHERE o.type IN ('U', 'V'){filter} \
             ORDER BY s.name, o.name, c.column_id"
        )
    }

    /// Effective schema for a single-table read: the explicit argument wins,
    /// otherwise the driver's conventional default (`dbo`). Never a hardcoded
    /// literal at the call site, so the convention stays owned by the driver.
    fn effective_schema<'a>(&self, schema: Option<&'a str>) -> Option<&'a str> {
        schema
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or(self.default_schema())
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
        let (encryption, default_trust) = Self::ssl_settings(&config.ssl_mode);
        cfg.encryption(encryption);

        let trust_server_certificate = config
            .options
            .as_ref()
            .and_then(|options| options.get("trustServerCertificate"))
            .and_then(|value| value.as_bool())
            .unwrap_or(default_trust);
        if trust_server_certificate {
            cfg.trust_cert();
        }

        if let Some(application_name) = config
            .options
            .as_ref()
            .and_then(|options| options.get("applicationName"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            cfg.application_name(application_name);
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

    /// SQL Server addresses relations as `schema.table`, so a single-table read
    /// must be given an explicit schema and `None` is a caller bug.
    fn has_schema_level(&self) -> bool {
        true
    }

    /// SQL Server resolves unqualified names in the user's default schema,
    /// which is `dbo` unless the login was created with another one.
    fn default_schema(&self) -> Option<&'static str> {
        Some("dbo")
    }

    /// F7: qualify unqualified table references with the T-SQL three-part
    /// name (`[db].[schema].t`; `[schema].t` when only a schema is given).
    /// A database-only target is never inlined — a two-part `[db].t` would
    /// mean *schema* db in T-SQL — and stays on the host
    /// `ensure_session_database` pin. Temp tables are skipped. Parse
    /// failures pass SQL through unchanged; see `sql_target::qualify_sql`.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        Some(crate::sql_target::qualify_sql(sql, database, schema))
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
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        // Listing may legitimately span every schema; an explicit schema just
        // narrows the set. A schema-less model would be a caller bug.
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let sql = Self::build_tables_sql(database, schema);
        let result = Self::run(client, &sql).await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| {
                let schema_name = r
                    .get(0)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))
                    .unwrap_or_default();
                let name = r
                    .get(1)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))?;
                let kind = r
                    .get(2)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))
                    .unwrap_or_default();
                Some(TableInfo {
                    name,
                    // The backup/dump path feeds this back into
                    // `get_table_schema`, which requires an exact schema.
                    schema: Some(schema_name),
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
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        // The validator already guarantees a schema for this driver; resolve it
        // through `default_schema()` rather than hardcoding `dbo` at the call
        // site, so the convention stays owned by the driver.
        let schema = self.effective_schema(schema).unwrap_or_default();
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let sql = Self::build_table_schema_sql(database, schema, table);
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

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let result = Self::run(client, &Self::build_all_columns_sql(database, schema)).await?;

        let mut all_columns: HashMap<String, (Vec<ColumnSchema>, Vec<String>)> = HashMap::new();
        let mut owners: HashMap<String, String> = HashMap::new();

        for row in &result.rows {
            // SQL: schema_name(0), table_name(1), column_name(2), data_type(3),
            //      is_nullable(4), is_identity(5), default_value(6), comment(7),
            //      is_pk(8)
            let table_schema = row
                .get(0)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let table_name = row
                .get(1)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let col_name = row
                .get(2)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let data_type = row
                .get(3)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let nullable = Self::bit_true(&row.get(4).cloned().flatten());
            let is_pk = Self::bit_true(&row.get(8).cloned().flatten());

            // The payload is keyed by bare table name, so two same-named tables
            // in different schemas cannot both be represented. Keep the first
            // and say so rather than merging their columns into one table.
            if let Some(existing) = owners.get(&table_name) {
                if existing != &table_schema {
                    tracing::warn!(
                        table = %table_name,
                        kept = %existing,
                        skipped = %table_schema,
                        "get_all_columns: same-named table in another schema skipped"
                    );
                }
                continue;
            }
            owners.insert(table_name.clone(), table_schema);

            let column = ColumnSchema {
                name: col_name.clone(),
                data_type,
                nullable,
                default_value: row
                    .get(6)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v)),
                comment: row
                    .get(7)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v)),
                is_primary_key: is_pk,
                is_auto_increment: Self::bit_true(&row.get(5).cloned().flatten()),
            };

            let entry = all_columns.entry(table_name).or_default();
            entry.0.push(column);
            if is_pk {
                entry.1.push(col_name);
            }
        }

        Ok(all_columns)
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
        if let Some(result) =
            try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
        {
            return Ok(result);
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
    fn sqlserver_declares_a_schema_level_with_dbo_default() {
        let driver = SqlServerDriver::new();
        assert!(driver.has_schema_level());
        assert_eq!(driver.default_schema(), Some("dbo"));
    }

    #[test]
    fn effective_schema_prefers_the_argument_then_the_convention() {
        let driver = SqlServerDriver::new();
        assert_eq!(driver.effective_schema(Some("sales")), Some("sales"));
        assert_eq!(driver.effective_schema(Some("  sales ")), Some("sales"));
        assert_eq!(driver.effective_schema(Some("   ")), Some("dbo"));
        assert_eq!(driver.effective_schema(None), Some("dbo"));
    }

    #[test]
    fn exact_schema_reads_require_an_explicit_schema() {
        let driver = SqlServerDriver::new();
        assert!(
            validate_schema_target(&driver, "app", Some("dbo"), SchemaScope::ExactSchema).is_ok()
        );
        // Replaces the old `use_database` session-switch coverage: the target is
        // now the explicit argument, and a missing schema is a hard error
        // instead of silently landing on whatever the session pointed at.
        assert!(
            validate_schema_target(&driver, "app", None, SchemaScope::ExactSchema).is_err(),
            "a schema-aware driver must reject an exact-schema read without a schema"
        );
        assert!(validate_schema_target(&driver, "app", None, SchemaScope::AnySchema).is_ok());
    }

    #[test]
    fn build_table_schema_sql_filters_schema_and_qualifies_catalog() {
        let sql = SqlServerDriver::build_table_schema_sql("sales", "dbo", "users");
        assert!(sql.contains("FROM [sales].INFORMATION_SCHEMA.COLUMNS"));
        assert!(sql.contains("c.TABLE_SCHEMA = 'dbo'"));
        assert!(sql.contains("c.TABLE_NAME = 'users'"));
        assert!(sql.contains("is_primary_key = 1"));
        assert!(sql.contains("is_pk"));
        // The object lookup must be keyed on (schema, name): joining on the bare
        // name would let a same-named table in another schema duplicate rows.
        assert!(sql.contains("obj.schema_name = c.TABLE_SCHEMA"));
        assert!(!sql.contains("ON o.name = c.TABLE_NAME"));
        // No session switch may be embedded in a read path.
        assert!(!sql.to_uppercase().contains("USE ["));
    }

    #[test]
    fn build_table_schema_sql_stays_local_when_database_is_blank() {
        let sql = SqlServerDriver::build_table_schema_sql("", "dbo", "users");
        assert!(sql.contains("FROM INFORMATION_SCHEMA.COLUMNS"));
        assert!(!sql.contains("[]."));
    }

    #[test]
    fn build_table_schema_sql_escapes_quotes() {
        let sql = SqlServerDriver::build_table_schema_sql("db]", "d'bo", "us'ers");
        assert!(sql.contains("FROM [db]]].INFORMATION_SCHEMA.COLUMNS"));
        assert!(sql.contains("c.TABLE_SCHEMA = 'd''bo'"));
        assert!(sql.contains("c.TABLE_NAME = 'us''ers'"));
    }

    #[test]
    fn build_tables_sql_populates_schema_and_optionally_filters() {
        let all = SqlServerDriver::build_tables_sql("sales", None);
        assert!(all.contains("JOIN [sales].sys.schemas s"));
        assert!(all.contains("s.name AS schema_name"));
        assert!(!all.contains("WHERE s.name"));

        let filtered = SqlServerDriver::build_tables_sql("sales", Some("dbo"));
        assert!(filtered.contains("WHERE s.name = 'dbo'"));
        assert_eq!(filtered.matches("WHERE s.name = 'dbo'").count(), 2);
    }

    #[test]
    fn build_all_columns_sql_filters_schema_and_qualifies_catalog() {
        let sql = SqlServerDriver::build_all_columns_sql("sales", Some("dbo"));
        assert!(sql.contains("FROM [sales].sys.columns c"));
        assert!(sql.contains("AND s.name = 'dbo'"));
        assert!(sql.contains("s.name AS schema_name"));
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
