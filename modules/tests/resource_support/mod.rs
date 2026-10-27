//! 租约 / 连接池层的测试替身与夹具。
//!
//! 为什么单独一个模块：`tests/` 编译 lib 时**不带 `cfg(test)`**，
//! `datazen_modules::execution::runtime::testing` 下的 fixture 在集成测试里根本不存在。
//! 所以这里按生产代码自己声明的公开接缝 —— [`MonotonicSource`]、`PhysicalTransport`、
//! `HandleCloser` —— 自建替身，而不是去改生产代码的可见性。
//!
//! 三条纪律：
//!
//! 1. **没有真实等待**：时间只经 [`MonotonicTestClock`] 手动推进，没有 `sleep`。
//!    台账时钟与物理时钟是**同一个源**，所以「61 秒后过期」这类边界不需要真的等。
//! 2. **物理动作有序**：[`PhysicalEvent`] 记的是顺序而不是计数 ——
//!    `Close` 排在 `Reset` 之后这种事实，计次是查不出来的，只能靠顺序断言。
//! 3. **断言里不写死额度**：额度压力由配置显式制造，账本状态一律读回来再比。

// 三个门禁二进制共用本文件，但各自只用其中一部分，所以整文件豁免 dead_code。
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use datazen_modules::shared::id::ExecutionId;

use datazen_modules::execution::runtime::budget::{
    BudgetClock, BudgetConfig, InProcessBudgetCoordinator,
};
use datazen_modules::execution::runtime::connection::port::CancelDisposition;
use datazen_modules::execution::runtime::connection::types::ClientInstanceId;
use datazen_modules::execution::runtime::connection::{
    ConfigRevision, ConnectionId, Counter, HandleId, HandleKind, NamespaceTarget, OwnerRef,
    PoolKeyInputs, ResourceId, SessionHandleRef,
};
use datazen_modules::execution::runtime::resource::{
    HostConditionSnapshot, LeaseManager, LeasePurpose, LeaseRequest, MonotonicSource,
    PhysicalTransport, ResetDisposition, ResourceError, ResourceManager,
};

use crate::support;

// ===========================================================================
// 时钟
// ===========================================================================

/// 手动推进的单调时钟：**同时**充当物理台账的单调源与预算台账的毫秒源。
pub struct MonotonicTestClock {
    nanos: AtomicU64,
}

impl MonotonicTestClock {
    /// 从 1ms 起 —— 非零起点让「时间倒流」这类缺陷不至于被 0 值掩盖。
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nanos: AtomicU64::new(1_000_000),
        })
    }

    pub fn advance(&self, by: Duration) {
        self.nanos.fetch_add(by.as_nanos() as u64, Ordering::SeqCst);
    }
}

impl MonotonicSource for MonotonicTestClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }
}

impl BudgetClock for MonotonicTestClock {
    fn now_ms(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst) / 1_000_000
    }
}

// ===========================================================================
// 物理端口替身
// ===========================================================================

/// 物理动作的**有序**记录：复位与关闭的先后关系只能靠顺序断言，计次是查不出来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalEvent {
    Open(ResourceId),
    Reset(ResourceId),
    Close(ResourceId),
}

#[derive(Debug, Default)]
pub struct Journal {
    pub events: Vec<PhysicalEvent>,
    pub close_refused: bool,
    pub reset_refused: bool,
    /// 驱动明确回答 `Discard`（**正常判决**，首版全部 path 驱动都走这一格）。
    ///
    /// 与 `reset_refused` 是**两格**，刻意不合并：那是握手没确认（`Err`），
    /// 这是拿到了确定的答复「无法证明已复位」。
    pub reset_discarded: bool,
}

pub struct ScriptedTransport {
    journal: Mutex<Journal>,
}

