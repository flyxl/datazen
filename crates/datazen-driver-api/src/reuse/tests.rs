use super::*;
use crate::mock_driver::{MockDriver, MockDriverOptions};
use crate::query_stream::QUERY_STREAM_BATCH_SIZE;
use crate::types::{ColumnInfo, QueryResult, StatementResult};
use std::sync::Mutex;

struct FakeDriver {
    streamed: Mutex<bool>,
    override_stream: bool,
    rows: usize,
    precise_cancel: bool,
    ddl_atomicity: Option<DdlAtomicity>,
    full_fk_catalog_visibility: bool,
    /// Sentinel returned by the [`DatabaseDriver::build_insert_statement`]
    /// override. `None` models a driver that keeps the trait default, so a
    /// completed seam has to be set explicitly.
    insert_sentinel: Option<String>,
    /// Sentinel returned by [`DatabaseDriver::insert_returning_clause`].
    returning_sentinel: Option<String>,
}

impl FakeDriver {
    fn new(rows: usize, override_stream: bool) -> Arc<Self> {
        Arc::new(Self {
            streamed: Mutex::new(false),
            override_stream,
            rows,
            precise_cancel: false,
            ddl_atomicity: None,
            full_fk_catalog_visibility: false,
            insert_sentinel: None,
            returning_sentinel: None,
        })
    }

    fn new_precise(rows: usize) -> Arc<Self> {
        Arc::new(Self {
            streamed: Mutex::new(false),
            override_stream: true,
            rows,
            precise_cancel: true,
            ddl_atomicity: None,
            full_fk_catalog_visibility: false,
            insert_sentinel: None,
            returning_sentinel: None,
        })
    }

    fn with_ddl_atomicity(rows: usize, atomicity: DdlAtomicity) -> Arc<Self> {
        Arc::new(Self {
            streamed: Mutex::new(false),
            override_stream: false,
            rows,
            precise_cancel: false,
            ddl_atomicity: Some(atomicity),
            full_fk_catalog_visibility: false,
            insert_sentinel: None,
            returning_sentinel: None,
        })
    }

    fn with_full_fk_catalog_visibility(mut self: Arc<Self>) -> Arc<Self> {
        Arc::get_mut(&mut self)
            .expect("fake driver is uniquely owned")
            .full_fk_catalog_visibility = true;
        self
    }

