//! Behavioural unit tests for the Redis resource provider.
//!
//! Split out of `tests.rs` purely for file size: that file owns the doubles, the
//! fixtures and the declaration arithmetic — what the provider *claims*. This
//! one owns what it *does* once a resource exists: refusing a handle it never
//! issued, refusing transactions it cannot honour, spending and returning the
//! budget, and refusing to invent a context switch or an observation it did not
//! make.
//!
//! Still fully offline, and with the same honesty about it: seeds go through
//! the same `register_resource` the acquire path calls, with only the socket
//! skipped. So every method that must talk to the server fails here, and each
//! of those failures is asserted to be an `Err` with a reason — never a
//! plausible-looking `Ok`.

use std::sync::Arc;

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    Baseline, ConnectionCostPolicy, InitializationRequirement, ResourceError, ResourceHandle,
    ResourceProvider,
};
use datazen_driver_api::session::{CloseDisposition, ResetDisposition};
use datazen_driver_api::{require_resource_provider, DatabaseDriverFactory, DriverError};

use crate::driver::RedisDriver;
use crate::{RedisFactory, RedisResourceProvider};

use super::capabilities::{redis_connection_cost, REDIS_PROVIDER_ID};
use super::tests::{
    command, execution_id, foreign_handle, interactive_scope, provider, seed, target,
    transaction_options, BudgetLedger, RecordingSink,
};

/// `ResourceError` wraps a `DriverError` and is deliberately not `PartialEq`, so
/// a disposition is compared by shape. The message carries the outcome, so a
/// failure names the actual value instead of a type mismatch.
fn assert_disposition<T: std::fmt::Debug + PartialEq>(
    outcome: Result<T, ResourceError>,
    expected: T,
    context: &str,
) {
    assert!(
        matches!(&outcome, Ok(actual) if *actual == expected),
        "{context}: expected {expected:?}, got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// Ownership: a handle this provider did not issue
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_handle_taking_method_rejects_a_foreign_handle() {
    // The core assertion of this whole track, expressed as one table: none of
    // the nine stateful methods that take a handle may answer `Ok` for a handle
    // this provider does not own. A silent success here is the exact defect
    // shape the contract exists to prevent.
    //
    // `describe_resource` and `acquire_resource` are absent on purpose: they
    // take no handle, so there is no ownership question to ask. `acquire`
    // cannot be reached with somebody else's handle — that is what *makes* a
    // resource.
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = foreign_handle();
    let sink = RecordingSink::default();

    let outcomes: Vec<(&str, Result<(), ResourceError>)> = vec![
        (
            "execute_on_resource",
            provider
                .execute_on_resource(&handle, &execution_id(), &command(), &sink)
                .await
                .map(drop),
        ),
        (
            "observe_session",
            provider.observe_session(&handle).await.map(drop),
        ),
        (
            "change_context",
            provider.change_context(&handle, &target()).await.map(drop),
        ),
        (
            "begin_transaction",
            provider
                .begin_transaction(&handle, &transaction_options())
                .await
                .map(drop),
        ),
        (
            "commit_transaction",
            provider.commit_transaction(&handle).await.map(drop),
        ),
        (
            "rollback_transaction",
            provider.rollback_transaction(&handle).await.map(drop),
        ),
        (
            "request_cancel",
            provider
                .request_cancel(&handle, &execution_id())
                .await
                .map(drop),
        ),
        (
            "reset_resource",
            provider
                .reset_resource(&handle, &Baseline::new(Vec::new()))
                .await
                .map(drop),
        ),
        (
            "close_resource",
            provider.close_resource(&handle).await.map(drop),
        ),
    ];

    assert_eq!(
        outcomes.len(),
        9,
        "the contract has eleven stateful methods; two take no handle, so nine do. If a method \
         was added, add it to this table."
    );

    for (method, outcome) in outcomes {
        assert!(
            matches!(
                outcome,
                Err(ResourceError::ResourceOwnershipMismatch { .. })
                    | Err(ResourceError::StaleRuntimeEpoch { .. })
            ),
            "{method} did not reject a handle from another provider; it answered {outcome:?}"
        );
    }
    assert!(
        ledger.released().is_empty(),
        "a refused close must not release a permit"
    );
}

/// Ownership is checked before the record is looked up, and the two failures
/// are told apart: a handle minted by a *different* provider is a mismatch, a
/// handle minted by *this* provider at an old epoch is a stale epoch. Reporting
/// both as "unknown key" would tell a caller nothing about which mistake it made.
#[tokio::test]
async fn ownership_and_epoch_are_reported_as_two_different_failures() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());

    let someone_elses = ResourceHandle::issue("postgres", "redis_test", 1);
    assert!(
        matches!(
            provider.observe_session(&someone_elses).await,
            Err(ResourceError::ResourceOwnershipMismatch { ref expected, ref actual })
                if expected == REDIS_PROVIDER_ID && actual == "postgres"
        ),
        "a handle from another provider must name both sides"
    );

    // Same provider id, an epoch this provider never issued. It is *this*
    // provider's handle, just not from this runtime.
    let our_key_but_old_runtime = ResourceHandle::issue(
        REDIS_PROVIDER_ID,
        "redis_test",
        provider.runtime_epoch() + 1,
    );
    assert!(
        matches!(
            provider.observe_session(&our_key_but_old_runtime).await,
            Err(ResourceError::StaleRuntimeEpoch { .. })
        ),
        "a handle from a previous runtime must be reported as stale, not as somebody else's"
    );

    // And a key this provider genuinely does not hold, at the right epoch.
    let unknown = ResourceHandle::issue(
        REDIS_PROVIDER_ID,
        "never_acquired",
        provider.runtime_epoch(),
    );
    assert!(
        matches!(
            provider.observe_session(&unknown).await,
            Err(ResourceError::InvalidResourceState { ref state, .. })
                if state.contains("never acquired")
        ),
        "an unknown key must say the resource was never acquired here"
    );

    assert!(ledger.released().is_empty());
}

