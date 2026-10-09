//! Out-of-tree conformance tests for the P2 resource contract.
//!
//! Everything a provider needs is reached through the crate's public API only:
//! the fakes live in `support` and no test can reach a private item of
//! `datazen-driver-api`. If the contract were not implementable from outside
//! the crate, this target would not compile.

mod support;

use support::*;

#[tokio::test]
async fn an_out_of_tree_provider_can_drive_the_whole_contract() {
    let provider = FakeProvider::stateful();
    let budget = Arc::new(AllowanceBudget::new(4));

    let descriptor = provider
        .describe_resource(&request(
            datazen_driver_api::resource::ResourcePurpose::InteractiveQuery,
        ))
        .await
        .expect("describe works");
    assert!(descriptor.is_fixed_session());

    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire works");

    let sink = CollectingSink::new();
    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("query", serde_json::json!({ "sql": "select 1" })),
            &sink,
        )
        .await
        .expect("execute works");
    assert_eq!(completion.completion_status, CompletionStatus::Succeeded);
    assert!(completion
        .context_after
        .matches_target(&NamespaceTarget::empty().with_database("app")));
    assert_eq!(sink.chunk_count(), 1);
    assert_eq!(sink.completed.load(Ordering::SeqCst), 1);

    let observed = provider
        .observe_session(&handle)
        .await
        .expect("observe works");
    assert_eq!(observed.state, SessionState::Ready);
    assert_eq!(observed.context_revision, 11);

    assert_eq!(
        provider
            .change_context(&handle, &NamespaceTarget::empty().with_database("app"))
            .await
            .expect("context change answers"),
        ContextChangeDisposition::Confirmed
    );
    assert_eq!(
        provider
            .change_context(&handle, &NamespaceTarget::empty().with_database("other"))
            .await
            .expect("context change answers"),
        ContextChangeDisposition::RequiresReplacement
    );

    let begun = provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin works");
    assert_eq!(begun.state, TransactionState::Active);
    assert_eq!(
        provider
            .commit_transaction(&handle)
            .await
            .expect("commit works")
            .state,
        TransactionState::Idle
    );
    assert_eq!(
        provider
            .rollback_transaction(&handle)
            .await
            .expect("rollback works")
            .state,
        TransactionState::Aborted
    );

    assert_eq!(
        provider
            .reset_resource(&handle, &Baseline::default())
            .await
            .expect("reset answers"),
        ResetDisposition::Clean
    );
}

#[tokio::test]
async fn precise_cancel_distinguishes_all_four_outcomes_instead_of_one_success() {
    let provider = FakeProvider::stateful();
    let budget = Arc::new(AllowanceBudget::new(4));
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire works");

    // The execution has to exist on this resource before a precise cancel can
    // name it; that is what makes `NotRegistered` a real distinction.
    provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("query", serde_json::json!({ "sql": "select 1" })),
            &CollectingSink::new(),
        )
        .await
        .expect("execute works");

    // 1. accepted
    let first = provider
        .request_cancel(&handle, &QueryExecutionId::new("exec-1"))
        .await
        .expect("cancel answers");
    assert_eq!(first.disposition, CancelDisposition::Requested);
    assert!(first.disposition.is_accepted());
    assert_eq!(first.state, Some(ExecutionState::CancelRequested));

    // 2. already finished — a different, explicit answer
    let second = provider
        .request_cancel(&handle, &QueryExecutionId::new("exec-1"))
        .await
        .expect("cancel answers");
    assert_eq!(second.disposition, CancelDisposition::AlreadyFinished);
    assert!(!second.disposition.is_accepted());

    // 3. never registered
    let unknown = provider
        .request_cancel(&handle, &QueryExecutionId::new("never-ran"))
        .await
        .expect("cancel answers");
    assert_eq!(unknown.disposition, CancelDisposition::NotRegistered);
    assert!(unknown.state.is_none());

    // 4. the driver cannot address a single execution at all
    let minimal = FakeProvider::minimal();
    let minimal_handle = minimal
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire works");
    let refused = minimal
        .request_cancel(&minimal_handle, &QueryExecutionId::new("exec-1"))
        .await
        .expect("a refusal is a receipt, not an error");
    assert_eq!(refused.disposition, CancelDisposition::Unsupported);
}