    /// Arms the two INSERT seams with values no default implementation can
    /// produce, so forwarding is observable rather than merely plausible.
    fn with_insert_seams(
        mut self: Arc<Self>,
        insert_sentinel: &str,
        returning_sentinel: Option<&str>,
    ) -> Arc<Self> {
        let this = Arc::get_mut(&mut self).expect("fake driver is uniquely owned");
        this.insert_sentinel = Some(insert_sentinel.to_string());
        this.returning_sentinel = returning_sentinel.map(str::to_string);
        self
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

    fn ddl_atomicity(&self) -> DdlAtomicity {
        self.ddl_atomicity.unwrap_or(DdlAtomicity::Unknown)
    }

    /// Mirrors a real driver that extends the default INSERT builder: the
    /// sentinel proves the wrapper reached *this* body instead of inheriting
    /// the trait default.
    fn build_insert_statement(
        &self,
        _table: &str,
        _values: &[(&str, Option<Value>)],
    ) -> Result<BoundSqlStatement, DriverError> {
        Ok(BoundSqlStatement {
            sql: self
                .insert_sentinel
                .clone()
                .unwrap_or_else(|| "FAKE_DRIVER_DEFAULT_UNSET".into()),
            parameters: Vec::new(),
        })
    }

    fn insert_returning_clause(&self, _columns: &[&str]) -> Option<String> {
        self.returning_sentinel.clone()
    }

    async fn has_complete_foreign_key_catalog_visibility(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        Ok(self.full_fk_catalog_visibility)
    }

    async fn physical_database_identity(
        &self,
        _handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        Ok(Some(format!("physical:{database}")))
    }

    async fn schema_scope_identity(
        &self,
        _handle: &ConnectionHandle,
        database: &str,
        schema: &str,
    ) -> Result<Option<String>, DriverError> {
        Ok(Some(format!("schema:{database}:{schema}")))
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        Ok(ConnectionHandle {
            id: "h".into(),
            pool_id: "p".into(),
        })
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: "fake".into(),
        })
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        table: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Ok(TableSchema {
            table_name: table.to_string(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: TableOptions::default(),
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

    async fn prepare_query_execution(
        &self,
        _handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    async fn query_stream_with_execution(
        &self,
        handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.query_stream(handle, sql, limit, on_event).await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Ok(0)
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn cancel_query_with_execution(
        &self,
        _handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    async fn cleanup_query_execution(
        &self,
        _handle: &ConnectionHandle,
        _execution_id: &QueryExecutionId,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    fn supports_query_execution_cancel(&self) -> bool {
        self.precise_cancel
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
            schema: Option<&str>,
        ) -> Result<Vec<TableInfo>, DriverError> {
            self.inner.get_tables(handle, database, schema).await
        }
        async fn get_table_schema(
            &self,
            handle: &ConnectionHandle,
            table: &str,
            database: &str,
            schema: Option<&str>,
        ) -> Result<TableSchema, DriverError> {
            self.inner
                .get_table_schema(handle, table, database, schema)
                .await
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
        async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
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
            precise_cancel: false,
            ddl_atomicity: None,
            full_fk_catalog_visibility: false,
            insert_sentinel: None,
            returning_sentinel: None,
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

/// `ReuseDriver` must forward both INSERT seams to the wrapped driver.
///
/// The sibling test in `traits/sql_text.rs` uses `MockDriver`, which does
/// *not* override `build_insert_statement`; there the wrapper and the inner
/// driver both fall through to the same default body, so that assertion
/// cannot tell forwarding from inheritance. This one arms a real override
/// with a sentinel, which is the only way to observe the wrapper's decision.
#[test]
fn reuse_driver_forwards_both_insert_seams_to_the_inner_driver() {
    let inner = FakeDriver::new(0, false)
        .with_insert_seams("SENTINEL INNER INSERT", Some("RETURNING `id`"));
    let reuse = ReuseDriver::new(inner, "doris");

    let statement = reuse
        .build_insert_statement("rows", &[("name", Some(Value::Integer(1)))])
        .expect("forwarded insert builder");
    assert_eq!(
        statement.sql, "SENTINEL INNER INSERT",
        "the wrapper inherited the generic builder instead of forwarding"
    );

    assert_eq!(
        reuse.insert_returning_clause(&["id"]),
        Some("RETURNING `id`".to_string()),
        "the wrapper inherited `None` instead of forwarding the inner clause"
    );
}

#[tokio::test]
async fn reuse_driver_forwards_complete_fk_catalog_visibility() {
    let inner = FakeDriver::new(0, false).with_full_fk_catalog_visibility();
    let reuse = ReuseDriver::new(inner, "mariadb");
    let handle = ConnectionHandle {
        id: "h".into(),
        pool_id: "p".into(),
    };

    assert!(reuse
        .has_complete_foreign_key_catalog_visibility(&handle)
        .await
        .expect("forward visibility query"));
}

#[tokio::test]
async fn reuse_driver_forwards_precise_execution_protocol_and_capability() {
    let inner = FakeDriver::new_precise(1);
    let reuse = ReuseDriver::new(inner, "cloudberry");
    let handle = ConnectionHandle {
        id: "h".into(),
        pool_id: "p".into(),
    };
    let execution_id = QueryExecutionId::new("exec-1");

    assert!(reuse.supports_query_execution_cancel());
    reuse
        .prepare_query_execution(&handle, &execution_id)
        .await
        .unwrap();
    reuse
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .unwrap();
    reuse
        .cleanup_query_execution(&handle, &execution_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn disabled_reuse_precise_protocol_does_not_call_inner_cancel() {
    let inner = FakeDriver::new_precise(1);
    let reuse = ReuseDriver::new_with_precise_cancel(inner, "questdb", false);
    let handle = ConnectionHandle {
        id: "h".into(),
        pool_id: "p".into(),
    };
    let error = reuse
        .cancel_query_with_execution(&handle, &QueryExecutionId::new("exec-1"))
        .await
        .unwrap_err();

    assert!(!reuse.supports_query_execution_cancel());
    assert!(
        matches!(error, DriverError::Unsupported(message) if message.contains("compatibility driver"))
    );
}

#[tokio::test]
async fn reuse_driver_forwards_database_and_schema_scope_identities() {
    let reuse = ReuseDriver::new(FakeDriver::new(1, false), "sqlserver");
    let handle = ConnectionHandle {
        id: "h".into(),
        pool_id: "p".into(),
    };

    assert_eq!(
        reuse
            .physical_database_identity(&handle, "DataZen")
            .await
            .unwrap()
            .as_deref(),
        Some("physical:DataZen")
    );
    assert_eq!(
        reuse
            .schema_scope_identity(&handle, "DataZen", "sales")
            .await
            .unwrap()
            .as_deref(),
        Some("schema:DataZen:sales")
    );
}

#[test]
fn test_tester_reuse_driver_forwards_ddl_atomicity() {
    let inner = FakeDriver::with_ddl_atomicity(1, DdlAtomicity::Transactional);
    let reuse = ReuseDriver::new(inner, "mariadb");
    assert_eq!(reuse.ddl_atomicity(), DdlAtomicity::Transactional);
}

/// The wrapper must forward the keyset capability, or a line-level alias
/// would silently fall back to `OFFSET` paging while the wrapped driver
/// advertises the capability to the host.
#[test]
fn reuse_driver_forwards_keyset_capability() {
    let inner: Arc<dyn DatabaseDriver> = MockDriver::new(
        "postgres",
        MockDriverOptions {
            supports_keyset_pagination: true,
            ..MockDriverOptions::default()
        },
    );
    let reuse = ReuseDriver::new(inner.clone(), "questdb");

    assert!(reuse.supports_keyset_pagination());
    let order = vec![("id".to_string(), false)];
    let boundary = vec![("id".to_string(), Value::Integer(42))];
    assert_eq!(
        reuse.keyset_predicate_sql(&order, &boundary),
        inner.keyset_predicate_sql(&order, &boundary),
        "the wrapper must forward the condition unchanged"
    );
    assert_eq!(
        reuse.keyset_predicate_sql(&order, &boundary).as_deref(),
        Some("(\"id\" > 42)")
    );
}

/// A wrapper around a driver that never opted in must not invent the
/// capability either.
#[test]
fn reuse_driver_does_not_invent_the_keyset_capability() {
    let inner: Arc<dyn DatabaseDriver> = MockDriver::new("postgres", MockDriverOptions::default());
    let reuse = ReuseDriver::new(inner, "questdb");
    assert!(!reuse.supports_keyset_pagination());
}
