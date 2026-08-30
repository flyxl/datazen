//! Reuse drivers — thin wrappers that present an existing driver
//! under a different `DatabaseType` id.
//!
//! Used for engines that speak a compatible wire protocol:
//! - MySQL protocol: Doris, StarRocks, ManticoreSearch, OceanBase (MySQL mode)
//! - PostgreSQL wire: QuestDB, Cloudberry

use crate::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ReuseDriver {
    inner: Arc<dyn DatabaseDriver>,
    db_type: DatabaseType,
}

impl ReuseDriver {
    pub fn new(inner: Arc<dyn DatabaseDriver>, db_type: &str) -> Self {
        Self {
            inner,
            db_type: db_type.to_string(),
        }
    }
}

#[async_trait]
impl DatabaseDriver for ReuseDriver {
    fn driver_type(&self) -> DatabaseType {
        self.db_type.clone()
    }

    fn driver_category(&self) -> DriverCategory {
        self.inner.driver_category()
    }

    fn quote_char(&self) -> char {
        self.inner.quote_char()
    }

    fn skip_count_query(&self) -> bool {
        self.inner.skip_count_query()
    }

    fn supports_offset(&self) -> bool {
        self.inner.supports_offset()
    }

    fn supports_explain(&self) -> bool {
        self.inner.supports_explain()
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        self.inner.format_sql_literal(value)
    }

