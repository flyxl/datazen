//! 内存会话目录的实现体。
//!
//! 结构只有一个 `Mutex<Inner>`：**所有**状态变更都在同一把锁内完成，因此
//! 「到期裁决」「替换切换」「作废」这些必须串行的动作天然串行，不存在两个并发裁决
//! 互相打架的可能。这也是为什么端口是 `async` 而临界区里一个 `.await` 都没有——
//! 临界区短到不值得让出执行权，让出反而会打开锁的争用窗口。
//!
//! `Inner` 里的四个表彼此职责分明：
//!
//! | 表 | 键 | 值 | 生命周期 |
//! |---|---|---|---|
//! | `entries` | `DbSessionId` | `DirectoryEntry` | 到期或清扫时删除 |
//! | `digests` | `DbSessionId` | `TokenDigest` | 与条目同生共死 |
//! | `operations` | `ReplacementOperationKey` | `ReplacementRecord` | 替换流程内 |
//! | `invalidations` | `DbSessionId` | `InvalidationRecord` | 只增不减（内存） |
//!
//! **没有第五张表。** 目录不落盘，进程退出四张表一起没了；磁盘上不存在任何由本模块写出的文件。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use super::entry::RouteRejection;
use datazen_platform_api::dto::session::SessionHandle;
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{DbSessionId, RuntimeEpoch, Timestamp};
use datazen_platform_api::ports::session_directory::{
    CloseDisposition, InvalidationReason, ReplacementCommit, ReplacementOperation,
    ReplacementOutcome, SessionDirectory, SessionOwner,
};

use super::commit::{
    RecordState, ReplacementOperationKey, ReplacementRecord, UnknownCommitOutcome,
};
use super::entry::{ClosureReason, DirectoryEntry, InvalidationRecord, RoutingState};
use super::id::{DbSessionIdGenerator, RuntimeEpochGenerator, SessionIdEntropy};
use super::{MonoInstant, SharedClock, SystemDirectoryClock};

/// 目录的全部内存状态。
#[derive(Default)]
pub(crate) struct Inner {
    pub(crate) entries: HashMap<DbSessionId, DirectoryEntry>,
    pub(crate) digests: HashMap<DbSessionId, super::TokenDigest>,
    pub(crate) operations: HashMap<ReplacementOperationKey, ReplacementRecord>,
    pub(crate) invalidations: HashMap<DbSessionId, InvalidationRecord>,
    /// 单调递增的状态版本号。只用于观测与测试断言「状态确实动过」。
    pub(crate) revision: u64,
}

/// 单进程内存会话目录。
pub struct InMemorySessionDirectory {
    pub(crate) inner: Mutex<Inner>,
    pub(crate) clock: SharedClock,
    pub(crate) ids: DbSessionIdGenerator,
    pub(crate) epochs: RuntimeEpochGenerator,
    pub(crate) entropy: Arc<dyn SessionIdEntropy>,
    pub(crate) lost_commit_reply: Mutex<Option<UnknownCommitOutcome>>,
    /// 本进程里**当前** worker 的 epoch。`start_worker_epoch` 每调一次换一次；
    /// `None` 表示调用方根本没走领 epoch 这条路（自己把 epoch 传进 `open_session`），
    /// 这时目录没有发言权，句柄只跟条目自己记的 epoch 对账。
    pub(crate) current_epoch: Mutex<Option<RuntimeEpoch>>,
}

impl InMemorySessionDirectory {
    /// 用系统单调时钟 + OS 熵建目录。
    pub fn new() -> Self {
        let entropy: Arc<dyn SessionIdEntropy> = Arc::new(super::id::OsEntropy);
        Self::build(Arc::new(SystemDirectoryClock::new()), entropy)
    }

    /// 换掉时钟（测试注入假时钟）。
    pub fn with_clock(clock: SharedClock) -> Self {
        Self::build(clock, Arc::new(super::id::OsEntropy))
    }

    /// 同时换掉时钟与熵源（测试用来强制碰撞）。
    pub fn with_sources(clock: SharedClock, entropy: Arc<dyn SessionIdEntropy>) -> Self {
        Self::build(clock, entropy)
    }

