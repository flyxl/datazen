//! WR-G1 Wave6 第二刀重跑：CM-68「候选替换发布完整面」W1 断言（重做版）。
//!
//! 此前一版 993 行实现因沙箱回滚丢失；本文件按上版登记的断言账目重建，
//! 测试名与语义环节逐条保持一致。上版账目登记「全部 server 可达」，本版
//! 逐一核对内核锚点后落笔（锚点清单见下方「内核锚点」节）。
//!
//! ## 断言账目表（代码即账本：scenario × 语义环节 × 可达面）
//!
//! | # | 测试 | 语义环节 | 可达面 |
//! |---|------|----------|--------|
//! | 1 | `cm68_a_committed_replacement_publishes_the_candidate_and_retires_the_old_session_id` | 两阶段候选→提交发布；回执携带新会话+被替换旧 id+新 attachmentToken；提交后旧 id 404 固定文案、新会话立即可路由 | 真路由 oneshot：`build_router_with_auth` + `sessions_router` + 真 SQLite 文件库 + 真登录会话行/CSRF；内核 `ContextReplacer` 在用例内一次走完 Prepared→Committed |
//! | 2 | `cm68_a_stale_expected_context_revision_conflicts_and_leaves_the_old_session_serving` | `expectedContextRevision` 不匹配→`contextConflict` 固定文案、不切换 | 用例面（`LeaseConnectionUseCases` 租约委托）+ `status_for` 的 409 wire 映射 |
//! | 3 | `cm68_the_receipt_token_binds_only_the_published_candidate` | 旧令牌打新会话被拒（`contextConflict`+"attachment token rejected"）、新令牌 attach 成功且失败尝试不烧绑定 | 用例面（目录 attach 摘要闸 `authorize_attachment`） |
//! | 4 | `cm68_a_lost_response_replayed_with_the_same_key_returns_the_same_receipt` | 同键重放返回同一份回执（同 session/token/replacedSessionId）；回放令牌真实作数 | 用例面直调（owner 内存回执表 + `replayed_token`） |
//! | 5 | `cm68_a_replay_with_a_changed_fingerprint_is_an_idempotency_conflict_not_a_second_replacement` | `IdempotencyConflict` 无副作用 | 用例面直调（指纹冲突分支） |
//! | 6 | `cm68_a_prepared_candidate_is_not_routable_until_the_commit_switches_routing` | prepared 不可路由+旧条目保持服务；Committed 同一临界区 promote+关闭旧路由 | `ServerSessionDirectory`（`SessionDirectory` 端口，单实例 v1） |
//! | 7 | `cm68_a_commit_without_a_prepared_candidate_is_refused_instead_of_invented` | 无 prepared 的 Committed→`NotFound`（不凭空发明候选） | `ServerSessionDirectory` 端口 |
//! | 8 | `cm68_a_rolled_back_replacement_keeps_the_old_session_and_never_publishes_the_candidate` | RolledBack 旧会话原状、候选销毁；迟到 Committed 不能复活 | `ServerSessionDirectory` 端口 |
//!
//! ## 不可达面登记（沿用上版划分，非本文件职责）
//!
//! - **ResourceManager / ReplacementLedger 桌面宿主形状**（`begin/confirm/
//!   publish/abandon`，`crates/datazen/src/execution/runtime/resource/`）没有 server
//!   入口——该形状由 modules 内嵌测试钉住，不在此重复。
//! - **prepared 屏障中途观测**：`InMemorySessionDirectory::take_lost_commit_reply`
//!   是 `pub(crate)`，W1 无法从 server 表达「应答丢失→屏障→按 key 查」；
//!   屏障语义归 modules 内嵌测试。
//! - **WN 多实例共享目录**归 P9（`ServerSessionDirectory` 模块头已登记单进程 v1）。
//! - **W1 持久化投影侧**（§3.5 落库时点、`uq_idempotency_live_subject`、§6.3
//!   profile CAS、§7.2 未终结幂等记录行）归 CM-70 的 W1 断言，不入本文件
//!   （persistence-model.md §9 的 CM-68 行）。
//! - **HTTP 同键重放门**：已替换旧 id 的同键重放在 HTTP 上先撞
//!   `ensure_visible` 的 404 固定文案（存在性不泄漏优先），幂等回执重放只在
//!   用例面可达（测试 4/5）——按现状登记，测试 1 附带钉住这一门。
//!
//! ## 内核锚点（重读核对）
//!
//! `orchestration/operations.rs`·`set_session_context`（回执回放/指纹冲突分支）、
//! `orchestration/tokens.rs`·`issue`/`validate_token`、`orchestration/convert.rs`·
//! `runtime`（固定文案）、`registry/context.rs`·`ContextReplacer::replace`/
//! `replayed_token`、`registry/actor.rs`·open（contextRevision=1、Ready）、
//! `runtime/directory/directory.rs`·`do_commit_replacement`/`expect_prepared`、
//! `runtime/directory/lifecycle.rs`·`commit_status`/`attach_from_client`、
//! `runtime/directory/attachment.rs`·`authorize_attachment`（AlreadyAttached 幂等）、
//! `runtime/connection/error.rs`·`api_code`、`session_directory.rs`·
//! `commit_replacement`/`lookup`、`http/domains/sessions.rs`·
//! `handler_set_session_context`/`ensure_visible`、`http/response.rs`·`status_for`。

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use tower::ServiceExt;

