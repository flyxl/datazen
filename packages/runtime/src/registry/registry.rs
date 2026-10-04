//! §4 / §7 会话登记表：进程内登记、仲裁与额度的**唯一持有者**。
//!
//! ## 登记表不是锁住整张表
//!
//! 本文件里出现的每一个锁都只做两件事之一：**定位**（按 `dbSessionId` 找 actor）
//! 或**记账**（插入 / 移除 / 额度加减）。锁在任何 `.await` 之前一律释放，
//! 跨 `.await` 存活的是克隆出来的 `SessionActor`，不是锁。
//!
//! ```text
//!   let record = table.locate(id);   // 持读锁，只为 clone 一个 Arc
//!                                    ↓ 锁在此处已经放掉
//!   record.actor.exec(..).await     // 真正的 await：actor 内部串行，会话之间互不阻塞
//! ```
//!
//! 换成「一把大锁串行整个登记表」会得到一个能跑但错误的实现：会话 A 的
//! 慢查询会把会话 B 的取消请求一起锁住，而 §6.3 要求控制旁路在飞行中必须可服务。
//!
//! | 文件 | 职责 | 关键约束 |
//! | --- | --- | --- |
//! | `mod.rs` | 对外门面与转出 | 只转出，不重定义端口侧取值 |
//! | `registry.rs` | 登记表 / 额度 / 审计泵 + `SessionPort` 实现 | 锁不跨 await |
//! | `actor.rs` | 单会话仲裁器 | 会话内串行，跨会话并行 |
//! | `handles.rs` | §6.5 句柄登记与取消绑定 | 两条独立的表 |
//! | `epoch.rs` | runtimeEpoch 归属与 D-02 出口折叠 | 纯函数 |
//! | `audit.rs` | CM-72 审计条目 | 只存线上字面量 |
//! | `backend.rs` | 物理资源端口 | driver 对已交出的句柄没有可见性 |
//! | `receipt.rs` | D-01 `CancelReceipt` | 三处置 |
//!
//! ## 登记的三个「不得」（CM-74 / §4.1）
//!
//! 1. 物理资源进入 Routable **之前**不得把会话放进可见表——否则「已登记」会被当成「可用」。
//! 2. 登记失败**不得**留下表项：调用方拿不到成功，也查不到一个半成品会话。
//! 3. 释放时**先在 actor 里注销句柄并终结**（`actor::release`），再关物理资源。
//!
//! ## 一个 id 只有一个 owner
//!
//! `dbSessionId` 一旦占位（正在打开）就不接受第二次登记；登记失败会把占位退回。
//! 占位表 `opening` **不对外可见**：`table` 里没有它，所以任何按 id 的读都拿到
//! `UnknownSession`——这正是「未登记的句柄一律拒绝」，而不是一句需要单开的特例。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use async_trait::async_trait;
use indexmap::IndexMap;
use tokio::sync::mpsc;

use crate::connection::{
    CloseMode, DbSessionId, ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState,
    RuntimeError, SessionHandle, SessionView, WorkerId,
};

use crate::registry::actor::{
    spawn_actor, AuditOutbox, ControlCommand, ExecCommand, OpenRequest, SessionActor,
};
use crate::registry::audit::{AuditKind, AuditLog, Outcome, RegistryAuditEntry};
use crate::registry::backend::SessionBackend;
use crate::registry::epoch::RuntimeEpoch;
use crate::registry::port::SessionPort;
use crate::registry::receipt::CancelReceipt;

