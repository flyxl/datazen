//! CM-74 断言 3 的定点用例：**driver 返回 `Clean` 时若宿主仍有已登记句柄，
//! 宿主检查必须失败**（§9.4）。
//!
//! 与既有三份覆盖的分工（三者不重叠，合并才构成断言 3 的完整证据）：
//!
//! | 落点 | 已有用例 | 钉住什么 |
//! |---|---|---|
//! | `ClosePrecondition::returns_to_pool()` | `fake_resource::close_cases::pooling_refuses_a_host_ledger_that_undercounts_the_registered_handles` | 实测 vs 声明两份账分叉 ⇒ 不归池 |
//! | `HostConditionSnapshot::first_blocker()` | `tests::resource_return_to_pool::an_unreleased_session_handle_refuses_reuse_and_is_reset_before_the_close` | host 传入 `unreleased_handles` ⇒ 报 `hostUnreleasedSessionHandle` |
//! | **`CleanupPlan::of` 的归池判据** | **本文件** | **驱动 `Clean` 单独一个不能覆盖任何一项宿主条件** |
//!
//! 第三行不是前两行的复述：前两行各自只测「半边」。`CleanupPlan::of` 才是把两个半边
//! 合成 `ReturnedToPool` 的那个点，也是最容易在重构里被悄悄改坏的点。
//!
//! # 为什么这些用例能杀死特定变形
//!
//! 归池判据有一处**唯一正确**的读法：四项宿主条件**在裁决时**必须全空。由此推出三个
//! 变形，每个都被本文件里至少一条用例钉死：
//!
//! 1. **把句柄那一格从 `first_blocker()` 里删掉** ⇒
//!    [`a_clean_driver_verdict_never_carries_a_registered_handle_into_the_pool`] 转红：
//!    阻断项会变成 `none`，而它断言的是 `hostUnreleasedSessionHandle`。
//! 2. **只看 driver 的 `Clean`、不看宿主那四项** ⇒ 同一条用例转红（`returned_to_pool()`
//!    会变成真）。这也是本条用例存在的首要理由：`DriverCleanVerdict` 只有一个私有 bool，
//!    「驱动说行」与「宿主说行」是两个独立事实。
//! 3. **读注销之后的余量而不是裁决时的快照** ⇒ 本文件用 `LeaseManager` 覆盖的那一条
//!    （见下）专治这个：`lease_manager` 第 (1) 步先 `closer.close` 再 `deregister`，
//!    归还时登记册已经空了。判据若改读「现在还有几个」，这里就当场放行归池。
//!
//! 全程**不 sleep**：时间只经 [`TestClock::advance`] 手动推进，物理动作由
//! [`ScriptedTransport`] 按发生顺序记账。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use datazen_modules::shared::id::OrganizationId;

use datazen_modules::execution::runtime::budget::{
    BudgetClock, BudgetConfig, InProcessBudgetCoordinator, OrganizationPrincipal,
};
use datazen_modules::execution::runtime::connection::port::CancelDisposition;
use datazen_modules::execution::runtime::connection::types::ClientInstanceId;
use datazen_modules::execution::runtime::connection::{
    ConfigRevision, ConnectionId, Counter, ExecutionId, HandleId, HandleKind, NamespaceTarget,
    OwnerRef, PoolKeyInputs, ResourceId, SessionHandleRef,
};
use datazen_modules::execution::runtime::resource::{
    CleanupDisposition, DriverCleanVerdict, HostConditionSnapshot, LeaseManager, LeasePurpose,
    LeaseRequest, LeaseState, MonotonicSource, NoHandleCloser, PhysicalTransport, ResetDisposition,
    ResourceError, ResourceManager,
};

/// 夹具自报的组织。`LeaseManager` 的预算协调器按它开账。
const ORG: &str = "org-cm74-assert3";

// ---- 时钟替身：同时充当物理台账单调源与预算台账毫秒源 ----

/// 从 1ms 起 —— 非零起点让「时间倒流」这类缺陷不至于被 0 值掩盖。
struct TestClock {
    nanos: AtomicU64,
}

impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            nanos: AtomicU64::new(1_000_000),
        })
    }
}

impl MonotonicSource for TestClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }
}

impl BudgetClock for TestClock {
    fn now_ms(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst) / 1_000_000
    }
}

// ---- 物理端口替身：按**发生顺序**记账 ----

