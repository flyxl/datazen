//! 资源层端口 DTO —— 九个操作及其返回值。
//!
//! fake provider 走的是**资源层**而不是 driver 层：这里只有一般会话语义，
//! 不含任何 MySQL/PostgreSQL 方言实现。

use serde::{Deserialize, Serialize};

use crate::connection::capability::CapabilitySnapshot;
use crate::connection::error::ProviderError;
use crate::connection::execution::{
    EffectOutcome, ExecutionErrorCode, StatementResult, TruncationRecord,
};
use crate::connection::session::{
    SessionContext, SessionHandleRef, SessionState, SessionView, TransactionState,
};
use crate::connection::types::{
    fnv1a64_hex, Counter, DbSessionId, ExecutionId, ExecutionTarget, HandleId, NamespaceTarget,
    OwnerRef, PoolKeyFingerprint, ResourceId, TargetNamespaceShape, UNKNOWN_SENTINEL,
};

/// 会话连续性。fake 固定为 `fixed`（`describeResource`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionContinuity {
    Fixed,
}

impl SessionContinuity {
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionContinuity::Fixed => "fixed",
        }
    }
}

/// 连接成本策略。一个 fake resource ≡ 一个物理连接。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionCostPolicy {
    PerPhysicalConnection,
}

/// 归池复用策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReusePolicy {
    /// 归池前必须通过全部前置检查。
    ResetBeforeReturn,
    CloseOnly,
}

/// 资源描述符。端口契约要求包含的七项齐全。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDescriptor {
    pub provider_id: String,
    pub resource_key: String,
    pub session_continuity: SessionContinuity,
    pub reuse_policy: ReusePolicy,
    pub initialization_requirements: Vec<String>,
    pub connection_cost_policy: ConnectionCostPolicy,
    pub namespace_shape: TargetNamespaceShape,
    pub capabilities: CapabilitySnapshot,
}

/// 资源健康度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceHealth {
    Healthy,
    Degraded,
    Lost,
}

/// API 定义的不透明句柄。只能由 provider 签发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceHandle {
    pub resource_id: ResourceId,
    pub runtime_epoch: Counter,
    pub owner_token: String,
}

impl ResourceHandle {
    /// `ownerToken = fnv1a64(ownerHash | runtimeEpoch | resourceId)`。
    pub fn issue(resource_id: &ResourceId, epoch: &Counter, owner: &OwnerRef) -> Self {
        let material = format!("{}|{}|{}", owner.hash(), epoch.get(), resource_id);
        Self {
            resource_id: resource_id.clone(),
            runtime_epoch: *epoch,
            owner_token: fnv1a64_hex(material.as_bytes()),
        }
    }

    /// provider 在**每个**操作上校验 `resourceId` + `runtimeEpoch` + owner。
    pub fn verify(
        &self,
        resource_id: &ResourceId,
        epoch: &Counter,
        owner: &OwnerRef,
    ) -> Result<(), ProviderError> {
        if &self.resource_id != resource_id {
            return Err(ProviderError::SessionLost(format!(
                "resourceId 不匹配：句柄属于 {}，操作指向 {}",
                self.resource_id, resource_id
            )));
        }
        if self.runtime_epoch != *epoch {
            return Err(ProviderError::RuntimeEpochMismatch(format!(
                "runtimeEpoch 不匹配：句柄 epoch={}，当前 epoch={}",
                self.runtime_epoch.get(),
                epoch.get()
            )));
        }
        let expected = Self::issue(resource_id, epoch, owner);
        if self.owner_token != expected.owner_token {
            return Err(ProviderError::SessionLost(
                "ownerToken 不匹配：这是另一个 owner 的句柄".to_owned(),
            ));
        }
        Ok(())
    }
}

/// `executeOnResource` 的完成状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompletionStatus {
    Ok,
    Error,
}

/// 事务观测。`None` / `InTransaction`，提交后置 `Unknown`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionObservation {
    None,
    InTransaction { handle_id: HandleId },
    Unknown,
    Unsupported,
}

/// `ExecutionCompletion`。端口契约要求九项齐全。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionCompletion {
    pub completion_status: CompletionStatus,
    /// 派发**之后**的终态错误码。派发前拒绝不存在此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<ExecutionErrorCode>,
    pub effect_outcome: EffectOutcome,
    pub statement_results: Vec<StatementResult>,
    pub context_before: Option<SessionContext>,
    pub context_after: Option<SessionContext>,
    pub transaction_observation: TransactionObservation,
    /// 本次执行交给宿主的句柄。驱动必须如实报告，没有就是空数组。
    pub session_handles: Vec<SessionHandleRef>,
    pub protocol_drained: bool,
    pub resource_health: ResourceHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<TruncationRecord>,
}

