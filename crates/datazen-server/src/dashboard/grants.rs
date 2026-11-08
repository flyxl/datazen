//! Monitor 授权单的服务端流程（W8 刀3d-2）。
//!
//! 桌面对照物：`datazen-desktop/src/monitor/grants.rs`。授权单词汇
//! （`MonitorGrant` / `MonitorScope` / revision）、只读 SQL 校验与固定
//! 目标收集规则单家在共享层 `monitor`；本件是组织键投影后的流程编排：
//!
//! - [`authorize`]——用户批准一个 interval 调度：仪表板/组件状态闸 →
//!   工作流形状闸（无变量/无 blocks/固定只读查询步骤）→ 目标规范化
//!   （与执行 `prepare` 同一台机器）→ 授权单落库 → 冻结事实复验。
//! - [`revoke`]——撤销本 widget 的最新授权单（仅批准人可撤）。
//! - [`read`]——读最新授权单（仅批准人可读——桌面「grant owner
//!   required」同语义）。
//! - [`GrantFacts`]——调度执行时的授权单事实核验（最新性 + profile
//!   启用/版本——桌面 `scheduled::validate` 的 server 投影）。
//!
//! **身份口径**：`approved_by` = HTTP 会话主体（L2 认证链事实）；授权单
//! 的**执行**身份是 `grant.context()` 派生的服务主体（`monitor-service:
//! {id}`）——批准与执行分离，桌面同款。

use std::sync::Arc;

use datazen::connection::domain::catalog::ConnectionRepository as _;
use datazen::connection::domain::profile::ProfileReader as _;
use datazen::dashboard::repository::get_dashboard;
use datazen::dashboard::types::{Dashboard, DashboardWidget};
use datazen::execution::target::{ExecutionTarget, NamespaceTarget};
use datazen::monitor::application::fixed_targets;
use datazen::monitor::domain::{revision, validate_read_only_sql, MonitorGrant, MonitorScope};
use datazen::shared::context::RequestContext;
use datazen::shared::id::ConnectionId;
use datazen::workflow::application::context::WorkflowContext;
use datazen::workflow::model::{WorkflowDefinition, WorkflowStep};

use crate::execution::resource_host::ServerResourceExecutionHost;
use crate::storage::{
    ServerConnections, ServerDashboardStore, ServerMonitorGrants, ServerProfiles,
    ServerWorkflowStore,
};

/// 授权面错误（域路由 → 400 `invalidArgument` 家族；文案与桌面 IPC
/// `CommandError::Validation` 逐字同源——IPC 契约不变纪律）。
#[derive(Debug, thiserror::Error)]
pub enum GrantFlowError {
    #[error("{0}")]
    Validation(String),
}

fn validation(message: impl Into<String>) -> GrantFlowError {
    GrantFlowError::Validation(message.into())
}

