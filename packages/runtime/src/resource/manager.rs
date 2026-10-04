//! [`ResourceManager`] 门面：签发、执行态、归还裁决、配置漂移。
//!
//! 这里是宿主台账对上层暴露的**唯一**写入口（A3.1/A1 的 `ResourceManager`）。
//! 轮换/禁用/排队在 `rotation`，候选替换在 `adoption`，数据结构在 `table`。
//!
//! 归还路径（`release`）是 §3.2 双条件裁决的落点：驱动结论与宿主条件**分别**进来，
//! 本模块只判定宿主自己的条件，两者同时通过才允许复用（A3.3 / CM-69）。

use std::sync::Arc;

use indexmap::IndexSet;

use crate::connection::{ConfigRevision, ConnectionId, ExecutionId, LeaseId};
use crate::resource::cleanup::{
    CleanupDisposition, CleanupPlan, CleanupReport, DriverCleanVerdict, HostConditionSnapshot,
    SharedTransport,
};
use crate::resource::generation::{CacheFillOutcome, CacheRevision, GenerationRegistry};
use crate::resource::lease::{LeaseRecord, LeaseRequest, LeaseState, PoolKeyGeneration};
use crate::resource::publication::DirectoryPublisher;
use crate::resource::replacement::ReplacementLedger;
use crate::resource::table::{ConfigRevisionDrift, QueuedLease, ResourceTable};
use crate::resource::{MonotonicSource, ResourceError, IDLE_POOL_TTL_SECONDS};

/// 宿主资源台账的写入口。
///
/// 字段一律 `pub(super)`：同属 `resource` 的 `rotation` / `adoption` 只能通过本类型
/// 的方法改账，不许自己动表结构——这样「谁能写台账」有唯一答案。
pub struct ResourceManager {
    pub(super) table: ResourceTable,
    pub(super) generations: GenerationRegistry,
    pub(super) ledger: ReplacementLedger,
    pub(super) transport: SharedTransport,
    pub(super) clock: Arc<dyn MonotonicSource>,
    pub(super) disabled: IndexSet<ConnectionId>,
    pub(super) queue: Vec<QueuedLease>,
    pub(super) idle_ttl_seconds: u64,
}

impl ResourceManager {
    pub fn new(transport: SharedTransport, clock: Arc<dyn MonotonicSource>) -> Self {
        Self {
            table: ResourceTable::new(),
            generations: GenerationRegistry::new(),
            ledger: ReplacementLedger::new(),
            transport,
            clock,
            disabled: IndexSet::new(),
            queue: Vec::new(),
            idle_ttl_seconds: IDLE_POOL_TTL_SECONDS,
        }
    }

    /// 接上公开目录端口（CM-68 的发布侧）。
    pub fn with_directory(mut self, publisher: Arc<dyn DirectoryPublisher>) -> Self {
        self.ledger.set_publisher(publisher);
        self
    }

    /// 接上公开目录端口（可变版本）。
    pub fn set_directory(&mut self, publisher: Arc<dyn DirectoryPublisher>) {
        self.ledger.set_publisher(publisher);
    }

    pub fn now_nanos(&self) -> u64 {
        self.clock.now_nanos()
    }

    /// 活跃物理连接数（= 槽位数）。**计数**不是句柄，可以公开。
    pub fn occupied_slots(&self) -> usize {
        self.table.occupied_slots()
    }

    /// 空池元数据条数（§9.2 上限 32）。
    pub fn empty_pool_metadata_len(&self) -> usize {
        self.table.empty_pool_metadata_len()
    }

    pub fn idle_lease_count(&self) -> usize {
        self.table.total_idle()
    }

    pub fn lease_count(&self) -> usize {
        self.table.all_leases().len()
    }

    pub fn lease(&self, lease_id: &LeaseId) -> Option<&LeaseRecord> {
        self.table.lease(lease_id)
    }

    pub fn leases(&self) -> Vec<LeaseRecord> {
        self.table.all_leases()
    }

    pub fn is_connection_disabled(&self, connection_id: &ConnectionId) -> bool {
        self.disabled.contains(connection_id)
    }

