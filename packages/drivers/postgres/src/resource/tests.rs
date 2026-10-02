//! Unit tests for the PostgreSQL resource provider.
//!
//! Every test here is **offline**: no server is contacted, so the suite is
//! deterministic and needs no credentials. What is tested is the contract
//! surface — what the provider declares, what it refuses, and which real code
//! each decision runs — not the wire protocol, which the live suite in
//! `tests/postgres_resource_contract.rs` covers.
//!
//! The seeds use `register_resource_for_test`, which is the same
//! `register_resource` the acquire path calls, with the socket half skipped.
//! Everything downstream of the seed (ownership checks, budget accounting,
//! transaction bookkeeping, the execution registry) is the production code.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_driver_api::capabilities::{CapabilityError, SessionContinuity};
use datazen_driver_api::namespace::{NamespaceLevelKind, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, ConnectionCostPolicy,
    DescribeResourceRequest, IdentityScope, InitializationRequirement, ResourceError,
    ResourceHandle, ResourceProvider, ResourcePurpose, ResourceScope, ResultChunk, ResultSink,
};
use datazen_driver_api::session::{CancelDisposition, TransactionOptions};
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, DriverError, QueryExecutionId};

use crate::postgres::PostgresDriver;

use super::capabilities::{
    postgres_capability_registry, postgres_capability_set, postgres_connection_cost,
    postgres_namespace_shape, CONTROL_POOL_CONNECTIONS, POSTGRES_CAPABILITY_REVISION,
    POSTGRES_DRIVER_VERSION, POSTGRES_PROVIDER_ID,
};
use super::PostgresResourceProvider;

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

/// Records every charge and every release so "released exactly once" is
/// checkable rather than assumed.
#[derive(Default)]
pub(crate) struct BudgetLedger {
    inner: Mutex<BudgetLedgerState>,
}

#[derive(Default)]
struct BudgetLedgerState {
    acquired: Vec<u32>,
    released: Vec<String>,
}

impl BudgetLedger {
    pub(crate) fn acquired(&self) -> Vec<u32> {
        self.lock().acquired.clone()
    }

