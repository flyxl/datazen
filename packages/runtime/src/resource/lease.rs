//! 池键代（poolKey generation）与租约生命周期状态机。
//!
//! 本文件只做两件事：把「谁是同一条池」这件事说清楚（[`PoolKeyGeneration`]），
//! 以及把一条租约从拿到到死掉的全过程写成**带进入条件与退出条件**的转移表
//! （[`LEASE_TRANSITIONS`]）。转移表既是文档也是实现：`LeaseState::transition`
//! 只认表里的边，表里没有的边一律拒绝，因此不可能出现「只有进入没有退出」的单向死锁。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::connection::{
    ConfigRevision, ConnectionId, ExecutionId, LeaseId, OwnerRef, PoolKeyFingerprint,
    PoolKeyInputs, ResourceId,
};

use crate::resource::transition::lookup_transition;
use crate::resource::ResourceError;

/// 租约用途。§9.1 资源策略表在宿主侧的投影，决定归还前要跑哪些检查。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeasePurpose {
    /// 固定会话：长时间持有，只能由显式关闭结束（QueryPanel、导出 Job 阶段…）。
    /// §7.5 步骤 5：v1 里用户自开的任意 SQL 会话关闭时直接关物理资源。
    FixedSession,
    /// 短操作租约：跑完即归还，归还前必须走完 §9.4 的全部检查并复位回 `Clean`。
    ShortOperation,
    /// 元数据资源：只读元数据查询，签发节奏与短操作池一致。
    Metadata,
}

impl LeasePurpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedSession => "fixedSession",
            Self::ShortOperation => "shortOperation",
            Self::Metadata => "metadata",
        }
    }
}

/// 池键代。
///
/// `PoolKeyFingerprint::derive`（connection/types.rs）已经把 **database**（经
/// `NamespaceTarget` 的四个层）与 **policy**（`policy_isolation_key`）折叠进指纹，
/// 所以 A3.4「按库与策略分片」的第一层由它承担。
///
/// 但 §9.6 要求「凭据 / 网络路由 / ACL 任一变化 ⇒ 新键」，
/// 而 runtime 的指纹里**没有**这三个代的位置（`connection/types.rs` 是冻结的、不可重定义）。
/// 因此宿主在指纹之上再带两代：凭据代与网络路由代。三层合起来才是完整的池键代：
///
/// ```text
/// PoolKeyGeneration = (fingerprint(database + policy), credential_revision, network_route_revision)
/// ```
///
/// 任一分量变化 ⇒ 新键 ⇒ **旧空闲不再被签发并被关闭**（CM-67、CM-38）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoolKeyGeneration {
    pub fingerprint: PoolKeyFingerprint,
    pub credential_revision: u64,
    pub network_route_revision: u64,
}

impl PoolKeyGeneration {
    /// 从一次租约请求派生池键代。**不含调用者自填的凭据**——凭据只能以代号出现。
    pub fn of(request: &LeaseRequest) -> Self {
        Self {
            fingerprint: PoolKeyFingerprint::derive(&request.pool_key_inputs),
            // 未声明代号（`None`）的申请按**当前代**落键：宿主自己取新材料，
            // 调用方手里并不攥着凭据。声明了代号就必须与当前代相符，
            // 否则是过期材料 —— 那由 `ResourceManager::acquire` 拒绝，不是这里。
            credential_revision: request.credential_revision.unwrap_or(0),
            network_route_revision: request.network_route_revision.unwrap_or(0),
        }
    }

    pub const fn credential_revision(&self) -> u64 {
        self.credential_revision
    }

    pub const fn network_route_revision(&self) -> u64 {
        self.network_route_revision
    }
}

/// 一次租约申请。**申请不是承诺**：`ResourceManager::acquire` 才会真正落表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRequest {
    /// 已含 database + policy 的池键输入（connection/types.rs，不重定义）。
    pub pool_key_inputs: PoolKeyInputs,
    /// 凭据代号（只传代号，不传凭据本身）。`None` = 不声明，宿主取当前代；
    /// `Some(r)` = 声明「我按凭据代 r 建模」，与当前代不符即拒绝。
    pub credential_revision: Option<u64>,
    /// 网络路由代号，语义同上。
    pub network_route_revision: Option<u64>,
    pub owner: OwnerRef,
    pub purpose: LeasePurpose,
    /// A3.5：`Some(r)` 表示「我按 revision r 的配置建模」。宿主当时的实际代与之不符时，
    /// 按显式策略**冲突**，绝不静默改写；`None` 表示不绑定代（允许拿当前代）。
    pub expected_config_revision: Option<ConfigRevision>,
    /// 同 key 重试返回同一条租约/同一份回执，不建第二条连接（§7.4 幂等）。
    pub idempotency_key: Option<String>,
    /// §9.2 短操作池的取用超时；超时按 `ResourceBusy` 取消。
    pub acquire_timeout_ms: u64,
    /// `Some(key)` 表示本次申请建的是**候选资源**：未提交前不对外可见、不接受执行。
    pub candidate_for: Option<String>,
}

