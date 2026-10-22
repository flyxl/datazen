use super::{command::WorkflowCommandStep, error::WorkflowError};
use crate::mcp::permission::McpPermissionMode;
use datazen_driver_api::CommandResult;

#[async_trait::async_trait]
pub trait WorkflowHost: Send + Sync {
    async fn connection_name(&self, connection_id: &str) -> Result<String, WorkflowError>;
    async fn execute_command(
        &self,
        step: &WorkflowCommandStep,
        workflow_connection: Option<&str>,
        permission_mode: Option<McpPermissionMode>,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError>;
    async fn complete_ai(&self, prompt: String) -> Result<String, WorkflowError>;
}
