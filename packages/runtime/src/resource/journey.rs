//! 归池双条件与会话句柄先复位后关闭的连续旅程。
//!
//! 每个用例都跑完整条链路：签发 → 使用 → 归还裁决 → 再签发。
//! 断言不只看状态字段，还看**物理端口的动作顺序**与**是否真的又开了一条连接**——
//! 「拒绝复用」在行为上的定义就是：下一次签发必须重新 `Open`，而不是偷偷捡回旧的。
//!
//! 时间只由 [`TestClock`] 推进，**全文件无 `sleep`**。

use std::sync::Arc;
use std::time::Duration;

use crate::connection::{
    ConnectionId, Counter, ExecutionId, HandleId, HandleKind, LeaseId, ResourceId, SessionHandleRef,
};
use crate::resource::cleanup::{
    CleanupDisposition, DriverCleanVerdict, HostConditionSnapshot, ProtocolDebt, ReturnReason,
};
use crate::resource::harness::{
    pool_key_inputs, workflow_owner, Fault, RecordingTransport, TestClock, TransportEvent,
};
use crate::resource::lease::{LeasePurpose, LeaseRecord, LeaseRequest, LeaseState};
use crate::resource::{MonotonicSource, ResourceManager, EMPTY_POOL_METADATA_LRU_LIMIT};

struct Fixture {
    manager: ResourceManager,
    transport: RecordingTransport,
    clock: Arc<TestClock>,
    connection: ConnectionId,
}

impl Fixture {
    fn new() -> Self {
        let transport = RecordingTransport::new();
        let clock = Arc::new(TestClock::new());
        let manager = ResourceManager::new(
            transport.shared(),
            Arc::clone(&clock) as Arc<dyn MonotonicSource>,
        );
        Self {
            manager,
            transport,
            clock,
            connection: ConnectionId::new("conn-1"),
        }
    }

    fn request_for(&self, database: &str, purpose: LeasePurpose) -> LeaseRequest {
        LeaseRequest::new(
            pool_key_inputs(&self.connection, database, "tenant-a"),
            workflow_owner(),
            purpose,
        )
    }

    fn request(&self, purpose: LeasePurpose) -> LeaseRequest {
        self.request_for("analytics", purpose)
    }

    fn acquire(&mut self, purpose: LeasePurpose) -> LeaseRecord {
        let request = self.request(purpose);
        expect_ok(self.manager.acquire(&request), "acquire")
    }

    /// 归还：驱动结论与宿主条件**分别**传进来，本模块只裁决宿主自己的那一半。
    fn release(
        &mut self,
        lease_id: &LeaseId,
        host: HostConditionSnapshot,
        driver: DriverCleanVerdict,
    ) -> crate::resource::CleanupReport {
        expect_ok(self.manager.release(lease_id, host, driver), "release")
    }

    fn state_of(&self, lease_id: &LeaseId) -> Option<LeaseState> {
        self.manager.lease(lease_id).map(|record| record.state)
    }

    fn opened(&self) -> Vec<ResourceId> {
        self.transport.snapshot().opened
    }
}

fn expect_ok<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{what} should have succeeded but failed: {error}"),
    }
}

fn expect_err<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> E {
    match result {
        Ok(_) => panic!("{what} should have been refused but succeeded"),
        Err(error) => error,
    }
}

fn first_reason_code(report: &crate::resource::CleanupReport) -> &'static str {
    report
        .blockers
        .first()
        .map(|reason| reason.reason_code())
        .unwrap_or("none")
}

// ---------------------------------------------------------------------------
// 阳性对照：双条件都通过才回池
// ---------------------------------------------------------------------------

