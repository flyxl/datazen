//! Structure-default unit tests for [`crate::DatabaseDriver`].
//! Loaded from `traits.rs` via `#[path]` under `#[cfg(test)]`.

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
        _: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        Ok(ConnectionHandle {
            id: "c".into(),
            pool_id: "p".into(),
        })
    }

    async fn disconnect(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn test_connection(&self, _: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: "0".into(),
            server_type: "stub".into(),
        })
    }

    async fn get_databases(&self, _: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _: &ConnectionHandle,
        _: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn get_table_schema(
        &self,
        _: &ConnectionHandle,
        _: &str,
    ) -> Result<TableSchema, DriverError> {
        Err(DriverError::NotSupported("get_table_schema".into()))
    }

    async fn query(
        &self,
        _: &ConnectionHandle,
        _: &str,
    ) -> Result<QueryResult, DriverError> {
        unreachable!()
    }

    async fn query_multi(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        unreachable!()
    }

    async fn query_with_params(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: &[Value],
    ) -> Result<QueryResult, DriverError> {
        unreachable!()
    }

    async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
        unreachable!()
    }

    async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[tokio::test]
async fn default_structure_methods_are_unsupported() {
    let driver = StubDriver;
    let handle = ConnectionHandle {
        id: "c".into(),
        pool_id: "p".into(),
    };
    let err = driver
        .plan_structure_changes(
            &handle,
            &StructureChangeRequest {
                mode: StructureChangeMode::Create,
                schema: None,
                table: "t".into(),
                original_columns: vec![],
                current_columns: vec![],
                original_indexes: vec![],
                current_indexes: vec![],
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DriverError::Unsupported(_)));
}