// ---------------------------------------------------------------------------
// Transactions, cancel, reset
// ---------------------------------------------------------------------------

#[tokio::test]
async fn begin_transaction_is_refused_not_ignored() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    let error = provider
        .begin_transaction(&handle, &transaction_options())
        .await
        .expect_err("MULTI/EXEC is a command batch, not a transaction");

    assert!(matches!(
        error,
        ResourceError::OperationNotSupported { operation, .. } if operation == "begin_transaction"
    ));
}

#[tokio::test]
async fn begin_transaction_quotes_the_isolation_level_it_cannot_honour() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);
    let mut options = transaction_options();
    options.isolation_level = Some("serializable".into());

    let error = provider
        .begin_transaction(&handle, &options)
        .await
        .expect_err("Redis has no isolation levels to honour");

    assert!(
        matches!(&error, ResourceError::OperationNotSupported { ref reason, .. }
            if reason.contains("serializable")),
        "the refused option must be named, not dropped in silence: {error:?}"
    );
}

#[tokio::test]
async fn commit_and_rollback_are_refused_rather_than_acknowledged() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    for operation in ["commit_transaction", "rollback_transaction"] {
        let outcome = match operation {
            "commit_transaction" => provider.commit_transaction(&handle).await.map(|_| ()),
            _ => provider.rollback_transaction(&handle).await.map(|_| ()),
        };
        assert!(
            matches!(&outcome, Err(ResourceError::OperationNotSupported { operation: op, .. })
                if op == operation),
            "{operation} returned {outcome:?}; there was no transaction to finish"
        );
    }
}

#[tokio::test]
async fn request_cancel_is_refused_rather_than_acknowledged() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    let error = provider
        .request_cancel(&handle, &execution_id())
        .await
        .expect_err("Redis has no per-execution cancel");

    assert!(matches!(
        error,
        ResourceError::OperationNotSupported { operation, .. } if operation == "request_cancel"
    ));
}

#[tokio::test]
async fn reset_without_a_baseline_discards_and_leaves_the_session_alone() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    let disposition = provider
        .reset_resource(&handle, &Baseline::new(Vec::new()))
        .await
        .expect("an empty baseline has nothing to replay");

    assert_disposition(
        Ok(disposition),
        ResetDisposition::Discard,
        "nothing was replayed and nothing was verified clean, so `Clean` would be a lie",
    );
    // The handle survives a reset, and the budget is untouched — a reset is not
    // an acquire and not a release.
    let after = provider
        .begin_transaction(&handle, &transaction_options())
        .await;
    assert!(
        matches!(&after, Err(ResourceError::OperationNotSupported { .. })),
        "the handle must still be owned by this provider after a reset, got {after:?}"
    );
    assert!(ledger.acquired().is_empty());
    assert!(ledger.released().is_empty());
}

