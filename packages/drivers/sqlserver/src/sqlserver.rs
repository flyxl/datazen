//! SQL Server driver backed by `tiberius`.

use async_trait::async_trait;
use datazen_driver_api::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, QueryItem};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

type SqlClient = Client<Compat<TcpStream>>;

pub struct SqlServerDriver {
    clients: RwLock<HashMap<String, SqlClient>>,
    transactions: Mutex<HashMap<String, ActiveTransaction>>,
}

struct ActiveTransaction {
    id: String,
    restore_isolation: Option<&'static str>,
}

impl SqlServerDriver {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
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

    fn quote_identifier(identifier: &str) -> Result<String, DriverError> {
        if identifier.is_empty() || identifier.contains('\0') {
            return Err(DriverError::InvalidConfig(
                "SQL Server transfer relation identifiers must be non-empty and contain no NUL"
                    .into(),
            ));
        }
        Ok(format!("[{}]", identifier.replace(']', "]]")))
    }

    fn transfer_identity_relation(
        database: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Result<String, DriverError> {
        let mut parts = Vec::with_capacity(3);
        if !database.trim().is_empty() {
            parts.push(Self::quote_identifier(database)?);
        }
        let schema = schema
            .map(str::trim)
            .filter(|schema| !schema.is_empty())
            .unwrap_or("dbo");
        parts.push(Self::quote_identifier(schema)?);
        parts.push(Self::quote_identifier(table)?);
        Ok(parts.join("."))
    }

    /// Accept only a relation rendered by Data Transfer's SQL Server
    /// identifier quoter. This prevents a caller from smuggling SQL into the
    /// session toggle or the OBJECT_ID condition.
    fn quoted_transfer_relation_parts(relation: &str) -> Option<Vec<&str>> {
        let bytes = relation.as_bytes();
        let mut cursor = 0;
        let mut parts = Vec::new();
        while cursor < bytes.len() {
            if bytes[cursor] != b'[' {
                return None;
            }
            let part_start = cursor;
            cursor += 1;
            let mut has_identifier_content = false;
            let mut closed = false;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b']' if bytes.get(cursor + 1) == Some(&b']') => {
                        has_identifier_content = true;
                        cursor += 2;
                    }
                    b']' => {
                        cursor += 1;
                        closed = true;
                        break;
                    }
                    _ => {
                        has_identifier_content = true;
                        cursor += 1;
                    }
                }
            }
            if !closed || !has_identifier_content {
                return None;
            }
            parts.push(&relation[part_start..cursor]);
            if cursor == bytes.len() {
                break;
            }
            if bytes[cursor] != b'.' {
                return None;
            }
            cursor += 1;
            if cursor == bytes.len() {
                return None;
            }
        }
        (1..=3).contains(&parts.len()).then_some(parts)
    }

    fn render_sql_file_identity_insert(
        insert_sql: &str,
        target_relation: &str,
        mapped_target_columns: &[String],
    ) -> Result<String, DriverError> {
        if mapped_target_columns.is_empty() {
            return Ok(insert_sql.to_string());
        }
        if mapped_target_columns
            .iter()
            .any(|column| column.is_empty() || column.contains('\0'))
        {
            return Err(DriverError::InvalidConfig(
                "SQL Server SQL-file identity wrapper requires valid mapped target column names"
                    .into(),
            ));
        }
        let relation_parts =
            Self::quoted_transfer_relation_parts(target_relation).ok_or_else(|| {
                DriverError::InvalidConfig(
                    "SQL Server SQL-file identity wrapper requires a safely quoted target relation"
                        .into(),
                )
            })?;
        let object_name = target_relation.replace('\'', "''");
        let identity_catalog = relation_parts
            .get(2)
            .map(|_| format!("{}.sys.identity_columns", relation_parts[0]))
            .unwrap_or_else(|| "sys.identity_columns".into());
        let mapped_columns = mapped_target_columns
            .iter()
            .map(|column| format!("N'{}'", column.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!(
            "IF EXISTS (SELECT 1 FROM {identity_catalog} WHERE object_id = OBJECT_ID(N'{object_name}', N'U') AND name IN ({mapped_columns}))\nBEGIN\n    BEGIN TRY\n        SET IDENTITY_INSERT {target_relation} ON;\n        {insert_sql};\n        SET IDENTITY_INSERT {target_relation} OFF;\n    END TRY\n    BEGIN CATCH\n        BEGIN TRY\n            SET IDENTITY_INSERT {target_relation} OFF;\n        END TRY\n        BEGIN CATCH\n            THROW;\n        END CATCH\n        THROW;\n    END CATCH\nEND\nELSE\nBEGIN\n    {insert_sql};\nEND"
        ))
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

    /// Template used by catalog-builder unit tests; object names are bound by
    /// the caller rather than interpolated into this SQL text.
    #[cfg(test)]
    fn build_table_schema_sql(database: &str, schema: &str, table: &str) -> String {
        let _ = (schema, table);
        crate::metadata::columns_sql(database)
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
             CASE \
               WHEN tp.is_user_defined = 1 THEN QUOTENAME(tp_schema.name) + N'.' + QUOTENAME(tp.name) \
               WHEN tp.name IN ('nvarchar', 'nchar') THEN CONCAT(tp.name, '(', CASE WHEN c.max_length = -1 THEN 'max' ELSE CONVERT(varchar(10), c.max_length / 2) END, ')') \
               WHEN tp.name IN ('varchar', 'char', 'varbinary', 'binary') THEN CONCAT(tp.name, '(', CASE WHEN c.max_length = -1 THEN 'max' ELSE CONVERT(varchar(10), c.max_length) END, ')') \
               WHEN tp.name IN ('decimal', 'numeric') THEN CONCAT(tp.name, '(', c.precision, ',', c.scale, ')') \
               WHEN tp.name IN ('time', 'datetime2', 'datetimeoffset') THEN CONCAT(tp.name, '(', c.scale, ')') \
               WHEN tp.name = 'float' THEN CONCAT(tp.name, '(', c.precision, ')') \
               ELSE tp.name \
             END AS data_type, c.is_nullable, c.is_identity, dc.definition AS default_value, \
             CAST(ep.value AS nvarchar(max)) AS comment, \
             CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk, \
             ISNULL(pk.key_ordinal, 0) AS pk_ordinal \
             FROM {catalog}sys.columns c \
             JOIN {catalog}sys.objects o ON c.object_id = o.object_id \
             JOIN {catalog}sys.schemas s ON o.schema_id = s.schema_id \
             JOIN {catalog}sys.types tp ON c.user_type_id = tp.user_type_id \
             LEFT JOIN {catalog}sys.schemas tp_schema ON tp_schema.schema_id = tp.schema_id \
             LEFT JOIN {catalog}sys.default_constraints dc ON c.default_object_id = dc.object_id \
             LEFT JOIN {catalog}sys.extended_properties ep ON ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
             LEFT JOIN ( \
               SELECT ic.object_id, ic.column_id, ic.key_ordinal \
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

    fn value_from_column(data: &ColumnData<'static>) -> Option<Value> {
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
            ColumnData::Binary(v) => v.as_ref().map(|b| Value::Bytes(b.to_vec())),
            ColumnData::Numeric(v) => v.map(|n| Value::String(n.to_string())),
            ColumnData::Xml(v) => v.as_ref().map(|x| Value::String(x.to_string())),
            ColumnData::DateTime(_)
            | ColumnData::SmallDateTime(_)
            | ColumnData::Time(_)
            | ColumnData::Date(_)
            | ColumnData::DateTime2(_)
            | ColumnData::DateTimeOffset(_) => Self::temporal_value(data),
        }
    }

    /// TDS date/time values arrive as tiberius' own calendar structs, whose
    /// `Debug` output (`Date(739617)`, `Time { increments: … }`) is meaningless
    /// to a user. Decode them through tiberius' `chrono` bridge and render the
    /// same textual shapes the MySQL and PostgreSQL drivers return:
    /// `YYYY-MM-DD`, `HH:MM:SS[.f]`, `YYYY-MM-DD HH:MM:SS[.f]` and RFC 3339 for
    /// the offset-aware type.
    fn temporal_value(data: &ColumnData<'static>) -> Option<Value> {
        use tiberius::time::chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
        use tiberius::FromSql;

        // A `None` payload is a real SQL NULL and must stay NULL. A *failed*
        // conversion is not a NULL, so it degrades to the raw shape rather than
        // silently turning a value into NULL.
        let payload_is_null = match data {
            ColumnData::Date(v) => v.is_none(),
            ColumnData::Time(v) => v.is_none(),
            ColumnData::DateTime(v) => v.is_none(),
            ColumnData::SmallDateTime(v) => v.is_none(),
            ColumnData::DateTime2(v) => v.is_none(),
            ColumnData::DateTimeOffset(v) => v.is_none(),
            _ => return None,
        };
        if payload_is_null {
            return None;
        }

        let rendered = match data {
            ColumnData::Date(_) => NaiveDate::from_sql(data)
                .ok()
                .flatten()
                .map(|d| d.to_string()),
            ColumnData::Time(_) => NaiveTime::from_sql(data)
                .ok()
                .flatten()
                .map(|t| t.to_string()),
            ColumnData::DateTime(_) | ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => {
                NaiveDateTime::from_sql(data)
                    .ok()
                    .flatten()
                    .map(|dt| dt.to_string())
            }
            ColumnData::DateTimeOffset(_) => DateTime::<FixedOffset>::from_sql(data)
                .ok()
                .flatten()
                .map(|dt| dt.to_rfc3339()),
            _ => None,
        };
        Some(Value::String(
            rendered.unwrap_or_else(|| format!("{data:?}")),
        ))
    }

    async fn run(client: &mut SqlClient, sql: &str) -> Result<QueryResult, DriverError> {
        Self::run_routed(client, sql, false).await
    }

    async fn run_with_params(
        client: &mut SqlClient,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        let bound = crate::parameters::bind_values(params);
        let refs = crate::parameters::to_sql_refs(&bound);
        Self::run_routed_with_params(client, sql, &refs, false).await
    }

    /// Read a result set from a statement that must go out as a real batch even
    /// though its text would normally be routed through `sp_executesql`.
    ///
    /// `SET SHOWPLAN_TEXT ON` is the reason this exists: while the flag is set
    /// the server answers with the plan *instead of executing*, and the RPC path
    /// (prepare + `sp_executesql`) returns no rows for it — the plan only
    /// arrives over a plain batch.
    async fn run_batch(client: &mut SqlClient, sql: &str) -> Result<QueryResult, DriverError> {
        Self::run_routed(client, sql, true).await
    }

    async fn execute_batch(client: &mut SqlClient, sql: &str) -> Result<(), DriverError> {
        use futures_util::TryStreamExt;
        let mut stream = client
            .simple_query(sql)
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?;
        while stream
            .try_next()
            .await
            .map_err(|e| DriverError::TransactionError(e.to_string()))?
            .is_some()
        {}
        Ok(())
    }

    async fn current_isolation_level(client: &mut SqlClient) -> Result<&'static str, DriverError> {
        let result = Self::run_batch(client, "DBCC USEROPTIONS WITH NO_INFOMSGS").await?;
        let value = result.rows.iter().find_map(|row| {
            let name = row
                .first()
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))?;
            if !name.eq_ignore_ascii_case("isolation level") {
                return None;
            }
            row.get(1)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
        });
        let normalized = value.map(|value| value.to_ascii_lowercase());
        match normalized.as_deref() {
            Some("read uncommitted") => Ok("READ UNCOMMITTED"),
            Some("read committed" | "read committed snapshot") => Ok("READ COMMITTED"),
            Some("repeatable read") => Ok("REPEATABLE READ"),
            Some("serializable") => Ok("SERIALIZABLE"),
            Some("snapshot") => Ok("SNAPSHOT"),
            _ => Err(DriverError::TransactionError(
                "DBCC USEROPTIONS did not return a recognized isolation level".into(),
            )),
        }
    }

    async fn ensure_no_open_transaction(client: &mut SqlClient) -> Result<(), DriverError> {
        let result = Self::run(client, "SELECT @@TRANCOUNT AS [transaction_count]").await?;
        let count = result
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(Option::as_ref)
            .map(datazen_driver_http_support::value_display)
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or_else(|| {
                DriverError::TransactionError(
                    "SQL Server did not return the active transaction count".into(),
                )
            })?;
        if count != 0 {
            return Err(DriverError::TransactionError(
                "SQL Server session already has an open transaction".into(),
            ));
        }
        Ok(())
    }

    async fn run_routed(
        client: &mut SqlClient,
        sql: &str,
        force_batch: bool,
    ) -> Result<QueryResult, DriverError> {
        Self::run_routed_with_params(client, sql, &[], force_batch).await
    }

    async fn run_routed_with_params(
        client: &mut SqlClient,
        sql: &str,
        params: &[&dyn tiberius::ToSql],
        force_batch: bool,
    ) -> Result<QueryResult, DriverError> {
        use futures_util::TryStreamExt;
        let start = Instant::now();
        let mut stream = if force_batch || needs_own_batch(sql) {
            if !params.is_empty() {
                return Err(DriverError::Unsupported(
                    "SQL Server batch-only statements cannot accept bound parameters; use a parameterized RPC-compatible statement"
                        .into(),
                ));
            }
            client.simple_query(sql).await
        } else {
            client.query(sql, params).await
        }
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
        let mut stream = if needs_own_batch(&effective) {
            client.simple_query(&effective).await
        } else {
            client.query(&effective, &[]).await
        }
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

/// Split a multi-statement script into individual statements.
///
/// A plain `split(';')` breaks on any semicolon inside a string literal, a
/// bracketed identifier or a comment (`SELECT ';' AS [a]` failed with error 105
/// "Unclosed quotation mark"), so this delegates to the shared, quote/comment
/// aware scanner in `driver-api`.
fn split_statements(sql: &str) -> Vec<String> {
    use datazen_driver_api::sql_split::{is_comment_only_or_empty, split_sql_statements};
    split_sql_statements(sql)
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !is_comment_only_or_empty(s))
        .collect()
}

