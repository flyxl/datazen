//! Shared connection application implementation; transports inject repositories, policy and drivers.
mod backend;
mod convert;
mod idle;
mod live;
mod operations;
mod results;
mod tokens;

pub use idle::{IdlePolicy, IdleReclaimer, IdleReclamation};
pub use live::{
    LiveConnection, LiveConnectionError, LiveConnectionLease, LiveConnectionLedger,
    LiveConnectionRequest, LiveConnectionSource, LiveConnections, LivePurpose, LiveRelease,
};

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
    /// 出向端口：宿主取活连接的唯一入口。未接线是配置事实，不是故障。
    live: Mutex<LiveConnections>,
    /// 空闲回收策略。`deadline == None` 表示本部署不做空闲回收。
    idle: Mutex<IdlePolicy>,
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
impl RuntimeClock {
    /// 单调毫秒。与 `OpenRequest::idle_deadline_ms` / `evict_idle_at(at_ms)` 共用同一把
    /// 尺子——两处单位不一致会让「到期」变成一个随机器负载漂移的量。
    fn now_ms(&self) -> u64 {
        self.now_nanos() / 1_000_000
    }
}
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
                live: Mutex::new(LiveConnections::unwired()),
                idle: Mutex::new(IdlePolicy::default()),
                receipts: tokio::sync::Mutex::new(HashMap::new()),
            }),
        }
    }
    /// 接线活连接出口端口。
    ///
    /// 与 [`Self::new`] 分开而不是塞进参数：端口是**宿主装配期**决定的事，而 `new` 的
    /// 三个参数是业务依赖。分开之后既有的调用方与既有测试一行都不用改。
    /// 重复调用是**覆盖**而不是叠加——门面只有一个，台账也是同一个。
    pub fn with_live_connections(&self, source: Arc<dyn LiveConnectionSource>) -> &Self {
        *lock(&self.inner.live) = LiveConnections::wired(source);
        self
    }
    /// 覆盖空闲回收策略。默认是 §9.2 的业务空闲 30 分钟。
    pub fn with_idle_policy(&self, policy: IdlePolicy) -> &Self {
        *lock(&self.inner.idle) = policy;
        self
    }
    /// 当前空闲策略（只读投影）。
    pub fn idle_policy(&self) -> IdlePolicy {
        *lock(&self.inner.idle)
    }
    /// 本次建会话要写入的空闲期限（绝对单调毫秒）。策略关闭时为 `None`。
    fn idle_deadline_ms(&self) -> Option<u64> {
        let policy = *lock(&self.inner.idle);
        policy.deadline_ms().map(|span| {
            let at = self.inner.clock.now_ms().saturating_add(span);
            // 期限早于扫描时刻就意味着「一开就到期」。截到扫描时刻而不是 `None`：
            // 用「未设期限」来表达「已经到期」会让驱逐器永远够不着它。
            at.max(self.inner.clock.now_ms())
        })
    }
    /// 活连接端口是否已接线。
    pub fn has_live_connections(&self) -> bool {
        lock(&self.inner.live).is_wired()
    }
    /// 未归还的活连接借用总数。只读投影。
    pub fn live_connection_borrows(&self) -> usize {
        lock(&self.inner.live).outstanding()
    }
    /// 活连接借用台账行数。只读投影。
    pub fn live_connection_rows(&self) -> usize {
        lock(&self.inner.live).rows()
    }
    /// registry 在册会话 id（只读投影）。异常路径「没留下登记行」就靠它断言。
    pub fn registered_session_ids(&self) -> Vec<crate::connection::DbSessionId> {
        self.inner.registry.registered_ids()
    }
    /// registry 是否仍登记着这个会话。
    pub fn is_registered(&self, id: &crate::connection::DbSessionId) -> bool {
        self.inner.registry.is_registered(id)
    }
    /// 应用层 `sessions` 表里**仍然开着**的行数（只读投影）。
    ///
    /// 与 [`Self::registered_session_ids`] **分列**，因为「registry 摘了但这张表还在」
    /// 正是只做驱逐不做应用层清理时的确切症状；两个数一起看才能定位泄漏在哪一侧。
    ///
    /// 注意计数口径：这张表在 `close_session` 之后**保留墓碑行**（`entry.closed` 记着
    /// 关闭回执，好让重复关闭直接回放同一张回执），因此这里数的是 `closed == None`
    /// 的行。墓碑不是登记——会话已不在 registry、不在目录、不在活连接台账里，
    /// 也不可再被执行（见 §3 INV-10 关闭幂等）。
    pub fn tracked_session_rows(&self) -> usize {
        lock(&self.inner.sessions)
            .values()
            .filter(|entry| entry.closed.is_none())
            .count()
    }
    /// 目录里是否还挂着这个会话的附件（只读投影）。
    pub fn is_attached(&self, handle: &SessionHandle) -> bool {
        self.inner.directory.resolve(handle).is_ok()
    }
    /// 借一个**活连接**：宿主执行网关取得 `(Arc<dyn DatabaseDriver>, ConnectionHandle)` 的
    /// 唯一正路。
    ///
    /// 前置检查与执行路径同源（授权 → 会话视图 → 会话登记行 → epoch 校验），因此借出成功
    /// 蕴含「该会话此刻仍然登记着」；此后才向端口要引用。返回值是借用凭证，必须归还——
    /// 显式 [`LiveConnectionLease::release`] 或直接丢弃，两条路等价。
    pub async fn borrow_live_connection(
        &self,
        ctx: &RequestContext,
        handle: &SessionHandle,
        purpose: LivePurpose,
    ) -> Result<LiveConnectionLease, ApiError> {
        self.authorize(ctx, AuthorizationAction::Query).await?;
        let view = self.session_view(ctx, handle).await?;
        self.session_entry(ctx, handle)?;
        // 先把门面**拷出来**再 await：锁守卫不可跨 await 点持有。
        let live = lock(&self.inner.live).clone();
        live.borrow(LiveConnectionRequest {
            handle: convert::handle(handle)?,
            connection_id: view.connection_id,
            purpose,
        })
        .await
        .map_err(convert::live)
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
                // 绝对单调毫秒，与 `reclaim_idle` 的 `at_ms` 同尺。`None` 表示本部署
                // 不做空闲回收（配置事实），不是「忘了配」。
                idle_deadline_ms: self.idle_deadline_ms(),
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
