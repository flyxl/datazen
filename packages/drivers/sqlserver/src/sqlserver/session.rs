//! Live-session mechanics: dialing and TLS setup, choosing between the RPC and
//! the batch path, converting tiberius results, streaming, and the transaction
//! lifecycle.
//!
//! Split out from `super` because everything here needs a live TDS connection
//! plus the driver's shared `clients` / `transactions` state, whereas the driver
//! type and its SQL text do not. `super` keeps every trait signature and
//! delegates to these inherent methods, so the `DatabaseDriver` impl stays one
//! contiguous block as Rust requires.

use super::*;

impl SqlServerDriver {
    /// Map DataZen SSL mode → (tiberius encryption, trust server certificate).
    ///
    /// - `Disable`: plaintext TDS (no TLS)
    /// - `Prefer` / `Require`: encrypt, trust server cert (common for self-signed)
    /// - `VerifyCa` / `VerifyFull`: encrypt and verify the certificate chain
    pub(crate) fn ssl_settings(mode: &SslMode) -> (EncryptionLevel, bool) {
        match mode {
            SslMode::Disable => (EncryptionLevel::NotSupported, false),
            SslMode::Prefer => (EncryptionLevel::On, true),
            SslMode::Require => (EncryptionLevel::Required, true),
            SslMode::VerifyCa | SslMode::VerifyFull => (EncryptionLevel::Required, false),
        }
    }

    pub(crate) fn build_config(config: &ConnectionConfig) -> Result<Config, DriverError> {
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

    /// The exact socket [`Self::connect_client`] opens. Split out so the
    /// host-facing `default_port()` declaration has something assertable to be
    /// compared against — a constant nothing dials would drift silently.
    pub(crate) fn dial_addr(config: &ConnectionConfig) -> Result<String, DriverError> {
        let host = config
            .host
            .clone()
            .ok_or_else(|| DriverError::InvalidConfig("host is required".into()))?;
        Ok(format!("{host}:{}", config.port.unwrap_or(DEFAULT_PORT)))
    }

    pub(crate) async fn connect_client(
        config: &ConnectionConfig,
    ) -> Result<SqlClient, DriverError> {
        let cfg = Self::build_config(config)?;
        let addr = Self::dial_addr(config)?;
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

    pub(crate) fn value_from_column(data: &ColumnData<'static>) -> Option<Value> {
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
    pub(crate) fn temporal_value(data: &ColumnData<'static>) -> Option<Value> {
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

    pub(crate) async fn run(client: &mut SqlClient, sql: &str) -> Result<QueryResult, DriverError> {
        Self::run_routed(client, sql, false).await
    }

    pub(crate) async fn run_with_params(
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
    pub(crate) async fn run_batch(
        client: &mut SqlClient,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        Self::run_routed(client, sql, true).await
    }

    pub(crate) async fn execute_batch(
        client: &mut SqlClient,
        sql: &str,
    ) -> Result<(), DriverError> {
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

    pub(crate) async fn current_isolation_level(
        client: &mut SqlClient,
    ) -> Result<&'static str, DriverError> {
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

    pub(crate) async fn ensure_no_open_transaction(
        client: &mut SqlClient,
    ) -> Result<(), DriverError> {
        let result = Self::run(client, "SELECT @@TRANCOUNT AS [transaction_count]").await?;
        let count = result
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(Option::as_ref)
            .and_then(Self::parse_transaction_count)
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

    pub(crate) fn parse_transaction_count(value: &Value) -> Option<i64> {
        match value {
            Value::Integer(count) => Some(*count),
            Value::String(count) => count.parse().ok(),
            _ => None,
        }
    }

    pub(crate) async fn run_routed(
        client: &mut SqlClient,
        sql: &str,
        force_batch: bool,
    ) -> Result<QueryResult, DriverError> {
        Self::run_routed_with_params(client, sql, &[], force_batch).await
    }

    pub(crate) async fn run_routed_with_params(
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

    pub(crate) fn columns_from_tiberius(cols: &[tiberius::Column]) -> Vec<ColumnInfo> {
        cols.iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: format!("{:?}", c.column_type()),
                nullable: true,
            })
            .collect()
    }

    pub(crate) async fn stream_one(
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

    pub(crate) async fn test_connection(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ServerInfo, DriverError> {
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

    pub(crate) async fn connect(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        let client = Self::connect_client(config).await?;
        let pool_id = format!("sqlserver_{}", uuid::Uuid::new_v4());
        self.clients.write().await.insert(pool_id.clone(), client);
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    pub(crate) async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
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

    pub(crate) async fn discard_connection(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<(), DriverError> {
        self.transactions.lock().await.remove(&handle.id);
        self.clients.write().await.remove(&handle.pool_id);
        Ok(())
    }

    pub(crate) async fn set_identity_insert(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
        table: &str,
        enabled: bool,
    ) -> Result<(), DriverError> {
        let statement = Self::identity_insert_statement(database, schema, table, enabled)?;
        let mut clients = self.clients.write().await;
        let client = clients
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        // IDENTITY_INSERT is scoped to the physical SQL Server session, so
        // send it as a standalone batch on this mapped client.
        Self::execute_batch(client, &statement).await
    }

    pub(crate) async fn execute_with_params(
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

    pub(crate) async fn execute(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<u64, DriverError> {
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

    pub(crate) async fn begin_transaction(
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

    pub(crate) async fn begin_read_snapshot(
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

    pub(crate) async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
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

    pub(crate) async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
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
}
