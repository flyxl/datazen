use super::*;
use crate::testing::{app_state::TestAppState, mock_driver::MockDriverOptions};

async fn instance(test: &TestAppState) -> Arc<Instance> {
    let adapter = test.state.platform_require().unwrap().clone();
    let identity = adapter.identity();
    let context = RequestContext::new(
        identity.organization_id().clone(),
        identity.principal_id().clone(),
        None,
        crate::platform::new_client_instance_id(),
        crate::platform::identity::new_request_id(),
        None,
    );
    Arc::new(Instance {
        wapp: "test.sessions".into(),
        adapter,
        context,
        expires: Instant::now() + Duration::from_secs(3600),
        ended: AtomicBool::new(false),
        sessions: McpSessionRegistry::new(),
        gate: tokio::sync::Mutex::new(()),
        executions: Mutex::new(HashMap::new()),
        active: Mutex::new(HashSet::new()),
    })
}
async fn open(instance: &Instance, ttl: Duration) -> crate::mcp::session::SessionBinding {
    let runtime = instance.adapter.runtime().unwrap();
    let token = runtime
        .issue_submission_token(&instance.context, IdempotentOperation::OpenSession, None)
        .await
        .unwrap();
    let receipt = runtime
        .open_session(
            &instance.context,
            OpenSessionRequest::new(
                ExecutionTarget::new(
                    ConnectionId::new("wapp-cleanup"),
                    NamespaceTarget::database("app"),
                ),
                OwnerRef::ClientSession {
                    client_instance_id: instance.context.client_instance_id.clone(),
                    purpose: "wapp-stateful-session".into(),
                },
                token.idempotency_key,
            ),
        )
        .await
        .unwrap();
    instance.sessions.open(
        instance.context.client_instance_id.as_str(),
        "wapp-cleanup",
        receipt.session.handle,
        receipt.session.context_revision,
        ttl,
        Instant::now(),
    )
}
async fn cleaned(instance: &Instance, binding: &crate::mcp::session::SessionBinding) {
    let runtime = instance.adapter.runtime().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !runtime.is_registered(&datazen_runtime::connection::DbSessionId::new(
                binding.handle.db_session_id.as_str(),
            )) && instance.sessions.pending_cleanup().is_empty()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("original resource cleanup confirmed");
}
#[tokio::test]
async fn ttl_revokes_without_lookup_and_end_preserves_other_instance_on_same_connection() {
    let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
    test.save_connection("wapp-cleanup").await;
    let a = instance(&test).await;
    let b = instance(&test).await;
    let expired = open(&a, Duration::from_millis(20)).await;
    let other = open(&b, Duration::from_secs(60)).await;
    // Another Wapp/iframe cannot look up this ref or close the original Runtime handle.
    assert!(a
        .sessions
        .lookup(
            &expired.reference,
            b.context.client_instance_id.as_str(),
            Instant::now()
        )
        .is_err());
    assert!(b
        .adapter
        .runtime()
        .unwrap()
        .get_session(&b.context, expired.handle.clone())
        .await
        .is_err());
    spawn_cleanup(uuid::Uuid::new_v4().to_string(), a.clone());
    cleaned(&a, &expired).await;
    assert!(a
        .sessions
        .lookup(
            &expired.reference,
            a.context.client_instance_id.as_str(),
            Instant::now()
        )
        .is_err());
    assert!(b
        .adapter
        .runtime()
        .unwrap()
        .get_session(&b.context, other.handle.clone())
        .await
        .is_ok());
    // End is idempotent, invalidates admission immediately and releases only this owner.
    b.end();
    b.end();
    assert!(b.check().is_err());
    spawn_cleanup(uuid::Uuid::new_v4().to_string(), b.clone());
    cleaned(&b, &other).await;
    a.end();
    let snapshot = b
        .adapter
        .runtime()
        .unwrap()
        .observability_snapshot()
        .await
        .unwrap();
    assert_eq!(snapshot.budget_in_use, 0);
    assert_eq!(snapshot.in_use_total, 0);
}
#[tokio::test]
async fn end_during_open_retains_worker_until_late_binding_is_cleaned() {
    let test = TestAppState::with_resource_provider(MockDriverOptions::default()).await;
    test.save_connection("wapp-cleanup").await;
    let instance = instance(&test).await;
    let gate = instance.gate.lock().await;
    instance.end();
    spawn_cleanup(uuid::Uuid::new_v4().to_string(), instance.clone());
    tokio::time::sleep(Duration::from_millis(20)).await;
    let binding = open(&instance, Duration::from_secs(60)).await;
    // Production open checks ended after binding and revokes a late acquisition.
    assert!(instance.check().is_err());
    instance.sessions.revoke(
        &binding.reference,
        instance.context.client_instance_id.as_str(),
    );
    drop(gate);
    cleaned(&instance, &binding).await;
}
