//! Desktop bindings for shared workflow execution.
use crate::commands::AppState;
use datazen_ai_api::{ChatMessage, CompletionRequest, MessageRole};
use datazen_feature_core::mcp::permission::McpPermissionMode;
use datazen_feature_core::workflow::command::WorkflowCommandStep;
use datazen_feature_core::workflow::error::WorkflowError;
use datazen_feature_core::workflow::model::{
    StepExecutionResult, UnattendedDestructivePolicy, WorkflowMigrationOperation,
};
use datazen_feature_core::workflow::{context::WorkflowContext, host::WorkflowHost};

#[async_trait::async_trait]
impl WorkflowHost for AppState {
    async fn connection_name(&self, id: &str) -> Result<String, WorkflowError> {
        self.store
            .platform_profile(id)
            .await
            .filter(|(_, metadata)| metadata.enabled)
            .map(|(config, _)| config.name)
            .ok_or_else(|| WorkflowError::ConnectionFailed {
                connection_id: id.into(),
                message: "profile unavailable".into(),
            })
    }
    async fn execute_command(
        &self,
        step: &WorkflowCommandStep,
        workflow_connection: Option<&str>,
        permission_mode: Option<McpPermissionMode>,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<datazen_driver_api::CommandResult, WorkflowError> {
        super::command_runtime::execute_command_with_deadline(
            self,
            step,
            workflow_connection,
            permission_mode,
            deadline,
        )
        .await
    }
    async fn complete_ai(&self, prompt: String) -> Result<String, WorkflowError> {
        let config = self
            .store
            .get_ai_config()
            .await
            .ok_or_else(|| WorkflowError::Ai("AI not configured".into()))?;
        let provider = self
            .ai_registry
            .get(&config.provider_type)
            .await
            .ok_or_else(|| WorkflowError::Ai("AI provider not available".into()))?;
        let gate = datazen_feature_core::ai::safety::effective_safety_gate(
            &self.store.get_ai_settings_config().await,
            &self.store.execution_settings().await,
        );
        let prompt = datazen_feature_core::ai::safety::redact_for_gate(&prompt, &gate);
        let request = CompletionRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            model: config.model.clone(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.3),
            stop: None,
            tools: None,
            previous_response_id: None,
            cancel_token: None,
        };
        provider
            .complete(&request)
            .await
            .map(|response| response.content)
            .map_err(|error| WorkflowError::Ai(error.to_string()))
    }
    async fn execute_profile_step(
        &self,
        id: &str,
        operation: WorkflowMigrationOperation,
        profile_id: &str,
        revision: Option<&str>,
        destructive_policy: UnattendedDestructivePolicy,
        sql_file_token_variable: Option<&str>,
        context: &mut WorkflowContext,
    ) -> Result<StepExecutionResult, WorkflowError> {
        super::migration::execute_profile_step(
            self,
            id,
            operation,
            profile_id,
            revision,
            destructive_policy,
            sql_file_token_variable,
            context,
        )
        .await
    }
}
