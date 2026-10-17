use super::{
    command::WorkflowCommandStep, context::WorkflowContext, error::WorkflowError, model::*,
};
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::CommandResult;

#[async_trait::async_trait]
pub trait WorkflowHost: Send + Sync {
    /// Gate the entire run before independent steps dispatch. Current adapters
    /// have no authorized WorkflowBlock Job owner or block TTL lifecycle.
    fn validate_session_block(&self) -> Result<(), WorkflowError> {
        Err(WorkflowError::Validation("sessionBlock requires an authorized WorkflowBlock Job owner and TTL lifecycle; execution is unsupported".into()))
    }
    /// Metadata-only preflight; never acquires a database resource.
    async fn target_requirements(
        &self,
        connection: &str,
        command: &str,
    ) -> Result<super::target_plan::TargetRequirements, WorkflowError> {
        let resources = self.resources().ok_or_else(|| {
            WorkflowError::Validation(
                "CapabilityUnsupported: workflow driver target metadata unavailable".into(),
            )
        })?;
        let (config, _, enabled) = resources.profile(connection).await?;
        if !enabled {
            return Err(WorkflowError::Validation("connection disabled".into()));
        }
        let driver = resources.driver(&config).await?;
        let requirements = driver.target_requirements(command).ok_or_else(|| {
            WorkflowError::UnsupportedCommand {
                command: command.into(),
                connection_id: connection.into(),
            }
        })?;
        Ok(super::target_plan::TargetRequirements::from_driver(
            requirements.namespace.database,
            driver.has_multi_database(),
            command,
        ))
    }
    /// Must clean candidate resources if cancelled before returning a receipt.
    /// An unconfirmed admission must return SqlUnconfirmed and must not replay.
    async fn open_session_block(
        &self,
        _target: &super::target_plan::ExecutionTarget,
        _deadline: tokio::time::Instant,
    ) -> Result<super::session_block::RetainedSession, WorkflowError> {
        Err(WorkflowError::Validation(
            "trusted workflow Job session adapter unavailable".into(),
        ))
    }
    fn resources(&self) -> Option<&dyn super::resource_execution::ResourceExecutionHost> {
        None
    }
    async fn connection_name(&self, connection_id: &str) -> Result<String, WorkflowError>;
    async fn execute_command(
        &self,
        step: &WorkflowCommandStep,
        workflow_connection: Option<&str>,
        permission_mode: Option<McpPermissionMode>,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError>;
    async fn complete_ai(&self, prompt: String) -> Result<String, WorkflowError>;
    async fn execute_profile_step(
        &self,
        id: &str,
        operation: WorkflowMigrationOperation,
        profile_id: &str,
        revision: Option<&str>,
        destructive_policy: UnattendedDestructivePolicy,
        sql_file_token_variable: Option<&str>,
        context: &mut WorkflowContext,
    ) -> Result<StepExecutionResult, WorkflowError>;
}
