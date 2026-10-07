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

/// The port SQL Server dials when the config names none. `connect_client`
/// substitutes it explicitly, and `build_config` leaves it to tiberius, whose
/// own unset-port default is the same value; one constant feeds both plus
/// `DatabaseDriver::default_port`, so the host can never compare endpoints
/// against a port the driver does not dial. SQL Server has no implicit *host* —
/// `build_config` rejects a missing one — so `default_host` stays `None`.
const DEFAULT_PORT: u16 = 1433;

mod catalog;
mod dialect;
#[cfg(test)]
mod file_line_cap;
mod session;
#[cfg(test)]
mod tests;

use dialect::{
    apply_sqlserver_top, needs_own_batch, parse_physical_database_identity,
    parse_schema_scope_identity, schema_migration_blockers_for_indexes, split_statements,
    PHYSICAL_DATABASE_IDENTITY_SQL,
};

impl SqlServerDriver {
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            transactions: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl DatabaseDriver for SqlServerDriver {
    fn default_port(&self) -> Option<u16> {
        Some(DEFAULT_PORT)
    }

    fn migration_renderer(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationRenderer>> {
        Some(std::sync::Arc::new(super::SqlServerMigrationRenderer))
    }

    fn migration_capabilities(
        &self,
    ) -> Option<std::sync::Arc<dyn datazen_driver_api::MigrationCapabilities>> {
        Some(std::sync::Arc::new(super::SqlServerMigrationCapabilities))
    }

    fn type_normalizer(&self) -> Option<std::sync::Arc<dyn datazen_driver_api::TypeNormalizer>> {
        Some(std::sync::Arc::new(super::SqlServerTypeNormalizer))
    }

    async fn physical_database_identity(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        let mut clients = self.clients.write().await;
        let client = clients
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let parameters = [Value::String(database.trim().to_owned())];
        let result =
            Self::run_with_params(client, PHYSICAL_DATABASE_IDENTITY_SQL, &parameters).await?;
        Ok(parse_physical_database_identity(&result))
    }

    async fn schema_scope_identity(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: &str,
    ) -> Result<Option<String>, DriverError> {
        let mut clients = self.clients.write().await;
        let client = clients
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let parameters = [Value::String(schema.trim().to_owned())];
        let result = Self::run_with_params(
            client,
            &crate::metadata::schema_scope_identity_sql(database),
            &parameters,
        )
        .await?;
        Ok(parse_schema_scope_identity(&result))
    }

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
        SqlServerDriver::test_connection(self, config).await
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        SqlServerDriver::connect(self, config).await
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        SqlServerDriver::disconnect(self, handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        SqlServerDriver::get_databases(self, handle).await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        SqlServerDriver::get_tables(self, handle, database, schema).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        SqlServerDriver::get_table_schema(self, handle, table, database, schema).await
    }

    async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        SqlServerDriver::get_all_columns(self, handle, database, schema).await
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

    fn explicit_identity_insert_requires_session_toggle(&self) -> bool {
        true
    }

    fn transfer_sql_file_begin_transaction(&self) -> &'static str {
        "BEGIN TRANSACTION;"
    }

    fn transfer_sql_file_commit_transaction(&self) -> &'static str {
        "COMMIT TRANSACTION;"
    }

    async fn set_identity_insert(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
        table: &str,
        enabled: bool,
    ) -> Result<(), DriverError> {
        SqlServerDriver::set_identity_insert(self, handle, database, schema, table, enabled).await
    }

    async fn discard_connection(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        SqlServerDriver::discard_connection(self, handle).await
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
        SqlServerDriver::execute_with_params(self, handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        SqlServerDriver::execute(self, handle, sql).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        SqlServerDriver::begin_transaction(self, handle).await
    }

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        SqlServerDriver::begin_read_snapshot(self, handle).await
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        SqlServerDriver::commit(self, tx).await
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        SqlServerDriver::rollback(self, tx).await
    }

    /// SQL Server has no cancellation this call could honestly perform: the
    /// `ATTENTION` packet needs a second, concurrent session that this driver
    /// never opens, so one call cannot prove the query stopped.
    ///
    /// This used to answer `Ok(())`, which reported a cancellation that had
    /// never happened; Track O turns it into an explicit refusal.
    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Err(DriverError::Unsupported(
            "SQL Server has no per-execution cancellation; the legacy session-wide cancel does nothing"
                .into(),
        ))
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
