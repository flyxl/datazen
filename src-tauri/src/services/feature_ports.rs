use super::ConnectionManager;
use crate::store::Store;
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DatabaseDriver};
use datazen_feature_core::services::ports::{
    ConnectionCatalog, DatabaseToolHost, SchemaContextSource,
};
use std::sync::Arc;
#[async_trait::async_trait]
impl DatabaseToolHost for ConnectionManager {
    async fn resolve_session_for_connection(
        &self,
        id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        self.resolve_session_for_connection(id)
            .await
            .map_err(|error| error.to_string())
    }
    async fn get_session_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        self.get_session_config(id)
            .await
            .map_err(|error| error.to_string())
    }
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        let (_, driver, handle) = self
            .resolve_session_for_connection(id)
            .await
            .map_err(|error| error.to_string())?;
        // The legacy session adapter historically exposes explain through the
        // driver trait, independently of its advertised command definitions.
        if command == "explain" {
            let sql = input
                .get("sql")
                .and_then(serde_json::Value::as_str)
                .ok_or("Explain requires sql")?;
            let result = driver
                .explain(&handle, sql)
                .await
                .map_err(|error| error.to_string())?;
            return serde_json::to_value(result)
                .map(datazen_driver_api::CommandResult::new)
                .map_err(|error| error.to_string());
        }
        driver
            .execute_command(&handle, command, input)
            .await
            .map_err(|error| error.to_string())
    }
    async fn execution_settings(&self) -> datazen_platform_api::settings::ExecutionSettings {
        self.feature_execution_settings().await
    }
}
#[async_trait::async_trait]
impl SchemaContextSource for ConnectionManager {
    async fn get_session(
        &self,
        id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        self.get_session(id)
            .await
            .map_err(|error| error.to_string())
    }
    async fn session_schema(&self, id: &str) -> Option<String> {
        self.get_session_config(id)
            .await
            .ok()
            .and_then(|config| config.schema)
    }
}
#[async_trait::async_trait]
impl ConnectionCatalog for Store {
    async fn get_connections(&self) -> Vec<ConnectionConfig> {
        self.get_connections().await
    }
}

#[async_trait::async_trait]
impl DatabaseToolHost for crate::commands::AppState {
    async fn resolve_session_for_connection(
        &self,
        id: &str,
    ) -> Result<(String, Arc<dyn DatabaseDriver>, ConnectionHandle), String> {
        self.connection_manager
            .resolve_session_for_connection(id)
            .await
            .map_err(|error| error.to_string())
    }
    async fn get_session_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        self.connection_manager
            .get_session_config(id)
            .await
            .map_err(|error| error.to_string())
    }
    async fn execution_settings(&self) -> datazen_platform_api::settings::ExecutionSettings {
        self.store.execution_settings().await
    }
    async fn profile_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        self.store
            .platform_profile(id)
            .await
            .filter(|(_, metadata)| metadata.enabled)
            .map(|(config, _)| config)
            .ok_or("Profile unavailable".into())
    }
    async fn driver_for_connection(&self, id: &str) -> Result<Arc<dyn DatabaseDriver>, String> {
        let config = self.profile_config(id).await?;
        self.driver_registry
            .get(&config.database_type)
            .await
            .ok_or("Driver unavailable".into())
    }
    async fn execute_target(
        &self,
        id: &str,
        command: &str,
        input: serde_json::Value,
    ) -> Result<datazen_driver_api::CommandResult, String> {
        let step = datazen_feature_core::workflow::command::WorkflowCommandStep::new(
            "tool",
            command,
            Some(id.into()),
            input,
        );
        datazen_feature_core::workflow::resource_execution::execute(
            self, &step, id, None, None, false,
        )
        .await
        .map_err(|error| error.to_string())
    }
}
