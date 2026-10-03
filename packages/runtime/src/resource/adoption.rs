//! 候选替换在 [`ResourceManager`] 上的门面（CM-68）。
//!
//! 候选（candidate）**不进公开目录、不接受执行、不接受订阅**；提交屏障翻转后旧会话
//! 立即不再执行，且只能**取回同一份**新回执。提交成功但目录发布失败时返回
//! `CandidateNotPublished`，绝不允许出现「成功却没有目录记录」的回执。

use crate::connection::{ConfigRevision, ConnectionId, DbSessionId, LeaseId, ResourceId};
use crate::resource::lease::{LeasePurpose, LeaseRecord, LeaseRequest, LeaseState};
use crate::resource::publication::{CommitBarrier, PublishOutcome, ReplacementReceipt};
use crate::resource::replacement::BeginOutcome;
use crate::resource::ResourceError;

impl super::manager::ResourceManager {
    // ---- 候选替换（A3.6 / CM-68） ----

    /// 开始一次候选替换。候选**不进公开目录、不接受执行、不接受订阅**。
    ///
    /// 同一 `idempotency_key` 重试返回 `Resumed`，**不**新建第二条物理连接。
    pub fn begin_replacement(
        &mut self,
        idempotency_key: &str,
        old_session_id: DbSessionId,
        new_session_id: DbSessionId,
        old_resource: ResourceId,
        request: &LeaseRequest,
    ) -> Result<BeginOutcome, ResourceError> {
        if let Some(existing) = self.ledger.candidate(idempotency_key) {
            if existing.old_session_id != old_session_id {
                return Err(ResourceError::DuplicateCandidate(format!(
                    "idempotencyKey {} already binds another old session",
                    idempotency_key
                ))
                .logged("replacement idempotency key collision"));
            }
            return Ok(BeginOutcome::Resumed {
                new_session_id: existing.new_session_id.clone(),
                state: existing.state,
            });
        }
        let now_nanos = self.now_nanos();
        let connection_id = request.pool_key_inputs.connection_id.clone();
        let pool_key = self.current_pool_key(request)?;
        let config_revision = self
            .generations
            .current_config_revision(&connection_id)
            .unwrap_or(ConfigRevision::new(1));
        let candidate_resource = self.transport.open(request)?;
        let lease = LeaseRecord {
            lease_id: LeaseId::new(format!("candidate-{}", idempotency_key)),
            resource_id: candidate_resource.clone(),
            connection_id,
            pool_key,
            purpose: LeasePurpose::FixedSession,
            owner: request.owner.clone(),
            config_revision,
            state: LeaseState::Acquired,
            // 关键：候选在发布前 `published == false`，因此 `accepts_execution` 必然拒绝。
            published: false,
            owner_key: request.owner.hash(),
            created_at_nanos: now_nanos,
            idle_since_nanos: None,
            active_execution: None,
            idle_for_issue: false,
        };
        let new_lease = lease.lease_id.clone();
        self.table.insert(candidate_resource.clone(), lease);
        match self.ledger.begin(
            idempotency_key,
            old_session_id,
            old_resource,
            candidate_resource,
            new_session_id,
            new_lease.clone(),
            config_revision,
        ) {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.table.forget(&new_lease);
                Err(error.logged("begin a replacement candidate"))
            }
        }
    }

    /// 确认替换：提交屏障 + **目录发布**，两者都成功才算成功。
    ///
    /// 目录发布失败 ⇒ 返回 `CandidateNotPublished` 而**不是**一份「成功」回执：
    /// 「提交成功但没有目录记录」绝不能被当成替换成功（§7.4 / CM-68）。
    pub fn confirm_replacement(
        &mut self,
        idempotency_key: &str,
        connection_id: ConnectionId,
        attachment_token: impl Into<String>,
    ) -> Result<ReplacementReceipt, ResourceError> {
        let now_nanos = self.now_nanos();
        let receipt = self
            .ledger
            .commit(idempotency_key, connection_id, attachment_token, now_nanos)
            .map_err(|error| error.logged("commit the replacement barrier"))?;
        match self.ledger.publish(idempotency_key, now_nanos) {
            Ok(PublishOutcome::Published { .. }) => {
                self.mark_candidate_published(idempotency_key)?;
                Ok(receipt)
            }
            Ok(PublishOutcome::Deferred { .. }) => Err(ResourceError::CandidateNotPublished
                .logged("replacement committed but the directory record is missing")),
            Err(error) => Err(error.logged("publish the replacement into the directory")),
        }
    }

    /// 重试目录发布并取回**同一份**回执（幂等恢复）。
    pub fn recover_replacement(
        &mut self,
        idempotency_key: &str,
    ) -> Result<ReplacementReceipt, ResourceError> {
        let now_nanos = self.now_nanos();
        self.ledger
            .publish(idempotency_key, now_nanos)
            .map_err(|error| error.logged("republish a committed replacement"))?;
        let receipt = self
            .ledger
            .recover(idempotency_key)
            .map_err(|error| error.logged("recover a replacement receipt"))?
            .clone();
        if receipt.published_at_nanos.is_none() {
            return Err(ResourceError::CandidateNotPublished
                .logged("replacement has no directory record yet"));
        }
        self.mark_candidate_published(idempotency_key)?;
        Ok(receipt)
    }

    pub(super) fn mark_candidate_published(
        &mut self,
        idempotency_key: &str,
    ) -> Result<(), ResourceError> {
        let lease_id = self
            .ledger
            .candidate(idempotency_key)
            .map(|candidate| candidate.new_lease.clone())
            .ok_or(ResourceError::CandidateNotPublished)?;
        if let Some(entry) = self.table.lease_mut(&lease_id) {
            entry.published = true;
            entry.move_to(LeaseState::InUse)?;
        }
        Ok(())
    }

    /// 提交**之前**失败：旧会话仍然有效，候选被清理掉。
    pub fn abandon_replacement(&mut self, idempotency_key: &str) -> Result<(), ResourceError> {
        let candidate_resource = self
            .ledger
            .abandon(idempotency_key)
            .map_err(|error| error.logged("abandon a replacement candidate"))?;
        if let Some(lease_id) = self
            .ledger
            .candidate(idempotency_key)
            .map(|candidate| candidate.new_lease.clone())
        {
            self.table.forget(&lease_id);
        }
        self.transport.close(&candidate_resource)?;
        Ok(())
    }

    /// 旧会话此刻还能不能执行。提交一旦落成，这里立刻变成 `false`（CM-68：
    /// 「旧 ID 不再执行」只允许有一个事实来源，就是台账里的 `superseded`）。
    pub fn old_session_executable(&self, session_id: &DbSessionId) -> bool {
        self.ledger.old_session_executable(session_id)
    }

    pub fn candidate_barrier(&self, idempotency_key: &str) -> Option<&CommitBarrier> {
        self.ledger
            .candidate(idempotency_key)
            .map(|candidate| &candidate.barrier)
    }
}