#[test]
fn only_a_clean_driver_report_and_clear_host_conditions_return_the_lease() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default(),
        DriverCleanVerdict::reported_clean(),
    );

    assert_eq!(report.disposition, CleanupDisposition::ReturnedToPool);
    assert!(report.returned_to_pool());
    assert!(report.blockers.is_empty(), "no blocker may be reported");
    assert!(
        !report.physical_budget_released,
        "a pooled lease keeps its budget"
    );
    // **归池前必须真的跑一次复位。** 依据三条原文：
    // * `connection-management.md` §9.4（:657）「reset 不支持、失败或超时直接关闭」——
    //   这句预设宿主知道复位结果，而宿主唯一知道的方式是自己去跑它；
    // * §7.3 第 6 步（:551）「结果完全消费后 reset；只有 Clean 才归池，其余关闭」；
    // * `cleanup.rs` 里 `PhysicalTransport::reset` 的文档「复位。短租约归池前必跑」。
    //
    // 这里原先是 `assert!(!report.session_reset_performed)`：归池路径上宿主**从没**碰过
    // 复位端口，只是转述一个自称含复位的私有 bool。那条断言钉的是缺陷本身而不是期望
    // 行为，所以它随本缺陷的修复一起翻真 —— 不是为了让测试变绿而放宽。
    assert!(
        report.session_reset_performed,
        "归池必须报告复位已执行（§9.4 / §7.3 第 6 步）"
    );
    assert_eq!(fixture.manager.idle_lease_count(), 1);
    assert_eq!(fixture.state_of(&lease_id), Some(LeaseState::Acquired));
    assert!(
        fixture.transport.snapshot().closed.is_empty(),
        "returning to the pool must not close the physical resource"
    );
    // 复位落在**被归池的那条原资源**上，且顺序是 Open → Reset —— 归池不新开连接，
    // 也没有任何关闭混进来。
    assert_eq!(
        fixture.transport.snapshot().events,
        vec![
            TransportEvent::Open(resource_id.clone()),
            TransportEvent::Reset(resource_id),
        ],
        "the pre-pool reset runs on the resource being parked"
    );

    // 真的复用了：同键再签发拿到同一条租约，且没有新的 Open。
    let reused = fixture.acquire(LeasePurpose::ShortOperation);
    assert_eq!(reused.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 1);
    assert_eq!(fixture.manager.occupied_slots(), 1);
}

// ---------------------------------------------------------------------------
// 三个宿主条件各自独立地否决复用
// ---------------------------------------------------------------------------

#[test]
fn a_clean_driver_report_does_not_return_a_lease_that_still_has_an_active_execution() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let execution = ExecutionId::new("exec-1");
    expect_ok(
        fixture
            .manager
            .begin_execution(&lease_id, execution.clone()),
        "begin_execution",
    );

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default().with_active_execution(execution),
        DriverCleanVerdict::reported_clean(),
    );

    assert!(!report.returned_to_pool());
    assert_eq!(report.disposition, CleanupDisposition::Quarantined);
    assert_eq!(first_reason_code(&report), "hostActiveExecution");
    assert!(!report.physical_budget_released);
    assert_eq!(fixture.manager.idle_lease_count(), 0);
    assert_eq!(fixture.state_of(&lease_id), Some(LeaseState::Quarantined));
    assert!(
        fixture.transport.snapshot().closed.is_empty(),
        "a running execution must not be closed under it"
    );

    // 隔离的租约不会被再次签发出去。
    let next = fixture.acquire(LeasePurpose::ShortOperation);
    assert_ne!(next.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 2);
}

#[test]
fn an_outstanding_protocol_refuses_reuse_even_though_the_driver_reports_clean() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default()
            .with_outstanding_protocol(ProtocolDebt::new("batch-9", "two-phase commit pending")),
        DriverCleanVerdict::reported_clean(),
    );

    assert!(!report.returned_to_pool());
    assert_eq!(report.disposition, CleanupDisposition::Closed);
    assert_eq!(first_reason_code(&report), "hostOutstandingProtocol");
    assert!(report.physical_budget_released);
    assert_eq!(fixture.manager.idle_lease_count(), 0);
    assert_eq!(
        fixture.transport.snapshot().closed,
        vec![resource_id.clone()]
    );

    let next = fixture.acquire(LeasePurpose::ShortOperation);
    assert_ne!(next.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 2);
}

#[test]
fn an_unreleased_session_handle_refuses_reuse_and_is_rolled_back_before_close() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();
    let handle = SessionHandleRef::new(
        HandleId::new("handle-1"),
        HandleKind::Transaction,
        resource_id.clone(),
        Counter::new(7),
    );

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default().with_unreleased_handle(handle),
        DriverCleanVerdict::reported_clean(),
    );

    assert!(!report.returned_to_pool());
    assert_eq!(report.disposition, CleanupDisposition::Closed);
    assert_eq!(first_reason_code(&report), "hostUnreleasedSessionHandle");
    assert!(report.session_reset_performed, "reset must happen");
    assert!(
        report.outstanding_handles.is_empty(),
        "after the reset no handle may still be committable"
    );
    assert_eq!(fixture.manager.idle_lease_count(), 0);

    // 动作顺序：Open → Reset → Close，且三者都在**同一个**物理资源上。
    let events = fixture.transport.snapshot().events;
    assert_eq!(
        events,
        vec![
            TransportEvent::Open(resource_id.clone()),
            TransportEvent::Reset(resource_id.clone()),
            TransportEvent::Close(resource_id),
        ]
    );

    let next = fixture.acquire(LeasePurpose::ShortOperation);
    assert_ne!(next.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 2);
}

