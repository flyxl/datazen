//! Acquisition deadline seam; production uses the fenced Workflow Job adapter.
use datazen_driver_api::{CommandResult, ConnectionConfig, DatabaseDriver};
use datazen_feature_core::workflow::{
    context::WorkflowContext,
    executor::WorkflowExecutor,
    host::WorkflowHost,
    model::*,
    resource_execution::{PreparedExecution, ResourceExecutionHost},
    session_block::RetainedSession,
    target_plan::{ExecutionTarget, TargetRequirements},
    WorkflowCommandStep, WorkflowError,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
struct Host {
    acquisitions: AtomicUsize,
    cleaned: AtomicUsize,
    dispatches: AtomicUsize,
}
struct Candidate<'a>(&'a AtomicUsize);
impl Drop for Candidate<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[async_trait::async_trait]
impl WorkflowHost for Host {
    fn resources(&self) -> Option<&dyn ResourceExecutionHost> {
        Some(self)
    }
    fn validate_session_block(&self) -> Result<(), WorkflowError> {
        Ok(())
    }
    async fn target_requirements(
        &self,
        _: &str,
        _: &str,
    ) -> Result<TargetRequirements, WorkflowError> {
        Ok(TargetRequirements::DATABASE)
    }
    async fn open_session_block(
        &self,
        _block_id: &str,
        _: &ExecutionTarget,
        deadline: tokio::time::Instant,
    ) -> Result<RetainedSession, WorkflowError> {
        assert!(deadline > tokio::time::Instant::now());
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        let _candidate = Candidate(&self.cleaned);
        std::future::pending().await
    }
    async fn connection_name(&self, _: &str) -> Result<String, WorkflowError> {
        panic!("must not dispatch a step")
    }
    async fn execute_command(
        &self,
        _: &WorkflowCommandStep,
        _: Option<&str>,
        _: Option<datazen_feature_core::mcp::permission::McpPermissionMode>,
        _: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError> {
        self.dispatches.fetch_add(1, Ordering::SeqCst);
        panic!("must not submit SQL")
    }
    async fn complete_ai(&self, _: String) -> Result<String, WorkflowError> {
        panic!("must not dispatch AI")
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
        panic!("must not dispatch migration")
    }
}
#[async_trait::async_trait]
impl ResourceExecutionHost for Host {
    async fn profile(&self, _: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        panic!("acquisition timeout must not execute commands")
    }
    async fn driver(&self, _: &ConnectionConfig) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        panic!("acquisition timeout must not execute commands")
    }
    async fn safe_mode(&self) -> bool {
        panic!("acquisition timeout must not execute commands")
    }
    async fn prepare(
        &self,
        _: &str,
        _: u64,
        _: &str,
        _: datazen_platform_api::target::ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        panic!("acquisition timeout must not execute commands")
    }
}
#[tokio::test]
async fn workflow_deadline_stops_block_acquisition_and_next_run_does_not_replay_it() {
    let host = Host {
        acquisitions: AtomicUsize::new(0),
        cleaned: AtomicUsize::new(0),
        dispatches: AtomicUsize::new(0),
    };
    let mut definition: WorkflowDefinition = serde_json::from_value(serde_json::json!({"id":"w", "name":"w", "description":"", "version":"2", "connection":"synthetic", "database":"app", "timeout_secs":1, "steps":[], "blocks":[{"id":"b", "type":"sessionBlock", "steps":[{"type":"query", "id":"first", "sql":"select 1"}, {"type":"query", "id":"second", "sql":"select 2"}]}]})).expect("synthetic workflow");
    let result = WorkflowExecutor::execute(&definition, &host, None, &serde_json::json!({}))
        .await
        .expect("execution report");
    assert!(!result.success);
    assert!(result.error.as_deref().is_some_and(
        |error| error.contains("BlockAcquisitionTimeout") && error.contains("Unknown")
    ));
    assert!(result.steps.is_empty());
    assert_eq!(host.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(host.cleaned.load(Ordering::SeqCst), 1);
    assert_eq!(host.dispatches.load(Ordering::SeqCst), 0);
    definition.timeout_secs = Some(0);
    assert!(
        WorkflowExecutor::execute(&definition, &host, None, &serde_json::json!({}))
            .await
            .is_err()
    );
    assert_eq!(
        host.acquisitions.load(Ordering::SeqCst),
        1,
        "expired next run cannot revive admission"
    );
    assert_eq!(host.cleaned.load(Ordering::SeqCst), 1);
}
