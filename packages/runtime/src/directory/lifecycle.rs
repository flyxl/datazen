//! 会话生命周期：开启 / 解析 / 挂载 / 执行 / 关闭 / 到期。
//!
//! [`InMemorySessionDirectory`]（`directory.rs`）实现的是**端口形状**——七个方法，
//! 里面没有任何便利构造。这层补的是**日常用法**：开一个会话要顺手拿到令牌、
//! 要能挂载要能挂心跳、要能给事务装期限、要能标出执行中。
//!
//! 两条贯穿全局的规矩：
//!
//! 1. **不返回半成品。** 任何一步不成立就整体 `Err`，绝不出现「登记失败但调用方
//!    以为拿到了句柄」这种情况。
//! 2. **生命周期动作不改期限。** `attach` / `heartbeat` / `touch` / `begin_execution`
//!    一个期限都不碰；只有显式的 `arm_*` / `clear_*` 动期限，裁决只有 `adjudicate` 动状态。

use std::time::Duration;

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    AttachmentToken, ClientInstanceId, ConnectionId, DbSessionId, ExecutionId, JobId,
    OrganizationId, PrincipalId, RuntimeEpoch, Timestamp, WorkerId,
};
use datazen_platform_api::ports::session_directory::SessionOwner;

use super::attachment::{AttachmentOutcome, AttachmentRejection, AttachmentRequest};
use super::commit::{CommitStatus, RecordState, ReplacementOperationKey, UnknownCommitOutcome};
use super::directory::{InMemorySessionDirectory, Inner};
use super::entry::{
    Adjudication, DirectoryEntry, InvalidationRecord, RouteRejection, SessionSnapshot,
};
use super::ttl::{DeadlineKind, DeadlineSet};
use super::{AttachmentClaim, MonoInstant, SessionHandle};

/// 开一个会话要凑齐的字段。**刻意不含任何连接凭据**——
/// 目录条目只有 owner / epoch / TTL 三样东西，这里就不给凭据留位置。
#[derive(Debug, Clone)]
pub struct SessionDraft {
    pub organization_id: OrganizationId,
    pub principal_id: PrincipalId,
    pub connection_id: ConnectionId,
    pub owner: OwnerRef,
    pub worker_id: WorkerId,
    pub resource_epoch: u64,
}

impl SessionDraft {
    pub fn new(
        organization_id: OrganizationId,
        principal_id: PrincipalId,
        connection_id: ConnectionId,
        owner: OwnerRef,
        worker_id: WorkerId,
    ) -> Self {
        Self {
            organization_id,
            principal_id,
            connection_id,
            owner,
            worker_id,
            resource_epoch: 0,
        }
    }

    pub fn with_resource_epoch(mut self, resource_epoch: u64) -> Self {
        self.resource_epoch = resource_epoch;
        self
    }

    fn into_owner(
        self,
        db_session_id: DbSessionId,
        runtime_epoch: RuntimeEpoch,
        registered_at: Timestamp,
    ) -> SessionOwner {
        SessionOwner {
            db_session_id,
            organization_id: self.organization_id,
            principal_id: self.principal_id,
            connection_id: self.connection_id,
            owner: self.owner,
            worker_id: self.worker_id,
            runtime_epoch,
            resource_epoch: self.resource_epoch,
            last_business_activity: registered_at,
        }
    }
}

/// 会话里有没有正在跑的查询。用于替换前的排空判断：没有活跃执行才允许原子切换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOccupancy {
    Idle,
    Busy(ExecutionId),
}

impl ExecutionOccupancy {
    pub const fn is_idle(&self) -> bool {
        matches!(self, ExecutionOccupancy::Idle)
    }

    pub fn execution_id(&self) -> Option<&ExecutionId> {
        match self {
            ExecutionOccupancy::Idle => None,
            ExecutionOccupancy::Busy(execution_id) => Some(execution_id),
        }
    }
}

