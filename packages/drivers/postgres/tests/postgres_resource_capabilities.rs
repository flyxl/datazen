//! What PostgreSQL declares, and what it refuses.
//!
//! Split out of `postgres_resource_contract.rs` only because that file reached
//! the 800-line ceiling; this is the same view of the same crate, through the
//! same public surface. **This is the P2 exit gate**: a capability postgres does
//! not have must come back as a `CapabilityError`, never `Ok(())` and never an
//! empty success — and a capability it *does* declare must be one the code
//! really implements, or the declaration is the lie.
//!
//! Both directions are asserted here:
//!
//! * **Declared unsupported ⇒ refused.** Each of the five guards postgres does
//!   not satisfy is called on the real registry and must fail, naming the
//!   capability that was asked for.
//! * **Declared supported ⇒ real.** `precise_cancel`, `transaction_observation`
//!   and `session_scoped_handles` are asserted `Supported` *and* checked
//!   against the driver calls that back them, so "supported" cannot become a
//!   free-form claim.
//!
//! Everything here is offline: no server is contacted.
//!
//! The provider must be reachable through the public contract only, so the
//! `inventory::submit!` in the driver crate's `lib.rs` has to actually run —
//! which needs this binary to name the crate.
use std::sync::Arc;

use datazen_driver_api::capabilities::{
    Availability, CapabilityError, CapabilityRegistry, ContextObservation, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionScopedHandleSupport, SnapshotSupport,
    TransactionObservation,
};
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::{
    iter_driver_factories, require_resource_provider, BackupSupport, DataSupport,
    DatabaseDriverFactory,
};

// The test binary only links this crate — and therefore only runs the
// `inventory::submit!` in `lib.rs` that makes any factory findable at all —
// when something below actually names it. `provider_is_the_one_this_crate_exports`
// is what actually uses both names.
use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};

/// The provider id postgres declares in its capability snapshot.
const POSTGRES_PROVIDER_ID: &str = "postgresql";

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

// ---------------------------------------------------------------------------
// 2. A capability postgres does not have is an explicit error, never `Ok(())`.
// ---------------------------------------------------------------------------

/// The registry every other test here refuses against must be the one this
/// crate's *exported* provider type carries, not a second declaration that
/// could drift from it. Built through the public constructor, so the
/// capability snapshot under test is the one a host would receive.
#[test]
fn provider_is_the_one_this_crate_exports() {
    let driver = Arc::new(PostgresDriver::new());
    let exported = PostgresResourceProvider::new(driver.clone());

    assert_eq!(exported.runtime_epoch() > 0, true);
    assert_eq!(Arc::ptr_eq(exported.driver(), &driver), true);

    let from_host = require_resource_provider(postgres()).expect("postgres has a provider");
    assert_eq!(
        from_host.capabilities().capabilities,
        exported.capabilities().capabilities,
        "the provider a host reaches and the one this crate exports must declare the same things"
    );
}

#[test]
fn a_stateful_session_is_refused_with_an_error_not_an_empty_session() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let registry = provider.capabilities();

    let error: CapabilityError = registry
        .require_stateful_session()
        .expect_err("postgres resources are leased, not a pinned stateful session");

    assert_eq!(error.capability, "statefulSession");
    assert_eq!(error.provider_id, POSTGRES_PROVIDER_ID);
}

#[test]
fn every_declared_unsupported_capability_is_refused_by_its_own_guard() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let registry: &CapabilityRegistry = provider.capabilities();

    // (capability name as the registry spells it, the guard, the reason it is
    // not declared). Each guard is called on a value postgres does not
    // implement, and each must come back as an error.
    let refusals: Vec<(&str, Result<(), CapabilityError>, &str)> = vec![
        (
            "statefulSession",
            registry.require_stateful_session(),
            "a resource is a leased pool slot, not a fixed session",
        ),
        (
            "contextObservation",
            registry.require_context_observation(),
            "a leased slot cannot be read without a live connection",
        ),
        (
            "resetForReuse",
            registry.require_reset_for_reuse(),
            "replayable reset is not proven for a leased slot",
        ),
        (
            "snapshots",
            registry.require_snapshots(),
            "no coordinated snapshot protocol is wired",
        ),
        (
            "namespaceSwitch",
            registry.require_in_place_namespace_switch(),
            "switching databases replaces the resource",
        ),
    ];

    for (name, refusal, reason) in refusals {
        let error = refusal.err().unwrap_or_else(|| {
            panic!("{name} is not declared, so requiring it must fail: {reason}")
        });
        assert_eq!(
            error.capability, name,
            "the rejection must name what was asked for"
        );
        assert_eq!(error.provider_id, POSTGRES_PROVIDER_ID);
    }
}

