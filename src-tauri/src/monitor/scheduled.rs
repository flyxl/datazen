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
use std::sync::Arc;
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
