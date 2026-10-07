//! FakeResourceProvider —— 假资源提供方。
//!
//! 本目录即模块表里 `P[FakeResourceProvider]` 与 `R[FakeResource state machine]` 两个
//! 节点的落点，职责拆到七个文件：
//!
//! | 文件 | 职责 |
//! |---|---|
//! | [`script`] | `FakeScript`：`FakeResourceProvider` 的故障/延迟脚本字段（故障种类与竞态） |
//! | [`state`] | `FakeResource` 状态机 + permit 记账锚点 |
//! | [`handles`] | 句柄铸造与「登记 vs 造句柄」 |
//! | [`ops`] | 八个操作的实现（`ResourceHandle` 逐操作校验） |
//! | [`close`] | op 9 `closeResource`：permit 归还口径 + 释放顺序 + 归池判据 |
//! | [`transaction`] | op 6 `transactionOperation`：回滚/提交对资源状态与 permit 的影响 |
//! | `catalog_guard` | 只在 `#[cfg(test)]` 下编译：把 `script.rs` 的 F 编号与文档表格**对撞**（编号不得错位） |
//! | `close_cases` | 只在 `#[cfg(test)]` 下编译：`closeResource` 的归池判据 / 凭证出口用例（句柄登记、驱动不支持重置、幂等关闭） |
//! | `close_unconfirmed` | 只在 `#[cfg(test)]` 下编译：`closeResource` 的「关闭未确认」用例 |
//!
//! **依赖方向**：本目录向下依赖 `connection::{port, session, types, capability, execution}`，
//! 向上只被 `journal` 的**测试**引用 —— 分层要求的「`journal.rs` 不依赖 `fake_resource`」
//! 因此在任何方向上都成立（`state.rs` 连 `journal` 与 `script` 都不碰）。
//!
//! 假提供方**不开任何出站 socket**；本目录没有任何网络或凭据代码。
//!
//! **测试辅助函数的落点**：[`tests`]、[`close_cases`] 与 [`close_unconfirmed`] 都从本文件底部那一组
//! `#[cfg(test)]` 辅助函数取 `provider` / `acquire` / `close_and_release`，
//! 而不是各自复制一份。复制会漂移：同一个 `pool_key` 派生算法写两遍，正例
//! （同池复用）和反例（换 key）就会在两个文件里各自成立。

#[cfg(test)]
mod catalog_guard;
mod close;
#[cfg(test)]
mod close_cases;
#[cfg(test)]
mod close_unconfirmed;
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
#[cfg(test)]
use crate::connection::error::ProviderError;
use crate::connection::port::PermitId;
#[cfg(test)]
use crate::connection::port::{AcquireResourceRequest, BudgetClass, CloseResourceRequest};
use crate::connection::testing::clock::FakeClock;
use crate::connection::testing::ids::FakeIds;
use crate::connection::testing::journal::CommandJournal;
#[cfg(test)]
use crate::connection::testing::journal::{JournalEntry, ResourceEvent};
use crate::connection::types::{ConfigRevision, ExecutionTarget, LeaseId, ResourceId, WorkerId};
#[cfg(test)]
use crate::connection::types::{
    ConnectionId, JobId, NamespaceTarget, OrganizationId, OwnerRef, PoolKeyFingerprint,
    PoolKeyInputs,
};

pub use handles::AcquiredResource;
pub use script::{EvictionRace, FakeScript, FaultKind, ResourceOp, ScriptStep};
pub use state::{Accounting, FakeResource, FakeResourceState, PermitEventPlan};

/// 假提供方自报的 `driverId`。所有资源描述、能力快照都用它。
pub const PROVIDER_ID: &str = "fake-sql";

/// 假提供方自报的 `driverVersion`。
const PROVIDER_VERSION: &str = "0.0.0-fake";

/// `PROFILE_P` 的 `configRevision`，作为假资源的默认值。
const DEFAULT_CONFIG_REVISION: u64 = 7;