/// T-SQL accepts a few statements **only as the first statement of a batch**:
/// the programmable-object definitions (`CREATE`/`ALTER` `SCHEMA`, `VIEW`,
/// `PROCEDURE`/`PROC`, `FUNCTION`, `TRIGGER`, `RULE`, `DEFAULT`). tiberius sends
/// `Client::query`/`Client::execute` through `sp_executesql`, which rejects them
/// with error 156 (`Incorrect syntax near the keyword 'SCHEMA'`) — verified
/// live against Azure SQL Database. Such statements must go out as a real batch
/// via `Client::simple_query`.
///
/// Session-scoped statements (`SET`, `USE`, transaction control) are routed the
/// same way for the same underlying reason: `sp_executesql` runs them in a
/// module whose scope ends with the call, so the setting or transaction would be
/// discarded (and transaction control fails with error 266).
fn needs_own_batch(sql: &str) -> bool {
    let mut words = leading_keywords(sql).into_iter();
    let Some(first) = words.next() else {
        return false;
    };
    match first.as_str() {
        // Session-scoped statements do not survive the module boundary that
        // `sp_executesql` (tiberius `Client::query`) creates: the setting, the
        // `USE` context or the open transaction is rolled back when the module
        // exits. Transaction control additionally fails outright with error 266
        // ("Transaction count after EXECUTE indicates a mismatching number of
        // BEGIN and COMMIT statements"). Send these as a real batch.
        //
        // `SET SHOWPLAN_TEXT ON` is the sharpest case: if it does not stick,
        // `explain()` executes the statement it was asked to plan.
        "BEGIN" | "COMMIT" | "ROLLBACK" | "SAVE" | "SET" => return true,
        "CREATE" | "ALTER" => {}
        _ => return false,
    }
    let mut kind = words.next();
    if kind.as_deref() == Some("OR") {
        // `CREATE OR ALTER <kind>`, the SQL Server 2016 SP1+ form.
        let _ = words.next();
        kind = words.next();
    }
    matches!(
        kind.as_deref(),
        Some(
            "SCHEMA" | "VIEW" | "PROCEDURE" | "PROC" | "FUNCTION" | "TRIGGER" | "RULE" | "DEFAULT"
        )
    )
}

