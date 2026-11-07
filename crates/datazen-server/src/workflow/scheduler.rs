//! 核心工作流调度器的 server 接线（W8 刀3c-2）。
//!
//! 桌面对照物：`datazen-desktop/src/workflow/scheduler.rs` 的
//! `WorkflowScheduler` + `DesktopScheduleHost`。时钟/在途守卫/授权闸门
//! （上膛冻结 → 触发比对）全部在共享内核 `engine::scheduler`；本件只提供
//! 宿主绑定与生命周期：
//!
//! - [`ServerScheduleHost`]——`WorkflowScheduleHost` 的 server 实现：
//!   `list_workflows` 按宿主配置的组织列 user-可见定义（dashboardHidden
//!   是仪表板私有面，不进调度）；`execute_workflow` 做 owner 归属核对
//!   （授权单的组织 = 宿主配置的组织，失败关闭）后经
//!   `WorkflowExecutor` 以**服务身份上下文**派发（`PrincipalScopedHost`
//!   机制：`prepare` 产出的执行上下文覆写为授权单派生的
//!   `service_principal`，审计主体 kind=Service、无委托——比桌面形态
//!   更进一步，桌面沿用本地平台调用上下文是它的形态限制）。
//! - [`ServerWorkflowScheduler`]——spawn/停车句柄：bootstrap 在全部
//!   消费者装配之后 `spawn`（`core.start`），`axum::serve` 的 graceful
//!   drain 完成后 `stop()`（共享层 `WorkflowScheduler::stop`，幂等）。
//!
//! **arm-not-fire**：核心调度器首次观察某定时工作流只上膛不触发（重启
//! 不补跑每个备份任务）；授权单在上膛时冻结。最小间隔 30s（
//! `clamp_interval_secs`）。
//!
//! **组织口径**：server v1 单组织（`config.organization_id`）——调度器
//! 扫描宿主配置的组织（与桌面单组织同构）；多组织扩容时调度器的
//! id 命名空间裁决（跨组织同名 id 的键隔离）随多租户立项再上桌。

use std::sync::Arc;

use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen::shared::scope::OrganizationScope;
use datazen::workflow::application::execution_identity::ScheduledExecutionIdentity;
use datazen::workflow::domain::history::new_run_id;
use datazen::workflow::domain::model::{
    WorkflowDefinition, WorkflowExecutionResult, WorkflowVisibility,
};
use datazen::workflow::engine::scheduler::{
    WorkflowScheduleHost, WorkflowScheduler as CoreScheduler,
};
use datazen::workflow::executor::WorkflowExecutor;
use datazen::workflow::registry::WorkflowRegistry;

use crate::execution::resource_host::PrincipalScopedHost;
use crate::storage::{ServerProfiles, ServerWorkflowHistoryStore};
use crate::workflow::host::ServerWorkflowHost;

/// `WorkflowScheduleHost` 的 server 实现。
pub struct ServerScheduleHost {
    registry: Arc<WorkflowRegistry>,
    history: ServerWorkflowHistoryStore,
    /// 派发执行的原语宿主（裸 `ServerResourceExecutionHost`；每次派发
    /// 包一层服务身份的 `PrincipalScopedHost`）。
    execution: Arc<dyn datazen::workflow::resource_execution::ResourceExecutionHost>,
    profiles: Arc<ServerProfiles>,
    organization_id: String,
}

impl ServerScheduleHost {
    /// 构造（bootstrap 装配入口）。
    pub fn new(
        registry: Arc<WorkflowRegistry>,
        history: ServerWorkflowHistoryStore,
        execution: Arc<dyn datazen::workflow::resource_execution::ResourceExecutionHost>,
        profiles: Arc<ServerProfiles>,
        organization_id: &str,
    ) -> Self {
        Self {
            registry,
            history,
            execution,
            profiles,
            organization_id: organization_id.to_owned(),
        }
    }

