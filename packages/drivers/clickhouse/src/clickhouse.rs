//! ClickHouse driver — HTTP interface with `default_format=JSON`.
//!
//! Schema metadata is read from the `system` tables.

use async_trait::async_trait;
use datazen_driver_api::*;
use datazen_driver_http_support::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// ClickHouse's own default database, used when neither the explicit argument
/// nor the connection's configured database names one.
const DEFAULT_DATABASE: &str = "default";

struct PoolEntry {
    client: reqwest::Client,
    base: String,
    database: Option<String>,
}

pub struct ClickHouseDriver {
    pools: RwLock<HashMap<String, PoolEntry>>,
}

impl ClickHouseDriver {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }

    fn get<'a>(
        pools: &'a HashMap<String, PoolEntry>,
        handle: &ConnectionHandle,
    ) -> Result<&'a PoolEntry, DriverError> {
        pools
            .get(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))
    }

    /// Database a metadata read targets.
    ///
    /// The explicit argument wins; a blank one falls back to the database this
    /// handle was connected to and finally to ClickHouse's `default`. That is
    /// the pre-contract behavior minus the mutable per-pool "current database"
    /// that `use_database` used to write — no read consults session state.
    fn effective_database(entry: &PoolEntry, database: &str) -> String {
        let explicit = database.trim();
        if !explicit.is_empty() {
            return explicit.to_string();
        }
        entry
            .database
            .as_deref()
            .map(str::trim)
            .filter(|db| !db.is_empty())
            .unwrap_or(DEFAULT_DATABASE)
            .to_string()
    }

    /// Strip a `` `db`. `` / `db.` prefix from a caller-supplied relation name,
    /// so a qualified reference still resolves to the bare table.
    fn bare_table_name(table: &str) -> &str {
        let stripped = match table.rsplit_once('.') {
            Some((_, name)) if !name.is_empty() => name,
            _ => table,
        };
        stripped.trim().trim_matches('`')
    }

    async fn http_query(
        client: &reqwest::Client,
        base: &str,
        sql: &str,
        database: Option<&str>,
    ) -> Result<serde_json::Value, DriverError> {
        let url = format!("{base}/");
        let mut req = client
            .post(&url)
            .query(&[("default_format", "JSON"), ("max_result_rows", "100000")]);
        if let Some(db) = database {
            let trimmed = db.trim();
            if !trimmed.is_empty() {
                req = req.query(&[("database", trimmed)]);
            }
        }
        let resp = req
            .body(sql.to_string())
            .send()
            .await
            .map_err(|e| http_error("ClickHouse request failed", e))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| http_error("ClickHouse read response failed", e))?;
        if !status.is_success() {
            return Err(status_error("ClickHouse query failed", status, &text));
        }
        serde_json::from_str(&text)
            .map_err(|e| DriverError::QueryFailed(format!("ClickHouse JSON parse failed: {e}")))
    }

    fn result_from_json(v: &serde_json::Value) -> QueryResult {
        let meta = v
            .get("meta")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();
        let columns: Vec<ColumnInfo> = meta
            .iter()
            .filter_map(|m| {
                Some(ColumnInfo {
                    name: m.get("name")?.as_str()?.to_string(),
                    data_type: m
                        .get("type")
                        .map(|t| t.as_str().unwrap_or("").to_string())
                        .unwrap_or_default(),
                    nullable: true,
                })
            })
            .collect();
        let data = v
            .get("data")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default();
        let rows: Vec<Vec<Option<Value>>> = data
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|c| row.get(c.name.as_str()).and_then(json_to_value))
                    .collect()
            })
            .collect();
        QueryResult {
            columns,
            rows,
            rows_affected: v.get("rows").and_then(|r| r.as_u64()),
            execution_time_ms: 0,
        }
    }

    fn stream_json(
        v: &serde_json::Value,
        sql: &str,
        limit: Option<u32>,
        on_event: &QueryStreamCallback,
        ms: u64,
    ) {
        let columns: Vec<ColumnInfo> = v
            .get("meta")
            .and_then(|m| m.as_array())
            .into_iter()
            .flatten()
            .filter_map(|m| {
                Some(ColumnInfo {
                    name: m.get("name")?.as_str()?.to_string(),
                    data_type: m
                        .get("type")
                        .map(|t| t.as_str().unwrap_or("").to_string())
                        .unwrap_or_default(),
                    nullable: true,
                })
            })
            .collect();
        let mut batcher = QueryRowBatcher::new(Arc::clone(on_event), 0, sql.to_string(), limit);
        batcher.start(columns.clone());
        if let Some(data) = v.get("data").and_then(|d| d.as_array()) {
            for row in data {
                let decoded: Vec<Option<Value>> = columns
                    .iter()
                    .map(|c| row.get(c.name.as_str()).and_then(json_to_value))
                    .collect();
                if !batcher.push(decoded) {
                    break;
                }
            }
        }
        batcher.finish(ms, v.get("rows").and_then(|r| r.as_u64()));
    }
}

