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

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
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

    async fn dump_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.inner.dump_database(handle, database, opts).await
    }

    async fn restore_sql(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<(), DriverError> {
        self.inner.restore_sql(handle, sql).await
    }
}