impl InMemorySessionDirectory {
    /// 开一个会话：生成 `dbSessionId`、登记、发 attachment 令牌。
    ///
    /// 令牌原文**只在这里出现一次**，返回值拿走即散；目录侧只留摘要。
    /// 生成撞上已占用的 ID 就**重新生成**再试；重试耗尽返回 `Err`，
    /// 绝不会「换个 ID 蒙混过关」，也绝不会「返回成功但其实没登记」。
    pub fn open_session(
        &self,
        draft: SessionDraft,
        runtime_epoch: RuntimeEpoch,
    ) -> Result<(SessionHandle, AttachmentToken), PortError> {
        let now = self.now();
        let registered_at = self.clock.project(now);
        let db_session_id = self.ids.generate_distinct(
            |candidate| self.is_taken(candidate),
            super::id::MAX_ID_GENERATION_ATTEMPTS,
        )?;

        let owner = draft.into_owner(db_session_id.clone(), runtime_epoch, registered_at);
        let handle = self.do_register(owner)?;
        let token = super::attachment::new_attachment_token(&self.entropy);
        let digest = super::attachment::digest_token(&token);

        let mut inner = self.lock_inner();
        inner.digests.insert(db_session_id, digest);
        let connection_id = inner
            .entries
            .get(&handle.db_session_id)
            .map(|entry| entry.owner().connection_id.clone());
        drop(inner);

        tracing::info!(
            db_session_id = %handle.db_session_id,
            runtime_epoch = %handle.runtime_epoch,
            ?connection_id,
            "session opened; directory holds no connection and no credential"
        );
        Ok((handle, token))
    }

    /// 给一个**已存在**的条目补签 attachment 令牌。
    ///
    /// # 为什么 `open_session` 之外还要这条路
    ///
    /// §7.4-6 的替换里，新会话是**候选**：它在屏障上被创建、被发布的那一刻，
    /// 还没有任何人持有它的令牌，所以走不了 `open_session`（那条路会自己发号，
    /// 并且在登记的同一刻把令牌交出去）。提交协议也**不会**顺带签发：
    /// `Prepared` 不写摘要，`Committed` 只发布条目。没有这一下，
    /// §7.4 回执里的 `attachmentToken` 就是个谁也用不了的字段。
    ///
    /// 规则与 `open_session` 完全一致：原文只在这里出现一次，目录侧只留摘要。
    /// 重复签发会**换掉**旧摘要——旧令牌随即作废，不存在两枚令牌同时有效的窗口。
    /// 条目没发布（仍在屏障上）或 epoch 对不上时一律拒签，
    /// 免得「还没提交就拿到了新会话的令牌」。
    pub fn issue_attachment_token(
        &self,
        handle: &SessionHandle,
    ) -> Result<AttachmentToken, AttachmentRejection> {
        if let Some(rejection) = self.dead_epoch(handle) {
            return Err(AttachmentRejection::NotRoutable(rejection));
        }
        let mut inner = self.lock_inner();
        let entry = inner
            .entries
            .get(&handle.db_session_id)
            .ok_or(AttachmentRejection::NotRoutable(RouteRejection::Unknown))?;
        entry
            .route(handle)
            .map_err(AttachmentRejection::NotRoutable)?;

        let token = super::attachment::new_attachment_token(&self.entropy);
        let digest = super::attachment::digest_token(&token);
        inner.digests.insert(handle.db_session_id.clone(), digest);
        tracing::info!(
            db_session_id = %handle.db_session_id,
            runtime_epoch = %handle.runtime_epoch,
            "attachment token issued for an existing entry; directory keeps only the digest"
        );
        Ok(token)
    }

    /// 解析句柄：拿到当前 owner。epoch 不对就是显式失败，**绝不**退回配置去找一个替代会话。
    pub fn resolve(&self, handle: &SessionHandle) -> Result<SessionOwner, PortError> {
        self.alive(handle)?;
        let inner = self.lock_inner();
        let entry = inner.entries.get(&handle.db_session_id).ok_or_else(|| {
            PortError::NotFound(format!(
                "stale session handle: dbSessionId={} runtimeEpoch={}",
                handle.db_session_id, handle.runtime_epoch
            ))
        })?;
        entry
            .route(handle)
            .map_err(|rejection| rejection.to_port_error(&handle.db_session_id))?;
        Ok(entry.owner().clone())
    }

    /// 当前到期投影（最早期限的 UTC）。执行中且无适用期限时为 `None`。
    pub fn expiration_projection(
        &self,
        handle: &SessionHandle,
    ) -> Result<Option<Timestamp>, PortError> {
        let inner = self.lock_inner();
        let entry = self.entry_of(&inner, handle)?;
        Ok(entry.expiration_projection(self.clock.as_ref()))
    }