    // ---- 缓存代次闸门（CM-67） ----

    /// 当前缓存代次；连接没登记过返回 `None`。
    pub fn cache_revision(&self, connection_id: &ConnectionId) -> Option<CacheRevision> {
        self.generations.current(connection_id).cloned()
    }

    /// **发起**一次元数据抓取时拍下代次快照。
    ///
    /// 调用方必须把这张快照一路带到 `admit_cache_fill`；只有它能证明回填的是哪一代。
    pub fn capture_cache_revision(
        &mut self,
        request: &LeaseRequest,
    ) -> Result<CacheRevision, ResourceError> {
        let connection_id = &request.pool_key_inputs.connection_id;
        if let Some(current) = self.generations.current(connection_id) {
            return Ok(current.clone());
        }
        Ok(self.generations.register(
            connection_id,
            request.pool_key(),
            request
                .expected_config_revision
                .unwrap_or(ConfigRevision::new(1)),
        ))
    }

    /// 回填闸门：慢结果回来时按代次判定。
    ///
    /// 代次对不上 ⇒ `RejectedStale`，**没有任何「就地升级」的旁路**（A3.4 / CM-67）。
    pub fn admit_cache_fill(
        &self,
        connection_id: &ConnectionId,
        offered: &CacheRevision,
    ) -> CacheFillOutcome {
        self.generations.admit_fill(connection_id, offered)
    }

    // ---- 签发 ----

    /// 一次租约申请**该挂哪个池键**（CM-67 / CM-38 的唯一裁决点，签发与候选替换共用）。
    ///
    /// * 申请没声明代号（`None`）⇒ 宿主采用当前代新材料，调用方不必知道代次；
    /// * 申请声明了代号 ⇒ 必须**正好**等于当前代。声明的是上一代材料时直接拒绝，
    ///   绝不因为「反正连得上」就替它悄悄换个代 —— 那等于把轮换前的凭据又用了一次。
    ///
    /// 连接从未登记过时按申请自身的代号开代（首签）。
    pub(super) fn current_pool_key(
        &self,
        request: &LeaseRequest,
    ) -> Result<PoolKeyGeneration, ResourceError> {
        let connection_id = &request.pool_key_inputs.connection_id;
        let mut pool_key = request.pool_key();
        let Some(current) = self.generations.current(connection_id) else {
            return Ok(pool_key);
        };
        let current_key = current.pool_key.clone();
        let stale = |pinned: Option<u64>, current_revision: u64| matches!(pinned, Some(pinned) if pinned != current_revision);
        if stale(request.credential_revision, current_key.credential_revision)
            || stale(
                request.network_route_revision,
                current_key.network_route_revision,
            )
        {
            return Err(ResourceError::PoolKeyRotated
                .logged("lease request pinned to material from a rotated pool key generation"));
        }
        pool_key.credential_revision = current_key.credential_revision;
        pool_key.network_route_revision = current_key.network_route_revision;
        Ok(pool_key)
    }

