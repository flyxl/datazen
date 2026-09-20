//! Core driver traits.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::query_stream::{emit_multi_query_as_stream, QueryStreamCallback};
use crate::schema_migration::{MigrationCapabilities, MigrationRenderer, TypeNormalizer};
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

    fn sync_category(&self) -> SyncCategory {
        match self.driver_category() {
            DriverCategory::Sql => SyncCategory::Sql,
            DriverCategory::KeyValue => SyncCategory::Kv,
            DriverCategory::Document => SyncCategory::Document,
        }
    }

    fn sync_family(&self) -> String {
        self.driver_type()
    }

    fn migration_renderer(&self) -> Option<Arc<dyn MigrationRenderer>> {
        None
    }

    fn migration_capabilities(&self) -> Option<Arc<dyn MigrationCapabilities>> {
        None
    }

    fn type_normalizer(&self) -> Option<Arc<dyn TypeNormalizer>> {
        None
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

    fn supports_offset(&self) -> bool {
        true
    }

    fn supports_explain(&self) -> bool {
        true
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        DdlAtomicity::Unknown
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
    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError>;
    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError>;
    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError>;
    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError>;
    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError>;
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
