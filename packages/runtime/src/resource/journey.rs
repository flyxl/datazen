//! CM-69（归池双条件）与 CM-73（会话句柄先复位后关闭）的连续旅程。
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
    CleanupDisposition, DriverCleanVerdict, HostConditionSnapshot, ProtocolDebt,
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
    assert!(!report.session_reset_performed);
    assert_eq!(fixture.manager.idle_lease_count(), 1);
    assert_eq!(fixture.state_of(&lease_id), Some(LeaseState::Acquired));
    assert!(
        fixture.transport.snapshot().closed.is_empty(),
        "returning to the pool must not close the physical resource"
    );

    // 真的复用了：同键再签发拿到同一条租约，且没有新的 Open。
    let reused = fixture.acquire(LeasePurpose::ShortOperation);
    assert_eq!(reused.lease_id, lease_id);
    assert_eq!(fixture.opened().len(), 1);
    assert_eq!(fixture.manager.occupied_slots(), 1);
}

// ---------------------------------------------------------------------------
// CM-69：三个宿主条件各自独立地否决复用
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
    assert!(report.session_reset_performed, "CM-73 reset must happen");
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
    // 40 个不同 database ⇒ 40 个不同池键（CM-67 的分片维度），每个键的空闲桶都会变空一次。
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
