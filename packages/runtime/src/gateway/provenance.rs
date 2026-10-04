//! CM-61 / CM-62：来源与权限重校验。
//!
//! ## 来源（CM-61）
//!
//! 每次执行的 `source` 来自**请求**，在受理那一刻冻结进执行登记，
//! 之后任何事件、任何状态变更都不得改写它（[`ExecutionSource`] 没有 setter，
//! `ExecutionRegistry::record_source` 只接受 `Option` 之外的新登记而不接受覆盖）。
//!
//! CM-61 还要求落盘结构里**不能**出现 `dbSessionId` / `SessionHandle` /
//! `resourceBindingId` / lease / cancel 句柄——它们是内存态运行句柄，
//! 落盘就会在会话重建后指向一个早已失效的绑定。本模块用
//! [`ExecutionSource::forbidden_persistable_fields`] 把这条禁令写成可执行的清单，
//! 由单测逐条比对序列化后的 JSON 键名。
//!
//! ## 权限（CM-62）
//!
//! §7.2 把授权检查放在**两处**：第 1 步（进入网关时）与第 4 步
//! （进入 actor、**真正执行前**）。两次之间隔着排队、可能还有用户交互，
//! 权限可能在窗口期内被收回；只查一次等于让一条已撤销的授权跑完整个 SQL。
//!
//! [`Authorizer`] 因此是网关的构造依赖而不是可选回调：网关**无法**在
//! 没有授权器的情况下被构造出来，所以「忘了重校验」在类型上就不可能发生。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::connection::{DbSessionId, OrganizationId, PrincipalId, SessionView};

/// 发起一次执行的来源类别。
///
/// 与 `connection::types::OwnerRef` 的四种归属对齐，但**刻意不共用类型**：
/// `OwnerRef` 描述的是「资源归谁」，`ExecutionSource` 描述的是「这条命令从哪条产品链路来」，
/// 前者是授权输入，后者是审计输入。把两者合成一个 newtype 会让审计记录被迫带上
/// 授权侧的字段，反之亦然。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceKind {
    Editor,
    Job,
    WorkflowBlock,
    ClientSession,
}

impl SourceKind {
    /// 审计落盘用的稳定字面量。
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Editor => "editor",
            SourceKind::Job => "job",
            SourceKind::WorkflowBlock => "workflowBlock",
            SourceKind::ClientSession => "clientSession",
        }
    }

    /// 反查。未知字面量返回 `None`，调用方自行决定是否拒绝。
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "editor" => Some(SourceKind::Editor),
            "job" => Some(SourceKind::Job),
            "workflowBlock" => Some(SourceKind::WorkflowBlock),
            "clientSession" => Some(SourceKind::ClientSession),
            _ => None,
        }
    }

    /// 该来源链路是否属于「非交互」：作业与工作流块没有人在前台等结果。
    ///
    /// 这是一条**纯分类**，只描述来源本身，**不描述网关行为**。网关对四类来源
    /// 的受理路径完全一致：来源既不改变放行闸，也不改变回执形状，它只在两处
    /// 参与判定——CM-61 的执行来源冻结，以及 CM-61 的幂等指纹（同一幂等键换
    /// 来源是冲突，不是重发）。别把这读成「后台来源另有一套处置」。
    pub fn is_background(self) -> bool {
        matches!(self, SourceKind::Job | SourceKind::WorkflowBlock)
    }
}

/// 一次执行的来源（CM-61 的落盘形态）。
///
/// 只有**可持久化**的字段：`kind` / `source_id` / `organization_id` / `principal_id`。
/// 没有 `db_session_id`、没有 `runtime_epoch`、没有任何 cancel 句柄。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSource {
    kind: SourceKind,
    source_id: String,
    organization_id: Option<OrganizationId>,
    principal_id: Option<PrincipalId>,
}

impl ExecutionSource {
    /// 构造来源。`source_id` 必须非空——审计记录里「来自哪」不能是空串。
    pub fn new(
        kind: SourceKind,
        source_id: impl Into<String>,
        organization_id: Option<OrganizationId>,
        principal_id: Option<PrincipalId>,
    ) -> Self {
        Self {
            kind,
            source_id: source_id.into(),
            organization_id,
            principal_id,
        }
    }

    pub fn kind(&self) -> SourceKind {
        self.kind
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn organization_id(&self) -> Option<&OrganizationId> {
        self.organization_id.as_ref()
    }

    pub fn principal_id(&self) -> Option<&PrincipalId> {
        self.principal_id.as_ref()
    }

    /// 是否可以直接进入审计落盘。
    pub fn is_persistable(&self) -> bool {
        !self.source_id.is_empty() && SourceKind::from_str(self.kind.as_str()).is_some()
    }

    /// CM-61 明令不得出现在落盘结构里的字段名。
    ///
    /// 写成清单而不是写成注释，是为了让「以后有人加了个 runtime 句柄字段」
    /// 在单测里立刻变红，而不是留到审计对账时才发现。
    pub fn forbidden_persistable_fields() -> &'static [&'static str] {
        &[
            "dbSessionId",
            "sessionHandle",
            "runtimeEpoch",
            "resourceBindingId",
            "leaseId",
            "cancelHandle",
            "executionIdentityKey",
        ]
    }

    /// 审计落盘形态。
    pub fn to_persistable_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "sourceId": self.source_id,
            "organizationId": self.organization_id.as_ref().map(|v| v.as_str()),
            "principalId": self.principal_id.as_ref().map(|v| v.as_str()),
        })
    }
}