#[test]
fn a_reset_that_does_not_succeed_quarantines_the_lease_instead_of_closing_it() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();
    let handle = SessionHandleRef::new(
        HandleId::new("handle-1"),
        HandleKind::Transaction,
        resource_id.clone(),
        Counter::new(7),
    );
    fixture.transport.inject(Fault::Reset);

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default().with_unreleased_handle(handle),
        DriverCleanVerdict::reported_clean(),
    );

    assert_eq!(report.disposition, CleanupDisposition::Quarantined);
    assert!(!report.session_reset_performed);
    assert_eq!(report.outstanding_handles, vec![HandleId::new("handle-1")]);
    assert!(!report.physical_budget_released);
    assert_eq!(fixture.state_of(&lease_id), Some(LeaseState::Quarantined));
    assert!(
        fixture.transport.snapshot().closed.is_empty(),
        "an unknown rollback outcome must not be closed away"
    );
    expect_err(
        fixture.manager.accepts_execution(&lease_id),
        "executing on a quarantined lease",
    );
    assert_eq!(fixture.opened().len(), 1);
}

// ---------------------------------------------------------------------------
// 归池前的复位：双条件都过了，宿主仍然必须自己跑一次复位
// ---------------------------------------------------------------------------

/// **归池时复位失败 ⇒ 不许归池**（§9.4 :657「reset 不支持、失败或超时直接关闭」）。
///
/// 这条用例是缺陷的反面：修复之前归池分支**从不**调用 `transport.reset`，所以
/// 复位失败在归池路径上根本不可能被观测到 —— 一个没跑过的复位自然也没有失败分支。
/// 现在跑它，失败必须把结论作废。
///
/// 落点是**关闭**而不是隔离，这里逐条钉住为什么：
/// * §9.4 给的后果就是「直接关闭」，不是「隔离」；
/// * 隔离的事由（回滚结果待核验 / 活动执行 / 归属被禁用）在这里一个都不成立：
///   归池裁决成立的前提是宿主四项全清；
/// * 关闭这一步被确认 ⇒ 预算当场核销（§9.3「只有实际 close 才释放」）；隧道引用的
///   结算读的是**最终**处置（`Closed`），不是 `CleanupPlan::of` 的初值（`ReturnedToPool`），
///   所以不会出现「预算没核销而引用先还了」的错拍。配对证据见
///   `packages/runtime/tests/tunnel_budget_pairing.rs` 的同名用例。
#[test]
fn a_reset_that_fails_during_the_idle_return_closes_the_lease_instead_of_parking_it() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();
    // 宿主四项全清 + 驱动 Clean ⇒ 裁决本意是归池；复位是唯一失败的那一步。
    fixture.transport.inject(Fault::Reset);

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default(),
        DriverCleanVerdict::reported_clean(),
    );

    // 1. 归池被否决。
    assert!(
        !report.returned_to_pool(),
        "复位未被确认 ⇒ §9.4 拒绝这次归池，绝不把未知状态的资源留在池里"
    );
    assert!(
        !report.quarantined(),
        "文档给的后果是关闭，不是隔离：此处没有待核验的回滚义务"
    );
    assert_eq!(report.disposition, CleanupDisposition::Closed);

    // 2. 双条件结论作废，并且点名「归池前的复位」这个新事由。
    assert!(!report.decision.admits_to_idle_pool());
    assert_eq!(first_reason_code(&report), "hostResetUnconfirmed");
    assert!(!report.blockers.is_empty());

    // 3. 报告不许谎称复位已完成。
    assert!(
        !report.session_reset_performed,
        "复位失败时 session_reset_performed 必须为假 —— 这是审计的行为证据"
    );

    // 4. 预算：**关闭**被确认 ⇒ 预算此刻核销。归池被否决不等于预算被释放，也不等于
    //    预算被扣住 —— 只有「关闭未被确认」才保留预算（下面 `Closed` 分支的既有语义）。
    assert!(
        report.physical_budget_released,
        "落到关闭、且关闭被确认 ⇒ §9.3 要求此刻核销预算"
    );
    assert_eq!(
        fixture.manager.occupied_slots(),
        0,
        "确认关闭之后物理槽位必须腾空，否则预算被核销了两次口径"
    );

    // 5. 空闲池与状态：归池被否决 ⇒ 一条空闲都没有，租约行随确认关闭被摘除。
    assert_eq!(fixture.manager.idle_lease_count(), 0);
    assert_eq!(
        fixture.state_of(&lease_id),
        None,
        "关闭被确认 ⇒ 行被 forget，与 `Closed` 处置一致"
    );
    expect_err(
        fixture.manager.accepts_execution(&lease_id),
        "a lease whose reset failed must never serve traffic again",
    );

    // 6. 账面证据。本替身的账记的是**已完成**的物理动作（失败的那一步不入账），
    //    所以「复位失败」的直接证据是上面那条 `hostResetUnconfirmed` 阻断项 ——
    //    一次都没跑过复位的代码造不出这个名字。账面上看得见的是：
    //    归池没有变成停放，而是变成了一次**已完成**的关闭，且没有任何复位被记成完成。
    assert!(
        fixture.transport.snapshot().reset.is_empty(),
        "复位没有被确认过 ⇒ 账上不得出现一次已完成的复位（与 session_reset_performed 同口径）"
    );
    assert_eq!(
        fixture.transport.snapshot().closed,
        vec![resource_id.clone()],
        "归池被否决的落点是一次关闭，发生在**同一条**原资源上"
    );
    assert_eq!(
        fixture.transport.snapshot().events,
        vec![
            TransportEvent::Open(resource_id.clone()),
            TransportEvent::Close(resource_id),
        ],
        "失败的归池复位必须落到一次关闭上，而不是留在池里"
    );

    // 7. 非空跑：下一次签发必须重新开一条物理连接，而不是捡回这条复位不明的。
    let next = fixture.acquire(LeasePurpose::ShortOperation);
    assert_ne!(next.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 2);
}

