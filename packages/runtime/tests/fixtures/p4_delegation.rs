use super::*;
fn delegated(policy: &Policy) -> RequestContext {
    let mut context = ctx("service", "scheduler");
    context.delegation_id = Some(DelegationId::new("grant"));
    *lock(&policy.delegation) = Some(DelegationGrant {
        delegation: datazen_platform_api::context::DelegationRef::new(
            DelegationId::new("grant"),
            context.organization_id.clone(),
            context.principal_id.clone(),
            context.client_instance_id.clone(),
            "database:a".into(),
            Timestamp::new((chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339()),
        ),
        actions: vec![AuthorizationAction::Connect, AuthorizationAction::Query],
    });
    context
}
#[tokio::test]
async fn delegation_missing_expired_and_wrong_principal_fail_before_resources() {
    let (service, backend, _, policy) = setup();
    let context = delegated(&policy);
    let valid = lock(&policy.delegation).clone();
    for case in 0..3 {
        *lock(&policy.delegation) = valid.clone();
        match case {
            0 => *lock(&policy.delegation) = None,
            1 => {
                lock(&policy.delegation)
                    .as_mut()
                    .unwrap()
                    .delegation
                    .expires_at = Timestamp::new("2000-01-01T00:00:00Z")
            }
            _ => {
                lock(&policy.delegation)
                    .as_mut()
                    .unwrap()
                    .delegation
                    .principal_id = PrincipalId::new("wrong-service")
            }
        }
        assert!(service
            .issue_submission_token(&context, IdempotentOperation::ExecuteAtTarget, None)
            .await
            .is_err());
        assert_eq!(backend.opened.load(Ordering::SeqCst), 0);
    }
}
#[tokio::test]
async fn delegated_target_scope_rejects_before_resource_admission() {
    let (service, backend, _, policy) = setup();
    let context = delegated(&policy);
    let token = service
        .issue_submission_token(&context, IdempotentOperation::ExecuteAtTarget, None)
        .await
        .unwrap();
    assert!(service
        .execute_at_target(
            &context,
            ExecuteAtTargetRequest::new(
                target("b"),
                Counter::new(1),
                CommandCall::new("query", serde_json::json!({"sql":"select 1"})),
                token.idempotency_key
            )
        )
        .await
        .is_err());
    assert_eq!(backend.opened.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn delegated_enqueue_and_dispatch_recheck_revocation_and_cleanup_still_works() {
    let (service, backend, _, policy) = setup();
    let context = delegated(&policy);
    let opened = open(&service, &context, "synthetic-service-session").await;
    let token = service
        .issue_submission_token(
            &context,
            IdempotentOperation::ExecuteInSession,
            Some(&opened.session.handle),
        )
        .await
        .unwrap();
    policy.revoke_on_target_check.store(2, Ordering::SeqCst);
    let receipt = service
        .execute_in_session(
            &context,
            ExecuteInSessionRequest::new(
                opened.session.handle.clone(),
                CommandCall::new("query", serde_json::json!({"sql":"select 1"})),
                token.idempotency_key,
            ),
        )
        .await
        .unwrap();
    for _ in 0..100 {
        if policy.target_checks.load(Ordering::SeqCst) >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    assert_eq!(policy.target_checks.load(Ordering::SeqCst), 2);
    assert!(
        lock(&backend.ids).is_empty(),
        "revoked-before-dispatch never reached driver"
    );
    service
        .close_session(
            &context,
            CloseSessionRequest::new(opened.session.handle, CloseMode::RollbackAndClose),
        )
        .await
        .unwrap();
    assert_eq!(
        backend.closed.load(Ordering::SeqCst),
        1,
        "revocation cannot strand owned resource cleanup"
    );
    policy.allow.store(true, Ordering::SeqCst);
    let view = service
        .get_execution(&context, receipt.execution_id)
        .await
        .unwrap();
    assert_eq!(view.state, ExecutionState::Failed);
}
#[tokio::test]
async fn valid_delegated_execution_reaches_driver_under_service_identity() {
    let (service, backend, _, policy) = setup();
    let context = delegated(&policy);
    let opened = open(&service, &context, "synthetic-service-session").await;
    let receipt = execute(&service, &context, &opened.session.handle, "query").await;
    for _ in 0..100 {
        if !lock(&backend.ids).is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    assert_eq!(lock(&backend.ids).len(), 1);
    assert!(policy.target_checks.load(Ordering::SeqCst) >= 2);
    let view = service
        .get_execution(&context, receipt.execution_id)
        .await
        .unwrap();
    assert_eq!(view.provenance.unwrap().principal_id, context.principal_id);
    service
        .close_session(
            &context,
            CloseSessionRequest::new(opened.session.handle, CloseMode::RollbackAndClose),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn delegated_results_require_current_policy_and_exact_live_execution_binding() {
    use datazen_platform_api::ports::artifact::{ArtifactRepository, ArtifactSpec, ChunkPayload};
    let (service, backend, _, policy) = setup();
    let context = delegated(&policy);
    let dir = tempfile::tempdir().unwrap();
    let store =
        Arc::new(datazen_runtime::artifact::FileArtifactRepository::open(dir.path()).unwrap());
    service
        .result_sink()
        .with_artifact_store(store.clone())
        .unwrap();
    let opened = open(&service, &context, "delegated-results").await;
    let receipt = execute(&service, &context, &opened.session.handle, "query").await;
    let view = terminal(&service, &context, &receipt.execution_id).await;
    assert_eq!(view.state, ExecutionState::Succeeded);
    assert_eq!(view.effect_outcome, EffectOutcome::Completed);
    assert_eq!(view.artifact_ids.len(), 1);
    let artifact = view.artifact_ids[0].clone();
    let mut another_request = context.clone();
    another_request.request_id = RequestId::new("another-request");
    assert_eq!(
        service
            .read_artifact_chunk(&another_request, artifact.clone(), Counter::ZERO)
            .await
            .unwrap()
            .rows,
        vec![serde_json::json!([1])]
    );
    // The public artifact port never acquires delegated arbitrary capabilities.
    assert!(store
        .read_chunk(&context, artifact.clone(), Counter::ZERO)
        .await
        .is_err());
    assert!(store
        .create(
            &context,
            ArtifactSpec::new(
                context.organization_id.clone(),
                context.principal_id.clone(),
                ExecutionId::new("arbitrary"),
                "scope".into()
            )
        )
        .await
        .is_err());
    assert!(store
        .authorize_export(
            &context,
            LeaseId::new("forbidden-export"),
            dir.path().join("forbidden.sql")
        )
        .is_err());
    let mut owner = context.clone();
    owner.delegation_id = None;
    assert!(
        store
            .read_chunk(&owner, artifact.clone(), Counter::ZERO)
            .await
            .is_err(),
        "a delegated result is not an ordinary owner artifact"
    );
    let (unrelated, mut writer) = store
        .create(
            &owner,
            ArtifactSpec::new(
                owner.organization_id.clone(),
                owner.principal_id.clone(),
                ExecutionId::new("unrelated"),
                "scope".into(),
            ),
        )
        .await
        .unwrap();
    store
        .append_chunk(&mut writer, ChunkPayload::new(b"unrelated".to_vec()))
        .await
        .unwrap();
    store.finalize(writer).await.unwrap();
    assert!(service
        .read_artifact_chunk(&context, unrelated.clone(), Counter::ZERO)
        .await
        .is_err());
    for field in 0..4 {
        let mut mismatch = context.clone();
        match field {
            0 => mismatch.organization_id = OrganizationId::new("other-org"),
            1 => mismatch.principal_id = PrincipalId::new("other-principal"),
            2 => mismatch.client_instance_id = ClientInstanceId::new("other-client"),
            _ => {
                mismatch.authentication_session_id =
                    Some(AuthenticationSessionId::new("other-authentication"))
            }
        }
        assert!(service
            .read_artifact_chunk(&mismatch, artifact.clone(), Counter::ZERO)
            .await
            .is_err());
    }
    // Even a separately valid delegation with the same org/principal/client
    // cannot borrow another delegation's admitted result capability.
    let original = lock(&policy.delegation).clone();
    let mut sibling = context.clone();
    sibling.delegation_id = Some(DelegationId::new("sibling-grant"));
    lock(&policy.delegation)
        .as_mut()
        .unwrap()
        .delegation
        .delegation_id = DelegationId::new("sibling-grant");
    assert!(service
        .read_artifact_chunk(&sibling, artifact.clone(), Counter::ZERO)
        .await
        .is_err());
    *lock(&policy.delegation) = original;
    policy.allow.store(false, Ordering::SeqCst);
    assert!(service
        .read_artifact_chunk(&context, artifact.clone(), Counter::ZERO)
        .await
        .is_err());
    policy.allow.store(true, Ordering::SeqCst);
    assert!(service
        .read_artifact_chunk(&context, artifact.clone(), Counter::ZERO)
        .await
        .is_ok());
    service
        .close_session(
            &context,
            CloseSessionRequest::new(opened.session.handle, CloseMode::RollbackAndClose),
        )
        .await
        .unwrap();
    assert_eq!(
        lock(&backend.ids).len(),
        1,
        "result reads never resubmit SQL"
    );
    let recovered =
        Arc::new(datazen_runtime::artifact::FileArtifactRepository::open(dir.path()).unwrap());
    assert!(recovered.describe(&owner, artifact.clone()).await.is_err());
    assert!(recovered.describe(&owner, unrelated).await.is_ok());
    let (restarted, restarted_backend, _, restarted_policy) = setup();
    let restarted_context = delegated(&restarted_policy);
    restarted
        .result_sink()
        .with_artifact_store(recovered)
        .unwrap();
    assert!(restarted
        .read_artifact_chunk(&restarted_context, artifact, Counter::ZERO)
        .await
        .is_err());
    assert_eq!(restarted_backend.opened.load(Ordering::SeqCst), 0);
}
