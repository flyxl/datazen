//! The parser/model stays compatible; unsupported execution must fail closed.
use datazen_driver_api::CommandResult;
use datazen_feature_core::{
    mcp::permission::McpPermissionMode,
    workflow::{
        context::WorkflowContext, executor::WorkflowExecutor, host::WorkflowHost, model::*,
        WorkflowCommandStep, WorkflowError,
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};
struct NoDispatch(AtomicUsize);
impl NoDispatch {
    fn touched(&self) -> WorkflowError {
        self.0.fetch_add(1, Ordering::SeqCst);
        WorkflowError::Validation("unexpected host dispatch".into())
    }
}
#[async_trait::async_trait]
impl WorkflowHost for NoDispatch {
    async fn connection_name(&self, _: &str) -> Result<String, WorkflowError> {
        Err(self.touched())
    }
    async fn execute_command(
        &self,
        _: &WorkflowCommandStep,
        _: Option<&str>,
        _: Option<McpPermissionMode>,
        _: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError> {
        Err(self.touched())
    }
    async fn complete_ai(&self, _: String) -> Result<String, WorkflowError> {
        Err(self.touched())
    }
    async fn execute_profile_step(
        &self,
        _: &str,
        _: WorkflowMigrationOperation,
        _: &str,
        _: Option<&str>,
        _: UnattendedDestructivePolicy,
        _: Option<&str>,
        _: &mut WorkflowContext,
    ) -> Result<StepExecutionResult, WorkflowError> {
        Err(self.touched())
    }
}
#[tokio::test]
async fn nonempty_blocks_refuse_before_empty_success_or_independent_prefix_dispatch() {
    for version in [None, Some("1"), Some("2")] {
        for kind in ["sessionBlock", "transactionBlock"] {
            for prefix in [false, true] {
                let steps = if prefix {
                    serde_json::json!([{"type":"query","id":"prefix","sql":"SELECT 1","connection":"pg","database":"app"}])
                } else {
                    serde_json::json!([])
                };
                let definition: WorkflowDefinition = serde_json::from_value(serde_json::json!({
                    "id":"unsupported-block","name":"unsupported-block","description":"",
                    "version":version,"steps":steps,
                    "blocks":[{"id":"block","type":kind,"steps":[{"type":"query","id":"inside","sql":"SELECT 2"}]}]
                })).unwrap();
                let host = NoDispatch(AtomicUsize::new(0));
                let error =
                    WorkflowExecutor::execute(&definition, &host, None, &serde_json::json!({}))
                        .await
                        .unwrap_err();
                assert!(error.contains("CapabilityUnsupported"));
                assert_eq!(host.0.load(Ordering::SeqCst), 0);
            }
        }
    }
}
