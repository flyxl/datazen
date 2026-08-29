//! Core driver traits.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::query_stream::{emit_multi_query_as_stream, QueryStreamCallback};
use crate::types::*;
use crate::{
    execute_command_definition, query_command_definition, schema_catalog_command_definitions,
    try_execute_schema_catalog_command, CommandResult, DriverCommandDefinition,
};

#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::Sql
    }

    fn quote_char(&self) -> char {
        '"'
    }

    fn quote_ident(&self, name: &str) -> String {
        let q = self.quote_char();
        if q == '`' {
            format!("`{}`", name.replace('`', "``"))
        } else {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
    }

    fn skip_count_query(&self) -> bool {
        false
    }

    /// Whether the driver's SQL dialect supports `OFFSET` in pagination.
    /// Drivers that don't (e.g. Presto/Hive via Superset) should return `false`.
    fn supports_offset(&self) -> bool {
        true
    }

    /// Whether the driver supports EXPLAIN query plan analysis.
    fn supports_explain(&self) -> bool {
        true
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        match value {
            None | Some(Value::Null) => "NULL".to_string(),
            Some(Value::Bool(b)) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Some(Value::Integer(i)) => i.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!("'{}'", String::from_utf8_lossy(b).replace('\'', "''"))
            }
            Some(Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        }
    }

    fn build_update_sql(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> String {
        let set_clauses: Vec<String> = set_columns
            .iter()
            .map(|(col, val)| {
                format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(val)
                )
            })
            .collect();
        let where_clauses: Vec<String> = pk_columns
            .iter()
            .map(|(col, val)| match val {
                None | Some(Value::Null) => format!("{} IS NULL", self.quote_ident(col)),
                Some(v) => format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(&Some(v.clone()))
                ),
            })
            .collect();
        format!(
            "UPDATE {} SET {} WHERE {}",
            self.quote_ident(table),
            set_clauses.join(", "),
            where_clauses.join(" AND ")
        )
    }

    /// Build `DELETE FROM … WHERE pk…` for row-level deletes (mirrors `build_update_sql`).
    fn build_delete_sql(&self, table: &str, pk_columns: &[(&str, Option<Value>)]) -> String {
        let where_clauses: Vec<String> = pk_columns
            .iter()
            .map(|(col, val)| match val {
                None | Some(Value::Null) => format!("{} IS NULL", self.quote_ident(col)),
                Some(v) => format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(&Some(v.clone()))
                ),
            })
            .collect();
        format!(
            "DELETE FROM {} WHERE {}",
            self.quote_ident(table),
            where_clauses.join(" AND ")
        )
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError>;

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError>;

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError>;

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError>;

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError>;

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError>;

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let schema = self.get_table_schema(handle, table).await?;
        Ok((schema.columns, schema.primary_keys))
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str)
        -> Result<QueryResult, DriverError>;

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError>;

    /// Stream query results as row batches.
    ///
    /// `limit` is the SQL result cap from the host "limit SELECT results"
    /// setting (`None` = do not rewrite/cap SQL). It is **not** the IPC batch
    /// size. Drivers that can stream from the wire should override this;
    /// the default materializes [`Self::query_multi`] then emits chunks.
    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let result = self.query_multi(handle, sql, limit).await?;
        emit_multi_query_as_stream(result, &on_event);
        Ok(())
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError>;

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError>;

    /// Return commands supported by this driver.
    ///
    /// Existing SQL drivers get the standard `query` and `execute` commands.
    /// A driver with additional capabilities can override this method and append
    /// its own command definitions.
    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let mut defs = vec![query_command_definition(), execute_command_definition()];
        defs.extend(schema_catalog_command_definitions());
        defs
    }

    /// Execute a driver command.
    ///
    /// The default implementation maps the existing SQL APIs to commands so
    /// existing drivers remain source-compatible. Driver plugins can override
    /// this method to implement driver-specific commands without adding another
    /// application-level dispatch path.
    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Ok(result) => return Ok(result),
            Err(DriverError::Unsupported(_)) => {}
            Err(err) => return Err(err),
        }
        if let Some(result) =
            try_execute_schema_catalog_command(self, handle, command, input).await?
        {
            return Ok(result);
        }
        Err(DriverError::Unsupported(format!(
            "unsupported driver command: {command}"
        )))
    }

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn explain(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        Err(DriverError::QueryFailed(
            "Not supported for this driver type".into(),
        ))
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError>;

    /// Fetch server version info using an existing connection handle.
    /// Unlike `test_connection` which creates a temporary pool, this reuses the live connection.
    async fn get_server_info(&self, _handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: self.driver_type(),
        })
    }

    /// Switch the active database for subsequent queries.
    /// Drivers that maintain per-session state (e.g. Kiwi) should override this.
    async fn use_database(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    /// F7: dialect-aware SQL target qualification.
    ///
    /// Given optional targeting information (`database`, and for PG-family
    /// engines `schema`), rewrite unqualified table references in `sql` into
    /// dialect-qualified names (AST-level, see [`crate::sql_target`]). This
    /// lets a command land on the caller-selected target with no session
    /// switch and no `USE`.
    ///
    /// Return value:
    /// - `Some(qualified_sql)` — the driver can rewrite; parse failures pass
    ///   the original text back through (the rewrite is best-effort).
    /// - `None` — the driver has no rewrite capability (default). The host
    ///   executes the SQL as-is, logs, and the existing host-side
    ///   `ensure_session_database` pin remains the fallback for the database
    ///   dimension.
    ///
    /// Implementations must be pure/stateless and idempotent (re-qualifying
    /// already-qualified SQL is a no-op).
    fn qualify_sql_target(
        &self,
        _sql: &str,
        _database: Option<&str>,
        _schema: Option<&str>,
    ) -> Option<String> {
        None
    }

    /// Return driver-specific prompt overrides for AI features.
    ///
    /// Templates can use `{{variable}}` placeholders. Available variables per
    /// scenario are documented in the main application's `PromptResolver`.
    /// The default implementation returns an empty map (use global defaults).
    fn prompt_overrides(&self) -> HashMap<PromptScenario, PromptTemplate> {
        HashMap::new()
    }

    /// Emit `CREATE TABLE` DDL for a single table.
    ///
    /// Default builds DDL from [`Self::get_table_schema`].
    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<String, DriverError> {
        crate::sql_dump::dump_table_ddl_from_schema::<Self>(self, handle, table).await
    }

    /// Emit `CREATE VIEW` (or equivalent) for a view / materialized view.
    ///
    /// Default is [`DriverError::NotSupported`]; backup then writes a comment
    /// and still skips `INSERT INTO` for view-like objects.
    async fn dump_view_ddl(
        &self,
        _handle: &ConnectionHandle,
        view: &str,
    ) -> Result<String, DriverError> {
        Err(DriverError::NotSupported(format!(
            "View DDL dump is not supported for {view}"
        )))
    }

    /// Dump stored procedures and functions for the current database.
    ///
    /// Default is an empty string (driver has no routines, or they are skipped).
    async fn dump_routines(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(String::new())
    }

    /// Dump triggers for the current database.
    ///
    /// Default is an empty string.
    async fn dump_triggers(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(String::new())
    }

    /// Dump an entire database to SQL text.
    ///
    /// Default refuses `create_database` with [`DriverError::NotSupported`] and
    /// otherwise delegates to [`crate::sql_dump::dump_sql_database`].
    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.dump_database_with_progress(handle, database, opts, &mut |_| {})
            .await
    }

    /// Same as [`Self::dump_database`] with per-object progress.
    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        if opts.create_database {
            return Err(DriverError::NotSupported(
                "Backup option 'create' (CREATE DATABASE) is not supported for this driver".into(),
            ));
        }
        crate::sql_dump::dump_sql_database_with_progress::<Self, _>(
            self,
            handle,
            database,
            opts,
            on_progress,
        )
        .await
    }

    /// When true (SQL drivers), the host streams the dump file into
    /// [`crate::sql_dump::RestoreSession`]. Override to `false` to take over
    /// the whole restore via [`Self::restore_sql_with_progress`].
    fn uses_sql_restore_pipeline(&self) -> bool {
        matches!(self.driver_category(), DriverCategory::Sql)
    }

    /// Statement scanner for the default restore pipeline.
    /// Override to disable `DELIMITER`, change quote rules, or swap splitters.
    fn new_sql_scanner(&self) -> crate::sql_split::SqlStatementScanner {
        crate::sql_split::SqlStatementScanner::new()
    }

    /// Split a complete SQL buffer. Default uses [`Self::new_sql_scanner`].
    fn split_restore_sql(&self, sql: &str) -> Vec<String> {
        let mut scanner = self.new_sql_scanner();
        let mut out = scanner.push(sql);
        out.extend(scanner.finish());
        out
    }

    /// After a restore statement fails: clear an aborted PG transaction,
    /// create missing `nextval` sequences, and when `overwrite` is set, drop an
    /// existing relation and ask the pipeline to retry `CREATE`.
    async fn recover_restore_statement(
        &self,
        handle: &ConnectionHandle,
        stmt: &str,
        error: &DriverError,
        overwrite: bool,
    ) -> Result<bool, DriverError> {
        crate::sql_dump::recover_restore_statement_default(self, handle, stmt, error, overwrite)
            .await
    }

    /// Restore a SQL dump by executing statements against the live connection.
    ///
    /// Default uses [`crate::sql_dump::RestoreSession`] (streaming-capable) and
    /// honors [`BackupRestoreOptions::single_transaction`] when the **user**
    /// requested it. Dump-header `-- Options: single-transaction` is dump-time
    /// snapshot only and is not treated as restore atomicity.
    /// Override this method to replace the entire restore pipeline.
    async fn restore_sql(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
    ) -> Result<(), DriverError> {
        self.restore_sql_with_progress(handle, sql, opts, &mut |_| {})
            .await
    }

    /// Same as [`Self::restore_sql`] with per-statement progress.
    async fn restore_sql_with_progress(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        crate::sql_dump::restore_sql_statements_with_progress::<Self, _>(
            self,
            handle,
            sql,
            opts,
            on_progress,
        )
        .await
    }

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(StructureCapabilities {
            dialect_id: self.driver_type(),
            ..Default::default()
        })
    }

    async fn plan_structure_changes(
        &self,
        _handle: &ConnectionHandle,
        _request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        Err(DriverError::Unsupported(
            "table structure planning is not supported by this driver".into(),
        ))
    }
}