use datazen::connection::domain::{NamespaceTarget, ProfileRepository};
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::orchestration::{
    RuntimeConnectionUseCases, RuntimeResultSink, SystemRuntimeClock,
};
use datazen::execution::application::policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, PolicyChangeStream,
    PolicyService as ModulesPolicyService, PolicyVersionChange,
};
use datazen::execution::application::requests::{
    AttachmentRequest, OpenSessionRequest, SetSessionContextRequest,
};
use datazen::execution::application::use_cases::ConnectionUseCases;
use datazen::execution::runtime::connection as rc;
use datazen::execution::runtime::directory::InMemorySessionDirectory;
use datazen::execution::runtime::gateway::MonotonicClock;
use datazen::execution::runtime::registry::audit::CapabilityVersions;
use datazen::execution::runtime::registry::backend::{
    CancelOnResource, CapabilityProvenance, CloseResource, CloseResourceOutcome, ExecuteOnResource,
    FinalizeHandles, HandleFinalization, OpenResource, OpenedResource, ResourceCancel,
    ResourceExecution, SessionBackend,
};
use datazen::execution::{
    AttachmentState, ContextChangeReceipt, DbSessionId, ExecutionTarget, IdempotentOperation,
    OpenSessionReceipt, ReplacementCommit, ReplacementOperation, SessionDirectory, SessionHandle,
    SessionOwner, SessionState, SessionView,
};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::{ApiErrorCode, PortError};
use datazen::shared::id::{
    AuthenticationSessionId, ClientInstanceId, ConnectionId, Counter, IdempotencyKey,
    OrganizationId, PrincipalId, RequestId, RuntimeEpoch, Timestamp, WorkerId,
};

use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::execution::lease::RuntimeLeaseSource;
use datazen_server::execution::lease_use_cases::LeaseConnectionUseCases;
use datazen_server::execution::session_directory::ServerSessionDirectory;
use datazen_server::http::domains::sessions::{sessions_router, SessionsState};
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::http::response::status_for;
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::{AdminStore, LoginSessions, ServerProfiles};

const NOW: &str = "2026-01-01T00:00:00.000Z";
/// 真实时钟戳（g2 冻结项先例）：登录会话的过期面以真实时钟判定，
/// 硬编码日期会随真实时间腐烂成 401。
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn future_timestamp(hours: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

const SERVER_SECRET: &[u8] = b"wr-g1-cm68-server-secret-32-bytes!";
const LOGIN_SESSION_ID: &str = "sess-cm68-full";

// ── 公共装配（沿用 g1_w1_gaps.rs 的 server 测试约定）──────────────────────

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("store");
    store.migrate().expect("migrate");
    Arc::new(store)
}

fn ctx(org: &str, principal: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(org),
        PrincipalId::new(principal),
        Some(AuthenticationSessionId::new("auth-1")),
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

/// 与 HTTP `SessionIdentity` 同形的调用方身份（router.rs 的提取器把
/// clientInstanceId 盖章为 "http"、登录会话半边留空）——直调签发面时必须
/// 与 HTTP 调用逐字段一致，`validate_token` 的 scope 核对才放行。
fn http_ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("http"),
        RequestId::new("req-http"),
        None,
    )
}

/// open_session 的归属盖章：ClientSession 归属的 clientInstanceId 必须与
/// ctx 一致（`convert::owner` → `check_owner` 的可采信裁决）。
fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-1"),
        purpose: "g1-cm68-full".into(),
    }
}

/// `ProfileRepository` 落库：真实 `ServerProfiles`。
async fn create_profile(store: &Arc<AdminStore>, ctx: &RequestContext, name: &str) -> ConnectionId {
    let profiles = ServerProfiles::new(store.clone());
    let record = profiles
        .create(
            ctx,
            ProfileDraft::new(name, "postgres"),
            &IdempotencyKey::new("idem-cm68"),
        )
        .await
        .expect("profile create");
    record.connection_id
}

/// modules `PolicyService` 假体：全允许（授权裁决归 f7 真适配器钉子；本文件
/// 钉的是替换发布语义，不是授权矩阵）。
struct AllowAllPolicy;

#[async_trait]
impl ModulesPolicyService for AllowAllPolicy {
    async fn authorize(
        &self,
        _ctx: &RequestContext,
        _subject: AuthorizationSubject,
        _action: AuthorizationAction,
    ) -> Result<AuthorizationDecision, PortError> {
        Ok(AuthorizationDecision::Allow)
    }

    async fn verify_delegation(
        &self,
        _ctx: &RequestContext,
        _delegation: datazen::shared::context::DelegationRef,
    ) -> Result<datazen::execution::application::policy::DelegationGrant, PortError> {
        Err(PortError::TokenInvalid)
    }

    fn isolation_key(
        &self,
        _ctx: &RequestContext,
    ) -> Result<datazen::shared::id::PolicyIsolationKey, PortError> {
        Ok(datazen::shared::id::PolicyIsolationKey::new("g1-cm68-full"))
    }

    fn subscribe_version_changes(&self) -> PolicyChangeStream {
        Box::new(std::iter::empty::<PolicyVersionChange>())
    }
}

