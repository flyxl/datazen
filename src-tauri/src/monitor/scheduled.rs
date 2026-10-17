//! Frozen Monitor execution with a backend-issued service context.
use super::grants;
use crate::{
    commands::AppState,
    workflow::{WorkflowCommandStep, WorkflowError},
};
use datazen_driver_api::{CommandResult, ConnectionConfig, DatabaseDriver};
use datazen_feature_core::{
    dashboard::{
        execute::WidgetExecutionHost,
        schedule_grant::MonitorGrant,
        types::{DashboardWidget, WidgetRun},
    },
    workflow::{
        context::WorkflowContext,
        executor::{WorkflowExecuteOptions, WorkflowExecutor},
        host::WorkflowHost,
        model::*,
        resource_execution::{PreparedExecution, ResourceExecutionHost},
    },
};
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::{
    context::OwnerRef,
    dto::{
        execution::EffectOutcome,
        job::{Checkpoint, JobDefinition, JobDomainResult, JobProgress, JobResultItem},
    },
    id::{IdempotencyKey, JobId, StageId, WorkerId},
};
use datazen_runtime::job::{
    CancelToken, EndpointRef, EndpointRole, JobError, JobHandler, RecoveryVerdict, StageOutcome,
    StageSpec, StageTerminal,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
pub(crate) async fn validate(state: &AppState, grant: &MonitorGrant) -> Result<(), String> {
    if grants::current(&state.store, &grant.context())? != *grant {
        return Err("monitor grant changed".into());
    }
    for scope in &grant.scopes {
        let (config, metadata) = state
            .store
            .platform_profile(scope.target.connection_id.as_str())
            .await
            .ok_or("monitor connection missing")?;
        if !metadata.enabled
            || metadata.config_revision != scope.config_revision
            || !matches!(config.database_type.as_str(), "postgresql" | "mysql")
        {
            return Err("monitor connection disabled or revision changed".into());
        }
    }
    Ok(())
}
struct ScheduledWorkflowHost<'a> {
    state: &'a AppState,
    grant: &'a MonitorGrant,
    context: RequestContext,
}
#[async_trait::async_trait]
impl ResourceExecutionHost for ScheduledWorkflowHost<'_> {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        validate(self.state, self.grant)
            .await
            .map_err(WorkflowError::Validation)?;
        ResourceExecutionHost::profile(self.state, id).await
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
        validate(self.state, self.grant)
            .await
            .map_err(WorkflowError::Validation)?;
        let adapter = self
            .state
            .platform_require()
            .map_err(|_| WorkflowError::Validation("platform unavailable".into()))?;
        let canonical = adapter
            .resolve_execution_target(
                &self.context,
                datazen_platform_api::id::Counter::new(revision),
                command,
                &target,
            )
            .await
            .map_err(|_| WorkflowError::Validation("scheduled target invalid".into()))?;
        let target = crate::platform::into_execution_target(canonical);
        if !self.grant.scopes.iter().any(|scope| {
            scope.target == target
                && scope.command == command
                && scope.target.connection_id.as_str() == id
                && scope.config_revision == revision
        }) {
            return Err(WorkflowError::Validation(
                "scheduled command outside grant scope".into(),
            ));
        }
        Ok(PreparedExecution {
            runtime: adapter
                .runtime()
                .map_err(|_| WorkflowError::Validation("runtime unavailable".into()))?,
            context: self.context.clone(),
            target,
        })
    }
}
#[async_trait::async_trait]
impl WorkflowHost for ScheduledWorkflowHost<'_> {
    fn resources(&self) -> Option<&dyn ResourceExecutionHost> {
        Some(self)
    }
    async fn connection_name(&self, id: &str) -> Result<String, WorkflowError> {
        WorkflowHost::connection_name(self.state, id).await
    }
    async fn execute_command(
        &self,
        step: &WorkflowCommandStep,
        connection: Option<&str>,
        _: Option<datazen_feature_core::mcp::permission::McpPermissionMode>,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError> {
        let id = step
            .effective_connection(connection)
            .ok_or_else(|| WorkflowError::Validation("scheduled target required".into()))?;
        datazen_feature_core::workflow::resource_execution::execute(
            self, step, id, None, deadline, true,
        )
        .await
    }
    async fn complete_ai(&self, _: String) -> Result<String, WorkflowError> {
        Err(WorkflowError::Validation("scheduled AI unsupported".into()))
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
            "scheduled migration unsupported".into(),
        ))
    }
}
struct ScheduledWidgetHost<'a> {
    state: &'a AppState,
    grant: &'a MonitorGrant,
    workflow: WorkflowDefinition,
}
#[async_trait::async_trait]
impl WidgetExecutionHost for ScheduledWidgetHost<'_> {
    async fn execute_workflow(&self, id: &str) -> Result<WorkflowExecutionResult, String> {
        if id != self.grant.workflow_id {
            return Err("scheduled workflow mismatch".into());
        }
        validate(self.state, self.grant).await?;
        let host = ScheduledWorkflowHost {
            state: self.state,
            grant: self.grant,
            context: self.grant.context(),
        };
        WorkflowExecutor::execute_with_options(
            &self.workflow,
            &host,
            None,
            &serde_json::json!({}),
            WorkflowExecuteOptions {
                permission_mode: None,
                ..Default::default()
            },
        )
        .await
    }
    async fn persist_run(&self, run: &WidgetRun) -> Result<(), String> {
        crate::dashboard::runs::write_run(
            &self.state.store.app_db(),
            run,
            &crate::dashboard::store::load_monitor_settings(&self.state.store.get_settings().await),
        )
        .map_err(|_| "monitor run persistence failed".into())
    }
}
pub(crate) async fn execute(
    state: &AppState,
    dashboard_id: &str,
    widget: &DashboardWidget,
    grant: &MonitorGrant,
) -> Result<WidgetRun, String> {
    validate(state, grant).await?;
    if dashboard_id != grant.dashboard_id
        || widget.id != grant.widget_id
        || widget.workflow_id != grant.workflow_id
    {
        return Err("scheduled widget mismatch".into());
    }
    let refresh_sec = widget
        .refresh
        .interval_secs()
        .ok_or("scheduled interval unavailable")?;
    let slot = chrono::Utc::now()
        .timestamp()
        .div_euclid(i64::from(refresh_sec));
    let context = grant.context();
    let requested_job_id = JobId::new(uuid::Uuid::new_v4().to_string());
    let idempotency_key = occurrence_idempotency_key(grant, slot);
    let payload = serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "monitorOccurrence": {
            "dashboardId": dashboard_id,
            "widgetId": widget.id,
            "workflowId": widget.workflow_id,
            "workflowRevision": grant.workflow_revision,
            "grantId": grant.id,
            "grantVersion": grant.revocation_version,
            "slot": slot,
            "scopes": grant.scopes,
        }
    });
    let definition = JobDefinition {
        job_id: requested_job_id.clone(),
        kind: "monitorOccurrence".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: context.client_instance_id.clone(),
            purpose: "monitorOccurrence".into(),
        },
        payload,
        created_at: state.desktop_job_host.now(),
    };
    let endpoints = occurrence_endpoints(state, grant).await?;
    state
        .desktop_job_host
        .ensure_endpoint_services(&endpoints)
        .map_err(|_| "monitor job budget unavailable")?;
    let accepted = state
        .desktop_job_host
        .accept(&context, definition, &idempotency_key)
        .await
        .map_err(|_| "monitor occurrence admission failed")?;
    if accepted.view.job_id != requested_job_id {
        // An idempotency receipt already exists for this scheduled slot. A
        // restart or duplicate tick must never turn it into another query.
        return Err("monitor occurrence already accepted".into());
    }

    let handler = Arc::new(MonitorOccurrenceHandler::new(
        Arc::new(state.clone()),
        dashboard_id.to_owned(),
        widget.clone(),
        grant.clone(),
    ));
    let dispatch = state
        .desktop_job_host
        .dispatch(
            &context,
            &accepted.view.job_id,
            &WorkerId::new("monitor-scheduler"),
            &endpoints,
            handler.clone(),
        )
        .await;
    match handler.take_run() {
        Some(run) => Ok(run),
        None => match dispatch {
            Ok(_) => Err("monitor occurrence has no run receipt".into()),
            Err(_) => Err("monitor occurrence dispatch failed".into()),
        },
    }
}

