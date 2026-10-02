//! The resource lifecycle: describe, acquire, execute, observe, close.
//!
//! These tests hold the provider to the parts of the contract where a mistake
//! costs a caller something real: a charge that is taken twice, a charge that is
//! never returned, a refused request that still went to the wire, a health
//! reading that claims more than one round trip proved, or a handle from
//! another instance being accepted as if it belonged to this one.

use std::sync::Arc;

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    Baseline, ConnectionCostPolicy, InitializationRequirement, ResourceError, ResourceHandle,
    ResourceProvider, ResourcePurpose, ResourceScope, ReusePolicy,
};
use datazen_driver_api::session::{
    CloseDisposition, ObservationConfidence, ResetDisposition, ResourceHealth, SessionState,
    TransactionState,
};
use datazen_driver_api::DatabaseDriver;

use crate::resource::capabilities::HBASE_PROVIDER_ID;

use super::support::{
    acquire_request, baseline, config, config_for, config_without_endpoint, describe_request,
    execution_id, provider, refusal_reason, scan_call, scope, stargate_answering,
    stargate_failing_scans, stargate_probe_answers_wrongly, unacquired_handle, BudgetLedger,
    RecordingSink,
};

// ---------------------------------------------------------------------------
// describe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn describing_a_resource_is_free_and_claims_nothing_it_cannot_keep() {
    let provider = provider();
    let config = config();
    let descriptor = provider
        .describe_resource(&describe_request(&config))
        .await
        .expect("describing must not need a server");

    assert_eq!(descriptor.provider_id, HBASE_PROVIDER_ID);
    assert_eq!(descriptor.resource_key, "hbase_resource:hbase-cfg");
    // A pool of one HTTP client is not a session; `is_fixed_session` is the
    // check that keeps a size-one client from passing as one.
    assert!(!descriptor.is_fixed_session());
    // This driver speaks no SQL, so it can neither issue nor replay an
    // initialization step, and a "replay this before reuse" promise is one it
    // could not keep.
    assert!(descriptor.initialization_requirements.is_empty());
    assert_eq!(descriptor.reuse_policy, ReusePolicy::SingleUse);
    assert_eq!(
        descriptor.connection_cost_policy,
        ConnectionCostPolicy::PoolBounded {
            max_physical_connections: 1
        }
    );
    // Describing opened nothing, so nothing is charged and nothing is held.
    let handle = unacquired_handle(&provider, &descriptor.resource_key);
    assert!(
        provider.close_resource(&handle).await.is_err(),
        "describe must not have created a resource behind the caller's back"
    );
}

#[tokio::test]
async fn describing_refuses_a_level_this_driver_does_not_have() {
    let provider = provider();
    let request = datazen_driver_api::resource::DescribeResourceRequest {
        target: NamespaceTarget {
            database: Some("default".into()),
            ..describe_request(&config()).target
        },
        ..describe_request(&config())
    };
    let error = provider
        .describe_resource(&request)
        .await
        .expect_err("a Stargate endpoint has no database level to name");
    assert!(matches!(
        error,
        ResourceError::NonexistentNamespaceLevel { .. }
    ));
}

// ---------------------------------------------------------------------------
// acquire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn acquiring_charges_one_connection_and_registers_exactly_one_resource() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let config = config_for(&server);

    let handle = provider
        .acquire_resource(&acquire_request(&config), &budget.as_port())
        .await
        .expect("connect builds a client and registers it without a round trip");

    // The key is derived from the connection config, so two acquires of the same
    // connection are the same resource and one acquires is one charge.
    assert_eq!(handle.resource_key(), "hbase_resource:hbase-cfg");
    assert_eq!(budget.requested(), vec![1]);
    assert_eq!(budget.granted(), vec![1]);
    assert!(
        budget.released().is_empty(),
        "nothing is released while the resource is live"
    );

    // The permit is not returned early: the resource is still usable.
    let sink = RecordingSink::default();
    let completion = provider
        .execute_on_resource(&handle, &execution_id(), &scan_call(), &sink)
        .await
        .expect("the mounted scanner answers");
    assert_eq!(completion.statement_results.len(), 1);
    assert_eq!(budget.released(), Vec::<String>::new());
}

