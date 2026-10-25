//! Desktop bindings for shared workflow execution.
use crate::commands::AppState;
use datazen_ai_providers::{ChatMessage, CompletionRequest, MessageRole};
use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use datazen_feature_core::mcp::permission::McpPermissionMode;
use datazen_feature_core::workflow::command::WorkflowCommandStep;
use datazen_feature_core::workflow::error::WorkflowError;
use datazen_feature_core::workflow::host::WorkflowHost;
use datazen_feature_core::workflow::resource_execution::{
    PreparedExecution, ResourceExecutionHost,
};
use std::sync::Arc;

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
}

/// The desktop identity/target binding for the shared independent resource executor.
///
/// The rules — what a step may dispatch, how the target is resolved and which
/// commands are allowed in a workflow — live in
/// `datazen_feature_core::workflow::resource_execution`. This binding only
/// supplies the desktop primitives: the persisted profile, the driver
/// registry, the safe-mode policy and the platform's canonical target.
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
            artifacts: crate::platform::feature_ports::HostArtifactChunks::new(runtime.clone())
                .shared(),
            use_cases: runtime,
            context: call.context().clone(),
            target: crate::platform::into_execution_target(canonical),
        })
    }
}

/// The shared executor running on the desktop host.
///
/// The executor itself lives in `datazen_feature_core::workflow::executor`;
/// what these tests pin down is that the desktop host binding of it (the
/// `WorkflowHost` impl above) carries a workflow's default connection into
/// its steps and runs data steps without a database.
#[cfg(test)]
mod host_execution_tests {
    use datazen_feature_core::workflow::executor::WorkflowExecutor;
    use datazen_feature_core::workflow::model::{
        MergeSource, StepStatus, TransformColumn, WorkflowDefinition, WorkflowStep,
    };

    #[tokio::test]
    async fn workflow_default_connection_is_inherited_by_legacy_query() {
        let test = crate::testing::app_state::TestAppState::with_resource_provider(
            crate::testing::mock_driver::MockDriverOptions::default(),
        )
        .await;
        test.save_connection("wf-default").await;
        let workflow = WorkflowDefinition {
            database: None,
            id: "inherit".into(),
            name: "Inherit".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: Some("wf-default".into()),
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.steps[0].step_type, "query");
        assert_eq!(result.steps[0].status, StepStatus::Success);
    }

    #[tokio::test]
    async fn step_connection_override_executes_command() {
        let test = crate::testing::app_state::TestAppState::with_resource_provider(
            crate::testing::mock_driver::MockDriverOptions::default(),
        )
        .await;
        test.save_connection("wf-default").await;
        test.save_connection("wf-override").await;
        let workflow = WorkflowDefinition {
            database: None,
            id: "override".into(),
            name: "Override".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: Some("wf-default".into()),
            steps: vec![WorkflowStep::Command {
                id: "c1".into(),
                command: "query".into(),
                connection: Some("wf-override".into()),
                input: serde_json::json!({ "sql": "SELECT 1" }),
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.steps[0].step_type, "query");
    }

    #[tokio::test]
    async fn merge_and_transform_steps_execute_without_db() {
        let test = crate::testing::app_state::TestAppState::new().await;
        let workflow = WorkflowDefinition {
            database: None,
            id: "data-step".into(),
            name: "DataStep".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: None,
            steps: vec![
                WorkflowStep::Transform {
                    id: "t1".into(),
                    from: "[{\"a\":\"x\",\"amount\":10},{\"a\":\"y\",\"amount\":30}]".into(),
                    add_columns: vec![TransformColumn {
                        name: "taxed".into(),
                        expr: "amount * 1.1".into(),
                    }],
                    filter: Some("amount > 5".into()),
                    sort_by: Some("-amount".into()),
                    offset: None,
                    limit: None,
                    timeout_secs: None,
                    on_error: None,
                },
                WorkflowStep::Merge {
                    id: "m1".into(),
                    sources: vec![
                        MergeSource {
                            source: "steps.t1.rows".into(),
                            columns: serde_json::Map::new(),
                            add: serde_json::json!({"src":"CALC"})
                                .as_object()
                                .cloned()
                                .unwrap(),
                        },
                        MergeSource {
                            source: "[{\"a\":\"z\",\"amount\":1}]".into(),
                            columns: serde_json::Map::new(),
                            add: serde_json::json!({"src":"LIT"})
                                .as_object()
                                .cloned()
                                .unwrap(),
                        },
                    ],
                    columns: None,
                    timeout_secs: None,
                    on_error: None,
                },
            ],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: Default::default(),
        };
        let result =
            WorkflowExecutor::execute(&workflow, &test.state, None, &serde_json::json!({}))
                .await
                .unwrap();
        assert!(result.success, "{:?}", result.error);
        // transform t1: rows y(30) then x(10), each with taxed column.
        assert_eq!(result.steps[0].step_type, "transform");
        let t1 = result.steps[0].result.clone().unwrap();
        assert_eq!(t1["rows"][0]["amount"], 30);
        // merge m1: concat t1 (2 rows) + literal (1 row) = 3 rows, each with src.
        assert_eq!(result.steps[1].step_type, "merge");
        let m1 = result.steps[1].result.clone().unwrap();
        assert_eq!(m1["rows_count"], 3);
        assert_eq!(m1["rows"][2]["src"], "LIT");
    }
}
