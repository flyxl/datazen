//! Offline unit tests for the Redis resource provider.
//!
//! Every test here contacts **no server**, so the suite is deterministic and
//! needs no credentials. What is under test is the contract surface — what the
//! provider declares, what it refuses, and which real code each decision runs —
//! not the wire protocol.
//!
//! Two properties of these tests are deliberate and worth stating, because they
//! are what makes them worth having:
//!
//! * **The seeds are production code.** Resources are registered through
//!   `register_resource_for_test`, which is the same `register_resource` the
//!   acquire path calls, with only the socket half skipped. Ownership checks,
//!   budget accounting, revision tracking and the `closed` set are the real
//!   implementations.
//! * **A missing connection is an error, never a default.** Because the seeds
//!   carry no live `RedisConn`, every method that needs to talk to the server
//!   fails. That is the point: a test that asserted `Ok` here would be
//!   asserting a shell. If any of these methods ever grows a "returns a
//!   plausible value when the connection is gone" path, these tests go red.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_driver_api::capabilities::{
    Availability, CapabilityError, CapabilityRegistry, ContextObservation, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionContinuity, SessionScopedHandleSupport,
    SnapshotSupport,
};
use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
use datazen_driver_api::namespace::{NamespaceLevelKind, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, IdentityScope, InitializationRequirement, ResourceError,
    ResourceHandle, ResourceProvider, ResourcePurpose, ResourceScope, ResultChunk, ResultSink,
};
use datazen_driver_api::session::TransactionOptions;
use datazen_driver_api::{ConnectionConfig, ConnectionHandle, QueryExecutionId};

use crate::driver::RedisDriver;
use crate::RedisResourceProvider;

use super::capabilities::{
    redis_capability_registry, redis_capability_set, redis_namespace_shape,
    REDIS_CAPABILITY_REVISION, REDIS_DRIVER_VERSION, REDIS_PROVIDER_ID,
};

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

/// Records every charge and every release, so "released exactly once" is
/// checkable rather than assumed.
#[derive(Default)]
pub(super) struct BudgetLedger {
    inner: Mutex<LedgerState>,
}

#[derive(Default)]
pub(super) struct LedgerState {
    acquired: Vec<u32>,
    released: Vec<String>,
}

impl BudgetLedger {
    pub(super) fn acquired(&self) -> Vec<u32> {
        self.lock().acquired.clone()
    }