/// 批准一个 widget 的 interval 调度（`authorize_monitor_widget_schedule`
/// 的服务端流程）。
///
/// `approver` 是 HTTP 会话上下文（L2 认证链事实）。桌面校验链逐环投影：
/// ttl 词表 → 仪表板/组件状态 → 工作流形状（无变量/无 blocks/固定
/// 只读查询）→ SQL 模板拒绝 → 只读 SQL → profile 启用 + PostgreSQL/
/// MySQL（调度只信任有事务只读语义的驱动族）→ 目标规范化（授权单
/// 冻结**规范化后**的目标——与执行闸的比对口径一致）→ 落库 → 冻结
/// 事实复验（定义 revision 未漂移）。
#[allow(clippy::too_many_arguments)]
pub async fn authorize(
    dashboards: &ServerDashboardStore,
    workflows: &ServerWorkflowStore,
    grants_store: &ServerMonitorGrants,
    profiles: &Arc<ServerProfiles>,
    connections: &Arc<ServerConnections>,
    resource_host: &Arc<ServerResourceExecutionHost>,
    approver: &RequestContext,
    dashboard_id: &str,
    widget_id: &str,
    ttl_secs: u32,
) -> Result<MonitorGrant, GrantFlowError> {
    if !(60..=86400).contains(&ttl_secs) {
        return Err(validation(
            "monitor grant lifetime must be 60..86400 seconds",
        ));
    }
    let scope = dashboards.scope();

    // 仪表板/组件状态闸（桌面同链）。
    let dashboard = get_dashboard(&scope, dashboards, dashboard_id)
        .map_err(|_| validation("dashboard missing"))?;
    let widget = widget_of(&dashboard, widget_id)?;
    if !dashboard.enabled
        || dashboard.refresh_paused
        || !widget.enabled
        || widget.refresh.mode != datazen::dashboard::types::RefreshMode::Interval
    {
        return Err(validation(
            "enable the interval schedule before authorizing it",
        ));
    }

    // 工作流形状闸：定义行（YAML 原文）→ 解析 → 无变量/无 blocks →
    // 固定目标收集。
    let record = workflows
        .raw_definition(&widget.workflow_id)
        .await
        .map_err(|_| validation("workflow missing"))?
        .ok_or_else(|| validation("workflow missing"))?;
    let (definition_yaml, _) = record;
    let workflow: WorkflowDefinition =
        serde_yaml::from_str(&definition_yaml).map_err(|_| validation("workflow invalid"))?;
    if !workflow.variables.is_empty() || !workflow.blocks.is_empty() {
        return Err(validation(
            "scheduled monitor supports fixed read-only query steps only",
        ));
    }
    workflow
        .clone()
        .into_executable()
        .map_err(|_| validation("workflow version invalid"))?;
    let workflow_revision = revision(&definition_yaml);
    let normalized =
        fixed_targets::prepare_definition(&workflow, &WorkflowContext::new(&serde_json::json!({})))
            .map_err(|_| validation("explicit workflow targets required"))?;

    let mut scopes = Vec::new();
    for step in &normalized.steps {
        let (command, sql, connection, database, schema) = match step {
            WorkflowStep::Query {
                sql,
                connection,
                database,
                ..
            } => (
                "query",
                sql.as_str(),
                connection.as_deref(),
                database.as_deref(),
                None,
            ),
            WorkflowStep::Command {
                command,
                connection,
                input,
                ..
            } if command == "query" => (
                command.as_str(),
                input["sql"]
                    .as_str()
                    .ok_or_else(|| validation("SQL required"))?,
                connection.as_deref(),
                input["database"].as_str(),
                input["schema"].as_str(),
            ),
            _ => {
                return Err(validation(
                    "scheduled monitor supports fixed read-only query steps only",
                ))
            }
        };
        if sql.contains("{{") || sql.contains("${") {
            return Err(validation(
                "scheduled SQL must not contain dynamic templates",
            ));
        }
        validate_read_only_sql(sql)
            .map_err(|_| validation("scheduled monitor requires read-only SQL"))?;
        let connection = connection.ok_or_else(|| validation("explicit connection required"))?;
        let database = database
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| validation("explicit database required"))?;
        let profile = profiles
            .get(approver, ConnectionId::new(connection.to_owned()))
            .await
            .map_err(|_| validation("connection missing"))?
            .ok_or_else(|| validation("connection missing"))?;
        // 调度只信任有事务只读语义的驱动族（桌面同闸：PostgreSQL/MySQL）。
        if !profile.enabled || !matches!(profile.driver_id.as_str(), "postgresql" | "mysql") {
            return Err(validation(
                "scheduled monitor requires enabled PostgreSQL/MySQL profile",
            ));
        }
        // 连接配置在场性（目录行；缺行 = 拒绝——目标规范化需要方言）。
        let config = connections
            .get(
                &datazen::connection::domain::ConnectionScope::new(
                    approver.organization_id.as_str(),
                ),
                connection,
            )
            .await
            .map_err(|_| validation("connection missing"))?
            .ok_or_else(|| validation("connection missing"))?;

        // 目标规范化（授权单冻结规范化身份——与执行 `prepare` 同机器）。
        let target = ExecutionTarget::new(
            ConnectionId::new(connection.to_owned()),
            NamespaceTarget::new(
                Some(database.to_owned()),
                None,
                schema.map(str::to_owned).or(config.schema.clone()),
                Vec::new(),
            ),
        );
        let canonical = resource_host
            .canonical_target(
                command,
                &target.connection_id,
                &target,
                &config.database_type,
            )
            .await
            .map_err(|_| validation("monitor target invalid"))?;

        scopes.push(MonitorScope {
            target: canonical,
            config_revision: profile.config_revision.counter().get(),
            command: command.to_owned(),
        });
    }

    let id = uuid::Uuid::new_v4().to_string();
    let grant = MonitorGrant {
        id: id.clone(),
        dashboard_id: dashboard_id.to_owned(),
        widget_id: widget_id.to_owned(),
        workflow_id: widget.workflow_id.clone(),
        workflow_revision: workflow_revision.clone(),
        organization_id: approver.organization_id.as_str().to_owned(),
        service_principal: format!("monitor-service:{id}"),
        approved_by: approver.principal_id.as_str().to_owned(),
        scopes,
        actions: vec!["connect".into(), "query".into()],
        read_only: true,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(i64::from(ttl_secs)),
        revocation_version: 1,
        revoked: false,
    };

    // 冻结事实复验（异步元数据/驱动检查后，定义未漂移才落库——桌面
    // 同款「Re-read frozen facts」）。
    let (latest_yaml, _) = workflows
        .raw_definition(&grant.workflow_id)
        .await
        .map_err(|_| validation("workflow missing"))?
        .ok_or_else(|| validation("workflow missing"))?;
    if revision(&latest_yaml) != grant.workflow_revision {
        return Err(validation("workflow changed during authorization"));
    }
    grants_store
        .save(&grant)
        .map_err(|_| validation("monitor grant persistence failed"))?;
    // 服务身份委托行物化（调度执行的服务主体授权机制；见存储件模块头
    // 的两个命名裁决）。
    grants_store
        .materialize_service_identity(&grant)
        .map_err(|_| validation("monitor service identity persistence failed"))?;
    Ok(grant)
}

