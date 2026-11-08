//! 共享刷新监控引擎的 server 接线（W8 刀3d-2；「双端调度」的 dashboard
//! 半边）。
//!
//! 桌面对照物：`datazen-desktop/src/monitor/engine.rs` 的
//! `DesktopMonitorHost` 与 `MonitorEngine` 包装。时钟/到期判定/在途守卫/
//! 并发信号量/工作流锁全部在共享内核 `monitor::application::MonitorEngine`；
//! 本件提供宿主绑定与生命周期：
//!
//! - [`ServerMonitorHost`]——`MonitorHost` 的 server 实现：
//!   - `dashboards`——v14 存储（宿主配置组织）；
//!   - `max_concurrent_queries`——共享设置面（读取失败回落 1——最保守
//!     并发，fail-closed）；
//!   - `execute_widget`（手工 tick）——**身份槽**语义：HTTP 面的
//!     `runDashboardWidget` 在域级互斥下先 `set_interactive_identity`
//!     再 tick（槽位只在互斥内读写，无竞态；调度路径不走此面）。槽位
//!     空缺时回落引擎自己的合成身份（`server-monitor-host`）——不借
//!     任何用户；
//!   - `scheduled_authorization`——widget 最新授权单 + `GrantFacts`
//!     事实核验（无单/撤销/过期/漂移一律拒绝——失败关闭）；
//!   - `execute_scheduled_widget`——定义 revision 复验 + 授权单服务
//!     身份/作用域宿主跑共享 `execute_widget_once` 编排（超时/表提取/
//!     行数上限/告警评估/错误运行全在共享层，不复制形状）+ 运行落账；
//!   - `deliver_run`——结构化审计日志（通知/webhook 与 SSE 事件面是
//!     登记在案的后续切片，不假装在场）。
//! - [`ServerMonitorScheduler`]——spawn/停车句柄：bootstrap 在全部消费者
//!   装配之后 `spawn`（共享引擎 `start`），graceful drain 完成后 `stop`
//!   （共享层 `MonitorEngine::stop`，W8 刀3d 补齐——两形态受益）。
//!
//! **arm-not-fire**（与刀3c workflow 调度器同语义）：引擎启动首刷只装
//! 载调度表不触发执行（重启不补跑每个 interval 槽位）；授权闸门失败
//! 关闭。**桌面 job-occurrence 幂等 receipts 不在本刀的 server 面**（模块
//! 头登记——与 workflow 调度器同形的直接执行）。
//!
//! **组织口径**：server v1 单组织（宿主配置组织）；多组织扩容随多租户
//! 立项再上桌（与 workflow 调度器同裁决）。

use std::sync::Arc;

use datazen::dashboard::execute::{execute_widget_once, WidgetExecutionHost};
use datazen::dashboard::repository::list_dashboards;
use datazen::dashboard::runs::write_run;
use datazen::dashboard::types::{Dashboard, DashboardWidget, MonitorSettings, WidgetRun};
use datazen::monitor::application::{MonitorEngine as CoreEngine, MonitorHost};
use datazen::monitor::domain::{revision, MonitorGrant};
use datazen::settings::SettingsRepository as _;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen::shared::scope::OrganizationScope;
use datazen::workflow::domain::model::WorkflowExecutionResult;
use datazen::workflow::executor::WorkflowExecutor;
use datazen::workflow::registry::WorkflowRegistry;

use crate::dashboard::execution::{GrantScopedHost, ServerWidgetExecutionHost};
use crate::dashboard::grants::GrantFacts;
use crate::execution::resource_host::{PrincipalScopedHost, ServerResourceExecutionHost};
use crate::storage::{
    ServerDashboardRuns, ServerDashboardStore, ServerMonitorGrants, ServerProfiles,
    ServerSettingsStore, ServerWorkflowStore,
};

/// `MonitorHost` 的 server 实现。
pub struct ServerMonitorHost {
    dashboards: ServerDashboardStore,
    runs: ServerDashboardRuns,
    grants: ServerMonitorGrants,
    workflows: ServerWorkflowStore,
    registry: Arc<WorkflowRegistry>,
    settings: Arc<ServerSettingsStore>,
    profiles: Arc<ServerProfiles>,
    /// 派发执行的原语宿主（裸 `ServerResourceExecutionHost`；交互路径包
    /// 请求主体、调度路径在 `prepare` 产出上覆写授权单服务身份）。
    resource_host: Arc<ServerResourceExecutionHost>,
    /// 手工 tick 的身份槽（域级互斥下读写；见模块头「身份槽」语义）。
    interactive_identity: tokio::sync::RwLock<Option<RequestContext>>,
    organization_id: String,
}

