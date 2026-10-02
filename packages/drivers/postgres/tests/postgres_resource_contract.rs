//! The PostgreSQL resource contract, seen from outside the crate.
//!
//! The unit tests in `src/resource/tests.rs` can reach private helpers; this
//! file cannot, and that is the point. Everything here goes through the public
//! surface a host uses — `iter_driver_factories`, `require_resource_provider`,
//! `DatabaseDriverFactory::resource_capabilities` and the `ResourceProvider`
//! trait — so a provider that only works because of a `pub(crate)` back door
//! cannot pass here.
//!
//! Three things are checked, in order of how badly a failure would hurt:
//!
//! 1. **The provider exists and is bound to this crate.** The postgres factory
//!    answers `require_resource_provider` and the capability snapshot it hands
//!    out carries this crate's own id and version.
//! 2. **Un-migrated paths stay fail-closed.** `questdb` and `cloudberry` are
//!    still registered by this crate and have not been migrated; asking them
//!    for a provider must error rather than hand back `None` that a caller
//!    treats as "nothing to do". The un-migrated *methods* fail closed too: a
//!    handle this provider never issued, or one from a retired epoch, is an
//!    error on every entry point.
//!
//! The capability declarations themselves — the P2 exit gate — live in
//! `postgres_resource_capabilities.rs`, which is the same view of the same
//! crate split out to stay under the 800-line ceiling.
//!
//! Everything above the live section is offline: it needs no server. The
//! live section at the bottom is `#[ignore]`d and reads credentials from the
//! **process environment only** (`TEST_PG_*`), matching
//! `tests/postgres_cross_database.rs` — no file fallback, see AGENTS.md
//! 「本地环境变量文件保护」.

use std::sync::{Arc, Mutex};

use datazen_driver_api::capabilities::{NamespaceSwitch, SessionContinuity};
use datazen_driver_api::namespace::{NamespaceLevelKind, NamespaceTarget};
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, CommandCall, ConnectionCostPolicy,
    DescribeResourceRequest, IdentityScope, InitializationRequirement, ResourceError,
    ResourceHandle, ResourceProvider, ResourcePurpose, ResourceScope, ResultChunk, ResultSink,
};
use datazen_driver_api::session::{CancelDisposition, CloseDisposition, TransactionOptions};
use datazen_driver_api::{
    iter_driver_factories, require_resource_provider, ConnectionConfig, DatabaseDriverFactory,
    ResourceProviderMissing,
};
// The test binary only links this crate — and therefore only runs the
// `inventory::submit!` in `lib.rs` that makes any factory findable at all —
// when something below actually names it.
use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};

/// The provider id postgres declares in its capability snapshot.
const POSTGRES_PROVIDER_ID: &str = "postgresql";
/// One physical connection is reserved for cancellation, on its own pool.
const CONTROL_POOL_CONNECTIONS: u32 = 1;

fn factory(driver_id: &str) -> &'static dyn DatabaseDriverFactory {
    iter_driver_factories()
        .into_iter()
        .copied()
        .find(|factory| factory.driver_id() == driver_id)
        .unwrap_or_else(|| panic!("driver id {driver_id} must be registered by this crate"))
}

fn postgres() -> &'static dyn DatabaseDriverFactory {
    factory(POSTGRES_PROVIDER_ID)
}