/// `SessionBackend` 假体：会话登记/关闭全程无物理驱动（与 `g1_w1_gaps.rs`
/// 的 StubBackend 同形状）。
struct StubBackend;

#[async_trait]
impl SessionBackend for StubBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        Ok(OpenedResource {
            context: rc::SessionContext::new(request.initial_target, "stub"),
            capabilities: CapabilityVersions {
                contract: "1".into(),
                driver_api: "1".into(),
            },
            capability_snapshot: CapabilityProvenance::new("stub-driver", "0.0.0", 1),
            resource_id: "res_stub".into(),
            driver_supports_cancel: false,
        })
    }
    async fn execute(
        &self,
        _request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        Err(rc::ProviderError::HostRejected("unexpectedExecute".into()))
    }
    async fn cancel(
        &self,
        _request: CancelOnResource,
    ) -> Result<ResourceCancel, rc::ProviderError> {
        Err(rc::ProviderError::HostRejected("unexpectedCancel".into()))
    }
    async fn finalize_handles(
        &self,
        _request: FinalizeHandles,
    ) -> Result<HandleFinalization, rc::ProviderError> {
        Ok(HandleFinalization {
            finalized: 0,
            remaining: 0,
            effect_outcome: rc::EffectOutcome::NotStarted,
        })
    }
    async fn close(
        &self,
        _request: CloseResource,
    ) -> Result<CloseResourceOutcome, rc::ProviderError> {
        Ok(CloseResourceOutcome::Closed)
    }
}

/// 测试专用租约源：恒返回**同一** runtime 实例（会话语义的前置；主线
/// 每调用现构造的装配偏差见 g1_w1_gaps.rs 文件头）。
struct FixedLeaseSource(Arc<RuntimeConnectionUseCases>);

#[async_trait]
impl RuntimeLeaseSource for FixedLeaseSource {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        Ok(self.0.clone())
    }
}

/// 一个真实装配的 runtime（真 `ServerProfiles` 落库 + 全允许策略 + 桩后端；
/// 目录实例由 runtime 自建）。
fn session_runtime(store: Arc<AdminStore>) -> Arc<RuntimeConnectionUseCases> {
    let profiles: Arc<dyn ProfileRepository> = Arc::new(ServerProfiles::new(store));
    let backend: Arc<dyn SessionBackend> = Arc::new(StubBackend);
    Arc::new(RuntimeConnectionUseCases::new(
        profiles,
        Arc::new(AllowAllPolicy),
        backend,
    ))
}

/// 共享目录形态：bootstrap 装配纪律的测试版——runtime 与 sessions 域
/// 可见性闸持有**同一** `InMemorySessionDirectory`（`new_with_parts` 的
/// 第六参把共享目录注入用例）。
fn shared_runtime(
    store: Arc<AdminStore>,
    directory: Arc<InMemorySessionDirectory>,
) -> Arc<RuntimeConnectionUseCases> {
    let profiles: Arc<dyn ProfileRepository> = Arc::new(ServerProfiles::new(store));
    let backend: Arc<dyn SessionBackend> = Arc::new(StubBackend);
    Arc::new(RuntimeConnectionUseCases::new_with_parts(
        profiles,
        Arc::new(AllowAllPolicy),
        backend,
        Arc::new(SystemRuntimeClock(std::time::Instant::now())) as Arc<dyn MonotonicClock>,
        RuntimeResultSink::default(),
        directory,
    ))
}

fn session_cases(store: Arc<AdminStore>) -> LeaseConnectionUseCases {
    LeaseConnectionUseCases::new(Arc::new(FixedLeaseSource(session_runtime(store))))
}

/// 走完「签发提交令牌 → 构造 open 请求」的前置（open 的 validate_token
/// 要求幂等键来自本 runtime 的签发面）。
async fn open_session(
    cases: &LeaseConnectionUseCases,
    ctx: &RequestContext,
    connection_id: &str,
) -> OpenSessionReceipt {
    let token = cases
        .issue_submission_token(ctx, IdempotentOperation::OpenSession, None)
        .await
        .expect("open submission token");
    let target = ExecutionTarget::new(
        ConnectionId::new(connection_id),
        NamespaceTarget::database("app"),
    );
    cases
        .open_session(
            ctx,
            OpenSessionRequest::new(target, owner(), token.idempotency_key),
        )
        .await
        .expect("open session")
}

/// 签发 `setSessionContext` 的提交令牌（会话绑定操作必须有 handle）。
async fn issue_context_token(
    cases: &LeaseConnectionUseCases,
    ctx: &RequestContext,
    handle: &SessionHandle,
) -> IdempotencyKey {
    cases
        .issue_submission_token(ctx, IdempotentOperation::SetSessionContext, Some(handle))
        .await
        .expect("context submission token")
        .idempotency_key
}

// ── 真路由 oneshot 驱动 ───────────────────────────────────────────────────

/// oneshot 驱动认证链 Router：带 cookie（+ 可选 CSRF 头）与可选 JSON 体。
/// 返回 `(status, json)`；空体回 `Null`。
async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf_token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, cookie);
    if let Some(token) = csrf_token {
        builder = builder.header(csrf::CSRF_HEADER, token);
    }
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    let request = builder
        .body(Body::from(
            body.map(|value| value.to_string()).unwrap_or_default(),
        ))
        .expect("request builds");
    let response = app.clone().oneshot(request).await.expect("oneshot");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body bytes");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, json)
}