    /// 签发一条租约：先查当前代的空闲池，没有才新建物理资源。
    pub fn acquire(&mut self, request: &LeaseRequest) -> Result<LeaseRecord, ResourceError> {
        let now_nanos = self.now_nanos();
        let connection_id = request.pool_key_inputs.connection_id.clone();
        if self.disabled.contains(&connection_id) {
            return Err(
                ResourceError::OwnerDisabled(connection_id.as_str().to_owned())
                    .logged("acquire on a disabled connection"),
            );
        }
        let pool_key = self.current_pool_key(request)?;

        // CM-38：带 `expectedRevision` 的申请必须与当前代一致，否则冲突。
        let current_revision = self.generations.current_config_revision(&connection_id);
        if let Some(expected) = request.expected_config_revision {
            let actual = current_revision.unwrap_or(ConfigRevision::new(0));
            if expected.get() != actual.get() {
                return Err(ResourceError::ConfigRevisionConflict {
                    expected: expected.get(),
                    actual: actual.get(),
                }
                .logged("acquire with a stale expectedConfigRevision"));
            }
        }

        // 淘汰本键下已过 TTL 的空闲（§9.2）。
        let (idle, expired) = self
            .table
            .take_idle(&pool_key, now_nanos, self.idle_ttl_seconds);
        for lease_id in expired {
            self.force_close(&lease_id)?;
        }

        if let Some(lease_id) = idle {
            if let Some(record) = self.table.lease_mut(&lease_id) {
                record.move_to(LeaseState::InUse)?;
                return Ok(record.clone());
            }
        }

        let resource_id = self.transport.open(request)?;
        let revision = match current_revision {
            Some(revision) => revision,
            None => {
                self.generations
                    .register(&connection_id, pool_key.clone(), ConfigRevision::new(1))
                    .config_revision
            }
        };
        let lease = LeaseRecord {
            lease_id: LeaseId::new(format!("lease-{}", resource_id.as_str())),
            resource_id: resource_id.clone(),
            connection_id,
            pool_key,
            purpose: request.purpose,
            owner: request.owner.clone(),
            config_revision: revision,
            state: LeaseState::Acquired,
            published: true,
            owner_key: request.owner.hash(),
            created_at_nanos: now_nanos,
            idle_since_nanos: None,
            active_execution: None,
            idle_for_issue: false,
        };
        self.table.insert(resource_id, lease.clone());
        let mut handed_out = lease;
        handed_out.move_to(LeaseState::InUse)?;
        if let Some(record) = self.table.lease_mut(&handed_out.lease_id) {
            *record = handed_out.clone();
        }
        Ok(handed_out)
    }

    /// 该租约现在能不能接受执行（候选未发布 ⇒ 不能）。
    pub fn accepts_execution(&self, lease_id: &LeaseId) -> Result<(), ResourceError> {
        self.table
            .lease(lease_id)
            .ok_or_else(|| ResourceError::UnknownResource(lease_id.as_str().to_owned()))?
            .accepts_execution()
    }

    pub fn begin_execution(
        &mut self,
        lease_id: &LeaseId,
        execution_id: ExecutionId,
    ) -> Result<(), ResourceError> {
        let record = self
            .table
            .lease_mut(lease_id)
            .ok_or_else(|| ResourceError::UnknownResource(lease_id.as_str().to_owned()))?;
        record.accepts_execution()?;
        record.active_execution = Some(execution_id);
        Ok(())
    }

    pub fn end_execution(&mut self, lease_id: &LeaseId) -> Result<(), ResourceError> {
        let record = self
            .table
            .lease_mut(lease_id)
            .ok_or_else(|| ResourceError::UnknownResource(lease_id.as_str().to_owned()))?;
        record.active_execution = None;
        Ok(())
    }

    // ---- 归还裁决（A3.3 / CM-69） ----

