//! Desktop scheduling adapter. The clock and in-flight lifecycle live in modules.
use super::schedule_grant::DesktopScheduleGrantReader;
use datazen::workflow::application::execution_identity::ScheduledExecutionIdentity;
use datazen::workflow::application::grant_reader::WorkflowScheduleGrantReader;
use datazen::workflow::scheduler::{WorkflowScheduleHost, WorkflowScheduler as CoreScheduler};
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
        // 授权单端口与宿主同源装配：派生器读同一 AppState 的既有存储
        // （WR-A4：未绑定授权单端口的调度器不派发——失败关闭）。
        let grants = Arc::new(DesktopScheduleGrantReader::new(app.clone()));
        start_core_on_desktop_runtime(&self.core, Arc::new(DesktopScheduleHost(app)), grants);
    }
}
// Setup is synchronous; only the desktop adapter enters its owned runtime.
fn start_core_on_desktop_runtime(
    core: &Arc<CoreScheduler>,
    host: Arc<dyn WorkflowScheduleHost>,
    grants: Arc<dyn WorkflowScheduleGrantReader>,
) {
    let runtime = tauri::async_runtime::handle();
    let _entered = runtime.inner().enter();
    core.start(host, grants);
}

struct DesktopScheduleHost(tauri::AppHandle);
#[async_trait::async_trait]
impl WorkflowScheduleHost for DesktopScheduleHost {
    async fn list_workflows(
        &self,
    ) -> Result<Vec<datazen::workflow::model::WorkflowDefinition>, String> {
        let state = self.0.state::<crate::commands::AppState>();
        // 组织作用域取宿主解析的那一份（调度器不在请求周期内，但宿主状态在手）。
        let scope = state.organization_scope().map_err(|e| e.to_string())?;
        state.workflow_registry.ensure_loaded(&scope).await?;
        Ok(state.workflow_registry.list_definitions(&scope).await)
    }
    async fn execute_workflow(
        &self,
        id: &str,
        identity: &ScheduledExecutionIdentity,
    ) -> Result<datazen::workflow::model::WorkflowExecutionResult, String> {
        let state = self.0.state::<crate::commands::AppState>();
        // Owner 归属核对（失败关闭）：调度执行上下文的组织必须等于宿主解析的
        // 组织作用域——服务身份只代表授权单冻结的那个组织，归属不一致时
        // 不落执行路径（与引擎侧授权闸门同一拒绝语义）。
        let scope = state.organization_scope().map_err(|e| e.to_string())?;
        if scope.organization_id().as_str() != identity.organization_id() {
            return Err(format!(
                "scheduled workflow '{id}' dispatch rejected: organization mismatch"
            ));
        }
        // 审计（桌面形态 = 本地日志，§10.1）：主体 kind=Service（事件 envelope
        // 由 modules `application::execution_identity` 提供，测试钉在
        // live_acceptance_tests 的 fake-driver 旅程里）。
        tracing::info!(
            workflow_id = %id,
            service_principal = %identity.service_principal(),
            organization = %identity.organization_id(),
            approved_by = %identity.approved_by(),
            "workflow_scheduled_dispatch"
        );
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
        ) -> Result<Vec<datazen::workflow::model::WorkflowDefinition>, String> {
            self.0.send(()).map_err(|error| error.to_string())?;
            Ok(Vec::new())
        }
        async fn execute_workflow(
            &self,
            _: &str,
            _identity: &ScheduledExecutionIdentity,
        ) -> Result<datazen::workflow::model::WorkflowExecutionResult, String> {
            Err("empty startup schedule must not execute workflows".into())
        }
    }
    /// 启动测试桩：空授权单源（空列表下调度器根本不会走到派发闸门）。
    struct NoGrants;
    #[async_trait::async_trait]
    impl WorkflowScheduleGrantReader for NoGrants {
        async fn schedule_grant(
            &self,
            _: &str,
        ) -> Result<
            Option<datazen::workflow::domain::grant::WorkflowScheduleGrant>,
            datazen::shared::error::PortError,
        > {
            Ok(None)
        }
    }
    #[test]
    fn sync_setup_starts_workflow_scheduler_without_ambient_tokio_context() {
        std::thread::spawn(|| {
            assert!(tokio::runtime::Handle::try_current().is_err());
            let (send, received) = mpsc::channel();
            let core = CoreScheduler::new();
            start_core_on_desktop_runtime(&core, Arc::new(StartupHost(send)), Arc::new(NoGrants));
            assert!(tokio::runtime::Handle::try_current().is_err());
            received
                .recv_timeout(Duration::from_secs(3))
                .expect("scheduler first poll ran on the desktop runtime");
        })
        .join()
        .expect("sync setup must not panic");
    }
}
