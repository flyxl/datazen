//! Desktop user-initiated Workflow Jobs. Frozen definitions remain process-local;
//! the durable plan contains only a digest and versioned recovery metadata.
use super::{WorkflowCommandStep, WorkflowError, WorkflowExecutor};
use crate::commands::AppState;
use datazen_driver_api::{CommandResult, ConnectionConfig, DatabaseDriver};
use datazen_feature_core::workflow::{
    context::WorkflowContext,
    host::WorkflowHost,
    model::*,
    resource_execution::{PreparedExecution, ResourceExecutionHost},
    session_block::RetainedSession,
};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::{
        execution::EffectOutcome,
        job::{Checkpoint, JobDefinition, JobProgress},
    },
    id::*,
};
use datazen_runtime::job::{
    CancelToken, FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageOwnerPermit,
    StageSpec, StageTerminal,
};
use std::{collections::BTreeMap, sync::Arc};

struct Host<'a> {
    state: &'a AppState,
    context: RequestContext,
    revisions: BTreeMap<String, u64>,
    permit: Option<StageOwnerPermit>,
    cancel: Option<CancelToken>,
    deadline: tokio::time::Instant,
}
impl Host<'_> {
    fn check(&self) -> Result<(), WorkflowError> {
        if self.cancel.as_ref().is_some_and(CancelToken::is_cancelled) {
            return Err(WorkflowError::Step("workflow Job cancelled".into()));
        }
        Ok(())
    }
}
#[async_trait::async_trait]
impl ResourceExecutionHost for Host<'_> {
    fn cancellation(&self) -> Option<Arc<std::sync::atomic::AtomicBool>> {
        self.cancel.as_ref().map(CancelToken::flag)
    }
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        self.check()?;
        if let Some(permit) = &self.permit {
            permit
                .verify_active()
                .await
                .map_err(|_| WorkflowError::Validation("Workflow Job claim unavailable".into()))?;
        }
        let result = ResourceExecutionHost::profile(self.state, id).await?;
        if !matches!(result.0.database_type.as_str(), "postgresql" | "mysql") {
            return Err(WorkflowError::Validation(
                "CapabilityUnsupported: Workflow Jobs support PostgreSQL/MySQL".into(),
            ));
        }
        if self
            .revisions
            .get(id)
            .is_some_and(|revision| *revision != result.1)
        {
            return Err(WorkflowError::Validation(
                "frozen Workflow profile revision changed".into(),
            ));
        }
        Ok(result)
    }
    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        ResourceExecutionHost::driver(self.state, config).await
    }
    async fn safe_mode(&self) -> bool {
        ResourceExecutionHost::safe_mode(self.state).await
    }
    async fn prepare(
        &self,
        id: &str,
        revision: u64,
        command: &str,
        target: datazen_platform_api::target::ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        self.check()?;
        let (_, current, enabled) = self.profile(id).await?;
        if current != revision || !enabled {
            return Err(WorkflowError::Validation(
                "frozen Workflow target unavailable".into(),
            ));
        }
        let platform = self
            .state
            .platform_require()
            .map_err(|_| WorkflowError::Validation("platform unavailable".into()))?;
        let canonical = platform
            .resolve_execution_target(&self.context, Counter::new(revision), command, &target)
            .await
            .map_err(|_| WorkflowError::Validation("Workflow target rejected".into()))?;
        Ok(PreparedExecution {
            runtime: platform
                .runtime()
                .map_err(|_| WorkflowError::Validation("runtime unavailable".into()))?,
            context: self.context.clone(),
            target: crate::platform::into_execution_target(canonical),
        })
    }
}
#[async_trait::async_trait]
impl WorkflowHost for Host<'_> {
    fn workflow_deadline(&self) -> Option<tokio::time::Instant> {
        Some(self.deadline)
    }
    fn validate_session_block(&self) -> Result<(), WorkflowError> {
        self.check()
    }
    fn resources(&self) -> Option<&dyn ResourceExecutionHost> {
        Some(self)
    }
    async fn connection_name(&self, id: &str) -> Result<String, WorkflowError> {
        Ok(self.profile(id).await?.0.name)
    }
    async fn open_session_block(
        &self,
        block: &str,
        target: &datazen_feature_core::workflow::target_plan::ExecutionTarget,
        deadline: tokio::time::Instant,
    ) -> Result<RetainedSession, WorkflowError> {
        self.check()?;
        let permit = self
            .permit
            .as_ref()
            .ok_or_else(|| WorkflowError::Validation("accepted Workflow stage required".into()))?;
        let (config, revision, enabled) = self.profile(&target.connection_id).await?;
        if !enabled {
            return Err(WorkflowError::Validation("connection disabled".into()));
        }
        let declared = datazen_platform_api::target::ExecutionTarget::new(
            ConnectionId::new(&target.connection_id),
            datazen_platform_api::target::NamespaceTarget::new(
                target.database.clone(),
                None,
                config.schema,
                vec![],
            ),
        );
        let prepared = self
            .prepare(&target.connection_id, revision, "query", declared)
            .await?;
        let lease = permit
            .open_workflow_block(
                prepared.runtime.clone(),
                BlockId::new(block),
                prepared.target.clone(),
                deadline,
            )
            .await
            .map_err(|error| {
                WorkflowError::Validation(format!("block admission: {:?}", error.code))
            })?;
        RetainedSession::from_job(prepared, lease)
    }
    async fn execute_command(
        &self,
        step: &WorkflowCommandStep,
        connection: Option<&str>,
        mode: Option<datazen_feature_core::mcp::permission::McpPermissionMode>,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError> {
        self.check()?;
        let id = step
            .connection
            .as_deref()
            .or(connection)
            .ok_or_else(|| WorkflowError::Validation("explicit target required".into()))?;
        let permit = self
            .permit
            .as_ref()
            .ok_or_else(|| WorkflowError::Validation("accepted Job stage required".into()))?;
        let (config, revision, enabled) = self.profile(id).await?;
        if !enabled {
            return Err(WorkflowError::Validation("connection disabled".into()));
        }
        let namespace = datazen_platform_api::target::NamespaceTarget::new(
            step.input
                .get("database")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .or(config.database),
            None,
            step.input
                .get("schema")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .or(config.schema),
            vec![],
        );
        let prepared = self
            .prepare(
                id,
                revision,
                &step.command,
                datazen_platform_api::target::ExecutionTarget::new(
                    ConnectionId::new(id),
                    namespace,
                ),
            )
            .await?;
        let lease = permit
            .open_job_session(
                prepared.runtime.clone(),
                prepared.target.clone(),
                deadline.unwrap_or(self.deadline).min(self.deadline),
            )
            .await
            .map_err(|e| WorkflowError::Validation(format!("Job session: {:?}", e.code)))?;
        let mut session = RetainedSession::from_job(prepared, lease)?;
        let result = datazen_feature_core::workflow::resource_execution::execute_with_session(
            self,
            step,
            id,
            mode,
            deadline,
            true,
            Some(&session),
        )
        .await;
        session.close().await?;
        result
    }
    async fn complete_ai(&self, _: String) -> Result<String, WorkflowError> {
        Err(WorkflowError::Validation(
            "AI unsupported in Workflow Job slice".into(),
        ))
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
        Err(WorkflowError::Validation(
            "migration unsupported in Workflow Job slice".into(),
        ))
    }
}
struct Handler {
    state: AppState,
    definition: WorkflowDefinition,
    variables: serde_json::Value,
    context: RequestContext,
    revisions: BTreeMap<String, u64>,
    deadline: tokio::time::Instant,
    result: tokio::sync::Mutex<Option<Result<WorkflowExecutionResult, String>>>,
}
#[async_trait::async_trait]
impl JobHandler for Handler {
    fn kind(&self) -> &str {
        "workflowRun"
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, _: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        Ok(vec![StageSpec {
            stage_id: StageId::new("workflow"),
            kind: "workflowRun".into(),
            depends_on: vec![],
        }])
    }
    async fn run_stage(&self, _: &StageSpec, _: &CancelToken) -> Result<StageOutcome, JobError> {
        Err(JobError::StaleClaim)
    }
    async fn run_stage_authorized(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
        owner: StageOwnerPermit,
    ) -> Result<StageOutcome, JobError> {
        let host = Host {
            state: &self.state,
            context: self.context.clone(),
            revisions: self.revisions.clone(),
            permit: Some(owner),
            cancel: Some(cancel.clone()),
            deadline: self.deadline,
        };
        let result =
            WorkflowExecutor::execute(&self.definition, &host, None, &self.variables).await;
        let success = result.as_ref().is_ok_and(|result| result.success);
        // A failed workflow may already have committed independent writes. Never
        // infer rollback from an error string or a cancellation request.
        let terminal = if success {
            StageTerminal::Succeeded
        } else {
            StageTerminal::Unknown
        };
        *self.result.lock().await = Some(result);
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal,
            progress: JobProgress::default(),
            commit_boundaries: vec![],
            execution_ids: vec![],
            artifact_ids: vec![],
            effect_outcome: if success {
                EffectOutcome::Completed
            } else {
                EffectOutcome::Unknown
            },
            error_code: None,
        })
    }
    fn verify_recovery(&self, _: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "Workflow resource state cannot be reconstructed".into(),
        }
    }
}

