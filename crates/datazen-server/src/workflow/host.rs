//! `WorkflowHost` 的 server 实现（W8 刀3c-2）。
//!
//! 桌面对照物：`datazen-desktop/src/workflow/host.rs` 的
//! `impl WorkflowHost for AppState`。四端口的 server 形态：
//!
//! - **`connection_name`**：profile 读取（`ProfileReader` 端口，组织
//!   限定）——名字供步骤结果的归因文案；禁用档案按「不可用」拒绝
//!   （桌面 `platform_profile` + `metadata.enabled` 的同语义）。
//! - **`execute_command`**：共享层 `workflow::command_runtime::
//!   execute_command`（规则单家：显式命名空间校验 → 有效连接解析 →
//!   workflow 命令派发），宿主原语经 [`ResourceExecutionHost`] 注入——
//!   **per-request 身份包装**（`PrincipalScopedHost` 先例：`prepare`
//!   产出的上下文覆写为请求真主体，审计归因不落合成服务主体）。
//!   调度派发用同一包装，上下文来自授权单派生的服务身份。
//! - **`sql_dialect`**：执行端口的 `profile`（`ConnectionConfig` 携带
//!   `database_type`）→ `driver` → `sql_literal_dialect`（桌面
//!   `connection_sql_dialect` 的同链；档案/驱动缺失回落标准方言——
//!   模板渲染的 fail-safe 默认，不是错误面）。
//! - **`complete_ai`**：**fail-closed**——server 形态本刀不带 AI 供应商
//!   注册表（aiHost 域的 provider 面尚未上 HTTP）。含 AI 步骤的交互
//!   执行得到明确的 `WorkflowError::Ai`（HTTP 面 400 `invalidArgument`
//!   家族），调度面则更早被授权单派生拒绝（服务身份不放行 AI 步骤——
//!   `collect_schedule_targets` 的 `None` 分支）。两条路都是显式
//!   拒绝，不是静默降级。

use std::sync::Arc;

use datazen::connection::domain::ProfileReader as _;
use datazen::execution::domain::permission::McpPermissionMode;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, ConnectionId, OrganizationId, PrincipalId, RequestId};
use datazen::sql_template::SqlTemplateDialect;
use datazen::workflow::domain::command::WorkflowCommandStep;
use datazen::workflow::domain::error::WorkflowError;
use datazen::workflow::host::WorkflowHost;
use datazen::workflow::resource_execution::ResourceExecutionHost;
use datazen_driver_api::CommandResult;

use crate::storage::ServerProfiles;

/// server 形态的 workflow 执行宿主。
///
/// 构造期注入执行原语（`ResourceExecutionHost`——与 aiHost 目标执行 /
/// MCP 七工具 / driverGateway 同一实例族，身份包装由调用方决定）与
/// profile 读取面；每请求/每派发各造一个轻量值（字段都是 `Arc`）。
pub struct ServerWorkflowHost {
    execution: Arc<dyn ResourceExecutionHost>,
    profiles: Arc<ServerProfiles>,
    organization_id: String,
}

impl ServerWorkflowHost {
    /// 构造（bootstrap 装配入口）。
    ///
    /// `execution` 通常是 `PrincipalScopedHost { inner: resource_host,
    /// context }`——per-request/per-dispatch 真主体；直接注入裸宿主也
    /// 合法（身份面退回合成服务主体，与 MCP 七工具的既定登记同款）。
    /// `organization_id` 是宿主配置的组织（v1 单组织；profile 读取的
    /// 限定边界）。
    pub fn new(
        execution: Arc<dyn ResourceExecutionHost>,
        profiles: Arc<ServerProfiles>,
        organization_id: &str,
    ) -> Self {
        Self {
            execution,
            profiles,
            organization_id: organization_id.to_owned(),
        }
    }
}

#[async_trait::async_trait]
impl WorkflowHost for ServerWorkflowHost {
    async fn connection_name(&self, connection_id: &str) -> Result<String, WorkflowError> {
        let unavailable = || WorkflowError::ConnectionFailed {
            connection_id: connection_id.to_owned(),
            message: "profile unavailable".into(),
        };
        // 后台读取上下文（与桌面 `platform_profile` 同语义：组织按宿主
        // 配置的单一组织；主体是本宿主件自己的合成身份——只读面，
        // 不产生审计行）。
        let profile = self
            .profiles
            .get(
                &background_context(&self.organization_id),
                ConnectionId::new(connection_id.to_owned()),
            )
            .await
            .map_err(|error| WorkflowError::ConnectionFailed {
                connection_id: connection_id.to_owned(),
                message: error.to_string(),
            })?
            .ok_or_else(unavailable)?;
        if !profile.enabled {
            return Err(unavailable());
        }
        Ok(profile.name)
    }

    async fn execute_command(
        &self,
        step: &WorkflowCommandStep,
        workflow_connection: Option<&str>,
        permission_mode: Option<McpPermissionMode>,
        deadline: Option<tokio::time::Instant>,
    ) -> Result<CommandResult, WorkflowError> {
        // 规则单家：显式命名空间校验、有效连接解析与 workflow 命令
        // 派发全在共享层；本宿主只供执行原语。
        datazen::workflow::command_runtime::execute_command(
            self.execution.as_ref(),
            step,
            workflow_connection,
            permission_mode,
            deadline,
        )
        .await
    }

    async fn sql_dialect(&self, connection_id: &str) -> SqlTemplateDialect {
        // 档案（含 database_type 的 ConnectionConfig）→ 驱动 → 方言；
        // 任一环缺失回落标准方言（fail-safe 默认，桌面同链同回落）。
        let Ok((config, _, _)) = self.execution.profile(connection_id).await else {
            return SqlTemplateDialect::default();
        };
        let Ok(driver) = self.execution.driver(&config).await else {
            return SqlTemplateDialect::default();
        };
        driver
            .sql_literal_dialect()
            .map(SqlTemplateDialect::from)
            .unwrap_or_default()
    }

    async fn complete_ai(&self, _prompt: String) -> Result<String, WorkflowError> {
        // fail-closed：本刀 server 形态无 AI 供应商面（见模块头）。
        Err(WorkflowError::Ai(
            "AI steps are not served by this host: the AI provider registry is not wired into \
             the server form in this slice (no execution was started for the step)."
                .into(),
        ))
    }
}

/// 只读面的后台上下文（`profile` 读取的组织限定；与
/// `schedule_grant` 的后台上下文同一裁决：主体是宿主件自己的合成
/// 身份，不借请求身份也不借桌面用户）。
pub(crate) fn background_context(organization_id: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(organization_id.to_owned()),
        PrincipalId::new("server-workflow-host".to_owned()),
        None,
        ClientInstanceId::new("server-workflow-host".to_owned()),
        RequestId::new(format!("workflow-host-{}", uuid::Uuid::new_v4())),
        None,
    )
}
