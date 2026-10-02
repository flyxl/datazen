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
    CloseDisposition, ObservationConfidence, ResourceHealth, SessionState, TransactionState,
};
use datazen_driver_api::DatabaseDriver;

use crate::resource::capabilities::VECTOR_PROVIDER_ID;

use super::support::{
    acquire_request, config, config_at, config_without_endpoint, describe_request, execution_id,
    provider, refusal_reason, scope, search_call, stub_qdrant, unacquired_handle, BudgetLedger,
    RecordingSink, ERROR_BODY, ONE_POINT,
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

    assert_eq!(descriptor.provider_id, VECTOR_PROVIDER_ID);
    assert_eq!(descriptor.resource_key, "vector_resource:vector-cfg");
    // A pool of one is not a session; `is_fixed_session` is the check that keeps
    // a size-one HTTP client from passing as one.
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
            database: Some("books".into()),
            ..NamespaceTarget::empty()
        },
        ..describe_request(&config())
    };
    let error = provider
        .describe_resource(&request)
        .await
        .expect_err("a Qdrant instance has no database level to name");
    assert!(matches!(
        error,
        ResourceError::NonexistentNamespaceLevel { .. }
    ));
}

// ---------------------------------------------------------------------------
// acquire
// ---------------------------------------------------------------------------

#[tokio::test]
async fn acquiring_charges_the_budget_exactly_once() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());

    let handle = provider
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect("a refused port still registers a client, so the acquire succeeds");

    assert_eq!(budget.requested(), vec![1]);
    assert_eq!(budget.granted(), vec![1]);
    assert!(
        budget.released().is_empty(),
        "the charge is released at close, not at acquire"
    );
    assert_eq!(handle.provider_id(), VECTOR_PROVIDER_ID);
}

#[tokio::test]
async fn a_denied_budget_is_reported_and_leaves_the_caller_charged_nothing() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    budget.deny(true);

    let error = provider
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect_err("a denied charge must not produce a resource");
    assert!(matches!(
        error,
        ResourceError::BudgetDenied { requested: 1, .. }
    ));
    assert_eq!(
        budget.requested(),
        vec![1],
        "the budget was asked, and refused"
    );
    assert!(budget.granted().is_empty());
    assert!(
        budget.released().is_empty(),
        "nothing was granted, so nothing may be released"
    );
}

#[tokio::test]
async fn a_scope_that_needs_a_transaction_is_refused_before_any_charge() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());

    let error = provider
        .acquire_resource(
            &acquire_request(
                &config(),
                ResourceScope {
                    holds_open_transaction: true,
                    ..scope()
                },
            ),
            &budget.as_port(),
        )
        .await
        .expect_err("there is no transaction for this resource to hold");
    assert!(refusal_reason(&error).contains("no transaction to hold"));
    assert!(budget.requested().is_empty(), "a refusal costs nothing");

    let error = provider
        .acquire_resource(
            &acquire_request(
                &config(),
                ResourceScope {
                    pin_for_streaming: true,
                    ..scope()
                },
            ),
            &budget.as_port(),
        )
        .await
        .expect_err("this driver issues no handle to pin");
    assert!(refusal_reason(&error).contains("pinning"));
    assert!(budget.requested().is_empty());

    let error = provider
        .acquire_resource(
            &acquire_request(
                &config(),
                ResourceScope {
                    max_physical_connections: 0,
                    ..scope()
                },
            ),
            &budget.as_port(),
        )
        .await
        .expect_err("a scope of zero connections cannot open this resource");
    assert!(matches!(
        error,
        ResourceError::BudgetDenied { requested: 1, .. }
    ));
    assert!(budget.requested().is_empty());
}

#[tokio::test]
async fn a_baseline_this_driver_cannot_replay_is_refused() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let mut request = acquire_request(&config(), scope());
    request.baseline = Baseline {
        initialization_requirements: vec![InitializationRequirement {
            id: "warm-cache".into(),
            sql: "SELECT 1".into(),
            mandatory: true,
            idempotent: true,
        }],
    };

    let error = provider
        .acquire_resource(&request, &budget.as_port())
        .await
        .expect_err("a replayable baseline needs a statement to replay");
    assert!(refusal_reason(&error).contains("initialization requirement"));
    assert!(budget.requested().is_empty());
}

