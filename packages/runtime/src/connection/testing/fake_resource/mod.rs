//! FakeResourceProvider —— 假资源提供方（fake-runtime-fixtures.md §2、§3、§4.1、§5.1）。
//!
//! 本目录即 §2 模块表里 `P[FakeResourceProvider]` 与 `R[FakeResource state machine]` 两个
//! 节点的落点，职责按 §3.1 拆到六个文件：
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`script`] | `FakeScript`：`FakeResourceProvider` 的故障/延迟脚本字段（§4.1 F1–F12、§9.3 竞态） |
//! | [`state`] | `FakeResource` 状态机 + permit 记账锚点（§3.1、§3.2） |
//! | [`handles`] | 句柄铸造与「登记 vs 造句柄」（§5.1 L416、§9.1、§9.2） |
//! | [`ops`] | 八个操作的实现（§3.1 `ResourceHandle` 逐操作校验） |
//! | [`close`] | op 9 `closeResource`：§5.3 规则 2/3/6 的 permit 口径 + CM-74 释放顺序 + §9.4(b) 归池判据 |
//! | `catalog_guard` | 只在 `#[cfg(test)]` 下编译：把 `script.rs` 的 F 编号与 §4.1 的表**对撞**（§4.1 F1–F12 不得错位） |
//!
//! **依赖方向**：本目录向下依赖 `connection::{port, session, types, capability, execution}`，
//! 向上只被 `journal` 的**测试**引用 —— §2 要求的「`journal.rs` 不依赖 `fake_resource`」
//! 因此在任何方向上都成立（`state.rs` 连 `journal` 与 `script` 都不碰）。
//!
//! §13：假提供方**不开任何出站 socket**；本目录没有任何网络或凭据代码。

#[cfg(test)]
mod catalog_guard;
mod close;
mod handles;
mod ops;
mod script;
mod state;
#[cfg(test)]
mod tests;
mod transaction;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;

use crate::connection::capability::CapabilitySnapshot;
use crate::connection::port::PermitId;
use crate::connection::testing::clock::FakeClock;
use crate::connection::testing::ids::FakeIds;
use crate::connection::testing::journal::CommandJournal;
use crate::connection::types::{ConfigRevision, ExecutionTarget, LeaseId, ResourceId, WorkerId};

pub use handles::AcquiredResource;
pub use script::{EvictionRace, FakeScript, FaultKind, ResourceOp, ScriptStep};
pub use state::{Accounting, FakeResource, FakeResourceState, PermitEventPlan};

/// 假提供方自报的 `driverId`。所有资源描述、能力快照都用它。
pub const PROVIDER_ID: &str = "fake-sql";

/// 假提供方自报的 `driverVersion`。
const PROVIDER_VERSION: &str = "0.0.0-fake";

/// §8.1 `PROFILE_P` 的 `configRevision`，作为假资源的默认值。
const DEFAULT_CONFIG_REVISION: u64 = 7;

/// F2 注入的「预算占满」窗口。只由 `ops.rs` 的 `acquire_resource` 写与读。
#[derive(Debug, Clone)]
pub(crate) struct BudgetBusy {
    /// 以**假单调时钟**（`FakeClock`）的纳秒计，不是墙钟 —— 断言顺序只看 journal，不靠 sleep。
    until_nanos: u64,
    reason: &'static str,
}

impl Default for BudgetBusy {
    fn default() -> Self {
        Self {
            until_nanos: 0,
            reason: "",
        }
    }
}

/// §3.1 的 `FakeResourceProvider`。
///
/// 字段与 §3.1 的定义一一对应（`script, clock, ids, resources, journal, id_counter, worker_id`），
/// 另加三项由本实现的取值方式决定的字段：
///
/// - `target`：`AcquireResourceRequest` **没有** target 字段（见 `port.rs`），所以
///   资源要绑定的目标由构造时传入，之后每次 acquire 打在同一个夹具目标上。
/// - `execution_identity`：填进 `SessionContext::effective_identity`。做成 builder
///   而不是读 `fixtures.rs`，是为了不给 §2 的 DAG 加一条 `P → F` 边。
/// - `leases`：资源 → 钉在它上面的 Lease。`closeResource` 要据此归还 lease，
///   否则 §4.3 的 I3（`live_leases.is_empty()`）在驱逐路径上无法收口。
pub struct FakeResourceProvider {
    pub(crate) script: Arc<FakeScript>,
    pub(crate) clock: Arc<FakeClock>,
    pub(crate) ids: Arc<FakeIds>,
    pub(crate) journal: CommandJournal,
    pub(crate) resources: Mutex<IndexMap<String, FakeResource>>,
    pub(crate) leases: Mutex<IndexMap<String, Vec<LeaseId>>>,
    pub(crate) budget_busy: Mutex<BudgetBusy>,
    pub(crate) id_counter: AtomicU64,
    pub(crate) worker_id: Option<WorkerId>,
    pub(crate) target: ExecutionTarget,
    pub(crate) execution_identity: String,
    pub(crate) config_revision: ConfigRevision,
}

