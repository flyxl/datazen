//! 归还裁决的公开面集成测试（三组宿主条件、复位次序、空闲复用）。
//!
//! 替身全部手写：`datazen_runtime::testing` 下的 fixture 由 `cfg(test)` 门控，
//! 而 `tests/` 编译 lib 时并不带 `cfg(test)`，因此这里按本模块自己声明的公开接缝
//! ——[`MonotonicSource`] 与 [`PhysicalTransport`]——自建替身。
//!
//! 时间只经 [`TestClock::advance`] 手动推进，**不出现 `sleep`**：每一步断言都是确定性的。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use datazen_runtime::connection::port::CancelDisposition;
use datazen_runtime::connection::types::ClientInstanceId;
use datazen_runtime::connection::{
    ConfigRevision, ConnectionId, Counter, ExecutionId, HandleId, HandleKind, NamespaceTarget,
    OwnerRef, PoolKeyInputs, ResourceId, SessionHandleRef,
};
use datazen_runtime::resource::{
    DriverCleanVerdict, HostConditionSnapshot, LeasePurpose, LeaseRequest, LeaseState,
    MonotonicSource, PhysicalTransport, ProtocolDebt, ResourceError, ResourceManager,
};

// ---- 时钟替身 ----

/// 手动推进的单调时钟：`advance` 之前的时间对台账而言根本没有发生过。
struct TestClock {
    nanos: AtomicU64,
}

impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            nanos: AtomicU64::new(0),
        })
    }

    fn advance(&self, by: Duration) {
        self.nanos.fetch_add(by.as_nanos() as u64, Ordering::SeqCst);
    }
}

impl MonotonicSource for TestClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }
}

// ---- 物理端口替身 ----

/// 物理动作的**有序**记录：复位与关闭的先后关系只能靠顺序断言，计次是查不出来的。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PhysicalEvent {
    Open(ResourceId),
    Reset(ResourceId),
    Close(ResourceId),
}

#[derive(Debug, Default)]
struct Journal {
    events: Vec<PhysicalEvent>,
    reset_fails: bool,
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

    fn fail_reset(&self) {
        self.lock().reset_fails = true;
    }

    fn events(&self) -> Vec<PhysicalEvent> {
        self.lock().events.clone()
    }

    fn open_count(&self) -> usize {
        self.lock()
            .events
            .iter()
            .filter(|event| matches!(event, PhysicalEvent::Open(_)))
            .count()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Journal> {
        // 断言失败一次不该把后续断言全变成 panic 噪声，所以不传播 poison。
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

    fn reset(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        let mut journal = self.lock();
        journal
            .events
            .push(PhysicalEvent::Reset(resource_id.clone()));
        if journal.reset_fails {
            return Err(ResourceError::TransportRefused("session reset rejected"));
        }
        Ok(())
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
        client_instance_id: ClientInstanceId::new("client-it-1"),
    }
}

/// 短操作租约：用途允许归还池，所以双条件裁决对它真实成立。
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
            execution_identity_key: "owner-hash-1".to_owned(),
            policy_isolation_key: "policy-a".to_owned(),
        },
        owner(),
        LeasePurpose::ShortOperation,
    )
}

fn manager() -> (
    ResourceManager,
    Arc<ScriptedTransport>,
    Arc<TestClock>,
    ConnectionId,
) {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let connection_id = connection("conn-pool");
    let manager = ResourceManager::new(transport.clone(), clock.clone());
    (manager, transport, clock, connection_id)
}

fn first_blocker_code(report: &datazen_runtime::resource::CleanupReport) -> &'static str {
    report
        .blockers
        .first()
        .map(|reason| reason.reason_code())
        .expect("a refused lease always names its first blocker")
}

// ---- 驱动结论与宿主条件是两道**互相独立**的门 ----