#[async_trait]
impl DatabaseDriver for ClickHouseDriver {
    fn driver_type(&self) -> DatabaseType {
        "clickhouse".to_string()
    }

    /// F7: qualify unqualified table references with the target database
    /// (`` `db`.`t` `` — true cross-database inline qualifier). The schema
    /// argument has no meaning on ClickHouse and is ignored. Parse failures
    /// pass SQL through unchanged; see `sql_target::qualify_sql`.
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        Some(crate::sql_target::qualify_sql(sql, database, schema))
    }

    fn supports_explain(&self) -> bool {
        true
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let v = Self::http_query(
            &client,
            &base,
            "SELECT version()",
            config.database.as_deref(),
        )
        .await?;
        let version = v
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|r| r.get("version()"))
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(ServerInfo {
            server_version: version,
            server_type: "clickhouse".to_string(),
        })
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        let client = build_http_client(
            config.connection_timeout,
            config.username.as_deref(),
            config.password.as_deref(),
        )?;
        let base = base_url(config)?;
        let pool_id = format!("clickhouse_{}", uuid::Uuid::new_v4());
        self.pools.write().await.insert(
            pool_id.clone(),
            PoolEntry {
                client,
                base,
                database: config.database.clone(),
            },
        );
        Ok(ConnectionHandle {
            id: pool_id.clone(),
            pool_id,
        })
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.pools.write().await.remove(&handle.pool_id);
        Ok(())
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let v = Self::http_query(
            &entry.client,
            &entry.base,
            "SELECT name FROM system.databases ORDER BY name",
            None,
        )
        .await?;
        Ok(v.get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|r| r.get("name").and_then(|x| x.as_str()).map(String::from))
            .collect())
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        // ClickHouse's "schema" *is* the database, so a schema argument is a
        // caller bug (the validator rejects it).
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let db = Self::effective_database(entry, database);
        let sql = format!(
            "SELECT name, engine FROM system.tables WHERE database = '{}' AND is_temporary = 0 ORDER BY name",
            db.replace('\'', "''")
        );
        // `system.tables` is fully qualified and the database is an explicit
        // predicate: the read never switches the session's database (no `USE`).
        let v = Self::http_query(&entry.client, &entry.base, &sql, None).await?;
        Ok(v.get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some(TableInfo {
                    name: r.get("name")?.as_str()?.to_string(),
                    // ClickHouse has no schema level: reporting the database
                    // name here would fabricate a bogus extra tree level.
                    schema: None,
                    table_type: TableType::Table,
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
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let db = Self::effective_database(entry, database);
        let bare_table = Self::bare_table_name(table);
        let sql = format!(
            "SELECT name, type, default_expression, comment FROM system.columns WHERE database = '{}' AND table = '{}' ORDER BY position",
            db.replace('\'', "''"),
            bare_table.replace('\'', "''")
        );
        // Fully qualified `system.columns` + explicit database predicate: the
        // explicit argument is the target, with no session switch (no `USE`).
        let v = Self::http_query(&entry.client, &entry.base, &sql, None).await?;
        let columns: Vec<ColumnSchema> = v
            .get("data")
            .and_then(|d| d.as_array())
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some(ColumnSchema {
                    name: r.get("name")?.as_str()?.to_string(),
                    data_type: r
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    nullable: true,
                    default_value: r
                        .get("default_expression")
                        .and_then(|d| d.as_str())
                        .map(String::from),
                    comment: r.get("comment").and_then(|c| c.as_str()).map(String::from),
                    is_primary_key: false,
                    is_auto_increment: false,
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

    async fn dump_table_ddl(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        sql_dump::dump_table_ddl_from_schema(self, handle, table, database, schema).await
    }

    async fn dump_view_ddl(
        &self,
        _handle: &ConnectionHandle,
        view: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        Err(DriverError::NotSupported(format!(
            "View DDL dump is not supported for {view}"
        )))
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        // The explicit argument is the target database; no session/`USE` state
        // is consulted (or written) to reach it.
        let db = Self::effective_database(entry, database);
        let sql = format!(
            "SELECT table, name, type, default_kind, default_expression, comment \
             FROM system.columns WHERE database = '{}' ORDER BY table, position",
            db.replace('\'', "''")
        );
        let v = Self::http_query(&entry.client, &entry.base, &sql, None).await?;

        let mut result: HashMap<String, (Vec<ColumnSchema>, Vec<String>)> = HashMap::new();

        if let Some(data) = v.get("data").and_then(|d| d.as_array()) {
            for row in data {
                let table_name = row
                    .get("table")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let col_name = row
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let data_type = row
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let default_value = row
                    .get("default_expression")
                    .and_then(|d| d.as_str())
                    .map(String::from);
                let comment = row
                    .get("comment")
                    .and_then(|c| c.as_str())
                    .map(String::from);

                let column = ColumnSchema {
                    name: col_name.clone(),
                    data_type,
                    nullable: true, // ClickHouse columns are nullable by default
                    default_value,
                    comment,
                    is_primary_key: false, // ClickHouse has no primary key concept in schema
                    is_auto_increment: false,
                };

                let entry = result.entry(table_name).or_default();
                entry.0.push(column);
            }
        }

        Ok(result)
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let start = Instant::now();
        let v =
            Self::http_query(&entry.client, &entry.base, sql, entry.database.as_deref()).await?;
        let mut result = Self::result_from_json(&v);
        result.execution_time_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        let result = self.query(handle, sql).await?;
        Ok(MultiQueryResult {
            results: vec![StatementResult {
                sql: sql.to_string(),
                columns: result.columns,
                rows: result.rows,
                rows_affected: result.rows_affected,
                execution_time_ms: result.execution_time_ms,
                truncated: false,
            }],
            total_time_ms: result.execution_time_ms,
        })
    }

    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let start = Instant::now();
        let (effective, applied) = append_select_limit(sql, limit);
        let v = Self::http_query(
            &entry.client,
            &entry.base,
            &effective,
            entry.database.as_deref(),
        )
        .await?;
        let ms = start.elapsed().as_millis() as u64;
        Self::stream_json(&v, sql, applied, &on_event, ms);
        on_event(QueryStreamEvent::Done { total_time_ms: ms });
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
        let pools = self.pools.read().await;
        let entry = Self::get(&pools, handle)?;
        let v =
            Self::http_query(&entry.client, &entry.base, sql, entry.database.as_deref()).await?;
        Ok(v.get("rows").and_then(|r| r.as_u64()).unwrap_or(0))
    }

    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        let result = self.query(handle, &format!("EXPLAIN {sql}")).await?;
        Ok(explain_result_from_query(result))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        let mut cmds = vec![
            query_command_definition(),
            execute_command_definition(),
            query_stream_command_definition(),
        ];
        cmds.extend(schema_catalog_command_definitions());
        cmds
    }

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

    async fn structure_capabilities(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(crate::structure::clickhouse_capabilities(
            &self.driver_type(),
        ))
    }

    async fn plan_structure_changes(
        &self,
        _handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = crate::structure::clickhouse_capabilities(&self.driver_type());
        crate::structure::plan_structure_changes(&caps, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn collect_events() -> (QueryStreamCallback, Arc<Mutex<Vec<QueryStreamEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_cb = Arc::clone(&events);
        (
            Arc::new(move |ev| {
                events_cb.lock().unwrap().push(ev);
            }),
            events,
        )
    }

    #[test]
    fn stream_json_applies_host_limit_without_holding_multi_result() {
        let v = serde_json::json!({
            "meta": [{"name": "id", "type": "UInt32"}],
            "data": [{"id": 1}, {"id": 2}, {"id": 3}],
            "rows": 3
        });
        let (cb, events) = collect_events();
        ClickHouseDriver::stream_json(&v, "SELECT id FROM t", Some(2), &cb, 9);
        let events = events.lock().unwrap();
        let rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(rows, 2);
        assert!(matches!(
            events.last(),
            Some(QueryStreamEvent::StatementEnd {
                truncated: true,
                ..
            })
        ));
    }

    fn http_config(server: &wiremock::MockServer) -> ConnectionConfig {
        http_config_with_database(server, None)
    }

    fn http_config_with_database(
        server: &wiremock::MockServer,
        database: Option<&str>,
    ) -> ConnectionConfig {
        let addr = server.address();
        ConnectionConfig {
            id: "ch".into(),
            name: "ch".into(),
            database_type: "clickhouse".into(),
            host: Some(addr.ip().to_string()),
            port: Some(addr.port()),
            database: database.map(str::to_string),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: 5,
            max_pool_size: 4,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    #[tokio::test]
    async fn query_stream_hits_http_and_emits_done() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "meta": [{"name": "id", "type": "UInt8"}],
                "data": [{"id": 1}, {"id": 2}],
                "rows": 2
            })))
            .mount(&server)
            .await;

        let driver = ClickHouseDriver::new();
        let handle = driver.connect(&http_config(&server)).await.unwrap();
        let (cb, events) = collect_events();
        driver
            .query_stream(&handle, "SELECT 1", None, cb)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
        let rows: usize = events
            .iter()
            .filter_map(|e| match e {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .sum();
        assert_eq!(rows, 2);
    }

    // ── schema-dimension contract ──────────────────────────────────────────

    fn offline_handle() -> ConnectionHandle {
        ConnectionHandle {
            id: "offline".into(),
            pool_id: "offline".into(),
        }
    }

    fn pool_entry(database: Option<&str>) -> PoolEntry {
        PoolEntry {
            client: reqwest::Client::new(),
            base: String::new(),
            database: database.map(str::to_string),
        }
    }

    #[test]
    fn clickhouse_has_no_schema_level() {
        assert!(!ClickHouseDriver::new().has_schema_level());
    }

    #[test]
    fn effective_database_prefers_explicit_then_connection_then_default() {
        let connected = pool_entry(Some("conn_db"));
        // The explicit argument is authoritative.
        assert_eq!(
            ClickHouseDriver::effective_database(&connected, "analytics"),
            "analytics"
        );
        assert_eq!(
            ClickHouseDriver::effective_database(&connected, "  analytics  "),
            "analytics"
        );
        // Blank falls back to the database the handle was connected to …
        assert_eq!(
            ClickHouseDriver::effective_database(&connected, ""),
            "conn_db"
        );
        assert_eq!(
            ClickHouseDriver::effective_database(&connected, "   "),
            "conn_db"
        );
        // … and finally to ClickHouse's own default.
        let unbound = pool_entry(None);
        assert_eq!(
            ClickHouseDriver::effective_database(&unbound, ""),
            "default"
        );
        let blank = pool_entry(Some("   "));
        assert_eq!(ClickHouseDriver::effective_database(&blank, ""), "default");
    }

    #[test]
    fn bare_table_name_strips_a_database_qualifier() {
        assert_eq!(ClickHouseDriver::bare_table_name("events"), "events");
        assert_eq!(ClickHouseDriver::bare_table_name("db.events"), "events");
        assert_eq!(ClickHouseDriver::bare_table_name("`db`.`events`"), "events");
        assert_eq!(ClickHouseDriver::bare_table_name("  events  "), "events");
    }

    /// A schema argument is a caller bug on ClickHouse (its databases *are* the
    /// namespace), and the validator must reject it before any pool lookup — so
    /// this needs no live server.
    #[tokio::test]
    async fn schema_argument_is_rejected_before_touching_the_connection() {
        let driver = ClickHouseDriver::new();
        let handle = offline_handle();

        for result in [
            driver.get_tables(&handle, "default", Some("public")).await,
            driver
                .get_table_schema(&handle, "events", "default", Some("public"))
                .await
                .map(|_| Vec::new()),
            driver
                .get_all_columns(&handle, "default", Some("public"))
                .await
                .map(|_| Vec::new()),
            driver
                .dump_table_ddl(&handle, "events", "default", Some("public"))
                .await
                .map(|_| Vec::new()),
            driver
                .dump_view_ddl(&handle, "events_view", "default", Some("public"))
                .await
                .map(|_| Vec::new()),
        ] {
            let err = result.expect_err("a schema must be rejected");
            assert!(matches!(err, DriverError::InvalidConfig(_)), "{err:?}");
        }

        // No schema: validation passes and the read proceeds to the pool
        // lookup, which fails for this never-connected handle.
        let err = driver
            .get_tables(&handle, "default", None)
            .await
            .expect_err("no pool for an offline handle");
        assert!(matches!(err, DriverError::ConnectionFailed(_)), "{err:?}");
    }

    /// `get_tables` must (a) report `schema: None` — ClickHouse has no schema
    /// level, so echoing the database name there fabricates a tree level — and
    /// (b) never switch the session database for a metadata read.
    #[tokio::test]
    async fn get_tables_reports_no_schema_and_never_switches_database() {
        use wiremock::matchers::{body_string_contains, method, path, query_param_is_missing};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("system.tables"))
            .and(body_string_contains("database = 'analytics'"))
            .and(query_param_is_missing("database"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"name": "events", "engine": "MergeTree"}]
            })))
            .mount(&server)
            .await;

        let driver = ClickHouseDriver::new();
        let handle = driver.connect(&http_config(&server)).await.unwrap();
        let tables = driver
            .get_tables(&handle, "analytics", None)
            .await
            .expect("get_tables");
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "events");
        assert!(
            tables[0].schema.is_none(),
            "ClickHouse has no schema level: got {:?}",
            tables[0].schema
        );
    }

    /// `get_all_columns` must honour the explicit `database` argument (not the
    /// session's remembered database) and resolve it without a `USE`.
    #[tokio::test]
    async fn get_all_columns_filters_by_the_explicit_database() {
        use wiremock::matchers::{body_string_contains, method, path, query_param_is_missing};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("system.columns"))
            .and(body_string_contains("database = 'analytics'"))
            .and(query_param_is_missing("database"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"table": "events", "name": "id", "type": "UInt32",
                     "default_kind": "", "default_expression": "", "comment": ""},
                    {"table": "events", "name": "kind", "type": "String",
                     "default_kind": "", "default_expression": "", "comment": ""}
                ]
            })))
            .mount(&server)
            .await;

        // The connection is bound to a *different* database, so a driver that
        // still read session state would filter on `conn_db` and fail to match
        // the mock above.
        let driver = ClickHouseDriver::new();
        let handle = driver
            .connect(&http_config_with_database(&server, Some("conn_db")))
            .await
            .unwrap();
        let columns = driver
            .get_all_columns(&handle, "analytics", None)
            .await
            .expect("get_all_columns");
        let events = columns.get("events").expect("events present");
        assert_eq!(events.0.len(), 2);
        assert_eq!(events.0[0].name, "id");
        assert_eq!(events.0[1].name, "kind");
    }

    /// A blank `database` keeps the pre-contract behavior: the database the
    /// handle was connected to is used, still with no session switch.
    #[tokio::test]
    async fn get_table_schema_falls_back_to_the_connection_database() {
        use wiremock::matchers::{body_string_contains, method, path, query_param_is_missing};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_string_contains("system.columns"))
            .and(body_string_contains("database = 'conn_db'"))
            .and(body_string_contains("table = 'events'"))
            .and(query_param_is_missing("database"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"name": "id", "type": "UInt32",
                          "default_expression": "", "comment": ""}]
            })))
            .mount(&server)
            .await;

        let driver = ClickHouseDriver::new();
        let handle = driver
            .connect(&http_config_with_database(&server, Some("conn_db")))
            .await
            .unwrap();
        // A qualified relation name is accepted too: the database qualifier is
        // stripped and the explicit namespace above decides the target.
        let schema = driver
            .get_table_schema(&handle, "`conn_db`.`events`", "", None)
            .await
            .expect("get_table_schema");
        assert_eq!(schema.columns.len(), 1);
        assert_eq!(schema.columns[0].name, "id");
    }
}
