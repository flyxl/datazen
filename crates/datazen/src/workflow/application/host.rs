use crate::execution::domain::permission::McpPermissionMode;
use crate::sql_template::SqlTemplateDialect;
use crate::workflow::domain::command::WorkflowCommandStep;
use crate::workflow::domain::error::WorkflowError;
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

    /// SQL literal dialect of the target connection. Hosts that know their
    /// driver override this; the standard dialect is the fail-safe default.
    async fn sql_dialect(&self, _connection_id: &str) -> SqlTemplateDialect {
        SqlTemplateDialect::Standard
    }
}