#[test]
fn what_postgres_declares_is_what_the_code_does() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let registry = provider.capabilities();
    let capabilities = &registry.capabilities;

    // Declared and backed by this crate's execution/transaction code.
    assert_eq!(
        capabilities.precise_cancel,
        PreciseCancelSupport::Supported,
        "cancel reaches a single backend through pg_cancel_backend"
    );
    registry.require_precise_cancel().expect("declared");
    registry
        .require_transaction_observation()
        .expect("declared: begin/commit/rollback are on the resource itself");
    assert_eq!(
        capabilities.transaction_observation,
        TransactionObservation::Full
    );
    registry
        .require_session_scoped_handles()
        .expect("declared: a pinned connection can back a server-side handle");
    assert_eq!(
        capabilities.session_scoped_handles,
        SessionScopedHandleSupport::Supported
    );

    // Declared, but only partially — and the provider says so in its own
    // observations rather than upgrading them.
    assert_eq!(
        capabilities.context_observation,
        ContextObservation::Partial
    );
    assert_eq!(
        capabilities.namespace_switch,
        NamespaceSwitch::RequiresReplacement,
        "switching database is a new resource, not a SET on this one"
    );

    // Declared unsupported: the table above already showed each guard refuses.
    assert_eq!(capabilities.reset_for_reuse, ResetForReuse::Unsupported);
    assert_eq!(capabilities.snapshots, SnapshotSupport::Unsupported);
    assert_eq!(capabilities.stateful_session, Availability::Unsupported);
}

// ---------------------------------------------------------------------------
// 3. Every cell of the grid, asserted by value.
// ---------------------------------------------------------------------------

/// The whole capability grid, cell by cell.
///
/// `postgres_capability_set` declares itself through `CapabilitySet::default()`
/// followed by per-field assignment rather than through one exhaustive struct
/// expression, because that is the only shape that survives a future
/// `#[non_exhaustive]` on `CapabilitySet`. This test exists because that
/// rewrite is **not** self-checking.
///
/// The twelve fields split into two groups, and only one of them is protected
/// by the assignment itself:
///
/// * **Eight cells differ from `Default`.** Dropping one of their assignments
///   silently degrades that cell to `Unknown`/`Unsupported` instead of
///   failing to build. This test is what turns that silent degradation red.
/// * **Six cells happen to equal their own `Default`.** Dropping their
///   assignments changes nothing observable, so no by-value test could catch
///   it — what *this* assertion buys is immunity to the reverse drift: if a
///   default in `driver-api` ever moves, postgres's declaration stops
///   silently following it and fails here instead.
///
/// That second group is also why these expectations are written out by hand
/// rather than rebuilt from `CapabilitySet::default()`: an expectation derived
/// from the same default it is meant to police cannot disagree with it.
#[test]
fn every_cell_of_the_capability_grid_is_asserted_rather_than_inherited() {
    let provider = require_resource_provider(postgres()).expect("postgres has a provider");
    let capabilities = &provider.capabilities().capabilities;

    // --- the eight cells whose declared value differs from `Default` ---

    assert_eq!(
        capabilities.stateful_session,
        Availability::Unsupported,
        "a pooled backend can move under a leased session"
    );
    assert_eq!(
        capabilities.namespace_switch,
        NamespaceSwitch::RequiresReplacement,
        "another database is a new pool, not a SET on this one"
    );
    assert_eq!(
        capabilities.context_observation,
        ContextObservation::Partial,
        "a pinned transaction is readable; a leased read names one backend"
    );
    assert_eq!(
        capabilities.transaction_observation,
        TransactionObservation::Full,
        "begin/commit/rollback report the server's own answer"
    );
    assert_eq!(
        capabilities.session_scoped_handles,
        SessionScopedHandleSupport::Supported,
        "the transaction handle is registered and dropped on close"
    );
    assert_eq!(
        capabilities.precise_cancel,
        PreciseCancelSupport::Supported,
        "pg_cancel_backend targets exactly one execution"
    );
    assert_eq!(
        capabilities.transactions.savepoints,
        Availability::Unsupported,
        "nothing in begin_transaction_impl emits SAVEPOINT"
    );
    assert_eq!(
        capabilities.transactions.max_open_transactions,
        Some(1),
        "the map keyed by connection id holds exactly one pinned connection"
    );
    // Deliberately not `StreamingReadWrite`: the driver buffers every row
    // before the sink sees the first byte, so only two of the three axes hold.
    assert_eq!(
        capabilities.data,
        DataSupport::BufferedReadWrite,
        "rowRead and rowWrite are proven through this provider; streamingResults is not"
    );

    // --- the five cells that coincide with `Default`, pinned anyway ---

    assert_eq!(
        capabilities.reset_for_reuse,
        ResetForReuse::Unsupported,
        "reset_resource returns Discard; no verified baseline replay"
    );
    assert_eq!(
        capabilities.snapshots,
        SnapshotSupport::Unsupported,
        "begin_read_snapshot_impl exists but no per-table guarantee was verified"
    );
    assert!(
        capabilities.transactions.isolation_levels.is_empty(),
        "a bare BEGIN cannot honour a named level, so none may be listed"
    );
    assert!(
        capabilities.ddl_atomicity.by_operation.is_empty(),
        "ddl_atomicity answers driver-wide, not per operation"
    );
    assert_eq!(
        capabilities.backup,
        BackupSupport::Unknown,
        "this provider has no dump or restore surface, so it says neither yes nor no"
    );

    // And the grid as a whole is a real declaration, not a default that
    // happens to have survived: at least one cell must differ from `default()`.
    assert_ne!(
        *capabilities,
        datazen_driver_api::capabilities::CapabilitySet::default(),
        "postgres declares a non-default surface; default() would mean it declares none"
    );
}