impl ExecutionCompletion {
    /// 成功且协议已排空的基线完成件。
    pub fn ok() -> Self {
        Self {
            completion_status: CompletionStatus::Ok,
            error_code: None,
            effect_outcome: EffectOutcome::Completed,
            statement_results: Vec::new(),
            context_before: None,
            context_after: None,
            transaction_observation: TransactionObservation::None,
            session_handles: Vec::new(),
            protocol_drained: true,
            resource_health: ResourceHealth::Healthy,
            truncation: None,
        }
    }
}

/// `observeSession` 的结果。`unknown` 字段**禁止**回填 `initialTarget`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionObservation {
    pub context: SessionContext,
    pub transaction_state: TransactionState,
    pub handle_count: usize,
}

impl SessionObservation {
    /// 三个字段都可以是 `unknown`：此时 context 的 confidence 必须是 `Unknown`。
    pub fn all_unknown() -> Self {
        Self {
            context: SessionContext::unknown(
                &NamespaceTarget::unknown_placeholder(),
                UNKNOWN_SENTINEL,
            ),
            transaction_state: TransactionState::Unknown,
            handle_count: 0,
        }
    }
}

/// `changeContext` 结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeContextOutcome {
    Confirmed {
        context: SessionContext,
        context_revision: Counter,
    },
    RequiresReplacement {
        reason: String,
    },
    Unsupported,
}

/// `resetResource` 结果。**`Clean` 不等于事务终结**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetOutcome {
    Clean,
    Discard { reason: ResetDiscardReason },
}

/// driver 侧 `Discard` 的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetDiscardReason {
    SessionStillExecuting,
    SessionStateAborted,
    TemporaryObjectsPresent,
    HealthDegraded,
}

/// `closeResource` 结果。幂等：重复调用仍然是 `Closed`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseOutcome {
    Closed,
    CloseUnconfirmed,
}

/// 精确取消的处置。`unsupported` 是**正常返回值，不是异常**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelDisposition {
    Requested,
    Unsupported,
    AlreadyFinished,
}

impl CancelDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            CancelDisposition::Requested => "requested",
            CancelDisposition::Unsupported => "unsupported",
            CancelDisposition::AlreadyFinished => "alreadyFinished",
        }
    }
}

/// 预算类别。permit 台账按它分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BudgetClass {
    ShortOpPool,
    Session,
    Service,
    Control,
}

impl BudgetClass {
    pub const ALL: [BudgetClass; 4] = [
        BudgetClass::ShortOpPool,
        BudgetClass::Session,
        BudgetClass::Service,
        BudgetClass::Control,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            BudgetClass::ShortOpPool => "shortOpPool",
            BudgetClass::Session => "session",
            BudgetClass::Service => "service",
            BudgetClass::Control => "control",
        }
    }
}

/// permit 归还原因。允许四种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermitReason {
    Acquire,
    Close,
    Quarantine,
    Writeoff,
}

impl PermitReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            PermitReason::Acquire => "acquire",
            PermitReason::Close => "close",
            PermitReason::Quarantine => "quarantine",
            PermitReason::Writeoff => "writeoff",
        }
    }
}

/// 不透明许可标识。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitId(pub String);

impl PermitId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PermitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 一次申请得到的许可集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitSet {
    pub class: BudgetClass,
    pub permit_ids: Vec<PermitId>,
}

/// 预算端口。按**实际物理连接**申请与归还。
pub trait BudgetPort: Send + Sync {
    fn request(&self, class: BudgetClass, permits: u32) -> Result<PermitSet, ProviderError>;
    fn release(&self, permit: &PermitId, reason: PermitReason) -> Result<(), ProviderError>;
    fn outstanding(&self, class: BudgetClass) -> usize;
}

/// 取消回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelReceipt {
    pub execution_id: ExecutionId,
    pub disposition: CancelDisposition,
}

/// 关闭回执。`resourceRelease` 三态对应「立即归零 / 待核验 / 已隔离」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseReceipt {
    pub db_session_id: DbSessionId,
    pub state: SessionState,
    pub effect_outcome: EffectOutcome,
    pub resource_release: ResourceRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceRelease {
    Pending,
    Confirmed,
    Quarantined,
}

/// 上下文切换回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextChangeReceipt {
    pub context_revision: Counter,
    pub before: SessionContext,
    pub after: SessionContext,
}