#[tokio::test]
async fn a_failed_connect_releases_exactly_the_charge_it_took() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());

    let error = provider
        .acquire_resource(
            &acquire_request(&config_without_endpoint(), scope()),
            &budget.as_port(),
        )
        .await
        .expect_err("a config with no endpoint cannot build a base URL");
    assert!(matches!(error, ResourceError::Driver(_)));

    assert_eq!(budget.granted(), vec![1]);
    assert_eq!(
        budget.released(),
        vec!["1".to_string()],
        "a charge taken and then not used must come straight back"
    );
}

// ---------------------------------------------------------------------------
// execute
// ---------------------------------------------------------------------------

#[tokio::test]
async fn executing_streams_the_rows_and_reports_only_what_it_proved() {
    let port = stub_qdrant(200, ONE_POINT).await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(
            &acquire_request(&config_at(port), scope()),
            &budget.as_port(),
        )
        .await
        .expect("the stub answers, so the command can run");
    let sink = RecordingSink::default();

    let completion = provider
        .execute_on_resource(&handle, &execution_id(), &search_call(), &sink)
        .await
        .expect("the search succeeds against the stub");

    assert_eq!(completion.statement_results.len(), 1);
    assert_eq!(
        sink.row_count(),
        1,
        "the point the stub returned must be streamed"
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
        "this driver issues no cursor or prepared statement"
    );
}

#[tokio::test]
async fn a_failing_command_ends_the_execution_and_surfaces_the_driver_error() {
    let port = stub_qdrant(500, ERROR_BODY).await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(
            &acquire_request(&config_at(port), scope()),
            &budget.as_port(),
        )
        .await
        .expect("the stub is reachable");
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(&handle, &execution_id(), &search_call(), &sink)
        .await
        .expect_err("a 500 is not a successful query");
    assert!(matches!(error, ResourceError::Driver(_)));
    assert!(
        !sink.completed(),
        "an execution that failed must not report completion"
    );
    assert_eq!(
        sink.failed().is_some(),
        true,
        "the consumer is told the execution ended, not left waiting for rows"
    );
    assert_eq!(sink.row_count(), 0);
}

#[tokio::test]
async fn a_sink_that_refuses_rows_is_reported_as_a_delivery_failure() {
    let port = stub_qdrant(200, ONE_POINT).await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(
            &acquire_request(&config_at(port), scope()),
            &budget.as_port(),
        )
        .await
        .expect("the stub is reachable");
    let sink = RecordingSink::default();
    sink.reject();

    let error = provider
        .execute_on_resource(&handle, &execution_id(), &search_call(), &sink)
        .await
        .expect_err("rows nobody accepted are not a successful execution");
    assert!(matches!(error, ResourceError::SinkRejected { .. }));
    assert!(sink.chunks().is_empty());
}

// ---------------------------------------------------------------------------
// observe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn observation_reports_ready_only_when_the_instance_answers() {
    let port = stub_qdrant(200, ONE_POINT).await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(
            &acquire_request(&config_at(port), scope()),
            &budget.as_port(),
        )
        .await
        .expect("the stub is reachable");

    let first = provider
        .observe_session(&handle)
        .await
        .expect("the handle is live");
    assert_eq!(first.state, SessionState::Ready);
    assert_eq!(first.resource_health, ResourceHealth::Healthy);

    let second = provider
        .observe_session(&handle)
        .await
        .expect("a second probe is still a live resource");
    assert!(
        second.context_revision > first.context_revision,
        "each observation advances the revision a caller compares against"
    );
    // A liveness probe proves nothing else, so nothing else is claimed.
    assert_eq!(first.context.confidence, ObservationConfidence::Unknown);
    assert_eq!(first.transaction.state, TransactionState::Unsupported);
    assert!(first.handles.is_empty());
    assert!(first.protocol_drained);
}

#[tokio::test]
async fn an_unreachable_instance_is_unknown_rather_than_healthy() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    // Port 1: nothing is listening, so the probe gets no status at all.
    let handle = provider
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect("the client registers without a round trip");

    let observation = provider
        .observe_session(&handle)
        .await
        .expect("a refused connection is still an observation");
    assert_ne!(
        observation.state,
        SessionState::Ready,
        "a refused connection is not proof the instance is serving"
    );
    assert_eq!(observation.state, SessionState::Unknown);
    assert_eq!(observation.resource_health, ResourceHealth::Degraded);
}