impl ServerMonitorHost {
    /// 构造（bootstrap 装配入口）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dashboards: ServerDashboardStore,
        runs: ServerDashboardRuns,
        grants: ServerMonitorGrants,
        workflows: ServerWorkflowStore,
        registry: Arc<WorkflowRegistry>,
        settings: Arc<ServerSettingsStore>,
        profiles: Arc<ServerProfiles>,
        resource_host: Arc<ServerResourceExecutionHost>,
        organization_id: &str,
    ) -> Self {
        Self {
            dashboards,
            runs,
            grants,
            workflows,
            registry,
            settings,
            profiles,
            resource_host,
            interactive_identity: tokio::sync::RwLock::new(None),
            organization_id: organization_id.to_owned(),
        }
    }

    /// 手工 tick 的身份注入（`runDashboardWidget` handler 在域级互斥内
    /// 调用；tick 结束 `clear_interactive_identity`）。
    pub async fn set_interactive_identity(&self, context: RequestContext) {
        *self.interactive_identity.write().await = Some(context);
    }

    /// 清空身份槽（tick 收尾；槽位空缺 = 合成身份回落）。
    pub async fn clear_interactive_identity(&self) {
        *self.interactive_identity.write().await = None;
    }

    /// 引擎自己的合成身份（只读面与手工回落的归因主体——不借请求身份
    /// 也不借桌面用户；与 `workflow::host::background_context` 同裁决）。
    fn host_context(&self) -> RequestContext {
        RequestContext::new(
            OrganizationId::new(self.organization_id.clone()),
            PrincipalId::new("server-monitor-host".to_owned()),
            None,
            ClientInstanceId::new("server-monitor-host".to_owned()),
            RequestId::new(format!("monitor-host-{}", uuid::Uuid::new_v4())),
            None,
        )
    }

    /// monitor 设置（读取失败按共享层缺省保留策略——设置面故障不阻断
    /// 运行落账）。
    async fn monitor_settings(&self) -> MonitorSettings {
        self.settings
            .read_settings()
            .await
            .map(|shared| shared.monitor)
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl MonitorHost for ServerMonitorHost {
    async fn dashboards(&self) -> Result<Vec<Dashboard>, String> {
        let scope = self.dashboards.scope();
        list_dashboards(&scope, &self.dashboards).map_err(|error| error.to_string())
    }

    async fn max_concurrent_queries(&self) -> usize {
        self.settings
            .read_settings()
            .await
            .map(|shared| shared.monitor.max_concurrent_queries.max(1) as usize)
            .unwrap_or(1)
    }

    async fn execute_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<WidgetRun, String> {
        // 身份槽（域级互斥下由 handler 设置；空缺回落合成身份）。
        let identity = self
            .interactive_identity
            .read()
            .await
            .clone()
            .unwrap_or_else(|| self.host_context());
        let scope = self.dashboards.scope();
        let host = ServerWidgetExecutionHost {
            execution: Arc::new(PrincipalScopedHost {
                inner: self.resource_host.clone(),
                context: identity,
            }),
            profiles: self.profiles.clone(),
            registry: self.registry.clone(),
            runs: &self.runs,
            settings: self.settings.clone(),
            scope: &scope,
        };
        execute_widget_once(&host, dashboard_id, widget).await
    }

    async fn scheduled_authorization(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
    ) -> Result<MonitorGrant, String> {
        let grant = self
            .grants
            .for_widget(dashboard_id, &widget.id)
            .map_err(|_| "monitor grant lookup failed".to_owned())?
            .ok_or("explicit schedule authorization required")?;
        let facts = GrantFacts {
            grants: self.grants.clone(),
            profiles: self.profiles.clone(),
        };
        facts.validate_current(&grant).await?;
        Ok(grant)
    }

    async fn execute_scheduled_widget(
        &self,
        dashboard_id: &str,
        widget: &DashboardWidget,
        grant: &MonitorGrant,
    ) -> Result<WidgetRun, String> {
        let scope = self.dashboards.scope();
        let host = ScheduledWidgetExecutionHost {
            host: self,
            grant: grant.clone(),
            scope: &scope,
        };
        execute_widget_once(&host, dashboard_id, widget).await
    }

    async fn deliver_run(&self, dashboard_id: &str, widget: &DashboardWidget, run: &WidgetRun) {
        // 结构化审计（通知/webhook 与 SSE 事件面是登记在案的后续切片）。
        tracing::info!(
            dashboard_id = %dashboard_id,
            widget_id = %widget.id,
            workflow_id = %widget.workflow_id,
            run_id = %run.id,
            status = ?run.status,
            row_count = run.row_count,
            alert_fired = ?run.alert_fired,
            "dashboard_widget_run_delivered"
        );
    }
}

/// 调度执行的 `WidgetExecutionHost`（每派发一个轻量值；借宿主的存储件）。
///
/// `execute_workflow`：定义 revision 复验（授权单冻结的工作流身份未漂移）
/// → 授权单服务身份/作用域宿主（`GrantScopedHost`——`prepare` 的准入闸
/// 与上下文覆写）跑共享 `WorkflowExecutor`。`persist_run`：v14 运行表
/// （保留策略随 monitor 设置）。
struct ScheduledWidgetExecutionHost<'a> {
    host: &'a ServerMonitorHost,
    grant: MonitorGrant,
    scope: &'a OrganizationScope,
}

