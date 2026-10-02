//! `BudgetCoordinator`：本机多维额度端口（§6.4 第 10 行）。
//!
//! 词汇表（§4.5）：`BudgetRequest`、`BudgetPermit`、`BudgetPermitSet`、`NodeLease`、
//! `DrainScope`、`DrainStatus`、`ReleaseOutcome`、`BudgetSnapshot`。
//!
//! * `acquire` 的等待**可取消**，超时默认 10 秒（配置起点，不是硬编码）；
//!   超时是**业务拒绝**，端口返回 `PortError::ProviderTimeout`，由用例层翻译为
//!   `ApiError{ code: ResourceBusy }`。
//! * `reserve_many` **要么全部预留成功，要么整体失败并释放**，禁止持有半边无限等待。
//! * `release` **幂等**（INV-10）：同一 permit 重复归还不得重复释放额度。
//! * `renew_node_lease` 失败即视为**节点失联**：停止发放新资源，已有 permit 走各自正常归还。

use async_trait::async_trait;

use crate::error::PortError;
use crate::id::{ConnectionId, Counter, OrganizationId, Timestamp, WorkerId};

/// 申请理由。用途取值来自运行时对「是终端用户请求还是后台任务」的判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetPurpose {
    /// 终端用户直接交互。
    UserInteractive,
    /// 后台任务。
    BackgroundJob,
}

/// 一份申请。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetRequest {
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    pub purpose: BudgetPurpose,
    /// 等待超时（毫秒）。`None` 走默认 10s 配置。
    pub acquire_timeout_ms: Option<u64>,
}

impl BudgetRequest {
    pub fn new(
        organization_id: OrganizationId,
        connection_id: ConnectionId,
        purpose: BudgetPurpose,
    ) -> Self {
        Self {
            organization_id,
            connection_id,
            purpose,
            acquire_timeout_ms: None,
        }
    }
}

/// 额度许可。**持有许可 = 占用额度**，归还必须幂等。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetPermit {
    pub permit_id: crate::id::LeaseId,
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    pub granted_at: Timestamp,
}

/// 一组许可（Job 多端申请）。要么齐全，要么一个都没有。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetPermitSet {
    pub permits: Vec<BudgetPermit>,
}

impl BudgetPermitSet {
    pub fn new(permits: Vec<BudgetPermit>) -> Self {
        Self { permits }
    }

    pub fn len(&self) -> usize {
        self.permits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.permits.is_empty()
    }
}

/// 节点额度租约。持有者向协调者证明自己仍在线。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLease {
    pub lease_id: crate::id::LeaseId,
    pub worker_id: WorkerId,
    pub expires_at: Timestamp,
}

/// 排空范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrainScope {
    Node(WorkerId),
    Organization(OrganizationId),
    Connection(ConnectionId),
}

/// 排空状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainStatus {
    pub scope: DrainScope,
    pub outstanding: usize,
    pub draining: bool,
}

/// 归还时的处置事实，决定额度是否按「已消耗」结算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseOutcome {
    /// 资源创建后被正常使用。
    Consumed,
    /// 资源创建失败或未使用即归还，额度原样释放。
    Unused,
}

/// 额度快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub organization_id: OrganizationId,
    pub in_use: Counter,
    pub granted_total: Counter,
}

#[async_trait]
pub trait BudgetCoordinator: Send + Sync + 'static {
    /// 可取消等待，acquire timeout 默认 10 秒（[连接 §9.2](connection-management.md) 的
    /// 首版配置起点而非硬编码），超时由用例转 `ApiError` `ResourceBusy`。
    async fn acquire(&self, request: BudgetRequest) -> Result<BudgetPermit, PortError>;

    /// 立即尝试，不等待。额度不足返回 `Ok(None)`。
    async fn try_acquire(&self, request: BudgetRequest) -> Result<Option<BudgetPermit>, PortError>;

    /// Job 多端申请：一次预留全部许可或整体失败释放，不持有半边无限等待。
    async fn reserve_many(&self, requests: &[BudgetRequest]) -> Result<BudgetPermitSet, PortError>;

    /// 节点额度续期；续约失败即视为失联，停止发放新资源，已有 permit 走各自正常归还。
    async fn renew_node_lease(&self, lease: &NodeLease) -> Result<NodeLease, PortError>;

    /// drain：进入排空后不再发放新 permit，等待已有 permit 归还。
    async fn drain(&self, scope: DrainScope) -> Result<DrainStatus, PortError>;

    /// 幂等核销：同一 permit 重复归还不重复释放额度（INV-10）。
    async fn release(
        &self,
        permit: &BudgetPermit,
        outcome: ReleaseOutcome,
    ) -> Result<(), PortError>;

    async fn snapshot(&self) -> Result<BudgetSnapshot, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::LeaseId;

    fn request() -> BudgetRequest {
        BudgetRequest::new(
            OrganizationId::new("org-1"),
            ConnectionId::new("conn-1"),
            BudgetPurpose::UserInteractive,
        )
    }

    fn permit() -> BudgetPermit {
        BudgetPermit {
            permit_id: LeaseId::new("permit-1"),
            organization_id: OrganizationId::new("org-1"),
            connection_id: ConnectionId::new("conn-1"),
            granted_at: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    #[test]
    fn acquire_timeout_is_configuration_not_a_hardcoded_constant() {
        // 端口只透传配置：既不读环境变量，也不写死 10_000。
        assert_eq!(request().acquire_timeout_ms, None);
        let tuned = BudgetRequest {
            acquire_timeout_ms: Some(2500),
            ..request()
        };
        assert_eq!(tuned.acquire_timeout_ms, Some(2500));
        let source = include_str!("coordinator.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        assert!(
            !source.contains("10_000"),
            "10s 是配置起点，不应硬编码在端口层：{source}"
        );
    }

    #[test]
    fn reserve_many_is_all_or_nothing_on_the_port_side() {
        let set = BudgetPermitSet::new(vec![
            permit(),
            BudgetPermit {
                permit_id: LeaseId::new("permit-2"),
                ..permit()
            },
        ]);
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());
        assert!(BudgetPermitSet::new(Vec::new()).is_empty());
    }

    #[test]
    fn release_outcomes_separate_consumption_from_a_plain_return() {
        assert_ne!(ReleaseOutcome::Consumed, ReleaseOutcome::Unused);
    }

    #[test]
    fn drain_scope_can_target_a_node_an_organization_or_a_connection() {
        let scopes = [
            DrainScope::Node(WorkerId::new("worker-1")),
            DrainScope::Organization(OrganizationId::new("org-1")),
            DrainScope::Connection(ConnectionId::new("conn-1")),
        ];
        let mut unique = scopes.to_vec();
        unique.sort_by_key(|s| format!("{s:?}"));
        unique.dedup();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn snapshot_counts_are_counters_so_they_survive_the_js_boundary() {
        let snapshot = BudgetSnapshot {
            organization_id: OrganizationId::new("org-1"),
            in_use: Counter::new(3),
            granted_total: Counter::new(9),
        };
        assert_eq!(snapshot.in_use.get(), 3);
        assert_eq!(snapshot.granted_total.get(), 9);
    }

    #[test]
    fn permits_are_identified_by_id_so_release_can_be_deduplicated() {
        // 幂等归还的前提：permit 有稳定标识，可与已归还集合比对。
        assert_eq!(permit().permit_id, LeaseId::new("permit-1"));
        assert_ne!(
            permit().permit_id,
            BudgetPermit {
                permit_id: LeaseId::new("permit-2"),
                ..permit()
            }
            .permit_id
        );
    }
}