/// 撤销本 widget 的最新授权单（仅批准人；无授权单 = 幂等 no-op，桌面
/// 同语义）。
pub fn revoke(
    grants_store: &ServerMonitorGrants,
    approver: &RequestContext,
    dashboard_id: &str,
    widget_id: &str,
) -> Result<(), GrantFlowError> {
    if let Some(mut grant) = grants_store
        .for_widget(dashboard_id, widget_id)
        .map_err(|_| validation("grant lookup failed"))?
    {
        if grant.approved_by != approver.principal_id.as_str() {
            return Err(validation("grant owner required"));
        }
        grant.revoked = true;
        grant.revocation_version = grant
            .revocation_version
            .checked_add(1)
            .ok_or_else(|| validation("grant version overflow"))?;
        grants_store
            .save(&grant)
            .map_err(|_| validation("grant revoke failed"))?;
        // 服务身份委托行连带撤销（best-effort 卫生操作；过期 TTL 兜底）。
        grants_store
            .revoke_service_identity(&grant.id)
            .map_err(|_| validation("service identity revoke failed"))?;
    }
    Ok(())
}

/// 读本 widget 的最新授权单（仅批准人可读）。
pub fn read(
    grants_store: &ServerMonitorGrants,
    requester: &RequestContext,
    dashboard_id: &str,
    widget_id: &str,
) -> Result<Option<MonitorGrant>, GrantFlowError> {
    let grant = grants_store
        .for_widget(dashboard_id, widget_id)
        .map_err(|_| validation("grant lookup failed"))?;
    if let Some(grant) = grant.as_ref() {
        if grant.approved_by != requester.principal_id.as_str() {
            return Err(validation("grant owner required"));
        }
    }
    Ok(grant)
}

/// 调度执行的授权单事实核验（`GrantScopedHost` 的两闸共用；桌面
/// `scheduled::validate` 的 server 投影）。
pub struct GrantFacts {
    pub grants: ServerMonitorGrants,
    pub profiles: Arc<ServerProfiles>,
}

impl GrantFacts {
    /// 核验一条冻结的授权单在当下仍然成立：未被更新替换（最新授权单
    /// 与冻结单逐字节一致）、profile 仍启用且版本未漂移。
    pub async fn validate_current(&self, frozen: &MonitorGrant) -> Result<(), String> {
        let latest = self
            .grants
            .for_widget(&frozen.dashboard_id, &frozen.widget_id)
            .map_err(|_| "monitor grant lookup failed".to_owned())?
            .ok_or_else(|| "monitor grant missing".to_owned())?;
        if &latest != frozen {
            return Err("monitor grant superseded".into());
        }
        if latest.revoked || latest.expires_at <= chrono::Utc::now() {
            return Err("monitor grant revoked or expired".into());
        }
        for scope in &frozen.scopes {
            let profile = self
                .profiles
                .get(&frozen.context(), scope.target.connection_id.clone())
                .await
                .map_err(|_| "monitor connection missing".to_owned())?
                .ok_or_else(|| "monitor connection missing".to_owned())?;
            if !profile.enabled
                || profile.config_revision.counter().get() != scope.config_revision
                || !matches!(profile.driver_id.as_str(), "postgresql" | "mysql")
            {
                return Err("monitor connection disabled or revision changed".into());
            }
        }
        Ok(())
    }
}

fn widget_of(dashboard: &Dashboard, widget_id: &str) -> Result<DashboardWidget, GrantFlowError> {
    dashboard
        .widgets
        .iter()
        .find(|widget| widget.id == widget_id)
        .cloned()
        .ok_or_else(|| validation("widget missing"))
}
