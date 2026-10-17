use datazen_feature_core::workflow::executor::WorkflowExecutor;
use datazen_feature_core::workflow::{
    context::WorkflowContext, host::WorkflowHost, model::*, WorkflowCommandStep, WorkflowError,
};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Host(AtomicUsize);
static CLEANED_ACQUISITIONS: AtomicUsize = AtomicUsize::new(0);
struct CandidateGuard;
impl Drop for CandidateGuard {
    fn drop(&mut self) {
        CLEANED_ACQUISITIONS.fetch_add(1, Ordering::SeqCst);
    }
}
#[async_trait::async_trait]
impl WorkflowHost for Host {
    async fn open_session_block(
        &self,
        _: &datazen_feature_core::workflow::target_plan::ExecutionTarget,
        deadline: tokio::time::Instant,
    ) -> Result<datazen_feature_core::workflow::session_block::RetainedSession, WorkflowError> {
        assert!(
            deadline > tokio::time::Instant::now(),
            "deadline reaches trusted adapter"
        );
        let _candidate = CandidateGuard;
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        Err(WorkflowError::Validation(
            "synthetic admission rejected".into(),
        ))
    }

    async fn target_requirements(
        &self,
        connection: &str,
        _: &str,
    ) -> Result<datazen_feature_core::workflow::target_plan::TargetRequirements, WorkflowError>
    {
        Ok(
            datazen_feature_core::workflow::target_plan::TargetRequirements {
                requires_database: connection != "single-database",
            },
        )
    }
    async fn connection_name(&self, _: &str) -> Result<String, WorkflowError> {
        Ok("synthetic".into())
    }
    async fn execute_command(
        &self,
        _: &WorkflowCommandStep,
        _: Option<&str>,
        _: Option<datazen_feature_core::mcp::permission::McpPermissionMode>,
        _: Option<tokio::time::Instant>,
    ) -> Result<datazen_driver_api::CommandResult, WorkflowError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(datazen_driver_api::CommandResult::new(
            serde_json::json!({"results":[{"sql":"select 1","columns":[],"rows":[],"executionTimeMs":0}],"totalTimeMs":0}),
        ))
    }
    async fn complete_ai(&self, _: String) -> Result<String, WorkflowError> {
        panic!("unexpected AI dispatch")
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
        panic!("unexpected migration dispatch")
    }
}
fn definition(version: Option<&str>, kind: Option<&str>) -> WorkflowDefinition {
    let mut value = serde_json::json!({"id":"w", "name":"w", "description":"", "version":version, "connection":"synthetic", "database":"app", "steps":[{"type":"query","id":"independent","sql":"select 1"}]});
    if let Some(kind) = kind {
        value["blocks"] = serde_json::json!([{"id":"block", "type":kind,"steps":[{"type":"query","id":"first","sql":"create temporary table sample (id int)"},{"type":"query","id":"second","sql":"select * from sample"}]}]);
    }
    serde_json::from_value(value).expect("synthetic definition")
}
#[tokio::test]
async fn version_journey_never_silently_discards_resource_blocks() {
    let host = Host(AtomicUsize::new(0));
    for version in [None, Some("1"), Some("2")] {
        let result = WorkflowExecutor::execute(
            &definition(version, None),
            &host,
            None,
            &serde_json::json!({}),
        )
        .await
        .expect("independent step accepted");
        assert!(result.success);
    }
    assert_eq!(host.0.load(Ordering::SeqCst), 3);
    for kind in ["sessionBlock", "transactionBlock"] {
        let error = WorkflowExecutor::execute(
            &definition(Some("2"), Some(kind)),
            &host,
            None,
            &serde_json::json!({}),
        )
        .await
        .expect_err("block must be rejected before independent SQL");
        assert!(
            error.contains(if kind == "sessionBlock" {
                "WorkflowBlock Job owner"
            } else {
                "transactionBlock"
            }),
            "{error}"
        );
        assert_eq!(host.0.load(Ordering::SeqCst), 3);
    }
    let error = WorkflowExecutor::execute(
        &definition(None, Some("sessionBlock")),
        &host,
        None,
        &serde_json::json!({}),
    )
    .await
    .expect_err("v1 must reject blocks");
    assert!(error.contains("version '2'"));
    assert_eq!(host.0.load(Ordering::SeqCst), 3);
}
#[tokio::test]
async fn explicit_incomplete_or_conflicting_targets_never_dispatch() {
    let host = Host(AtomicUsize::new(0));
    let mut workflow = definition(Some("2"), Some("sessionBlock"));
    for target in [
        serde_json::json!({"connection":"other","database":"app"}),
        serde_json::json!({"connection":"synthetic"}),
        serde_json::json!({"database":"app"}),
    ] {
        let mut value = serde_json::to_value(&workflow).expect("encode");
        value["blocks"][0]["steps"][0]["connection"] = target
            .get("connection")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        value["blocks"][0]["steps"][0]["database"] = target
            .get("database")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        workflow = serde_json::from_value(value).expect("definition");
        assert!(
            WorkflowExecutor::execute(&workflow, &host, None, &serde_json::json!({}))
                .await
                .is_err()
        );
    }
    assert_eq!(host.0.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn v2_never_backfills_callers_connection_but_v1_remains_compatible() {
    let host = Host(AtomicUsize::new(0));
    let mut workflow = definition(Some("2"), None);
    workflow.connection = None;
    workflow.database = None;
    assert!(
        WorkflowExecutor::execute(&workflow, &host, Some("caller"), &serde_json::json!({}))
            .await
            .is_err()
    );
    assert_eq!(host.0.load(Ordering::SeqCst), 0);
    workflow.version = None;
    assert!(
        WorkflowExecutor::execute(&workflow, &host, Some("caller"), &serde_json::json!({}))
            .await
            .expect("v1 caller compatibility")
            .success
    );
    assert_eq!(host.0.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn single_database_sql_targets_follow_metadata_before_dispatch() {
    let host = Host(AtomicUsize::new(0));
    for command in ["query", "execute", "explain"] {
        for (connection, accepted) in [("single-database", true), ("synthetic", false)] {
            let mut workflow = definition(Some("2"), None);
            workflow.connection = Some(connection.into());
            workflow.database = None;
            if command != "query" {
                workflow.steps = vec![WorkflowStep::Command {
                    id: "command".into(),
                    command: command.into(),
                    connection: None,
                    input: serde_json::json!({"sql":"select 1"}),
                    timeout_secs: None,
                    on_error: None,
                }];
            }
            let before = host.0.load(Ordering::SeqCst);
            let result =
                WorkflowExecutor::execute(&workflow, &host, None, &serde_json::json!({})).await;
            if accepted {
                assert!(
                    result
                        .expect("single database metadata permits absent database")
                        .success
                );
            } else {
                assert!(result
                    .expect_err("multi database requires explicit database")
                    .contains("requires a database"));
            }
            assert_eq!(
                host.0.load(Ordering::SeqCst),
                before + usize::from(accepted)
            );
        }
    }
}

#[tokio::test]
async fn acquisition_timeout_cleans_candidate_preserves_unknown_and_dispatches_no_step() {
    use datazen_feature_core::workflow::{
        session_block::acquire_block,
        target_plan::{ExecutionTarget, TargetSource},
    };
    let host = Host(AtomicUsize::new(0));
    let target = ExecutionTarget {
        connection_id: "synthetic".into(),
        database: Some("app".into()),
        source: TargetSource::Block,
    };
    let cleaned = CLEANED_ACQUISITIONS.load(Ordering::SeqCst);
    let error = acquire_block(
        &host,
        &target,
        tokio::time::Instant::now() + std::time::Duration::from_millis(5),
    )
    .await
    .err()
    .expect("admission times out");
    assert!(
        matches!(error, WorkflowError::SqlUnconfirmed { state, effects, code, .. } if state == "Unknown" && effects == "Unknown" && code == "BlockAcquisitionTimeout")
    );
    assert_eq!(CLEANED_ACQUISITIONS.load(Ordering::SeqCst), cleaned + 1);
    assert_eq!(host.0.load(Ordering::SeqCst), 0);
    // The next block is a new admission and does not revive the timed-out one.
    let result = acquire_block(
        &host,
        &target,
        tokio::time::Instant::now() + std::time::Duration::from_millis(100),
    )
    .await;
    assert!(
        matches!(result, Err(WorkflowError::Validation(message)) if message == "synthetic admission rejected")
    );
    assert_eq!(CLEANED_ACQUISITIONS.load(Ordering::SeqCst), cleaned + 2);
    let expired = acquire_block(&host, &target, tokio::time::Instant::now()).await;
    assert!(expired.is_err());
    assert_eq!(
        CLEANED_ACQUISITIONS.load(Ordering::SeqCst),
        cleaned + 2,
        "expired run never starts admission"
    );
    assert_eq!(host.0.load(Ordering::SeqCst), 0);
}
