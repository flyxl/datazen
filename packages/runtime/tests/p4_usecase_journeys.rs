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
    collections::{BTreeMap, HashMap},
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
    delegation: Mutex<Option<DelegationGrant>>,
    target_checks: AtomicU64,
    revoke_on_target_check: AtomicU64,
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
        delegation: datazen_platform_api::context::DelegationRef,
    ) -> Result<DelegationGrant, PortError> {
        let grant = lock(&self.delegation)
            .clone()
            .ok_or(PortError::TokenInvalid)?;
        if grant.delegation != delegation {
            return Err(PortError::TokenInvalid);
        }
        Ok(grant)
    }
    async fn resolve_delegation(&self, _: &RequestContext) -> Result<DelegationGrant, PortError> {
        lock(&self.delegation)
            .clone()
            .ok_or(PortError::TokenInvalid)
    }
    async fn authorize_target(
        &self,
        context: &RequestContext,
        subject: AuthorizationSubject,
        action: AuthorizationAction,
        target: &ExecutionTarget,
        command: &str,
        _: &serde_json::Value,
    ) -> Result<AuthorizationDecision, PortError> {
        if context.is_delegated() {
            let check = if command == "open_session" {
                0
            } else {
                self.target_checks.fetch_add(1, Ordering::SeqCst) + 1
            };
            if check != 0 && self.revoke_on_target_check.load(Ordering::SeqCst) == check {
                self.allow.store(false, Ordering::SeqCst);
            }
            let grant = lock(&self.delegation)
                .clone()
                .ok_or(PortError::TokenInvalid)?;
            if grant.delegation.scope
                != format!(
                    "database:{}",
                    target.namespace.database.as_deref().unwrap_or_default()
                )
            {
                return Err(PortError::TokenInvalid);
            }
        }
        self.authorize(context, subject, action).await
    }
    fn isolation_key(&self, _: &RequestContext) -> Result<PolicyIsolationKey, PortError> {
        Ok(PolicyIsolationKey::new("test"))
    }
    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResourceEvent {
    Opened(String, String),
    OpenRejected(String),
    Executed(String, String),
    Closed(String),
}
#[derive(Default)]
struct Backend {
    contexts: Mutex<HashMap<String, rt::SessionContext>>,
    epochs: Mutex<HashMap<String, u64>>,
    transactions: Mutex<HashMap<String, rt::SessionHandleRef>>,
    unknown_commit: AtomicBool,
    sink: Mutex<Option<RuntimeResultSink>>,
    ids: Mutex<Vec<ExecutionId>>,
    cancelled: Mutex<Vec<ExecutionId>>,
    opened: AtomicU64,
    closed: AtomicU64,
    fail_open: AtomicBool,
    cancel_unsupported: AtomicBool,
    journal: Mutex<Vec<ResourceEvent>>,
    started: Notify,
    release: Notify,
}
#[async_trait]
impl SessionBackend for Backend {
    async fn open(&self, r: OpenResource) -> Result<OpenedResource, rt::ProviderError> {
        let database = r.initial_target.database.clone();
        if self.fail_open.load(Ordering::SeqCst) {
            lock(&self.journal).push(ResourceEvent::OpenRejected(database));
            return Err(rt::ProviderError::SessionLost("injected".into()));
        }
        let id = format!("physical-{}", self.opened.fetch_add(1, Ordering::SeqCst));
        lock(&self.journal).push(ResourceEvent::Opened(id.clone(), database));
        let context = rt::SessionContext::new(r.initial_target, "observed-identity");
        lock(&self.contexts).insert(id.clone(), context.clone());
        lock(&self.epochs).insert(id.clone(), r.runtime_epoch);
        Ok(OpenedResource {
            context,
            capabilities: datazen_runtime::registry::CapabilityVersions::new("7", "2.3.4"),
            capability_snapshot: datazen_runtime::registry::backend::CapabilityProvenance {
                driver_id: "test".into(),
                driver_version: "2.3.4".into(),
                protocol_version: 7,
                capability_revision: 9,
                confirmed: BTreeMap::from([("statefulSession".into(), "supported".into())]),
            },
            resource_id: id,
            driver_supports_cancel: true,
        })
    }
    async fn execute(&self, r: ExecuteOnResource) -> Result<ResourceExecution, rt::ProviderError> {
        lock(&self.ids).push(r.execution_id.clone());
        let database = lock(&self.contexts)[&r.resource_id]
            .namespace
            .database
            .clone();
        lock(&self.journal).push(ResourceEvent::Executed(
            r.resource_id.clone(),
            database.clone(),
        ));
        r.cancel_handle_sink.publish("test-cancel");
        self.started.notify_one();
        let sink = lock(&self.sink).clone().unwrap();
        sink.publish_rows(
            &r.execution_id,
            serde_json::json!([{"name":"value"}]),
            vec![if r.command.command == "marker" {
                serde_json::json!([database, r.resource_id])
            } else {
                serde_json::json!([1])
            }],
        )
        .unwrap();
        if r.command.command == "hold-prefix" {
            sink.publish_rows(
                &r.execution_id,
                serde_json::json!([{"name":"value"}]),
                vec![serde_json::json!([2]), serde_json::json!([3])],
            )
            .unwrap();
        }
        if matches!(r.command.command.as_str(), "hold" | "hold-prefix") {
            self.release.notified().await;
        }
        let mut context = lock(&self.contexts).get(&r.resource_id).cloned().unwrap();
        if matches!(
            r.command.command.as_str(),
            "begin" | "platform.beginTransaction"
        ) {
            context.transaction_state = rt::TransactionState::Active;
        }
        if matches!(
            r.command.command.as_str(),
            "commit" | "platform.commitTransaction" | "platform.rollbackTransaction"
        ) {
            context.transaction_state = rt::TransactionState::None;
        }
        let handles = match r.command.command.as_str() {
            "platform.beginTransaction" => {
                let handle = rt::SessionHandleRef::new(
                    rt::HandleId::new(format!("tx-{}", r.execution_id)),
                    rt::HandleKind::Transaction,
                    rt::ResourceId::new(&r.resource_id),
                    rt::Counter::new(lock(&self.epochs)[&r.resource_id]),
                );
                lock(&self.transactions).insert(r.resource_id.clone(), handle.clone());
                vec![handle]
            }
            "platform.commitTransaction" | "platform.rollbackTransaction" => {
                lock(&self.transactions)
                    .remove(&r.resource_id)
                    .map(|mut h| {
                        h.closed = true;
                        vec![h]
                    })
                    .unwrap_or_default()
            }
            _ => vec![],
        };
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
            effect_outcome: if cancelled
                || r.command.command == "unknown"
                || (r.command.command == "platform.commitTransaction"
                    && self.unknown_commit.load(Ordering::SeqCst))
            {
                rt::EffectOutcome::Unknown
            } else if r.command.command == "platform.rollbackTransaction" {
                rt::EffectOutcome::RolledBack
            } else {
                rt::EffectOutcome::Completed
            },
            context_after: context,
            context_revision: r.expected_context_revision,
            handles,
            cancel_handle: "test-cancel".into(),
        })
    }
    async fn cancel(&self, r: CancelOnResource) -> Result<ResourceCancel, rt::ProviderError> {
        if self.cancel_unsupported.load(Ordering::SeqCst) {
            return Ok(ResourceCancel {
                disposition: rt::port::CancelDisposition::Unsupported,
                state: rt::ExecutionState::Running,
            });
        }
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
        lock(&self.journal).push(ResourceEvent::Closed(r.resource_id.clone()));
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
        delegation: Mutex::new(None),
        target_checks: AtomicU64::new(0),
        revoke_on_target_check: AtomicU64::new(0),
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
    let capability_snapshot = &view.provenance.as_ref().unwrap().capability_snapshot;
    assert_eq!(capability_snapshot.driver_id, "test");
    assert_eq!(capability_snapshot.driver_version, "2.3.4");
    assert_eq!(capability_snapshot.protocol_version, 7);
    assert_eq!(capability_snapshot.capability_revision.get(), 9);
    assert_eq!(
        capability_snapshot
            .confirmed
            .get("statefulSession")
            .and_then(serde_json::Value::as_str),
        Some("supported")
    );
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

// CM-08 / CM-15 / CM-16: exercise the public application boundary, not a
// fabricated UI receipt. The backend journal observes physical resource use.
#[tokio::test]
async fn cm08_cm15_cm16_replacement_journal_and_100_interleaved_reads() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let first = open(&service, &c, "first").await;
    let second = open(&service, &c, "second").await;
    let token = service
        .issue_submission_token(
            &c,
            IdempotentOperation::SetSessionContext,
            Some(&first.session.handle),
        )
        .await
        .unwrap();
    let request = SetSessionContextRequest::new(
        first.session.handle.clone(),
        first.session.context_revision,
        NamespaceTarget::database("b"),
        token.idempotency_key,
    );

    backend.fail_open.store(true, Ordering::SeqCst);
    assert!(service
        .set_session_context(&c, request.clone())
        .await
        .is_err());
    assert_eq!(backend.opened.load(Ordering::SeqCst), 2);
    assert_eq!(backend.closed.load(Ordering::SeqCst), 0);
    assert_eq!(lock(&backend.contexts).len(), 2);
    let still_live = execute(&service, &c, &first.session.handle, "marker").await;
    assert_eq!(
        terminal(&service, &c, &still_live.execution_id).await.state,
        ExecutionState::Succeeded
    );
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
    let replacement = service.set_session_context(&c, request).await.unwrap();
    assert_ne!(replacement.session.handle, first.session.handle);
    // Retirement makes the old ID unusable. The desktop explicitly closes it
    // after rebinding; this is the same release call as EditorSessionController.
    assert!(service
        .get_session(&c, first.session.handle.clone())
        .await
        .is_err());
    service
        .close_session(
            &c,
            CloseSessionRequest::new(first.session.handle, CloseMode::RequireNoTransaction),
        )
        .await
        .unwrap();
    assert_eq!(
        *lock(&backend.journal),
        vec![
            ResourceEvent::Opened("physical-0".into(), "a".into()),
            ResourceEvent::Opened("physical-1".into(), "a".into()),
            ResourceEvent::OpenRejected("b".into()),
            ResourceEvent::Executed("physical-0".into(), "a".into()),
            ResourceEvent::Opened("physical-2".into(), "b".into()),
            ResourceEvent::Closed("physical-0".into()),
        ]
    );
    assert_eq!(lock(&backend.contexts).len(), 2);
    for _ in 0..100 {
        for (handle, database, resource) in [
            (&replacement.session.handle, "b", "physical-2"),
            (&second.session.handle, "a", "physical-1"),
        ] {
            let receipt = execute(&service, &c, handle, "marker").await;
            let view = terminal(&service, &c, &receipt.execution_id).await;
            assert_eq!(view.state, ExecutionState::Succeeded);
            assert_eq!(
                view.provenance
                    .as_ref()
                    .unwrap()
                    .context_after
                    .namespace
                    .database
                    .as_deref(),
                Some(database)
            );
            let chunk = service
                .read_artifact_chunk(&c, view.artifact_ids[0].clone(), Counter::new(0))
                .await
                .unwrap();
            assert_eq!(chunk.rows, vec![serde_json::json!([database, resource])]);
            assert_eq!(
                service
                    .get_session(&c, handle.clone())
                    .await
                    .unwrap()
                    .observed_context
                    .namespace
                    .database
                    .as_deref(),
                Some(database)
            );
        }
    }
    assert_eq!(backend.opened.load(Ordering::SeqCst), 3);
    for handle in [replacement.session.handle, second.session.handle] {
        service
            .close_session(
                &c,
                CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
            )
            .await
            .unwrap();
    }
    assert_eq!(backend.closed.load(Ordering::SeqCst), 3);
    assert!(lock(&backend.contexts).is_empty());
}