    /// 归还一条租约：驱动结论 + 宿主条件一起进来，本模块**只做宿主自己的检查**。
    pub fn release(
        &mut self,
        lease_id: &LeaseId,
        host: HostConditionSnapshot,
        driver: DriverCleanVerdict,
    ) -> Result<CleanupReport, ResourceError> {
        let now_nanos = self.now_nanos();
        let record = self
            .table
            .lease(lease_id)
            .ok_or_else(|| ResourceError::UnknownResource(lease_id.as_str().to_owned()))?
            .clone();
        if record.state != LeaseState::InUse {
            return Err(ResourceError::IllegalTransition {
                from: record.state,
                to: LeaseState::Acquired,
            }
            .logged("release a lease that is not in use"));
        }
        let mut plan = CleanupPlan::of(&record, &host, driver);

        // CM-73：还有未释放的会话级句柄时，必须先在**原资源**上复位（回滚事务、
        // 解除映射），再关。复位结果不明 ⇒ 进隔离，绝不静默丢弃，也不允许换一个新
        // 资源把旧事务蒙混过去。
        let needs_reset = host.unreleased_handles.iter().any(|handle| !handle.closed);
        let mut session_reset_performed = false;
        if plan.disposition == CleanupDisposition::Closed && needs_reset {
            match self.transport.reset(&record.resource_id) {
                Ok(()) => session_reset_performed = true,
                Err(error) => {
                    tracing::warn!(
                        lease_id = lease_id.as_str(),
                        reason = error.reason(),
                        "reset before close failed; the resource must not be reused"
                    );
                    plan.disposition = CleanupDisposition::Quarantined;
                    plan.next_state = LeaseState::Quarantined;
                }
            }
        }

        if let Some(entry) = self.table.lease_mut(lease_id) {
            entry.move_to(plan.next_state)?;
        }
        match plan.disposition {
            CleanupDisposition::ReturnedToPool => {
                if let Some(entry) = self.table.lease_mut(lease_id) {
                    entry.mark_idle(now_nanos);
                }
                self.table
                    .push_idle(record.lease_id.clone(), record.pool_key.clone(), now_nanos);
            }
            CleanupDisposition::Closed => {
                // FIXME(cross-track, CM-28 登记项 E)：这里的 `?` 与三处已定事实冲突。
                // **本轨不改** —— 改动会外溢到 CM-73 的交接面，只登记，不修：
                //
                // * `resource/cleanup.rs:482` 的 `PhysicalTransport::close` 文档：
                //   「关闭物理连接。返回 `Err` 表示关闭握手没确认，调用方必须转隔离而不是放行。」
                // * `resource/transition.rs` 给 `InUse→Closing` 写的 `exits_when`：
                //   「关闭确认 ⇒ Closed；若关闭过程本身状态不明 ⇒ Quarantined」。
                // * `docs/architecture/platform/connection-management.md:640`（§9.2
                //   「可调初始值」表 `cancel/cleanup deadline` 行的「行为」列）：
                //   「不能确认则隔离并核验」。
                //
                // 三处一致要求「不能确认就隔离」，而 `:657`（§9.4 归池前检查）
                // 「任一失败都关闭，即使 driver 返回 Clean 也不能绕过宿主检查。」要求
                // 失败之后**仍然**必须关闭。合起来，这条 `?` 把租约卡在 `Closing`
                // 是不自洽的：关闭义务被丢弃，既不隔离也不核销预算，并且与
                // `force_close` 把同一错误映成 `Quarantined` 的做法互相矛盾。
                //
                // 登记号 **E**，待跨轨裁定。见 `progress.md` 与 `hub.md`（协调者侧）。
                self.transport.close(&record.resource_id)?;
                if let Some(entry) = self.table.lease_mut(lease_id) {
                    entry.move_to(LeaseState::Closed)?;
                }
                self.table.forget(lease_id);
            }
            CleanupDisposition::Quarantined => {
                tracing::warn!(
                    lease_id = lease_id.as_str(),
                    reason = "host condition or driver verdict refused reuse",
                    "lease quarantined: it keeps its physical budget and is never issued again"
                );
            }
        }
        Ok(CleanupReport::from_plan(
            &record,
            &host,
            plan,
            now_nanos,
            session_reset_performed,
        ))
    }

    /// 强制关闭一条租约（隔离排障、轮换淘汰）。关闭失败 ⇒ 留在隔离并报错。
    pub fn retire(&mut self, lease_id: &LeaseId) -> Result<CleanupReport, ResourceError> {
        let now_nanos = self.now_nanos();
        let record = self
            .table
            .lease(lease_id)
            .ok_or_else(|| ResourceError::UnknownResource(lease_id.as_str().to_owned()))?
            .clone();
        self.force_close(lease_id)?;
        Ok(CleanupReport {
            resource_id: record.resource_id,
            lease_id: record.lease_id,
            decision: crate::resource::cleanup::ReturnDecision::CleanupRequired(
                crate::resource::cleanup::ReturnReason::DriverUnclean,
            ),
            disposition: CleanupDisposition::Closed,
            blockers: Vec::new(),
            outstanding_handles: Vec::new(),
            session_reset_performed: false,
            physical_budget_released: true,
            decided_at_nanos: now_nanos,
        })
    }