/// 锁中毒不该把会话登记表带走：被毒化的锁里没有不一致状态，只有一个没写完的临界区，
/// 而我们所有临界区都是「算完再写」的。取回内部值继续用，并把毒化本身留给 tracing。
fn guard<T>(result: std::sync::LockResult<T>) -> T {
    match result {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// 登记表里的一项。
#[derive(Clone)]
pub(super) struct SessionRecord {
    pub(super) db_session_id: DbSessionId,
    /// §12 / §4.4：epoch 是旧请求的**唯一**归属依据，也是 D-02 折叠的输入。
    runtime_epoch: RuntimeEpoch,
    worker_id: WorkerId,
    pub(super) actor: SessionActor,
}

#[derive(Default)]
struct Table {
    sessions: IndexMap<DbSessionId, SessionRecord>,
}

impl Table {
    /// 按 id 取一个**克隆**。调用方随后可以无锁地 await：克隆里装的是 `SessionActor`，
    /// 它本身就是多生产者邮箱的所有权凭证。
    fn locate(&self, id: &DbSessionId) -> Option<SessionRecord> {
        self.sessions.get(id).cloned()
    }

    fn contains(&self, id: &DbSessionId) -> bool {
        self.sessions.contains_key(id)
    }

    fn records(&self) -> Vec<SessionRecord> {
        self.sessions.values().cloned().collect()
    }

    fn owned_by(&self, worker_id: &WorkerId) -> Vec<SessionRecord> {
        self.sessions
            .values()
            .filter(|record| &record.worker_id == worker_id)
            .cloned()
            .collect()
    }

    /// 同 id 已登记则拒绝——「一个 id 只有一个 owner」。
    fn insert(&mut self, record: SessionRecord) -> Result<(), RuntimeError> {
        if self.sessions.contains_key(&record.db_session_id) {
            return Err(RuntimeError::InvariantBroken("duplicateDbSessionId"));
        }
        self.sessions.insert(record.db_session_id.clone(), record);
        Ok(())
    }

    fn remove(&mut self, id: &DbSessionId) -> Option<SessionRecord> {
        self.sessions.shift_remove(id)
    }
}

/// §9.3 额度账。
///
/// `outstanding` **包含**尚未确认隔离的陈旧占用。这是 CM-58 的全部要害：
/// worker 崩溃不等于它的连接已经关掉，账上留着这份额度直到隔离确认，
/// 才不会在网络分区期间把同一批物理连接超额发出去。
pub(super) struct QuotaLedger {
    outstanding: AtomicUsize,
    limit: usize,
    /// 被扣住但尚未确认隔离的 worker → 被扣住会话的 id（连带审计用）。
    stale: Mutex<HashMap<WorkerId, Vec<DbSessionId>>>,
}

impl QuotaLedger {
    fn new(limit: usize) -> Self {
        Self {
            outstanding: AtomicUsize::new(0),
            limit,
            stale: Mutex::new(HashMap::new()),
        }
    }

    /// 预留一份额度。用 `fetch_add` 的返回值判定而不是「先读再加」：
    /// 两个并发登记不会同时读到旧的 `outstanding` 而双双通过检查。
    pub(super) fn try_reserve(&self) -> Result<(), RuntimeError> {
        let previous = self.outstanding.fetch_add(1, Ordering::SeqCst);
        if previous >= self.limit {
            self.outstanding.fetch_sub(1, Ordering::SeqCst);
            return Err(RuntimeError::BudgetExhausted("sessionQuota"));
        }
        Ok(())
    }

    pub(super) fn release(&self) {
        self.outstanding.fetch_sub(1, Ordering::SeqCst);
    }

    fn remaining(&self) -> usize {
        self.limit
            .saturating_sub(self.outstanding.load(Ordering::SeqCst))
    }

    /// 把这些会话的额度记成该 worker 的陈旧占用：**只记账，不释放**。
    fn hold_stale(&self, worker_id: &WorkerId, ids: Vec<DbSessionId>) {
        let mut stale = guard(self.stale.lock());
        stale.entry(worker_id.clone()).or_default().extend(ids);
    }

    /// 隔离确认后才归还陈旧额度。未知 worker 返回 0——重复确认不得二次归还。
    /// 返回被释放的会话 id，供审计逐条落账。
    fn confirm_quarantined(&self, worker_id: &WorkerId) -> Vec<DbSessionId> {
        let released = guard(self.stale.lock())
            .remove(worker_id)
            .unwrap_or_default();
        if !released.is_empty() {
            self.outstanding.fetch_sub(released.len(), Ordering::SeqCst);
        }
        released
    }

    fn stale_held(&self, worker_id: &WorkerId) -> usize {
        guard(self.stale.lock())
            .get(worker_id)
            .map(Vec::len)
            .unwrap_or(0)
    }
}

/// 审计泵的接收端。与 `AuditOutbox` 分离成两端，是为了让 actor 只拿到发送端：
/// 接收端因此永远不会被克隆到别的任务里，也就不存在「谁把接收端搬走了」的问题。
struct AuditSink {
    rx: Mutex<mpsc::UnboundedReceiver<RegistryAuditEntry>>,
    log: Mutex<AuditLog>,
}

impl AuditSink {
    fn new(rx: mpsc::UnboundedReceiver<RegistryAuditEntry>) -> Self {
        Self {
            rx: Mutex::new(rx),
            log: Mutex::new(AuditLog::new()),
        }
    }

    fn rx(&self) -> MutexGuard<'_, mpsc::UnboundedReceiver<RegistryAuditEntry>> {
        guard(self.rx.lock())
    }

    /// 收干队列。`try_recv` 是同步的，所以这把锁从不跨 `.await`。
    fn pump(&self) -> Vec<RegistryAuditEntry> {
        let drained: Vec<RegistryAuditEntry> = {
            let mut rx = self.rx();
            let mut drained = Vec::new();
            while let Ok(entry) = rx.try_recv() {
                drained.push(entry);
            }
            drained
        };
        if drained.is_empty() {
            return drained;
        }
        let mut log = guard(self.log.lock());
        for entry in &drained {
            log.record(entry.clone());
        }
        drained
    }

    /// 累积日志快照（排障用）。
    fn snapshot(&self) -> Vec<RegistryAuditEntry> {
        guard(self.log.lock()).entries().to_vec()
    }
}

/// 进程内会话登记表。
///
/// 本类型**拥有** [`SessionPort`] 的实现：对外只有一个 `Arc<dyn SessionPort>` 入口，
/// 不需要再包一层适配器。
pub struct SessionRegistry {
    table: RwLock<Table>,
    opening: Mutex<Vec<DbSessionId>>,
    pub(super) quota: QuotaLedger,
    audit: AuditSink,
    pub(super) outbox: AuditOutbox,
    pub(super) backend: Arc<dyn SessionBackend>,
    pub(super) epoch_seq: AtomicU64,
    session_limit: usize,
}

impl SessionRegistry {
    /// 新建登记表。`session_limit` 是**会话数**上限，与 §9.3 的连接额度分开计。
    pub fn new(backend: Arc<dyn SessionBackend>, session_limit: usize) -> Self {
        let (outbox, sink_rx) = mpsc::unbounded_channel();
        Self {
            table: RwLock::new(Table::default()),
            opening: Mutex::new(Vec::new()),
            quota: QuotaLedger::new(session_limit),
            audit: AuditSink::new(sink_rx),
            outbox,
            backend,
            epoch_seq: AtomicU64::new(0),
            session_limit,
        }
    }

    pub fn session_limit(&self) -> usize {
        self.session_limit
    }

    /// CM-58 的**查询端口**：剩余可登记会话数。
    ///
    /// 只提供查询，不实现 Job——Job 的受理、claim、恢复核验属于 §10 的另一条轨道，
    /// 本轨道不越界造一个半成品 Job 出来。
    pub fn remaining_quota(&self) -> usize {
        self.quota.remaining()
    }

    /// 某 worker 尚被扣住但未确认隔离的会话数（CM-58 的可观测面）。
    pub fn stale_quota_for(&self, worker_id: &WorkerId) -> usize {
        self.quota.stale_held(worker_id)
    }

    /// 已登记的会话 id 快照。
    pub fn registered_ids(&self) -> Vec<DbSessionId> {
        self.audit.pump();
        self.read_table().sessions.keys().cloned().collect()
    }

    /// 收干审计队列，返回本次新收到的条目。
    pub fn drain_audit(&self) -> Vec<RegistryAuditEntry> {
        self.audit.pump()
    }

    /// 累积审计日志快照（先收干队列再取快照）。
    pub fn audit_log(&self) -> Vec<RegistryAuditEntry> {
        self.audit.pump();
        self.audit.snapshot()
    }

    /// 会话是否已登记。**只认已登记**，`opening` 里「正在打开」的不算——
    /// 那正是 §4.1「登记不得先于 Routable」的可观测面。
    pub fn is_registered(&self, db_session_id: &DbSessionId) -> bool {
        self.audit.pump();
        self.read_table().contains(db_session_id)
    }

    /// §12 / D-02：登记表侧记录的世代号。
    ///
    /// 单会话的 epoch 权威归属在 actor 里；这里保留一份是给**登记表级**判定用的：
    /// worker 租约失效时需要按 epoch 区分「这个会话属于失效的那个世代」，
    /// 而这条判定不能靠读视图（读视图要走 actor 邮箱，正在执行中的会话会排队）。
    pub fn epoch_of(&self, db_session_id: &DbSessionId) -> Result<RuntimeEpoch, RuntimeError> {
        Ok(self.locate(db_session_id)?.runtime_epoch)
    }

    fn read_table(&self) -> RwLockReadGuard<'_, Table> {
        guard(self.table.read())
    }

    fn write_table(&self) -> RwLockWriteGuard<'_, Table> {
        guard(self.table.write())
    }

    /// 按 id 定位 actor。**锁在这里就结束了**，返回的是可自由 await 的克隆。
    pub(super) fn locate(
        &self,
        db_session_id: &DbSessionId,
    ) -> Result<SessionRecord, RuntimeError> {
        self.read_table()
            .locate(db_session_id)
            .ok_or_else(|| RuntimeError::UnknownSession(db_session_id.to_string()))
    }

    /// §4.1 / CM-74：登记一个会话。
    ///
    /// 顺序不可调换：先占 id、再占额度、再打开物理资源，只有拿到投影之后才放进
    /// 可见表。任何一步失败都**不留痕**——没有表项、没有半开额度、没有占位。
    pub async fn register_session(
        &self,
        request: OpenRequest,
    ) -> Result<SessionView, RuntimeError> {
        self.audit.pump();
        let db_session_id = request.db_session_id.clone();
        let worker_id = request.worker_id.clone();

        // 占位：同一 id 的第二次登记当场拒绝，不排队、不覆盖。
        {
            let mut opening = guard(self.opening.lock());
            if opening.contains(&db_session_id) {
                return Err(RuntimeError::InvariantBroken("duplicateDbSessionId"));
            }
            opening.push(db_session_id.clone());
        }

        match self.open_and_publish(request, worker_id).await {
            Ok(view) => {
                self.release_opening(&db_session_id);
                self.audit.pump();
                Ok(view)
            }
            Err(error) => {
                self.release_opening(&db_session_id);
                Err(error)
            }
        }
    }

    async fn open_and_publish(
        &self,
        request: OpenRequest,
        worker_id: WorkerId,
    ) -> Result<SessionView, RuntimeError> {
        let db_session_id = request.db_session_id.clone();
        self.quota.try_reserve()?;

        let runtime_epoch = RuntimeEpoch::new(self.epoch_seq.fetch_add(1, Ordering::SeqCst) + 1);
        // actor 消费一份请求，登记表自己留一份要发回去：这样 `Open` 命令里带的
        // 输入与 actor 自己的 epoch 一定是同一份，不会出现两处各改一次的漂移。
        let open_input = request.clone();
        let actor = spawn_actor(
            request,
            runtime_epoch,
            Arc::clone(&self.backend),
            self.outbox.clone(),
        );

        // 打开失败时 actor 自己会走「从登记表注销并关闭」的分支；这里只需要
        // 把账退回去——**不得**把失败变成成功，也不得留下额度。
        let opened = actor
            .exec(|reply| ExecCommand::Open {
                request: open_input,
                reply,
            })
            .await;
        let view = match opened {
            Ok(view) => view,
            Err(error) => {
                self.quota.release();
                return Err(error);
            }
        };

        let record = SessionRecord {
            db_session_id,
            runtime_epoch,
            worker_id,
            actor,
        };
        if self.write_table().insert(record).is_err() {
            self.quota.release();
            return Err(RuntimeError::InvariantBroken("duplicateDbSessionId"));
        }
        Ok(view)
    }

    fn release_opening(&self, db_session_id: &DbSessionId) {
        let mut opening = guard(self.opening.lock());
        opening.retain(|held| held != db_session_id);
    }

    /// §7.4-6：§12 原子切换**之后**才把候选放进可见表。
    ///
    /// 与 [`open_and_publish`](Self::open_and_publish) 的差别全在这一句：候选在
    /// §12 切换之前对宿主**不可见**，切换之后才插表，于是没有任何入口能在切换前
    /// 定位到它。表项形状是 `registry` 的私有事实，所以这条留在本文件，
    /// 与 [`crate::registry::candidate`] 里的四条原语配套。
    pub(super) fn publish_candidate(
        &self,
        worker_id: WorkerId,
        runtime_epoch: RuntimeEpoch,
        view: SessionView,
        actor: SessionActor,
    ) -> Result<SessionView, RuntimeError> {
        let record = SessionRecord {
            db_session_id: view.handle.db_session_id.clone(),
            runtime_epoch,
            worker_id,
            actor,
        };
        if self.write_table().insert(record).is_err() {
            return Err(RuntimeError::InvariantBroken("duplicateDbSessionId"));
        }
        Ok(view)
    }

    /// §6.4：按注入的绝对毫秒驱逐到期的空闲会话。
    ///
    /// 时间由调用方注入：本模块**不读** `Instant::now()`，到期判定因此在测试里
    /// 完全确定，不依赖真实时钟。
    pub async fn evict_idle_at(&self, at_ms: u64) -> Vec<SessionView> {
        self.audit.pump();
        let targets = self.read_table().records();
        let mut evicted = Vec::new();
        for record in targets {
            let Ok(Some(view)) = record
                .actor
                .exec(|reply| ExecCommand::Evict { at_ms, reply })
                .await
            else {
                continue;
            };
            self.forget(&record.db_session_id);
            evicted.push(view);
        }
        evicted
    }

    /// 关闭成功后从表里摘除并归还额度。
    ///
    /// 摘除只对**仍在表里的那一项**生效：actor 关闭期间可能已有别的路径把它摘掉，
    /// 重复摘除会二次归还额度，把账算松。
    fn forget(&self, db_session_id: &DbSessionId) -> bool {
        let removed = self.write_table().remove(db_session_id).is_some();
        if removed {
            self.quota.release();
        }
        removed
    }

    /// §12 / CM-58：协调器租约失效后，把该 worker 名下的会话全部作废。
    ///
    /// 作废**不归还额度**：这些连接还在别处活着，隔离确认之前这份额度不能发出去。
    /// 归还由 [`SessionRegistry::confirm_worker_quarantined`] 完成。
    ///
    /// ## 三种结果，三种动作（D-R2-2）
    ///
    /// 控制旁路的答复分三种，它们对「要不要摘表项」的影响完全不同：
    ///
    /// | `control()` 返回 | 含义 | 摘表项？ |
    /// | --- | --- | --- |
    /// | `Ok(true)` | 送达，且本会话确属该 worker，会话已在 actor 里回滚关闭 | 是 |
    /// | `Ok(false)` | **同样送达了**，但答复是「这不归这个 worker 管 / 已无物理资源」 | 是 |
    /// | `Err(_)` | **没送达**：控制通道已断，或投递之后回执丢失 | **否** |
    ///
    /// `Ok(false)` 是**被确认的否定答复**，不是投递失败，所以它照常摘行。
    /// 这一格在登记表这条路径上**确实可达**：`owned_by` 按 worker 筛一遍之后，
    /// actor 侧的第二道防线 `state.physical.is_none()` 仍能把它筛出来——§9.4 释放
    /// 例程在物理层关不掉（`Undecidable`）时**先**把 `physical` 清空再返回
    /// `SessionLost`，而 [`SessionRegistry::evict_idle_at`] 收不到成功视图就跳过
    /// 摘行，留下一条「资源已清空、行还在表里」的记录。下一次租约失效就走这一格。
    /// 所以绝不能把它塞进失败分支——把否定答复当成投递失败，代价是这条会话永远
    /// 留在表里、额度永远挂在 stale 上等一个不会来的隔离确认。
    ///
    /// 此前这里是 `let _ = ...` 然后**无条件**摘行，等于把第三种也当成第二种处理。
    /// 后果是具体的：控制通道已经断了，物理资源却还在别处活着，登记表却已经把这条
    /// 记成「已作废」，额度随后被 `quota.hold_stale` 挂住等人来隔离确认——
    /// 而一个已经没有控制通道的会话不会被隔离，那份额度就永久挂住。
    ///
    /// ## 为什么 `Err` 分支仍然必须存在
    ///
    /// 在**正常退出路径**上 `Err` 不可达，理由是结构性的：
    /// [`SessionActor`] 自己持有 `UnboundedSender`，`run_actor` 只有在
    /// `exec_rx.recv()` 读到 `None` 时才会把 `exec_open` 置否（见 `actor.rs`），
    /// 而 `None` 要求**所有** `ExecCommand` 发送端都被 drop。`owned_by` 是按值克隆出
    /// `SessionRecord` 的，克隆里的 `actor` 在 `.await` 期间就是一个活着的发送端，
    /// 因此**摘表项不可能造成 `Err`**——摘行只会多丢一份克隆，丢的是表里那份。
    ///
    /// 能造出 `Err` 的只剩 actor 任务异常终止（panic / abort 把 `exec_rx` 一起丢掉）。
    /// 现实里最常见的一种是**驱动回调把 actor 任务带走**：§9.4 在任务内部直接
    /// `await backend.close(..)`，回调一崩，展开就在回执发出去之前把它丢掉，
    /// `control()` 于是拿到 `Err`（这一格由 `投递失败保留表项_额度不挂stale等重投` 钉住）。
    /// 类型系统挡不住这件事，注释也挡不住。所以这里选择把「正常路径不可能」
    /// 写成**分支**而不是注释：`Err` 时保留表项并留下可查的痕迹。
    /// 代价是那条会话仍然显示为「已登记」——这个取舍与本模块「宁可留一条会诱发
    /// 重投的可见记录，也不要在物理资源仍活着时把它静默记成已作废」同源。
    pub async fn invalidate_worker(&self, worker_id: &WorkerId) -> Vec<DbSessionId> {
        self.audit.pump();
        let targets = self.read_table().owned_by(worker_id);
        let mut lost = Vec::new();
        for record in targets {
            // 控制旁路（§6.3）：即使此刻有执行在飞行，租约失效也必须立刻送达。
            let delivered = match record
                .actor
                .control(|reply| ControlCommand::InvalidateWorker {
                    worker_id: worker_id.clone(),
                    reply,
                })
                .await
            {
                // `Ok(_)` 一律算送达：`false` 是被确认的否定答复（见上表），
                // 它说明控制旁路工作正常，只是这条不归该 worker 管、或已无物理资源可关。
                Ok(_) => true,
                Err(error) => {
                    // 没送达：保留表项，且**不要**把它算进 `lost`——
                    // `lost` 会驱动 `quota.hold_stale`，把额度挂给一个不会到来的隔离确认。
                    tracing::warn!(
                        db_session_id = record.db_session_id.as_str(),
                        worker_id = worker_id.as_str(),
                        reason = error.reason(),
                        "租约失效的控制命令没有送达；保留表项等待重投，其物理资源可能仍在别处活着，隔离确认前不得归还额度"
                    );
                    false
                }
            };
            if !delivered {
                continue;
            }
            let dropped = self.write_table().remove(&record.db_session_id).is_some();
            if dropped {
                lost.push(record.db_session_id);
            }
        }
        if !lost.is_empty() {
            self.quota.hold_stale(worker_id, lost.clone());
            for db_session_id in &lost {
                self.emit_quota(AuditKind::QuotaHeldStale, db_session_id, Outcome::Undecided);
            }
        }
        self.audit.pump();
        lost
    }

    /// CM-58：隔离确认后归还该 worker 的陈旧额度，返回归还的会话数。
    pub fn confirm_worker_quarantined(&self, worker_id: &WorkerId) -> usize {
        self.audit.pump();
        let released = self.quota.confirm_quarantined(worker_id);
        for db_session_id in &released {
            self.emit_quota(
                AuditKind::QuotaReleasedStale,
                db_session_id,
                Outcome::Succeeded,
            );
        }
        self.audit.pump();
        released.len()
    }

    fn emit_quota(&self, kind: AuditKind, db_session_id: &DbSessionId, outcome: Outcome) {
        let _ = self.outbox.send(RegistryAuditEntry {
            kind,
            db_session_id: db_session_id.clone(),
            runtime_epoch: 0,
            outcome,
            error_code: None,
            disposition: None,
            execution_state: None,
            // 额度条目**不写** effectOutcome：它不是一次执行的效果判定。
            effect_outcome: None,
            capability_versions: None,
            handle_count: 0,
        });
    }

    /// §7.6 取消（登记表侧）—— **D-01 的正式落点**：返回三字段
    /// [`CancelReceipt`]（`{ executionId, disposition, state }`），
    /// 让「不支持取消」与「已是终态」这两种**正常返回**能和真正的取消受理区分开。
    ///
    /// 冻结的 [`SessionPort::cancel_execution`] 里没有 `cancelHandle`，
    /// 它只交回状态一列；端口形状以冻结契约为准，处置语义在这里。两条路径
    /// 最终都走同一个 actor 命令、同一次绑定校验，端口不构成绕过。
    pub async fn cancel_registered(
        &self,
        handle: &SessionHandle,
        execution_id: &ExecutionId,
    ) -> Result<CancelReceipt, RuntimeError> {
        self.audit.pump();
        let record = self.locate(&handle.db_session_id)?;
        record
            .actor
            .control(|reply| ControlCommand::Cancel {
                handle: handle.clone(),
                execution_id: execution_id.clone(),
                cancel_handle: None,
                reply,
            })
            .await
    }

    /// §7.6 带调用方自带 cancelHandle 的取消入口（CM-24 的伪造面）。
    ///
    /// 冻结的 [`SessionPort::cancel_execution`] 形状里没有 `cancelHandle`，
    /// 伪造校验因此**不可能**通过它被触发；本方法是把那条校验暴露成可测的入口。
    /// 两条路径最终都比对 actor 在飞行开始时登记的同一个绑定——端口形状不同，
    /// 不等于校验可以更松。
    pub async fn cancel_execution_bound(
        &self,
        handle: &SessionHandle,
        execution_id: &ExecutionId,
        cancel_handle: &str,
    ) -> Result<CancelReceipt, RuntimeError> {
        self.audit.pump();
        let record = self.locate(&handle.db_session_id)?;
        record
            .actor
            .control(|reply| ControlCommand::Cancel {
                handle: handle.clone(),
                execution_id: execution_id.clone(),
                cancel_handle: Some(cancel_handle.to_owned()),
                reply,
            })
            .await
    }

    /// §7.5 关闭会话（登记表侧）。
    ///
    /// 释放顺序由 [`crate::registry::actor`] 负责：先在原资源上终结已登记句柄，
    /// 再关物理资源，最后才从登记表摘除并归还额度。
    pub async fn close_registered(
        &self,
        handle: &SessionHandle,
        mode: CloseMode,
    ) -> Result<(), RuntimeError> {
        self.audit.pump();
        let record = self.locate(&handle.db_session_id)?;
        let result = record
            .actor
            .exec(|reply| ExecCommand::Close {
                handle: handle.clone(),
                mode,
                reply,
            })
            .await;
        self.audit.pump();
        match result {
            Ok(state) => {
                // 关闭无论走到 `Closed` 还是 `Lost`，会话都已从登记表注销，
                // 额度都该归还——`Lost` 丢的是物理资源，不是额度账。
                self.forget(&record.db_session_id);
                let _ = state;
                Ok(())
            }
            // R-01：`Err` 不是一个语义，**两种**返回对应两种事实：
            //
            // * `CloseRejected` 是派发前的拒绝，物理资源原封不动——行与额度必须留着，
            //   否则一次被拒的关闭就凭空造出一个额度（账松了）。
            // * `SessionLost` 是 §9.4 四步**全部跑完**之后才写出来的：句柄已终结、
            //   物理资源已关闭、绑定已作废、墓碑已落 `Lost`。不可判定的是某个句柄的
            //   命运，不是「有没有关掉」。此时若照 `Err` 就把行留下，额度会**永久**卡死：
            //   上层看到的是「关不掉」，实际上它已经关掉了，重试多少次都是同一个答案。
            //
            // 无论哪种，返回给调用方的错误都**原样**传递——改写错误等于替不可判定背书。
            Err(error) => {
                if matches!(error, RuntimeError::SessionLost(_)) {
                    self.forget(&record.db_session_id);
                }
                Err(error)
            }
        }
    }

    /// §7.2 提交一次执行。冻结端口方法之外的具名入口，便于集成测试直连。
    pub async fn submit_execution(
        &self,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        self.audit.pump();
        let record = self.locate(&request.handle.db_session_id)?;
        record
            .actor
            .exec(|reply| ExecCommand::Execute { request, reply })
            .await
    }
}