#[tokio::test]
async fn a_resource_whose_client_is_gone_is_reported_lost() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect("the client registers without a round trip");

    // Drop the client from the driver's pool behind the provider's back — what
    // a driver restart looks like from here.
    let connection = provider
        .connection_of_for_test(&handle)
        .expect("the resource has a driver connection");
    provider
        .driver()
        .disconnect(connection)
        .await
        .expect("the client is dropped from the pool");

    let observation = provider
        .observe_session(&handle)
        .await
        .expect("the resource record is still open, so this is an observation");
    assert_eq!(observation.state, SessionState::Lost);
    assert_eq!(
        observation.resource_health,
        ResourceHealth::Lost,
        "every handle this driver issued is dead when the client is gone"
    );
}

// ---------------------------------------------------------------------------
// close
// ---------------------------------------------------------------------------

#[tokio::test]
async fn closing_releases_the_charge_once_and_stays_idempotent() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect("the client registers without a round trip");

    assert_eq!(
        provider
            .close_resource(&handle)
            .await
            .expect("a close is answered"),
        CloseDisposition::Closed
    );
    assert_eq!(budget.released(), vec!["1".to_string()]);
    assert!(budget.released().len() == 1);

    // A second close is a confirmation, not a second refund.
    assert_eq!(
        provider
            .close_resource(&handle)
            .await
            .expect("a repeated close is answered"),
        CloseDisposition::Closed
    );
    assert_eq!(budget.released(), vec!["1".to_string()]);

    // A confirmed close stays confirmed however many times it is asked for:
    // the host may retry a close whose confirmation it never saw, and turning
    // that retry into an error would push it to treat a closed resource as
    // leaked.
    assert_eq!(
        provider
            .close_resource(&handle)
            .await
            .expect("a third close is answered"),
        CloseDisposition::Closed
    );
    assert_eq!(
        budget.released(),
        vec!["1".to_string()],
        "one open, one refund"
    );

    // A key this provider never opened is a different matter: there is nothing
    // to confirm, so it stays an error.
    let error = provider
        .close_resource(&unacquired_handle(
            &provider,
            "vector_resource:never-opened",
        ))
        .await
        .expect_err("a close of a key this provider never opened cannot be confirmed");
    assert!(matches!(error, ResourceError::InvalidResourceState { .. }));
    assert_eq!(budget.released().len(), 1);
}

// ---------------------------------------------------------------------------
// ownership
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_handle_from_another_provider_instance_is_stale() {
    let first = provider();
    let second = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = first
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect("the client registers without a round trip");

    let error = second
        .execute_on_resource(
            &handle,
            &execution_id(),
            &search_call(),
            &RecordingSink::default(),
        )
        .await
        .expect_err("another instance's handle is not this instance's resource");
    assert!(matches!(error, ResourceError::StaleRuntimeEpoch { .. }));
}

#[tokio::test]
async fn a_handle_from_another_driver_is_an_ownership_mismatch() {
    let provider = provider();
    let other = ResourceHandle::issue(
        "mysql",
        "vector_resource:vector-cfg",
        provider.runtime_epoch(),
    );
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(&other, &execution_id(), &search_call(), &sink)
        .await
        .expect_err("a handle minted by another driver is never this provider's");
    assert!(matches!(
        error,
        ResourceError::ResourceOwnershipMismatch { .. }
    ));
    assert!(sink.chunks().is_empty(), "a foreign handle reaches no wire");
}

#[tokio::test]
async fn an_operation_on_a_never_acquired_key_reports_invalid_state() {
    let provider = provider();
    let handle: ResourceHandle = unacquired_handle(&provider, "vector_resource:never-opened");
    let sink = RecordingSink::default();

    let error = provider
        .execute_on_resource(&handle, &execution_id(), &search_call(), &sink)
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
}

/// The purpose a scope carries is echoed into nothing here, but the fixture
/// keeps the default explicit so a future change to `ResourceScope` fails loudly
/// rather than quietly defaulting.
#[test]
fn the_scope_fixture_asks_for_an_interactive_query() {
    assert_eq!(scope().purpose, ResourcePurpose::InteractiveQuery);
}