/// 物理动作的有序记录。复位与关闭的先后关系只能靠顺序断言，计次查不出来。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PhysicalEvent {
    Open(ResourceId),
    Reset(ResourceId),
    Close(ResourceId),
}

#[derive(Debug, Default)]
struct Journal {
    events: Vec<PhysicalEvent>,
    issued: u64,
}

struct ScriptedTransport {
    journal: Mutex<Journal>,
}

impl ScriptedTransport {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            journal: Mutex::new(Journal::default()),
        })
    }

    fn events(&self) -> Vec<PhysicalEvent> {
        self.lock().events.clone()
    }

    /// 关句柄那一刻，物理层已经做过多少次 reset / close。
    ///
    ///  caller 在 [`HandleCloser`] 里调它 —— 顺序是这里唯一要证的命题。
    fn physical_progress(&self) -> (usize, usize) {
        let events = self.lock().events.clone();
        let resets = events
            .iter()
            .filter(|event| matches!(event, PhysicalEvent::Reset(_)))
            .count();
        let closes = events
            .iter()
            .filter(|event| matches!(event, PhysicalEvent::Close(_)))
            .count();
        (resets, closes)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Journal> {
        self.journal
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

impl PhysicalTransport for ScriptedTransport {
    fn open(&self, _request: &LeaseRequest) -> Result<ResourceId, ResourceError> {
        let mut journal = self.lock();
        journal.issued += 1;
        let resource_id = ResourceId::new(format!("res-{}", journal.issued));
        journal
            .events
            .push(PhysicalEvent::Open(resource_id.clone()));
        Ok(resource_id)
    }

    fn close(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        self.lock()
            .events
            .push(PhysicalEvent::Close(resource_id.clone()));
        Ok(())
    }

    fn reset(&self, resource_id: &ResourceId) -> Result<ResetDisposition, ResourceError> {
        self.lock()
            .events
            .push(PhysicalEvent::Reset(resource_id.clone()));
        // 本文件只关心「宿主某一侧有没有放行」，复位一律报 `Clean`：
        // 让复位自己变成拒绝理由，就分不清拒绝来自驱动还是来自宿主了。
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

// ---- 申请构造 ----

fn connection(id: &str) -> ConnectionId {
    ConnectionId::new(id.to_owned())
}

fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-cm74-a3"),
    }
}

/// 短操作租约：用途允许归还池，所以双条件裁决对它**真实**成立
/// （换成 `FixedSession` 的话，归池被用途这一格挡掉，断言 3 就什么都测不到了）。
fn request(connection_id: &ConnectionId, database: &str) -> LeaseRequest {
    LeaseRequest::new(
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
            execution_identity_key: "owner-hash-cm74".to_owned(),
            policy_isolation_key: "policy-a".to_owned(),
        },
        owner(),
        LeasePurpose::ShortOperation,
    )
}

fn manager() -> (ResourceManager, Arc<ScriptedTransport>, Arc<TestClock>) {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let manager = ResourceManager::new(transport.clone(), clock.clone());
    (manager, transport, clock)
}

/// 一条挂着会话句柄的短操作租约 + 它的句柄。
fn leased_with_handle(
    manager: &mut ResourceManager,
    connection_id: &ConnectionId,
    handle_id: &str,
) -> (
    datazen_modules::execution::runtime::connection::LeaseId,
    ResourceId,
    SessionHandleRef,
) {
    let leased = manager
        .acquire(&request(connection_id, "appdb"))
        .expect("a fresh request must open a physical resource");
    let handle = SessionHandleRef::new(
        HandleId::new(handle_id.to_owned()),
        HandleKind::Transaction,
        leased.resource_id.clone(),
        Counter::new(1),
    );
    (leased.lease_id, leased.resource_id, handle)
}

// ===========================================================================
// 1. 判据那一格：`CleanupPlan::of` 裁决时必须读「此刻还有没有登记句柄」
// ===========================================================================

/// driver 报 `Clean`、宿主四项**全清**、但还有一条**已登记**的事务句柄 ⇒
/// 宿主检查必须失败：**不得归池，必须关闭**。
///
/// 这条用例杀死的变形：把「未释放句柄」这一格从 `first_blocker()` 里删掉，
/// 或让 `Returned` 只看 driver 的那一个 bool。两种变形下它都会看到
/// `returned_to_pool() == true`，而那正是 §9.4 点名禁止的：
/// 「driver 对已交出的句柄没有可见性，其 `Clean` 不构成事务终结的证据」。
#[test]
fn a_clean_driver_verdict_never_carries_a_registered_handle_into_the_pool() {
    let (mut manager, transport, _clock) = manager();
    let connection_id = connection("conn-cm74-a3");
    let (lease_id, resource_id, handle) =
        leased_with_handle(&mut manager, &connection_id, "hdl-cm74-a3-1");

    // 注意：这里宿主四项全清，只有「句柄」这一格非空。驱动报 Clean。
    let report = manager
        .release(
            &lease_id,
            HostConditionSnapshot::default().with_unreleased_handle(handle.clone()),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("a refusal is reported as a plan, not surfaced as a ledger error");

    assert!(
        !report.returned_to_pool(),
        "driver Clean does not terminate a transaction the host never saw: §9.4 \
         refuses the park while a §6.5 handle is still registered"
    );
    assert!(
        !report.quarantined(),
        "an unreleased handle is a close reason, not a quarantine reason: \
         the rollback is performed on the original resource first"
    );
    assert_eq!(report.disposition, CleanupDisposition::Closed);
    assert_eq!(
        report.blockers.first().map(|reason| reason.reason_code()),
        Some("hostUnreleasedSessionHandle"),
        "the blocker must name the registered handle, not fall through to none"
    );
    assert_eq!(manager.idle_lease_count(), 0, "a refused lease never parks");

    // 句柄必须在**原资源**上被复位（回滚 + 解除映射），然后才关。
    assert!(
        report.session_reset_performed,
        "the host must roll the handle back on the original resource before closing"
    );
    assert!(
        report.outstanding_handles.is_empty(),
        "after a performed reset no handle may still be committable"
    );
    assert_eq!(
        transport.events(),
        vec![
            PhysicalEvent::Open(resource_id.clone()),
            PhysicalEvent::Reset(resource_id.clone()),
            PhysicalEvent::Close(resource_id),
        ],
        "the reset must land on the original resource and precede the close"
    );
}

/// 同一条判据的**另一半**：游标与事务是两种 `HandleKind`，登记表对它们一视同仁。
///
/// 单测一个 kind 会留下「只拦事务、漏了游标」的变形空间，而 §6.5 把三者并列：
/// 事务、游标、服务端预处理对象。
#[test]
fn a_clean_driver_verdict_never_carries_a_registered_cursor_into_the_pool() {
    let (mut manager, _transport, _clock) = manager();
    let connection_id = connection("conn-cm74-a3-cursor");
    let (lease_id, _resource_id, _) =
        leased_with_handle(&mut manager, &connection_id, "hdl-cm74-a3-cursor");
    let cursor = SessionHandleRef::new(
        HandleId::new("hdl-cm74-a3-cursor"),
        HandleKind::Cursor,
        _resource_id,
        Counter::new(1),
    );

    let report = manager
        .release(
            &lease_id,
            HostConditionSnapshot::default().with_unreleased_handle(cursor),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("a refusal is reported, not an error");

    assert!(!report.returned_to_pool());
    assert_eq!(
        report.blockers.first().map(|reason| reason.reason_code()),
        Some("hostUnreleasedSessionHandle"),
        "a cursor is a §6.5 handle like any other: the host check must fail for it too"
    );
    assert_eq!(manager.idle_lease_count(), 0);
}

/// **已关闭**的句柄不再阻断归池 —— 否则判据会退化成「碰过句柄就永远不许复用」，
/// 那条租约再怎么彻底收尾都回不了池。
///
/// 这条同时是上一条的对照：没有它，「永远拒绝」与「判据在判」在断言上长得一样。
#[test]
fn a_closed_session_handle_no_longer_blocks_the_return() {
    let (mut manager, _transport, _clock) = manager();
    let connection_id = connection("conn-cm74-a3-closed");
    let (lease_id, _resource_id, mut handle) =
        leased_with_handle(&mut manager, &connection_id, "hdl-cm74-a3-closed");
    handle.closed = true;

    let report = manager
        .release(
            &lease_id,
            HostConditionSnapshot::default().with_unreleased_handle(handle),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("a closed handle leaves the park decision to the other gates");

    assert!(
        report.returned_to_pool(),
        "a closed handle is not an outstanding obligation: the lease must park"
    );
    assert_eq!(manager.idle_lease_count(), 1);
    assert_eq!(report.disposition, CleanupDisposition::ReturnedToPool);
}

// ===========================================================================
// 2. 生产路径：`LeaseManager::release` 上同一组合必须同样被拒
// ===========================================================================

/// 断言 3 在**生产**路径上的落点：`LeaseManager::release`。
///
/// 这里是三个既有覆盖都够不到的地方，也是最容易「看起来对」的地方。
/// `lease_manager` 的顺序是：
///
/// ```text
/// (1) for handle in self.handles.for_lease(lease) { closer.close(&handle)?; deregister(..); }
/// (2) merged.unreleased_handles = host 里未关闭的那一份  ∪  登记册此刻还剩的
/// (3) self.resources.release(lease, merged, driver)?        ← 这里才裁决
/// ```
///
/// 第 (2) 步的正确形状是**并集**：本层是登记册的唯一权威，但它并入的必须是
/// 「登记册**此刻还剩**的句柄」，不是「关之前有过哪些」。差别是实质的：
///
/// * 关得掉 ⇒ (1) 已经摘干净 ⇒ 并入空 ⇒ 归池前置回到四项全清，租约**可以**归池。
///   这正是 `lease_handle_registry.rs:102` 与 `lease_transitions_matrix.rs:570`
///   钉的形状（"once the registry is clean the lease may go back to the idle pool"）——
///   §6.5 的次序是「先在原 resource 上回滚/关闭句柄并从 actor 注销，**确认后才释放资源**」，
///   关掉并注销就是那一项归池前置**已满足**。
/// * 关不掉 ⇒ (1) 的 `?` 提前返回，归还整体失败，登记保留、租约保持 `InUse`、可重试。
///   这是既有 `a_handle_that_cannot_be_closed_blocks_the_return_and_stays_retryable`
///   钉的形状。
///
/// 于是本用例钉的是**第三种**形状，也就是既有覆盖真的没碰的那一个：
/// **closer 关成功了、登记也摘了，但调用方在 `host` 里谎报「没有任何句柄」**。
/// 此时归池仍然必须成立（登记册干净了），并且**关句柄那一刻物理层必须是干净的** ——
/// 否则「先关句柄再释放资源」这条次序就只是碰巧。
///
/// 顺序用 `physical_progress()` 断言而不是计数：计次查不出「关句柄时物理层动没动」。
#[test]
fn a_clean_driver_verdict_parks_after_the_registry_is_actually_clean() {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let budget = Arc::new(
        InProcessBudgetCoordinator::new(BudgetConfig::team_default(), OrganizationId::new(ORG))
            .with_clock(clock.clone())
            .with_principals(Arc::new(OrganizationPrincipal)),
    );
    let resources = ResourceManager::new(transport.clone(), clock.clone());
    let mut manager = LeaseManager::new(resources, budget);
    let connection_id = connection("conn-cm74-a3-prod");

    let record = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("acquire must open a physical resource");
    manager
        .register_handle(
            &record.lease_id,
            SessionHandleRef::new(
                HandleId::new("hdl-prod-1"),
                HandleKind::Transaction,
                record.resource_id.clone(),
                Counter::new(1),
            ),
        )
        .expect("registering a session handle on an issued lease is legal");
    assert_eq!(
        manager.handles().len(),
        1,
        "the premise is a registered handle"
    );

    // 关句柄那一刻物理层还没动过 —— 这是断言 1 与断言 3 共享的顺序证据。
    let progress = Arc::clone(&transport);
    let mut closed_order: Vec<(String, usize, usize)> = Vec::new();
    let released = manager
        .release(
            &record.lease_id,
            // 调用方**谎报**：声称四项全清、没有任何句柄。
            &HostConditionSnapshot::default(),
            &DriverCleanVerdict::reported_clean(),
            &mut |handle: &SessionHandleRef| {
                let (resets, closes) = progress.physical_progress();
                closed_order.push((handle.handle_id.as_str().to_owned(), resets, closes));
                Ok(())
            },
        )
        .expect("a closable handle must not block the return");

    assert_eq!(
        closed_order.len(),
        1,
        "the handle must really be closed, otherwise the parking below proves nothing"
    );
    for (handle_id, resets, closes) in &closed_order {
        assert_eq!(
            (*resets, *closes),
            (0, 0),
            "handle {handle_id} was closed after a physical reset/close had already run: \
             §6.5 requires close-then-deregister-then-release"
        );
    }

    // 关完了 ⇒ 摘登记。归还后登记册必须空（对称性）。
    assert_eq!(released.handles.closed_handles, 1);
    assert_eq!(released.handles.deregistered.len(), 1);
    assert_eq!(released.handles.deregistered[0].as_str(), "hdl-prod-1");
    assert!(released.handles.residual_dropped.is_empty());
    assert_eq!(
        manager.handles().len(),
        0,
        "registration and release are two halves"
    );

    // 登记册已清空 ⇒ 那一项归池前置已满足 ⇒ 允许归池。
    // driver 报 Clean、host 四项全清、句柄已关并摘净：四个条件都过了。
    assert!(
        released.cleanup.returned_to_pool(),
        "once the registry is actually clean the lease must park: §6.5 orders \
         close-deregister-then-release, and a fully settled lease that still cannot park \
         would leak a physical connection on every short operation"
    );
    assert!(released.pool.park);
    assert_eq!(
        released.cleanup.disposition,
        CleanupDisposition::ReturnedToPool
    );
    assert_eq!(
        released.cleanup.outstanding_handles,
        Vec::new(),
        "a handle that was closed and deregistered is no longer an obligation"
    );
    assert_eq!(manager.resources().idle_lease_count(), 1);

    // 归池不核销预算：空闲连接仍然占着物理槽位（§9.3）。
    assert!(!released.budget_settled);
    assert_eq!(manager.permits_outstanding(), 1);
    // **归池前必须真的跑一次复位**（§9.4「reset 不支持、失败或超时直接关闭」预设宿主
    // 知道复位结果；§7.3 第 6 步「结果完全消费后 reset；只有 Clean 才归池，其余关闭」）。
    // 复位落在**被归池的那条原资源**上，顺序 Open → Reset，且不混入任何关闭。
    assert!(
        released.cleanup.session_reset_performed,
        "the pre-pool reset must run"
    );
    assert_eq!(
        transport.events(),
        vec![
            PhysicalEvent::Open(record.resource_id.clone()),
            PhysicalEvent::Reset(record.resource_id.clone()),
        ],
        "the pre-pool reset runs on the resource being parked, and no close may follow"
    );

    // 非空跑：复用必须是**真的**复用（同一个租约、同一个物理资源、没新开连接）。
    let reused = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the idle pool serves the next request");
    assert_eq!(reused.lease_id, record.lease_id);
    let opened = transport
        .events()
        .iter()
        .filter(|event| matches!(event, PhysicalEvent::Open(_)))
        .count();
    assert_eq!(opened, 1, "reuse must not open a second resource");
}

/// **判负对照**：登记册里还剩着关不掉的句柄时，调用方同样声称四项全清，
/// 归池仍然必须被拒 —— 「以调用方为准」会让这一笔变成 `ReturnedToPool`。
///
/// 与上一条的惟一差别是 closer 能不能关掉那条句柄。两条一起才证明
/// 「并集读的是登记册的**当前**状态」，而不是「有没有句柄」这个静态事实。
#[test]
fn a_handle_left_in_the_registry_never_parks_even_when_the_host_claims_clear() {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let budget = Arc::new(
        InProcessBudgetCoordinator::new(BudgetConfig::team_default(), OrganizationId::new(ORG))
            .with_clock(clock.clone())
            .with_principals(Arc::new(OrganizationPrincipal)),
    );
    let resources = ResourceManager::new(transport.clone(), clock.clone());
    let mut manager = LeaseManager::new(resources, budget);
    let connection_id = connection("conn-cm74-a3-stuck-but-clear");

    let record = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("acquire must open a physical resource");
    manager
        .register_handle(
            &record.lease_id,
            SessionHandleRef::new(
                HandleId::new("hdl-stuck-2"),
                HandleKind::Transaction,
                record.resource_id.clone(),
                Counter::new(1),
            ),
        )
        .expect("registering a session handle on an issued lease is legal");

    // closer 什么都不做：登记册里那条**仍在**（没有 deregister），
    // 于是 (2) 的并集非空 —— 归池前置的「§6.5 已登记句柄为空」不成立。
    let released = manager
        .release(
            &record.lease_id,
            &HostConditionSnapshot::default(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        )
        .expect_err("a handle still in the registry must block the return");

    assert_eq!(released.reason(), "resourceTransportRefused");
    assert_eq!(
        manager
            .resources()
            .lease(&record.lease_id)
            .map(|lease| lease.state),
        Some(LeaseState::InUse),
        "the lease keeps its state so the caller can retry"
    );
    assert_eq!(
        manager.handles().len(),
        1,
        "the registration survives the refusal"
    );
    assert_eq!(manager.resources().idle_lease_count(), 0);
    assert_eq!(
        transport.events(),
        vec![PhysicalEvent::Open(record.resource_id.clone())],
        "nothing physical may happen when the handle could not be closed"
    );
}

/// 同一条生产路径上的**反例**：登记册是空的（从来没有过句柄）。
///
/// 没有它，上面那条「拒绝归池」就分不清是判据在判，还是这条路径从来不放行。
/// 这正是「判负必须有同构造的正例对照」这条纪律。
#[test]
fn a_lease_without_any_handle_still_parks_on_the_production_release_path() {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let budget = Arc::new(
        InProcessBudgetCoordinator::new(BudgetConfig::team_default(), OrganizationId::new(ORG))
            .with_clock(clock.clone())
            .with_principals(Arc::new(OrganizationPrincipal)),
    );
    let resources = ResourceManager::new(transport.clone(), clock.clone());
    let mut manager = LeaseManager::new(resources, budget);
    let connection_id = connection("conn-cm74-a3-clean");

    let record = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("acquire must open a physical resource");

    let released = manager
        .release(
            &record.lease_id,
            &HostConditionSnapshot::default(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        )
        .expect("a clean lease with no handles parks");

    assert!(
        released.cleanup.returned_to_pool(),
        "the control case must park, otherwise the refusal above is vacuous"
    );
    assert!(released.pool.park);
    assert_eq!(released.handles.closed_handles, 0);
    assert_eq!(manager.resources().idle_lease_count(), 1);
}

// ===========================================================================
// 3. 关不掉 ⇒ 归还被拒（不是静默放行）
// ===========================================================================

/// 「关不掉句柄」与「句柄非空」是两件事，但都指向同一条宿主检查失败。
///
/// `lease_manager` 第 (1) 步的 `closer.close(&handle)?` 把 `?` 放在摘登记**之前**，
/// 所以关不掉 ⇒ 整个 release 中断，租约保持 `InUse`、登记保留 —— 可重试。
/// 这个形状是已知的（简报点名「不是你的 bug」），本用例只是把它钉住，
/// 以免有人顺手把 `?` 挪到 `deregister` 之后，「关不掉也照样摘」于是悄悄成立。
#[test]
fn a_handle_that_cannot_be_closed_never_parks_and_stays_retryable() {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let budget = Arc::new(
        InProcessBudgetCoordinator::new(BudgetConfig::team_default(), OrganizationId::new(ORG))
            .with_clock(clock.clone())
            .with_principals(Arc::new(OrganizationPrincipal)),
    );
    let resources = ResourceManager::new(transport.clone(), clock.clone());
    let mut manager = LeaseManager::new(resources, budget);
    let connection_id = connection("conn-cm74-a3-stuck");

    let record = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("acquire must open a physical resource");
    manager
        .register_handle(
            &record.lease_id,
            SessionHandleRef::new(
                HandleId::new("hdl-stuck-1"),
                HandleKind::Transaction,
                record.resource_id.clone(),
                Counter::new(1),
            ),
        )
        .expect("registering a handle is legal on an issued lease");

    // `NoHandleCloser` 什么都关不掉：归还必须失败，且不能留下任何归池痕迹。
    let error = manager
        .release(
            &record.lease_id,
            &HostConditionSnapshot::default(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        )
        .expect_err("a handle that cannot be closed must block the return");

    assert_eq!(error.reason(), "resourceTransportRefused");
    assert_eq!(
        manager
            .resources()
            .lease(&record.lease_id)
            .map(|lease| lease.state),
        Some(LeaseState::InUse),
        "the lease keeps its state so the caller can retry"
    );
    assert_eq!(
        manager.handles().len(),
        1,
        "the registration survives the refusal — that is what makes a retry possible"
    );
    assert_eq!(manager.resources().idle_lease_count(), 0);
    assert_eq!(
        transport.events(),
        vec![PhysicalEvent::Open(record.resource_id.clone())],
        "nothing physical may happen when the handle could not be closed"
    );
}
