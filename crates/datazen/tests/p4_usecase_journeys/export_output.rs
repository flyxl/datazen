use super::*;

#[tokio::test]
async fn export_binding_requires_dedicated_owned_target_and_current_revision() {
    let (service, _, profiles, _) = setup();
    let ctx = ctx("u", "client");
    let editor = open(&service, &ctx, "editor").await;
    assert!(service
        .bind_export_output(
            &ctx,
            &editor.session.handle,
            &target("a"),
            ConfigRevision::new(1)
        )
        .await
        .is_err());
    let token = service
        .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
        .await
        .unwrap();
    let opened = service
        .open_session(
            &ctx,
            OpenSessionRequest::new(
                target("a"),
                OwnerRef::ClientSession {
                    client_instance_id: ctx.client_instance_id.clone(),
                    purpose: "tableExport".into(),
                },
                token.idempotency_key,
            ),
        )
        .await
        .unwrap();
    let handle = opened.session.handle;
    let other = super::ctx("other", "client");
    assert!(service
        .bind_export_output(&other, &handle, &target("a"), ConfigRevision::new(1))
        .await
        .is_err());
    assert!(service
        .bind_export_output(&ctx, &handle, &target("b"), ConfigRevision::new(1))
        .await
        .is_err());
    assert!(service
        .bind_export_output(&ctx, &handle, &target("a"), ConfigRevision::new(2))
        .await
        .is_err());
    let binding = service
        .bind_export_output(&ctx, &handle, &target("a"), ConfigRevision::new(1))
        .await
        .unwrap();
    assert!(service
        .bind_export_output(&ctx, &handle, &target("a"), ConfigRevision::new(1))
        .await
        .is_err());
    assert!(binding.validate(&service).await.is_ok());
    profiles.revision.store(2, Ordering::SeqCst);
    assert!(binding.validate(&service).await.is_err());
    drop(binding);
    service
        .close_session(
            &ctx,
            CloseSessionRequest::new(handle.clone(), CloseMode::RollbackAndClose),
        )
        .await
        .unwrap();
    assert!(service
        .bind_export_output(&ctx, &handle, &target("a"), ConfigRevision::new(2))
        .await
        .is_err());
}