#[tokio::test]
async fn a_refused_budget_stops_acquisition_before_any_client_is_built() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    budget.deny(true);

    let error = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect_err("a denied budget cannot be spent around");
    assert!(matches!(error, ResourceError::BudgetDenied { .. }));
    assert_eq!(budget.requested(), vec![1]);
    assert_eq!(
        budget.granted(),
        Vec::<u32>::new(),
        "a refusal must not be reported as a grant"
    );
    assert_eq!(
        budget.released(),
        Vec::<String>::new(),
        "nothing was granted, so nothing may be released"
    );
}

#[tokio::test]
async fn a_scope_narrower_than_the_declared_cost_is_refused() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let request = datazen_driver_api::resource::AcquireResourceRequest {
        // One acquire is one client, so a scope that allows zero of them cannot
        // be served — and the cost is reported, not quietly clamped.
        scope: ResourceScope {
            max_physical_connections: 0,
            ..scope()
        },
        ..acquire_request(&config())
    };

    let error = provider
        .acquire_resource(&request, &budget.as_port())
        .await
        .expect_err("a scope of 0 cannot be served by a cost of 1");
    assert!(matches!(
        error,
        ResourceError::BudgetDenied { requested: 1, .. }
    ));
    assert_eq!(budget.granted(), Vec::<u32>::new());
}

#[tokio::test]
async fn a_scope_wider_than_the_declared_cost_is_honoured_as_an_upper_bound() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let request = datazen_driver_api::resource::AcquireResourceRequest {
        // A scope is a ceiling, not a target: 8 allowed still costs 1, because
        // `PoolBounded { 1 }` is what one acquire creates.
        scope: ResourceScope {
            max_physical_connections: 8,
            ..scope()
        },
        ..acquire_request(&config_for(&server))
    };

    provider
        .acquire_resource(&request, &budget.as_port())
        .await
        .expect("a wider ceiling does not make the resource unavailable");
    assert_eq!(budget.granted(), vec![1]);
}

#[tokio::test]
async fn a_failed_connect_returns_the_charge_it_already_took() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());

    // No host at all: `base_url` cannot be built, so `HBaseDriver::connect`
    // fails after the permit was granted.
    let error = provider
        .acquire_resource(
            &acquire_request(&config_without_endpoint()),
            &budget.as_port(),
        )
        .await
        .expect_err("a config with no endpoint cannot connect");
    assert!(matches!(error, ResourceError::Driver(_)));
    assert_eq!(budget.granted(), vec![1]);
    assert_eq!(
        budget.released(),
        vec!["1".to_string()],
        "a charge taken for a connect that never happened must be handed back"
    );
}

#[tokio::test]
async fn a_baseline_this_driver_cannot_replay_is_refused() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let request = datazen_driver_api::resource::AcquireResourceRequest {
        baseline: Baseline {
            initialization_requirements: vec![InitializationRequirement {
                id: "session-preamble".to_string(),
                sql: "set x = 1".to_string(),
                mandatory: true,
                idempotent: false,
            }],
        },
        ..acquire_request(&config())
    };

    let error = provider
        .acquire_resource(&request, &budget.as_port())
        .await
        .expect_err("a provider that declares no initialization cannot accept one");
    assert!(matches!(error, ResourceError::OperationNotSupported { .. }));
    assert_eq!(
        budget.granted(),
        Vec::<u32>::new(),
        "a refused request must not be charged"
    );
}

// ---------------------------------------------------------------------------
// execute
// ---------------------------------------------------------------------------

#[tokio::test]
async fn executing_a_scan_streams_the_rows_and_reports_only_what_it_proved() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the stub answers, so the scan can run");
    let sink = RecordingSink::default();

    let completion = provider
        .execute_on_resource(&handle, &execution_id(), &scan_call(), &sink)
        .await
        .expect("the scan succeeds against the mounted scanner");

    assert_eq!(completion.statement_results.len(), 1);
    assert_eq!(
        sink.row_count(),
        2,
        "both cells the scanner returned must be streamed"
    );
    assert!(sink.completed(), "a finished execution ends the sink");
    // The client will not be read again for this execution.
    assert!(completion.protocol_drained);
    assert_eq!(completion.resource_health, ResourceHealth::Healthy);
    // Nothing was observed before or after: echoing the caller's own target
    // back would be the provider agreeing with itself.
    assert_eq!(
        completion.context_after.confidence,
        ObservationConfidence::Unknown
    );
    assert_eq!(
        completion.transaction_observation.state,
        TransactionState::Unsupported
    );
    assert!(
        completion.session_handles.is_empty(),
        "a scanner is opened, read and deleted inside one command, so none outlives it"
    );
}