// CM-36 / CM-61: the artifact stays readable after resource retirement but a
// runtime binding never migrates to the replacement physical connection.
#[tokio::test]
async fn cm36_cm61_replacement_invalidates_binding_but_preserves_artifact_and_provenance() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let first = open(&service, &c, "first").await;
    let receipt = execute(&service, &c, &first.session.handle, "marker").await;
    let before = terminal(&service, &c, &receipt.execution_id).await;
    assert!(before.runtime_binding.is_some());
    let artifact = before.artifact_ids[0].clone();
    let chunk = service
        .read_artifact_chunk(&c, artifact.clone(), Counter::new(0))
        .await
        .unwrap();
    let token = service
        .issue_submission_token(
            &c,
            IdempotentOperation::SetSessionContext,
            Some(&first.session.handle),
        )
        .await
        .unwrap();
    let replacement = service
        .set_session_context(
            &c,
            SetSessionContextRequest::new(
                first.session.handle.clone(),
                first.session.context_revision,
                NamespaceTarget::database("b"),
                token.idempotency_key,
            ),
        )
        .await
        .unwrap();
    service
        .close_session(
            &c,
            CloseSessionRequest::new(first.session.handle, CloseMode::RequireNoTransaction),
        )
        .await
        .unwrap();
    let after = service
        .get_execution(&c, receipt.execution_id)
        .await
        .unwrap();
    assert!(after.runtime_binding.is_none());
    assert_eq!(after.provenance, before.provenance);
    let recovered = service
        .read_artifact_chunk(&c, artifact, Counter::new(0))
        .await
        .unwrap();
    assert_eq!(recovered.rows, chunk.rows);
    assert_eq!(recovered.output, chunk.output);
    assert_eq!(recovered.source, chunk.source);
    assert_eq!(
        after
            .provenance
            .unwrap()
            .requested_target
            .namespace
            .database
            .as_deref(),
        Some("a")
    );
    assert_eq!(
        replacement
            .session
            .observed_context
            .namespace
            .database
            .as_deref(),
        Some("b")
    );
    assert_eq!(backend.closed.load(Ordering::SeqCst), 1);
}