#[tokio::test]
async fn reset_with_a_baseline_is_refused_and_leaves_the_handle_usable() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);
    let baseline = Baseline::new(vec![InitializationRequirement {
        id: "warm".into(),
        sql: "SET a 1".into(),
        mandatory: false,
        idempotent: true,
    }]);

    let error = provider
        .reset_resource(&handle, &baseline)
        .await
        .expect_err("there is no verified way to replay a baseline for this driver");

    assert!(matches!(
        error,
        ResourceError::OperationNotSupported { operation, .. } if operation == "reset_resource"
    ));

    // The refusal must not have consumed the resource: `FLUSHDB` was never
    // reached, so the session is still ours to close.
    assert!(
        matches!(
            provider.close_resource(&handle).await,
            Ok(CloseDisposition::Closed)
        ),
        "close must report a confirmed close: {:?}",
        provider.close_resource(&handle).await
    );
}

// ---------------------------------------------------------------------------
// Commands and observation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_command_on_a_session_with_no_connection_fails_the_sink_instead_of_completing_it() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(&handle, &execution_id(), &command(), &sink)
        .await
        .expect_err("there is no connection behind this handle");

    assert!(matches!(error, ResourceError::Driver(_)), "got {error:?}");
    assert_eq!(sink.rows_written(), 0);
    assert!(
        sink.failure().is_some(),
        "the sink must learn the execution ended"
    );
    assert!(
        !sink.was_completed(),
        "a failed execution must not also report a clean completion"
    );
}

#[tokio::test]
async fn observation_of_a_dead_session_is_an_error_not_a_default() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    let error = provider
        .observe_session(&handle)
        .await
        .expect_err("a session that cannot answer INFO server is not Ready");

    assert!(matches!(error, ResourceError::Driver(_)), "got {error:?}");
}

// ---------------------------------------------------------------------------
// change_context
// ---------------------------------------------------------------------------

#[tokio::test]
async fn change_context_refuses_a_target_that_is_not_a_logical_database() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    let error = provider
        .change_context(
            &handle,
            &NamespaceTarget::empty().with_database("production"),
        )
        .await
        .expect_err("Redis logical databases are numeric");

    assert!(
        matches!(&error, ResourceError::NamespaceTargetRejected { ref reason }
            if reason.contains("db3")),
        "the refusal must name the database the session is actually on: {error:?}"
    );
}

#[tokio::test]
async fn change_context_really_issues_a_select_instead_of_returning_confirmed() {
    // The seed has no live connection, so a `SELECT` cannot succeed. If
    // `change_context` ever short-circuits to `Ok(Confirmed)` without talking to
    // the server, this test goes red — which is the whole point: the disposition
    // must be earned by a round trip, never asserted.
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    let outcome = provider
        .change_context(&handle, &NamespaceTarget::empty().with_database("db5"))
        .await;

    assert!(
        outcome.is_err(),
        "Confirmed must require a real SELECT: got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// close_resource and the budget
// ---------------------------------------------------------------------------

#[tokio::test]
async fn close_releases_the_budget_exactly_once() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    assert_disposition(
        provider.close_resource(&handle).await,
        CloseDisposition::Closed,
        "removing the connection from the driver registry cannot fail after the remove",
    );
    assert_eq!(ledger.released(), vec!["permit-1".to_string()]);
}

#[tokio::test]
async fn close_is_idempotent_and_does_not_release_twice() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());
    let handle = seed(&provider, &ledger);

    assert_disposition(
        provider.close_resource(&handle).await,
        CloseDisposition::Closed,
        "the first close is the real one",
    );
    assert_disposition(
        provider.close_resource(&handle).await,
        CloseDisposition::Closed,
        "closing an already-closed resource is a no-op, not an error",
    );
    assert_eq!(
        ledger.released(),
        vec!["permit-1".to_string()],
        "a second close must not return a permit that was already returned"
    );
}