#[tokio::test]
async fn a_failing_scan_ends_the_execution_and_surfaces_the_driver_error() {
    let server = stargate_failing_scans().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the server is reachable");
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(&handle, &execution_id(), &scan_call(), &sink)
        .await
        .expect_err("a 500 is not a successful scan");
    assert!(matches!(error, ResourceError::Driver(_)));
    assert!(
        !sink.completed(),
        "an execution that failed must not report completion"
    );
    assert!(
        sink.failed().is_some(),
        "the consumer is told the execution ended, not left waiting for rows"
    );
    assert_eq!(sink.row_count(), 0);
}

#[tokio::test]
async fn a_sink_that_refuses_rows_is_reported_as_a_delivery_failure() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the server is reachable");
    let sink = RecordingSink::default();
    sink.reject();

    let error = provider
        .execute_on_resource(&handle, &execution_id(), &scan_call(), &sink)
        .await
        .expect_err("rows nobody accepted are not a successful execution");
    assert!(matches!(error, ResourceError::SinkRejected { .. }));
    assert!(sink.chunks().is_empty());
}

/// A catalog command answers with a `CommandResult`, not a result set. Reporting
/// it as "zero rows" would be a successful no-op, which is the failure this
/// decoder exists to prevent.
#[tokio::test]
async fn a_catalog_command_is_refused_by_the_shape_that_arrived() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the server is reachable");
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(
            &handle,
            &execution_id(),
            &super::support::catalog_call("list_databases"),
            &sink,
        )
        .await
        .expect_err("a command result is not a result set");
    assert!(matches!(error, ResourceError::OperationNotSupported { .. }));
    let reason = refusal_reason(&error);
    assert!(
        reason.contains("databases"),
        "the refusal must name what actually arrived: {reason}"
    );
    assert!(sink.chunks().is_empty());
    assert!(!sink.completed());
}

// ---------------------------------------------------------------------------
// observe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn observation_reports_ready_only_when_the_endpoint_answers() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the server is reachable");

    let first = provider
        .observe_session(&handle)
        .await
        .expect("the handle is live");
    assert_eq!(first.state, SessionState::Ready);
    assert_eq!(first.resource_health, ResourceHealth::Healthy);
    assert_eq!(
        first.context.confidence,
        ObservationConfidence::Unknown,
        "Stargate exposes no session context, so nothing is claimed about it"
    );
    assert_eq!(first.transaction.state, TransactionState::Unsupported);
    assert!(first.protocol_drained);
    assert!(
        first.handles.is_empty(),
        "no handle is issued just by looking at the cluster"
    );

    // A second observation is a fresh round trip, and the revision advances so
    // a caller can tell the two readings apart.
    let second = provider
        .observe_session(&handle)
        .await
        .expect("the handle is still live");
    assert!(second.context_revision > first.context_revision);
}

#[tokio::test]
async fn something_listening_that_is_not_stargate_is_degraded_not_ready() {
    let server = stargate_probe_answers_wrongly().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("a client is registered without a round trip");

    let observation = provider
        .observe_session(&handle)
        .await
        .expect("the handle is live");
    assert_eq!(
        observation.state,
        SessionState::Unknown,
        "a 404 on the cluster root is not a usable session"
    );
    assert_eq!(
        observation.resource_health,
        ResourceHealth::Degraded,
        "something answered, so this is not a lost resource"
    );
}

#[tokio::test]
async fn a_client_the_driver_no_longer_holds_is_reported_as_lost() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the server is reachable");

    // Drop the client behind the provider's back. The registry entry is
    // deliberately left alone, so the probe has to notice on its own.
    let connection = provider
        .connection_of_for_test(&handle)
        .expect("a live resource has a connection");
    provider
        .driver()
        .disconnect(connection)
        .await
        .expect("disconnecting the client is not a failure");

    let observation = provider
        .observe_session(&handle)
        .await
        .expect("the handle is still this provider's to answer for");
    assert_eq!(
        observation.state,
        SessionState::Lost,
        "a client the driver dropped cannot answer a single request"
    );
    assert_eq!(observation.resource_health, ResourceHealth::Lost);
}

// ---------------------------------------------------------------------------
// reset and close
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resetting_discards_a_resource_whose_baseline_is_empty() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the server is reachable");

    let disposition = provider
        .reset_resource(&handle, &baseline())
        .await
        .expect("an empty baseline has nothing to replay");
    assert_eq!(
        disposition,
        ResetDisposition::Discard,
        "nothing was verified, so nothing may be reported as verified"
    );
    // A discard keeps the resource, so it is still usable afterwards.
    assert!(provider.observe_session(&handle).await.is_ok());
}