impl ScriptedTransport {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            journal: Mutex::new(Journal::default()),
        })
    }

    /// 让 `close` 握手失败（关闭未被确认）。事件仍然入账，因为**尝试过**也是事实。
    pub fn refuse_close(&self) {
        self.lock().close_refused = true;
    }

    pub fn accept_close(&self) {
        self.lock().close_refused = false;
    }

    /// 让 `reset` 失败。事件仍然入账 —— 与 [`Self::refuse_close`] 同规：
    /// **尝试过**复位也是事实，而「宿主跑没跑归池前那道复位」正是 §9.4 要问的。
    pub fn refuse_reset(&self) {
        self.lock().reset_refused = true;
    }

    pub fn accept_reset(&self) {
        self.lock().reset_refused = false;
    }

    /// 让驱动明确回复 `Discard`：**无法证明已复位**，不得再次签发。
    ///
    /// 名字里没有 `fail`，因为首版全部 path 驱动（15/15）都走这一格
    /// （`ResetForReuse::Unsupported`）—— 这是**常态**，不是故障注入。
    /// 事件照样入账：复位真的被请求过、也真的被答复过。
    pub fn discard_reset(&self) {
        self.lock().reset_discarded = true;
    }

    /// 某类物理动作的发生次数。
    pub fn count(&self, label: &'static str) -> usize {
        self.lock()
            .events
            .iter()
            .filter(|event| {
                matches!(
                    (label, event),
                    ("open", PhysicalEvent::Open(_))
                        | ("reset", PhysicalEvent::Reset(_))
                        | ("close", PhysicalEvent::Close(_))
                )
            })
            .count()
    }

    /// 物理动作的**顺序**标签。
    pub fn labels(&self) -> Vec<&'static str> {
        self.lock()
            .events
            .iter()
            .map(|event| match event {
                PhysicalEvent::Open(_) => "open",
                PhysicalEvent::Reset(_) => "reset",
                PhysicalEvent::Close(_) => "close",
            })
            .collect()
    }

    pub fn events(&self) -> Vec<PhysicalEvent> {
        self.lock().events.clone()
    }

    fn lock(&self) -> MutexGuard<'_, Journal> {
        // 断言失败一次不该把后续断言全变成 panic 噪声，所以不传播 poison。
        self.journal
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

impl PhysicalTransport for ScriptedTransport {
    fn open(&self, _request: &LeaseRequest) -> Result<ResourceId, ResourceError> {
        let mut journal = self.lock();
        let resource_id = ResourceId::new(format!(
            "res-{}",
            journal
                .events
                .iter()
                .filter(|event| matches!(event, PhysicalEvent::Open(_)))
                .count()
                + 1
        ));
        journal
            .events
            .push(PhysicalEvent::Open(resource_id.clone()));
        Ok(resource_id)
    }

    fn close(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        let mut journal = self.lock();
        journal
            .events
            .push(PhysicalEvent::Close(resource_id.clone()));
        if journal.close_refused {
            return Err(ResourceError::TransportRefused(
                "close refused by handshake",
            ));
        }
        Ok(())
    }

    fn reset(&self, resource_id: &ResourceId) -> Result<ResetDisposition, ResourceError> {
        let mut journal = self.lock();
        journal
            .events
            .push(PhysicalEvent::Reset(resource_id.clone()));
        if journal.reset_refused {
            return Err(ResourceError::TransportRefused(
                "reset refused by handshake",
            ));
        }
        if journal.reset_discarded {
            // 判决**原样**返回：宿主必须自己决定「Discard ⇒ 不能进池」。
            return Ok(ResetDisposition::Discard);
        }
        Ok(ResetDisposition::Clean)
    }

    fn cancel(
        &self,
        _resource_id: &ResourceId,
        _execution_id: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError> {
        Ok(CancelDisposition::Requested)
    }
}

// ===========================================================================
// 申请构造
// ===========================================================================

pub fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-it-1"),
    }
}

pub fn pool_inputs(connection_id: &ConnectionId, database: &str) -> PoolKeyInputs {
    PoolKeyInputs {
        connection_id: connection_id.clone(),
        config_revision: ConfigRevision::new(1),
        driver_id: "postgres".to_owned(),
        namespace: NamespaceTarget {
            database: database.to_owned(),
            catalog: "appdb".to_owned(),
            schema: "public".to_owned(),
            path: "appdb.public".to_owned(),
        },
        execution_identity_key: "owner-hash-1".to_owned(),
        policy_isolation_key: "policy-a".to_owned(),
    }
}

