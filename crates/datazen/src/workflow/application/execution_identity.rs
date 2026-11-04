//! 定时调度执行的服务身份上下文（WR-A4 收尾：执行上下文接线）。
//!
//! # 它接什么线
//!
//! `engine::scheduler` 的派发闸门（上膛冻结 → 触发比对）放行后，引擎手里有
//! 当次放行所依据的 [`WorkflowScheduleGrant`]。本模块把那份授权单转成**执行
//! 上下文**：调度执行以 [`WorkflowScheduleGrant::service_principal`] 的名义
//! 派发——
//!
//! * **owner 归属**：组织取授权单冻结的 `organization_id`（授权单由定义冻结
//!   事实派生，组织归属随之冻结），绝不取调用方声明；
//! * **审计主体**：[`AuditPrincipalKind::Service`]（§3.7 CHECK 字面量
//!   `service`），principal = 服务身份字符串；
//! * **委托**：恒 `None`——服务身份不是委托链路（CM-06 的委托走
//!   `DelegationRef`，与调度身份不是同一条路径）。
//!
//! # 为什么不是又一个窄端口
//!
//! 身份派生是授权单的**纯函数**，不需要宿主资源（无存储；审计时刻由调用方
//! 传入），因此**不设** `ExecutionIdentitySource` 一类的宿主端口。宿主资源
//! 出现在**消费侧**：宿主经 `WorkflowScheduleHost::execute_workflow` 的身份
//! 参数接收上下文（桌面半边 = `DesktopScheduleHost`，负责归属核对与本地
//! 审计日志）。与 `crate::execution::application::identity` 的
//! `ExecutionIdentity`（服务端会话/凭据隔离键）也不是同一层：这里铸造的是
//! **调度派发的主体归属事实**，不参与执行身份键与池键。

use crate::shared::audit::{AuditEvent, AuditOutcome, AuditPrincipalKind};
use crate::shared::id::Timestamp;
use crate::shared::id::{DelegationId, OrganizationId, PrincipalId};
use crate::workflow::domain::grant::WorkflowScheduleGrant;

/// 一次定时派发的执行上下文：以服务身份执行的事实集合。
///
/// 字段私有、只能由 [`schedule_execution_identity`] 从授权单派生——
/// 「身份由后端派生，不由客户端声明」（与 `service_principal`、monitor
/// 授权 IPC 同一裁决）在类型层面收口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledExecutionIdentity {
    workflow_id: String,
    workflow_revision: String,
    organization_id: String,
    service_principal: String,
    approved_by: String,
}

/// 从**放行所依据的授权单**派生执行上下文。
///
/// 调用点是 `engine::scheduler` 的派发路径：闸门放行返回的那份授权单就是
/// 本函数的唯一输入——闸门拒绝时根本没有派发，也就没有上下文。
pub fn schedule_execution_identity(grant: &WorkflowScheduleGrant) -> ScheduledExecutionIdentity {
    ScheduledExecutionIdentity {
        workflow_id: grant.workflow_id.clone(),
        workflow_revision: grant.workflow_revision.clone(),
        organization_id: grant.organization_id.clone(),
        service_principal: grant.service_principal.clone(),
        approved_by: grant.approved_by.clone(),
    }
}

impl ScheduledExecutionIdentity {
    /// 被派发的工作流。
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// 派发所依据的定义修订（授权单冻结事实，审计可回溯到具体定义）。
    pub fn workflow_revision(&self) -> &str {
        &self.workflow_revision
    }

    /// owner 归属：授权单冻结的组织。
    pub fn organization_id(&self) -> &str {
        &self.organization_id
    }

    /// 审计主体：`workflow-schedule:{workflow_id}:{修订前缀}`。
    pub fn service_principal(&self) -> &str {
        &self.service_principal
    }

    /// 批准事实（桌面形态 = 带 schedule 的定义保存动作）。
    pub fn approved_by(&self) -> &str {
        &self.approved_by
    }

    /// 服务身份不走委托链路：恒 `None`。委托是 CM-06 的另一条路径，
    /// 混用会让「谁授权、谁执行」在审计里不可判读。
    pub fn delegation_id(&self) -> Option<DelegationId> {
        None
    }