fn occurrence_idempotency_key(grant: &MonitorGrant, slot: i64) -> IdempotencyKey {
    let identity = format!(
        "{}:{}:{}:{}:{}:{}:{}",
        grant.id,
        grant.revocation_version,
        grant.dashboard_id,
        grant.widget_id,
        grant.workflow_id,
        grant.workflow_revision,
        slot,
    );
    IdempotencyKey::new(format!(
        "monitor-occurrence:{:x}",
        Sha256::digest(identity.as_bytes())
    ))
}

async fn occurrence_endpoints(
    state: &AppState,
    grant: &MonitorGrant,
) -> Result<Vec<EndpointRef>, String> {
    let mut by_connection = BTreeMap::new();
    for scope in &grant.scopes {
        let connection_id = scope.target.connection_id.as_str();
        if by_connection.contains_key(connection_id) {
            continue;
        }
        let (mut config, metadata) = state
            .store
            .platform_profile(connection_id)
            .await
            .ok_or("monitor connection missing")?;
        if !metadata.enabled || metadata.config_revision != scope.config_revision {
            return Err("monitor connection revision changed".into());
        }
        config.database = scope.target.namespace.database.clone().or(config.database);
        config.schema = scope.target.namespace.schema.clone().or(config.schema);
        let driver = state
            .driver_registry
            .get(&config.database_type)
            .await
            .ok_or("monitor driver unavailable")?;
        let identity =
            datazen_feature_core::services::migration_endpoint::identify(&config, driver.as_ref());
        by_connection.insert(
            connection_id.to_owned(),
            EndpointRef {
                connection_id: identity.connection_id,
                service_key: identity.service_key,
                objects: Vec::new(),
                role: EndpointRole::Control,
            },
        );
    }
    Ok(by_connection.into_values().collect())
}

