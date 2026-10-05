use async_trait::async_trait;
use datazen_application::{dto::requests::*, error::ApiErrorCode, sessions::ConnectionUseCases};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::{
        execution::{EffectOutcome, ExecutionState},
        idempotency::IdempotentOperation,
        session as pa,
    },
    error::PortError,
    id::*,
    ports::{policy::*, profile::*},
    target::{ExecutionTarget, NamespaceTarget},
};
use datazen_runtime::{
    application::{RuntimeConnectionUseCases, RuntimeResultSink},
    connection as rt,
    registry::{backend::*, SessionBackend},
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::Notify;
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap()
}
struct Profiles {
    revision: AtomicU64,
}
#[async_trait]
impl ProfileRepository for Profiles {
    async fn list(
        &self,
        c: &RequestContext,
        _: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        Ok(vec![self.get(c, ConnectionId::new("p")).await?.unwrap()])
    }
    async fn get(
        &self,
        c: &RequestContext,
        id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        let mut p = ProfileRecord::new(c.organization_id.clone(), id, "test");
        p.config_revision = ConfigRevision::new(self.revision.load(Ordering::SeqCst));
        Ok(Some(p))
    }
    async fn create(
        &self,
        _: &RequestContext,
        _: datazen_platform_api::dto::profile::ProfileDraft,
        _: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn compare_and_set(
        &self,
        _: &RequestContext,
        _: ConnectionId,
        _: ConfigRevision,
        _: datazen_platform_api::dto::profile::ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn disable(
        &self,
        _: &RequestContext,
        _: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        unreachable!()
    }
    async fn advance_credential_revision(
        &self,
        _: &RequestContext,
        _: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        unreachable!()
    }
}
struct Policy {
    allow: AtomicBool,
}
#[async_trait]
impl PolicyService for Policy {
    async fn authorize(
        &self,
        _: &RequestContext,
        _: AuthorizationSubject,
        _: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(if self.allow.load(Ordering::SeqCst) {
            AuthorizationDecision::Allow
        } else {
            AuthorizationDecision::Deny {
                reason_code: "revoked",
            }
        })
    }
    async fn verify_delegation(
        &self,
        _: &RequestContext,
        _: datazen_platform_api::context::DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        unreachable!()
    }
    fn isolation_key(&self, _: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(PolicyIsolationKey::new("test"))
    }
    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}
#[derive(Default)]
struct Backend {
    contexts: Mutex<HashMap<String, rt::SessionContext>>,
    sink: Mutex<Option<RuntimeResultSink>>,
    ids: Mutex<Vec<ExecutionId>>,
    cancelled: Mutex<Vec<ExecutionId>>,
    opened: AtomicU64,
    closed: AtomicU64,
    fail_open: AtomicBool,
    started: Notify,
    release: Notify,
}
#[async_trait]
impl SessionBackend for Backend {
    async fn open(&self, r: OpenResource) -> Result<OpenedResource, rt::ProviderError> {
        if self.fail_open.load(Ordering::SeqCst) {
            return Err(rt::ProviderError::SessionLost("injected".into()));
        }
        let id = format!("physical-{}", self.opened.fetch_add(1, Ordering::SeqCst));
        let context = rt::SessionContext::new(r.initial_target, "observed-identity");
        lock(&self.contexts).insert(id.clone(), context.clone());
        Ok(OpenedResource {
            context,
            capabilities: datazen_runtime::registry::CapabilityVersions::new("test", "test"),
            resource_id: id,
            driver_supports_cancel: true,
        })
    }
    async fn execute(&self, r: ExecuteOnResource) -> Result<ResourceExecution, rt::ProviderError> {
        lock(&self.ids).push(r.execution_id.clone());
        r.cancel_handle_sink.publish("test-cancel");
        self.started.notify_one();
        let sink = lock(&self.sink).clone().unwrap();
        sink.publish_rows(
            &r.execution_id,
            serde_json::json!([{"name":"value"}]),
            vec![serde_json::json!([1])],
        )
        .unwrap();
        if r.command.command == "hold" {
            self.release.notified().await;
        }
        let mut context = lock(&self.contexts).get(&r.resource_id).cloned().unwrap();
        if r.command.command == "begin" {
            context.transaction_state = rt::TransactionState::Active;
        }
        if r.command.command == "commit" {
            context.transaction_state = rt::TransactionState::None;
        }
        lock(&self.contexts).insert(r.resource_id, context.clone());
        let cancelled = lock(&self.cancelled).contains(&r.execution_id);
        Ok(ResourceExecution {
            execution_id: r.execution_id.clone(),
            stream_id: StreamId::new(format!("actual-{}", r.execution_id)),
            state: if cancelled {
                rt::ExecutionState::Cancelled
            } else {
                rt::ExecutionState::Succeeded
            },
            effect_outcome: if cancelled || r.command.command == "unknown" {
                rt::EffectOutcome::Unknown
            } else {
                rt::EffectOutcome::Completed
            },
            context_after: context,
            context_revision: r.expected_context_revision,
            handles: Vec::new(),
            cancel_handle: "test-cancel".into(),
        })
    }
    async fn cancel(&self, r: CancelOnResource) -> Result<ResourceCancel, rt::ProviderError> {
        lock(&self.cancelled).push(r.execution_id);
        self.release.notify_one();
        Ok(ResourceCancel {
            disposition: rt::port::CancelDisposition::Requested,
            state: rt::ExecutionState::CancelRequested,
        })
    }
    async fn finalize_handles(
        &self,
        r: FinalizeHandles,
    ) -> Result<HandleFinalization, rt::ProviderError> {
        Ok(HandleFinalization {
            finalized: r.handles.len(),
            remaining: 0,
            effect_outcome: rt::EffectOutcome::RolledBack,
        })
    }
    async fn close(&self, r: CloseResource) -> Result<CloseResourceOutcome, rt::ProviderError> {
        self.closed.fetch_add(1, Ordering::SeqCst);
        lock(&self.contexts).remove(&r.resource_id);
        Ok(CloseResourceOutcome::Closed)
    }
}
fn ctx(user: &str, client: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org"),
        PrincipalId::new(user),
        None,
        ClientInstanceId::new(client),
        RequestId::new("request"),
        None,
    )
}
fn target(database: &str) -> ExecutionTarget {
    ExecutionTarget::new(ConnectionId::new("p"), NamespaceTarget::database(database))
}
fn setup() -> (
    RuntimeConnectionUseCases,
    Arc<Backend>,
    Arc<Profiles>,
    Arc<Policy>,
) {
    let backend = Arc::new(Backend::default());
    let profiles = Arc::new(Profiles {
        revision: AtomicU64::new(1),
    });
    let policy = Arc::new(Policy {
        allow: AtomicBool::new(true),
    });
    let service = RuntimeConnectionUseCases::new(profiles.clone(), policy.clone(), backend.clone());
    *lock(&backend.sink) = Some(service.result_sink());
    (service, backend, profiles, policy)
}
async fn open(
    service: &RuntimeConnectionUseCases,
    c: &RequestContext,
    editor: &str,
) -> pa::OpenSessionReceipt {
    let token = service
        .issue_submission_token(c, IdempotentOperation::OpenSession, None)
        .await
        .unwrap();
    service
        .open_session(
            c,
            OpenSessionRequest::new(
                target("a"),
                OwnerRef::Editor {
                    client_instance_id: c.client_instance_id.clone(),
                    editor_session_id: EditorSessionId::new(editor),
                },
                token.idempotency_key,
            ),
        )
        .await
        .unwrap()
}
async fn execute(
    service: &RuntimeConnectionUseCases,
    c: &RequestContext,
    h: &pa::SessionHandle,
    command: &str,
) -> datazen_platform_api::dto::execution::ExecutionReceipt {
    let token = service
        .issue_submission_token(c, IdempotentOperation::ExecuteInSession, Some(h))
        .await
        .unwrap();
    service
        .execute_in_session(
            c,
            ExecuteInSessionRequest::new(
                h.clone(),
                CommandCall::new(command, serde_json::json!({})),
                token.idempotency_key,
            ),
        )
        .await
        .unwrap()
}
async fn terminal(
    service: &RuntimeConnectionUseCases,
    c: &RequestContext,
    id: &ExecutionId,
) -> datazen_platform_api::dto::execution::ExecutionView {
    for _ in 0..1000 {
        let view = service.get_execution(c, id.clone()).await.unwrap();
        if view.state.is_terminal() {
            return view;
        }
        tokio::task::yield_now().await;
    }
    panic!("execution did not settle")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execution_receipt_driver_live_chunks_cancel_and_artifact_share_one_id() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "editor").await;
    let receipt = execute(&service, &c, &opened.session.handle, "hold").await;
    let subscription = service
        .subscribe_events(&c, receipt.stream_id.clone(), None)
        .await
        .unwrap();
    let events = tokio::task::spawn_blocking(move || subscription.collect::<Vec<_>>());
    backend.started.notified().await;
    let cancel = service
        .cancel_execution(&c, receipt.execution_id.clone())
        .await
        .unwrap();
    assert_eq!(cancel.execution_id, receipt.execution_id);
    assert_eq!(cancel.disposition, pa::CancelDisposition::Requested);
    let view = terminal(&service, &c, &receipt.execution_id).await;
    assert_eq!(view.state, ExecutionState::Cancelled);
    assert_eq!(view.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(lock(&backend.ids)[0], receipt.execution_id);
    assert_eq!(lock(&backend.cancelled)[0], receipt.execution_id);
    let events = events.await.unwrap();
    assert!(events.len() >= 4);
    assert!(events
        .windows(2)
        .all(|e| e[1].sequence.get() == e[0].sequence.get() + 1));
    let chunk = service
        .read_artifact_chunk(&c, view.artifact_ids[0].clone(), Counter::new(0))
        .await
        .unwrap();
    assert_eq!(chunk.source.execution_id, receipt.execution_id);
    assert_eq!(chunk.rows, vec![serde_json::json!([1])]);
    assert!(!chunk.complete);
    assert_eq!(chunk.published_chunk_count.get(), 1);
    assert!(service
        .get_execution(&ctx("other", "c"), receipt.execution_id.clone())
        .await
        .is_err());
    service
        .close_session(
            &c,
            CloseSessionRequest::new(opened.session.handle, CloseMode::RollbackAndClose),
        )
        .await
        .unwrap();
    assert!(service
        .get_execution(&c, receipt.execution_id)
        .await
        .unwrap()
        .runtime_binding
        .is_none());
}

#[tokio::test]
async fn replay_owner_epoch_attachment_and_independent_editor_journey() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let token = service
        .issue_submission_token(&c, IdempotentOperation::OpenSession, None)
        .await
        .unwrap();
    let request = OpenSessionRequest::new(
        target("a"),
        OwnerRef::Editor {
            client_instance_id: c.client_instance_id.clone(),
            editor_session_id: EditorSessionId::new("a"),
        },
        token.idempotency_key,
    );
    let first = service.open_session(&c, request.clone()).await.unwrap();
    assert_eq!(
        first,
        service.open_session(&c, request.clone()).await.unwrap()
    );
    assert_eq!(backend.opened.load(Ordering::SeqCst), 1);
    let mut changed = request;
    changed.initial_target = target("b");
    assert_eq!(
        service.open_session(&c, changed).await.unwrap_err().code,
        ApiErrorCode::IdempotencyConflict
    );
    assert!(service
        .get_session(&ctx("other", "c"), first.session.handle.clone())
        .await
        .is_err());
    let mut stale = first.session.handle.clone();
    stale.runtime_epoch = RuntimeEpoch::new("rte-99999999");
    assert_eq!(
        service.get_session(&c, stale).await.unwrap_err().code,
        ApiErrorCode::SessionLost
    );
    let attachment =
        AttachmentRequest::new(first.session.handle.clone(), first.attachment_token.clone());
    assert_eq!(
        service
            .detach_session(&c, attachment.clone())
            .await
            .unwrap()
            .attachment_state,
        pa::AttachmentState::Detached
    );
    assert_eq!(
        service
            .attach_session(&c, attachment)
            .await
            .unwrap()
            .attachment_state,
        pa::AttachmentState::Attached
    );
    let second = open(&service, &c, "b").await;
    assert_ne!(first.session.handle, second.session.handle);
}