    /// 挂载。必须同时出示令牌与归属身份；两者缺一即拒。
    pub fn attach(
        &self,
        request: &AttachmentRequest,
    ) -> Result<AttachmentOutcome, AttachmentRejection> {
        if let Some(rejection) = self.dead_epoch(&request.handle) {
            return Err(AttachmentRejection::NotRoutable(rejection));
        }
        let mut inner = self.lock_inner();
        let db_session_id = request.handle.db_session_id.clone();
        let stored = inner.digests.get(&db_session_id).copied();
        let entry = inner
            .entries
            .get_mut(&db_session_id)
            .ok_or(AttachmentRejection::NotRoutable(RouteRejection::Unknown))?;
        super::attachment::authorize_attachment(entry, request, stored, self.clock.as_ref())
    }

    /// 客户端页签挂载的常用形式。
    pub fn attach_from_client(
        &self,
        handle: &SessionHandle,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        token: AttachmentToken,
    ) -> Result<AttachmentOutcome, AttachmentRejection> {
        let request = AttachmentRequest::new(
            handle.clone(),
            principal_id,
            AttachmentClaim::Client(client_instance_id),
            Some(token),
        );
        self.attach(&request)
    }

    /// Job 执行体挂载的常用形式。
    pub fn attach_from_job(
        &self,
        handle: &SessionHandle,
        principal_id: PrincipalId,
        job_id: JobId,
        token: AttachmentToken,
    ) -> Result<AttachmentOutcome, AttachmentRejection> {
        let request = AttachmentRequest::new(
            handle.clone(),
            principal_id,
            AttachmentClaim::Job(job_id),
            Some(token),
        );
        self.attach(&request)
    }

    /// 摘掉挂载。**不**改任何期限——重复 detach 也不会续命。
    pub fn detach(&self, handle: &SessionHandle) -> Result<(), PortError> {
        let mut inner = self.lock_inner();
        let entry = self.entry_of_mut(&mut inner, handle)?;
        entry.detach();
        Ok(())
    }

    /// 心跳 / 读状态。**不**推任何期限，只记业务活动时间。
    pub fn heartbeat(&self, handle: &SessionHandle, at: Timestamp) -> Result<(), PortError> {
        self.do_touch(handle, at)
    }

    /// 给事务装空闲期限。从 `armed_at` 起算 `ttl`。
    pub fn arm_transaction_idle(
        &self,
        handle: &SessionHandle,
        ttl: Duration,
        armed_at: MonoInstant,
    ) -> Result<(), PortError> {
        self.arm(handle, DeadlineKind::TransactionIdle, armed_at, ttl)
    }

    /// 给整个会话装空闲期限。
    pub fn arm_session_idle(
        &self,
        handle: &SessionHandle,
        ttl: Duration,
        armed_at: MonoInstant,
    ) -> Result<(), PortError> {
        self.arm(handle, DeadlineKind::SessionIdle, armed_at, ttl)
    }

    /// 给断连装宽限期限。
    pub fn arm_disconnect_grace(
        &self,
        handle: &SessionHandle,
        ttl: Duration,
        armed_at: MonoInstant,
    ) -> Result<(), PortError> {
        self.arm(handle, DeadlineKind::DisconnectGrace, armed_at, ttl)
    }

    /// 摘掉事务空闲期限（提交 / 回滚之后）。
    pub fn clear_transaction_idle(&self, handle: &SessionHandle) -> Result<(), PortError> {
        self.cancel(handle, DeadlineKind::TransactionIdle)
    }

    /// 摘掉断连宽限。
    pub fn clear_disconnect_grace(&self, handle: &SessionHandle) -> Result<(), PortError> {
        self.cancel(handle, DeadlineKind::DisconnectGrace)
    }

    /// 当前挂着的期限。测试用它区分「哪个期限在起作用」。
    pub fn deadlines(&self, handle: &SessionHandle) -> Result<DeadlineSet, PortError> {
        let inner = self.lock_inner();
        let entry = self.entry_of(&inner, handle)?;
        Ok(entry.deadlines().clone())
    }

    /// 标出执行开始。会话不可路由或已有执行在跑时都失败——**不会**悄悄顶掉上一个执行。
    pub fn begin_execution(
        &self,
        handle: &SessionHandle,
        execution_id: ExecutionId,
    ) -> Result<(), PortError> {
        let mut inner = self.lock_inner();
        let entry = self.entry_of_mut(&mut inner, handle)?;
        entry
            .begin_execution(execution_id.clone())
            .map_err(|occupied| {
                PortError::cas_conflict("session", format!("execution busy: {occupied}"))
            })
    }