// ── 场景 1：提交发布发布候选、退役旧 id（真路由 oneshot）──────────────────

/// CM-68（W1）：`PUT /api/v1/sessions/{id}/context` 一次驱动内核的两阶段
/// 候选→提交发布（`ContextReplacer`：prepared 屏障 → Committed 同一临界区
/// 切换 → 旧会话走唯一释放例程 → 候选此刻才进可见表 → 新会话自己的令牌
/// 签发并挂载）。W1 可观测面：
///
/// 1. 回执携带新会话 + 被替换旧 id + **新** attachmentToken；
/// 2. 提交后旧 id 404 固定文案（`ensure_visible` 对已替换旧 id 先 404——
///    HTTP 同键重放门按此登记：同键重放触不到用例的回执回放，只会 404）；
/// 3. 新会话立即可路由（真目录活表可查、真 HTTP GET 200）。
///
/// 装配：真 SQLite 文件库 + 真登录会话行 + 真 cookie/CSRF 头 + runtime 与
/// sessions 域共享同一目录实例（bootstrap 纪律）。
#[tokio::test]
async fn cm68_a_committed_replacement_publishes_the_candidate_and_retires_the_old_session_id() {
    // 真 SQLite 文件库。
    let dir = std::env::temp_dir().join(format!("dz-g1-cm68-full-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply");

    // memberships 行是 login_sessions 的外键目标（g2 冻结项先例）：经
    // PolicyService 的管理用例落行（同一 SQL 面），不绕过外键语义。
    PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership");

    // 真登录会话行（L2）+ 真 CSRF（L4 与 router 共用同一 server_secret）。
    LoginSessions::new(store.clone())
        .create(NewLoginSession {
            organization_id: "org-1".into(),
            session_id: LOGIN_SESSION_ID.into(),
            principal_id: "user-1".into(),
            auth_time: now_rfc3339(),
            issued_at: now_rfc3339(),
            absolute_expires_at: future_timestamp(24),
            idle_expires_at: future_timestamp(8),
            permission_version: 1,
            client_binding: "ua|client-1".into(),
            mode: LoginSessionMode::Cookie,
            native_secret_hash: None,
        })
        .expect("live login session");
    let csrf_secret = CsrfSecret::new(SERVER_SECRET.to_vec()).expect("csrf secret");
    let csrf_token = csrf::mint(&csrf_secret, LOGIN_SESSION_ID).expect("mint csrf token");
    let cookie = format!("__Host-dz_session={LOGIN_SESSION_ID}; __Host-dz_csrf={csrf_token}");

    // runtime 与 sessions 域共享同一目录实例。
    let directory = Arc::new(InMemorySessionDirectory::new());
    let cases = Arc::new(LeaseConnectionUseCases::new(Arc::new(FixedLeaseSource(
        shared_runtime(store.clone(), directory.clone()),
    ))));

    // 替换发布的版本前置：org-1 名下的真实连接配置。
    let connection = create_profile(&store, &ctx("org-1", "user-1"), "cm68-replacement-full").await;

    let router = build_router_with_auth(AuthDeps {
        store: store.clone(),
        policy: Arc::new(
            PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy"),
        ),
        server_secret: SERVER_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: None,
        mcp: None,
        wapp_assets: None,
        domain_routers: Some(vec![sessions_router(SessionsState {
            sessions: cases.clone(),
            directory: directory.clone() as Arc<dyn SessionDirectory>,
        })]),
    })
    .expect("router");

    // ① openSession（真路由）：201 + Ready + contextRevision=1。
    // NamespaceTarget 的四个层级字段全部显式（wire 形状无 default）。
    let open_token = cases
        .issue_submission_token(&http_ctx(), IdempotentOperation::OpenSession, None)
        .await
        .expect("open token");
    let open_body = serde_json::json!({
        "initialTarget": {
            "connectionId": connection.as_str(),
            "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
        },
        "owner": { "kind": "clientSession", "clientInstanceId": "http", "purpose": "cm68-full" },
        "idempotencyKey": open_token.idempotency_key.as_str(),
        "submissionToken": serde_json::to_value(&open_token).expect("token json"),
    });
    let (status, body) = call(
        &router,
        "POST",
        "/api/v1/sessions",
        &cookie,
        Some(&csrf_token),
        Some(open_body),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "openSession → 201: {body}");
    let opened: OpenSessionReceipt = serde_json::from_value(body).expect("OpenSessionReceipt");
    let old_handle = opened.session.handle.clone();
    let old_token = opened.attachment_token.clone();
    assert_eq!(opened.session.state, SessionState::Ready, "开箱即 Ready");
    assert_eq!(
        opened.session.context_revision.get(),
        1,
        "contextRevision=1（actor open 钉的初值）"
    );

    // ② setSessionContext（真路由）：两阶段候选→提交发布。
    let context_key = issue_context_token(&cases, &http_ctx(), &old_handle).await;
    let context_body = serde_json::json!({
        "handle": serde_json::to_value(&old_handle).expect("handle json"),
        "expectedContextRevision": "1",
        "desired": { "database": "reports", "catalog": null, "schema": null, "path": [] },
        "idempotencyKey": context_key.as_str(),
        "submissionToken": {
            "idempotencyKey": context_key.as_str(),
            "expiresAt": "2026-01-02T00:00:00.000Z",
        },
    });
    let put_uri = format!(
        "/api/v1/sessions/{}/context",
        old_handle.db_session_id.as_str()
    );
    let (status, body) = call(
        &router,
        "PUT",
        &put_uri,
        &cookie,
        Some(&csrf_token),
        Some(context_body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "setSessionContext → 200: {body}");
    let receipt: ContextChangeReceipt = serde_json::from_value(body).expect("ContextChangeReceipt");

    // 回执携带新会话 + 被替换旧 id + 新 attachmentToken。
    let new_handle = receipt.session.handle.clone();
    assert_ne!(
        new_handle.db_session_id, old_handle.db_session_id,
        "候选发布：回执携带新的 dbSessionId"
    );
    assert_eq!(
        receipt
            .replaced_session_id
            .as_ref()
            .map(DbSessionId::as_str),
        Some(old_handle.db_session_id.as_str()),
        "回执记下被替换的旧会话 id"
    );
    let new_token = receipt
        .attachment_token
        .clone()
        .expect("回执携带新会话自己的挂载令牌");
    assert_ne!(new_token, old_token, "新令牌是新签发的，不是旧令牌重发");
    assert_eq!(
        receipt
            .session
            .observed_context
            .namespace
            .database
            .as_deref(),
        Some("reports"),
        "候选带期望命名空间发布"
    );

    // ③ 提交后旧 id 404 固定文案；同键重放同门（HTTP 同键重放门登记）。
    for (label, method, payload) in [
        ("GET", "GET", None),
        ("同键重放的 PUT", "PUT", Some(context_body)),
    ] {
        let uri = if method == "PUT" {
            put_uri.clone()
        } else {
            format!("/api/v1/sessions/{}", old_handle.db_session_id.as_str())
        };
        let (status, body) = call(&router, method, &uri, &cookie, Some(&csrf_token), payload).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{label} 旧 id 不可寻址: {body}"
        );
        assert_eq!(body["code"], "notFound", "wire 词表 camelCase");
        assert_eq!(
            body["message"].as_str(),
            Some(NOT_VISIBLE_MESSAGE),
            "404 固定文案（不含资源名/归属，防 ID 枚举）"
        );
    }

    // ④ 新会话立即可路由（真 HTTP GET 200 + 真目录活表可查）。
    let (status, body) = call(
        &router,
        "GET",
        &format!("/api/v1/sessions/{}", new_handle.db_session_id.as_str()),
        &cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "提交发布后候选立刻可路由: {body}");
    let view: SessionView = serde_json::from_value(body).expect("SessionView");
    assert_eq!(view.handle, new_handle, "路由寻址回答的是当前视图");
    assert_eq!(
        view.attachment_state,
        AttachmentState::Attached,
        "替换编排已用回执令牌挂载新会话"
    );
    assert_eq!(
        view.observed_context.namespace.database.as_deref(),
        Some("reports")
    );
    let gone = directory
        .lookup(old_handle.db_session_id.clone())
        .await
        .expect("lookup")
        .is_none();
    assert!(gone, "目录活表已无旧条目（Committed 关闭旧路由）");
}

// ── 场景 2：stale expectedContextRevision 冲突且旧会话继续服务 ─────────────

/// CM-68（W1）：`expectedContextRevision` 是乐观并发闸门不是提示——不匹配
/// 即 `ContextRevisionMismatch` → `contextConflict` 固定文案（`convert::runtime`
/// 的脱敏分类），旧会话一点不动：contextRevision 不推进、命名空间不切换、
/// 状态保持 Ready。wire 面 `contextConflict` → 409（`status_for`）。
#[tokio::test]
async fn cm68_a_stale_expected_context_revision_conflicts_and_leaves_the_old_session_serving() {
    let store = store();
    let cases = session_cases(store.clone());
    let user = ctx("org-1", "user-1");
    let connection = create_profile(&store, &user, "cm68-stale").await;
    let opened = open_session(&cases, &user, connection.as_str()).await;
    let old_handle = opened.session.handle.clone();

    let key = issue_context_token(&cases, &user, &old_handle).await;
    let request = SetSessionContextRequest::new(
        old_handle.clone(),
        Counter::new(9),
        NamespaceTarget::database("reports"),
        key,
    );
    let error = cases
        .set_session_context(&user, request)
        .await
        .expect_err("a stale expected revision must conflict");
    assert_eq!(
        error.code,
        ApiErrorCode::ContextConflict,
        "runtime ContextRevisionMismatch → contextConflict（api_code 单向映射）"
    );
    assert_eq!(
        error.message, "connection runtime operation failed",
        "固定文案：runtime 拒绝不外传句柄/版本差（脱敏分类）"
    );
    assert_eq!(
        status_for(ApiErrorCode::ContextConflict),
        StatusCode::CONFLICT,
        "wire 面：contextConflict → 409"
    );

    // 不切换：旧会话原样继续服务。
    let view = cases
        .get_session(&user, old_handle.clone())
        .await
        .expect("the old session still serves");
    assert_eq!(view.handle, old_handle);
    assert_eq!(
        view.context_revision.get(),
        1,
        "contextRevision 未被 stale 请求推进"
    );
    assert_eq!(
        view.observed_context.namespace.database.as_deref(),
        Some("app"),
        "命名空间未被切换"
    );
    assert_eq!(view.state, SessionState::Ready, "旧会话保持可服务");
}

// ── 场景 3：回执令牌只绑定已发布的候选 ─────────────────────────────────────

/// CM-68（W1）：旧会话的 attachment 令牌对新会话**没有**挂载权利（签发绑定
/// 是「令牌 ↔ 签发它的那次登记」，替换换了 dbSessionId 就对不上）——
/// `authorize_attachment` 摘要不符 → `TokenMismatch` → 用例层
/// `contextConflict`+"attachment token rejected"；回执里的新令牌 attach 成功，
/// 且失败尝试**不烧绑定**（挂载不重签摘要，同令牌随后仍可幂等重挂）。
#[tokio::test]
async fn cm68_the_receipt_token_binds_only_the_published_candidate() {
    let store = store();
    let cases = session_cases(store.clone());
    let user = ctx("org-1", "user-1");
    let connection = create_profile(&store, &user, "cm68-token-binding").await;
    let opened = open_session(&cases, &user, connection.as_str()).await;
    let old_handle = opened.session.handle.clone();
    let old_token = opened.attachment_token.clone();

    let key = issue_context_token(&cases, &user, &old_handle).await;
    let receipt = cases
        .set_session_context(
            &user,
            SetSessionContextRequest::new(
                old_handle.clone(),
                Counter::new(1),
                NamespaceTarget::database("reports"),
                key,
            ),
        )
        .await
        .expect("the replacement commits");
    let new_handle = receipt.session.handle.clone();
    let new_token = receipt
        .attachment_token
        .clone()
        .expect("receipt carries the new token");

    // 旧令牌打新会话：摘要对不上 → 拒绝（不是静默换绑）。
    let error = cases
        .attach_session(
            &user,
            AttachmentRequest::new(new_handle.clone(), old_token.clone()),
        )
        .await
        .expect_err("the old token must not attach the published candidate");
    assert_eq!(error.code, ApiErrorCode::ContextConflict);
    assert_eq!(error.message, "attachment token rejected");

    // 新令牌 attach 成功。
    let view = cases
        .attach_session(
            &user,
            AttachmentRequest::new(new_handle.clone(), new_token.clone()),
        )
        .await
        .expect("the receipt token attaches the published candidate");
    assert_eq!(view.handle, new_handle);
    assert_eq!(view.attachment_state, AttachmentState::Attached);

    // 失败尝试不烧绑定：同一新令牌再挂仍作数（AlreadyAttached 幂等）。
    let again = cases
        .attach_session(&user, AttachmentRequest::new(new_handle, new_token))
        .await
        .expect("a failed foreign attempt must not burn the binding");
    assert_eq!(again.attachment_state, AttachmentState::Attached);

    // 旧令牌也回不去旧会话：替换已提交，旧 id 整体退役。
    let error = cases
        .attach_session(&user, AttachmentRequest::new(old_handle.clone(), old_token))
        .await
        .expect_err("the old session is retired as a whole");
    assert_eq!(
        error.code,
        ApiErrorCode::SessionLost,
        "旧句柄对已关闭会话 → sessionLost（409 冲突类）"
    );
}

// ── 场景 4：响应丢失后同键重放返回同一份回执 ───────────────────────────────

/// CM-68（W1）：「网络丢失后同键重试返回原 receipt」——幂等短路排在所有旧
/// 会话检查之前（替换已提交后旧会话注销，先定位就会先撞 UnknownSession）；
/// owner 内存按操作键留存**即将发进回执的那一枚**令牌，重放回放同一枚而不是
/// 重签（重签会换掉目录摘要、让调用方手上那枚当场作废）。回放令牌真实作数：
/// 拿它挂载新会话成功。
#[tokio::test]
async fn cm68_a_lost_response_replayed_with_the_same_key_returns_the_same_receipt() {
    let store = store();
    let cases = session_cases(store.clone());
    let user = ctx("org-1", "user-1");
    let connection = create_profile(&store, &user, "cm68-replay").await;
    let opened = open_session(&cases, &user, connection.as_str()).await;
    let old_handle = opened.session.handle.clone();

    let key = issue_context_token(&cases, &user, &old_handle).await;
    let request = || {
        SetSessionContextRequest::new(
            old_handle.clone(),
            Counter::new(1),
            NamespaceTarget::database("reports"),
            key.clone(),
        )
    };

    let first = cases
        .set_session_context(&user, request())
        .await
        .expect("the replacement commits");
    // 响应丢失后的同键重放：整份回执原样发回。
    let replayed = cases
        .set_session_context(&user, request())
        .await
        .expect("the same-key replay returns the original receipt");
    assert_eq!(
        replayed, first,
        "同一份回执：同 session/token/replacedSessionId（ContextChangeReceipt 逐字段相等）"
    );
    assert_eq!(replayed.session.handle, first.session.handle);
    assert_eq!(replayed.replaced_session_id, first.replaced_session_id);
    assert_eq!(
        replayed.attachment_token, first.attachment_token,
        "回放的是同一枚令牌，不是重签"
    );

    // 回放令牌真实作数（用例面直调）。
    let new_handle = first.session.handle.clone();
    let token = first.attachment_token.clone().expect("the receipt token");
    let view = cases
        .attach_session(&user, AttachmentRequest::new(new_handle, token))
        .await
        .expect("the replayed token attaches");
    assert_eq!(view.attachment_state, AttachmentState::Attached);
}

// ── 场景 5：改指纹的同键重放是幂等冲突而非第二次替换 ───────────────────────

/// CM-68（W1）：同键不同请求体 → `IdempotencyConflict`+固定文案
/// "context request changed"（指纹冲突分支），且**无副作用**：已发布候选
/// 原样（没有第二次替换把它再切走）、旧会话保持退役、原始指纹的同键重放
/// 仍返回原回执（账目没被冲突重放污染）。
#[tokio::test]
async fn cm68_a_replay_with_a_changed_fingerprint_is_an_idempotency_conflict_not_a_second_replacement(
) {
    let store = store();
    let cases = session_cases(store.clone());
    let user = ctx("org-1", "user-1");
    let connection = create_profile(&store, &user, "cm68-fingerprint").await;
    let opened = open_session(&cases, &user, connection.as_str()).await;
    let old_handle = opened.session.handle.clone();

    let key = issue_context_token(&cases, &user, &old_handle).await;
    let request = |database: &str| {
        SetSessionContextRequest::new(
            old_handle.clone(),
            Counter::new(1),
            NamespaceTarget::database(database),
            key.clone(),
        )
    };

    let first = cases
        .set_session_context(&user, request("reports"))
        .await
        .expect("the replacement commits");
    let new_handle = first.session.handle.clone();

    // 同键、不同指纹：幂等冲突（不是第二次替换）。
    let conflict = cases
        .set_session_context(&user, request("analytics"))
        .await
        .expect_err("a changed fingerprint must conflict");
    assert_eq!(conflict.code, ApiErrorCode::IdempotencyConflict);
    assert_eq!(conflict.message, "context request changed");

    // 无副作用：新会话原样。
    let view = cases
        .get_session(&user, new_handle.clone())
        .await
        .expect("the published candidate is untouched");
    assert_eq!(view.handle, new_handle);
    assert_eq!(
        view.observed_context.namespace.database.as_deref(),
        Some("reports"),
        "没有第二次替换把候选再切走"
    );
    assert_eq!(view.context_revision.get(), 1);

    // 旧会话保持退役（clone：`request` 闭包还借着头柄等原始指纹重放）。
    let error = cases
        .get_session(&user, old_handle.clone())
        .await
        .expect_err("the old session stays retired");
    assert_eq!(error.code, ApiErrorCode::SessionLost);

    // 原始指纹的同键重放仍返回原回执。
    let replayed = cases
        .set_session_context(&user, request("reports"))
        .await
        .expect("the original fingerprint still replays");
    assert_eq!(replayed, first);
}

// ── 场景 6/7/8：SessionDirectory 端口（ServerSessionDirectory，单实例 v1）──

/// 端口测试用的目录条目（全字段；归属与活动时间固定便于比对「原状」）。
fn port_owner(db_session_id: &str, resource_epoch: u64) -> SessionOwner {
    SessionOwner {
        db_session_id: DbSessionId::new(db_session_id),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new("user-1"),
        connection_id: ConnectionId::new("conn-cm68"),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "cm68-directory-port".into(),
        },
        worker_id: WorkerId::new("local-runtime"),
        runtime_epoch: RuntimeEpoch::new("rte-1"),
        resource_epoch,
        last_business_activity: Timestamp::new(NOW),
    }
}

