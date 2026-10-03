//! 池键轮换、配置版本与连接停用的公开面集成测试
//! （CM-67 的代次隔离、CM-38 的配置版本、CM-39 的停用裁决）。
//!
//! 替身全部手写：`datazen_runtime::testing` 下的 fixture 由 `cfg(test)` 门控，
//! `tests/` 编译 lib 时并不带 `cfg(test)`，只能按本文件声明的公开接缝自建。
//! 候选替换的旅程在 `resource_replacement.rs`。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use datazen_runtime::connection::port::CancelDisposition;
use datazen_runtime::connection::types::ClientInstanceId;
use datazen_runtime::connection::{
    ConfigRevision, ConnectionId, ExecutionId, NamespaceTarget, OwnerRef, PoolKeyInputs, ResourceId,
};
use datazen_runtime::resource::{
    CacheFillOutcome, CancellationOutcome, DriverCleanVerdict, HostConditionSnapshot, LeasePurpose,
    LeaseRequest, MonotonicSource, PhysicalTransport, ResourceError, ResourceManager, RotationKind,
};

/// 固定时刻的单调时钟：这三组旅程断言的是代次与处置，不是时长。
struct FixedClock {
    nanos: AtomicU64,
}

impl FixedClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            nanos: AtomicU64::new(1_000),
        })
    }
}

impl MonotonicSource for FixedClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Default)]
struct Journal {
    opened: Vec<ResourceId>,
    closed: Vec<ResourceId>,
    cancelled: Vec<ExecutionId>,
    cancel_disposition: Option<CancelDisposition>,
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

    /// 让取消按指定的**实际**结果返回，而不是假装每次都取消成功。
    fn cancel_as(&self, disposition: CancelDisposition) {
        self.lock().cancel_disposition = Some(disposition);
    }

    fn opened(&self) -> Vec<ResourceId> {
        self.lock().opened.clone()
    }

    fn closed(&self) -> Vec<ResourceId> {
        self.lock().closed.clone()
    }

    fn cancelled(&self) -> Vec<ExecutionId> {
        self.lock().cancelled.clone()
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
        journal.opened.push(resource_id.clone());
        Ok(resource_id)
    }

    fn close(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        self.lock().closed.push(resource_id.clone());
        Ok(())
    }

    fn reset(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        self.lock().closed.push(resource_id.clone());
        Ok(())
    }

    fn cancel(
        &self,
        _resource_id: &ResourceId,
        execution_id: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError> {
        let mut journal = self.lock();
        journal.cancelled.push(execution_id.clone());
        Ok(journal
            .cancel_disposition
            .unwrap_or(CancelDisposition::Requested))
    }
}

fn connection(id: &str) -> ConnectionId {
    ConnectionId::new(id.to_owned())
}

fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-it-1"),
    }
}

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

/// 这三组旅程只裁决代次与停用，目录端口不参与，替身少一半。
fn manager() -> (ResourceManager, Arc<ScriptedTransport>, ConnectionId) {
    let transport = ScriptedTransport::new();
    let connection_id = connection("conn-rotate");
    let manager = ResourceManager::new(transport.clone(), FixedClock::new());
    (manager, transport, connection_id)
}

fn reason_of(error: ResourceError) -> &'static str {
    error.reason()
}

// ---------------------------------------------------------------------------
// CM-67：轮换后旧代材料不再签发，旧缓存结果不回填
// ---------------------------------------------------------------------------