// CM-72: an unsupported cancel is a control receipt, never a fabricated
// cancelled/rolledBack execution. Check the public serialized response too.
#[tokio::test]
async fn cm72_unsupported_cancel_does_not_change_terminal_effect_or_wire_shape() {
    let (service, backend, _, _) = setup();
    backend.cancel_unsupported.store(true, Ordering::SeqCst);
    let c = ctx("u", "c");
    let opened = open(&service, &c, "first").await;
    let receipt = execute(&service, &c, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    let cancel = service
        .cancel_execution(&c, receipt.execution_id.clone())
        .await
        .unwrap();
    assert_eq!(cancel.disposition, pa::CancelDisposition::Unsupported);
    let json = serde_json::to_value(&cancel).unwrap();
    assert_eq!(json["disposition"], "unsupported");
    assert!(json.get("accepted").is_none());
    assert!(json.get("effectOutcome").is_none());
    assert_eq!(
        service
            .get_execution(&c, receipt.execution_id.clone())
            .await
            .unwrap()
            .state,
        ExecutionState::Running
    );
    assert!(lock(&backend.cancelled).is_empty());
    backend.release.notify_one();
    let completed = terminal(&service, &c, &receipt.execution_id).await;
    assert_eq!(completed.state, ExecutionState::Succeeded);
    assert_eq!(completed.effect_outcome, EffectOutcome::Completed);
    let finished = service
        .cancel_execution(&c, receipt.execution_id)
        .await
        .unwrap();
    assert_eq!(finished.disposition, pa::CancelDisposition::AlreadyFinished);
    assert_eq!(
        serde_json::to_value(finished).unwrap()["disposition"],
        "alreadyFinished"
    );
}

// §12.1 / CM-55/56/64: consumer loss is modeled by deliberately not reading
// a published chunk. All bytes and terminal receipts come from the runtime's
// real artifact/event stores. Recovery does not submit the command again.
#[tokio::test]
async fn published_prefix_disconnect_cancel_recover_missing_chunk_and_execute_again() {
    let (service, backend, _, _) = setup();
    let c = ctx("u", "c");
    let opened = open(&service, &c, "first").await;
    let receipt = execute(&service, &c, &opened.session.handle, "hold-prefix").await;
    backend.started.notified().await;
    let running = service
        .get_execution(&c, receipt.execution_id.clone())
        .await
        .unwrap();
    assert_eq!(running.state, ExecutionState::Running);
    let artifact = running.artifact_ids[0].clone();
    let first = service
        .read_artifact_chunk(&c, artifact.clone(), Counter::new(0))
        .await
        .unwrap();
    assert_eq!(first.rows, vec![serde_json::json!([1])]);
    assert_eq!(first.published_chunk_count.get(), 2);
    let mut projection = std::collections::BTreeMap::new();
    projection.insert(first.chunk_index.get(), first.rows);
    // A reader vanishes with chunk 1 missing, then requests cancel. A successful
    // request remains separate from the driver's cancelled/unknown verdict.
    let cancel = service
        .cancel_execution(&c, receipt.execution_id.clone())
        .await
        .unwrap();
    assert_eq!(cancel.disposition, pa::CancelDisposition::Requested);
    let final_view = terminal(&service, &c, &receipt.execution_id).await;
    assert_eq!(final_view.state, ExecutionState::Cancelled);
    assert_eq!(final_view.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(
        final_view.result_completeness,
        datazen_platform_api::dto::execution::ResultCompleteness::Truncated
    );
    assert_eq!(final_view.artifact_ids, vec![artifact.clone()]);
    // Replaying the already displayed chunk and then fetching the missing
    // published chunk cannot duplicate rows or turn truncation into complete.
    for _ in 0..2 {
        let metadata = service
            .read_artifact_chunk(&c, artifact.clone(), Counter::new(0))
            .await
            .unwrap();
        for index in 0..metadata.published_chunk_count.get() {
            let chunk = service
                .read_artifact_chunk(&c, artifact.clone(), Counter::new(index))
                .await
                .unwrap();
            assert_eq!(chunk.source.execution_id, receipt.execution_id);
            projection.entry(index).or_insert(chunk.rows);
        }
    }
    assert_eq!(
        projection.into_values().flatten().collect::<Vec<_>>(),
        vec![
            serde_json::json!([1]),
            serde_json::json!([2]),
            serde_json::json!([3])
        ]
    );
    assert_eq!(
        lock(&backend.ids).as_slice(),
        &[receipt.execution_id.clone()]
    );
    // Unknown write outcome is not automatically replayed. An explicit fresh
    // read is permitted on the same retained editor resource.
    let next = execute(&service, &c, &opened.session.handle, "query").await;
    assert_eq!(
        terminal(&service, &c, &next.execution_id).await.state,
        ExecutionState::Succeeded
    );
    assert_ne!(next.execution_id, receipt.execution_id);
    assert_eq!(backend.opened.load(Ordering::SeqCst), 1);
    assert_eq!(lock(&backend.ids).len(), 2);
}

#[path = "fixtures/p4_delegation.rs"]
mod delegation_tests;

#[path = "fixtures/p4_workflow_job.rs"]
mod workflow_job;

#[path = "p4_usecase_journeys/export_output.rs"]
mod export_output;

#[path = "fixtures/p4_session_transactions.rs"]
mod session_transactions;