#[test]
fn a_clean_driver_report_with_clear_host_conditions_returns_the_lease_to_the_idle_pool() {
    let (mut manager, transport, _clock, connection_id) = manager();
    let leased = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first lease opens a fresh physical resource");

    let report = manager
        .release(
            &leased.lease_id,
            HostConditionSnapshot::default(),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("a clean driver report and a clear host snapshot admit the lease");

    assert!(
        report.returned_to_pool(),
        "both conditions passed, so it must return"
    );
    assert_eq!(
        report.blockers.len(),
        0,
        "a returned lease has no blocker at all, not even an empty one"
    );
    // 归还池**不**释放物理预算：资源还在池里等着被再签发。
    assert!(!report.disposition.releases_physical_budget());
    assert_eq!(manager.idle_lease_count(), 1);

    // 非空跑：复用必须是**真的**复用（同一个租约、同一个物理资源、没新开连接）。
    let reused = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the idle pool serves the next request");
    assert_eq!(reused.lease_id, leased.lease_id);
    assert_eq!(
        transport.open_count(),
        1,
        "reuse must not open a second resource"
    );
}

#[test]
fn an_active_execution_refuses_reuse_and_the_next_lease_opens_a_new_resource() {
    let (mut manager, transport, _clock, connection_id) = manager();
    let leased = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first lease opens a fresh physical resource");
    let running = ExecutionId::new("exec-running");

    let report = manager
        .release(
            &leased.lease_id,
            HostConditionSnapshot::default().with_active_execution(running.clone()),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("the refusal is reported, not an error of the ledger");

    assert!(
        !report.returned_to_pool(),
        "an active execution blocks return"
    );
    assert_eq!(first_blocker_code(&report), "hostActiveExecution");
    assert_eq!(
        manager.idle_lease_count(),
        0,
        "a refused lease never enters the idle pool"
    );

    // 非空跑：资源被关闭而不是留在池里，下一次签发必须另开物理资源。
    assert_eq!(
        transport.events(),
        vec![
            PhysicalEvent::Open(ResourceId::new("res-1")),
            PhysicalEvent::Close(ResourceId::new("res-1")),
        ]
    );
    let fresh = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("a refused lease does not poison the next request");
    assert_ne!(fresh.lease_id, leased.lease_id);
    assert_eq!(transport.open_count(), 2);
}

#[test]
fn an_outstanding_protocol_refuses_reuse_even_though_the_driver_reports_clean() {
    let (mut manager, transport, _clock, connection_id) = manager();
    let leased = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first lease opens a fresh physical resource");

    let report = manager
        .release(
            &leased.lease_id,
            HostConditionSnapshot::default().with_outstanding_protocol(ProtocolDebt::new(
                "protocol-prepare",
                "the server side transaction is still open",
            )),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("the refusal is reported, not an error of the ledger");

    assert!(!report.returned_to_pool());
    assert_eq!(first_blocker_code(&report), "hostOutstandingProtocol");
    assert_eq!(manager.idle_lease_count(), 0);
    assert_eq!(
        transport.events().last(),
        Some(&PhysicalEvent::Close(ResourceId::new("res-1"))),
        "a lease with unfinished protocol is closed, never reissued"
    );
}

#[test]
fn an_unreleased_session_handle_refuses_reuse_and_is_reset_before_the_close() {
    let (mut manager, transport, _clock, connection_id) = manager();
    let leased = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first lease opens a fresh physical resource");
    let handle = SessionHandleRef::new(
        HandleId::new("handle-cursor-1"),
        HandleKind::Cursor,
        leased.resource_id.clone(),
        Counter::new(1),
    );

    let report = manager
        .release(
            &leased.lease_id,
            HostConditionSnapshot::default().with_unreleased_handle(handle.clone()),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("the refusal is reported, not an error of the ledger");

    assert!(!report.returned_to_pool());
    assert_eq!(first_blocker_code(&report), "hostUnreleasedSessionHandle");
    assert!(report.session_reset_performed);
    assert!(
        report.outstanding_handles.is_empty(),
        "a performed reset leaves no outstanding handle behind"
    );
    // 复位裁决的实质是**次序**：复位必须在关闭之前，否则旧事务会被一起丢掉。
    assert_eq!(
        transport.events(),
        vec![
            PhysicalEvent::Open(ResourceId::new("res-1")),
            PhysicalEvent::Reset(ResourceId::new("res-1")),
            PhysicalEvent::Close(ResourceId::new("res-1")),
        ],
        "the reset must be performed on the original resource before it is closed"
    );
}

#[test]
fn a_reset_that_fails_quarantines_the_resource_instead_of_closing_it() {
    let (mut manager, transport, _clock, connection_id) = manager();
    let leased = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first lease opens a fresh physical resource");
    transport.fail_reset();
    let handle = SessionHandleRef::new(
        HandleId::new("handle-cursor-2"),
        HandleKind::Cursor,
        leased.resource_id.clone(),
        Counter::new(1),
    );

    let report = manager
        .release(
            &leased.lease_id,
            HostConditionSnapshot::default().with_unreleased_handle(handle),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("quarantine is a disposition, not a ledger failure");

    assert!(
        report.quarantined(),
        "an unconfirmed reset must not close the resource"
    );
    assert!(!report.session_reset_performed);
    assert_eq!(
        transport.events().last(),
        Some(&PhysicalEvent::Reset(ResourceId::new("res-1"))),
        "no close may follow a failed reset"
    );
    // 隔离保留物理预算：资源还占着槽位，绝不静默丢弃。
    assert_eq!(manager.occupied_slots(), 1);
    assert_eq!(
        manager
            .lease(&leased.lease_id)
            .expect("a quarantined lease stays on the ledger")
            .state,
        LeaseState::Quarantined
    );
}

#[test]
fn an_unclean_driver_report_never_returns_to_the_idle_pool_even_when_the_host_is_clear() {
    let (mut manager, transport, _clock, connection_id) = manager();
    let leased = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first lease opens a fresh physical resource");

    let report = manager
        .release(
            &leased.lease_id,
            HostConditionSnapshot::default(),
            DriverCleanVerdict::reported_unclean(),
        )
        .expect("the refusal is reported, not an error of the ledger");

    assert!(!report.returned_to_pool());
    assert_eq!(first_blocker_code(&report), "driverUnclean");
    assert_eq!(manager.idle_lease_count(), 0);
    assert!(report.disposition.releases_physical_budget());
    assert_eq!(
        transport.events().last(),
        Some(&PhysicalEvent::Close(ResourceId::new("res-1"))),
        "an unclean lease is closed on the spot"
    );
}

#[test]
fn an_idle_lease_is_closed_once_the_idle_ttl_expires() {
    let (mut manager, transport, clock, connection_id) = manager();
    let leased = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first lease opens a fresh physical resource");
    manager
        .release(
            &leased.lease_id,
            HostConditionSnapshot::default(),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("the clean lease returns to the idle pool");
    assert_eq!(manager.idle_lease_count(), 1);

    // 空闲 TTL 是 60s；推进 61s 后同键的空闲租约必须先被淘汰再新建。
    clock.advance(Duration::from_secs(61));
    let fresh = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("an expired idle lease is replaced by a fresh resource");

    assert_ne!(fresh.lease_id, leased.lease_id);
    assert_eq!(transport.open_count(), 2);
    assert!(transport
        .events()
        .contains(&PhysicalEvent::Close(ResourceId::new("res-1"))));
}