impl LeaseRequest {
    /// 一个不带任何可选字段的申请；测试与上层按需补齐。
    pub fn new(pool_key_inputs: PoolKeyInputs, owner: OwnerRef, purpose: LeasePurpose) -> Self {
        Self {
            pool_key_inputs,
            credential_revision: None,
            network_route_revision: None,
            owner,
            purpose,
            expected_config_revision: None,
            idempotency_key: None,
            acquire_timeout_ms: 10_000,
            candidate_for: None,
        }
    }

    pub fn at_credential_revision(mut self, revision: u64) -> Self {
        self.credential_revision = Some(revision);
        self
    }

    pub fn at_network_route_revision(mut self, revision: u64) -> Self {
        self.network_route_revision = Some(revision);
        self
    }

    pub fn expecting_config_revision(mut self, revision: ConfigRevision) -> Self {
        self.expected_config_revision = Some(revision);
        self
    }

    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn as_candidate_for(mut self, idempotency_key: impl Into<String>) -> Self {
        let idempotency_key = idempotency_key.into();
        self.idempotency_key = Some(idempotency_key.clone());
        self.candidate_for = Some(idempotency_key);
        self
    }

    pub const fn is_candidate(&self) -> bool {
        self.candidate_for.is_some()
    }

    pub fn pool_key(&self) -> PoolKeyGeneration {
        PoolKeyGeneration::of(self)
    }
}

/// 租约生命周期状态。
///
/// 终态是 `Closed`：转移表里 `Closed` **没有任何出边**，所以一次关闭就是一次关闭，
/// 不存在「关完了还能再被复活成 InUse」的死锁。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeaseState {
    /// 已落表、已占用预算，但还没有任何执行挂上去。
    Acquired,
    /// 有执行或固定会话在用。
    InUse,
    /// 正在归还：已判定不能（或不需要）复用，走关闭/复位路径。
    Closing,
    /// 隔离：宿主条件或驱动结论不满足，禁止复用，只能走关闭。
    Quarantined,
    /// 终态。
    Closed,
}

/// 让 `IllegalTransition { from, to }` 的 `Display` 直接可用（thiserror 需要）。
impl fmt::Display for LeaseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl LeaseState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Acquired => "acquired",
            Self::InUse => "inUse",
            Self::Closing => "closing",
            Self::Quarantined => "quarantined",
            Self::Closed => "closed",
        }
    }

    pub const ALL: [LeaseState; 5] = [
        Self::Acquired,
        Self::InUse,
        Self::Closing,
        Self::Quarantined,
        Self::Closed,
    ];

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Closed)
    }

    /// 状态转移。**只认转移表里的边**：表外一律 `IllegalTransition`。
    pub fn transition(self, to: LeaseState) -> Result<LeaseState, ResourceError> {
        if lookup_transition(self, to).is_some() {
            Ok(to)
        } else {
            Err(ResourceError::IllegalTransition { from: self, to })
        }
    }
}

/// 一条租约在宿主台账里的记录。
///
/// 注意这里出现的是 `ResourceId`（**账目**）而不是任何句柄：台账管的是「谁欠着预算、
/// 谁没被关」，不是「socket 怎么连的」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRecord {
    pub lease_id: LeaseId,
    pub resource_id: ResourceId,
    /// 归属的连接配置 id（落盘持久化的那一份）。禁用/删除按它批量处置（CM-39）。
    pub connection_id: ConnectionId,
    pub pool_key: PoolKeyGeneration,
    pub purpose: LeasePurpose,
    pub owner: OwnerRef,
    pub config_revision: ConfigRevision,
    pub state: LeaseState,
    /// 候选资源在提交前**不发布**：`published == false` 时不得被公开签发、不得接受执行。
    pub published: bool,
    /// 归属的稳定键（`OwnerRef::hash()`），禁用/删除按它批量处置。
    pub owner_key: String,
    pub created_at_nanos: u64,
    /// 进入空闲的时刻；`None` 表示不空闲。§9.2 的 60s TTL 从这里算。
    pub idle_since_nanos: Option<u64>,
    /// 当前挂在这条租约上的执行。
    pub active_execution: Option<ExecutionId>,
    /// 是否仍可被再次签发（回池后才为真）。
    pub idle_for_issue: bool,
}

impl LeaseRecord {
    pub fn is_reusable(&self) -> bool {
        self.state == LeaseState::Acquired && self.published && self.idle_for_issue
    }

    pub fn accepts_execution(&self) -> Result<(), ResourceError> {
        if !self.published {
            return Err(ResourceError::CandidateNotPublished);
        }
        if !matches!(self.state, LeaseState::Acquired | LeaseState::InUse) {
            return Err(ResourceError::HostConditionUnmet("lease is not serving"));
        }
        Ok(())
    }