    fn build(clock: SharedClock, entropy: Arc<dyn SessionIdEntropy>) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            clock,
            ids: DbSessionIdGenerator::with_entropy(Arc::clone(&entropy)),
            epochs: RuntimeEpochGenerator::with_entropy(Arc::clone(&entropy)),
            entropy,
            lost_commit_reply: Mutex::new(None),
            current_epoch: Mutex::new(None),
        }
    }

    /// 毒锁（某个线程在临界区里 panic）不应该让整个会话层不可用：
    /// 这里没有跨调用的不变量被破坏，恢复被毒化的锁继续用即可。
    pub(crate) fn lock_inner(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn take_lost_commit_reply(&self) -> Option<UnknownCommitOutcome> {
        match self.lost_commit_reply.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        }
    }

    pub(crate) fn now(&self) -> MonoInstant {
        self.clock.now()
    }

    /// worker 每次启动领一个新的 `runtimeEpoch`。
    ///
    /// 领到就作废上一次的：worker 重启之后旧句柄一律显式失败（`StaleEpoch`），
    /// 目录**不会**拿着旧 `dbSessionId` 回配置里翻一个「看起来差不多的」会话顶上。
    pub fn start_worker_epoch(
        &self,
        worker_id: &datazen_platform_api::id::WorkerId,
    ) -> RuntimeEpoch {
        let epoch = self.epochs.start_worker_epoch(worker_id);
        match self.current_epoch.lock() {
            Ok(mut slot) => *slot = Some(epoch.clone()),
            Err(poisoned) => *poisoned.into_inner() = Some(epoch.clone()),
        }
        epoch
    }

    /// 句柄所属 epoch 是否还是本进程当前的 worker epoch。
    ///
    /// 领过 epoch（`start_worker_epoch`）之后，目录就知道「上一代 worker 的句柄一律作废」；
    /// 没领过时目录没有发言权，返回 `None`，句柄只跟条目自己记的 epoch 对账。
    pub(crate) fn dead_epoch(&self, handle: &SessionHandle) -> Option<RouteRejection> {
        let current = match self.current_epoch.lock() {
            Ok(slot) => slot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }?;
        (handle.runtime_epoch != current).then_some(RouteRejection::StaleEpoch { current })
    }

    // ---------------------------------------------------------------- 端口实现

    pub(crate) fn do_register(&self, owner: SessionOwner) -> Result<SessionHandle, PortError> {
        let now = self.now();
        let mut inner = self.lock_inner();
        if inner.entries.contains_key(&owner.db_session_id) {
            // 绝不静默改绑：同一个 ID 只能有一个登记 owner。
            return Err(PortError::cas_conflict(
                "dbSessionId",
                owner.db_session_id.to_string(),
            ));
        }
        let handle = owner.to_handle();
        inner
            .entries
            .insert(owner.db_session_id.clone(), DirectoryEntry::new(owner, now));
        inner.revision = inner.revision.wrapping_add(1);
        tracing::debug!(%handle.db_session_id, %handle.runtime_epoch, revision = inner.revision, "session registered");
        Ok(handle)
    }

    pub(crate) fn do_lookup(
        &self,
        db_session_id: DbSessionId,
    ) -> Result<Option<SessionOwner>, PortError> {
        let inner = self.lock_inner();
        Ok(inner
            .entries
            .get(&db_session_id)
            .filter(|entry| entry.is_routable())
            .map(|entry| entry.owner().clone()))
    }

    pub(crate) fn do_touch(
        &self,
        handle: &SessionHandle,
        business_activity: Timestamp,
    ) -> Result<(), PortError> {
        let mut inner = self.lock_inner();
        let entry = self
            .entry_for_handle(&mut inner, handle)
            .map_err(|rejection| rejection.to_port_error(&handle.db_session_id))?;
        // 只推业务活动时间。期限一个都不碰——心跳与读状态都不许续命。
        entry.record_business_activity(business_activity);
        Ok(())
    }

    pub(crate) fn do_commit_replacement(
        &self,
        commit: ReplacementCommit,
    ) -> Result<ReplacementOutcome, PortError> {
        let now = self.now();
        let key = ReplacementOperationKey::for_handle(&commit.old);
        let mut inner = self.lock_inner();

        // 先过旧条目的路由闸：旧句柄已经不可路由时，替换根本不该开始。
        self.entry_for_handle(&mut inner, &commit.old)
            .map_err(|rejection| rejection.to_port_error(&commit.old.db_session_id))?;

        match commit.operation {
            ReplacementOperation::Prepared => {
                let new_handle = commit.new_owner.to_handle();
                if new_handle.db_session_id == commit.old.db_session_id {
                    return Err(PortError::cas_conflict(
                        "replacement",
                        "candidate reuses the replaced db session id",
                    ));
                }
                if inner.entries.contains_key(&new_handle.db_session_id) {
                    return Err(PortError::cas_conflict(
                        "dbSessionId",
                        new_handle.db_session_id.to_string(),
                    ));
                }
                // 候选**从出生起就不可路由**：先落一条屏障态条目，
                // 顺带把这个 ID 占住，避免并发 register 抢走同一个候选 ID。
                inner.entries.insert(
                    new_handle.db_session_id.clone(),
                    DirectoryEntry::prepared(commit.new_owner.clone(), now),
                );
                inner.operations.insert(
                    key.clone(),
                    ReplacementRecord::prepared(key.clone(), commit.old.clone(), commit.new_owner),
                );
                inner.revision = inner.revision.wrapping_add(1);
                tracing::debug!(%key, %new_handle.db_session_id, "replacement prepared; candidate is not routable");
                Ok(ReplacementOutcome {
                    operation: ReplacementOperation::Prepared,
                    replaced: commit.old,
                })
            }
            ReplacementOperation::Committed => {
                let new_handle = self.expect_prepared(&mut inner, &key, "commit")?;
                let settled = self.take_lost_commit_reply();
                let truth = match settled {
                    Some(outcome) => outcome.state(),
                    None => {
                        self.apply_committed(
                            &mut inner,
                            &key,
                            &commit.old,
                            &new_handle,
                            RecordState::Committed,
                        );
                        inner.revision = inner.revision.wrapping_add(1);
                        tracing::info!(%key, %new_handle.db_session_id, "replacement committed; old entry closed for routing");
                        return Ok(ReplacementOutcome {
                            operation: ReplacementOperation::Committed,
                            replaced: commit.old,
                        });
                    }
                };
                // 服务端其实早已原子落定，只是确认没送到调用方。
                // 落定动作照做，随后立刻把屏障举到**双方都不可路由**为止。
                self.apply_settled(&mut inner, &key, &commit.old, &new_handle, truth);
                self.hold_barrier(&mut inner, &key, &commit.old, &new_handle, truth);
                tracing::warn!(
                    %key,
                    settled = truth.as_str(),
                    "replacement outcome unknown to the caller; both entries held behind the barrier"
                );
                Err(PortError::ProviderTimeout(format!(
                    "replacement {key} outcome unknown; query commit status by operation key"
                )))
            }
            ReplacementOperation::RolledBack => {
                let new_handle = self.expect_prepared(&mut inner, &key, "roll back")?;
                match self.take_lost_commit_reply() {
                    Some(outcome) => {
                        let truth = outcome.state();
                        self.apply_settled(&mut inner, &key, &commit.old, &new_handle, truth);
                        self.hold_barrier(&mut inner, &key, &commit.old, &new_handle, truth);
                        tracing::warn!(
                            %key,
                            settled = truth.as_str(),
                            "replacement outcome unknown to the caller; both entries held behind the barrier"
                        );
                        return Err(PortError::ProviderTimeout(format!(
                            "replacement {key} outcome unknown; query commit status by operation key"
                        )));
                    }
                    None => {
                        self.apply_rolled_back(&mut inner, &key);
                        inner.revision = inner.revision.wrapping_add(1);
                        tracing::info!(%key, "replacement rolled back; old entry untouched");
                        Ok(ReplacementOutcome {
                            operation: ReplacementOperation::RolledBack,
                            replaced: commit.old,
                        })
                    }
                }
            }
        }
    }

    pub(crate) fn do_invalidate(
        &self,
        db_session_id: DbSessionId,
        reason: InvalidationReason,
    ) -> Result<(), PortError> {
        let now = self.now();
        let mut inner = self.lock_inner();
        let entry = inner.entries.get(&db_session_id).ok_or_else(|| {
            PortError::NotFound(format!("unknown db session id: {db_session_id}"))
        })?;
        if entry.state() == RoutingState::Closed {
            return Err(PortError::cas_conflict(
                "session",
                format!("{db_session_id}:closed"),
            ));
        }
        let owner = entry.owner().clone();
        // 作废后**删除**条目：后续请求只能是「不知道」，绝无透明重建。
        inner.entries.remove(&db_session_id);
        inner.digests.remove(&db_session_id);
        inner.invalidations.insert(
            db_session_id.clone(),
            InvalidationRecord {
                db_session_id,
                reason,
                reason_code: super::entry::invalidation_reason_code(&reason),
                runtime_epoch: owner.runtime_epoch.clone(),
                invalidated_at: self.clock.project(now),
            },
        );
        inner.revision = inner.revision.wrapping_add(1);
        tracing::warn!(
            reason = super::entry::invalidation_reason_code(&reason),
            %owner.db_session_id,
            "session invalidated; no transparent rebuild"
        );
        Ok(())
    }

    pub(crate) fn do_release(
        &self,
        handle: &SessionHandle,
        disposition: CloseDisposition,
    ) -> Result<(), PortError> {
        let mut inner = self.lock_inner();
        let entry = self
            .entry_for_handle(&mut inner, handle)
            .map_err(|rejection| rejection.to_port_error(&handle.db_session_id))?;
        entry.close(ClosureReason::Released(disposition));
        inner.revision = inner.revision.wrapping_add(1);
        tracing::debug!(%handle.db_session_id, disposition = ?disposition, "session released");
        Ok(())
    }

    pub(crate) fn do_sweep_expired(&self, now: Timestamp) -> Result<Vec<SessionOwner>, PortError> {
        let server_now = self.now();
        self.note_caller_clock_drift(now, server_now);

        let mut inner = self.lock_inner();
        let mut expired = Vec::new();
        let mut purge = Vec::new();
        for (db_session_id, entry) in inner.entries.iter_mut() {
            match entry.state() {
                // 屏障由 commit_status 落定后才回收：在此之前谁也不许把它清掉，
                // 免得「未知结果」变成「查无此会话」，丢掉可恢复的那条线索。
                RoutingState::CommitBarrier => continue,
                // 已经裁定过期、等着被回收的条目：上一轮 adjudicate 把它推到 Closing，
                // 这一轮照样要报出来、并且真正删掉，不能让它永远赖在表里。
                RoutingState::Closing => {
                    expired.push(entry.owner().clone());
                    purge.push(db_session_id.clone());
                    continue;
                }
                RoutingState::Closed => {
                    purge.push(db_session_id.clone());
                    continue;
                }
                RoutingState::Routable => {}
            }
            if entry.adjudicate(server_now).is_expired() {
                expired.push(entry.owner().clone());
                purge.push(db_session_id.clone());
            }
        }
        for db_session_id in purge {
            inner.entries.remove(&db_session_id);
            inner.digests.remove(&db_session_id);
        }
        if !expired.is_empty() {
            inner.revision = inner.revision.wrapping_add(1);
            tracing::info!(
                count = expired.len(),
                revision = inner.revision,
                "expired sessions swept"
            );
        }
        Ok(expired)
    }

    // ---------------------------------------------------------------- 内部

    /// 取可路由条目。epoch 不对就是显式失败，不找替代。
    fn entry_for_handle<'a>(
        &self,
        inner: &'a mut Inner,
        handle: &SessionHandle,
    ) -> Result<&'a mut DirectoryEntry, super::RouteRejection> {
        if let Some(dead) = self.dead_epoch(handle) {
            return Err(dead);
        }
        let entry = inner
            .entries
            .get_mut(&handle.db_session_id)
            .ok_or(super::RouteRejection::Unknown)?;
        entry.route(handle)?;
        Ok(entry)
    }

    /// 取出候选句柄，并确认这次替换确实处在「已 prepare、待落定」的状态。
    fn expect_prepared(
        &self,
        inner: &Inner,
        key: &ReplacementOperationKey,
        action: &str,
    ) -> Result<SessionHandle, PortError> {
        let record = inner
            .operations
            .get(key)
            .ok_or_else(|| PortError::cas_conflict("replacement", format!("{key} not prepared")))?;
        if record.state() != RecordState::Prepared {
            return Err(PortError::cas_conflict(
                "replacement",
                format!(
                    "{key} cannot {action} from state {}",
                    record.state().as_str()
                ),
            ));
        }
        Ok(record.new_handle())
    }

    /// 原子提交：旧条目改关闭路由、新条目放行，两者在同一把锁里生效。
    fn apply_committed(
        &self,
        inner: &mut Inner,
        key: &ReplacementOperationKey,
        old: &SessionHandle,
        new: &SessionHandle,
        truth: RecordState,
    ) {
        if let Some(new_entry) = inner.entries.get_mut(&new.db_session_id) {
            new_entry.publish();
        }
        if let Some(old_entry) = inner.entries.get_mut(&old.db_session_id) {
            old_entry.close(ClosureReason::ReplacedBy(new.clone()));
        }
        if let Some(record) = inner.operations.get_mut(key) {
            record.set_state(truth);
        }
    }

    /// 原子回滚：新候选销毁，旧条目原样恢复（期限与挂载状态都不动）。
    fn apply_rolled_back(&self, inner: &mut Inner, key: &ReplacementOperationKey) {
        let new_id = inner
            .operations
            .get(key)
            .map(|record| record.new_handle().db_session_id);
        if let Some(new_id) = new_id {
            inner.entries.remove(&new_id);
            inner.digests.remove(&new_id);
        }
        if let Some(record) = inner.operations.get_mut(key) {
            record.set_state(RecordState::RolledBack);
        }
    }

    /// 按服务端已落定的结果执行切换。落定动作和「应答丢失」是两件事：
    /// 前者必须发生，后者只影响调用方能不能拿到确认。
    fn apply_settled(
        &self,
        inner: &mut Inner,
        key: &ReplacementOperationKey,
        old: &SessionHandle,
        new: &SessionHandle,
        truth: RecordState,
    ) {
        match truth {
            RecordState::Committed => self.apply_committed(inner, key, old, new, truth),
            _ => self.apply_rolled_back(inner, key),
        }
    }

    /// 提交结果未知时**举起屏障**：新旧两边都不可路由，直到按 operation key 查清。
    ///
    /// 这正是「绝不允许旧的和新的同时执行」的实现点：查清楚之前，两个 handle
    /// 拿到的都是拒绝，而不是「谁先抢到算谁的」。
    fn hold_barrier(
        &self,
        inner: &mut Inner,
        key: &ReplacementOperationKey,
        old: &SessionHandle,
        new: &SessionHandle,
        truth: RecordState,
    ) {
        if let Some(old_entry) = inner.entries.get_mut(&old.db_session_id) {
            old_entry.hold_commit_barrier(Some(new.clone()));
        }
        if let Some(new_entry) = inner.entries.get_mut(&new.db_session_id) {
            // 候选保持屏障态：它还没被任何一方确认。
            new_entry.hold_commit_barrier(None);
        }
        if let Some(record) = inner.operations.get_mut(key) {
            record.mark_undecided(truth);
        }
        inner.revision = inner.revision.wrapping_add(1);
    }

    /// 调用方时钟与服务端单调时钟的漂移只记日志，**绝不**据此提前关闭。
    ///
    /// 参数 `now` 是调用方观测到的墙上时间；判定仍然只按 `server_now`。
    pub(crate) fn note_caller_clock_drift(&self, now: Timestamp, server_now: MonoInstant) {
        // 服务端投影同样折回毫秒再比，clock-agnostic：判定只按单调值走，
        // 漂移报告只是把两边的墙上时间对齐看一眼。
        let server_millis = parse_fixed_millis(self.clock.project(server_now).as_str());
        match (server_millis, parse_fixed_millis(now.as_str())) {
            (Some(server_millis), Some(caller_millis)) if caller_millis < server_millis => {
                tracing::debug!(
                    caller = %now,
                    drift_millis = server_millis - caller_millis,
                    "sweep caller clock is behind the server projection; expiry still adjudicated on the server monotonic clock"
                );
            }
            (_, None) => {
                tracing::trace!(caller = %now, "sweep caller clock not in canonical UTC form; ignored");
            }
            (None, Some(_)) | (Some(_), Some(_)) => {}
        }
    }
}