/// 注入的「预算占满」窗口。只由 `ops.rs` 的 `acquire_resource` 写与读。
#[derive(Debug, Clone, Default)]
pub(crate) struct BudgetBusy {
    /// 以**假单调时钟**（`FakeClock`）的纳秒计，不是墙钟 —— 断言顺序只看 journal，不靠 sleep。
    until_nanos: u64,
    reason: &'static str,
}

/// `FakeResourceProvider`。
///
/// 字段与提供方的定义一一对应（`script, clock, ids, resources, journal, id_counter, worker_id`），
/// 另加三项由本实现的取值方式决定的字段：
///
/// - `target`：`AcquireResourceRequest` **没有** target 字段（见 `port.rs`），所以
///   资源要绑定的目标由构造时传入，之后每次 acquire 打在同一个夹具目标上。
/// - `execution_identity`：填进 `SessionContext::effective_identity`。做成 builder
///   而不是读 `fixtures.rs`，是为了不给依赖图加一条 `P → F` 边。
/// - `leases`：资源 → 钉在它上面的 Lease。`closeResource` 要据此归还 lease，
///   否则 lease 登记不变式（`live_leases.is_empty()`）在驱逐路径上无法收口。
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

    /// USER_A1 / USER_A2 共享 `IDENTITY_SHARED`，但 `policyIsolationKey` 不同。
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

    /// `configRevision`。`PoolKeyInputs` 派生指纹时必须用这里的值，
    /// 否则 `PROFILE_P` / `PROFILE_P_V2` 的换 key 判据会失效。
    pub fn config_revision(&self) -> ConfigRevision {
        self.config_revision
    }

    /// `execution_identity`，会进 `SessionContext::effective_identity`。
    pub fn execution_identity(&self) -> &str {
        self.execution_identity.as_str()
    }

    /// `worker_id: Option<WorkerId>`。`None` 表示该实例不声明归属。
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

    /// 仍在预算占用内的资源 id。
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

    /// `permitId` 用与 `resourceId` 同构的序号，便于按前缀在台账里比对。
    pub(crate) fn next_permit_id(&self) -> PermitId {
        let seq = self.id_counter.fetch_add(1, Ordering::Relaxed) + 1;
        PermitId(format!("pmt_{}_{seq:04}", self.ids.worker_id()))
    }

    /// 假单调时钟的纳秒值。**没有任何 sleep** —— 断言顺序只读 journal 的 `seq`。
    pub(crate) fn now_nanos(&self) -> u64 {
        self.clock.monotonic().as_nanos()
    }
}

// ---------------------------------------------------------------------------
// 测试脚手架 —— `tests.rs` / `close_cases.rs` / `close_unconfirmed.rs` 共用
// ---------------------------------------------------------------------------

/// 夹具目标一律取自 `fixtures`，用例里不写硬编码字面量。
#[cfg(test)]
pub(crate) fn target() -> ExecutionTarget {
    crate::connection::testing::harness::fixture_target(
        crate::connection::testing::fixtures::NS_A_KEY,
    )
}

/// `PROFILE_P` 归属：一个 job owner。`owner` 是 `OwnerRef`，不是裸 id。
#[cfg(test)]
pub(crate) fn owner() -> OwnerRef {
    OwnerRef::Job {
        organization_id: OrganizationId::new(crate::connection::testing::fixtures::ORG_A),
        job_id: JobId::new("job_org-alpha_0001"),
        stage_id: "job:job_org-alpha_0001/stage:1".to_owned(),
    }
}

/// `PoolKeyInputs` 派生。夹具走和
/// [`FakeHarness::pool_key`](crate::connection::testing::harness::FakeHarness::pool_key)
/// 完全一样的算法，
/// 否则「同池复用」的正例根本不成立（换 key 判据会失效）。
#[cfg(test)]
pub(crate) fn pool_key(
    provider: &FakeResourceProvider,
    policy_isolation_key: &str,
) -> PoolKeyFingerprint {
    PoolKeyFingerprint::derive(&PoolKeyInputs {
        connection_id: ConnectionId::new(crate::connection::testing::fixtures::PROFILE_P),
        config_revision: provider.config_revision(),
        driver_id: PROVIDER_ID.to_owned(),
        namespace: NamespaceTarget {
            database: provider.target().namespace.database.clone(),
            catalog: String::new(),
            schema: String::new(),
            path: String::new(),
        },
        execution_identity_key: provider.execution_identity().to_owned(),
        policy_isolation_key: policy_isolation_key.to_owned(),
    })
}