/// **`Discard` ⇒ 不进池**：驱动**明确**回答「无法证明已复位」时必须关闭，不得归池。
///
/// 这是本轨修掉的那条真缺陷的正面证据。缺陷态下 `PhysicalTransport::reset` 返回
/// `Result<(), _>`，`Ok(())` 把 `Clean` 与 `Discard` 压成同一格，于是归池分支的
/// `Ok(()) => session_reset_performed = true` 对两者一视同仁 —— 驱动诚实地说
/// 「我复位不了」，宿主看到的是成功，然后把资源塞回空闲池。
///
/// 与上一条 `a_reset_that_fails_during_the_idle_return_...` 并列但**不同格**：
/// 那里是 `Err`（握手从未确认 ⇒ 结果**未知**），这里是 `Ok(Discard)`
/// （驱动给了**确定**的答复）。两者都关闭，但阻断项不同名 —— 把「知道不能复用」
/// 与「不知道」记成同一件事，审计就再也分不出驱动到底答了什么。
///
/// **`Discard` 是正常的、预期内的结果**：首版 15/15 path 驱动都声明
/// `ResetForReuse::Unsupported`，postgres/mysql/sqlite 的 `reset_resource` 返回
/// `Discard`（理由：no *proven* initialization protocol）。所以这条用例断言的
/// 不是异常路径，而是**首版的常态路径**。
#[test]
fn a_discarded_reset_refuses_the_idle_park_and_closes_the_resource() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();
    // 宿主四项全清 + 驱动 Clean ⇒ 裁决本意是归池；复位判决是唯一否决它的那一步。
    fixture.transport.inject(Fault::ResetDiscard);

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default(),
        DriverCleanVerdict::reported_clean(),
    );

    // 1. **核心断言：不进池。** 驱动明说不能复用 ⇒ 绝不许出现在空闲池里。
    assert!(
        !report.returned_to_pool(),
        "驱动明确回答 Discard ⇒ §7.3「只有 Clean 才归池，其余关闭」：绝不归池"
    );
    assert!(
        !report.quarantined(),
        "驱动给了确定的答复，没有待核验的不确定性"
    );
    assert_eq!(report.disposition, CleanupDisposition::Closed);
    assert_eq!(
        fixture.manager.idle_lease_count(),
        0,
        "空闲池必须一条都没有 —— 池里不许出现驱动明说不能复用的资源"
    );

    // 2. 阻断项点名**驱动判决**，与 `hostResetUnconfirmed`（`Err` 那一格）不同名。
    assert!(!report.decision.admits_to_idle_pool());
    assert_eq!(first_reason_code(&report), "driverDiscardedReset");
    assert!(
        !report
            .blockers
            .iter()
            .any(|reason| reason.reason_code() == "hostResetUnconfirmed"),
        "Discard 是确定的答复，不是「没拿到答复」：两个阻断项不许互推"
    );

    // 3. `Discard` 是常态 ⇒ 不得被记成故障。
    assert!(
        !ReturnReason::DriverDiscardedReset.is_fault(),
        "首版全部驱动都走这一格，把它当故障会让真正的告警淹没在常态里"
    );

    // 4. `Discard` **不**谎报复位已完成：驱动没能证明回到基线，报告就不能替它说做到了。
    assert!(
        !report.session_reset_performed,
        "驱动没能证明复位 ⇒ 不得记成已完成"
    );

    // 5. 预算：落点是关闭、且关闭被确认 ⇒ 此刻核销（§9.3「只有实际 close 才释放」）。
    assert!(report.physical_budget_released);
    assert_eq!(fixture.manager.occupied_slots(), 0);
    assert_eq!(fixture.state_of(&lease_id), None, "确认关闭 ⇒ 行被摘除");

    // 6. 账面：复位**被请求过也真的入账了**（驱动答复过），紧随其后是一次关闭。
    //    这与 `Err` 那格形成对照 —— 那里失败的那一步不入账。
    assert_eq!(
        fixture.transport.snapshot().events,
        vec![
            TransportEvent::Open(resource_id.clone()),
            TransportEvent::Reset(resource_id.clone()),
            TransportEvent::Close(resource_id),
        ],
        "复位被请求、被答复 Discard，然后是关闭：判决不许把这条路径变成归池"
    );

    // 7. 非空跑：下一次签发必须重新开一条物理连接，绝不捡回驱动拒绝对外复用的那条。
    let next = fixture.acquire(LeasePurpose::ShortOperation);
    assert_ne!(next.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 2);
}