async fn execute_widget_once(
    state: &AppState,
    dashboard_id: &str,
    widget: &DashboardWidget,
    grant: &MonitorGrant,
) -> Result<WidgetRun, String> {
    validate(state, grant).await?;
    if dashboard_id != grant.dashboard_id
        || widget.id != grant.widget_id
        || widget.workflow_id != grant.workflow_id
    {
        return Err("scheduled widget mismatch".into());
    }
    let record = state
        .store
        .app_db()
        .get_workflow(&grant.workflow_id)
        .map_err(|_| "workflow missing")?;
    if grants::revision(&record.definition_yaml) != grant.workflow_revision {
        return Err("scheduled workflow revision changed".into());
    }
    let workflow = serde_yaml::from_str(&record.definition_yaml).map_err(|_| "workflow invalid")?;
    datazen_feature_core::dashboard::execute::execute_widget_once(
        &ScheduledWidgetHost {
            state,
            grant,
            workflow,
        },
        dashboard_id,
        widget,
    )
    .await
}

struct MonitorOccurrenceHandler {
    state: Arc<AppState>,
    dashboard_id: String,
    widget: DashboardWidget,
    grant: MonitorGrant,
    run: Mutex<Option<WidgetRun>>,
}

impl MonitorOccurrenceHandler {
    fn new(
        state: Arc<AppState>,
        dashboard_id: String,
        widget: DashboardWidget,
        grant: MonitorGrant,
    ) -> Self {
        Self {
            state,
            dashboard_id,
            widget,
            grant,
            run: Mutex::new(None),
        }
    }

    fn take_run(&self) -> Option<WidgetRun> {
        self.run
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    fn remember_run(&self, run: WidgetRun) {
        *self.run.lock().unwrap_or_else(|error| error.into_inner()) = Some(run);
    }
}

#[async_trait::async_trait]
impl JobHandler for MonitorOccurrenceHandler {
    fn kind(&self) -> &str {
        "monitorOccurrence"
    }

    fn handler_version(&self) -> u64 {
        1
    }

    fn validate_plan(
        &self,
        plan: &datazen_runtime::job::FrozenPlan,
    ) -> Result<Vec<StageSpec>, JobError> {
        if plan.kind != self.kind()
            || plan.is_apply
            || plan.plan_version != 1
            || plan.handler_version != self.handler_version()
            || plan.checkpoint_version != 1
        {
            return Err(JobError::CapabilityUnsupported(
                "monitor occurrence plan is incompatible".into(),
            ));
        }
        Ok(vec![StageSpec {
            stage_id: StageId::new("monitorOccurrence"),
            kind: "scheduledReadOnlyQuery".into(),
            depends_on: Vec::new(),
        }])
    }

    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        if spec.stage_id.as_str() != "monitorOccurrence" {
            return Err(JobError::CapabilityUnsupported(
                "monitor occurrence stage is incompatible".into(),
            ));
        }
        if cancel.is_cancelled() {
            return Ok(StageOutcome {
                stage_id: spec.stage_id.clone(),
                terminal: StageTerminal::Cancelled,
                progress: JobProgress::default(),
                commit_boundaries: Vec::new(),
                execution_ids: Vec::new(),
                artifact_ids: Vec::new(),
                effect_outcome: EffectOutcome::NotStarted,
                error_code: None,
            });
        }