#[async_trait::async_trait]
impl WidgetExecutionHost for ScheduledWidgetExecutionHost<'_> {
    async fn execute_workflow(&self, workflow_id: &str) -> Result<WorkflowExecutionResult, String> {
        // 定义 revision 复验（授权单冻结身份 vs 当下定义行）。
        let (definition_yaml, _) = self
            .host
            .workflows
            .raw_definition(workflow_id)
            .await
            .map_err(|_| "monitor workflow missing".to_owned())?
            .ok_or_else(|| "monitor workflow missing".to_owned())?;
        if revision(&definition_yaml) != self.grant.workflow_revision {
            return Err("monitor workflow revision changed".into());
        }
        let workflow = self
            .host
            .registry
            .get(self.scope, workflow_id)
            .await
            .ok_or_else(|| format!("Workflow '{workflow_id}' not found"))?;
        let grant_host = GrantScopedHost {
            inner: self.host.resource_host.clone(),
            facts: GrantFacts {
                grants: self.host.grants.clone(),
                profiles: self.host.profiles.clone(),
            },
            grant: self.grant.clone(),
        };
        let workflow_host = crate::workflow::host::ServerWorkflowHost::new(
            Arc::new(grant_host),
            self.host.profiles.clone(),
            &self.host.organization_id,
        );
        WorkflowExecutor::execute(&workflow, &workflow_host, None, &serde_json::json!({})).await
    }

    async fn persist_run(&self, run: &WidgetRun) -> Result<(), String> {
        write_run(
            self.scope,
            &self.host.runs,
            run,
            &self.host.monitor_settings().await,
        )
        .map_err(|error| error.to_string())
    }
}

/// server 监控引擎句柄：spawn 与停车的生命周期载体（镜像
/// `workflow::scheduler::ServerWorkflowScheduler` 的装配形态）。
#[derive(Clone)]
pub struct ServerMonitorScheduler {
    core: Arc<CoreEngine>,
    host: Arc<ServerMonitorHost>,
}

impl ServerMonitorScheduler {
    /// 绑定宿主并启动 tick 循环。
    pub fn spawn(host: Arc<ServerMonitorHost>) -> Self {
        let core = CoreEngine::new(host.clone());
        core.start();
        Self { core, host }
    }

    /// 请求停车（幂等）：下一个 tick 边界退出，在途派发自行收尾。
    pub fn stop(&self) {
        self.core.stop();
    }

    /// 引擎是否已停车（取消令牌视角）。
    pub fn is_stopped(&self) -> bool {
        self.core.is_stopped()
    }

    /// 宿主引用（`runDashboardWidget` 的域级互斥 + 身份注入用）。
    pub fn host(&self) -> &Arc<ServerMonitorHost> {
        &self.host
    }

    /// 共享引擎引用（`reload` 等维护面）。
    pub fn core(&self) -> &Arc<CoreEngine> {
        &self.core
    }
}