    /// 标出执行结束。只有对得上号的执行才算结束。
    pub fn end_execution(
        &self,
        handle: &SessionHandle,
        execution_id: &ExecutionId,
    ) -> Result<(), PortError> {
        let mut inner = self.lock_inner();
        let entry = self.entry_of_mut(&mut inner, handle)?;
        entry.end_execution(execution_id);
        Ok(())
    }

    pub fn execution_occupancy(
        &self,
        handle: &SessionHandle,
    ) -> Result<ExecutionOccupancy, PortError> {
        let inner = self.lock_inner();
        let entry = self.entry_of(&inner, handle)?;
        Ok(match entry.active_execution() {
            Some(execution_id) => ExecutionOccupancy::Busy(execution_id.clone()),
            None => ExecutionOccupancy::Idle,
        })
    }

    /// 只读观测投影：给测试与诊断看，不回写任何状态，也**不落盘**。
    pub fn observe(&self, handle: &SessionHandle) -> Result<SessionSnapshot, PortError> {
        let inner = self.lock_inner();
        let entry = self.entry_of(&inner, handle)?;
        Ok(entry.observe(self.clock.as_ref()))
    }

    /// 条目此刻是否可路由。epoch 不对、屏障、关闭中、已关闭都返回 `false`。
    pub fn is_routable(&self, handle: &SessionHandle) -> bool {
        self.dead_epoch(handle).is_none()
            && self
                .lock_inner()
                .entries
                .get(&handle.db_session_id)
                .is_some_and(|entry| entry.route(handle).is_ok() && entry.is_routable())
    }

    pub fn contains(&self, db_session_id: &DbSessionId) -> bool {
        self.lock_inner().entries.contains_key(db_session_id)
    }