/// 短操作：唯一允许归还池的用途。
pub fn main_request() -> LeaseRequest {
    LeaseRequest::new(
        pool_inputs(&ConnectionId::new(support::CONN), "appdb"),
        owner(),
        LeasePurpose::ShortOperation,
    )
}

/// 固定会话：用途本身就不允许归还池（§6.2「不参与归还」）。
pub fn fixed_request() -> LeaseRequest {
    LeaseRequest::new(
        pool_inputs(&ConnectionId::new(support::CONN), "appdb"),
        owner(),
        LeasePurpose::FixedSession,
    )
}

/// 元数据用途：也是可归还的，但它和短操作走同一个类桶。
pub fn metadata_request() -> LeaseRequest {
    LeaseRequest::new(
        pool_inputs(&ConnectionId::new(support::CONN), "appdb"),
        owner(),
        LeasePurpose::Metadata,
    )
}

/// 换一个数据库 ⇒ 换一个池键（指纹是池键的一半）。
pub fn other_database_request() -> LeaseRequest {
    LeaseRequest::new(
        pool_inputs(&ConnectionId::new(support::CONN), "reporting"),
        owner(),
        LeasePurpose::ShortOperation,
    )
}

/// 四个宿主阻断项**全清**的快照。
pub fn clear_host() -> HostConditionSnapshot {
    HostConditionSnapshot::default()
}

pub fn handle(id: &str, resource_id: &ResourceId) -> SessionHandleRef {
    SessionHandleRef::new(
        HandleId::new(id.to_owned()),
        HandleKind::Cursor,
        resource_id.clone(),
        Counter::new(1),
    )
}

// ===========================================================================
// 夹具
// ===========================================================================

pub struct Fixture {
    pub manager: LeaseManager,
    pub transport: Arc<ScriptedTransport>,
    pub budget: Arc<InProcessBudgetCoordinator>,
    pub clock: Arc<MonotonicTestClock>,
}

/// 按给定额度配置搭一套 `LeaseManager`。
///
/// 主体解析用固定替身：空表 ⇒ 回落到「组织即主体」，与生产默认一致，
/// 但不依赖 `std::time::Instant`，台账时间完全由 `clock` 决定。
pub fn build(config: BudgetConfig) -> Fixture {
    let transport = ScriptedTransport::new();
    let clock = MonotonicTestClock::new();
    let resources = ResourceManager::new(transport.clone(), clock.clone());
    let budget = Arc::new(
        InProcessBudgetCoordinator::new(config, support::org())
            .with_clock(clock.clone())
            .with_principals(support::FixedPrincipals::new(Vec::new())),
    );
    let manager = LeaseManager::new(resources, budget.clone())
        .with_principals(support::FixedPrincipals::new(Vec::new()));
    Fixture {
        manager,
        transport,
        budget,
        clock,
    }
}

/// 宽裕配置：每类都留了保留位，额度压力只由测试自己制造。
pub fn fixture() -> Fixture {
    build(support::config(8, [1, 2, 1, 0]))
}

/// 紧凑配置：总额 2、无保留位 —— 第三次物理占用必然超限。
pub fn tight_fixture() -> Fixture {
    build(support::config(2, [0, 0, 0, 0]))
}

/// `(在场许可数, 累计放行, 累计消费, 累计归还)`。
///
/// 台账把「消费」和「归还」分成两个计数器（`budget/ledger.rs:414`）：
/// `consumed_total` 是**真的占过物理槽位**的许可，`returned_total` 是额度原样退回、
/// 什么都没干的空转许可。本层核销的每一条许可背后都有一个真实开过的物理会话，
/// 所以第四位恒为 0 —— 它一旦非 0，就说明有人在给没干活的空名额记账。
pub fn counters(budget: &InProcessBudgetCoordinator) -> (usize, u64, u64, u64) {
    budget.with_ledger(|ledger| {
        (
            ledger.permits().count(),
            ledger.granted_total(),
            ledger.consumed_total(),
            ledger.returned_total(),
        )
    })
}