/// Default `query` / `execute` command dispatch shared by SQL drivers.
///
/// F7: the input object may carry optional targeting fields `database` /
/// `schema` (injected by the host from the IPC envelope). When present, the
/// SQL is rewritten through [`DatabaseDriver::qualify_sql_target`] before
/// execution; drivers without the capability execute as-is (logged), keeping
/// the host session pin as fallback.
pub async fn execute_standard_sql_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    match command {
        "query" => {
            let sql = sql_input_with_target(driver, &input, "query")?;
            let limit = input
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(u32::MAX as u64) as u32);
            let result = driver.query_multi(handle, &sql, limit).await?;
            let data = serde_json::to_value(result).map_err(|e| {
                DriverError::QueryFailed(format!("failed to serialize query result: {e}"))
            })?;
            Ok(CommandResult::new(data))
        }
        "execute" => {
            let sql = sql_input_with_target(driver, &input, "execute")?;
            let rows_affected = driver.execute(handle, &sql).await?;
            Ok(CommandResult::new(serde_json::json!({
                "rowsAffected": rows_affected
            })))
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported driver command: {other}"
        ))),
    }
}

/// Extract the `sql` input of a standard SQL command and apply F7 target
/// qualification when the host injected `database` / `schema` fields.
fn sql_input_with_target<D: DatabaseDriver + ?Sized>(
    driver: &D,
    input: &serde_json::Value,
    command: &str,
) -> Result<String, DriverError> {
    let sql = input
        .get("sql")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("command '{command}' requires string input 'sql'"))
        })?
        .to_string();

    let database = optional_target_field(input, "database");
    let schema = optional_target_field(input, "schema");
    if database.is_none() && schema.is_none() {
        return Ok(sql);
    }

    match driver.qualify_sql_target(&sql, database.as_deref(), schema.as_deref()) {
        Some(qualified) => {
            if qualified != sql {
                tracing::info!(
                    command,
                    database = database.as_deref().unwrap_or(""),
                    schema = schema.as_deref().unwrap_or(""),
                    "SQL target qualification applied by driver"
                );
            }
            Ok(qualified)
        }
        None => {
            // Legacy/rewrite-incapable driver: execute unchanged. The host's
            // ensure_session_database pin covers the database dimension; a
            // requested PG-family schema cannot be honored here.
            tracing::debug!(
                command,
                database = database.as_deref().unwrap_or(""),
                schema = schema.as_deref().unwrap_or(""),
                "driver has no SQL target rewrite capability; executing SQL as-is"
            );
            Ok(sql)
        }
    }
}

