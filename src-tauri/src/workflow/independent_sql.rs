//! Desktop identity/target binding for the shared independent resource executor.
use super::{WorkflowCommandStep, WorkflowError};
use crate::commands::AppState;
use datazen_driver_api::{CommandResult, ConnectionConfig, DatabaseDriver};
use datazen_feature_core::workflow::resource_execution::{
    PreparedExecution, ResourceExecutionHost,
};
use std::sync::Arc;
#[async_trait::async_trait]
impl ResourceExecutionHost for AppState {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        self.store
            .platform_profile(id)
            .await
            .map(|(config, metadata)| (config, metadata.config_revision, metadata.enabled))
            .ok_or_else(|| WorkflowError::ConnectionFailed {
                connection_id: id.into(),
                message: "profile unavailable".into(),
            })
    }
    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        self.driver_registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| WorkflowError::Driver("Driver unavailable".into()))
    }
    async fn safe_mode(&self) -> bool {
        self.store.execution_settings().await.safe_mode
    }
    async fn prepare(
        &self,
        _id: &str,
        revision: u64,
        command: &str,
        target: datazen_platform_api::target::ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        let adapter = self
            .platform_require()
            .map_err(|error| WorkflowError::Validation(error.to_string()))?;
        let call = adapter.begin_call();
        let runtime = adapter
            .runtime()
            .map_err(|error| WorkflowError::Validation(error.to_string()))?;
        let canonical = adapter
            .resolve_execution_target(
                call.context(),
                datazen_platform_api::id::Counter::new(revision),
                command,
                &target,
            )
            .await
            .map_err(|error| WorkflowError::Validation(error.message))?;
        Ok(PreparedExecution {
            runtime,
            context: call.context().clone(),
            target: crate::platform::into_execution_target(canonical),
        })
    }
}
pub(super) async fn execute(
    state: &AppState,
    step: &WorkflowCommandStep,
    connection_id: &str,
    mode: Option<crate::mcp::permission::McpPermissionMode>,
    deadline: Option<tokio::time::Instant>,
) -> Result<CommandResult, WorkflowError> {
    datazen_feature_core::workflow::resource_execution::execute(
        state,
        step,
        connection_id,
        mode,
        deadline,
        true,
    )
    .await
}
