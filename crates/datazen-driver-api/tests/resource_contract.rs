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
        provider
            .close_handle(&handle, &ref_for(&handle, provider.epoch, "seq-1"))
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

// ---------------------------------------------------------------------------
// §6.5 per-handle close
// ---------------------------------------------------------------------------

/// A ref of the shape a host receives in `ExecutionCompletion.session_handles`.
fn ref_for(handle: &ResourceHandle, epoch: u64, handle_id: &str) -> SessionHandleRef {
    SessionHandleRef::new(
        handle_id.to_string(),
        SessionHandleKind::ServerPrepared,
        handle.resource_key().to_string(),
        epoch,
    )
}

/// Acquires a resource and runs one execution, which is what mints the fake's
/// server-prepared ref. Returns the resource plus that ref, straight from the
/// provider rather than hand-built, so the test follows the host's real path.
async fn acquire_and_mint() -> (FakeProvider, ResourceHandle, SessionHandleRef) {
    let provider = FakeProvider::stateful();
    let budget = Arc::new(AllowanceBudget::new(4));
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire works");

    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("query", serde_json::json!({ "sql": "select 1" })),
            &CollectingSink::new(),
        )
        .await
        .expect("execute works");
    assert_eq!(
        completion.session_handles.len(),
        1,
        "the fake declares session-scoped handle support, so one execution mints one ref"
    );

    let session_handle = completion.session_handles[0].clone();
    assert_eq!(session_handle.kind, SessionHandleKind::ServerPrepared);
    (provider, handle, session_handle)
}

#[tokio::test]
async fn a_ref_the_provider_reported_can_be_closed_and_then_only_once() {
    let (provider, handle, session_handle) = acquire_and_mint().await;

    provider
        .close_handle(&handle, &session_handle)
        .await
        .expect("closing a ref the provider just reported must succeed");

    let closed = provider
        .closed_handles
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert!(
        closed.contains(&session_handle.handle_id),
        "the handle must be recorded as closed, exactly once"
    );
    assert_eq!(closed.len(), 1);
    drop(closed);

    // Closing a handle is not closing the resource: the host keys its own
    // bookkeeping off `ResourceHandle::is_closed`, so a handle close that
    // flipped it would look identical to a resource close.
    assert!(!handle.is_closed());

    // A second close of the same ref names nothing the provider still holds.
    // Reporting success here would tell the host a teardown happened twice.
    assert!(
        matches!(
            provider
                .close_handle(&handle, &session_handle)
                .await
                .unwrap_err(),
            ResourceError::InvalidResourceState { .. }
        ),
        "closing an already-closed ref must not succeed a second time"
    );
    assert!(!handle.is_closed());
}

#[tokio::test]
async fn a_handle_close_leaves_the_resources_budget_and_session_alone() {
    let (provider, handle, session_handle) = acquire_and_mint().await;

    provider
        .close_handle(&handle, &session_handle)
        .await
        .expect("close works");

    // The resource is still open and still observable — a handle close is a
    // narrow operation, and a provider that quietly tore the session down
    // would break every other caller sharing it.
    assert!(!handle.is_closed());
    let observed = provider
        .observe_session(&handle)
        .await
        .expect("the resource must still be usable after a handle close");
    assert_eq!(observed.state, SessionState::Ready);
}

#[tokio::test]
async fn a_provider_that_declares_no_session_handles_refuses_instead_of_claiming_a_close() {
    // `FakeProvider::minimal()` reports `session_scoped_handles` as anything but
    // `Supported`. It must refuse by name: returning `Ok(())` would be a claim
    // that a handle was torn down when this provider never held one.
    let provider = FakeProvider::minimal();
    let budget = Arc::new(AllowanceBudget::new(4));
    let handle = provider
        .acquire_resource(&acquire_request(), &port(&budget))
        .await
        .expect("acquire works");
    let session_handle = ref_for(&handle, provider.epoch, "seq-never-minted");

    match provider
        .close_handle(&handle, &session_handle)
        .await
        .unwrap_err()
    {
        ResourceError::OperationNotSupported {
            ref operation,
            ref driver,
            ..
        } => {
            assert_eq!(operation, "close_handle");
            assert_eq!(driver, "fake-provider");
        }
        other => panic!("expected an explicit refusal, got {other:?}"),
    }
    assert!(!handle.is_closed());
}

#[tokio::test]
async fn a_handle_close_checks_ownership_and_epoch_before_it_looks_at_the_ref() {
    // Same ordering every driver uses: a wrong handle is a wrong handle, not a
    // generic "not supported". The host cannot retry safely if the two faults
    // are collapsed into one.
    let (provider, handle, session_handle) = acquire_and_mint().await;

    let foreign = ResourceHandle::issue("someone-else", handle.resource_key(), provider.epoch);
    assert!(matches!(
        provider
            .close_handle(&foreign, &session_handle)
            .await
            .unwrap_err(),
        ResourceError::ResourceOwnershipMismatch { .. }
    ));

    let stale = ResourceHandle::issue("fake-provider", handle.resource_key(), provider.epoch - 1);
    assert!(matches!(
        provider
            .close_handle(&stale, &session_handle)
            .await
            .unwrap_err(),
        ResourceError::StaleRuntimeEpoch { .. }
    ));
}

#[tokio::test]
async fn a_ref_borrowed_from_another_resource_cannot_close_this_one() {
    // §6.5: a ref belongs to exactly one resource. Replaying it against a second
    // resource must not tear down whatever happens to be open there.
    let (provider, handle, session_handle) = acquire_and_mint().await;
    let borrowed = SessionHandleRef::new(
        session_handle.handle_id.clone(),
        session_handle.kind,
        "a-different-resource".to_string(),
        provider.epoch,
    );

    assert!(
        matches!(
            provider.close_handle(&handle, &borrowed).await.unwrap_err(),
            ResourceError::InvalidResourceState { .. }
        ),
        "a ref minted for another resource must not close this one"
    );
    // And the real ref is still closable: the refusal consumed nothing.
    provider
        .close_handle(&handle, &session_handle)
        .await
        .expect("the refusal must not have consumed the handle");
}

#[tokio::test]
async fn a_handle_close_after_the_resource_closed_is_a_no_op_success() {
    // The host can lose a race: `close_resource` may land first. The contract
    // says that is a success, because the handle's underlying object is already
    // gone with the session.
    let (provider, handle, session_handle) = acquire_and_mint().await;
    provider
        .close_handle(&handle, &session_handle)
        .await
        .expect("close works");
    provider
        .close_resource(&handle)
        .await
        .expect("resource close works");

    provider
        .close_handle(&handle, &session_handle)
        .await
        .expect("closing a handle after the resource closed is idempotent");
}