impl FakeResourceProvider {
    /// 构造一个假提供方。`clock` 与 `journal` 共享同一个 `FakeClock`，
    /// 于是「推进假时钟」与「journal 记录的单调时间」永远一致。
    pub fn new(worker_id: WorkerId, target: ExecutionTarget) -> Self {
        let clock = Arc::new(FakeClock::new());
        let journal = CommandJournal::new(FakeClock::clone(&clock));
        Self {
            script: Arc::new(FakeScript::new()),
            clock,
            ids: Arc::new(FakeIds::new(worker_id.clone())),
            journal,
            resources: Mutex::new(IndexMap::new()),
            leases: Mutex::new(IndexMap::new()),
            budget_busy: Mutex::new(BudgetBusy::default()),
            id_counter: AtomicU64::new(0),
            worker_id: Some(worker_id),
            target,
            execution_identity: crate::connection::types::UNKNOWN_SENTINEL.to_owned(),
            config_revision: ConfigRevision::new(DEFAULT_CONFIG_REVISION),
        }
    }

    /// §8.1：USER_A1 / USER_A2 共享 `IDENTITY_SHARED`，但 `policyIsolationKey` 不同。
    /// 这里是夹具选择执行身份的唯一入口。
    pub fn with_execution_identity(mut self, identity: impl Into<String>) -> Self {
        self.execution_identity = identity.into();
        self
    }

    /// 换成共享 journal：驱动与用例侧看到同一条台账。
    pub fn with_shared_journal(mut self, journal: CommandJournal) -> Self {
        self.journal = journal;
        self
    }

    pub fn with_config_revision(mut self, revision: ConfigRevision) -> Self {
        self.config_revision = revision;
        self
    }

    pub fn with_worker_id(mut self, worker_id: WorkerId) -> Self {
        self.worker_id = Some(worker_id);
        self
    }

    // ---- 只读访问器 ----

    pub fn script(&self) -> &FakeScript {
        &self.script
    }

    pub fn clock(&self) -> &FakeClock {
        &self.clock
    }

    pub fn ids(&self) -> &FakeIds {
        &self.ids
    }

    pub fn journal(&self) -> &CommandJournal {
        &self.journal
    }

    pub fn target(&self) -> &ExecutionTarget {
        &self.target
    }

    /// §8.1 的 `configRevision`。`PoolKeyInputs` 派生指纹时必须用这里的值，
    /// 否则 `PROFILE_P` / `PROFILE_P_V2` 的换 key 判据会失效（CM-05 / CM-67）。
    pub fn config_revision(&self) -> ConfigRevision {
        self.config_revision
    }

    /// §3.1 的 `execution_identity`，会进 `SessionContext::effective_identity`。
    pub fn execution_identity(&self) -> &str {
        self.execution_identity.as_str()
    }

    /// §3.1 的 `worker_id: Option<WorkerId>`。`None` 表示该实例不声明归属。
    pub fn worker_id(&self) -> Option<&WorkerId> {
        self.worker_id.as_ref()
    }

    /// 能力快照。所有假资源共享同一份。
    pub fn capabilities(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::defaults(PROVIDER_ID.to_owned(), PROVIDER_VERSION.to_owned())
    }

    /// 拿一份资源快照（克隆）。测试用它读状态，不拿它做写操作。
    pub fn resource(&self, resource_id: &ResourceId) -> Option<FakeResource> {
        self.lock().get(resource_id.as_str()).cloned()
    }

    pub fn resources(&self) -> Vec<FakeResource> {
        self.lock().values().cloned().collect()
    }

    /// 仍在预算占用内的资源 id（§4.3 的 I2 判据）。
    pub fn live_resources(&self) -> Vec<ResourceId> {
        self.lock()
            .values()
            .filter(|resource| resource.state.occupies_budget())
            .map(|resource| resource.resource_id.clone())
            .collect()
    }

    /// 找一张同 `pool_key` 且处于 `Ready` 的可复用资源。
    pub fn reusable_resource(
        &self,
        pool_key: &crate::connection::types::PoolKeyFingerprint,
    ) -> Option<ResourceId> {
        self.lock()
            .values()
            .find(|resource| resource.state.is_reusable() && &resource.pool_key == pool_key)
            .map(|resource| resource.resource_id.clone())
    }

    // ---- 内部工具 ----

    /// 资源表锁。`Mutex` 中毒时取内层值继续跑：夹具若因某个断言失败而 panic，
    /// 后续用例仍要能读到台账并给出完整失败信息。
    pub(crate) fn lock(&self) -> MutexGuard<'_, IndexMap<String, FakeResource>> {
        self.resources
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn leases_of(&self, resource_id: &ResourceId) -> Vec<LeaseId> {
        self.leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(resource_id.as_str())
            .cloned()
            .unwrap_or_default()
    }

    /// §8.2：`permitId` 用与 `resourceId` 同构的序号，便于按前缀在台账里比对。
    pub(crate) fn next_permit_id(&self) -> PermitId {
        let seq = self.id_counter.fetch_add(1, Ordering::Relaxed) + 1;
        PermitId(format!("pmt_{}_{seq:04}", self.ids.worker_id()))
    }

    /// 假单调时钟的纳秒值。**没有任何 sleep** —— 断言顺序只读 journal 的 `seq`。
    pub(crate) fn now_nanos(&self) -> u64 {
        self.clock.monotonic().as_nanos()
    }
}