/// 默认假提供方：假时钟与 journal 共享同一个 `FakeClock`。
#[cfg(test)]
pub(crate) fn provider() -> FakeResourceProvider {
    FakeResourceProvider::new(WorkerId::new("w1"), target())
        .with_execution_identity(crate::connection::testing::fixtures::IDENTITY_SHARED)
}

/// 走一次 `acquire`。
#[cfg(test)]
pub(crate) fn acquire(provider: &FakeResourceProvider) -> Result<AcquiredResource, ProviderError> {
    provider.acquire(&AcquireResourceRequest {
        descriptor: provider.descriptor(),
        pool_key: pool_key(provider, "pol-1"),
        budget_class: BudgetClass::Session,
        owner: owner(),
        db_session_id: provider.ids().next_db_session_id(),
    })
}

/// 按归池前置条件（协议已排空 + 无登记句柄）关掉一张资源。
///
/// 名字不叫 `close`：本目录有个 `close` 模块，`use super::close` 会与函数同名，
/// 两个 import 撞在一起比难读的名字难查得多。
///
/// 需要与该前置条件**分道扬镳**的用例（`protocol_drained = false`、宿主自述句柄数
/// 与实测不符）必须自己构造 [`CloseResourceRequest`] 调 [`close_resource`]，
/// 绕开这个「合法路径」封装 —— 封装里把两个字段都填成最有利于归池的值。
#[cfg(test)]
pub(crate) fn close_and_release(
    provider: &FakeResourceProvider,
    acquired: &AcquiredResource,
) -> Result<(), ProviderError> {
    provider.close_resource(&CloseResourceRequest {
        handle: acquired.handle.clone(),
        registered_handles: provider.registered_handles(&acquired.resource_id),
        protocol_drained: true,
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 台账读数工具 —— 归到本文件而不是各自复制：判据类用例的判负必须数**台账上真实
// 出现过几次**（`ReturnedToPool` / `Closed` 的条数），复制计数逻辑等于把
// 「幂等只记一次」这条判据交给每个文件各写一遍
// ---------------------------------------------------------------------------

/// 如实填报宿主账本的关闭请求（正例口径）。
///
/// 刻意**不**复用 `close_and_release`：那个包装丢掉 `CloseReceipt`，而要断言回执的
/// 用例（关闭未确认那组）必须自己拿到它。
#[cfg(test)]
pub(crate) fn close_request(
    provider: &FakeResourceProvider,
    acquired: &AcquiredResource,
) -> CloseResourceRequest {
    CloseResourceRequest {
        handle: acquired.handle.clone(),
        registered_handles: provider.registered_handles(&acquired.resource_id),
        protocol_drained: true,
    }
}

/// 某张资源在台账上的事件名（按 `seq`）。
#[cfg(test)]
pub(crate) fn resource_event_names(
    provider: &FakeResourceProvider,
    id: &ResourceId,
) -> Vec<&'static str> {
    provider
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Resource {
                resource_id, event, ..
            } if resource_id == id => Some(event.as_str()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn pooled_count(provider: &FakeResourceProvider, id: &ResourceId) -> usize {
    provider
        .journal()
        .entries()
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                JournalEntry::Resource {
                    resource_id,
                    event: ResourceEvent::ReturnedToPool { .. },
                    ..
                } if resource_id == id
            )
        })
        .count()
}

#[cfg(test)]
pub(crate) fn closed_count(provider: &FakeResourceProvider, id: &ResourceId) -> usize {
    provider
        .journal()
        .entries()
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                JournalEntry::Resource {
                    resource_id,
                    event: ResourceEvent::Closed,
                    ..
                } if resource_id == id
            )
        })
        .count()
}