#[tokio::test]
async fn replacement_rollback_revision_commit_and_replay() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let first = open(&service, &c, "a").await;
    let token = service
        .issue_submission_token(
            &c,
            IdempotentOperation::SetSessionContext,
            Some(&first.session.handle),
        )
        .await
        .unwrap();
    let mut request = SetSessionContextRequest::new(
        first.session.handle.clone(),
        Counter::new(99),
        NamespaceTarget::database("b"),
        token.idempotency_key,
    );
    assert_eq!(
        service
            .set_session_context(&c, request.clone())
            .await
            .unwrap_err()
            .code,
        ApiErrorCode::ContextConflict
    );
    request.expected_context_revision = service
        .get_session(&c, first.session.handle.clone())
        .await
        .unwrap()
        .context_revision;
    backend.fail_open.store(true, Ordering::SeqCst);
    assert!(service
        .set_session_context(&c, request.clone())
        .await
        .is_err());
    assert_eq!(
        service
            .get_session(&c, first.session.handle.clone())
            .await
            .unwrap()
            .observed_context
            .namespace
            .database
            .as_deref(),
        Some("a")
    );
    backend.fail_open.store(false, Ordering::SeqCst);
    let changed = service
        .set_session_context(&c, request.clone())
        .await
        .unwrap();
    assert_ne!(changed.session.handle, first.session.handle);
    assert_eq!(
        changed
            .session
            .observed_context
            .namespace
            .database
            .as_deref(),
        Some("b")
    );
    assert_eq!(
        changed,
        service.set_session_context(&c, request).await.unwrap()
    );
    assert!(service.get_session(&c, first.session.handle).await.is_err());
    let attachment =
        AttachmentRequest::new(changed.session.handle, changed.attachment_token.unwrap());
    assert!(service.attach_session(&c, attachment).await.is_ok());
}