#[async_trait]
impl SessionPort for SessionRegistry {
    async fn session_view(&self, handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
        self.audit.pump();
        let record = self.locate(&handle.db_session_id)?;
        record
            .actor
            .exec(|reply| ExecCommand::View {
                handle: handle.clone(),
                reply,
            })
            .await
    }

    async fn execute_in_session(
        &self,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        self.submit_execution(request).await
    }

    /// 冻结端口形状：只交回状态。处置在 [`SessionRegistry::cancel_registered`]——
    /// 投影而不是重算，两条路径因此不可能对同一次取消给出两种说法。
    async fn cancel_execution(
        &self,
        handle: &SessionHandle,
        execution_id: &ExecutionId,
    ) -> Result<ExecutionState, RuntimeError> {
        self.cancel_registered(handle, execution_id)
            .await
            .map(|receipt| receipt.state)
    }

    async fn close_session(
        &self,
        handle: &SessionHandle,
        mode: CloseMode,
    ) -> Result<(), RuntimeError> {
        self.close_registered(handle, mode).await
    }
}

// D-R2-2 的钉子测试放在独立文件里，与 `actor.rs` → `actor/tests.rs` 同一套分工：
// `registry.rs` 已经贴着单文件规模上限，用例再往里塞只会挤掉注释。
#[cfg(test)]
mod tests;