    /// 派发/完成审计事件：主体 kind = Service、principal = 服务身份、
    /// 归属授权单的组织，无客户端实例（调度不在请求周期内）、无委托。
    ///
    /// `action` 是开放词表动作（如 `workflow_scheduled_dispatch`）；
    /// `occurred_at` 由调用方经 `shared::clock` 提供（本模块不读时钟）。
    pub fn audit_event(
        &self,
        action: &str,
        outcome: AuditOutcome,
        occurred_at: Timestamp,
    ) -> AuditEvent {
        let mut event = AuditEvent::new(
            OrganizationId::new(self.organization_id.clone()),
            AuditPrincipalKind::Service,
            action,
            outcome,
            occurred_at,
        );
        event.principal_id = Some(PrincipalId::new(self.service_principal.clone()));
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::domain::grant::{definition_revision, WorkflowScheduleScope};

    const DEFINITION: &str = "id: demo\nsteps: []\n";

    /// 夹具组织 id 用形态中性的 `"datazen-local-org"`（与 schema_diff 单租户
    /// 夹具同一裁决：不借桌面规范 id，避免把测试钉在某一宿主形态上）。
    fn grant() -> WorkflowScheduleGrant {
        WorkflowScheduleGrant {
            workflow_id: "demo".into(),
            workflow_revision: definition_revision(DEFINITION),
            organization_id: "datazen-local-org".into(),
            service_principal: service_principal_for(),
            approved_by: "definition-save".into(),
            scopes: vec![WorkflowScheduleScope::namespace(
                "conn-1",
                Some("app".into()),
                None,
                1,
                "query",
            )],
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            revocation_version: 1,
            revoked: false,
        }
    }

    fn service_principal_for() -> String {
        crate::workflow::domain::grant::service_principal("demo", &definition_revision(DEFINITION))
    }

    fn occurred_at() -> Timestamp {
        Timestamp::new("2026-01-01T00:00:00.000Z")
    }

    /// 行为钉子：上下文的每个字段都来自授权单——owner 归属组织、审计主体
    /// 是服务身份、批准事实随单携带。派生是纯函数（同单同上下文）。
    #[test]
    fn the_identity_is_derived_from_the_grant_and_nothing_else() {
        let grant = grant();
        let identity = schedule_execution_identity(&grant);
        assert_eq!(
            identity,
            schedule_execution_identity(&grant),
            "同单同上下文"
        );
        assert_eq!(identity.workflow_id(), "demo");
        assert_eq!(identity.workflow_revision(), grant.workflow_revision);
        assert_eq!(identity.organization_id(), "datazen-local-org");
        assert_eq!(identity.service_principal(), grant.service_principal);
        assert_eq!(identity.service_principal(), service_principal_for());
        assert_eq!(identity.approved_by(), "definition-save");
        assert!(identity.delegation_id().is_none(), "服务身份不携带委托");
    }

    /// 行为钉子：定义变更 ⇒ 授权单换修订 ⇒ 服务身份随之更换 ⇒ 上下文变化。
    /// （调度器的「冻结比对」拒绝旧单，新单以新身份派发——两条链路语义对齐。）
    #[test]
    fn a_definition_change_moves_the_identity() {
        let mut changed = grant();
        changed.workflow_revision = definition_revision("id: changed\n");
        changed.service_principal =
            crate::workflow::domain::grant::service_principal("demo", &changed.workflow_revision);
        assert_ne!(
            schedule_execution_identity(&grant()),
            schedule_execution_identity(&changed),
            "定义变更必须换执行上下文"
        );
    }

    /// 行为钉子：审计事件主体 kind = Service（§3.7 字面量 `service`）、
    /// principal = 服务身份、归属授权单组织、无委托、无客户端实例。
    #[test]
    fn the_audit_event_records_a_service_principal_with_owner_attribution() {
        let identity = schedule_execution_identity(&grant());
        let event = identity.audit_event(
            "workflow_scheduled_dispatch",
            AuditOutcome::Succeeded,
            occurred_at(),
        );
        assert_eq!(event.principal_kind, AuditPrincipalKind::Service);
        assert_eq!(event.principal_kind.as_str(), "service");
        assert_eq!(
            event.principal_id.as_ref().map(|p| p.as_str()),
            Some(identity.service_principal()),
            "审计主体必须是服务身份，而不是批准者或桌面用户"
        );
        assert_eq!(event.organization_id.as_str(), "datazen-local-org");
        assert!(event.delegation_id.is_none());
        assert!(event.client_instance_id.is_none(), "调度不在请求周期内");
        assert_eq!(event.action, "workflow_scheduled_dispatch");
        assert_eq!(event.outcome, AuditOutcome::Succeeded);
        // Debug 形态不含秘密材料：服务身份本就是无秘密的派生词。
        let debug = format!("{identity:?}");
        assert!(!debug.to_ascii_lowercase().contains("password"));
        assert!(!debug.to_ascii_lowercase().contains("token"));
    }
}