    pub(super) fn released(&self) -> Vec<String> {
        self.lock().released.clone()
    }

    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, LedgerState> {
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

/// A sink that records what the provider wrote. A `complete()` with no
/// preceding `write()` is exactly how "reported an empty success" would show up.
#[derive(Default)]
pub(super) struct RecordingSink {
    chunks: Mutex<Vec<ResultChunk>>,
    completed: AtomicBool,
    failure: Mutex<Option<String>>,
}

impl RecordingSink {
    pub(super) fn rows_written(&self) -> usize {
        self.chunks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    pub(super) fn was_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    pub(super) fn failure(&self) -> Option<String> {
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

/// A TCP port nothing listens on. Reserved for "no server" in these tests.
const UNREACHABLE_REDIS_PORT: u16 = 65001;

pub(super) fn config() -> ConnectionConfig {
    ConnectionConfig {
        id: "cfg".into(),
        name: "n".into(),
        database_type: "redis".into(),
        // Deliberately unreachable. This crate has no live-server harness, so
        // every test here must behave the same on a developer machine that has
        // Redis on 6379 as on one that does not. Pointing this at the default
        // port would make the suite's outcome depend on what the host happens
        // to be running — and `acquire_resource` would silently succeed.
        host: Some("127.0.0.1".into()),
        port: Some(UNREACHABLE_REDIS_PORT),
        database: Some("db3".into()),
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

/// A connection handle shaped exactly like the one `RedisDriver::connect`
/// issues: `id == pool_id`, because there is no pool — only one connection.
pub(super) fn connection_handle(key: &str) -> ConnectionHandle {
    ConnectionHandle {
        id: key.to_string(),
        pool_id: key.to_string(),
    }
}

pub(super) fn provider() -> RedisResourceProvider {
    RedisResourceProvider::new(Arc::new(RedisDriver::new()))
}

pub(super) fn target() -> NamespaceTarget {
    NamespaceTarget::empty().with_database("db3")
}

pub(super) fn ledger_port(ledger: &Arc<BudgetLedger>) -> Arc<dyn BudgetPort> {
    Arc::clone(ledger) as Arc<dyn BudgetPort>
}

/// Register a live resource the way an acquire would, minus the socket. The
/// driver holds no connection for this key, so any method that must talk to the
/// server will fail — which is the honest offline state, not a fake success.
pub(super) fn seed(provider: &RedisResourceProvider, ledger: &Arc<BudgetLedger>) -> ResourceHandle {
    provider.register_resource_for_test(
        connection_handle("redis_test"),
        BudgetPermit {
            permit_id: "permit-1".into(),
            physical_connections: 1,
        },
        ledger_port(ledger),
        3,
    )
}

pub(super) fn describe_request(config: &ConnectionConfig) -> DescribeResourceRequest {
    DescribeResourceRequest {
        connection_config: config.clone(),
        identity_scope: IdentityScope::default(),
        target: target(),
        purpose: ResourcePurpose::InteractiveQuery,
    }
}

pub(super) fn acquire_request(
    config: &ConnectionConfig,
    scope: ResourceScope,
) -> AcquireResourceRequest {
    AcquireResourceRequest {
        connection_config: config.clone(),
        target: target(),
        scope,
        identity_scope: IdentityScope::default(),
        baseline: Baseline::new(Vec::new()),
    }
}

pub(super) fn interactive_scope() -> ResourceScope {
    ResourceScope {
        purpose: ResourcePurpose::InteractiveQuery,
        max_physical_connections: 8,
        holds_open_transaction: false,
        pin_for_streaming: false,
    }
}

pub(super) fn transaction_options() -> TransactionOptions {
    TransactionOptions {
        isolation_level: None,
        read_only: None,
        defers_commit: false,
    }
}

pub(super) fn execution_id() -> QueryExecutionId {
    QueryExecutionId::new("exec-1")
}

pub(super) fn command() -> CommandCall {
    CommandCall::new("redis.get", serde_json::json!({"key": "k"}))
}

/// A handle that is real-looking but belongs to nobody: minted by this provider
/// id at an epoch no runtime of this provider ever issued.
pub(super) fn foreign_handle() -> ResourceHandle {
    ResourceHandle::issue(REDIS_PROVIDER_ID, "redis_foreign", 999_999)
}

// ---------------------------------------------------------------------------
// What is declared
// ---------------------------------------------------------------------------

/// The three cells this provider flips off their fail-closed default. Named
/// once so the declaration test, the receipt test and the gate table cannot
/// drift apart — the count in `declares_exactly_the_three_provable_cells` is
/// this array's length, not a number typed twice.
const FLIPPED_CELLS: [&str; 3] = ["statefulSession", "namespaceSwitch", "contextObservation"];

#[test]
fn provider_id_is_redis() {
    assert_eq!(provider().provider_id(), REDIS_PROVIDER_ID);
}

#[test]
fn declares_more_than_the_default_set() {
    // The gate: an unmigrated driver's registry is exactly `CapabilitySet::default()`.
    // If this ever passes through the legacy adapter again, it goes red.
    let provider = provider();
    let registry = provider.capabilities();
    assert_ne!(
        registry.capabilities,
        datazen_driver_api::capabilities::CapabilitySet::default(),
        "a Redis provider that declares nothing is indistinguishable from an unmigrated driver"
    );
}

#[test]
fn declares_exactly_the_three_provable_cells() {
    let set = redis_capability_set();

    assert_eq!(
        FLIPPED_CELLS.len(),
        3,
        "the count in the test name is the count here"
    );
    assert_eq!(set.stateful_session, Availability::Supported);
    assert_eq!(set.namespace_switch, NamespaceSwitch::InPlace);
    assert_eq!(set.context_observation, ContextObservation::Partial);
    // `data` and `backup` stay at their defaults: the contract has no axis that
    // proves them, and the fail-closed answer is the true one.
    assert_eq!(set.data, DataSupport::default());
    assert_eq!(set.backup, BackupSupport::default());
}

#[test]
fn unprovable_cells_stay_at_their_fail_closed_default() {
    // These seven are not "unfinished": each is the true answer for Redis. The
    // test exists so a later track cannot quietly promote one of them without
    // this file noticing.
    let set = redis_capability_set();

    assert_eq!(
        set.session_scoped_handles,
        SessionScopedHandleSupport::Unknown
    );
    assert_eq!(set.reset_for_reuse, ResetForReuse::Unsupported);
    assert_eq!(set.precise_cancel, PreciseCancelSupport::Unknown);
    assert_eq!(set.snapshots, SnapshotSupport::Unsupported);
    assert_eq!(
        set.transaction_observation,
        datazen_driver_api::capabilities::TransactionObservation::Unsupported
    );
    assert!(set.transactions.isolation_levels.is_empty());
    assert_eq!(
        set.transactions.savepoints,
        Availability::Unknown,
        "Redis has no savepoints and no rollback; claiming otherwise would be a false promise"
    );
    assert_eq!(
        set.ddl_atomicity,
        datazen_driver_api::capabilities::DdlAtomicitySupport::default()
    );
}

#[test]
fn every_confirmed_capability_carries_evidence() {
    let provider = provider();
    let registry: &CapabilityRegistry = provider.capabilities();
    assert_eq!(registry.provider_id, REDIS_PROVIDER_ID);
    assert_eq!(registry.snapshot.driver_id, REDIS_PROVIDER_ID);
    assert_eq!(registry.snapshot.driver_version, REDIS_DRIVER_VERSION);
    assert_eq!(
        registry.snapshot.capability_revision, REDIS_CAPABILITY_REVISION,
        "bump REDIS_CAPABILITY_REVISION whenever redis_capability_set changes meaning"
    );
    assert!(
        !registry.snapshot.confirmed.is_empty(),
        "a confirmed capability with no receipt is an assertion, not evidence"
    );
}

/// The three flipped cells are the ones that get receipts; the declaration and
/// the evidence cannot drift apart because the same `BTreeMap` builds both.
#[test]
fn a_receipt_exists_for_every_capability_the_set_flips() {
    let registry = redis_capability_registry();

    for capability in FLIPPED_CELLS {
        let receipt = registry
            .snapshot
            .confirmed
            .get(capability)
            .unwrap_or_else(|| panic!("{capability} is flipped with no receipt under that key"));
        assert!(
            !receipt.trim().is_empty(),
            "{capability} has a receipt key with nothing in it"
        );
    }
}

/// The receipts are not only for the cells that were flipped: every cell left
/// at its default carries one too, prefixed `declined:` and naming why the
/// default is the *true* answer. That prefix is load-bearing, so it is checked
/// against the declaration — a `declined:` receipt on a flipped cell would mean
/// the two tables disagree about the same capability.
#[test]
fn a_declined_receipt_never_sits_on_a_flipped_cell() {
    let registry = redis_capability_registry();

    for (capability, receipt) in &registry.snapshot.confirmed {
        let declined = receipt.starts_with("declined:");
        assert_eq!(
            declined,
            !FLIPPED_CELLS.contains(&capability.as_str()),
            "{capability} is {declined} but the declaration says otherwise"
        );
    }
}

/// No receipt may invent a capability name that is not one of the twelve cells.
#[test]
fn every_receipt_is_keyed_by_a_real_capability_name() {
    const KNOWN: [&str; 12] = [
        "statefulSession",
        "namespaceSwitch",
        "contextObservation",
        "transactionObservation",
        "sessionScopedHandles",
        "resetForReuse",
        "preciseCancel",
        "snapshots",
        "transactions",
        "ddlAtomicity",
        "data",
        "backup",
    ];

    for capability in redis_capability_registry().snapshot.confirmed.keys() {
        assert!(
            KNOWN.contains(&capability.as_str()),
            "{capability} is not one of the twelve CapabilitySet cells"
        );
    }
}

/// `CapabilityRegistry`'s `require_*` methods are the gate every contract caller
/// goes through, so this table covers *all thirteen* of them. Two open; eleven
/// reject, each naming the capability it refused so a caller learns what it
/// lost instead of getting a bare `false`.
#[test]
fn exactly_two_of_the_thirteen_capability_gates_open() {
    let registry = redis_capability_registry();

    assert!(registry.require_stateful_session().is_ok());
    assert!(registry.require_in_place_namespace_switch().is_ok());

    for (method, expected, outcome) in [
        (
            "require_context_observation",
            "contextObservation",
            registry.require_context_observation(),
        ),
        (
            "require_transaction_observation",
            "transactionObservation",
            registry.require_transaction_observation(),
        ),
        (
            "require_session_scoped_handles",
            "sessionScopedHandles",
            registry.require_session_scoped_handles(),
        ),
        (
            "require_reset_for_reuse",
            "resetForReuse",
            registry.require_reset_for_reuse(),
        ),
        (
            "require_precise_cancel",
            "preciseCancel",
            registry.require_precise_cancel(),
        ),
        (
            "require_snapshots",
            "snapshots",
            registry.require_snapshots(),
        ),
        (
            "require_row_read",
            "data.rowRead",
            registry.require_row_read(),
        ),
        (
            "require_row_write",
            "data.rowWrite",
            registry.require_row_write(),
        ),
        (
            "require_streaming_results",
            "data.streamingResults",
            registry.require_streaming_results(),
        ),
        (
            "require_backup_artifact",
            "backup.artifact",
            registry.require_backup_artifact(),
        ),
        (
            "require_restore_from_artifact",
            "backup.restore",
            registry.require_restore_from_artifact(),
        ),
    ] {
        match outcome {
            Ok(()) => panic!("{method} granted a capability Redis never declared"),
            Err(CapabilityError { capability, .. }) => assert_eq!(
                capability, expected,
                "{method} refused the wrong capability"
            ),
        }
    }
}

/// `Partial` opens no gate, and that is the point of this test: the driver-api
/// predicate is `confirms_context() == Full` (capabilities.rs:390-396). So
/// declaring `Partial` keeps every confirmed-context caller refused, while still
/// telling them the session is worth observing. Flipping this cell to `Full`
/// would silently open `require_context_observation`, so the flip has to be
/// made here, in the open block above, and never anywhere else.
#[test]
fn a_partial_context_observation_still_refuses_a_confirmed_context_gate() {
    assert!(!ContextObservation::Partial.confirms_context());

    let registry = redis_capability_registry();
    assert_eq!(
        redis_capability_set().context_observation,
        ContextObservation::Partial
    );
    assert!(matches!(
        registry.require_context_observation(),
        Err(CapabilityError { .. })
    ));
}

// ---------------------------------------------------------------------------
// What is refused
// ---------------------------------------------------------------------------

#[test]
fn namespace_shape_has_a_database_and_nothing_else() {
    let shape = redis_namespace_shape();
    assert!(shape
        .level(NamespaceLevelKind::Database)
        .is_some_and(|level| level.exists));
    assert!(
        !shape
            .level(NamespaceLevelKind::Catalog)
            .is_some_and(|level| level.exists),
        "Redis has no catalog level; declaring one would make canonicalize accept a target \
         the server cannot honour"
    );
    assert!(
        !shape
            .level(NamespaceLevelKind::Schema)
            .is_some_and(|level| level.exists),
        "Redis keys are not schema-qualified"
    );
}

#[test]
fn canonicalize_refuses_a_catalog_or_schema_level() {
    let shape = redis_namespace_shape();

    let catalog = NamespaceTarget {
        database: Some("db3".into()),
        catalog: Some("main".into()),
        ..NamespaceTarget::empty()
    };
    assert!(
        shape.canonicalize(&catalog).is_err(),
        "a catalog target must be refused, not silently reduced to its database"
    );

    let schema = NamespaceTarget {
        database: Some("db3".into()),
        schema: Some("public".into()),
        ..NamespaceTarget::empty()
    };
    assert!(shape.canonicalize(&schema).is_err());
}

#[test]
fn canonicalize_leaves_an_absent_database_to_the_protocol_default() {
    // A fresh connection is on db 0 by protocol, so an empty target is legal
    // rather than an error. The shape reports no resolved path for it, and
    // `validate_acquisition` is what turns "no name" into `SELECT 0` — the
    // shape must not invent a name the caller never gave.
    let canonical = redis_namespace_shape()
        .canonicalize(&NamespaceTarget::empty())
        .expect("an empty target is legal: Redis always has a current database");
    assert!(
        canonical.path.is_empty(),
        "the shape resolved {:?} out of a target that named nothing",
        canonical.path
    );
    assert_eq!(canonical.requested, NamespaceTarget::empty());

    // A named database *is* resolved, and it is resolved verbatim: Redis folds
    // no case in the namespace, so `db3` must not become `DB3`.
    let named = redis_namespace_shape()
        .canonicalize(&target())
        .expect("db3 is a database this shape declares");
    assert_eq!(named.path, vec!["db3".to_string()]);
}

// ---------------------------------------------------------------------------
// describe_resource
// ---------------------------------------------------------------------------

#[tokio::test]
async fn describes_a_fixed_single_use_resource_costing_one_connection() {
    let provider = provider();
    let config = config();

    let descriptor = provider
        .describe_resource(&describe_request(&config))
        .await
        .expect("a legal target describes");

    assert_eq!(descriptor.provider_id, REDIS_PROVIDER_ID);
    assert_eq!(descriptor.resource_key, "redis_session:config:cfg");
    assert_eq!(
        descriptor.session_continuity,
        SessionContinuity::Fixed,
        "one Redis handle is one wire connection, so the session cannot be handed back and \
         leased to anyone else"
    );
    assert_eq!(
        descriptor.reuse_policy,
        datazen_driver_api::resource::ReusePolicy::SingleUse
    );
    assert!(
        descriptor.initialization_requirements.is_empty(),
        "no initialization protocol is proven for this driver, so none is declared"
    );
    assert_eq!(
        descriptor.connection_cost_policy,
        ConnectionCostPolicy::PoolBounded {
            max_physical_connections: 1
        },
        "a Redis session is exactly one connection; claiming a pool would misprice it"
    );
    assert_eq!(descriptor.namespace_shape, redis_namespace_shape());
}

#[tokio::test]
async fn describing_an_impossible_target_is_refused() {
    let provider = provider();
    let mut request = describe_request(&config());
    request.target = NamespaceTarget {
        database: Some("db3".into()),
        schema: Some("public".into()),
        ..NamespaceTarget::empty()
    };

    assert!(provider.describe_resource(&request).await.is_err());
}

// ---------------------------------------------------------------------------
// acquire_resource
// ---------------------------------------------------------------------------

#[tokio::test]
async fn acquire_refuses_a_scope_that_holds_a_transaction_without_charging() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let mut request = acquire_request(&config(), interactive_scope());
    request.scope.holds_open_transaction = true;

    let error = provider
        .acquire_resource(&request, &ledger_port(&ledger))
        .await
        .expect_err("Redis cannot hold a transaction open");

    assert!(matches!(
        error,
        ResourceError::OperationNotSupported { operation, .. } if operation == "acquire_resource"
            || operation == "begin_transaction"
    ));
    assert!(
        ledger.acquired().is_empty(),
        "a refused request must not cost the caller a permit"
    );
}

#[tokio::test]
async fn acquire_refuses_a_zero_connection_budget() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let mut scope = interactive_scope();
    scope.max_physical_connections = 0;

    let error = provider
        .acquire_resource(&acquire_request(&config(), scope), &ledger_port(&ledger))
        .await
        .expect_err("a scope of zero connections cannot buy one");

    assert!(matches!(
        error,
        ResourceError::BudgetDenied { requested: 1, .. }
    ));
}

#[tokio::test]
async fn acquire_refuses_a_baseline_it_cannot_replay() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let mut request = acquire_request(&config(), interactive_scope());
    request.baseline = Baseline::new(vec![InitializationRequirement {
        id: "warm".into(),
        sql: "SET a 1".into(),
        mandatory: true,
        idempotent: true,
    }]);

    let error = provider
        .acquire_resource(&request, &ledger_port(&ledger))
        .await
        .expect_err("replaying a baseline has no verified implementation here");

    assert!(matches!(
        error,
        ResourceError::OperationNotSupported { ref reason, .. } if reason.contains("1")
    ));
    assert!(ledger.acquired().is_empty());
}

#[tokio::test]
async fn acquire_accepts_pinning_because_the_session_is_declared_stateful() {
    // Not asserted as a success — there is no server here. Asserted as *not
    // refused*: `pin_for_streaming` is legal for a driver that declares
    // `stateful_session`, and a refusal would contradict the declaration.
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let mut scope = interactive_scope();
    scope.pin_for_streaming = true;

    let error = provider
        .acquire_resource(&acquire_request(&config(), scope), &ledger_port(&ledger))
        .await
        .expect_err("the fixture points at a closed port");

    // The refusal has to come from the connect, not from validation: pinning is
    // legal for a session that declares `stateful_session`, and a refusal here
    // would contradict that declaration. `Driver` is the connect failing.
    assert!(
        matches!(error, ResourceError::Driver(_)),
        "pinning must pass validation and fail at the connect, got {error:?}"
    );
}