#[tokio::test]
async fn closing_a_never_held_handle_is_an_error() {
    let provider = provider();
    let ledger = Arc::new(BudgetLedger::default());

    // A handle with this provider's id and epoch, but never registered.
    let handle =
        ResourceHandle::issue(REDIS_PROVIDER_ID, "redis_unknown", provider.runtime_epoch());

    let error = provider
        .close_resource(&handle)
        .await
        .expect_err("there is nothing to close");

    assert!(
        matches!(error, ResourceError::InvalidResourceState { .. }),
        "got {error:?}"
    );
    assert!(ledger.released().is_empty());
}

// ---------------------------------------------------------------------------
// Factory wiring
// ---------------------------------------------------------------------------

#[test]
fn the_factory_reaches_the_provider_and_agrees_with_it_on_capabilities() {
    let factory = RedisFactory;

    let provider = factory
        .resource_provider()
        .expect("redis implements the resource contract");
    assert_eq!(provider.provider_id(), REDIS_PROVIDER_ID);
    assert_eq!(
        factory.resource_capabilities(),
        provider.capabilities().capabilities,
        "the factory view and the provider view must not drift"
    );
    assert_ne!(factory.resource_capabilities(), Default::default());
}

#[test]
fn require_resource_provider_resolves_instead_of_swallowing() {
    let factory = RedisFactory;
    let resolved = require_resource_provider(&factory);
    assert!(
        resolved.is_ok(),
        "the resolver must hand back a provider, not a swallowed None"
    );
}

#[test]
fn the_provider_is_memoized_so_live_handles_survive_a_second_lookup() {
    let first = crate::resource_provider();
    let second = crate::resource_provider();

    assert!(
        Arc::ptr_eq(&first, &second),
        "the process must not mint a second provider"
    );
    assert_eq!(
        first.runtime_epoch(),
        second.runtime_epoch(),
        "a new provider instance would take the next epoch and invalidate every live handle"
    );
    // The stronger half. `RedisDriver::connect` mints the `pool_id` itself and
    // writes the connection into `RedisDriver::connections`
    // (`driver/mod.rs:114-116`); every later command resolves the socket by
    // `handle.pool_id` and nothing else. A second `RedisDriver` would therefore
    // be a *different* map, and every handle the first provider issued would
    // point at a key the second one does not have — a live session that fails
    // with "no such connection" the moment it is looked up again. One provider
    // and one driver, forever.
    assert!(
        Arc::ptr_eq(first.driver(), second.driver()),
        "both lookups must resolve to the same driver, or a second lookup invalidates every \
         handle the first one issued"
    );
}

#[test]
fn the_provider_shares_the_arc_the_factory_hands_out() {
    // A second `RedisDriver` would be a driver with no connections at all,
    // because `RedisDriver::connect` writes into `RedisDriver::connections` and
    // every later command resolves by `handle.pool_id`.
    let driver = crate::shared_driver();
    let provider = RedisResourceProvider::new(Arc::clone(&driver));
    assert!(Arc::ptr_eq(provider.driver(), &driver));
}

#[test]
fn a_fresh_provider_takes_the_next_epoch() {
    let first = RedisResourceProvider::new(Arc::new(RedisDriver::new()));
    let second = RedisResourceProvider::new(Arc::new(RedisDriver::new()));
    assert!(
        second.runtime_epoch() > first.runtime_epoch(),
        "an older instance's handles must fail the epoch check"
    );
}

// ---------------------------------------------------------------------------
// Namespace shape helpers
// ---------------------------------------------------------------------------

#[test]
fn the_declared_cost_is_the_same_number_the_budget_is_charged() {
    // `redis_connection_cost` is what `describe_resource` promises; if the
    // acquire path ever charged a different number, this would drift silently.
    let ConnectionCostPolicy::PoolBounded {
        max_physical_connections: required,
    } = redis_connection_cost()
    else {
        panic!("Redis must declare a bounded cost, or the budget cannot be enforced");
    };
    assert_eq!(required, 1);
    assert!(
        interactive_scope().max_physical_connections >= required,
        "the fixture scope must be able to afford the declared cost"
    );
}

#[test]
fn a_driver_error_from_an_absent_connection_is_a_driver_error() {
    // Guards the error type the contract promises at `execute_on_resource` when
    // the driver itself fails, as opposed to a contract refusal.
    let error = ResourceError::Driver(DriverError::QueryFailed("synthetic".into()));
    assert!(matches!(error, ResourceError::Driver(_)));
}