/// CM-68（W1）：替换的原子性在目录端口上可观测——`Prepared` 候选落在独立
/// prepared 表（`lookup` 只查活表 ⇒ 不可路由），旧条目在 prepared 期间保持
/// 服务；`Committed` 在**同一临界区**内 promote 候选 + 移除旧条目，一次提交
/// 之后两侧同时翻转：恰好一边可路由。
#[tokio::test]
async fn cm68_a_prepared_candidate_is_not_routable_until_the_commit_switches_routing() {
    let directory = ServerSessionDirectory::new();
    let old = port_owner("dbs-cm68-old6", 1);
    let old_handle = old.to_handle();
    let registered = directory.register(old).await.expect("register");
    assert_eq!(registered, old_handle, "登记返回的句柄与条目同一代");

    // prepared：候选不可路由，旧条目保持服务。
    let candidate = port_owner("dbs-cm68-cand6", 2);
    let candidate_id = candidate.db_session_id.clone();
    let outcome = directory
        .commit_replacement(ReplacementCommit {
            old: old_handle.clone(),
            new_owner: candidate,
            operation: ReplacementOperation::Prepared,
        })
        .await
        .expect("prepared");
    assert_eq!(outcome.operation, ReplacementOperation::Prepared);

    let serving = directory
        .lookup(old_handle.db_session_id.clone())
        .await
        .expect("lookup")
        .expect("the old entry keeps serving while prepared");
    assert_eq!(serving.db_session_id.as_str(), "dbs-cm68-old6");
    let prepared = directory
        .lookup(candidate_id.clone())
        .await
        .expect("lookup")
        .is_none();
    assert!(prepared, "prepared 候选不可路由");

    // committed：promote + 关旧路由一次完成（端口观测：一次提交两侧同时翻转）。
    let committed = directory
        .commit_replacement(ReplacementCommit {
            old: old_handle.clone(),
            new_owner: port_owner("dbs-cm68-cand6", 2),
            operation: ReplacementOperation::Committed,
        })
        .await
        .expect("committed");
    assert_eq!(committed.operation, ReplacementOperation::Committed);
    assert_eq!(
        committed.replaced.db_session_id, old_handle.db_session_id,
        "回执记下被替换者"
    );

    let promoted = directory
        .lookup(candidate_id)
        .await
        .expect("lookup")
        .expect("the candidate is routable after commit");
    assert_eq!(promoted.db_session_id.as_str(), "dbs-cm68-cand6");
    let retired = directory
        .lookup(old_handle.db_session_id)
        .await
        .expect("lookup")
        .is_none();
    assert!(retired, "旧条目随提交关闭路由");
}