    pub fn len(&self) -> usize {
        self.lock_inner().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 内部状态版本号。只用于观测与「状态确实动过」的断言。
    pub fn revision(&self) -> u64 {
        self.lock_inner().revision
    }

    /// 按 operation key 查替换结果。屏障挡着的时候，**这是唯一的解法**。
    ///
    /// 屏障挡的是调用方，不是服务端：切换动作早已原子落定，缺的只是一次确认。
    /// 因此这里给出的是确定答案，而不是让调用方在 Committed / RolledBack 之间二选一；
    /// 无论答案是什么，返回之前两边都会落到「只有一个可路由」的终态。
    pub fn commit_status(&self, key: &ReplacementOperationKey) -> CommitStatus {
        let mut inner = self.lock_inner();
        let (truth, new_handle, old) = match inner.operations.get(key) {
            // `Prepared` 还没有落定结果，不是真相——调用方该看到 `Pending`。
            Some(record) => match record.state() {
                RecordState::Undecided => (
                    record.undecided_truth(),
                    record.new_handle(),
                    record.old().clone(),
                ),
                RecordState::Prepared => (None, record.new_handle(), record.old().clone()),
                settled => (Some(settled), record.new_handle(), record.old().clone()),
            },
            None => return CommitStatus::Pending,
        };
        let Some(truth) = truth else {
            return CommitStatus::Pending;
        };

        match truth {
            RecordState::Committed => {
                // 补放行必须是**幂等**的（`publish_from_barrier`），不能无条件 `publish`：
                // 这一格会被每一次重试走一遍，而候选条目的当前态只有三种可能——
                // 还在屏障里（放行没跑到，补上）、已经可路由（放行早跑过了）、
                // 已经被调用方关闭或判死（终态）。第三种一旦被 `publish` 拉回可路由，
                // 重试就会给一枚「替换已提交」的回执，而那个 id 上什么都没有。
                if let Some(new_entry) = inner.entries.get_mut(&new_handle.db_session_id) {
                    new_entry.publish_from_barrier();
                }
                if let Some(old_entry) = inner.entries.get_mut(&old.db_session_id) {
                    old_entry.close(super::entry::ClosureReason::ReplacedBy(new_handle.clone()));
                }
                if let Some(record) = inner.operations.get_mut(key) {
                    record.set_state(RecordState::Committed);
                }
                inner.revision = inner.revision.wrapping_add(1);
                tracing::info!(%key, "replacement outcome resolved to committed by status query");
                CommitStatus::Committed { handle: new_handle }
            }
            _ => {
                inner.entries.remove(&new_handle.db_session_id);
                inner.digests.remove(&new_handle.db_session_id);
                if let Some(old_entry) = inner.entries.get_mut(&old.db_session_id) {
                    old_entry.restore();
                }
                if let Some(record) = inner.operations.get_mut(key) {
                    record.set_state(RecordState::RolledBack);
                }
                inner.revision = inner.revision.wrapping_add(1);
                tracing::info!(%key, "replacement outcome resolved to rolled back by status query");
                CommitStatus::RolledBack { restored: old }
            }
        }
    }

    /// 让下一次替换提交的应答「丢失」，用来确定性地踩到未知结果分支。
    ///
    /// 生产路径永远不调用它（不 arm 即等价于不注入）。它存在的唯一理由是让
    /// 「结果未知 → 举屏障 → 按 key 查」这条路径可以被测试覆盖，
    /// 而不是用真实超时去复现一个必须立刻失败的分支。
    pub fn lose_next_commit_reply(&self, outcome: Option<UnknownCommitOutcome>) {
        match self.lost_commit_reply.lock() {
            Ok(mut guard) => *guard = outcome,
            Err(poisoned) => *poisoned.into_inner() = outcome,
        }
    }

    pub fn invalidation_record(&self, db_session_id: &DbSessionId) -> Option<InvalidationRecord> {
        self.lock_inner().invalidations.get(db_session_id).cloned()
    }

    /// 判定某条目此刻是否应因期限而关闭。**串行**执行：同一把锁，同一个时刻。
    pub fn adjudicate(&self, handle: &SessionHandle) -> Result<Adjudication, PortError> {
        let now = self.now();
        let mut inner = self.lock_inner();
        let entry = self.entry_of_mut(&mut inner, handle)?;
        let adjudication = entry.adjudicate(now);
        let closure = entry.closure().map(super::entry::ClosureReason::code);
        if adjudication.is_expired() {
            inner.revision = inner.revision.wrapping_add(1);
            tracing::info!(
                db_session_id = %handle.db_session_id,
                closure,
                "deadline reached; session closed for routing"
            );
        }
        Ok(adjudication)
    }

    // ---------------------------------------------------------------- 内部

    fn is_taken(&self, candidate: &DbSessionId) -> bool {
        self.lock_inner().entries.contains_key(candidate)
    }

    fn arm(
        &self,
        handle: &SessionHandle,
        kind: DeadlineKind,
        armed_at: MonoInstant,
        ttl: Duration,
    ) -> Result<(), PortError> {
        let at = armed_at + ttl;
        let mut inner = self.lock_inner();
        let entry = self.entry_of_mut(&mut inner, handle)?;
        entry
            .arm(kind, at)
            .map_err(|rejection| rejection.to_port_error(&handle.db_session_id))
    }

    fn cancel(&self, handle: &SessionHandle, kind: DeadlineKind) -> Result<(), PortError> {
        let mut inner = self.lock_inner();
        let entry = self.entry_of_mut(&mut inner, handle)?;
        entry.cancel_deadline(kind);
        Ok(())
    }

    /// 上一代 worker 的句柄一律显式失败：绝不替它去配置里翻一个「看起来差不多的」会话顶上去。
    fn alive(&self, handle: &SessionHandle) -> Result<(), PortError> {
        match self.dead_epoch(handle) {
            Some(rejection) => Err(rejection.to_port_error(&handle.db_session_id)),
            None => Ok(()),
        }
    }

    fn entry_of<'a>(
        &self,
        inner: &'a Inner,
        handle: &SessionHandle,
    ) -> Result<&'a DirectoryEntry, PortError> {
        self.alive(handle)?;
        let entry = inner.entries.get(&handle.db_session_id).ok_or_else(|| {
            PortError::NotFound(format!("unknown db session id: {}", handle.db_session_id))
        })?;
        entry
            .route(handle)
            .map_err(|rejection| rejection.to_port_error(&handle.db_session_id))?;
        Ok(entry)
    }

    fn entry_of_mut<'a>(
        &self,
        inner: &'a mut Inner,
        handle: &SessionHandle,
    ) -> Result<&'a mut DirectoryEntry, PortError> {
        self.alive(handle)?;
        let entry = inner
            .entries
            .get_mut(&handle.db_session_id)
            .ok_or_else(|| {
                PortError::NotFound(format!("unknown db session id: {}", handle.db_session_id))
            })?;
        entry
            .route(handle)
            .map_err(|rejection| rejection.to_port_error(&handle.db_session_id))?;
        Ok(entry)
    }
}