/// 九个操作的入参。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribeResourceRequest {
    pub target: ExecutionTarget,
    pub owner: OwnerRef,
    pub identity_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquireResourceRequest {
    pub descriptor: ResourceDescriptor,
    pub pool_key: PoolKeyFingerprint,
    pub budget_class: BudgetClass,
    pub owner: OwnerRef,
    pub db_session_id: DbSessionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteOnResourceRequest {
    pub handle: ResourceHandle,
    pub execution_id: ExecutionId,
    pub command_id: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveSessionRequest {
    pub handle: ResourceHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeContextRequest {
    pub handle: ResourceHandle,
    pub expected_context_revision: Counter,
    pub target: ExecutionTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionOperation {
    Begin,
    Commit(HandleId),
    Rollback(HandleId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCancelRequest {
    pub handle: ResourceHandle,
    pub execution_id: ExecutionId,
    /// 精确 cancelHandle。命中未完成执行才返回 `Requested`。
    pub cancel_handle: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetResourceRequest {
    pub handle: ResourceHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseResourceRequest {
    pub handle: ResourceHandle,
    /// 宿主侧前置条件快照：已登记句柄是否为空、协议是否排空。
    pub registered_handles: usize,
    pub protocol_drained: bool,
}

/// 标记一个值**不得**写入 journal / 报告 / 测试输出（日志脱敏要求）。
///
/// 用类型把「能不能脱敏」变成编译期问题：只有持有 `Secret` 的路径能拿到内部串，
/// 而 `Debug` / `Display` 一律输出占位符。`attachmentToken` 与幂等令牌 nonce 是
/// 唯一允许走真实随机源的两类值，它们都必须经由本类型。
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// 构造一个受保护值。
    ///
    /// 唯一能拿到内部串的入口。调用点必须自己保证该值不会进入 journal、报告或
    /// 断言字面量（日志脱敏要求）；类型本身只保证 `Debug` / `Display` 不会泄漏。
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 取值。调用点必须在使用后立即丢弃，不得存入任何长期结构、不得拼进断言。
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// `openSession` 回执。`attachment_token` 走真实随机源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSessionReceipt {
    pub session: SessionView,
    pub attachment_token: Secret,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::types::{
        ClientInstanceId, ConnectionId, EditorSessionId, OrganizationId, PrincipalId,
    };

    fn owner() -> OwnerRef {
        OwnerRef::Editor {
            organization_id: OrganizationId::new("org-alpha"),
            principal_id: PrincipalId::new("user-alpha-1"),
            connection_id: ConnectionId::new("conn-fixture-p"),
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        }
    }

    #[test]
    fn resource_handle_rejects_a_foreign_epoch() {
        // 携带过期 runtimeEpoch 的句柄必须被拒绝。
        let id = ResourceId::new("res_w1_0001");
        let stale = ResourceHandle::issue(&id, &Counter::new(1), &owner());
        let err = stale
            .verify(&id, &Counter::new(2), &owner())
            .expect_err("epoch 变了必须拒绝");
        assert!(matches!(err, ProviderError::RuntimeEpochMismatch(_)));
    }

    #[test]
    fn resource_handle_rejects_another_owners_handle() {
        let id = ResourceId::new("res_w1_0001");
        let epoch = Counter::new(1);
        let handle = ResourceHandle::issue(&id, &epoch, &owner());
        let other = OwnerRef::Editor {
            organization_id: OrganizationId::new("org-beta"),
            principal_id: PrincipalId::new("user-beta-1"),
            connection_id: ConnectionId::new("conn-fixture-p"),
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("ed-1"),
        };
        let err = handle
            .verify(&id, &epoch, &other)
            .expect_err("别人的句柄必须拒绝");
        assert!(matches!(err, ProviderError::SessionLost(_)));
    }

    #[test]
    fn resource_handle_rejects_a_foreign_resource_id() {
        let handle =
            ResourceHandle::issue(&ResourceId::new("res_w1_0001"), &Counter::new(1), &owner());
        let err = handle
            .verify(&ResourceId::new("res_w1_0002"), &Counter::new(1), &owner())
            .expect_err("资源 id 必须校验");
        assert!(matches!(err, ProviderError::SessionLost(_)));
    }

    #[test]
    fn resource_handle_accepts_its_own_issuer_inputs() {
        let id = ResourceId::new("res_w1_0001");
        let epoch = Counter::new(3);
        let handle = ResourceHandle::issue(&id, &epoch, &owner());
        assert!(handle.verify(&id, &epoch, &owner()).is_ok());
    }

    #[test]
    fn cancel_disposition_literals_match_the_protocol_map() {
        assert_eq!(CancelDisposition::Requested.as_str(), "requested");
        assert_eq!(CancelDisposition::Unsupported.as_str(), "unsupported");
        assert_eq!(
            CancelDisposition::AlreadyFinished.as_str(),
            "alreadyFinished"
        );
    }

    #[test]
    fn permit_reasons_match_the_journal_vocabulary() {
        assert_eq!(PermitReason::Acquire.as_str(), "acquire");
        assert_eq!(PermitReason::Close.as_str(), "close");
        assert_eq!(PermitReason::Quarantine.as_str(), "quarantine");
        assert_eq!(PermitReason::Writeoff.as_str(), "writeoff");
    }

    #[test]
    fn completion_baseline_is_ok_completed_and_drained() {
        let completion = ExecutionCompletion::ok();
        assert_eq!(completion.completion_status, CompletionStatus::Ok);
        assert_eq!(completion.effect_outcome, EffectOutcome::Completed);
        assert!(completion.protocol_drained);
        assert!(completion.error_code.is_none());
        assert!(completion.session_handles.is_empty());
    }
}