#[tokio::test]
async fn explicit_target_executes_once_replays_and_releases_without_touching_editor() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let editor = open(&service, &c, "a").await;
    let token = service
        .issue_submission_token(&c, IdempotentOperation::ExecuteAtTarget, None)
        .await
        .unwrap();
    let request = ExecuteAtTargetRequest::new(
        target("b"),
        Counter::new(1),
        CommandCall::new("query", serde_json::json!({})),
        token.idempotency_key,
    );
    let receipt = service
        .execute_at_target(&c, request.clone())
        .await
        .unwrap();
    terminal(&service, &c, &receipt.execution_id).await;
    for _ in 0..100 {
        if backend.closed.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(backend.closed.load(Ordering::SeqCst), 1);
    assert_eq!(
        receipt,
        service.execute_at_target(&c, request).await.unwrap()
    );
    assert_eq!(backend.opened.load(Ordering::SeqCst), 2);
    assert_eq!(
        service
            .get_session(&c, editor.session.handle)
            .await
            .unwrap()
            .observed_context
            .namespace
            .database
            .as_deref(),
        Some("a")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsubscribe_unblocks_consumer_without_cancelling_running_execution() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "a").await;
    let receipt = execute(&service, &c, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    let stop = Arc::new(AtomicBool::new(false));
    let stream = service
        .subscribe_events_cancellable(&c, receipt.stream_id, None, stop.clone())
        .await
        .unwrap();
    let consuming = tokio::task::spawn_blocking(move || stream.collect::<Vec<_>>());
    stop.store(true, Ordering::Release);
    tokio::time::timeout(std::time::Duration::from_secs(1), consuming)
        .await
        .unwrap()
        .unwrap();
    assert!(lock(&backend.cancelled).is_empty());
    assert_eq!(
        service
            .get_execution(&c, receipt.execution_id.clone())
            .await
            .unwrap()
            .state,
        ExecutionState::Running
    );
    backend.release.notify_one();
    assert_eq!(
        terminal(&service, &c, &receipt.execution_id).await.state,
        ExecutionState::Succeeded
    );
}

#[tokio::test]
async fn known_completion_allows_next_execution_unknown_completion_requires_verification() {
    let (service, _, _, _) = setup();
    let c = ctx("u", "c");
    let session = open(&service, &c, "a").await;
    let first = execute(&service, &c, &session.session.handle, "query").await;
    terminal(&service, &c, &first.execution_id).await;
    let second = execute(&service, &c, &session.session.handle, "query").await;
    terminal(&service, &c, &second.execution_id).await;
    let unknown = execute(&service, &c, &session.session.handle, "unknown").await;
    assert_eq!(
        terminal(&service, &c, &unknown.execution_id)
            .await
            .effect_outcome,
        EffectOutcome::Unknown
    );
    let token = service
        .issue_submission_token(
            &c,
            IdempotentOperation::ExecuteInSession,
            Some(&session.session.handle),
        )
        .await
        .unwrap();
    assert!(service
        .execute_in_session(
            &c,
            ExecuteInSessionRequest::new(
                session.session.handle,
                CommandCall::new("unknown", serde_json::json!({})),
                token.idempotency_key
            )
        )
        .await
        .is_err());
}

#[tokio::test]
async fn active_transaction_close_rejection_config_rotation_and_cancel_terminal() {
    let (service, backend, profiles, _) = setup();
    let c = ctx("u", "c");
    let session = open(&service, &c, "a").await;
    let begin = execute(&service, &c, &session.session.handle, "begin").await;
    terminal(&service, &c, &begin.execution_id).await;
    assert_eq!(
        service
            .close_session(
                &c,
                CloseSessionRequest::new(
                    session.session.handle.clone(),
                    CloseMode::RequireNoTransaction
                )
            )
            .await
            .unwrap_err()
            .code,
        ApiErrorCode::TransactionResolutionRequired
    );
    assert_eq!(backend.closed.load(Ordering::SeqCst), 0);
    assert_eq!(
        service
            .cancel_execution(&c, begin.execution_id)
            .await
            .unwrap()
            .disposition,
        pa::CancelDisposition::AlreadyFinished
    );
    profiles.revision.store(2, Ordering::SeqCst);
    assert_eq!(
        service
            .get_session(&c, session.session.handle.clone())
            .await
            .unwrap_err()
            .code,
        ApiErrorCode::ConfigRevisionMismatch
    );
    let close = service
        .close_session(
            &c,
            CloseSessionRequest::new(session.session.handle.clone(), CloseMode::RollbackAndClose),
        )
        .await
        .unwrap();
    assert_eq!(close.resource_release, pa::ResourceRelease::Confirmed);
    assert_eq!(
        close,
        service
            .close_session(
                &c,
                CloseSessionRequest::new(session.session.handle, CloseMode::RollbackAndClose)
            )
            .await
            .unwrap()
    );
}