    fn build_update_sql(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> String {
        self.inner.build_update_sql(table, set_columns, pk_columns)
    }

    fn build_delete_sql(&self, table: &str, pk_columns: &[(&str, Option<Value>)]) -> String {
        self.inner.build_delete_sql(table, pk_columns)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.inner.connect(config).await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let info = self.inner.test_connection(config).await?;
        Ok(ServerInfo {
            server_type: self.db_type.clone(),
            ..info
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.inner.disconnect(handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.get_databases(handle).await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.get_tables(handle, database).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        self.inner.get_table_schema(handle, table).await
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        self.inner.get_columns(handle, table).await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query(handle, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.inner.query_multi(handle, sql, limit).await
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.inner.query_stream(handle, sql, limit, on_event).await
    }

    async fn prepare_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.inner
            .prepare_query_execution(handle, execution_id)
            .await
    }

    async fn query_stream_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.inner
            .query_stream_with_execution(handle, execution_id, sql, limit, on_event)
            .await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_with_params(handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.inner.execute(handle, sql).await
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        self.inner.command_definitions()
    }

    async fn execute_command(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        self.inner.execute_command(handle, command, input).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.inner.begin_transaction(handle).await
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.commit(tx).await
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.rollback(tx).await
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        self.inner.explain(handle, sql).await
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.inner.cancel_query(handle).await
    }

    async fn cancel_query_with_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.inner
            .cancel_query_with_execution(handle, execution_id)
            .await
    }

    async fn cleanup_query_execution(
        &self,
        handle: &ConnectionHandle,
        execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        self.inner
            .cleanup_query_execution(handle, execution_id)
            .await
    }

    fn supports_query_execution_cancel(&self) -> bool {
        self.inner.supports_query_execution_cancel()
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        let info = self.inner.get_server_info(handle).await?;
        Ok(ServerInfo {
            server_type: self.db_type.clone(),
            ..info
        })
    }

    async fn use_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<(), DriverError> {
        self.inner.use_database(handle, database).await
    }

    fn prompt_overrides(&self) -> HashMap<PromptScenario, PromptTemplate> {
        self.inner.prompt_overrides()
    }

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_table_ddl(handle, table).await
    }

    async fn dump_view_ddl(
        &self,
        handle: &ConnectionHandle,
        view: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_view_ddl(handle, view).await
    }

    async fn dump_routines(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_routines(handle, database).await
    }

    async fn dump_triggers(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<String, DriverError> {
        self.inner.dump_triggers(handle, database).await
    }

    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.inner.dump_database(handle, database, opts).await
    }

    async fn dump_database_with_progress(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        self.inner
            .dump_database_with_progress(handle, database, opts, on_progress)
            .await
    }

    fn uses_sql_restore_pipeline(&self) -> bool {
        self.inner.uses_sql_restore_pipeline()
    }

    fn new_sql_scanner(&self) -> crate::sql_split::SqlStatementScanner {
        self.inner.new_sql_scanner()
    }

    fn split_restore_sql(&self, sql: &str) -> Vec<String> {
        self.inner.split_restore_sql(sql)
    }

    async fn recover_restore_statement(
        &self,
        handle: &ConnectionHandle,
        stmt: &str,
        error: &DriverError,
        overwrite: bool,
    ) -> Result<bool, DriverError> {
        self.inner
            .recover_restore_statement(handle, stmt, error, overwrite)
            .await
    }

    async fn restore_sql(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
    ) -> Result<(), DriverError> {
        self.inner.restore_sql(handle, sql, opts).await
    }

    async fn restore_sql_with_progress(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        opts: Option<&BackupRestoreOptions>,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<(), DriverError> {
        self.inner
            .restore_sql_with_progress(handle, sql, opts, on_progress)
            .await
    }

    async fn structure_capabilities(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        self.inner.structure_capabilities(handle).await
    }

    async fn plan_structure_changes(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        self.inner.plan_structure_changes(handle, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_stream::QUERY_STREAM_BATCH_SIZE;
    use crate::types::{ColumnInfo, QueryResult, StatementResult};
    use std::sync::Mutex;

    struct FakeDriver {
        streamed: Mutex<bool>,
        override_stream: bool,
        rows: usize,
    }

    impl FakeDriver {
        fn new(rows: usize, override_stream: bool) -> Arc<Self> {
            Arc::new(Self {
                streamed: Mutex::new(false),
                override_stream,
                rows,
            })
        }

        fn statement(&self, sql: &str) -> StatementResult {
            StatementResult {
                sql: sql.to_string(),
                columns: vec![ColumnInfo {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                }],
                rows: (0..self.rows as i64)
                    .map(|i| vec![Some(Value::Integer(i))])
                    .collect(),
                rows_affected: Some(self.rows as u64),
                execution_time_ms: 1,
                truncated: false,
            }
        }
    }

    #[async_trait]
    impl DatabaseDriver for FakeDriver {
        fn driver_type(&self) -> DatabaseType {
            "fake".into()
        }

        async fn connect(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ConnectionHandle, DriverError> {
            Ok(ConnectionHandle {
                id: "h".into(),
                pool_id: "p".into(),
            })
        }

        async fn test_connection(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ServerInfo, DriverError> {
            Ok(ServerInfo {
                server_version: String::new(),
                server_type: "fake".into(),
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
            table: &str,
        ) -> Result<TableSchema, DriverError> {
            Ok(TableSchema {
                table_name: table.to_string(),
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
            sql: &str,
            _limit: Option<u32>,
        ) -> Result<MultiQueryResult, DriverError> {
            Ok(MultiQueryResult {
                results: vec![self.statement(sql)],
                total_time_ms: 1,
            })
        }

        async fn query_stream(
            &self,
            handle: &ConnectionHandle,
            sql: &str,
            limit: Option<u32>,
            on_event: QueryStreamCallback,
        ) -> Result<(), DriverError> {
            *self.streamed.lock().unwrap() = true;
            if self.override_stream {
                emit_multi_query_as_stream(self.query_multi(handle, sql, limit).await?, &on_event);
                return Ok(());
            }
            let result = self.query_multi(handle, sql, limit).await?;
            emit_multi_query_as_stream(result, &on_event);
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

    fn collect() -> (QueryStreamCallback, Arc<Mutex<Vec<QueryStreamEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_cb = Arc::clone(&events);
        (
            Arc::new(move |ev| {
                events_cb.lock().unwrap().push(ev);
            }),
            events,
        )
    }

    #[tokio::test]
    async fn default_query_stream_chunks_materialized_rows() {
        struct DefaultStreamDriver {
            inner: FakeDriver,
        }

        #[async_trait]
        impl DatabaseDriver for DefaultStreamDriver {
            fn driver_type(&self) -> DatabaseType {
                self.inner.driver_type()
            }
            async fn connect(
                &self,
                config: &ConnectionConfig,
            ) -> Result<ConnectionHandle, DriverError> {
                self.inner.connect(config).await
            }
            async fn test_connection(
                &self,
                config: &ConnectionConfig,
            ) -> Result<ServerInfo, DriverError> {
                self.inner.test_connection(config).await
            }
            async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
                self.inner.disconnect(handle).await
            }
            async fn get_databases(
                &self,
                handle: &ConnectionHandle,
            ) -> Result<Vec<String>, DriverError> {
                self.inner.get_databases(handle).await
            }
            async fn get_tables(
                &self,
                handle: &ConnectionHandle,
                database: &str,
            ) -> Result<Vec<TableInfo>, DriverError> {
                self.inner.get_tables(handle, database).await
            }
            async fn get_table_schema(
                &self,
                handle: &ConnectionHandle,
                table: &str,
            ) -> Result<TableSchema, DriverError> {
                self.inner.get_table_schema(handle, table).await
            }
            async fn query(
                &self,
                handle: &ConnectionHandle,
                sql: &str,
            ) -> Result<QueryResult, DriverError> {
                self.inner.query(handle, sql).await
            }
            async fn query_multi(
                &self,
                handle: &ConnectionHandle,
                sql: &str,
                limit: Option<u32>,
            ) -> Result<MultiQueryResult, DriverError> {
                self.inner.query_multi(handle, sql, limit).await
            }
            async fn query_with_params(
                &self,
                handle: &ConnectionHandle,
                sql: &str,
                params: &[Value],
            ) -> Result<QueryResult, DriverError> {
                self.inner.query_with_params(handle, sql, params).await
            }
            async fn execute(
                &self,
                handle: &ConnectionHandle,
                sql: &str,
            ) -> Result<u64, DriverError> {
                self.inner.execute(handle, sql).await
            }
            async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
                self.inner.cancel_query(handle).await
            }
        }

        let driver = DefaultStreamDriver {
            inner: FakeDriver {
                streamed: Mutex::new(false),
                override_stream: false,
                rows: QUERY_STREAM_BATCH_SIZE + 2,
            },
        };
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };
        let (cb, events) = collect();
        driver
            .query_stream(&handle, "SELECT 1", None, cb)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        let chunks = events
            .iter()
            .filter(|e| matches!(e, QueryStreamEvent::Rows { .. }))
            .count();
        assert!(chunks >= 2);
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    }

    #[tokio::test]
    async fn reuse_driver_forwards_query_stream() {
        let inner = FakeDriver::new(3, true);
        let reuse = ReuseDriver::new(inner.clone(), "mariadb");
        let handle = ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        };
        let (cb, events) = collect();
        reuse
            .query_stream(&handle, "SELECT 1", Some(10), cb)
            .await
            .unwrap();
        assert!(*inner.streamed.lock().unwrap());
        let events = events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, QueryStreamEvent::StatementStart { .. })));
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    }
}
