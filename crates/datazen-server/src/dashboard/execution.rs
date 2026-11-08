//! Widget 执行与授权单作用域执行的 server 绑定（W8 刀3d-2）。
//!
//! 桌面对照物：`datazen-desktop/src/dashboard/execute.rs` 的
//! `DesktopWidgetHost` 与 `datazen-desktop/src/monitor/scheduled.rs` 的
//! `ScheduledWorkflowHost`。执行编排/超时/错误运行/告警评估的规则全部
//! 在共享层 `dashboard::execute::execute_widget_once`；本件供两台宿主：
//!
//! - [`ServerWidgetExecutionHost`]——交互路径（`runDashboardWidget` /
//!   调度引擎的手工 tick）：`execute_workflow` 经 `ServerWorkflowHost`
//!   跑共享 `WorkflowExecutor`（执行原语 = 身份包装过的
//!   `ResourceExecutionHost`；工作流定义取自共享 registry——
//!   `DashboardHidden` 私有面可见，那是组件的载体，桌面同语义），
//!   `persist_run` 落 v14 运行表（保留策略随 monitor 设置）。
//! - [`GrantScopedHost`]——调度路径：`ResourceExecutionHost` 的授权单
//!   作用域装饰器。`profile`/`prepare` 先过授权单核验（最新授权单一致
//!   + profile 启用 + 版本冻结比对），`prepare` 产出后再核**规范化目标
//!   落在授权单 scope 内**，最后把执行上下文覆写为授权单派生的服务
//!   身份（`grant.context()`——审计归因不落请求主体，桌面同款）。

use std::sync::Arc;

use datazen::dashboard::execute::WidgetExecutionHost;
use datazen::dashboard::runs::write_run;
use datazen::dashboard::types::{MonitorSettings, WidgetRun};
use datazen::monitor::domain::MonitorGrant;
use datazen::settings::SettingsRepository as _;
use datazen::shared::context::RequestContext;
use datazen::shared::scope::OrganizationScope;
use datazen::workflow::domain::model::WorkflowExecutionResult;
use datazen::workflow::executor::WorkflowExecutor;
use datazen::workflow::registry::WorkflowRegistry;
use datazen::workflow::resource_execution::{PreparedExecution, ResourceExecutionHost};
use datazen_driver_api::ConnectionConfig;

use crate::dashboard::grants::GrantFacts;
use crate::execution::resource_host::PrincipalScopedHost;
use crate::storage::{ServerDashboardRuns, ServerProfiles, ServerSettingsStore};
use crate::workflow::host::ServerWorkflowHost;

/// 交互执行宿主（`runDashboardWidget` 与引擎手工 tick 共用；每请求构造）。
pub struct ServerWidgetExecutionHost<'a> {
    /// 执行原语（身份由调用方包装：HTTP 面 = 请求真主体）。
    pub execution: Arc<dyn ResourceExecutionHost>,
    pub profiles: Arc<ServerProfiles>,
    pub registry: Arc<WorkflowRegistry>,
    pub runs: &'a ServerDashboardRuns,
    pub settings: Arc<ServerSettingsStore>,
    pub scope: &'a OrganizationScope,
}

#[async_trait::async_trait]
impl WidgetExecutionHost for ServerWidgetExecutionHost<'_> {
    async fn execute_workflow(&self, workflow_id: &str) -> Result<WorkflowExecutionResult, String> {
        let workflow = self
            .registry
            .get(self.scope, workflow_id)
            .await
            .ok_or_else(|| format!("Workflow '{workflow_id}' not found"))?;
        let host = ServerWorkflowHost::new(
            self.execution.clone(),
            self.profiles.clone(),
            self.scope.organization_id().as_str(),
        );
        WorkflowExecutor::execute(&workflow, &host, None, &serde_json::json!({})).await
    }

    async fn persist_run(&self, run: &WidgetRun) -> Result<(), String> {
        write_run(self.scope, self.runs, run, &self.monitor_settings().await)
            .map_err(|error| error.to_string())
    }
}

