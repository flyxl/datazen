//! Core driver traits.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::query_stream::{emit_multi_query_as_stream, QueryStreamCallback};
use crate::types::*;
use crate::{
    execute_command_definition, query_command_definition, CommandResult, DriverCommandDefinition,
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
        vec![query_command_definition(), execute_command_definition()]
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
        execute_standard_sql_command(self, handle, command, input).await
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
        if opts.create_database {
            return Err(DriverError::NotSupported(
                "Backup option 'create' (CREATE DATABASE) is not supported for this driver".into(),
            ));
        }
        crate::sql_dump::dump_sql_database::<Self>(self, handle, database, opts).await
    }

    /// Restore a SQL dump by executing statements against the live connection.
    ///
    /// Default uses [`crate::sql_dump::split_sql_statements`] and honors
    /// [`BackupRestoreOptions::single_transaction`] / dump header flags.
    async fn restore_sql(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
    ) -> Result<(), DriverError> {
        crate::sql_dump::restore_sql_statements::<Self>(self, handle, sql, opts).await
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
pub async fn execute_standard_sql_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    match command {
        "query" => {
            let sql = input.get("sql").and_then(|v| v.as_str()).ok_or_else(|| {
                DriverError::InvalidConfig("command 'query' requires string input 'sql'".into())
            })?;
            let limit = input
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(u32::MAX as u64) as u32);
            let result = driver.query_multi(handle, sql, limit).await?;
            let data = serde_json::to_value(result).map_err(|e| {
                DriverError::QueryFailed(format!("failed to serialize query result: {e}"))
            })?;
            Ok(CommandResult::new(data))
        }
        "execute" => {
            let sql = input.get("sql").and_then(|v| v.as_str()).ok_or_else(|| {
                DriverError::InvalidConfig("command 'execute' requires string input 'sql'".into())
            })?;
            let rows_affected = driver.execute(handle, sql).await?;
            Ok(CommandResult::new(serde_json::json!({
                "rowsAffected": rows_affected
            })))
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported driver command: {other}"
        ))),
    }
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