fn optional_target_field(input: &serde_json::Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod structure_defaults_tests {
    use super::*;
    use crate::ReuseDriver;
    use std::sync::Arc;

    struct StubDriver;

    #[async_trait]
    impl DatabaseDriver for StubDriver {
        fn driver_type(&self) -> DatabaseType {
            "stub".to_string()
        }

        async fn connect(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ConnectionHandle, DriverError> {
            Ok(ConnectionHandle {
                id: "conn".into(),
                pool_id: "pool".into(),
            })
        }

        async fn test_connection(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ServerInfo, DriverError> {
            Ok(ServerInfo {
                server_version: String::new(),
                server_type: self.driver_type(),
            })
        }

        async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }

        async fn get_databases(
            &self,
            _handle: &ConnectionHandle,
        ) -> Result<Vec<String>, DriverError> {
            Ok(vec![])
        }

        async fn get_tables(
            &self,
            _handle: &ConnectionHandle,
            _database: &str,
        ) -> Result<Vec<TableInfo>, DriverError> {
            Ok(vec![])
        }

        async fn get_table_schema(
            &self,
            _handle: &ConnectionHandle,
            _table: &str,
        ) -> Result<TableSchema, DriverError> {
            Ok(TableSchema {
                table_name: String::new(),
                columns: vec![],
                primary_keys: vec![],
                indexes: vec![],
                foreign_keys: vec![],
            })
        }

        async fn query(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<QueryResult, DriverError> {
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms: 0,
            })
        }

        async fn query_multi(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _limit: Option<u32>,
        ) -> Result<MultiQueryResult, DriverError> {
            Ok(MultiQueryResult {
                results: vec![],
                total_time_ms: 0,
            })
        }

        async fn query_with_params(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _params: &[Value],
        ) -> Result<QueryResult, DriverError> {
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms: 0,
            })
        }

        async fn execute(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<u64, DriverError> {
            Ok(0)
        }

        async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }
    }

    fn sample_request() -> StructureChangeRequest {
        StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: Some("public".into()),
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![],
            original_indexes: vec![],
            current_indexes: vec![],
        }
    }

    #[tokio::test]
    async fn default_structure_capabilities_are_disabled() {
        let driver = StubDriver;
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "pool".into(),
        };

        let caps = driver.structure_capabilities(&handle).await.unwrap();
        assert_eq!(caps.dialect_id, "stub");
        assert_eq!(caps.alter_strategy, AlterStrategy::None);
        assert!(!caps.create_table);
        assert!(!caps.add_column);
        assert!(!caps.drop_column);
        assert!(!caps.rename_column);
        assert!(!caps.alter_type);
        assert!(!caps.alter_nullability);
        assert!(!caps.alter_default);
        assert!(!caps.alter_primary_key);
        assert!(!caps.reorder_column);
        assert!(!caps.comment);
        assert!(!caps.create_index);
        assert!(!caps.drop_index);
        assert!(!caps.rebuild_index);
        assert!(!caps.index_type);
        assert!(!caps.index_include);
        assert!(!caps.index_filter);
        assert!(!caps.index_comment);
        assert!(caps.index_methods.is_empty());
    }

    #[tokio::test]
    async fn default_plan_structure_changes_is_unsupported() {
        let driver = StubDriver;
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "pool".into(),
        };
        let request = sample_request();

        let err = driver
            .plan_structure_changes(&handle, &request)
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "table structure planning is not supported by this driver")
        );
    }

    #[tokio::test]
    async fn reuse_driver_forwards_structure_methods() {
        let inner: Arc<dyn DatabaseDriver> = Arc::new(StubDriver);
        let driver = ReuseDriver::new(inner, "reuse-stub");
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: "pool".into(),
        };

        let caps = driver.structure_capabilities(&handle).await.unwrap();
        assert_eq!(caps.dialect_id, "stub");
        assert_eq!(driver.driver_type(), "reuse-stub");

        let err = driver
            .plan_structure_changes(&handle, &sample_request())
            .await
            .unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(_)));
    }
}

#[async_trait]
pub trait KeyValueDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError>;

    async fn get_key_detail(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        key: &str,
    ) -> Result<KeyDetail, DriverError>;
}