    pub(crate) fn released(&self) -> Vec<String> {
        self.lock().released.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BudgetLedgerState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl BudgetPort for BudgetLedger {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        self.lock().acquired.push(requested);
        Ok(BudgetPermit {
            permit_id: format!("permit-{requested}"),
            physical_connections: requested,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        self.lock().released.push(permit.permit_id.clone());
        Ok(())
    }
}

/// A sink that records what the provider wrote. A `complete()` without any
/// `write()` is how "reported an empty success" would show up.
#[derive(Default)]
pub(crate) struct RecordingSink {
    chunks: Mutex<Vec<ResultChunk>>,
    completed: AtomicBool,
    failure: Mutex<Option<String>>,
}

impl RecordingSink {
    pub(crate) fn rows_written(&self) -> usize {
        self.chunks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    pub(crate) fn was_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    pub(crate) fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl ResultSink for RecordingSink {
    async fn write(&self, chunk: ResultChunk) -> Result<(), ResourceError> {
        self.chunks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(chunk);
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        self.completed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn fail(&self, reason: &str) -> Result<(), ResourceError> {
        *self
            .failure
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(reason.to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

pub(crate) fn config() -> ConnectionConfig {
    ConnectionConfig {
        id: "cfg".into(),
        name: "n".into(),
        database_type: "postgresql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(5432),
        database: Some("app_db".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

pub(crate) fn provider_with_driver() -> (Arc<PostgresDriver>, PostgresResourceProvider) {
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(Arc::clone(&driver));
    (driver, provider)
}

pub(crate) fn target() -> NamespaceTarget {
    NamespaceTarget::empty().with_database("app_db")
}

/// A target naming a level PostgreSQL does not have. PostgreSQL has a database
/// and a schema but no separate catalog, so this must always be refused.
pub(crate) fn catalog_target() -> NamespaceTarget {
    NamespaceTarget {
        database: Some("app_db".into()),
        catalog: Some("main".into()),
        ..NamespaceTarget::empty()
    }
}

/// A budget port the test can inspect afterwards.
pub(crate) fn ledger_port(ledger: &Arc<BudgetLedger>) -> Arc<dyn BudgetPort> {
    Arc::clone(ledger) as Arc<dyn BudgetPort>
}

fn describe_request(config: &ConnectionConfig) -> DescribeResourceRequest {
    DescribeResourceRequest {
        connection_config: config.clone(),
        identity_scope: IdentityScope::default(),
        target: target(),
        purpose: ResourcePurpose::InteractiveQuery,
    }
}

fn acquire_request(config: &ConnectionConfig, scope: ResourceScope) -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config.clone(),
        target: target(),
        scope,
        identity_scope: IdentityScope::default(),
        baseline: Baseline::new(Vec::new()),
    }
}

fn interactive_scope() -> ResourceScope {
    ResourceScope {
        purpose: ResourcePurpose::InteractiveQuery,
        max_physical_connections: 32,
        holds_open_transaction: false,
        pin_for_streaming: false,
    }
}

pub(crate) fn transaction_options() -> TransactionOptions {
    TransactionOptions {
        isolation_level: None,
        read_only: None,
        defers_commit: false,
    }
}

/// Register a live resource the way an acquire would, minus the socket.
pub(crate) fn seed_resource(
    provider: &PostgresResourceProvider,
    ledger: Arc<BudgetLedger>,
    label: &str,
) -> (ResourceHandle, ConnectionHandle) {
    let connection = ConnectionHandle {
        id: format!("conn-{label}"),
        pool_id: format!("pool-{label}"),
    };
    let handle = provider.register_resource_for_test(
        connection.clone(),
        BudgetPermit {
            permit_id: format!("permit-{label}"),
            physical_connections: 11,
        },
        ledger,
    );
    (handle, connection)
}

/// One initialization requirement the provider cannot replay.
pub(crate) fn unproven_requirement() -> AcquireResourceRequest {
    let config = config();
    AcquireResourceRequest {
        baseline: Baseline::new(vec![InitializationRequirement {
            id: "set_role".into(),
            sql: "SET ROLE admin".into(),
            mandatory: true,
            idempotent: true,
        }]),
        ..acquire_request(&config, interactive_scope())
    }
}

// ---------------------------------------------------------------------------
// 1. The provider is a real one, bound to real resources
// ---------------------------------------------------------------------------

#[tokio::test]
async fn provider_is_the_driver_it_was_built_from() {
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(Arc::clone(&driver));

    assert!(
        Arc::ptr_eq(provider.driver(), &driver),
        "the provider must share the driver's pools, not hold a second driver"
    );
    assert_eq!(provider.provider_id(), POSTGRES_PROVIDER_ID);
    assert_eq!(provider.provider_id(), "postgresql");
    assert_eq!(provider.capabilities().provider_id, POSTGRES_PROVIDER_ID);
    assert!(
        provider.runtime_epoch() > 0,
        "every handle carries a runtime epoch"
    );
}

#[tokio::test]
async fn two_provider_instances_do_not_share_a_runtime_epoch() {
    let (_driver_a, provider_a) = provider_with_driver();
    let (_driver_b, provider_b) = provider_with_driver();

    assert_ne!(
        provider_a.runtime_epoch(),
        provider_b.runtime_epoch(),
        "two live providers must not be able to accept each other's handles"
    );
}

// ---------------------------------------------------------------------------
// 2. The declaration matches the implementation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn declared_unsupported_capabilities_are_refused_by_the_registry() {
    let (_driver, provider) = provider_with_driver();
    let registry = provider.capabilities();

    let refusals: Vec<(&str, Result<(), CapabilityError>)> = vec![
        ("statefulSession", registry.require_stateful_session()),
        ("contextObservation", registry.require_context_observation()),
        ("resetForReuse", registry.require_reset_for_reuse()),
        ("snapshots", registry.require_snapshots()),
        // The registry names this capability by its declaration field, so the
        // rejection says `namespaceSwitch` even though the caller asked for an
        // in-place switch.
        (
            "namespaceSwitch",
            registry.require_in_place_namespace_switch(),
        ),
    ];

    for (capability, outcome) in refusals {
        let error = outcome
            .err()
            .unwrap_or_else(|| panic!("{capability} must be declared unsupported"));
        assert_eq!(
            error.capability, capability,
            "the rejection must name the capability the caller asked for"
        );
        assert_eq!(error.provider_id, POSTGRES_PROVIDER_ID);
    }
}

#[tokio::test]
async fn declared_supported_capabilities_are_the_ones_the_code_actually_has() {
    let (_driver, provider) = provider_with_driver();
    let registry = provider.capabilities();

    assert!(registry.require_precise_cancel().is_ok());
    assert!(registry.require_transaction_observation().is_ok());
    assert!(registry.require_session_scoped_handles().is_ok());

    let set = postgres_capability_set();
    assert!(
        set.declares_anything(),
        "postgres declares a non-default surface"
    );
    assert_eq!(set.transactions.max_open_transactions, Some(1));
    assert!(
        set.transactions.isolation_levels.is_empty(),
        "a bare BEGIN cannot honour a named level, so none may be listed"
    );
}

#[tokio::test]
async fn the_capability_snapshot_pins_identity_protocol_and_evidence() {
    let registry = postgres_capability_registry();
    let snapshot = &registry.snapshot;

    assert_eq!(snapshot.driver_id, POSTGRES_PROVIDER_ID);
    assert_eq!(snapshot.driver_version, POSTGRES_DRIVER_VERSION);
    assert_eq!(
        snapshot.protocol_version,
        datazen_driver_api::PROTOCOL_VERSION
    );
    assert_eq!(snapshot.capability_revision, POSTGRES_CAPABILITY_REVISION);

    for capability in [
        "preciseCancel",
        "transactionObservation",
        "sessionScopedHandles",
        "contextObservation",
        "namespaceSwitch",
        "statefulSession",
        "snapshots",
        "connectionCostPolicy",
    ] {
        assert!(
            snapshot.confirmed.contains_key(capability),
            "{capability} is declared, so it needs a recorded reason"
        );
    }
}

#[tokio::test]
async fn every_evidence_entry_that_names_a_source_file_names_a_real_one() {
    let registry = postgres_capability_registry();
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));

    for (capability, evidence) in &registry.snapshot.confirmed {
        for token in evidence.split_whitespace() {
            let Some(relative) = token.strip_prefix("src/") else {
                continue;
            };
            let Some(file) = relative.strip_suffix(".rs") else {
                continue;
            };
            let path = manifest_dir.join("src").join(format!("{file}.rs"));
            assert!(
                path.exists(),
                "the {capability} evidence names {token}, which is not a file in this crate"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Cost, namespace and continuity: pure math, no I/O
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_described_resource_charges_the_pool_plus_the_control_connection() {
    let (_driver, provider) = provider_with_driver();
    let config = config();

    let descriptor = provider
        .describe_resource(&describe_request(&config))
        .await
        .expect("a plain request describes cleanly");

    assert_eq!(descriptor.provider_id, POSTGRES_PROVIDER_ID);
    assert_eq!(
        descriptor.connection_cost_policy,
        ConnectionCostPolicy::PoolBounded {
            max_physical_connections: config.effective_max_pool_size() + CONTROL_POOL_CONNECTIONS,
        },
        "the real cost is the data pool plus the pg_cancel_backend control connection"
    );
    assert_eq!(
        descriptor.connection_cost_policy,
        postgres_connection_cost(&config)
    );
    assert_eq!(
        descriptor.session_continuity,
        SessionContinuity::Leased,
        "a PgPool is not a fixed session"
    );
    assert!(!descriptor.is_fixed_session());
    assert!(descriptor.initialization_requirements.is_empty());
    assert_eq!(descriptor.namespace_shape, postgres_namespace_shape());
    assert_ne!(descriptor.resource_key, "");
}

#[tokio::test]
async fn a_catalog_target_is_refused_because_postgres_has_no_catalog_level() {
    let (_driver, provider) = provider_with_driver();
    let config = config();
    let request = DescribeResourceRequest {
        target: catalog_target(),
        ..describe_request(&config)
    };

    match provider.describe_resource(&request).await {
        Err(ResourceError::NonexistentNamespaceLevel { kind }) => {
            assert_eq!(kind, NamespaceLevelKind::Catalog);
        }
        other => panic!("expected NonexistentNamespaceLevel, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 4. Acquire guards: every refusal is explicit, and none of them costs money
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_acquire_smaller_than_the_real_cost_is_denied() {
    let (_driver, provider) = provider_with_driver();
    let config = config();
    let scope = ResourceScope {
        max_physical_connections: 2,
        ..interactive_scope()
    };

    let outcome = provider
        .acquire_resource(
            &acquire_request(&config, scope),
            &ledger_port(&Arc::new(BudgetLedger::default())),
        )
        .await;

    match outcome {
        // `requested` is what the provider must ask the budget for — the same
        // meaning `BudgetPort::acquire_physical_connections` gives it. The
        // scope's own smaller ceiling is in the reason.
        Err(ResourceError::BudgetDenied { requested, reason }) => {
            assert_eq!(
                requested,
                config.effective_max_pool_size() + CONTROL_POOL_CONNECTIONS
            );
            assert!(
                reason.contains("2"),
                "the reason must name the scope that was refused: {reason}"
            );
        }
        other => panic!("expected BudgetDenied, got {other:?}"),
    }
}

#[tokio::test]
async fn an_acquire_that_pins_for_streaming_is_refused_not_downgraded() {
    let (_driver, provider) = provider_with_driver();
    let config = config();
    let scope = ResourceScope {
        pin_for_streaming: true,
        ..interactive_scope()
    };

    let error = provider
        .acquire_resource(
            &acquire_request(&config, scope),
            &ledger_port(&Arc::new(BudgetLedger::default())),
        )
        .await
        .expect_err("a pinned streaming connection is a fixed session");

    match error {
        ResourceError::OperationNotSupported {
            driver, operation, ..
        } => {
            assert_eq!(driver, POSTGRES_PROVIDER_ID);
            assert_eq!(operation, "acquire_resource");
        }
        other => panic!("expected OperationNotSupported, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unreplayable_baseline_is_refused() {
    let (_driver, provider) = provider_with_driver();
    let request = unproven_requirement();

    let error = provider
        .acquire_resource(&request, &ledger_port(&Arc::new(BudgetLedger::default())))
        .await
        .expect_err("no initialization replay is proven for postgres");

    assert!(
        matches!(error, ResourceError::OperationNotSupported { .. }),
        "expected OperationNotSupported, got {error:?}"
    );
}

#[tokio::test]
async fn no_refused_acquire_ever_charged_the_budget() {
    let (_driver, provider) = provider_with_driver();
    let config = config();
    let ledger = Arc::new(BudgetLedger::default());
    let budget = ledger_port(&ledger);

    let mut requests = vec![
        AcquireResourceRequest {
            target: catalog_target(),
            ..acquire_request(&config, interactive_scope())
        },
        acquire_request(
            &config,
            ResourceScope {
                pin_for_streaming: true,
                ..interactive_scope()
            },
        ),
        unproven_requirement(),
        acquire_request(
            &config,
            ResourceScope {
                max_physical_connections: 1,
                ..interactive_scope()
            },
        ),
    ];

    for request in &mut requests {
        assert!(
            provider.acquire_resource(request, &budget).await.is_err(),
            "every request in this list must be refused"
        );
    }

    assert!(
        ledger.acquired().is_empty(),
        "a refused acquire must be decided before the budget is charged; charged: {:?}",
        ledger.acquired()
    );
    assert!(ledger.released().is_empty());
}

#[tokio::test]
async fn a_connection_the_driver_cannot_open_is_reported_and_charges_nothing() {
    let (_driver, provider) = provider_with_driver();
    let config = ConnectionConfig {
        // Port 1 on loopback is refused immediately, so the test needs no
        // server and no credentials.
        port: Some(1),
        connection_timeout: 2,
        ..config()
    };
    let ledger = Arc::new(BudgetLedger::default());
    let budget = ledger_port(&ledger);

    let outcome = provider
        .acquire_resource(&acquire_request(&config, interactive_scope()), &budget)
        .await;

    match outcome {
        Err(ResourceError::Driver(DriverError::ConnectionFailed(_))) => {}
        other => panic!("expected the connect failure to surface, got {other:?}"),
    }
    assert_eq!(
        ledger.acquired(),
        vec![config.effective_max_pool_size() + CONTROL_POOL_CONNECTIONS]
    );
    assert_eq!(
        ledger.released().len(),
        1,
        "a failed connect must give the charge back, not strand it"
    );
}

// ---------------------------------------------------------------------------
// 5. Cancel precision — the headline contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancel_reaches_exactly_one_execution_and_leaves_every_other_untouched() {
    let (driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (target_handle, target_conn) = seed_resource(&provider, Arc::clone(&ledger), "target");
    let (other_handle, other_conn) = seed_resource(&provider, Arc::clone(&ledger), "other");

    let wanted = QueryExecutionId::new("exec-wanted");
    let sibling = QueryExecutionId::new("exec-sibling");
    let elsewhere = QueryExecutionId::new("exec-elsewhere");

    driver
        .prepare_query_execution_impl(&target_conn, &wanted)
        .await
        .expect("first execution registers");
    driver
        .prepare_query_execution_impl(&target_conn, &sibling)
        .await
        .expect("second execution registers");
    driver
        .prepare_query_execution_impl(&other_conn, &elsewhere)
        .await
        .expect("third execution registers on the other resource");

    let receipt = provider
        .request_cancel(&target_handle, &wanted)
        .await
        .expect("the cancel is addressed to a live execution");

    assert_eq!(receipt.execution_id, wanted);
    assert_eq!(receipt.disposition, CancelDisposition::Requested);
    assert!(
        receipt.state.is_none(),
        "a cancel request is not a terminal state, so no state may be substituted"
    );

    assert!(
        driver
            .is_cancel_requested(&target_conn, &wanted)
            .await
            .expect("the targeted execution is tracked"),
        "the requested execution must be marked"
    );
    assert!(
        !driver
            .is_cancel_requested(&target_conn, &sibling)
            .await
            .expect("the sibling is tracked"),
        "a sibling execution on the same connection must not be cancelled"
    );
    assert!(
        !driver
            .is_cancel_requested(&other_conn, &elsewhere)
            .await
            .expect("the other resource's execution is tracked"),
        "a concurrent execution on another resource must not be cancelled"
    );

    let _ = other_handle;
}

#[tokio::test]
async fn a_cancel_for_another_resources_execution_is_not_in_the_cancel_set() {
    let (driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (target_handle, target_conn) = seed_resource(&provider, Arc::clone(&ledger), "target");
    let (_other_handle, other_conn) = seed_resource(&provider, Arc::clone(&ledger), "other");

    let elsewhere = QueryExecutionId::new("exec-elsewhere");
    driver
        .prepare_query_execution_impl(&other_conn, &elsewhere)
        .await
        .expect("the execution registers on its own resource");

    let receipt = provider
        .request_cancel(&target_handle, &elsewhere)
        .await
        .expect("a miss is reported, not an error");

    assert_eq!(receipt.disposition, CancelDisposition::NotRegistered);
    assert!(receipt.state.is_none());
    assert!(
        !driver
            .is_cancel_requested(&other_conn, &elsewhere)
            .await
            .expect("the execution is still tracked"),
        "a rejected cancel must not have touched the other resource's execution"
    );
    let _ = target_conn;
}

#[tokio::test]
async fn a_cancel_for_an_unknown_execution_is_not_in_the_cancel_set() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    let receipt = provider
        .request_cancel(&handle, &QueryExecutionId::new("never-registered"))
        .await
        .expect("a miss is reported, not an error");

    assert_eq!(receipt.disposition, CancelDisposition::NotRegistered);
}

#[tokio::test]
async fn a_cancel_that_cannot_reach_the_backend_is_an_error_not_a_success() {
    let (driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, conn) = seed_resource(&provider, ledger, "target");

    let execution = QueryExecutionId::new("exec-bound");
    driver
        .prepare_query_execution_impl(&conn, &execution)
        .await
        .expect("the execution registers");
    driver
        .bind_backend_pid(&conn, &execution, 4242)
        .await
        .expect("the backend pid binds");

    // A pid is bound but this resource has no control pool, so
    // `pg_cancel_backend` cannot be issued. Reporting `Requested` here would be
    // a cancel that provably never happened.
    let outcome = provider.request_cancel(&handle, &execution).await;

    match outcome {
        Err(ResourceError::Driver(DriverError::ConnectionFailed(_))) => {}
        other => panic!(
            "a cancel that could not be issued must be an error, got {other:?} (a Requested \
             receipt would be a lie)"
        ),
    }
    assert!(
        !driver
            .is_cancel_requested(&conn, &execution)
            .await
            .expect("the execution is still tracked"),
        "a failed cancel must leave no pending flag behind"
    );
}