        let run = tokio::select! {
            biased;
            _ = wait_for_cancel(cancel) => {
                return Ok(StageOutcome {
                    stage_id: spec.stage_id.clone(),
                    terminal: StageTerminal::Unknown,
                    progress: JobProgress::default(),
                    commit_boundaries: Vec::new(),
                    execution_ids: Vec::new(),
                    artifact_ids: Vec::new(),
                    effect_outcome: EffectOutcome::Unknown,
                    error_code: None,
                });
            }
            result = execute_widget_once(&self.state, &self.dashboard_id, &self.widget, &self.grant) => result,
        };
        let (terminal, effect_outcome) = match &run {
            Ok(run)
                if run.status == datazen_feature_core::dashboard::types::WidgetRunStatus::Ok =>
            {
                (StageTerminal::Succeeded, EffectOutcome::Completed)
            }
            Ok(run)
                if run.status
                    == datazen_feature_core::dashboard::types::WidgetRunStatus::Timeout =>
            {
                (StageTerminal::Unknown, EffectOutcome::Unknown)
            }
            Ok(_) => (StageTerminal::Failed, EffectOutcome::NotStarted),
            Err(_) => (StageTerminal::Failed, EffectOutcome::NotStarted),
        };
        if let Ok(run) = run {
            self.remember_run(run);
        }
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal,
            progress: JobProgress::default(),
            commit_boundaries: Vec::new(),
            execution_ids: Vec::new(),
            artifact_ids: Vec::new(),
            effect_outcome,
            error_code: None,
        })
    }

    fn verify_recovery(&self, _checkpoint: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "monitorOccurrenceNeverReplaysAfterRestart".into(),
        }
    }

    fn durable_result(&self, outcome: &StageOutcome) -> Option<JobDomainResult> {
        let run = self
            .run
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .cloned()?;
        let status = match run.status {
            datazen_feature_core::dashboard::types::WidgetRunStatus::Ok => "ok",
            datazen_feature_core::dashboard::types::WidgetRunStatus::Error => "error",
            datazen_feature_core::dashboard::types::WidgetRunStatus::Timeout => "timeout",
        };
        Some(JobDomainResult {
            stage_id: outcome.stage_id.clone(),
            result_code: "monitorRun".into(),
            outcome_code: status.into(),
            counters: Vec::new(),
            items: vec![JobResultItem {
                item_id: run.id,
                outcome_code: status.into(),
                reason_code: None,
            }],
            artifact_ids: Vec::new(),
        })
    }
}

async fn wait_for_cancel(cancel: &CancelToken) {
    loop {
        if cancel.is_cancelled() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::occurrence_idempotency_key;
    use crate::monitor::grants::revision;
    use datazen_feature_core::dashboard::schedule_grant::MonitorGrant;

    fn grant() -> MonitorGrant {
        MonitorGrant {
            id: "grant-1".into(),
            dashboard_id: "dashboard-1".into(),
            widget_id: "widget-1".into(),
            workflow_id: "workflow-1".into(),
            workflow_revision: revision("id: workflow-1"),
            organization_id: "org-1".into(),
            service_principal: "monitor-service:grant-1".into(),
            approved_by: "user-1".into(),
            scopes: Vec::new(),
            actions: vec!["connect".into(), "query".into()],
            read_only: true,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            revocation_version: 1,
            revoked: false,
        }
    }

    #[test]
    fn occurrence_idempotency_is_stable_per_grant_revision_and_slot() {
        let original = grant();
        assert_eq!(
            occurrence_idempotency_key(&original, 42),
            occurrence_idempotency_key(&original, 42)
        );
        assert_ne!(
            occurrence_idempotency_key(&original, 42),
            occurrence_idempotency_key(&original, 43)
        );

        let mut revoked = original.clone();
        revoked.revocation_version += 1;
        assert_ne!(
            occurrence_idempotency_key(&original, 42),
            occurrence_idempotency_key(&revoked, 42)
        );

        let mut changed_workflow = original.clone();
        changed_workflow.workflow_revision = revision("id: changed");
        assert_ne!(
            occurrence_idempotency_key(&original, 42),
            occurrence_idempotency_key(&changed_workflow, 42)
        );
    }
}