#[test]
fn a_credential_rotation_retires_the_idle_leases_of_the_previous_generation() {
    let (mut manager, transport, connection_id) = manager();
    let old = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the old generation opens a resource");
    manager
        .release(
            &old.lease_id,
            HostConditionSnapshot::default(),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("a clean lease returns to the idle pool");
    assert_eq!(manager.idle_lease_count(), 1);

    let report = manager
        .rotate(
            RotationKind::Credentials,
            &request(&connection_id, "appdb").at_credential_revision(1),
        )
        .expect("credentials rotate forward");

    assert_eq!(report.retired_idles, vec![old.lease_id.clone()]);
    assert_eq!(
        transport.closed(),
        vec![old.resource_id.clone()],
        "the previous generation is closed, not left idle"
    );
    assert_eq!(manager.idle_lease_count(), 0);

    let fresh = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the new generation issues a fresh lease");
    assert_eq!(
        fresh.pool_key.credential_revision, 1,
        "an unpinned request adopts the current generation"
    );
}

#[test]
fn a_slow_cache_result_from_the_previous_generation_cannot_backfill_the_new_generation() {
    let (mut manager, _transport, connection_id) = manager();
    manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the first generation opens a resource");
    let offered = manager
        .cache_revision(&connection_id)
        .expect("the first generation is registered")
        .clone();

    manager
        .rotate(
            RotationKind::Credentials,
            &request(&connection_id, "appdb").at_credential_revision(1),
        )
        .expect("credentials rotate forward");
    assert_eq!(
        manager
            .cache_revision(&connection_id)
            .expect("the new generation is registered")
            .generation,
        2
    );

    let outcome = manager.admit_cache_fill(&connection_id, &offered);
    assert!(
        !outcome.stored(),
        "a result that belongs to the previous generation must not be stored"
    );
    assert_eq!(outcome.reason_code(), "cacheFillRejectedStale");
    assert!(matches!(
        outcome,
        CacheFillOutcome::RejectedStale {
            current: 2,
            offered: 1
        }
    ));
}

#[test]
fn a_lease_request_pinned_to_retired_material_is_refused() {
    let (mut manager, transport, connection_id) = manager();
    manager
        .rotate(
            RotationKind::NetworkRoute,
            &request(&connection_id, "appdb").at_network_route_revision(1),
        )
        .expect("the network route rotates forward");
    let before = transport.opened().len();

    let error = manager
        .acquire(&request(&connection_id, "appdb").at_network_route_revision(0))
        .expect_err("material from the retired generation must not be issued");
    assert_eq!(reason_of(error), "resourcePoolKeyRotated");
    assert_eq!(
        transport.opened().len(),
        before,
        "a refused request must not open a physical resource"
    );
}

// ---------------------------------------------------------------------------
// CM-38：配置版本漂移不静默重配活会话
// ---------------------------------------------------------------------------

#[test]
fn a_stale_expected_config_revision_is_refused_and_a_live_lease_is_not_reconfigured() {
    let (mut manager, _transport, connection_id) = manager();
    let live = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("the live lease opens a resource");
    assert!(
        !manager
            .config_drift(&live.lease_id)
            .expect("the live lease is registered against the current revision")
            .requires_replacement(),
        "a lease issued at the current revision is not drifting yet"
    );

    let error = manager
        .acquire(
            &request(&connection_id, "appdb").expecting_config_revision(ConfigRevision::new(99)),
        )
        .expect_err("an unknown expected revision is a conflict");
    assert_eq!(reason_of(error), "resourceConfigRevisionConflict");

    manager
        .rotate(
            RotationKind::AccessControl,
            &request(&connection_id, "appdb"),
        )
        .expect("an access control rotation advances the config revision");
    let drift = manager
        .config_drift(&live.lease_id)
        .expect("the live lease now drifts from the current revision");
    assert_eq!(drift.lease_revision.get(), live.config_revision.get());
    assert_eq!(drift.current_revision.get(), live.config_revision.get() + 1);
    assert_eq!(
        manager
            .lease(&live.lease_id)
            .expect("the live lease is still on the ledger")
            .config_revision,
        live.config_revision,
        "a live session is never silently reconfigured"
    );
}

// ---------------------------------------------------------------------------
// CM-39：停用后新请求不执行，在跑操作按实际取消结果记录
// ---------------------------------------------------------------------------

#[test]
fn disabling_a_connection_drops_queued_requests_and_records_the_actual_cancellation() {
    let (mut manager, transport, connection_id) = manager();
    transport.cancel_as(CancelDisposition::Unsupported);
    let running = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("a lease is issued before the disable");
    let execution = ExecutionId::new("exec-inflight");
    manager
        .begin_execution(&running.lease_id, execution.clone())
        .expect("the execution starts while the connection is still enabled");
    manager
        .enqueue(request(&connection_id, "appdb"))
        .expect("a queued request waits for a free resource");

    let outcome = manager
        .disable(&connection_id)
        .expect("disabling a connection is always possible");

    assert_eq!(
        outcome.dropped_requests, 1,
        "queued requests are dropped, not executed"
    );
    assert_eq!(manager.queued_len(), 0);
    assert_eq!(
        transport.cancelled(),
        vec![execution.clone()],
        "the running operation is cancelled through the host port"
    );
    assert_eq!(outcome.cancellations.len(), 1);
    let cancellation = &outcome.cancellations[0];
    assert_eq!(cancellation.execution_id, execution);
    assert_eq!(
        cancellation.outcome,
        CancellationOutcome::Unsupported {
            observed_at_nanos: 1_000
        }
    );
    assert!(
        !cancellation.outcome.cancelled(),
        "an unsupported cancel must be recorded as unsupported, not as cancelled"
    );

    let error = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect_err("a disabled connection issues no new lease");
    assert_eq!(reason_of(error), "resourceOwnerDisabled");
}

#[test]
fn a_cancel_that_actually_stopped_the_execution_is_recorded_as_cancelled() {
    let (mut manager, transport, connection_id) = manager();
    transport.cancel_as(CancelDisposition::Requested);
    let running = manager
        .acquire(&request(&connection_id, "appdb"))
        .expect("a lease is issued before the disable");
    let execution = ExecutionId::new("exec-cancelled");
    manager
        .begin_execution(&running.lease_id, execution.clone())
        .expect("the execution starts while the connection is still enabled");

    let outcome = manager
        .disable(&connection_id)
        .expect("disabling a connection is always possible");
    let cancellation = &outcome.cancellations[0];
    assert_eq!(cancellation.resource_id, running.resource_id);
    assert_eq!(
        cancellation.outcome,
        CancellationOutcome::Cancelled {
            observed_at_nanos: 1_000
        },
        "the outcome records what the port actually returned, stamped with the host clock"
    );
    assert!(cancellation.outcome.cancelled());
    assert!(
        outcome.quarantined.is_empty(),
        "an execution that really stopped does not need quarantine"
    );
    assert_eq!(transport.cancelled(), vec![execution]);
}