    /// 走一次受表约束的转移，并同步派生字段。
    pub fn move_to(&mut self, to: LeaseState) -> Result<(), ResourceError> {
        let next = self.state.transition(to)?;
        self.state = next;
        match next {
            LeaseState::Closing | LeaseState::Quarantined | LeaseState::Closed => {
                self.idle_for_issue = false;
                self.idle_since_nanos = None;
                self.active_execution = None;
            }
            LeaseState::Acquired => {}
            LeaseState::InUse => self.idle_since_nanos = None,
        }
        Ok(())
    }

    pub fn mark_idle(&mut self, now_nanos: u64) {
        self.idle_for_issue = true;
        self.idle_since_nanos = Some(now_nanos);
        self.active_execution = None;
    }

    /// 空闲 TTL 是否已到期（§9.2：60 秒）。没有进入空闲的租约永不到期。
    pub fn idle_expired_at(&self, now_nanos: u64, ttl_seconds: u64) -> bool {
        let Some(since) = self.idle_since_nanos else {
            return false;
        };
        now_nanos.saturating_sub(since)
            >= ttl_seconds.saturating_mul(crate::resource::NANOS_PER_SECOND)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::transition::LEASE_TRANSITIONS;

    #[test]
    fn the_terminal_state_has_no_exit() {
        for to in LeaseState::ALL {
            assert!(
                lookup_transition(LeaseState::Closed, to).is_none(),
                "Closed -> {} must not exist: a closed lease can never come back",
                to.as_str()
            );
        }
        let err = LeaseState::Closed
            .transition(LeaseState::InUse)
            .expect_err("a closed lease must not be reusable");
        assert_eq!(err.reason(), "resourceIllegalTransition");
    }

    /// 转移表上的可达性：从 `from` 出发能不能走到 `to`。
    fn reaches(from: LeaseState, to: LeaseState) -> bool {
        let mut frontier = vec![from];
        let mut seen = vec![from];
        while let Some(state) = frontier.pop() {
            if state == to {
                return true;
            }
            for rule in LEASE_TRANSITIONS.iter().filter(|rule| rule.from == state) {
                if !seen.contains(&rule.to) {
                    seen.push(rule.to);
                    frontier.push(rule.to);
                }
            }
        }
        false
    }

    #[test]
    fn every_non_terminal_state_can_still_exit_somewhere() {
        for from in LeaseState::ALL {
            let exits: Vec<_> = LEASE_TRANSITIONS
                .iter()
                .filter(|rule| rule.from == from)
                .map(|rule| rule.to)
                .collect();
            if from.is_terminal() {
                // 不变量 1：终态没有出边 —— 一次关闭就是一次关闭，不存在「关完又复活」。
                assert!(
                    exits.is_empty(),
                    "the terminal state {} must have no exit edge",
                    from.as_str()
                );
                continue;
            }
            // 不变量 2：每个非终态都至少有一条出边（不存在只有进入没有退出的单向死锁）。
            assert!(!exits.is_empty(), "{} has no exit edge", from.as_str());
            // 不变量 3：不要求「一步」跳到 Closed —— 关闭必须先跑握手、经过 Closing；
            // 要求的是**可达**，删掉 Acquired→Closing 或 Closing→Closed 任何一条都会红。
            assert!(
                reaches(from, LeaseState::Closed),
                "{} cannot reach the terminal state",
                from.as_str()
            );
        }
    }

    #[test]
    fn every_transition_rule_states_entry_in_state_and_exit_conditions() {
        for rule in LEASE_TRANSITIONS {
            assert!(!rule.enters_when.trim().is_empty(), "{:?}", rule);
            assert!(!rule.behaves_as.trim().is_empty(), "{:?}", rule);
            assert!(!rule.exits_when.trim().is_empty(), "{:?}", rule);
        }
        assert_eq!(LEASE_TRANSITIONS.len(), 9);
    }

    #[test]
    fn the_five_required_states_are_all_present_and_wired() {
        // A3.2 至少要有这条主链，且五个状态都得有文档化的归属。
        let mut state = LeaseState::Acquired;
        for next in [
            LeaseState::InUse,
            LeaseState::Closing,
            LeaseState::Quarantined,
            LeaseState::Closed,
        ] {
            state = state
                .transition(next)
                .expect("the main chain must be legal");
        }
        assert_eq!(state, LeaseState::Closed);
        assert!(LeaseState::ALL.contains(&LeaseState::Quarantined));
    }

    #[test]
    fn illegal_jumps_are_refused_rather_than_silently_applied() {
        let cases = [
            (LeaseState::Acquired, LeaseState::Closed),
            (LeaseState::InUse, LeaseState::Closed),
            (LeaseState::Quarantined, LeaseState::Closing),
            (LeaseState::Closing, LeaseState::Acquired),
            (LeaseState::Closing, LeaseState::InUse),
            (LeaseState::Quarantined, LeaseState::InUse),
            (LeaseState::Quarantined, LeaseState::Acquired),
        ];
        for (from, to) in cases {
            assert_eq!(
                from.transition(to),
                Err(ResourceError::IllegalTransition { from, to }),
                "{:?} -> {:?} must be refused",
                from,
                to
            );
        }
    }
}