pub(crate) async fn execute(
    state: &AppState,
    definition: &WorkflowDefinition,
    variables: &serde_json::Value,
) -> Result<WorkflowExecutionResult, String> {
    let platform = state
        .platform_require()
        .map_err(|_| "platform unavailable")?;
    let ctx = platform.begin_call().context().clone();
    platform
        .runtime()
        .map_err(|_| "runtime unavailable")?
        .authorize_workflow_job(&ctx)
        .await
        .map_err(|_| "Workflow Job authorization denied")?;
    let mut context = WorkflowContext::new(variables);
    context.set_builtin_variables();
    for var in &definition.variables {
        if !context.variables.contains_key(&var.name) {
            if let Some(default) = &var.default {
                context.variables.insert(
                    var.name.clone(),
                    default
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| default.to_string()),
                );
            }
        }
    }
    for variable in &definition.variables {
        if variable.required.unwrap_or(false)
            && context
                .variables
                .get(&variable.name)
                .map_or(true, |value| value.is_empty())
        {
            return Err("required Workflow variable missing".into());
        }
    }
    fn supported(steps: &[WorkflowStep]) -> bool {
        steps.iter().all(|step| match step {
            WorkflowStep::Query { .. } => true,
            WorkflowStep::Command { command, .. } => {
                matches!(command.as_str(), "query" | "execute" | "explain")
            }
            WorkflowStep::Condition {
                then_steps,
                else_steps,
                ..
            } => {
                supported(then_steps) && else_steps.as_ref().map_or(true, |steps| supported(steps))
            }
            WorkflowStep::ForEach { steps, .. } => supported(steps),
            _ => false,
        })
    }
    if !supported(&definition.steps)
        || definition
            .blocks
            .iter()
            .any(|block| !supported(&block.steps))
    {
        return Err(
            "CapabilityUnsupported: this Workflow Job slice accepts SQL and control steps only"
                .into(),
        );
    }
    let host = Host {
        state,
        context: ctx.clone(),
        revisions: BTreeMap::new(),
        permit: None,
        cancel: None,
        deadline: tokio::time::Instant::now()
            + std::time::Duration::from_secs(definition.timeout_secs.unwrap_or(300)),
    };
    let deadline = host.deadline;
    let frozen = datazen_feature_core::workflow::session_block::prepare_v2(
        definition, &context, &host, deadline,
    )
    .await
    .map_err(|e| e.to_string())?;
    let executable = frozen
        .clone()
        .into_executable()
        .map_err(|e| e.to_string())?;
    datazen_feature_core::workflow::session_block::plan_blocks(&executable, &host)
        .map_err(|e| e.to_string())?;
    // Freeze every referenced profile, including independent steps, before Job admission.
    let mut revisions = BTreeMap::new();
    fn connections(steps: &[WorkflowStep], ids: &mut std::collections::BTreeSet<String>) {
        for step in steps {
            match step {
                WorkflowStep::Query {
                    connection: Some(id),
                    ..
                }
                | WorkflowStep::Command {
                    connection: Some(id),
                    ..
                } => {
                    ids.insert(id.clone());
                }
                WorkflowStep::Condition {
                    then_steps,
                    else_steps,
                    ..
                } => {
                    connections(then_steps, ids);
                    if let Some(steps) = else_steps {
                        connections(steps, ids)
                    }
                }
                WorkflowStep::ForEach { steps, .. } => connections(steps, ids),
                _ => {}
            }
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    connections(&frozen.steps, &mut ids);
    for block in &frozen.blocks {
        connections(&block.steps, &mut ids);
        if let Some(id) = &block.connection {
            ids.insert(id.clone());
        }
    }
    if let Some(id) = &frozen.connection {
        ids.insert(id.clone());
    }
    for id in ids {
        let (_, revision, enabled) = host.profile(&id).await.map_err(|e| e.to_string())?;
        if !enabled {
            return Err("Workflow connection disabled".into());
        }
        revisions.insert(id, revision);
    }
    use sha2::{Digest, Sha256};
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&frozen).map_err(|_| "Workflow freeze failed")?)
    );
    let id = JobId::new(uuid::Uuid::new_v4().to_string());
    let job = JobDefinition {
        job_id: id.clone(),
        kind: "workflowRun".into(),
        owner: OwnerRef::Job {
            job_id: id.clone(),
            stage_id: StageId::new("workflow"),
        },
        payload: serde_json::json!({"kind":"workflowRun","planVersion":1,"handlerVersion":1,"checkpointVersion":1,"planDigest":digest}),
        created_at: state.desktop_job_host.now(),
    };
    state
        .desktop_job_host
        .accept(
            &ctx,
            job,
            &IdempotencyKey::new(uuid::Uuid::new_v4().to_string()),
        )
        .await
        .map_err(|_| "Workflow Job admission failed")?;
    let handler = Arc::new(Handler {
        state: state.clone(),
        definition: frozen,
        variables: variables.clone(),
        context: ctx.clone(),
        revisions,
        deadline,
        result: tokio::sync::Mutex::new(None),
    });
    state
        .desktop_job_host
        .dispatch(
            &ctx,
            &id,
            &WorkerId::new(format!("workflow-{}", id.as_str())),
            &[],
            handler.clone(),
        )
        .await
        .map_err(|_| "Workflow Job outcome requires verification")?;
    let result = handler
        .result
        .lock()
        .await
        .take()
        .ok_or_else(|| "Workflow Job did not dispatch".to_string())?;
    result
}