impl ServerWidgetExecutionHost<'_> {
    /// monitor 设置（读取失败按共享层缺省保留策略——设置面故障不阻断
    /// 运行落账，桌面 `get_settings` 同语义）。
    async fn monitor_settings(&self) -> MonitorSettings {
        self.settings
            .read_settings()
            .await
            .map(|shared| shared.monitor)
            .unwrap_or_default()
    }
}

/// 授权单作用域的执行宿主（调度路径）。
///
/// 每次调度派发构造一个（grant 是当次派发冻结的事实；`facts` 提供最新
/// 授权单核验——上膛冻结 → 触发比对，桌面 `scheduled::validate` 同语义）。
pub struct GrantScopedHost {
    /// 执行原语（裸 `ServerResourceExecutionHost`——身份在 `prepare`
    /// 产出上覆写为授权单服务身份）。
    pub inner: Arc<dyn ResourceExecutionHost>,
    /// 授权单事实核验（最新性/启用/版本）。
    pub facts: GrantFacts,
    /// 冻结的授权单。
    pub grant: MonitorGrant,
}

impl GrantScopedHost {
    /// 授权单核验（profile/prepare 两闸共用）。
    async fn validate(&self) -> Result<(), datazen::workflow::domain::error::WorkflowError> {
        self.facts
            .validate_current(&self.grant)
            .await
            .map_err(datazen::workflow::domain::error::WorkflowError::Validation)
    }
}

#[async_trait::async_trait]
impl ResourceExecutionHost for GrantScopedHost {
    async fn profile(
        &self,
        id: &str,
    ) -> Result<(ConnectionConfig, u64, bool), datazen::workflow::domain::error::WorkflowError>
    {
        self.validate().await?;
        self.inner.profile(id).await
    }

    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<
        Arc<dyn datazen_driver_api::DatabaseDriver>,
        datazen::workflow::domain::error::WorkflowError,
    > {
        self.inner.driver(config).await
    }

    async fn safe_mode(&self) -> bool {
        // monitor 是只读执行：安全档位随内层（内层读取失败按最严档），
        // 授权单自身已强制 read_only。
        self.inner.safe_mode().await
    }

    async fn prepare(
        &self,
        id: &str,
        revision: u64,
        command: &str,
        target: datazen::execution::target::ExecutionTarget,
    ) -> Result<PreparedExecution, datazen::workflow::domain::error::WorkflowError> {
        self.validate().await?;
        let prepared = self.inner.prepare(id, revision, command, target).await?;
        // 规范化目标必须落在授权单 scope 内（连接 + 命令 + 版本冻结；
        // 桌面 `ScheduledWorkflowHost::prepare` 同款三重比对）。
        if self.grant.scope(&prepared.target, Some(command)).is_none() {
            return Err(datazen::workflow::domain::error::WorkflowError::Validation(
                "scheduled command outside grant scope".into(),
            ));
        }
        // 执行上下文 = 授权单服务身份 + **按目标连接细分的委托 id**
        // （`"{grant_id}:{connection}"`——服务身份委托行按连接分行，
        // `authorize_target` 的委托分支按行内核对目标；见存储件
        // `materialize_service_identity` 的命名裁决）。
        let mut context: RequestContext = self.grant.context();
        context.delegation_id = Some(datazen::shared::id::DelegationId::new(format!(
            "{}:{}",
            self.grant.id,
            prepared.target.connection_id.as_str()
        )));
        Ok(PreparedExecution {
            context,
            ..prepared
        })
    }
}

/// 身份包装宿主的便捷构造（交互路径；dashboard 域路由用）。
pub fn principal_scoped(
    inner: Arc<dyn ResourceExecutionHost>,
    context: RequestContext,
) -> Arc<dyn ResourceExecutionHost> {
    Arc::new(PrincipalScopedHost { inner, context })
}