/// 请求主体。授权检查的输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPrincipal {
    principal_id: PrincipalId,
    organization_id: OrganizationId,
    org_session_id: DbSessionId,
}

impl RequestPrincipal {
    pub fn new(
        principal_id: PrincipalId,
        organization_id: OrganizationId,
        org_session_id: DbSessionId,
    ) -> Self {
        Self {
            principal_id,
            organization_id,
            org_session_id,
        }
    }

    pub fn principal_id(&self) -> &PrincipalId {
        &self.principal_id
    }

    pub fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }

    /// 组织会话 id。注意命名规范：这是**组织侧**会话，与 `dbSessionId` 不是同一个概念。
    pub fn org_session_id(&self) -> &DbSessionId {
        &self.org_session_id
    }
}

/// 网关在两个检查点上要判定的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayAction {
    /// §7.2 第 1 步 / 第 4 步：执行。
    Execute,
    /// §7.6：取消。
    Cancel,
}

impl GatewayAction {
    pub fn as_str(self) -> &'static str {
        match self {
            GatewayAction::Execute => "execute",
            GatewayAction::Cancel => "cancel",
        }
    }
}

/// 授权拒绝。`reason` 是稳定字面量，便于审计聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationDenial {
    pub action: GatewayAction,
    pub reason: &'static str,
}

impl AuthorizationDenial {
    pub fn new(action: GatewayAction, reason: &'static str) -> Self {
        Self { action, reason }
    }
}

/// 授权器：网关的必需构造依赖（CM-62）。
///
/// 实现方拿到的是「主体 + 动作 + 会话投影 + 来源」，足以判定组织归属、
/// 连接归属与权限级别。注意签名里**没有** `RuntimeError`：授权拒绝不是运行时故障，
/// 它是一条独立的判定结果，网关据此产出自己的拒绝而不是伪造一条端口错误。
pub trait Authorizer: Send + Sync + 'static {
    fn authorize(
        &self,
        principal: &RequestPrincipal,
        action: GatewayAction,
        view: &SessionView,
        source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial>;
}

/// 恒定放行的授权器。
///
/// 只用于**本模块的单测**与「鉴权在别处已经做完」的宿主装配场景。
/// 名字里的 `Always` 是刻意的：任何把它接到生产路径上的代码审查都应该停下来问一句。
#[derive(Debug, Default)]
pub struct AlwaysAllow;

impl Authorizer for AlwaysAllow {
    fn authorize(
        &self,
        _principal: &RequestPrincipal,
        _action: GatewayAction,
        _view: &SessionView,
        _source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial> {
        Ok(())
    }
}

impl AlwaysAllow {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self)
    }
}

/// 恒定拒绝的授权器。同样只用于夹具。
#[derive(Debug, Clone, Copy)]
pub struct AlwaysDeny {
    reason: &'static str,
}

impl AlwaysDeny {
    pub fn new(reason: &'static str) -> Self {
        Self { reason }
    }

    pub fn shared(reason: &'static str) -> Arc<Self> {
        Arc::new(Self::new(reason))
    }
}

impl Authorizer for AlwaysDeny {
    fn authorize(
        &self,
        _principal: &RequestPrincipal,
        action: GatewayAction,
        _view: &SessionView,
        _source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial> {
        Err(AuthorizationDenial::new(action, self.reason))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> ExecutionSource {
        ExecutionSource::new(
            SourceKind::Editor,
            "editor-session-42",
            Some(OrganizationId::new("org-1")),
            Some(PrincipalId::new("principal-7")),
        )
    }

    #[test]
    fn kind_literals_round_trip() {
        for kind in [
            SourceKind::Editor,
            SourceKind::Job,
            SourceKind::WorkflowBlock,
            SourceKind::ClientSession,
        ] {
            assert_eq!(SourceKind::from_str(kind.as_str()), Some(kind));
        }
        assert_eq!(SourceKind::from_str("nope"), None);
    }

    #[test]
    fn persistable_json_carries_no_runtime_handles() {
        let json = source().to_persistable_json();
        for forbidden in ExecutionSource::forbidden_persistable_fields() {
            assert!(
                json.get(*forbidden).is_none(),
                "CM-61：落盘结构不得包含 {forbidden}"
            );
        }
        assert_eq!(json.get("kind").and_then(Value::as_str), Some("editor"));
        assert_eq!(
            json.get("sourceId").and_then(Value::as_str),
            Some("editor-session-42")
        );
    }

    #[test]
    fn an_empty_source_id_is_not_persistable() {
        let empty = ExecutionSource::new(SourceKind::Editor, "", None, None);
        assert!(!empty.is_persistable());
        assert!(source().is_persistable());
    }

    #[test]
    fn background_sources_are_exactly_job_and_workflow_block() {
        assert!(SourceKind::Job.is_background());
        assert!(SourceKind::WorkflowBlock.is_background());
        assert!(!SourceKind::Editor.is_background());
        assert!(!SourceKind::ClientSession.is_background());
    }

    #[test]
    fn denial_carries_action_and_stable_reason() {
        let denial = AuthorizationDenial::new(GatewayAction::Cancel, "permissionDenied");
        assert_eq!(denial.action, GatewayAction::Cancel);
        assert_eq!(denial.reason, "permissionDenied");
        assert_eq!(GatewayAction::Execute.as_str(), "execute");
        assert_eq!(GatewayAction::Cancel.as_str(), "cancel");
    }
}
