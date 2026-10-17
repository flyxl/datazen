//! Desktop scheduling adapter. The clock and in-flight lifecycle live in feature-core.
pub use datazen_feature_core::workflow::scheduler::{
    clamp_interval_secs, due_decision, is_workflow_due, scheduled_interval_secs, DueDecision,
    MIN_INTERVAL_SECS,
};
use datazen_feature_core::workflow::scheduler::{
    WorkflowScheduleHost, WorkflowScheduler as CoreScheduler,
};
use std::sync::Arc;
use tauri::Manager;
pub struct WorkflowScheduler {
    core: Arc<CoreScheduler>,
}
impl WorkflowScheduler {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            core: CoreScheduler::new(),
        })
    }
    pub fn start(self: &Arc<Self>, app: tauri::AppHandle) {
        self.core.start(Arc::new(DesktopScheduleHost(app)));
    }
}
struct DesktopScheduleHost(tauri::AppHandle);
#[async_trait::async_trait]
impl WorkflowScheduleHost for DesktopScheduleHost {
    async fn list_workflows(&self) -> Result<Vec<super::model::WorkflowDefinition>, String> {
        let state = self.0.state::<crate::commands::AppState>();
        state.workflow_registry.ensure_loaded().await?;
        Ok(state.workflow_registry.list_definitions().await)
    }
    async fn execute_workflow(
        &self,
        id: &str,
    ) -> Result<super::model::WorkflowExecutionResult, String> {
        let state = self.0.state::<crate::commands::AppState>();
        crate::commands::workflow_execute_impl(
            state.inner(),
            id.into(),
            serde_json::json!({}),
            None,
        )
        .await
        .map_err(|error| error.to_string())
    }
}
