//! `JdbcDriver` — DatabaseDriver facade over the external Java Agent.
//!
//! Phase 0: all live operations return Unsupported. Phase 3 wires RPC.

use async_trait::async_trait;
use datazen_driver_api::*;
use std::sync::Arc;

use crate::agent_process::{AgentLaunchConfig, AgentProcessManager};

pub struct JdbcDriver {
    #[allow(dead_code)]
    agent: Arc<AgentProcessManager>,
}

impl JdbcDriver {
    pub fn new() -> Self {
        Self {
            agent: AgentProcessManager::new(AgentLaunchConfig::default()),
        }
    }

    pub fn with_agent(agent: Arc<AgentProcessManager>) -> Self {
        Self { agent }
    }

    fn not_ready(op: &str) -> DriverError {
        DriverError::Unsupported(format!(
            "JDBC driver: {op} not available yet (external Agent Phase 1–3). \
             See docs/todo/jdbc-agent-implementation-plan.md"
        ))
    }
}

impl Default for JdbcDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DatabaseDriver for JdbcDriver {
    fn driver_type(&self) -> DatabaseType {
        "jdbc".to_string()
    }

    fn supports_explain(&self) -> bool {
        false
    }

    fn supports_offset(&self) -> bool {
        // JDBC dialects vary; treat as best-effort once implemented.
        true
    }

    fn migration_renderer(&self) -> Option<std::sync::Arc<dyn MigrationRenderer>> {
        None
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        Err(Self::not_ready("connect"))
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Err(Self::not_ready("test_connection"))
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Err(Self::not_ready("get_databases"))
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Err(Self::not_ready("get_tables"))
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        _table: &str,
    ) -> Result<TableSchema, DriverError> {
        Err(Self::not_ready("get_table_schema"))
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<QueryResult, DriverError> {
        Err(Self::not_ready("query"))
    }

    async fn query_multi(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        Err(Self::not_ready("query_multi"))
    }

    async fn query_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        Err(Self::not_ready("query_with_params"))
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Err(Self::not_ready("execute"))
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_type_is_jdbc() {
        let d = JdbcDriver::new();
        assert_eq!(d.driver_type(), "jdbc");
        assert!(!d.supports_explain());
    }

    #[tokio::test]
    async fn connect_is_unsupported_in_phase0() {
        let d = JdbcDriver::new();
        let cfg = ConnectionConfig {
            id: "t".into(),
            name: "t".into(),
            database_type: "jdbc".into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: 5,
            max_pool_size: 4,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        };
        let err = d.connect(&cfg).await.unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(_)));
    }
}
