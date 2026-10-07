//! 候选替换（requiresReplacement + 原子发布）。
//!
//! 替换在这里被拆成**三个互不重叠的阶段**，每个阶段有自己的**可观测**证据：
//!
//! ```text
//! begin ──▶ 提交屏障（内存替换操作记录） ──▶ 目录发布
//!  │                                              │
//!  └─ 未提交前：旧会话仍然有效，候选可随时销毁        └─ 发布前：新会话对外**不存在**
//! ```
//!
//! 三条硬性后果：
//!
//! 1. **候选不注册**：候选资源由本模块的台账托管，不进公开目录 / 目录表，
//!    [`CandidateState::Proposed`] 期间不接受执行、不接订阅。
//! 2. **不许「成功但无记录」**：`commit` 只产出**提交证据**，不产出成功回执；
//!    只有 `publisher.publish` 真的落成目录记录才会给出 `PublishOutcome::Published`。
//!    发布失败一律是错误或 `Deferred`，**绝不会**被翻译成成功。
//! 3. **提交后只恢复同一份回执**：[`ReplacementLedger::recover`] 取回的是**同一份**
//!    [`ReplacementReceipt`]（同一 `new_session_id`、同一 `attachment_token`），
//!    绝不新造一个「反正差不多」的新会话。

use std::collections::HashSet;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::connection::{ConfigRevision, DbSessionId, LeaseId, ResourceId};

use crate::resource::publication::{
    CommitBarrier, DirectoryPublisher, Publication, PublishOutcome, ReplacementReceipt,
};
use crate::resource::ResourceError;

/// 候选资源在替换流程里的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateState {
    /// 已建立、已托管，**未提交**：不对外可见，不接受执行与订阅。
    Proposed,
    /// 已提交（内存替换操作记录已落）：旧会话**不再可执行**，新会话**尚未对外可见**。
    Committed,
    /// 已提交且目录记录已落：新会话对外可见，替换完成。
    Published,
    /// 提交前失败并销毁候选：旧会话仍然有效（「预算不足或连接失败 ⇒ 销毁候选，
    /// 保留旧会话」）。
    Abandoned,
}

impl CandidateState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Committed => "committed",
            Self::Published => "published",
            Self::Abandoned => "abandoned",
        }
    }

    /// 只有已发布的候选资源才对公开签发与执行开放。
    pub const fn publicly_usable(self) -> bool {
        matches!(self, Self::Published)
    }

    pub const fn is_committed(self) -> bool {
        matches!(self, Self::Committed | Self::Published)
    }
}

/// 一次候选替换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementCandidate {
    pub idempotency_key: String,
    pub old_session_id: DbSessionId,
    pub old_resource: ResourceId,
    pub candidate_resource: ResourceId,
    pub new_session_id: DbSessionId,
    pub new_lease: LeaseId,
    pub config_revision: ConfigRevision,
    pub barrier: CommitBarrier,
    pub state: CandidateState,
}

impl ReplacementCandidate {
    pub const fn accepts_execution(&self) -> bool {
        self.state.publicly_usable()
    }
}

/// `begin` 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginOutcome {
    /// 新建的候选（物理连接已由调用方建立，本模块只托管账目）。
    Started {
        new_session_id: DbSessionId,
        state: CandidateState,
    },
    /// 同一 `idempotencyKey` 的重试：返回既有候选，**不得**再连一条物理连接。
    Resumed {
        new_session_id: DbSessionId,
        state: CandidateState,
    },
}

/// 替换台账。
pub struct ReplacementLedger {
    entries: IndexMap<String, ReplacementCandidate>,
    receipts: IndexMap<String, ReplacementReceipt>,
    /// 已提交 ⇒ 其旧会话**不再可执行**。这里就是「提交后旧 ID 不执行」的唯一事实来源。
    superseded: HashSet<DbSessionId>,
    publisher: Option<Arc<dyn DirectoryPublisher>>,
}

impl ReplacementLedger {
    pub fn new() -> Self {
        Self {
            entries: IndexMap::new(),
            receipts: IndexMap::new(),
            superseded: HashSet::new(),
            publisher: None,
        }
    }

    pub fn with_publisher(mut self, publisher: Arc<dyn DirectoryPublisher>) -> Self {
        self.publisher = Some(publisher);
        self
    }

    pub fn set_publisher(&mut self, publisher: Arc<dyn DirectoryPublisher>) {
        self.publisher = Some(publisher);
    }