/// The first few keywords of `sql`, uppercased, with leading line and block
/// comments skipped.
fn leading_keywords(sql: &str) -> Vec<String> {
    let mut rest = sql;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail);
            continue;
        }
        if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
            continue;
        }
        break;
    }
    rest.split_whitespace()
        .take(4)
        .map(|word| {
            word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .to_ascii_uppercase()
        })
        .collect()
}

/// True when the statement paginates itself with a **top-level** `OFFSET`
/// clause.
///
/// T-SQL rejects `TOP` in the same query as `OFFSET … FETCH` (error 10741:
/// "A TOP can not be used in the same query or sub-query as a OFFSET"), so the
/// editor row cap must not be injected into a statement that already pages:
/// `SELECT … ORDER BY (SELECT NULL) OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY` —
/// exactly what the Visual Query Builder emits for SQL Server — failed on
/// execute until this check existed.
///
/// Only depth-0 occurrences count: `TOP` in an outer query next to an `OFFSET`
/// inside a sub-query is legal, so a nested one must not disable the cap.
/// String literals, quoted/bracketed identifiers and comments are skipped, so
/// `SELECT 'OFFSET 5 ROWS' AS [offset] FROM t` still gets its cap.
fn has_top_level_offset(sql: &str) -> bool {
    let chars: Vec<char> = sql.chars().collect();
    let mut depth = 0i32;
    let mut words: Vec<(i32, String)> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' | '"' => {
                let quote = c;
                i += 1;
                while i < chars.len() {
                    if chars[i] == quote {
                        // A doubled quote is an escaped quote, not the end.
                        if chars.get(i + 1) == Some(&quote) {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '[' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == ']' {
                        if chars.get(i + 1) == Some(&']') {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
            }
            '(' => {
                depth += 1;
                i += 1;
            }
            ')' => {
                depth -= 1;
                i += 1;
            }
            c if c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '#' | '$') => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric()
                        || matches!(chars[i], '_' | '@' | '#' | '$'))
                {
                    i += 1;
                }
                words.push((
                    depth,
                    chars[start..i]
                        .iter()
                        .collect::<String>()
                        .to_ascii_uppercase(),
                ));
            }
            _ => i += 1,
        }
    }

    // `OFFSET <count> [ROW|ROWS]`: the count is a literal or a variable, never a
    // bare identifier — that is what keeps a column named `offset` from
    // disabling the cap.
    words.windows(2).any(|pair| {
        pair[0].0 == 0
            && pair[0].1 == "OFFSET"
            && (pair[1].1.starts_with('@')
                || pair[1].1.chars().next().is_some_and(|c| c.is_ascii_digit()))
    })
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
    // A statement that pages itself is left alone: `TOP` cannot join it (10741)
    // and its own `FETCH NEXT` already bounds the result.
    if has_top_level_offset(trimmed) {
        return (stmt.to_string(), None);
    }
    let after_select = trimmed["SELECT".len()..].trim_start();
    let after_upper = after_select.to_ascii_uppercase();
    let (prefix, body) = if after_upper.starts_with("DISTINCT") {
        (
            "SELECT DISTINCT",
            after_select["DISTINCT".len()..].trim_start(),
        )
    } else {
        ("SELECT", after_select)
    };
    // `TOP` is this dialect's own row limit; the switch only caps SELECTs
    // *without* one, so a hand-written `TOP` is respected verbatim.
    if body.to_ascii_uppercase().starts_with("TOP") {
        return (stmt.to_string(), None);
    }
    (format!("{prefix} TOP {} {body}", lim + 1), Some(lim))
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

    fn has_multi_database(&self) -> bool {
        true
    }

    /// SQL Server resolves unqualified names in the user's default schema,
    /// which is `dbo` unless the login was created with another one.
    fn default_schema(&self) -> Option<&'static str> {
        Some("dbo")
    }

    /// T-SQL has no `LIMIT`: paging uses `OFFSET … ROWS FETCH NEXT … ROWS ONLY`,
    /// which is only legal on a statement that already carries `ORDER BY`. For
    /// an unordered read (`SELECT *` with no usable column) the driver supplies
    /// `(SELECT NULL)` so the caller never has to invent a dialect expression.
    fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax {
        PaginationSyntax {
            clause: format!("OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"),
            requires_order_by: true,
            order_by_fallback: Some("(SELECT NULL)"),
        }
    }

    /// F7: qualify unqualified table references with the T-SQL three-part
    /// name (`[db].[schema].t`; `[schema].t` when only a schema is given).
    /// A database-only target is never inlined — a two-part `[db].t` would
    /// mean *schema* db in T-SQL — so the caller must supply the schema. Temp
    /// tables are skipped. Parse failures pass SQL through unchanged; see
    /// `sql_target::qualify_sql`.
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
        let mut transactions = self.transactions.lock().await;
        if let Some(transaction) = transactions.remove(&handle.id) {
            if let Some(client) = self.clients.write().await.get_mut(&handle.pool_id) {
                // Disconnect must not return a still-open SQL Server
                // transaction to the live handle map. The client is removed
                // below even if rollback fails.
                let rollback = transaction.restore_isolation.map_or_else(
                    || "ROLLBACK TRANSACTION".to_string(),
                    |level| {
                        format!("ROLLBACK TRANSACTION; SET TRANSACTION ISOLATION LEVEL {level}")
                    },
                );
                let _ = Self::execute_batch(client, &rollback).await;
            }
        }
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
        let parameters = [
            Value::String(schema.to_string()),
            Value::String(table.to_string()),
            Value::String(database.trim().to_string()),
        ];
        let column_rows =
            Self::run_with_params(client, &crate::metadata::columns_sql(database), &parameters)
                .await?;
        let (columns, primary_keys) = crate::metadata::parse_columns(column_rows)?;
        // A relation always has at least one column, so "no columns" means the
        // table is absent (or not visible) rather than a column-less table.
        // Reporting `Ok` here would let callers cache a blank structure; this is
        // the same defect class PostgreSQL fixed in BUG-003.
        if columns.is_empty() {
            return Err(DriverError::QueryFailed(format!(
                "Table '{schema}.{table}' does not exist in database '{database}'"
            )));
        }
        let index_rows =
            Self::run_with_params(client, &crate::metadata::indexes_sql(database), &parameters)
                .await?;
        let indexes = crate::metadata::parse_indexes(index_rows)?;
        let foreign_key_rows = Self::run_with_params(
            client,
            &crate::metadata::foreign_keys_sql(database),
            &parameters,
        )
        .await?;
        let foreign_keys = crate::metadata::parse_foreign_keys(foreign_key_rows)?;
        let check_rows =
            Self::run_with_params(client, &crate::metadata::checks_sql(database), &parameters)
                .await?;
        let check_constraints = crate::metadata::parse_checks(check_rows)?;
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes,
            foreign_keys,
            check_constraints,
            table_options: TableOptions::default(),
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

        let mut all_columns: HashMap<String, (Vec<ColumnSchema>, Vec<(i64, String)>)> =
            HashMap::new();
        let mut owners: HashMap<String, String> = HashMap::new();

        for row in &result.rows {
            // SQL: schema_name(0), table_name(1), column_name(2), data_type(3),
            //      is_nullable(4), is_identity(5), default_value(6), comment(7),
            //      is_pk(8), pk_ordinal(9)
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
            // schema seen for a name and say so, rather than merging columns
            // from two different tables. Rows of the *same* table must all be
            // collected — only a name/schema clash skips a row.
            let seen_schema = owners.get(&table_name).cloned();
            match seen_schema {
                Some(existing) if existing != table_schema => {
                    tracing::warn!(
                        table = %table_name,
                        kept = %existing,
                        skipped = %table_schema,
                        "get_all_columns: same-named table in another schema skipped"
                    );
                    continue;
                }
                Some(_) => {}
                None => {
                    owners.insert(table_name.clone(), table_schema.clone());
                }
            }

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
                let key_ordinal = row
                    .get(9)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))
                    .and_then(|v| v.parse::<i64>().ok())
                    .filter(|ordinal| *ordinal > 0)
                    .ok_or_else(|| {
                        DriverError::QueryFailed(
                            "SQL Server returned incomplete composite primary-key metadata".into(),
                        )
                    })?;
                entry.1.push((key_ordinal, col_name));
            }
        }

        Ok(all_columns
            .into_iter()
            .map(|(table, (columns, mut key_columns))| {
                key_columns.sort_by_key(|(ordinal, _)| *ordinal);
                (
                    table,
                    (
                        columns,
                        key_columns.into_iter().map(|(_, name)| name).collect(),
                    ),
                )
            })
            .collect())
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
        let statements = split_statements(sql);
        let mut results = Vec::new();
        for stmt in statements {
            let start = Instant::now();
            // Share the streaming path's cap rewrite: the previous inline check
            // skipped the cap whenever "TOP" appeared *anywhere* in the
            // statement (`SELECT * FROM stopwatch`) and injected `TOP` into
            // statements that page with `OFFSET` (error 10741).
            let (limited, applied) = apply_sqlserver_top(&stmt, limit);
            let mut r = Self::run(client, &limited).await?;
            let truncated = applied.is_some_and(|lim| r.rows.len() as u32 > lim);
            if let Some(lim) = applied {
                if truncated {
                    r.rows.truncate(lim as usize);
                }
            }
            results.push(StatementResult {
                sql: stmt,
                columns: r.columns,
                rows: r.rows,
                rows_affected: r.rows_affected,
                execution_time_ms: r.execution_time_ms,
                truncated,
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
        let statements = split_statements(sql);
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
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        let bound = crate::parameters::bind_values(params);
        let refs = crate::parameters::to_sql_refs(&bound);
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        Self::run_routed_with_params(client, sql, &refs, false).await
    }

    fn parameter_placeholder(
        &self,
        index: usize,
        _data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        if !(1..=2100).contains(&index) {
            return Err(DriverError::InvalidConfig(
                "SQL Server parameter indexes must be between 1 and 2100".into(),
            ));
        }
        Ok(format!("@P{index}"))
    }

    fn max_bound_parameters(&self) -> usize {
        2100
    }

    fn transfer_explicit_identity_insert_requires_session_toggle(&self) -> bool {
        true
    }

    fn transfer_sql_file_begin_transaction(&self) -> &'static str {
        "BEGIN TRANSACTION;"
    }

    fn transfer_sql_file_commit_transaction(&self) -> &'static str {
        "COMMIT TRANSACTION;"
    }

    async fn set_transfer_identity_insert(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
        table: &str,
        enabled: bool,
    ) -> Result<(), DriverError> {
        let relation = Self::transfer_identity_relation(database, schema, table)?;
        let mode = if enabled { "ON" } else { "OFF" };
        let statement = format!("SET IDENTITY_INSERT {relation} {mode}");
        let mut clients = self.clients.write().await;
        let client = clients
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        Self::execute_batch(client, &statement).await
    }

    async fn discard_transfer_connection(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<(), DriverError> {
        self.transactions.lock().await.remove(&handle.id);
        self.clients.write().await.remove(&handle.pool_id);
        Ok(())
    }

    fn render_transfer_sql_file_identity_insert(
        &self,
        insert_sql: &str,
        target_relation: &str,
        mapped_target_columns: &[String],
    ) -> Result<String, DriverError> {
        Self::render_sql_file_identity_insert(insert_sql, target_relation, mapped_target_columns)
    }

    async fn execute_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        use futures_util::TryStreamExt;

        let bound = crate::parameters::bind_values(params);
        let refs = crate::parameters::to_sql_refs(&bound);
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        if needs_own_batch(sql) {
            if !refs.is_empty() {
                return Err(DriverError::Unsupported(
                    "SQL Server batch-only statements cannot accept bound parameters; use a parameterized RPC-compatible statement"
                        .into(),
                ));
            }
            let mut stream = client
                .simple_query(sql)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("SQL Server execute failed: {e}")))?;
            while stream
                .try_next()
                .await
                .map_err(|e| DriverError::QueryFailed(format!("SQL Server execute failed: {e}")))?
                .is_some()
            {}
            return Ok(0);
        }
        client
            .execute(sql, &refs)
            .await
            .map(|result| result.total())
            .map_err(|e| DriverError::QueryFailed(format!("SQL Server execute failed: {e}")))
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        use futures_util::TryStreamExt;
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        if needs_own_batch(sql) {
            // These statements are only legal as the first statement of a
            // batch, which `sp_executesql` cannot provide; T-SQL reports no row
            // count for them, so the result is 0.
            let mut stream = client
                .simple_query(sql)
                .await
                .map_err(|e| DriverError::QueryFailed(format!("SQL Server execute failed: {e}")))?;
            while stream
                .try_next()
                .await
                .map_err(|e| DriverError::QueryFailed(format!("SQL Server execute failed: {e}")))?
                .is_some()
            {}
            return Ok(0);
        }
        client
            .execute(sql, &[])
            .await
            .map(|r| r.total())
            .map_err(|e| DriverError::QueryFailed(format!("SQL Server execute failed: {e}")))
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let mut transactions = self.transactions.lock().await;
        if transactions.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }
        let mut clients = self.clients.write().await;
        let client = clients
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        Self::ensure_no_open_transaction(client).await?;
        if let Err(error) = Self::execute_batch(client, "BEGIN TRANSACTION").await {
            clients.remove(&handle.pool_id);
            return Err(error);
        }
        let id = format!("sqlserver_tx_{}", uuid::Uuid::new_v4());
        transactions.insert(
            handle.id.clone(),
            ActiveTransaction {
                id: id.clone(),
                restore_isolation: None,
            },
        );
        Ok(TransactionHandle {
            id,
            connection_id: handle.id.clone(),
        })
    }

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        let mut transactions = self.transactions.lock().await;
        if transactions.contains_key(&handle.id) {
            return Err(DriverError::TransactionError(
                "A transaction is already open on this connection".into(),
            ));
        }
        let mut clients = self.clients.write().await;
        let client = clients
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        Self::ensure_no_open_transaction(client).await?;
        let restore_isolation = match Self::current_isolation_level(client).await {
            Ok(level) => level,
            Err(error) => return Err(error),
        };
        if let Err(error) = Self::execute_batch(
            client,
            "SET TRANSACTION ISOLATION LEVEL SNAPSHOT; BEGIN TRANSACTION",
        )
        .await
        {
            // SNAPSHOT may be disabled for the active database. The TDS
            // connection is discarded so the session isolation setting cannot
            // leak into a later operation after an uncertain partial batch.
            clients.remove(&handle.pool_id);
            return Err(DriverError::TransactionError(format!(
                "could not begin a SQL Server SNAPSHOT transaction (enable ALLOW_SNAPSHOT_ISOLATION for this database): {error}"
            )));
        }
        let id = format!("sqlserver_snapshot_{}", uuid::Uuid::new_v4());
        transactions.insert(
            handle.id.clone(),
            ActiveTransaction {
                id: id.clone(),
                restore_isolation: Some(restore_isolation),
            },
        );
        Ok(TransactionHandle {
            id,
            connection_id: handle.id.clone(),
        })
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let mut transactions = self.transactions.lock().await;
        let restore_isolation = match transactions.get(&tx.connection_id) {
            Some(active) if active.id == tx.id => active.restore_isolation,
            Some(_) => {
                return Err(DriverError::TransactionError(
                    "Transaction handle does not match the active SQL Server transaction".into(),
                ));
            }
            None => {
                return Err(DriverError::TransactionError(
                    "Transaction not found or already ended".into(),
                ));
            }
        };
        let statement = restore_isolation.map_or_else(
            || "COMMIT TRANSACTION".to_string(),
            |level| format!("COMMIT TRANSACTION; SET TRANSACTION ISOLATION LEVEL {level}"),
        );
        let mut clients = self.clients.write().await;
        let result = match clients.get_mut(&tx.connection_id) {
            Some(client) => Self::execute_batch(client, &statement).await,
            None => Err(DriverError::ConnectionFailed(
                "Connection pool not found".into(),
            )),
        };
        transactions.remove(&tx.connection_id);
        if result.is_err() {
            // After a failed COMMIT the server-side transaction state is
            // uncertain. Drop the session rather than reuse it.
            clients.remove(&tx.connection_id);
        }
        result
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        let mut transactions = self.transactions.lock().await;
        let restore_isolation = match transactions.get(&tx.connection_id) {
            Some(active) if active.id == tx.id => active.restore_isolation,
            Some(_) => {
                return Err(DriverError::TransactionError(
                    "Transaction handle does not match the active SQL Server transaction".into(),
                ));
            }
            None => {
                return Err(DriverError::TransactionError(
                    "Transaction not found or already ended".into(),
                ));
            }
        };
        let statement = restore_isolation.map_or_else(
            || "ROLLBACK TRANSACTION".to_string(),
            |level| format!("ROLLBACK TRANSACTION; SET TRANSACTION ISOLATION LEVEL {level}"),
        );
        let mut clients = self.clients.write().await;
        let result = match clients.get_mut(&tx.connection_id) {
            Some(client) => Self::execute_batch(client, &statement).await,
            None => Err(DriverError::ConnectionFailed(
                "Connection pool not found".into(),
            )),
        };
        transactions.remove(&tx.connection_id);
        if result.is_err() {
            clients.remove(&tx.connection_id);
        }
        result
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
        // The planned statement must be sent as a real batch: under SHOWPLAN the
        // RPC path yields no result rows at all (and would be a data-loss trap
        // if the flag had not stuck).
        let plan = Self::run_batch(client, sql).await;
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
        // `list_objects` / `get_object_ddl` / `list_privileges` are shared
        // driver-api commands; the driver only supplies its dialect.
        if is_schema_object_command(command) {
            return execute_schema_object_command(
                self,
                &self.driver_type(),
                handle,
                command,
                input,
            )
            .await;
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
        assert!(sql.contains("FROM [sales].sys.columns c"));
        assert!(sql.contains("s.name = @P1 AND o.name = @P2"));
        assert!(sql.contains("is_primary_key = 1"));
        assert!(sql.contains("is_pk"));
        assert!(sql.contains("pk.key_ordinal"));
        assert!(sql.contains("c.max_length"));
        // No session switch may be embedded in a read path.
        assert!(!sql.to_uppercase().contains("USE ["));
    }

    #[test]
    fn build_table_schema_sql_stays_local_when_database_is_blank() {
        let sql = SqlServerDriver::build_table_schema_sql("", "dbo", "users");
        assert!(sql.contains("FROM sys.columns c"));
        assert!(!sql.contains("[]."));
    }

    #[test]
    fn build_table_schema_sql_escapes_quotes() {
        let sql = SqlServerDriver::build_table_schema_sql("db]", "d'bo", "us'ers");
        assert!(sql.contains("FROM [db]]].sys.columns c"));
        assert!(sql.contains("@P1"));
        assert!(sql.contains("@P2"));
        assert!(!sql.contains("d'bo"));
        assert!(!sql.contains("us'ers"));
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
        // A hand-written `TOP` is the dialect's own row limit: respected, and
        // no cap is reported as applied.
        assert_eq!(
            apply_sqlserver_top("SELECT TOP 5 * FROM t", Some(10)),
            ("SELECT TOP 5 * FROM t".into(), None)
        );
        assert_eq!(
            apply_sqlserver_top("SELECT DISTINCT name FROM t", Some(3)),
            ("SELECT DISTINCT TOP 4 name FROM t".into(), Some(3))
        );
        assert_eq!(
            apply_sqlserver_top("SELECT DISTINCT TOP 2 name FROM t", Some(3)),
            ("SELECT DISTINCT TOP 2 name FROM t".into(), None)
        );
        assert_eq!(
            apply_sqlserver_top("INSERT INTO t VALUES (1)", Some(10)),
            ("INSERT INTO t VALUES (1)".into(), None)
        );
        assert_eq!(
            apply_sqlserver_top("select id from t", Some(1)),
            ("SELECT TOP 2 id from t".into(), Some(1))
        );

        // `TOP` cannot share a query with `OFFSET … FETCH` (error 10741), so a
        // statement that pages itself is never rewritten — this is the statement
        // the Visual Query Builder emits for SQL Server.
        let paged = "SELECT [u].[id] FROM [users] AS [u] \
                     ORDER BY (SELECT NULL) OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY";
        assert_eq!(apply_sqlserver_top(paged, Some(100)), (paged.into(), None));
        // The same is true for an offset-only page and for either keyword case.
        assert_eq!(
            apply_sqlserver_top("select id from t order by id offset 2 rows", Some(10)),
            ("select id from t order by id offset 2 rows".into(), None)
        );
        assert_eq!(
            apply_sqlserver_top("SELECT id FROM t OFFSET @skip ROWS", Some(10)),
            ("SELECT id FROM t OFFSET @skip ROWS".into(), None)
        );

        // A nested `OFFSET` is legal next to an outer `TOP`, so the cap stays.
        assert_eq!(
            apply_sqlserver_top(
                "SELECT * FROM (SELECT id FROM t ORDER BY id OFFSET 2 ROWS) AS inner_q",
                Some(10)
            ),
            (
                "SELECT TOP 11 * FROM (SELECT id FROM t ORDER BY id OFFSET 2 ROWS) AS inner_q"
                    .into(),
                Some(10)
            )
        );
        // …and a column named `offset` is not an OFFSET clause.
        assert_eq!(
            apply_sqlserver_top("SELECT offset FROM t", Some(10)),
            ("SELECT TOP 11 offset FROM t".into(), Some(10))
        );
    }

    #[test]
    fn has_top_level_offset_ignores_literals_comments_and_identifiers() {
        assert!(has_top_level_offset(
            "SELECT id FROM t ORDER BY id OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY"
        ));
        assert!(has_top_level_offset("SELECT id FROM t OFFSET 5 ROWS"));

        // Inside a string literal, a bracketed/quoted identifier or a comment it
        // is data, not a clause.
        assert!(!has_top_level_offset(
            "SELECT 'x OFFSET 5 ROWS' AS [offset 3] FROM t"
        ));
        assert!(!has_top_level_offset("SELECT id FROM t -- OFFSET 5 ROWS\n"));
        assert!(!has_top_level_offset(
            "SELECT id /* OFFSET 5 ROWS */ FROM t"
        ));
        assert!(!has_top_level_offset("\"OFFSET 5\" AS c FROM t"));
        // A doubled bracket inside an identifier must not end it early.
        assert!(!has_top_level_offset(
            "SELECT [we]]ird OFFSET 5 ROWS] FROM t"
        ));
        // Depth matters: a sub-query's OFFSET is not the outer query's.
        assert!(!has_top_level_offset(
            "SELECT * FROM (SELECT id FROM t OFFSET 5 ROWS) AS q"
        ));
        assert!(has_top_level_offset(
            "SELECT * FROM (SELECT id FROM t) AS q OFFSET 1 ROWS"
        ));
    }

    /// Regression: the host used to append `LIMIT n OFFSET m`, which T-SQL
    /// rejects with "Incorrect syntax near 'LIMIT'".
    #[test]
    fn pagination_syntax_is_offset_fetch_and_never_limit() {
        let driver = SqlServerDriver::new();
        assert!(driver.supports_offset());

        let first_page = driver.pagination_syntax(25, 0);
        assert_eq!(
            first_page.clause,
            "OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY".to_string()
        );
        assert!(first_page.requires_order_by);
        assert_eq!(first_page.order_by_fallback, Some("(SELECT NULL)"));
        assert!(!first_page.clause.contains("LIMIT"));

        let deep_page = driver.pagination_syntax(50, 150);
        assert_eq!(
            deep_page.clause,
            "OFFSET 150 ROWS FETCH NEXT 50 ROWS ONLY".to_string()
        );
        assert!(!deep_page.clause.contains("LIMIT"));
    }

    #[test]
    fn batch_only_ddl_is_detected() {
        // Statements SQL Server rejects through `sp_executesql`.
        for stmt in [
            "CREATE SCHEMA [reporting]",
            "create schema reporting",
            "CREATE VIEW [dbo].[v] AS SELECT 1 AS c",
            "ALTER VIEW [dbo].[v] AS SELECT 1 AS c",
            "CREATE PROCEDURE [dbo].[p] AS SELECT 1",
            "CREATE PROC [dbo].[p] AS SELECT 1",
            "CREATE FUNCTION [dbo].[f]() RETURNS INT AS BEGIN RETURN 1 END",
            "CREATE TRIGGER [dbo].[tr] ON [dbo].[t] AFTER INSERT AS SELECT 1",
            "CREATE OR ALTER PROCEDURE [dbo].[p] AS SELECT 1",
            "CREATE OR ALTER VIEW [dbo].[v] AS SELECT 1 AS c",
            "  -- installs the reporting schema\nCREATE SCHEMA [reporting]",
            "/* bootstrap */ CREATE TRIGGER [dbo].[tr] ON [dbo].[t] AFTER INSERT AS SELECT 1",
            // Session-scoped statements must also bypass sp_executesql.
            "SET NOCOUNT ON",
            "SET IDENTITY_INSERT [dbo].[t] ON",
            "BEGIN TRAN; SELECT 1; COMMIT",
            "BEGIN TRANSACTION",
            "COMMIT",
            "ROLLBACK",
            "SAVE TRAN savepoint_one",
        ] {
            assert!(needs_own_batch(stmt), "expected own batch: {stmt}");
        }
    }

    #[test]
    fn statement_splitting_respects_literals_and_comments() {
        assert_eq!(
            split_statements("SELECT ';' AS [a]"),
            vec!["SELECT ';' AS [a]"]
        );
        assert_eq!(
            split_statements("SELECT 1; SELECT ';' AS [b]; -- trailing\n"),
            vec!["SELECT 1", "SELECT ';' AS [b]"]
        );
        assert_eq!(
            split_statements("SELECT '[;]' AS [c] /* ; */; SELECT 2"),
            vec!["SELECT '[;]' AS [c] /* ; */", "SELECT 2"]
        );
        assert!(split_statements("   ").is_empty());
    }

    #[test]
    fn preparable_statements_stay_on_the_rpc_path() {
        for stmt in [
            "CREATE TABLE [dbo].[t] ([id] INT NOT NULL)",
            "ALTER TABLE [dbo].[t] ADD [c] INT NULL",
            "DROP TABLE [dbo].[t]",
            "DROP SCHEMA [reporting]",
            "CREATE SEQUENCE [dbo].[s] AS INT START WITH 1",
            "CREATE TYPE [dbo].[ty] FROM INT",
            "INSERT INTO [dbo].[t] ([id]) VALUES (1)",
            "UPDATE [dbo].[t] SET [id] = 2",
            "DELETE FROM [dbo].[t]",
            "MERGE [dbo].[t] AS t USING [dbo].[s] AS s ON t.id = s.id WHEN MATCHED THEN DELETE;",
            "SELECT * FROM [dbo].[t]",
            "",
        ] {
            assert!(!needs_own_batch(stmt), "expected RPC path: {stmt}");
        }
    }

    /// Render a temporal cell and unwrap it to its text payload.
    fn temporal_text(data: &ColumnData<'static>) -> Option<String> {
        match SqlServerDriver::value_from_column(data) {
            Some(Value::String(text)) => Some(text),
            None => None,
            other => panic!("expected a text value, got {other:?}"),
        }
    }

    #[test]
    fn temporal_columns_render_as_text_not_debug_output() {
        use tiberius::time::{Date, DateTime2, DateTimeOffset, Time};

        // 2026-01-02 is 739617 days after 0001-01-01; 03:04:05 is 11 045 s.
        let date = temporal_text(&ColumnData::Date(Some(Date::new(739_617))));
        assert_eq!(date.as_deref(), Some("2026-01-02"));

        let time = temporal_text(&ColumnData::Time(Some(Time::new(110_450_000_000, 7))));
        assert_eq!(time.as_deref(), Some("03:04:05"));

        let datetime2 = temporal_text(&ColumnData::DateTime2(Some(DateTime2::new(
            Date::new(739_617),
            Time::new(110_450_000_000, 7),
        ))));
        assert_eq!(datetime2.as_deref(), Some("2026-01-02 03:04:05"));

        // TDS carries the UTC instant plus the original offset: the live wire
        // value for `CAST('2026-01-02T03:04:05+08:00' AS DATETIMEOFFSET)` is the
        // datetime2 `2026-01-01 19:04:05` (day 739616, 68 645 s) with offset 480.
        let offset = temporal_text(&ColumnData::DateTimeOffset(Some(DateTimeOffset::new(
            DateTime2::new(Date::new(739_616), Time::new(686_450_000_000, 7)),
            480,
        ))));
        assert_eq!(offset.as_deref(), Some("2026-01-02T03:04:05+08:00"));

        for text in [date, time, datetime2, offset].into_iter().flatten() {
            assert!(
                !text.contains("Date(") && !text.contains("Time {") && !text.contains("increments"),
                "temporal value must not leak tiberius' Debug output: {text}"
            );
        }
    }

    #[test]
    fn null_temporal_columns_stay_null() {
        assert_eq!(temporal_text(&ColumnData::Date(None)), None);
        assert_eq!(temporal_text(&ColumnData::DateTimeOffset(None)), None);
    }

    #[test]
    fn reports_sql_server_bound_parameter_limit() {
        assert_eq!(SqlServerDriver::new().max_bound_parameters(), 2100);
    }

    #[test]
    fn transfer_identity_insert_relation_quotes_every_sql_server_identifier() {
        assert_eq!(
            SqlServerDriver::transfer_identity_relation(
                "data]zen",
                Some("odd.schema"),
                "order]details"
            )
            .unwrap(),
            "[data]]zen].[odd.schema].[order]]details]"
        );
        assert_eq!(
            SqlServerDriver::transfer_identity_relation("", None, "items").unwrap(),
            "[dbo].[items]"
        );
        assert!(SqlServerDriver::transfer_identity_relation("db", Some(""), "").is_err());
    }

    #[test]
    fn sql_file_identity_wrapper_checks_catalog_and_cleans_up_on_error() {
        let script = SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO [odd.schema].[order]]details] ([id]) VALUES (1)",
            "[data]]zen].[odd.schema].[order]]details]",
            &["id".into()],
        )
        .unwrap();
        assert!(script.contains("OBJECT_ID(N'[data]]zen].[odd.schema].[order]]details]', N'U')"));
        assert!(script.contains(
            "FROM [data]]zen].sys.identity_columns WHERE object_id = OBJECT_ID(N'[data]]zen].[odd.schema].[order]]details]', N'U') AND name IN (N'id')"
        ));
        assert!(
            script.contains("SET IDENTITY_INSERT [data]]zen].[odd.schema].[order]]details] ON;")
        );
        assert!(
            script.contains("SET IDENTITY_INSERT [data]]zen].[odd.schema].[order]]details] OFF;")
        );
        assert!(script.contains("BEGIN CATCH\n        BEGIN TRY\n            SET IDENTITY_INSERT"));
        assert!(script.contains("        THROW;"));
        assert!(script.contains("ELSE\nBEGIN\n    INSERT INTO [odd.schema]"));
        let local_script = SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO [dbo].[items] ([id]) VALUES (1)",
            "[dbo].[items]",
            &["id".into()],
        )
        .unwrap();
        assert!(local_script.contains("FROM sys.identity_columns WHERE object_id"));
        let apostrophe_script = SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO [O'Brien].[items] ([id]) VALUES (1)",
            "[db].[O'Brien].[items]",
            &["user'id".into()],
        )
        .unwrap();
        assert!(apostrophe_script.contains("OBJECT_ID(N'[db].[O''Brien].[items]'"));
        assert!(apostrophe_script.contains("name IN (N'user''id')"));
    }

    #[test]
    fn sql_file_identity_wrapper_only_checks_inserted_mapped_columns() {
        // The first case represents a regular source column mapped to the
        // target identity column. The target metadata predicate discovers
        // that mapped identity at execution time, independently of source
        // auto-increment metadata.
        let mapped_identity = SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO [dbo].[items] ([id], [label]) VALUES (7, N'x')",
            "[dbo].[items]",
            &["id".into(), "label".into()],
        )
        .unwrap();
        assert!(mapped_identity.contains("AND name IN (N'id', N'label')"));
        assert!(mapped_identity.contains("SET IDENTITY_INSERT [dbo].[items] ON;"));

        // Here the source identity maps to a regular target column while a
        // different target identity column is omitted from the INSERT. The
        // script's predicate can only match the actual mapped target column,
        // so that unrelated identity cannot enable IDENTITY_INSERT.
        let unrelated_identity = SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO [dbo].[items] ([external_id]) VALUES (7)",
            "[dbo].[items]",
            &["external_id".into()],
        )
        .unwrap();
        assert!(unrelated_identity.contains("AND name IN (N'external_id')"));
        assert!(!unrelated_identity.contains("name IN (N'id'"));
        assert!(unrelated_identity.contains("ELSE\nBEGIN\n    INSERT INTO [dbo].[items]"));
    }

    #[test]
    fn sql_file_identity_wrapper_skips_empty_columns_and_rejects_invalid_names() {
        let insert_sql = "INSERT INTO [dbo].[items] DEFAULT VALUES";
        assert_eq!(
            SqlServerDriver::render_sql_file_identity_insert(insert_sql, "[dbo].[items]", &[],)
                .unwrap(),
            insert_sql
        );
        assert!(SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO [dbo].[items] ([id]) VALUES (1)",
            "[dbo].[items]",
            &["bad\0name".into()],
        )
        .is_err());
    }

    #[test]
    fn sql_file_identity_wrapper_rejects_unquoted_relation_fragments() {
        assert!(SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO t (id) VALUES (1)",
            "dbo.t; DROP TABLE users",
            &["id".into()],
        )
        .is_err());
        assert!(SqlServerDriver::render_sql_file_identity_insert(
            "INSERT INTO [dbo].[t] ([id]) VALUES (1)",
            "[dbo].",
            &["id".into()],
        )
        .is_err());
    }

    #[test]
    fn explicit_transfer_identity_insert_uses_a_session_toggle() {
        let driver = SqlServerDriver::new();
        assert!(driver.transfer_explicit_identity_insert_requires_session_toggle());
        assert_eq!(
            driver.transfer_sql_file_begin_transaction(),
            "BEGIN TRANSACTION;"
        );
        assert_eq!(
            driver.transfer_sql_file_commit_transaction(),
            "COMMIT TRANSACTION;"
        );
    }

    #[tokio::test]
    async fn reuse_driver_forwards_transfer_identity_and_sql_file_hooks() {
        let inner: Arc<dyn DatabaseDriver> = Arc::new(SqlServerDriver::new());
        let driver = ReuseDriver::new(inner, "sqlserver-alias");
        assert!(driver.transfer_explicit_identity_insert_requires_session_toggle());
        let script = driver
            .render_transfer_sql_file_identity_insert(
                "INSERT INTO [dbo].[items] ([id]) VALUES (1)",
                "[dbo].[items]",
                &["id".into()],
            )
            .unwrap();
        assert!(script.contains("SET IDENTITY_INSERT [dbo].[items] ON;"));
        assert_eq!(
            driver.transfer_sql_file_begin_transaction(),
            "BEGIN TRANSACTION;"
        );
        assert_eq!(
            driver.transfer_sql_file_commit_transaction(),
            "COMMIT TRANSACTION;"
        );

        let handle = ConnectionHandle {
            id: "missing".into(),
            pool_id: "missing".into(),
        };
        let error = driver
            .set_transfer_identity_insert(&handle, "db", Some("dbo"), "items", true)
            .await
            .unwrap_err();
        assert!(matches!(error, DriverError::ConnectionFailed(_)));
        driver.discard_transfer_connection(&handle).await.unwrap();
    }
}