#[tokio::test]
async fn closing_releases_the_charge_exactly_once_and_is_idempotent() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the server is reachable");

    let first = provider
        .close_resource(&handle)
        .await
        .expect("closing a live resource succeeds");
    assert_eq!(first, CloseDisposition::Closed);
    assert_eq!(
        budget.released(),
        vec!["1".to_string()],
        "exactly one permit, returned exactly once"
    );

    // Closing again is not an error — a caller that retries a close must not be
    // told it failed — and it must not return the same permit a second time.
    let second = provider
        .close_resource(&handle)
        .await
        .expect("a repeated close is still a close");
    assert_eq!(second, CloseDisposition::Closed);
    assert_eq!(
        budget.released(),
        vec!["1".to_string()],
        "an idempotent close must not double-release the budget"
    );
}

#[tokio::test]
async fn a_handle_from_another_epoch_is_refused_everywhere() {
    let mine = provider();
    let theirs = provider();
    let handle = unacquired_handle(&theirs, "hbase_resource:hbase-cfg");

    // Minted by another instance of the same provider, so the key is right and
    // the shape is right; only the epoch differs.
    let error = mine
        .close_resource(&handle)
        .await
        .expect_err("a handle from another instance is not ours to close");
    assert!(
        matches!(error, ResourceError::StaleRuntimeEpoch { .. }),
        "a right-shaped handle from another instance is a stale epoch, not a \
         missing resource: {error:?}"
    );
}

#[tokio::test]
async fn a_handle_from_another_driver_is_an_ownership_mismatch() {
    let provider = provider();
    // Minted under another driver's id: the key shape is plausible and the
    // epoch is current, so only the ownership check can catch it.
    let other = ResourceHandle::issue(
        "mysql",
        "hbase_resource:hbase-cfg",
        provider.runtime_epoch(),
    );
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(&other, &execution_id(), &scan_call(), &sink)
        .await
        .expect_err("a handle minted by another driver is never this provider's");
    assert!(
        matches!(error, ResourceError::ResourceOwnershipMismatch { .. }),
        "another driver's handle is an ownership mismatch, not a stale epoch or a \
         missing resource: {error:?}"
    );
    assert!(sink.chunks().is_empty(), "a foreign handle reaches no wire");
}

#[tokio::test]
async fn a_key_this_provider_never_opened_is_refused_rather_than_created() {
    let provider = provider();
    let handle = unacquired_handle(&provider, "hbase_resource:never-opened");
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(&handle, &execution_id(), &scan_call(), &sink)
        .await
        .expect_err("this provider holds no resource under that key");
    assert!(matches!(error, ResourceError::InvalidResourceState { .. }));
    assert!(sink.chunks().is_empty());
    assert!(!sink.completed());

    let error = provider
        .observe_session(&handle)
        .await
        .expect_err("observing a resource that was never acquired is an error");
    assert!(matches!(error, ResourceError::InvalidResourceState { .. }));

    let error = provider
        .close_resource(&handle)
        .await
        .expect_err("closing a key this provider never opened is an error too");
    assert!(matches!(error, ResourceError::InvalidResourceState { .. }));
}

/// The fixtures guard themselves. `scope()` and `baseline()` are used by the
/// acquires above that are *expected to succeed*, so a fixture that quietly
/// started asking for an open transaction or a pinned handle would turn every
/// happy-path test into a refusal — silently, and for the wrong reason.
#[test]
fn the_succeeding_fixtures_ask_for_nothing_this_driver_refuses() {
    let scope = scope();
    assert_eq!(scope.purpose, ResourcePurpose::InteractiveQuery);
    assert!(
        !scope.holds_open_transaction,
        "this driver has no transaction to hold, so a fixture that asked for one \
         would make every acquire above a refusal test"
    );
    assert!(
        !scope.pin_for_streaming,
        "this driver issues no pinnable handle, so a fixture that asked for one \
         would make every acquire above a refusal test"
    );
    assert!(
        scope.max_physical_connections >= 1,
        "the declared cost is one connection, so a scope below one is refused"
    );
}

#[test]
fn the_succeeding_fixtures_carry_the_empty_baseline() {
    // A non-empty baseline is refused by `validate_acquisition`, so a fixture
    // that smuggled one in would make the acquires above fail for a reason the
    // test never mentions.
    assert_eq!(baseline(), Baseline::default());
    assert!(baseline().initialization_requirements.is_empty());
}
