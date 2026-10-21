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
        start_core_on_desktop_runtime(&self.core, Arc::new(DesktopScheduleHost(app)));
    }
}
// Setup is synchronous; only the desktop adapter enters its owned runtime.
fn start_core_on_desktop_runtime(core: &Arc<CoreScheduler>, host: Arc<dyn WorkflowScheduleHost>) {
    let runtime = tauri::async_runtime::handle();
    let _entered = runtime.inner().enter();
    core.start(host);
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

#[cfg(test)]
mod startup_tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    struct StartupHost(mpsc::Sender<()>);
    #[async_trait::async_trait]
    impl WorkflowScheduleHost for StartupHost {
        async fn list_workflows(
            &self,
        ) -> Result<Vec<super::super::model::WorkflowDefinition>, String> {
            self.0.send(()).map_err(|error| error.to_string())?;
            Ok(Vec::new())
        }
        async fn execute_workflow(
            &self,
            _: &str,
        ) -> Result<super::super::model::WorkflowExecutionResult, String> {
            Err("empty startup schedule must not execute workflows".into())
        }
    }
    #[test]
    fn sync_setup_starts_workflow_scheduler_without_ambient_tokio_context() {
        std::thread::spawn(|| {
            assert!(tokio::runtime::Handle::try_current().is_err());
            let (send, received) = mpsc::channel();
            let core = CoreScheduler::new();
            start_core_on_desktop_runtime(&core, Arc::new(StartupHost(send)));
            assert!(tokio::runtime::Handle::try_current().is_err());
            received
                .recv_timeout(Duration::from_secs(3))
                .expect("scheduler first poll ran on the desktop runtime");
        })
        .join()
        .expect("sync setup must not panic");
    }
}