#[tokio::test]
async fn a_provider_that_cannot_answer_refuses_instead_of_succeeding_quietly() {
    let provider = FakeProvider::minimal();
    let budget = Arc::new(AllowanceBudget::new(4));
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire still works");

    for err in [
        provider.observe_session(&handle).await.unwrap_err(),
        provider
            .change_context(&handle, &NamespaceTarget::empty())
            .await
            .unwrap_err(),
        provider
            .begin_transaction(&handle, &TransactionOptions::default())
            .await
            .unwrap_err(),
        provider.commit_transaction(&handle).await.unwrap_err(),
        provider.rollback_transaction(&handle).await.unwrap_err(),
        provider
            .reset_resource(&handle, &Baseline::default())
            .await
            .unwrap_err(),
        provider
            .execute_on_resource(
                &handle,
                &QueryExecutionId::new("exec-1"),
                &CommandCall::new("query", serde_json::json!({})),
                &CollectingSink::new(),
            )
            .await
            .unwrap_err(),
    ] {
        assert!(
            matches!(err, ResourceError::OperationNotSupported { .. }),
            "expected an explicit refusal, got {err}"
        );
    }

    // And the descriptor told the truth about it up front.
    let descriptor = provider
        .describe_resource(&request(
            datazen_driver_api::resource::ResourcePurpose::MetadataInspection,
        ))
        .await
        .expect("describe works");
    assert!(!descriptor.is_fixed_session());
    assert_eq!(descriptor.session_continuity, SessionContinuity::Leased);
}

#[tokio::test]
async fn a_handle_from_another_provider_or_epoch_is_refused() {
    let provider = FakeProvider::new(
        9,
        ProviderProfile {
            stateful: true,
            observes_context: true,
            precise_cancel: true,
            verified_reset: true,
        },
    );
    // These handles are hand-issued, so nothing is acquired and no budget is
    // charged — the provider must reject them before any resource work.
    let stale = ResourceHandle::issue("fake-provider".to_string(), "app".to_string(), 8);
    let foreign = ResourceHandle::issue("someone-else".to_string(), "app".to_string(), 9);

    assert!(matches!(
        provider.observe_session(&stale).await.unwrap_err(),
        ResourceError::StaleRuntimeEpoch { .. }
    ));
    assert!(matches!(
        provider.observe_session(&foreign).await.unwrap_err(),
        ResourceError::ResourceOwnershipMismatch { .. }
    ));
    assert!(matches!(
        provider.close_resource(&stale).await.unwrap_err(),
        ResourceError::StaleRuntimeEpoch { .. }
    ));
}

#[tokio::test]
async fn the_budget_is_charged_per_physical_connection_and_released_once() {
    let provider = FakeProvider::stateful();
    let budget = Arc::new(AllowanceBudget::new(2));

    let handle = provider
        .acquire_resource(
            &AcquireResourceRequest {
                scope: ResourceScope {
                    max_physical_connections: 2,
                    ..acquire_request().scope
                },
                ..acquire_request()
            },
            &port(&budget),
        )
        .await
        .expect("acquire works");
    assert_eq!(budget.outstanding.load(Ordering::SeqCst), 1);

    assert_eq!(
        provider.close_resource(&handle).await.expect("close works"),
        SessionCloseDisposition::Closed
    );
    assert_eq!(provider.disconnects.load(Ordering::SeqCst), 1);

    // A closed resource refuses further work rather than resurrecting itself.
    assert!(provider.close_resource(&handle).await.is_ok());
}

#[tokio::test]
async fn a_namespace_level_the_database_lacks_is_refused_before_anything_opens() {
    let provider = FakeProvider::stateful();
    let budget = Arc::new(AllowanceBudget::new(4));

    let err = provider
        .acquire_resource(
            &AcquireResourceRequest {
                target: NamespaceTarget {
                    catalog: Some("no_such_level".into()),
                    ..acquire_request().target
                },
                ..acquire_request()
            },
            &port(&budget),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ResourceError::NonexistentNamespaceLevel { .. }),
        "got {err}"
    );
    assert_eq!(budget.issued.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_factory_that_declares_no_provider_is_an_error_not_an_empty_success() {
    let err = match require_resource_provider(&SilentFactory) {
        Ok(_) => panic!("a factory that declares no provider must not yield one"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("silent"), "{err}");

    let provider = Arc::new(FakeProvider::stateful());
    let factory = AdvertisingFactory { provider };
    let found = require_resource_provider(&factory).expect("the provider is reachable");
    assert_eq!(found.provider_id(), "fake-provider");
    assert_eq!(found.capabilities().snapshot.capability_revision, 9);
}

#[tokio::test]
async fn the_contract_survives_serde_at_the_boundary() {
    // Wire shape is camelCase on both sides; the handle is opaque but still
    // inspectable for routing, never re-authoritative.
    let json = serde_json::to_value(CancelDisposition::AlreadyFinished).expect("serializes");
    assert_eq!(json, serde_json::json!("alreadyFinished"));

    let descriptor = FakeProvider::stateful()
        .describe_resource(&request(
            datazen_driver_api::resource::ResourcePurpose::ExportJob,
        ))
        .await
        .expect("describe works");
    let encoded = serde_json::to_value(&descriptor).expect("descriptor serializes");
    assert_eq!(encoded["providerId"], "fake-provider");
    assert_eq!(encoded["sessionContinuity"], "fixed");

    let decoded: ResourceDescriptor = serde_json::from_value(encoded).expect("round trips");
    assert_eq!(decoded, descriptor);
}