impl Default for InMemorySessionDirectory {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InMemorySessionDirectory {
    /// 手写 `Debug`：**只**打印 ID 与状态。令牌材料一个都不在这里——
    /// 这不是脱敏约定，是根本没有可脱敏的字段。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.lock_inner();
        f.debug_struct("InMemorySessionDirectory")
            .field("entries", &inner.entries.len())
            .field("operations", &inner.operations.len())
            .field("invalidations", &inner.invalidations.len())
            .field("revision", &inner.revision)
            .finish()
    }
}

/// 解析本模块自己产出的定宽 UTC 形式。**解析不了就当作不可比**，
/// 绝不放宽成「尽力猜一个时间」。
fn parse_fixed_millis(raw: &str) -> Option<i64> {
    fn digits(bytes: &[u8]) -> Option<i64> {
        if bytes.is_empty() || !bytes.iter().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut acc = 0i64;
        for byte in bytes {
            acc = acc * 10 + i64::from(byte - b'0');
        }
        Some(acc)
    }
    let body = raw.strip_suffix('Z')?;
    if body.len() != 23 {
        return None;
    }
    let bytes = body.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'.'
    {
        return None;
    }
    let year = digits(&bytes[0..4])?;
    let month = digits(&bytes[5..7])?;
    let day = digits(&bytes[8..10])?;
    let hour = digits(&bytes[11..13])?;
    let minute = digits(&bytes[14..16])?;
    let second = digits(&bytes[17..19])?;
    let millis = digits(&bytes[20..23])?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = super::days_from_civil(year, month as u32, day as u32);
    Some(days * 86_400_000 + hour * 3_600_000 + minute * 60_000 + second * 1_000 + millis)
}

