//! Shared connection application implementation; transports inject repositories, policy and drivers.
mod backend;
mod convert;
mod operations;
mod results;
mod tokens;

use crate::{
    connection as rt,
    directory::{DbSessionIdGenerator, InMemorySessionDirectory},
    gateway::*,
    registry::{ContextReplacer, SessionBackend, SessionPort, SessionRegistry},
};
use datazen_application::error::{ApiError, ApiErrorCode};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::{execution::*, session::*},
    id::*,
    ports::{
        policy::{AuthorizationAction, AuthorizationSubject, PolicyService},
        profile::{ProfileRecord, ProfileRepository},
        session_directory::SessionDirectory,
    },
    target::ExecutionTarget,
};
use results::lock;
pub use results::{RuntimeArtifactChunk, RuntimeResultSink};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone)]
pub struct RuntimeConnectionUseCases {
    inner: Arc<Inner>,
}
struct Inner {
    profiles: Arc<dyn ProfileRepository>,
    policy: Arc<dyn PolicyService>,
    registry: Arc<SessionRegistry>,
    directory: Arc<InMemorySessionDirectory>,
    replacer: ContextReplacer,
    gateway: Arc<ExecutionGateway>,
    sink: RuntimeResultSink,
    physical: backend::PhysicalLedger,
    sessions: Mutex<HashMap<DbSessionId, SessionEntry>>,
    tokens: Mutex<HashMap<String, tokens::IssuedToken>>,
    keyring: TokenKeyring,
    clock: Arc<RuntimeClock>,
    receipts: tokio::sync::Mutex<HashMap<String, (serde_json::Value, serde_json::Value)>>,
}
#[derive(Clone)]
struct SessionEntry {
    ctx: RequestContext,
    owner: OwnerRef,
    initial: ExecutionTarget,
    handle: SessionHandle,
    attached: bool,
    closed: Option<CloseReceipt>,
}
struct RuntimeClock(Instant);
impl MonotonicClock for RuntimeClock {
    fn now_nanos(&self) -> u64 {
        self.0.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
}
struct BoundAuthorizer;
impl Authorizer for BoundAuthorizer {
    fn authorize(
        &self,
        p: &RequestPrincipal,
        action: GatewayAction,
        view: &rt::SessionView,
        source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial> {
        let valid = match &view.owner {
            rt::OwnerRef::Editor {
                organization_id,
                principal_id,
                ..
            } => organization_id == p.organization_id() && principal_id == p.principal_id(),
            rt::OwnerRef::ClientSession { .. } => {
                source.organization_id() == Some(p.organization_id())
                    && source.principal_id() == Some(p.principal_id())
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(AuthorizationDenial::new(action, "ownerMismatch"))
        }
    }
}
impl RuntimeConnectionUseCases {
    pub fn new(
        profiles: Arc<dyn ProfileRepository>,
        policy: Arc<dyn PolicyService>,
        backend: Arc<dyn SessionBackend>,
    ) -> Self {
        let sink = RuntimeResultSink::default();
        let physical = Arc::new(Mutex::new(HashMap::new()));
        let registry = Arc::new(SessionRegistry::new(
            Arc::new(backend::ObservedBackend {
                inner: backend,
                sink: sink.clone(),
                physical: physical.clone(),
            }),
            128,
        ));
        let directory = Arc::new(InMemorySessionDirectory::new());
        let clock = Arc::new(RuntimeClock(Instant::now()));
        let gateway = Arc::new(ExecutionGateway::new(
            registry.clone(),
            Arc::new(BoundAuthorizer),
            Arc::new(InMemoryIdempotencyStore::new()),
            clock.clone(),
        ));
        let secret = DbSessionIdGenerator::new()
            .generate()
            .as_str()
            .as_bytes()
            .to_vec();
        Self {
            inner: Arc::new(Inner {
                profiles,
                policy,
                replacer: ContextReplacer::new(registry.clone(), directory.clone()),
                registry,
                directory,
                gateway,
                sink,
                physical,
                sessions: Mutex::new(HashMap::new()),
                tokens: Mutex::new(HashMap::new()),
                keyring: TokenKeyring::single(secret),
                clock,
                receipts: tokio::sync::Mutex::new(HashMap::new()),
            }),
        }
    }
    pub async fn subscribe_events_cancellable(
        &self,
        ctx: &RequestContext,
        stream: StreamId,
        after: Option<Counter>,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<datazen_platform_api::ports::event::EventSubscription, ApiError> {
        self.authorize(ctx, AuthorizationAction::Query).await?;
        self.inner.sink.subscribe(ctx, &stream, after, Some(cancel))
    }
    pub fn result_sink(&self) -> RuntimeResultSink {
        self.inner.sink.clone()
    }
    pub async fn read_artifact_chunk(
        &self,
        ctx: &RequestContext,
        id: ArtifactId,
        index: Counter,
    ) -> Result<RuntimeArtifactChunk, ApiError> {
        self.authorize(ctx, AuthorizationAction::Query).await?;
        self.inner.sink.chunk(ctx, &id, index)
    }
    async fn authorize(
        &self,
        ctx: &RequestContext,
        action: AuthorizationAction,
    ) -> Result<(), ApiError> {
        // Delegations require a verified full grant; absent grants never fall back to caller permissions.
        if ctx.delegation_id.is_some() {
            return Err(ApiError::permission_denied(
                "verified delegation grant required",
            ));
        }
        let decision = self
            .inner
            .policy
            .authorize(
                ctx,
                AuthorizationSubject::Principal {
                    principal_id: ctx.principal_id.clone(),
                },
                action,
            )
            .await
            .map_err(convert::port)?;
        if decision.is_allowed() {
            Ok(())
        } else {
            Err(ApiError::permission_denied("connection permission denied"))
        }
    }
    async fn profile(
        &self,
        ctx: &RequestContext,
        id: &ConnectionId,
    ) -> Result<ProfileRecord, ApiError> {
        self.inner
            .profiles
            .get(ctx, id.clone())
            .await
            .map_err(convert::port)?
            .filter(|p| p.organization_id == ctx.organization_id && p.enabled)
            .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "connection unavailable"))
    }
    fn session_entry(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
    ) -> Result<SessionEntry, ApiError> {
        let entries = lock(&self.inner.sessions);
        let entry = entries
            .get(&handle.db_session_id)
            .filter(|e| {
                e.ctx.organization_id == ctx.organization_id
                    && e.ctx.principal_id == ctx.principal_id
                    && e.ctx.client_instance_id == ctx.client_instance_id
            })
            .ok_or_else(|| ApiError::new(ApiErrorCode::SessionNotFound, "session unavailable"))?;
        if entry.handle != *handle {
            return Err(ApiError::new(
                ApiErrorCode::SessionLost,
                "runtime epoch no longer matches",
            ));
        }
        Ok(entry.clone())
    }
    async fn session_view(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
    ) -> Result<SessionView, ApiError> {
        let entry = self.session_entry(ctx, handle)?;
        if entry.closed.is_some() {
            return Err(ApiError::new(
                ApiErrorCode::SessionLost,
                "session has been released",
            ));
        }
        self.inner
            .directory
            .resolve(handle)
            .map_err(convert::port)?;
        let profile = self.profile(ctx, &entry.initial.connection_id).await?;
        let view = self
            .inner
            .registry
            .session_view(&convert::handle(handle)?)
            .await
            .map_err(convert::runtime)?;
        if profile.config_revision.counter().get() != view.config_revision.get() {
            self.inner.sink.invalidate(handle);
            return Err(ApiError::new(
                ApiErrorCode::ConfigRevisionMismatch,
                "connection configuration changed",
            ));
        }
        Ok(convert::session(
            view,
            entry.owner,
            entry.initial,
            entry.attached,
        ))
    }
    fn source(ctx: &RequestContext, entry: &SessionEntry) -> ExecutionSource {
        let (kind, id) = match &entry.owner {
            OwnerRef::Editor {
                editor_session_id, ..
            } => (SourceKind::Editor, editor_session_id.as_str().to_owned()),
            _ => (
                SourceKind::ClientSession,
                ctx.client_instance_id.as_str().to_owned(),
            ),
        };
        ExecutionSource::new(
            kind,
            id,
            Some(ctx.organization_id.clone()),
            Some(ctx.principal_id.clone()),
        )
    }
    fn principal(ctx: &RequestContext, handle: &SessionHandle) -> RequestPrincipal {
        RequestPrincipal::new(
            ctx.principal_id.clone(),
            ctx.organization_id.clone(),
            handle.db_session_id.clone(),
        )
    }
    async fn create_session(
        &self,
        ctx: &RequestContext,
        target: ExecutionTarget,
        owner: OwnerRef,
    ) -> Result<OpenSessionReceipt, ApiError> {
        let profile = self.profile(ctx, &target.connection_id).await?;
        let rt_owner = convert::owner(ctx, &target.connection_id, &owner)?;
        let id = DbSessionIdGenerator::new().generate();
        let view = self
            .inner
            .registry
            .register_session(crate::registry::OpenRequest {
                db_session_id: id.clone(),
                worker_id: WorkerId::new("local-runtime"),
                connection_id: target.connection_id.clone(),
                config_revision: rt::ConfigRevision::new(profile.config_revision.counter().get()),
                owner: rt_owner,
                initial_target: convert::target(&target)?,
                expires_at: Timestamp::new(""),
                idle_deadline_ms: None,
            })
            .await
            .map_err(convert::runtime)?;
        let handle = convert::public_handle(&view.handle);
        let registration = self
            .inner
            .directory
            .register(
                datazen_platform_api::ports::session_directory::SessionOwner {
                    db_session_id: id.clone(),
                    organization_id: ctx.organization_id.clone(),
                    principal_id: ctx.principal_id.clone(),
                    connection_id: target.connection_id.clone(),
                    owner: owner.clone(),
                    worker_id: WorkerId::new("local-runtime"),
                    runtime_epoch: handle.runtime_epoch.clone(),
                    resource_epoch: view.handle.runtime_epoch.get(),
                    last_business_activity: Timestamp::new("1970-01-01T00:00:00.000Z"),
                },
            )
            .await;
        if let Err(error) = registration {
            let _ = self
                .inner
                .registry
                .close_registered(&view.handle, rt::CloseMode::RollbackAndClose)
                .await;
            return Err(convert::port(error));
        }
        let token = match self.inner.directory.issue_attachment_token(&handle) {
            Ok(token) => token,
            Err(_) => {
                let _ = self
                    .inner
                    .registry
                    .close_registered(&view.handle, rt::CloseMode::RollbackAndClose)
                    .await;
                return Err(ApiError::new(
                    ApiErrorCode::OutcomeUnknown,
                    "attachment publication failed",
                ));
            }
        };
        lock(&self.inner.sessions).insert(
            id,
            SessionEntry {
                ctx: ctx.clone(),
                owner: owner.clone(),
                initial: target.clone(),
                handle: handle.clone(),
                attached: true,
                closed: None,
            },
        );
        Ok(OpenSessionReceipt {
            session: convert::session(view, owner, target, true),
            attachment_token: token,
        })
    }
}