/// A real `ConnectionConfig` shaped like a host-built one. No server is
/// contacted while these tests run.
fn config() -> ConnectionConfig {
    ConnectionConfig {
        id: "pg-resource-contract".into(),
        name: "pg resource contract".into(),
        database_type: POSTGRES_PROVIDER_ID.into(),
        host: Some("127.0.0.1".into()),
        port: Some(5432),
        database: Some("app_db".into()),
        schema: Some("public".into()),
        username: Some("contract_reader".into()),
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

fn target() -> NamespaceTarget {
    NamespaceTarget::empty().with_database("app_db")
}

fn describe_request(config: &ConnectionConfig) -> DescribeResourceRequest {
    DescribeResourceRequest {
        connection_config: config.clone(),
        identity_scope: IdentityScope::default(),
        target: target(),
        purpose: ResourcePurpose::InteractiveQuery,
    }
}

fn interactive_scope(max_physical_connections: u32) -> ResourceScope {
    ResourceScope {
        purpose: ResourcePurpose::InteractiveQuery,
        max_physical_connections,
        holds_open_transaction: false,
        pin_for_streaming: false,
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

fn transaction_options() -> TransactionOptions {
    TransactionOptions {
        isolation_level: None,
        read_only: None,
        defers_commit: false,
    }
}

/// A budget port that records what it was asked for, so a test can prove a
/// refused acquire never charged anything.
#[derive(Default)]
struct BudgetLedger {
    state: Mutex<LedgerState>,
}

#[derive(Default)]
struct LedgerState {
    acquired: Vec<u32>,
    released: Vec<String>,
}

impl BudgetLedger {
    fn acquired(&self) -> Vec<u32> {
        self.state
            .lock()
            .map(|state| state.acquired.clone())
            .unwrap_or_default()
    }

    fn released(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|state| state.released.clone())
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl BudgetPort for BudgetLedger {
    async fn acquire_physical_connections(
        &self,
        requested: u32,
    ) -> Result<BudgetPermit, ResourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.acquired.push(requested);
        Ok(BudgetPermit {
            permit_id: format!("permit-{}", state.acquired.len()),
            physical_connections: requested,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.released.push(permit.permit_id.clone());
        Ok(())
    }
}

/// A sink that must never be written to by these tests. The contract says an
/// execution that cannot be attempted produces an error, not silent rows.
struct UnreachableSink {
    written: Mutex<u32>,
}

impl UnreachableSink {
    fn new() -> Self {
        Self {
            written: Mutex::new(0),
        }
    }

    fn written(&self) -> u32 {
        *self
            .written
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait::async_trait]
impl ResultSink for UnreachableSink {
    async fn write(&self, _chunk: ResultChunk) -> Result<(), ResourceError> {
        let mut written = self
            .written
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *written += 1;
        Ok(())
    }

    async fn complete(&self) -> Result<(), ResourceError> {
        Err(ResourceError::SinkRejected {
            reason: "this sink exists to prove nothing wrote through it".into(),
        })
    }

    async fn fail(&self, _reason: &str) -> Result<(), ResourceError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 1. The provider exists and is bound to this crate.
// ---------------------------------------------------------------------------

#[test]
fn the_postgres_factory_never_reports_a_missing_provider() {
    let factory = postgres();
    assert!(
        factory.resource_provider().is_some(),
        "postgres implements the resource contract; reporting None would be a silent downgrade"
    );

    let provider = require_resource_provider(factory)
        .expect("require_resource_provider must succeed for a migrated driver");
    let again = factory
        .resource_provider()
        .expect("the optional accessor agrees with the fail-closed one");
    assert!(
        Arc::ptr_eq(&provider, &again),
        "both accessors must hand back the one provider bound to the one driver, not two instances"
    );
}

#[test]
fn the_provider_identifies_itself_as_this_crate() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    assert_eq!(provider.provider_id(), POSTGRES_PROVIDER_ID);

    let registry = provider.capabilities();
    assert_eq!(registry.provider_id, POSTGRES_PROVIDER_ID);
    assert_eq!(
        registry.snapshot.driver_id, POSTGRES_PROVIDER_ID,
        "the snapshot must name the driver, not a generic string"
    );
    assert_eq!(
        registry.snapshot.driver_version,
        env!("CARGO_PKG_VERSION"),
        "the snapshot must carry the version of the crate that implemented it"
    );
    assert_eq!(
        registry.snapshot.protocol_version,
        datazen_driver_api::PROTOCOL_VERSION,
        "an unchanged contract means an unchanged protocol version"
    );
    assert!(
        registry.snapshot.capability_revision > 0,
        "a declared capability set must carry a revision so a stale snapshot is detectable"
    );
}

#[test]
fn the_factory_and_the_provider_declare_the_same_capabilities() {
    let factory = postgres();
    let from_factory = factory.resource_capabilities();
    let from_provider = require_resource_provider(factory)
        .expect("postgres has a provider")
        .capabilities()
        .capabilities
        .clone();

    assert_eq!(
        from_factory, from_provider,
        "the two declaration paths must not be able to drift apart"
    );
    assert!(
        from_factory.declares_anything(),
        "an empty set would make every host feature quietly unavailable"
    );
}

// ---------------------------------------------------------------------------
// 3. A resource is acquired for a real price, and nothing unproven is replayed.
// ---------------------------------------------------------------------------

/// A resource here is a leased pool slot, so switching the target namespace
/// needs a different resource. `SessionContinuity::Fixed` plus
/// `NamespaceSwitch::InPlace` is the shape that would make a caller believe a
/// `SET` had been applied; neither is declared.
#[tokio::test]
async fn a_descriptor_refuses_to_claim_a_fixed_session() {
    let config = config();
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");

    let descriptor = provider
        .describe_resource(&describe_request(&config))
        .await
        .expect("describe needs no server");

    assert_eq!(descriptor.session_continuity, SessionContinuity::Leased);
    assert!(!descriptor.session_continuity.is_fixed());
    assert!(!descriptor.is_fixed_session());
    assert_eq!(
        require_resource_provider(postgres())
            .expect("postgres has a provider")
            .capabilities()
            .capabilities
            .namespace_switch,
        NamespaceSwitch::RequiresReplacement,
        "switching database replaces the resource, so no in-place claim is made"
    );
}

#[tokio::test]
async fn describe_states_the_real_physical_cost_and_refuses_levels_postgres_has_not_got() {
    let config = config();
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");

    let descriptor = provider
        .describe_resource(&describe_request(&config))
        .await
        .expect("describe needs no server");

    assert_eq!(descriptor.provider_id, POSTGRES_PROVIDER_ID);
    match descriptor.connection_cost_policy {
        ConnectionCostPolicy::PoolBounded {
            max_physical_connections,
        } => assert_eq!(
            max_physical_connections,
            config.effective_max_pool_size() + CONTROL_POOL_CONNECTIONS,
            "the declared cost must include the control connection used for cancellation"
        ),
        other => panic!("postgres pools are bounded by the pool size, not {other:?}"),
    }
    assert_eq!(descriptor.session_continuity, SessionContinuity::Leased);
    assert!(!descriptor.is_fixed_session());
    assert!(
        descriptor.initialization_requirements.is_empty(),
        "no unproven initialization is replayed on acquire"
    );

    // PostgreSQL has a database and a schema. It has no separate catalog level,
    // so a catalog target must be refused by name, not rounded down to the
    // database.
    let with_catalog = DescribeResourceRequest {
        target: NamespaceTarget {
            catalog: Some("main".into()),
            ..target()
        },
        ..describe_request(&config)
    };
    match provider.describe_resource(&with_catalog).await {
        Err(ResourceError::NonexistentNamespaceLevel { kind }) => {
            assert_eq!(kind, NamespaceLevelKind::Catalog)
        }
        other => panic!("a catalog is not a level PostgreSQL has: {other:?}"),
    }
}

#[tokio::test]
async fn a_scope_that_cannot_pay_for_the_resource_is_denied_before_anything_is_opened() {
    let config = config();
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let ledger = Arc::new(BudgetLedger::default());
    let budget: Arc<dyn BudgetPort> = Arc::clone(&ledger) as Arc<dyn BudgetPort>;

    // The provider needs the data pool plus the control connection. A scope
    // that names fewer must be told so instead of being quietly satisfied.
    let error = provider
        .acquire_resource(&acquire_request(&config, interactive_scope(2)), &budget)
        .await
        .expect_err("a scope of 2 cannot pay for a pool of 10 plus a control connection");

    match error {
        ResourceError::BudgetDenied { requested, reason } => {
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
    assert!(
        ledger.acquired().is_empty(),
        "a refused acquire must not have charged the budget: {:?}",
        ledger.acquired()
    );
    assert!(
        ledger.released().is_empty(),
        "a refused acquire must not have released a permit it never took"
    );
}

#[tokio::test]
async fn an_unproven_initialization_step_is_refused_rather_than_replayed_or_ignored() {
    let config = config();
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let ledger = Arc::new(BudgetLedger::default());
    let budget: Arc<dyn BudgetPort> = Arc::clone(&ledger) as Arc<dyn BudgetPort>;

    let request = AcquireResourceRequest {
        baseline: Baseline::new(vec![InitializationRequirement {
            id: "unproven".into(),
            sql: "SET ROLE admin".into(),
            mandatory: true,
            idempotent: true,
        }]),
        ..acquire_request(&config, interactive_scope(32))
    };

    match provider.acquire_resource(&request, &budget).await {
        Err(ResourceError::OperationNotSupported { operation, .. }) => {
            assert_eq!(operation, "acquire_resource")
        }
        other => panic!("an unproven initialization step must be refused, got {other:?}"),
    }
    assert!(ledger.acquired().is_empty());
}

// ---------------------------------------------------------------------------
// 2. Un-migrated paths stay fail-closed.
// ---------------------------------------------------------------------------

#[test]
fn the_factories_this_crate_still_registers_without_a_provider_fail_closed() {
    // questdb and cloudberry share this crate's connection code but have not
    // been migrated. They must keep saying so through the error, not through
    // a `None` a caller can mistake for "nothing to do".
    for driver_id in ["questdb", "cloudberry"] {
        let factory = factory(driver_id);
        assert!(
            factory.resource_provider().is_none(),
            "{driver_id} has not been migrated; claiming a provider would be a lie"
        );
        assert!(
            !factory.resource_capabilities().declares_anything(),
            "{driver_id} must claim no capability until it has a provider"
        );

        match require_resource_provider(factory) {
            Err(ResourceProviderMissing {
                driver_id: reported,
            }) => {
                assert_eq!(
                    reported, driver_id,
                    "the error must name the driver that is missing"
                )
            }
            Ok(_) => panic!("{driver_id} must not resolve a provider it never implemented"),
        }
    }
}

#[tokio::test]
async fn a_handle_this_provider_never_issued_is_refused_on_every_entry_point() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let sink = UnreachableSink::new();

    // Minted by "some other provider": the ownership check must reject it
    // before anything looks at the resource key, and long before the wire.
    let foreign = ResourceHandle::issue("some-other-provider", "not-issued", 1);
    let execution = datazen_driver_api::QueryExecutionId::new("not-running");
    let call = CommandCall::new("query", serde_json::json!({ "sql": "SELECT 1" }));

    let outcomes: Vec<(&str, Result<(), ResourceError>)> = vec![
        (
            "execute_on_resource",
            provider
                .execute_on_resource(&foreign, &execution, &call, &sink)
                .await
                .map(|_| ()),
        ),
        (
            "observe_session",
            provider.observe_session(&foreign).await.map(|_| ()),
        ),
        (
            "change_context",
            provider
                .change_context(&foreign, &target())
                .await
                .map(|_| ()),
        ),
        (
            "begin_transaction",
            provider
                .begin_transaction(&foreign, &transaction_options())
                .await
                .map(|_| ()),
        ),
        (
            "commit_transaction",
            provider.commit_transaction(&foreign).await.map(|_| ()),
        ),
        (
            "rollback_transaction",
            provider.rollback_transaction(&foreign).await.map(|_| ()),
        ),
        (
            "request_cancel",
            provider
                .request_cancel(&foreign, &execution)
                .await
                .map(|_| ()),
        ),
        (
            "reset_resource",
            provider
                .reset_resource(&foreign, &Baseline::new(Vec::new()))
                .await
                .map(|_| ()),
        ),
        (
            "close_resource",
            provider.close_resource(&foreign).await.map(|_| ()),
        ),
    ];

    for (method, outcome) in outcomes {
        match outcome {
            Err(ResourceError::ResourceOwnershipMismatch { expected, actual }) => {
                assert_eq!(
                    expected, POSTGRES_PROVIDER_ID,
                    "{method} must name itself as the owner"
                );
                assert_eq!(actual, "some-other-provider");
            }
            other => panic!("{method} must refuse a handle it never issued, got {other:?}"),
        }
    }
    assert_eq!(sink.written(), 0, "no refused execution may reach the sink");
}

#[tokio::test]
async fn a_handle_from_another_instance_of_this_driver_is_still_refused() {
    // A second provider over a second driver. Same `provider_id`, different
    // runtime epoch: the ownership check alone would let it through, so the
    // epoch is what stops one instance addressing the other's resources.
    let other = PostgresResourceProvider::new(Arc::new(PostgresDriver::new()));
    let handle = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "any-key", other.runtime_epoch());
    assert_ne!(
        handle.runtime_epoch(),
        0,
        "a provider always starts at a non-zero epoch"
    );

    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    match provider.observe_session(&handle).await {
        Err(ResourceError::StaleRuntimeEpoch { .. }) => {}
        other => panic!("a sibling instance's handle must not be honoured, got {other:?}"),
    }

    // And the reverse direction: the issuing instance itself has no record of
    // that key, so a correct epoch still does not buy an observation.
    match other.observe_session(&handle).await {
        Err(ResourceError::InvalidResourceState {
            resource_key,
            operation,
            ..
        }) => {
            assert_eq!(resource_key, "any-key");
            assert_eq!(operation, "observe_session");
        }
        other => panic!("a provider must not observe a resource it never acquired, got {other:?}"),
    }
}

#[tokio::test]
async fn a_handle_from_a_retired_provider_epoch_is_refused_before_the_resource_is_touched() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");

    // Right provider, wrong epoch: a resource key that happens to exist today
    // must not be addressable by a handle minted against an older runtime.
    let stale = ResourceHandle::issue(POSTGRES_PROVIDER_ID, "any-key", 0);
    let error = provider
        .observe_session(&stale)
        .await
        .expect_err("a stale epoch names nothing, not an empty observation");
    assert!(
        matches!(error, ResourceError::StaleRuntimeEpoch { .. }),
        "expected a stale-epoch refusal, got {error:?}"
    );
}

#[tokio::test]
async fn a_cancel_is_never_accepted_for_an_execution_that_is_not_registered() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let foreign = ResourceHandle::issue("some-other-provider", "not-issued", 1);
    let execution = datazen_driver_api::QueryExecutionId::new("never-registered");

    // A cancel must never fall back to a session-wide sweep: with no resource
    // it is refused outright rather than "cancelling something".
    let error = provider
        .request_cancel(&foreign, &execution)
        .await
        .expect_err("no resource means no execution to cancel");
    assert!(
        matches!(error, ResourceError::ResourceOwnershipMismatch { .. }),
        "expected the ownership refusal, got {error:?}"
    );
    // Only a cancel that actually reached the target backend counts, which is
    // what `CancelReceipt` reports and what these two dispositions are not.
    assert!(!CancelDisposition::NotRegistered.is_accepted());
    assert!(!CancelDisposition::AlreadyFinished.is_accepted());
}

/// The live section below needs a real server and is `#[ignore]`d.
mod live {
    use super::*;

    fn env_var(key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|value| !value.is_empty())
    }

    fn live_config() -> Option<ConnectionConfig> {
        let host = env_var("TEST_PG_HOST")?;
        let mut config = config();
        config.host = Some(host);
        if let Some(port) = env_var("TEST_PG_PORT") {
            config.port = port.parse().ok();
        }
        if let Some(user) = env_var("TEST_PG_USER") {
            config.username = Some(user);
        }
        if let Some(database) = env_var("TEST_PG_DATABASE") {
            config.database = Some(database);
        }
        if let Some(password) = env_var("TEST_PG_PASSWORD") {
            config.password = Some(password);
        }
        Some(config)
    }

    #[tokio::test]
    #[ignore = "needs a live PostgreSQL from TEST_PG_* process env"]
    async fn an_acquired_resource_runs_a_query_and_gives_the_budget_back() {
        let config = live_config().expect("TEST_PG_HOST is required for the live run");
        let driver = postgres().create();
        let provider = require_resource_provider(postgres()).expect("postgres has a provider");

        let connection = driver
            .connect(&config)
            .await
            .expect("the live server must accept a connection");

        let ledger = Arc::new(BudgetLedger::default());
        let budget: Arc<dyn BudgetPort> = Arc::clone(&ledger) as Arc<dyn BudgetPort>;

        let handle = provider
            .acquire_resource(&acquire_request(&config, interactive_scope(32)), &budget)
            .await
            .expect("a scope of 32 pays for the pool of 10 plus the control connection");
        assert_eq!(ledger.acquired().len(), 1, "exactly one charge per acquire");

        let execution = datazen_driver_api::QueryExecutionId::new("live-1");
        let sink = UnreachableSink::new();
        let completion = provider
            .execute_on_resource(
                &handle,
                &execution,
                &CommandCall::new("query", serde_json::json!({ "sql": "SELECT 1" })),
                &sink,
            )
            .await
            .expect("a plain SELECT must run on a live resource");

        assert!(
            completion.completion_status.is_success(),
            "a plain SELECT completes: {:?}",
            completion.completion_status
        );

        let close = provider
            .close_resource(&handle)
            .await
            .expect("close must succeed on a live resource");
        assert_eq!(
            close,
            CloseDisposition::Closed,
            "close must report a confirmed close, not an unconfirmed one"
        );
        assert_eq!(
            ledger.released().len(),
            1,
            "exactly one release per acquire"
        );

        let _ = connection;
    }
}