#[async_trait::async_trait]
impl SessionDirectory for InMemorySessionDirectory {
    async fn register(&self, owner: SessionOwner) -> Result<SessionHandle, PortError> {
        self.do_register(owner)
    }

    async fn lookup(&self, db_session_id: DbSessionId) -> Result<Option<SessionOwner>, PortError> {
        self.do_lookup(db_session_id)
    }

    async fn touch(
        &self,
        handle: &SessionHandle,
        business_activity: Timestamp,
    ) -> Result<(), PortError> {
        self.do_touch(handle, business_activity)
    }

    async fn commit_replacement(
        &self,
        commit: ReplacementCommit,
    ) -> Result<ReplacementOutcome, PortError> {
        self.do_commit_replacement(commit)
    }

    async fn invalidate(
        &self,
        db_session_id: DbSessionId,
        reason: InvalidationReason,
    ) -> Result<(), PortError> {
        self.do_invalidate(db_session_id, reason)
    }

    async fn release(
        &self,
        handle: &SessionHandle,
        disposition: CloseDisposition,
    ) -> Result<(), PortError> {
        self.do_release(handle, disposition)
    }

    async fn sweep_expired(&self, now: Timestamp) -> Result<Vec<SessionOwner>, PortError> {
        self.do_sweep_expired(now)
    }
}