/// `Discard` 出现在**关闭分支**（还有未释放句柄）时：收尾照样进行，关闭不被否决。
///
/// 依据在 `ResourceManager::release` 的该分支上写全了，这里钉住行为：
/// 关闭本来就不需要复用性（§7.5 第 5 步「任意用户 SQL session 首版直接关闭物理资源；
/// **不复用未知会话状态**」），所以「驱动没能证明已复位」在这条路径上不构成额外阻断
/// —— 资源照样关掉、预算照样核销。
///
/// 但报告**必须**留着那批句柄：驱动没能证明回滚真的发生过，审计里就不能写成
/// 「已回滚」。
#[test]
fn a_discarded_reset_during_a_close_does_not_block_the_close_but_keeps_the_handles_visible() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();
    let handle = SessionHandleRef::new(
        HandleId::new("handle-discard-1"),
        HandleKind::Transaction,
        resource_id.clone(),
        Counter::new(3),
    );
    fixture.transport.inject(Fault::ResetDiscard);

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default().with_unreleased_handle(handle.clone()),
        DriverCleanVerdict::reported_clean(),
    );

    // 1. 落点仍然是关闭：`Discard` 不把关闭升级成隔离，也不阻止关闭。
    assert_eq!(
        report.disposition,
        CleanupDisposition::Closed,
        "关闭不需要复用性 ⇒ 驱动没能证明复位不构成额外阻断（§9.4 / §7.5 第 5 步）"
    );
    assert!(!report.quarantined());
    // 阻断项的**首个**事由仍然是宿主那一项（未释放句柄），`Discard` 不改判它。
    assert_eq!(first_reason_code(&report), "hostUnreleasedSessionHandle");

    // 2. 关闭被确认 ⇒ 预算核销。
    assert!(report.physical_budget_released);
    assert!(report.disposition.releases_physical_budget());
    assert_eq!(fixture.manager.occupied_slots(), 0);

    // 3. **关键**：驱动没能证明复位 ⇒ 不许谎称「回滚过了」，句柄必须留在报告里。
    assert!(
        !report.session_reset_performed,
        "Discard 没能证明复位 ⇒ 不得记成已完成"
    );
    assert_eq!(
        report.outstanding_handles,
        vec![handle.handle_id.clone()],
        "驱动没证明回滚发生过，审计里就不能把句柄写成已解除"
    );

    // 4. 次序不变：复位（被答复 Discard）仍然排在关闭之前。
    assert_eq!(
        fixture.transport.snapshot().events,
        vec![
            TransportEvent::Open(resource_id.clone()),
            TransportEvent::Reset(resource_id.clone()),
            TransportEvent::Close(resource_id),
        ]
    );
}

