use crate::store::DesktopRepository;
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use datazen_feature_core::services::ports::{ConnectionCatalog, DatabaseToolHost};
use std::sync::Arc;

#[async_trait::async_trait]
impl ConnectionCatalog for DesktopRepository {
    async fn get_connections(&self) -> Vec<ConnectionConfig> {
        self.get_connections().await
    }
}

#[async_trait::async_trait]
impl DatabaseToolHost for crate::commands::AppState {
    async fn get_session_config(&self, id: &str) -> Result<ConnectionConfig, String> {
        let adapter = self.platform_require().map_err(|error| error.to_string())?;
        let call = adapter.begin_call();
        let view = adapter
            .runtime()
            .map_err(|error| error.to_string())?
            .get_session_by_id(call.context(), id)
            .await
            .map_err(|error| error.to_string())?;
        self.store
            .platform_profile(view.connection_id.as_str())
            .await
            .map(|(config, _)| config)
            .ok_or_else(|| "Profile unavailable".to_string())
    }
    async fn execution_settings(&self) -> datazen_modules::settings::ExecutionSettings {
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