/// CM-68（W1）：没有 prepared 记录的 `Committed` 被 `expect_prepared` 拒绝
/// （`NotFound("prepared candidate")`）——目录不凭空发明候选；旧会话原样
/// 服务，幽灵候选从未存在。
#[tokio::test]
async fn cm68_a_commit_without_a_prepared_candidate_is_refused_instead_of_invented() {
    let directory = ServerSessionDirectory::new();
    let old = port_owner("dbs-cm68-old7", 1);
    let old_handle = old.to_handle();
    directory.register(old).await.expect("register");

    let error = directory
        .commit_replacement(ReplacementCommit {
            old: old_handle.clone(),
            new_owner: port_owner("dbs-cm68-ghost", 2),
            operation: ReplacementOperation::Committed,
        })
        .await
        .expect_err("a commit without a prepared candidate is refused");
    match &error {
        PortError::NotFound(message) => assert!(
            message.contains("prepared candidate"),
            "拒绝理由逐字可辨（不是笼统失败）: {message}"
        ),
        other => panic!("expected PortError::NotFound, got {other:?}"),
    }

    // 无副作用：旧会话原样服务，幽灵候选从未被写进任何表。
    assert!(
        directory
            .lookup(old_handle.db_session_id)
            .await
            .expect("lookup")
            .is_some(),
        "旧会话未被这次拒绝触碰"
    );
    assert!(
        directory
            .lookup(DbSessionId::new("dbs-cm68-ghost"))
            .await
            .expect("lookup")
            .is_none(),
        "没有凭空发明的候选"
    );
}