#[test]
fn an_unclean_driver_report_refuses_reuse_even_though_every_host_condition_is_clear() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default(),
        DriverCleanVerdict::reported_unclean(),
    );

    assert!(!report.returned_to_pool());
    assert_eq!(report.disposition, CleanupDisposition::Closed);
    assert_eq!(report.blockers.len(), 1);
    assert_eq!(first_reason_code(&report), "driverUnclean");
    assert!(report.physical_budget_released);
    assert_eq!(fixture.manager.idle_lease_count(), 0);
    assert_eq!(fixture.transport.snapshot().closed, vec![resource_id]);

    let next = fixture.acquire(LeasePurpose::ShortOperation);
    assert_ne!(next.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 2);
}

#[test]
fn a_fixed_session_never_returns_to_the_idle_pool() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::FixedSession);
    let lease_id = lease.lease_id.clone();
    let resource_id = lease.resource_id.clone();

    let report = fixture.release(
        &lease_id,
        HostConditionSnapshot::default(),
        DriverCleanVerdict::reported_clean(),
    );

    assert_eq!(report.disposition, CleanupDisposition::Closed);
    assert_eq!(first_reason_code(&report), "hostFixedSessionNotPoolable");
    assert_eq!(fixture.manager.idle_lease_count(), 0);
    assert_eq!(fixture.transport.snapshot().closed, vec![resource_id]);
}

// ---------------------------------------------------------------------------
// 空闲池自身的行为（时间由 FakeClock 推进，不 sleep）
// ---------------------------------------------------------------------------

#[test]
fn an_idle_lease_is_closed_once_the_idle_ttl_expires() {
    let mut fixture = Fixture::new();
    let lease = fixture.acquire(LeasePurpose::ShortOperation);
    let lease_id = lease.lease_id.clone();
    fixture.release(
        &lease_id,
        HostConditionSnapshot::default(),
        DriverCleanVerdict::reported_clean(),
    );
    assert_eq!(fixture.manager.idle_lease_count(), 1);

    fixture.clock.advance(Duration::from_secs(61));
    let next = fixture.acquire(LeasePurpose::ShortOperation);

    assert_ne!(next.lease_id, lease_id, "an expired idle may not be issued");
    assert_eq!(fixture.opened().len(), 2);
    assert_eq!(fixture.transport.snapshot().closed.len(), 1);
    assert_eq!(fixture.manager.occupied_slots(), 1);
}

#[test]
fn empty_pool_metadata_stays_within_the_documented_lru_limit() {
    let mut fixture = Fixture::new();
    // 40 个不同 database ⇒ 40 个不同池键（分片维度），每个键的空闲桶都会变空一次。
    for index in 0..40 {
        let request = fixture.request_for(&format!("db-{index}"), LeasePurpose::ShortOperation);
        let lease = expect_ok(fixture.manager.acquire(&request), "acquire");
        let lease_id = lease.lease_id.clone();
        fixture.release(
            &lease_id,
            HostConditionSnapshot::default(),
            DriverCleanVerdict::reported_clean(),
        );
        let again = fixture.request_for(&format!("db-{index}"), LeasePurpose::ShortOperation);
        expect_ok(
            fixture.manager.acquire(&again),
            "re-acquire from the idle pool",
        );
    }

    assert_eq!(
        fixture.manager.empty_pool_metadata_len(),
        EMPTY_POOL_METADATA_LRU_LIMIT,
        "empty pool metadata must be bounded by the documented LRU limit"
    );
}