    /// 登记一个候选。
    ///
    /// 幂等：同一 `idempotency_key` 重试返回 [`BeginOutcome::Resumed`]，
    /// **不**建立第二条物理连接（调用方据此跳过 `transport.open`）。
    /// 同一 key 却指向**另一个**旧会话则是缺陷，按 `DuplicateCandidate` 拒绝 ——
    /// 那意味着两次并发的替换撞了同一个幂等键。
    // 八个参数里除 `self` 外全是替换这一次操作必须同时确定的身份量：幂等键 / 旧会话 /
    // 旧资源 / 候选资源 / 新会话 / 新租约 / 配置修订。少任何一个，幂等判定都无从成立。
    // 收成结构体只是把这组信息换到调用点的另一处摆，不改这个方法的语义形状。
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        &mut self,
        idempotency_key: impl Into<String>,
        old_session_id: DbSessionId,
        old_resource: ResourceId,
        candidate_resource: ResourceId,
        new_session_id: DbSessionId,
        new_lease: LeaseId,
        config_revision: ConfigRevision,
    ) -> Result<BeginOutcome, ResourceError> {
        let key = idempotency_key.into();
        if let Some(existing) = self.entries.get(&key) {
            if existing.old_session_id != old_session_id {
                return Err(ResourceError::DuplicateCandidate(format!(
                    "idempotencyKey {} already binds old session {}",
                    key,
                    existing.old_session_id.as_str()
                )));
            }
            return Ok(BeginOutcome::Resumed {
                new_session_id: existing.new_session_id.clone(),
                state: existing.state,
            });
        }
        self.entries.insert(
            key.clone(),
            ReplacementCandidate {
                idempotency_key: key.clone(),
                old_session_id,
                old_resource,
                candidate_resource,
                new_session_id: new_session_id.clone(),
                new_lease,
                config_revision,
                barrier: CommitBarrier::open(),
                state: CandidateState::Proposed,
            },
        );
        Ok(BeginOutcome::Started {
            new_session_id,
            state: CandidateState::Proposed,
        })
    }

    pub fn candidate(&self, idempotency_key: &str) -> Option<&ReplacementCandidate> {
        self.entries.get(idempotency_key)
    }

    pub fn candidate_state(&self, idempotency_key: &str) -> Option<CandidateState> {
        self.entries.get(idempotency_key).map(|c| c.state)
    }

    /// 放行提交屏障并生成回执。**这一步不是「替换成功」**，只是内存提交证据。
    pub fn commit(
        &mut self,
        idempotency_key: &str,
        connection_id: crate::connection::ConnectionId,
        attachment_token: impl Into<String>,
        now_nanos: u64,
    ) -> Result<ReplacementReceipt, ResourceError> {
        if let Some(receipt) = self.receipts.get(idempotency_key) {
            // 幂等：重复确认返回同一份回执。
            return Ok(receipt.clone());
        }
        let candidate = self
            .entries
            .get_mut(idempotency_key)
            .ok_or_else(|| ResourceError::UnknownResource(idempotency_key.to_owned()))?;
        if candidate.state == CandidateState::Abandoned {
            return Err(
                ResourceError::CandidateNotPublished.logged("commit an abandoned candidate")
            );
        }
        let committed_at_nanos = candidate.barrier.commit(now_nanos);
        candidate.state = CandidateState::Committed;
        let receipt = ReplacementReceipt {
            new_session_id: candidate.new_session_id.clone(),
            old_session_id: candidate.old_session_id.clone(),
            connection_id,
            attachment_token: attachment_token.into(),
            config_revision: candidate.config_revision,
            committed_at_nanos,
            published_at_nanos: None,
        };
        self.superseded.insert(candidate.old_session_id.clone());
        self.receipts
            .insert(idempotency_key.to_owned(), receipt.clone());
        Ok(receipt)
    }

    /// 把已提交的候选写进公开目录。
    ///
    /// 未提交 ⇒ `CandidateNotPublished`；没有接目录端口 ⇒ 同样拒绝，
    /// 因为「没有目录记录」绝不等于「替换成功」。
    pub fn publish(
        &mut self,
        idempotency_key: &str,
        now_nanos: u64,
    ) -> Result<PublishOutcome, ResourceError> {
        let receipt = self
            .receipts
            .get(idempotency_key)
            .ok_or(ResourceError::CandidateNotPublished)?
            .clone();
        if receipt.published_at_nanos.is_some() {
            // 幂等：已发布过就返回同一份目录记录。
            return Ok(PublishOutcome::Published {
                publication: Publication {
                    session_id: receipt.new_session_id.clone(),
                    attachment_token: receipt.attachment_token.clone(),
                    published_at_nanos: receipt.published_at_nanos.unwrap_or(now_nanos),
                },
            });
        }
        let publisher = self
            .publisher
            .as_ref()
            .ok_or(ResourceError::CandidateNotPublished)?;
        match publisher.publish(&receipt) {
            Ok(publication) => {
                let stored = self
                    .receipts
                    .get_mut(idempotency_key)
                    .ok_or(ResourceError::CandidateNotPublished)?;
                stored.published_at_nanos = Some(publication.published_at_nanos);
                if let Some(candidate) = self.entries.get_mut(idempotency_key) {
                    candidate.state = CandidateState::Published;
                }
                Ok(PublishOutcome::Published { publication })
            }
            Err(fault) => {
                tracing::warn!(
                    idempotency_key,
                    reason = fault.reason_code(),
                    "replacement committed but directory publish deferred"
                );
                Ok(PublishOutcome::Deferred { fault })
            }
        }
    }

    /// 提交之后、发布之前的恢复：取回**同一份**回执。
    pub fn recover(&self, idempotency_key: &str) -> Result<&ReplacementReceipt, ResourceError> {
        self.receipts
            .get(idempotency_key)
            .ok_or(ResourceError::CandidateNotPublished)
    }

    /// 提交前失败：销毁候选，旧会话仍然有效。
    pub fn abandon(&mut self, idempotency_key: &str) -> Result<ResourceId, ResourceError> {
        let candidate = self
            .entries
            .get_mut(idempotency_key)
            .ok_or_else(|| ResourceError::UnknownResource(idempotency_key.to_owned()))?;
        if candidate.barrier.is_committed() {
            return Err(ResourceError::InvariantBroken(
                "cannot abandon an already committed candidate",
            ));
        }
        candidate.state = CandidateState::Abandoned;
        Ok(candidate.candidate_resource.clone())
    }

    /// 旧会话此刻还能不能执行。提交一旦落成，这里**立即**变成 `false`。
    pub fn old_session_executable(&self, session_id: &DbSessionId) -> bool {
        !self.superseded.contains(session_id)
    }

    pub fn superseded_sessions(&self) -> impl Iterator<Item = &DbSessionId> {
        self.superseded.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ReplacementLedger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::ConnectionId;

    fn ids(tag: &str) -> (DbSessionId, DbSessionId, ConnectionId) {
        (
            DbSessionId::new(format!("db-{tag}-old")),
            DbSessionId::new(format!("db-{tag}-new")),
            ConnectionId::new(format!("conn-{tag}")),
        )
    }

    #[test]
    fn a_candidate_is_not_usable_before_it_is_published() {
        assert!(!CandidateState::Proposed.publicly_usable());
        assert!(!CandidateState::Committed.publicly_usable());
        assert!(CandidateState::Published.publicly_usable());
    }

    #[test]
    fn the_same_idempotency_key_resumes_instead_of_building_a_second_candidate() {
        let (old, new, conn) = ids("dup");
        let mut ledger = ReplacementLedger::new();
        let first = ledger
            .begin(
                "k1",
                old.clone(),
                ResourceId::new("r-old"),
                ResourceId::new("r-new"),
                new.clone(),
                LeaseId::new("l-new"),
                ConfigRevision::new(1),
            )
            .expect("first begin");
        assert!(matches!(first, BeginOutcome::Started { .. }));
        let second = ledger
            .begin(
                "k1",
                old,
                ResourceId::new("r-other"),
                ResourceId::new("r-other"),
                DbSessionId::new("db-dup-other"),
                LeaseId::new("l-other"),
                ConfigRevision::new(1),
            )
            .expect("resumed, not a second candidate");
        assert_eq!(
            second,
            BeginOutcome::Resumed {
                new_session_id: DbSessionId::new("db-dup-new"),
                state: CandidateState::Proposed
            }
        );
        assert_eq!(ledger.len(), 1, "no second candidate may exist");
        assert_eq!(conn.as_str(), "conn-dup");
    }

    #[test]
    fn commit_alone_never_reports_published() {
        let (old, new, conn) = ids("norec");
        let mut ledger = ReplacementLedger::new();
        ledger
            .begin(
                "k2",
                old.clone(),
                ResourceId::new("r-old"),
                ResourceId::new("r-new"),
                new.clone(),
                LeaseId::new("l-new"),
                ConfigRevision::new(1),
            )
            .expect("begin");
        ledger
            .commit("k2", conn, "attach-1", 10)
            .expect("commit writes the in-memory record");
        assert_eq!(
            ledger.candidate_state("k2"),
            Some(CandidateState::Committed),
            "committed but not published is not a completed replacement"
        );
        let err = ledger
            .publish("k2", 20)
            .expect_err("without a directory there is no record, so no success");
        assert_eq!(err.reason(), "resourceCandidateNotPublished");
    }

    #[test]
    fn abandoning_before_commit_keeps_the_old_session_executable() {
        let (old, new, conn) = ids("abandon");
        let mut ledger = ReplacementLedger::new();
        ledger
            .begin(
                "k3",
                old.clone(),
                ResourceId::new("r-old"),
                ResourceId::new("r-new"),
                new,
                LeaseId::new("l-new"),
                ConfigRevision::new(1),
            )
            .expect("begin");
        let destroyed = ledger.abandon("k3").expect("abandon pre-commit");
        assert_eq!(destroyed.as_str(), "r-new");
        assert_eq!(
            ledger.candidate_state("k3"),
            Some(CandidateState::Abandoned)
        );
        assert!(
            ledger.old_session_executable(&old),
            "pre-commit failure must leave the old session usable"
        );
        let _ = conn;
    }

    #[test]
    fn a_committed_candidate_can_never_be_abandoned() {
        let (old, new, conn) = ids("locked");
        let mut ledger = ReplacementLedger::new();
        ledger
            .begin(
                "k4",
                old,
                ResourceId::new("r-old"),
                ResourceId::new("r-new"),
                new,
                LeaseId::new("l-new"),
                ConfigRevision::new(1),
            )
            .expect("begin");
        ledger.commit("k4", conn, "attach-2", 5).expect("commit");
        assert_eq!(
            ledger.abandon("k4"),
            Err(ResourceError::InvariantBroken(
                "cannot abandon an already committed candidate"
            ))
        );
    }
}