    fn scope(&self) -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new(self.organization_id.clone()))
    }

    /// 授权单派生的服务身份 → 执行上下文（`PrincipalScopedHost` 的覆写
    /// 值；owner 归属授权单组织、审计主体 = service_principal、无委托）。
    fn service_context(identity: &ScheduledExecutionIdentity) -> RequestContext {
        RequestContext::new(
            OrganizationId::new(identity.organization_id().to_owned()),
            PrincipalId::new(identity.service_principal().to_owned()),
            None,
            ClientInstanceId::new("server-workflow-scheduler".to_owned()),
            RequestId::new(format!("workflow-schedule-{}", uuid::Uuid::new_v4())),
            None,
        )
    }
}

#[async_trait::async_trait]
impl WorkflowScheduleHost for ServerScheduleHost {
    async fn list_workflows(&self) -> Result<Vec<WorkflowDefinition>, String> {
        // 调度器不在请求周期内：组织作用域取宿主配置的那一份。
        let scope = self.scope();
        self.registry.ensure_loaded(&scope).await?;
        Ok(self.registry.list_definitions(&scope).await)
    }

    async fn execute_workflow(
        &self,
        id: &str,
        identity: &ScheduledExecutionIdentity,
    ) -> Result<WorkflowExecutionResult, String> {
        // Owner 归属核对（失败关闭）：调度执行上下文的组织必须等于宿主
        // 配置的组织——服务身份只代表授权单冻结的那个组织，归属不一致
        // 不落执行路径（桌面同款拒绝语义）。
        if self.organization_id != identity.organization_id() {
            return Err(format!(
                "scheduled workflow '{id}' dispatch rejected: organization mismatch"
            ));
        }
        let scope = self.scope();
        let workflow = self
            .registry
            .get(&scope, id)
            .await
            .ok_or_else(|| format!("Workflow '{id}' not found"))?;

        // 审计（server 形态 = 结构化日志；事件面随 WR-G 切片接入）。
        tracing::info!(
            workflow_id = %id,
            service_principal = %identity.service_principal(),
            organization = %identity.organization_id(),
            approved_by = %identity.approved_by(),
            "workflow_scheduled_dispatch"
        );

        // 服务身份上下文的执行宿主（每派发一个轻量包装值）。
        let host = ServerWorkflowHost::new(
            Arc::new(PrincipalScopedHost {
                inner: self.execution.clone(),
                context: Self::service_context(identity),
            }),
            self.profiles.clone(),
            &self.organization_id,
        );
        let result = WorkflowExecutor::execute(&workflow, &host, None, &serde_json::json!({}))
            .await
            .map_err(|error| error)?;

        // 运行历史落账：DashboardHidden 不进用户面（桌面同语义）；
        // best-effort——落账失败不阻断执行应答。
        if workflow.visibility != WorkflowVisibility::DashboardHidden {
            let now = chrono::Local::now();
            let run_id = new_run_id(now);
            if let Err(error) = self
                .history
                .record(
                    &run_id,
                    &workflow.id,
                    &workflow.name,
                    &serde_json::json!({}),
                    &result,
                    &now.to_rfc3339(),
                )
                .await
            {
                tracing::warn!("Failed to record workflow history: {error}");
            }
        }

        Ok(result)
    }
}

/// server 调度器句柄：spawn 与停车的生命周期载体（镜像
/// `execution::idle_reclaimer::ServerIdleReclaimer` 的装配形态）。
pub struct ServerWorkflowScheduler {
    core: Arc<CoreScheduler>,
}

impl ServerWorkflowScheduler {
    /// 绑定宿主与授权单端口并启动 tick 循环。
    pub fn spawn(
        host: Arc<dyn WorkflowScheduleHost>,
        grants: Arc<dyn datazen::workflow::application::grant_reader::WorkflowScheduleGrantReader>,
    ) -> Self {
        let core = CoreScheduler::new();
        core.start(host, grants);
        Self { core }
    }

    /// 请求停车（幂等）：下一个 tick 边界退出，在途派发自行收尾。
    pub fn stop(&self) {
        self.core.stop();
    }

    /// 调度器是否已停车（取消令牌视角）。
    pub fn is_stopped(&self) -> bool {
        self.core.is_stopped()
    }
}