    pub(super) fn force_close(&mut self, lease_id: &LeaseId) -> Result<(), ResourceError> {
        let Some(record) = self.table.lease(lease_id).cloned() else {
            return Ok(());
        };
        self.table
            .drop_idle(lease_id, &record.pool_key, self.now_nanos());
        // 关闭是**有过程**的：先离开可签发态进入 `Closing`，关闭确认之后才落 `Closed`。
        // 终态不可回退，所以绝不能先把状态标成 `Closed` 再去关 —— 那样关闭失败就
        // 只能停在终态里，既进不了隔离，也丢掉了「关闭结果不明」这个事实。
        if let Some(entry) = self.table.lease_mut(lease_id) {
            match entry.state {
                LeaseState::Acquired | LeaseState::InUse => {
                    entry.move_to(LeaseState::Closing)?;
                }
                // 已有租约行 ⇒ 未被确认关闭 ⇒ 按 §9.4（`:657`「任一失败都关闭」）必须**再关一次**。
                // 因此关闭**尝试**的次数天然不受 CM-28「至多一次」约束，被约束的是
                // **被确认的**关闭 —— 口径见下方错误分支与 `tests/cm28_concurrent_release.rs`。
                LeaseState::Closing | LeaseState::Quarantined | LeaseState::Closed => {}
            }
        }
        match self.transport.close(&record.resource_id) {
            Ok(()) => {
                if let Some(entry) = self.table.lease_mut(lease_id) {
                    entry.move_to(LeaseState::Closed)?;
                }
                self.table.forget(lease_id);
                Ok(())
            }
            Err(error) => {
                // 关闭结果不明 ⇒ 转 `Quarantined`，等运维核验，绝不静默丢弃。
                //
                // 出处，逐字：`docs/architecture/platform/connection-management.md:657`
                // （§9.4 归池前检查）「归池条件是宿主检查全部通过 AND driver 返回 Clean。
                // ……任一失败都关闭，即使 driver 返回 Clean 也不能绕过宿主检查。reset 不支持、
                // 失败或超时直接关闭。」⇒ 关闭失败**不**终止关闭义务。
                // 同文 `:713`（§10.1 恢复决策表，`cleanup 未确认` 行）「保留预算占用/隔离资源；
                // 确认关闭或节点隔离后才核销」⇒ 未确认的关闭必须留一个可核验的去处，隔离就是它。
                //
                // **「有效关闭」的口径就定在这里。** CM-28（判据 `:1037`）只写「driver close
                // 至多一次**有效**关闭」，全文没有定义「有效」二字。取同一文档的既有词汇：
                // `:713` 的「确认关闭」、CM-26（`:1025`）的「close 未确认继续占预算；确认关闭后
                // 只核销一次」。故「有效关闭」≡ **被确认的关闭**。
                //
                // **为什么「至多一次」只能修饰「确认」而不能修饰「尝试」**：§9.4 `:657` 明写失败
                // 之后仍须关闭 —— 关闭尝试本就允许多次；若「至多一次」约束的是尝试次数，它与
                // `:657` 直接冲突。两条并列，唯一自洽的读法是「至多一次**被确认的**关闭」：
                // 20 个并发关闭请求里，第一个把关闭确认下来，其余请求只能看到墓碑并被拒绝。
                //
                // 契约测试（数的是物理端口上的 close 尝试数与确认数）：
                // `packages/runtime/tests/cm28_concurrent_release.rs`。
                if let Some(entry) = self.table.lease_mut(lease_id) {
                    let _ = entry.move_to(LeaseState::Quarantined);
                }
                Err(error.logged("force close a lease"))
            }
        }
    }
    /// 报告一条在用租约的版本漂移。宿主**不会**静默重配它。
    pub fn config_drift(&self, lease_id: &LeaseId) -> Option<ConfigRevisionDrift> {
        let record = self.table.lease(lease_id)?;
        let current = self
            .generations
            .current_config_revision(&record.connection_id)?;
        Some(ConfigRevisionDrift {
            lease_revision: record.config_revision,
            current_revision: current,
        })
    }
}