/// CM-68（W1）：`RolledBack` 候选销毁、旧会话**原状**恢复（条目、归属、
/// resource_epoch 都不动）；prepared 表已空之后迟到的 `Committed` 不能让
/// 候选复活（`NotFound`），旧会话继续原样服务。
#[tokio::test]
async fn cm68_a_rolled_back_replacement_keeps_the_old_session_and_never_publishes_the_candidate() {
    let directory = ServerSessionDirectory::new();
    let old = port_owner("dbs-cm68-old8", 1);
    let old_handle = old.to_handle();
    directory.register(old).await.expect("register");

    let candidate = port_owner("dbs-cm68-cand8", 2);
    let candidate_id = candidate.db_session_id.clone();
    directory
        .commit_replacement(ReplacementCommit {
            old: old_handle.clone(),
            new_owner: candidate,
            operation: ReplacementOperation::Prepared,
        })
        .await
        .expect("prepared");

    // 回滚：候选销毁，旧会话原状。
    let outcome = directory
        .commit_replacement(ReplacementCommit {
            old: old_handle.clone(),
            new_owner: port_owner("dbs-cm68-cand8", 2),
            operation: ReplacementOperation::RolledBack,
        })
        .await
        .expect("rolled back");
    assert_eq!(outcome.operation, ReplacementOperation::RolledBack);

    let restored = directory
        .lookup(old_handle.db_session_id.clone())
        .await
        .expect("lookup")
        .expect("the old session survives the rollback");
    assert_eq!(restored.db_session_id.as_str(), "dbs-cm68-old8");
    assert_eq!(
        restored.resource_epoch, 1,
        "旧条目原状：resource_epoch 未被候选顶掉"
    );
    assert!(
        directory
            .lookup(candidate_id.clone())
            .await
            .expect("lookup")
            .is_none(),
        "候选已销毁"
    );

    // 迟到的 Committed 不能让候选复活。
    let late = directory
        .commit_replacement(ReplacementCommit {
            old: old_handle.clone(),
            new_owner: port_owner("dbs-cm68-cand8", 2),
            operation: ReplacementOperation::Committed,
        })
        .await
        .expect_err("a late commit must not resurrect the candidate");
    assert!(
        matches!(late, PortError::NotFound(_)),
        "prepared 表已空：迟到提交按 NotFound 拒绝"
    );
    assert!(
        directory
            .lookup(candidate_id)
            .await
            .expect("lookup")
            .is_none(),
        "迟到的提交没有把候选写回活表"
    );
    assert!(
        directory
            .lookup(old_handle.db_session_id)
            .await
            .expect("lookup")
            .is_some(),
        "旧会话仍原样服务"
    );
}
