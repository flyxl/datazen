//! W8 刀1：execution 域四路由 + aiHost.executeTarget 点亮的全栈集成钉子。
//!
//! 覆盖（契约 §6.4a + v1.2 aiHost 域行的服务端半边）：
//!
//! - **绿路径**：`executeAtTarget`（无会话一次性执行，202 + 稳定 id →
//!   轮询终态 Succeeded）与 `executeInSession`（开会话 → 会话内执行 →
//!   终态）——幂等令牌全程走 identity 路由的 **runtime 轨**（本刀放开的
//!   会话绑定签发；HMAC 轨的令牌过不了 runtime 核验的分轨事实由
//!   `t9` 钉住）。
//! - **可见性三层**：会话目录闸（越组织/未知会话 404 固定文案；体
//!   handle 与路径不一致 400）→ L6 基线（`GET /api/v1/executions/{id}`
//!   的 Execution+Read 求值）→ 用例层 `visible_to` 五元组（同组织不同
//!   主体读他人执行统一 404「execution unavailable」，无存在性预言机）。
//! - **令牌纪律**：提交缺令牌 400；HMAC 轨令牌（startJob 形）喂给执行
//!   路由 → runtime 核验拒绝（IdempotencyExpired）——两轨不混用。
//! - **aiHost.executeTarget**：`independent` 声明真执行绿路径（MockDriver
//!   全栈，200 `CommandResult`）；`editorGrant` 501 fail-closed；闸序
//!   拒绝（未知连接 503）。
//!
//! 被测面全部是生产装配（真登录会话/CSRF/org 三主体 + MockDriver 全栈，
//! cm63 `http_fixture` 先例）；假体只在驱动层（`MockDriver` 的常态 CI
//! 形态）。

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use tower::ServiceExt;

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::use_cases::ConnectionUseCases as _;
use datazen::shared::id::{
    ClientInstanceId, IdempotencyKey, OrganizationId, PrincipalId, RequestId,
};

use datazen_driver_api::capabilities;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory, SslMode};

use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::execution::budget::ServerBudgetCoordinator;
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::ServerRuntimeLeaseSource;
use datazen_server::execution::lease_use_cases::LeaseConnectionUseCases;
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::execution::submission_token::ServerSubmissionTokenIssuer;
use datazen_server::http::domains::ai_host::{ai_host_router, AiHostState};
use datazen_server::http::domains::execution::{execution_router, ExecutionState};
use datazen_server::http::domains::sessions::{sessions_router, SessionsState};
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::http::router::{build_router_with_auth, AuthDeps, DomainState};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::{
    AdminStore, LoginSessions, ServerAudit, ServerConnections, ServerJobs, ServerProfiles,
    ServerSettingsStore,
};

const SERVER_SECRET: &[u8] = b"execution-routes-server-secret";

// ── fake-driver 装配（cm63 WireMockFactory / mcp GreenMockFactory 同构）──

struct RouteMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn DatabaseDriver>,
}

impl DatabaseDriverFactory for RouteMockFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::clone(&self.driver)
    }

    fn driver_id(&self) -> &'static str {
        self.driver_id
    }

    fn resource_provider(&self) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
        let shape = datazen_driver_api::namespace::NamespaceShape {
            levels: vec![
                datazen_driver_api::namespace::NamespaceLevel::present(
                    datazen_driver_api::namespace::NamespaceLevelKind::Database,
                    false,
                ),
                datazen_driver_api::namespace::NamespaceLevel::absent(
                    datazen_driver_api::namespace::NamespaceLevelKind::Catalog,
                ),
                datazen_driver_api::namespace::NamespaceLevel::present(
                    datazen_driver_api::namespace::NamespaceLevelKind::Schema,
                    false,
                ),
            ],
            ..datazen_driver_api::namespace::NamespaceShape::default()
        };
        let mut caps = capabilities::CapabilitySet::default();
        caps.stateful_session = capabilities::Availability::Supported;
        caps.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
        caps.context_observation = capabilities::ContextObservation::Full;
        caps.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
        caps.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
        Some(Arc::new(LegacyResourceAdapter::new(
            Arc::clone(&self.driver),
            self.driver_id,
            "execution-routes",
            1,
            shape,
            caps,
        )))
    }
}

// ── 夹具 ────────────────────────────────────────────────────────────────────

struct Fixture {
    _dir: PathBuf,
    _store: Arc<AdminStore>,
    app: axum::Router,
    connection_id: String,
    profile_revision: u64,
    /// org-1/user-1 的真 cookie（登录会话 + CSRF 双令牌）。
    cookie: String,
    csrf: String,
    /// org-1/user-2 的真 cookie（同组织第二主体，visible_to 钉子用）。
    cookie_user2: String,
    csrf_user2: String,
    /// 用例面（轮询终态的测试侧便捷面；非 HTTP 断言路径）。
    cases: Arc<LeaseConnectionUseCases>,
    ctx: datazen::shared::context::RequestContext,
}

async fn http_fixture() -> Fixture {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-exec-routes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    // 真 membership（org-1 两主体 + org-2 一主体）+ 真登录会话 + 真 CSRF
    // 令牌（g1 冻结项先例：真实时钟戳，硬编码日期会腐烂成 401）。
    let policy = PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy");
    policy
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership user-1");
    policy
        .create_membership("org-1", "user-2", Role::OrgMember, "idp:user-2")
        .expect("membership user-2");
    policy
        .create_membership("org-2", "user-2", Role::OrgMember, "idp:user-2-org2")
        .expect("membership org-2");

    let now = chrono::Utc::now();
    let rfc3339 = |value: chrono::DateTime<chrono::Utc>| {
        value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    let login = |session_id: &str, org: &str, principal: &str| NewLoginSession {
        organization_id: org.to_owned(),
        session_id: session_id.to_owned(),
        principal_id: principal.to_owned(),
        auth_time: rfc3339(now),
        issued_at: rfc3339(now),
        absolute_expires_at: rfc3339(now + chrono::Duration::hours(24)),
        idle_expires_at: rfc3339(now + chrono::Duration::hours(8)),
        permission_version: 1,
        client_binding: "ua|execution-routes".to_owned(),
        mode: LoginSessionMode::Cookie,
        native_secret_hash: None,
    };
    LoginSessions::new(store.clone())
        .create(login("sess-exec-u1", "org-1", "user-1"))
        .expect("login session user-1");
    LoginSessions::new(store.clone())
        .create(login("sess-exec-u2", "org-1", "user-2"))
        .expect("login session user-2");
    let csrf_secret = CsrfSecret::new(SERVER_SECRET.to_vec()).expect("csrf secret");
    let csrf = csrf::mint(&csrf_secret, "sess-exec-u1").expect("mint csrf");
    let csrf_user2 = csrf::mint(&csrf_secret, "sess-exec-u2").expect("mint csrf");
    let cookie = format!("__Host-dz_session=sess-exec-u1; __Host-dz_csrf={csrf}");
    let cookie_user2 = format!("__Host-dz_session=sess-exec-u2; __Host-dz_csrf={csrf_user2}");

    let ctx = datazen::shared::context::RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        // HTTP 面 SessionIdentity 固定铸造 `http`（router.rs 提取器）。
        ClientInstanceId::new("http"),
        RequestId::new("req-exec-routes"),
        None,
    );

    // 驱动注册表：懒形 + 测试驱动 + 测试工厂（闸序可达驱动层）。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let mut opts = MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(datazen_driver_api::Value::Integer(1))]];
    let driver = MockDriver::new("mockdb", opts);
    registry
        .register_test_driver("mockdb", driver.clone())
        .await;
    registry
        .register_test_factory(
            "mockdb",
            Box::leak(Box::new(RouteMockFactory {
                driver_id: "mockdb",
                driver: driver.clone(),
            })),
        )
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("exec-routes-conn", "mockdb"),
            &IdempotencyKey::new("idem-exec-routes-profile"),
        )
        .await
        .expect("profile create");
    let connection_id = record.connection_id.as_str().to_owned();
    let profile_revision = record.config_revision.counter().get();
    let scope = ConnectionScope::new("org-1");
    connections
        .save(
            &scope,
            datazen_driver_api::ConnectionConfig {
                id: connection_id.clone(),
                name: "exec routes conn".into(),
                database_type: "mockdb".into(),
                host: Some("localhost".into()),
                port: Some(1),
                database: Some("app".into()),
                schema: None,
                username: None,
                password: None,
                ssl_mode: SslMode::Disable,
                connection_timeout: 30,
                max_pool_size: 10,
                ssh_tunnel: None,
                tunnel_kind: None,
                tunnel_id: None,
                http_proxy_tunnel: None,
                websocket_tunnel: None,
                color_tag: None,
                group: None,
                last_connected_at: None,
                server_version: None,
                options: None,
                read_only: false,
                pinned: false,
            },
        )
        .await
        .expect("connection config saved");

    // 执行后端 + 发布钩子 + 真租约源（sessions/execution/aiHost/host 同源）。
    let bridge = Arc::new(BudgetPortBridge::new(
        Arc::new(ServerBudgetCoordinator::new(
            8,
            10_000,
            OrganizationId::new("org-1"),
        )),
        OrganizationId::new("org-1"),
    ));
    let backend = Arc::new(ServerDriverExecBackend::new(
        registry.clone(),
        connections.clone(),
        profiles.clone(),
        scope,
        bridge,
        None,
    ));
    let sink = datazen::execution::application::orchestration::RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let execution_policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let directory =
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
    let lease_source = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles.clone(),
        execution_policy,
        backend,
        sink,
        directory.clone(),
    ));
    let directory_face: Arc<dyn datazen::execution::SessionDirectory> =
        lease_source.session_directory();
    let cases = Arc::new(LeaseConnectionUseCases::new(lease_source.clone()));
    let host = Arc::new(ServerResourceExecutionHost::new(
        connections.clone(),
        profiles.clone(),
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        lease_source.clone(),
        "org-1",
    ));

    // transfer 面（DomainState 的 HMAC 轨 + tasks；token 路由挂载必需）。
    let clock: Arc<dyn datazen::shared::clock::Clock> =
        Arc::new(datazen::shared::clock::SystemClock);
    let hmac_issuer = Arc::new(
        ServerSubmissionTokenIssuer::new(SERVER_SECRET.to_vec(), clock.clone())
            .expect("hmac issuer"),
    );
    let audit = Arc::new(ServerAudit::new(store.clone()));
    let jobs = Arc::new(ServerJobs::new(
        store.clone(),
        Arc::new(datazen::jobs::domain::time::PortJobClock::new(
            datazen::shared::clock::SystemClock,
        )),
        900,
    ));
    struct AllowAllGate;
    #[async_trait::async_trait]
    impl datazen_server::transfer::PreSubmitGate for AllowAllGate {
        async fn check_submit(
            &self,
            _ctx: &datazen::shared::context::RequestContext,
            _submission: &datazen::execution::application::submission::VerifiedSubmission,
        ) -> Result<(), datazen::shared::error::ApiError> {
            Ok(())
        }
    }
    let submission = Arc::new(datazen_server::transfer::SubmissionService::new(
        hmac_issuer.clone(),
        jobs.clone(),
        Arc::new(AllowAllGate),
        audit.clone(),
        clock.clone(),
    ));

    let app = build_router_with_auth(AuthDeps {
        store: store.clone(),
        policy: Arc::new(
            PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy"),
        ),
        server_secret: SERVER_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: Some(DomainState {
            issuer: hmac_issuer,
            execution_tokens: cases.clone(),
            submission,
            uploads: Arc::new(datazen_server::transfer::UploadService::new(
                Arc::new(
                    datazen_server::storage::ServerArtifacts::open(store.clone(), &dir)
                        .expect("artifacts"),
                ),
                audit.clone(),
                clock.clone(),
            )),
            downloads: Arc::new(datazen_server::transfer::DownloadService::new(
                Arc::new(
                    datazen_server::storage::ServerArtifacts::open(store.clone(), &dir)
                        .expect("artifacts"),
                ),
                audit.clone(),
                clock.clone(),
            )),
            tasks: Arc::new(datazen_server::operations::TaskCenter::new(jobs, 50)),
        }),
        mcp: None,
        domain_routers: Some(vec![
            sessions_router(SessionsState {
                sessions: cases.clone(),
                directory: directory_face.clone(),
            }),
            execution_router(ExecutionState {
                sessions: cases.clone(),
                directory: directory_face.clone(),
            }),
            ai_host_router(AiHostState {
                settings: Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
                organization_id: "org-1".to_owned(),
                host,
            }),
        ]),
    })
    .expect("router");

    Fixture {
        _dir: dir,
        _store: store,
        app,
        connection_id,
        profile_revision,
        cookie,
        csrf,
        cookie_user2,
        csrf_user2,
        cases,
        ctx,
    }
}

/// oneshot 驱动认证链 Router（cm63 `call` 同款）。
async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf_token: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, cookie)
        .header(datazen_server::csrf::CSRF_HEADER, csrf_token);
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
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body bytes");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, json)
}

/// identity 路由的 runtime 轨签发（执行域令牌的正路）。
async fn issue_token(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    operation: &str,
    handle: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut body = serde_json::json!({ "operation": operation });
    if let Some(handle) = handle {
        body["handle"] = handle;
    }
    call(
        app,
        "POST",
        "/api/v1/submission-tokens",
        cookie,
        csrf,
        Some(body),
    )
    .await
}

/// 轮询一条执行到终态（测试侧上限 200 × 25ms；runtime 自负生命周期）。
async fn poll_terminal(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    execution_id: &str,
) -> serde_json::Value {
    for _ in 0..200 {
        let (status, view) = call(
            app,
            "GET",
            &format!("/api/v1/executions/{execution_id}"),
            cookie,
            csrf,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "poll: {view}");
        let state = view["state"].as_str().unwrap_or_default().to_owned();
        if matches!(state.as_str(), "succeeded" | "failed" | "cancelled") {
            return view;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("execution did not reach a terminal state in time: {execution_id}");
}

// ── 测试 ────────────────────────────────────────────────────────────────────

/// 绿路径 #1：executeAtTarget 全链——identity 路由 runtime 轨签发 →
/// POST /api/v1/executions → 202 `ExecutionReceipt`（稳定 id + 受理态）→
/// 轮询终态 `succeeded`。
#[tokio::test]
async fn t1_execute_at_target_green_path() {
    let fx = http_fixture().await;

    let (status, token) = issue_token(&fx.app, &fx.cookie, &fx.csrf, "executeAtTarget", None).await;
    assert_eq!(status, StatusCode::CREATED, "{token}");
    let key = token["idempotencyKey"].as_str().expect("key").to_owned();

    let (status, receipt) = call(
        &fx.app,
        "POST",
        "/api/v1/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "target": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": fx.profile_revision,
            "call": { "command": "query", "input": { "sql": "SELECT 1", "database": "app" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();
    assert!(
        execution_id.starts_with("exe_"),
        "stable execution id mints from the session namespace: {execution_id}"
    );
    assert_eq!(
        receipt["streamId"].as_str().is_some(),
        true,
        "streamId present"
    );
    // 受理态：非终态承诺（已接受 ≠ SQL 成功）。
    assert_ne!(receipt["state"], "succeeded");

    let view = poll_terminal(&fx.app, &fx.cookie, &fx.csrf, &execution_id).await;
    assert_eq!(view["state"], "succeeded", "{view}");
}

/// 绿路径 #2：executeInSession 全链——开会话 → 带 handle 签发 →
/// POST /api/v1/sessions/{id}/executions → 202 → 终态。
#[tokio::test]
async fn t2_execute_in_session_green_path() {
    let fx = http_fixture().await;

    // 开会话（openSession 的令牌不带 handle——首开）。
    let (status, token) = issue_token(&fx.app, &fx.cookie, &fx.csrf, "openSession", None).await;
    assert_eq!(status, StatusCode::CREATED, "{token}");
    let open_key = token["idempotencyKey"].as_str().expect("key").to_owned();
    let (status, opened) = call(
        &fx.app,
        "POST",
        "/api/v1/sessions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": open_key },
            "idempotencyKey": open_key,
            "initialTarget": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "owner": {
                "kind": "editor",
                "clientInstanceId": "http",
                "editorSessionId": "exec-routes-editor",
            },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let handle = opened["session"]["handle"].clone();
    let db_session_id = handle["dbSessionId"]
        .as_str()
        .expect("dbSessionId")
        .to_owned();

    // 会话内执行：令牌带 handle（runtime 轨的绑定事实）。
    let (status, token) = issue_token(
        &fx.app,
        &fx.cookie,
        &fx.csrf,
        "executeInSession",
        Some(handle.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{token}");
    let key = token["idempotencyKey"].as_str().expect("key").to_owned();

    let (status, receipt) = call(
        &fx.app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "handle": handle,
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();

    let view = poll_terminal(&fx.app, &fx.cookie, &fx.csrf, &execution_id).await;
    assert_eq!(view["state"], "succeeded", "{view}");
}

/// 可见性闸 #1：executeInSession 的 `{id}` 先过会话目录闸——未知会话与
/// 越组织会话统一 404 固定文案（防枚举），令牌核对在其后（目录闸先于
/// 令牌在场性：可见性先于一切执行语义）。
#[tokio::test]
async fn t3_execute_in_session_visibility_gate() {
    let fx = http_fixture().await;

    // 未知会话：404 固定文案（L6 NOT_VISIBLE_MESSAGE 与 sessions 域同一条）
    // ——体连令牌都不带，目录闸先行。
    let (status, wire) = call(
        &fx.app,
        "POST",
        "/api/v1/sessions/dbs_unknown/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "handle": { "dbSessionId": "dbs_unknown", "runtimeEpoch": "0" },
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], NOT_VISIBLE_MESSAGE);

    // 已知会话 + 缺令牌：400 invalidArgument（令牌在场性闸在可见性之后）。
    let (status, open_token) =
        issue_token(&fx.app, &fx.cookie, &fx.csrf, "openSession", None).await;
    assert_eq!(status, StatusCode::CREATED);
    let open_key = open_token["idempotencyKey"]
        .as_str()
        .expect("key")
        .to_owned();
    let (status, opened) = call(
        &fx.app,
        "POST",
        "/api/v1/sessions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": open_key },
            "idempotencyKey": open_key,
            "initialTarget": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "owner": { "kind": "editor", "clientInstanceId": "http", "editorSessionId": "e" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let db_session_id = opened["session"]["handle"]["dbSessionId"]
        .as_str()
        .expect("dbSessionId")
        .to_owned();
    let (status, wire) = call(
        &fx.app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "handle": opened["session"]["handle"],
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wire}");
    assert_eq!(wire["code"], "invalidArgument");

    // 越组织主体对同一未知会话：同一 404 固定文案（不可区分性）。
    let (status, wire) = call(
        &fx.app,
        "POST",
        "/api/v1/sessions/dbs_unknown/executions",
        &fx.cookie_user2,
        &fx.csrf_user2,
        Some(serde_json::json!({
            "handle": { "dbSessionId": "dbs_unknown", "runtimeEpoch": "0" },
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], NOT_VISIBLE_MESSAGE);
}

/// 可见性闸 #2：体 handle 与路径 `{id}` 不一致 → 400（路径是寻址事实，
/// 体是声明；先过目录闸再核对声明）。
#[tokio::test]
async fn t4_handle_mismatch_is_a_bad_request() {
    let fx = http_fixture().await;

    let (status, token) = issue_token(&fx.app, &fx.cookie, &fx.csrf, "openSession", None).await;
    assert_eq!(status, StatusCode::CREATED);
    let open_key = token["idempotencyKey"].as_str().expect("key").to_owned();
    let (status, opened) = call(
        &fx.app,
        "POST",
        "/api/v1/sessions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": open_key },
            "idempotencyKey": open_key,
            "initialTarget": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "owner": { "kind": "editor", "clientInstanceId": "http", "editorSessionId": "e" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let handle = opened["session"]["handle"].clone();
    let db_session_id = handle["dbSessionId"]
        .as_str()
        .expect("dbSessionId")
        .to_owned();

    let (status, token) = issue_token(
        &fx.app,
        &fx.cookie,
        &fx.csrf,
        "executeInSession",
        Some(handle.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let key = token["idempotencyKey"].as_str().expect("key").to_owned();
    // 路径指向真会话，体 handle 指向别处 → 400 invalidArgument。
    let (status, wire) = call(
        &fx.app,
        "POST",
        &format!("/api/v1/sessions/{db_session_id}/executions"),
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "handle": { "dbSessionId": "dbs_other", "runtimeEpoch": handle["runtimeEpoch"] },
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wire}");
    assert_eq!(wire["code"], "invalidArgument");
}

/// 可见性闸 #3：同组织第二主体读他人执行 → 统一 404「execution
/// unavailable」（visible_to 五元组；无存在性预言机——与未知 id 同文案）。
#[tokio::test]
async fn t5_cross_principal_execution_read_is_uniform_404() {
    let fx = http_fixture().await;

    let (status, token) = issue_token(&fx.app, &fx.cookie, &fx.csrf, "executeAtTarget", None).await;
    assert_eq!(status, StatusCode::CREATED);
    let key = token["idempotencyKey"].as_str().expect("key").to_owned();
    let (status, receipt) = call(
        &fx.app,
        "POST",
        "/api/v1/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "target": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": fx.profile_revision,
            "call": { "command": "query", "input": { "sql": "SELECT 1", "database": "app" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{receipt}");
    let execution_id = receipt["executionId"]
        .as_str()
        .expect("executionId")
        .to_owned();

    // 同组织第二主体：404 与未知 id 同文案（user-2 的 L6 基线 Read 放行，
    // 用例层 visible_to 拦下）。
    let (status, wire) = call(
        &fx.app,
        "GET",
        &format!("/api/v1/executions/{execution_id}"),
        &fx.cookie_user2,
        &fx.csrf_user2,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], "execution unavailable");

    // 完全未知 id：同一 404 同文案（不泄露存在性）。
    let (status, wire) = call(
        &fx.app,
        "GET",
        "/api/v1/executions/exe_nope_1",
        &fx.cookie,
        &fx.csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], "execution unavailable");
}

/// 令牌纪律：提交缺 `submissionToken` → 400；HMAC 轨令牌（startJob 形）
/// 喂给执行路由 → runtime 核验拒绝（`IdempotencyExpired`）——两轨不混用。
#[tokio::test]
async fn t6_submission_token_discipline() {
    let fx = http_fixture().await;

    // 缺令牌：400 invalidArgument（在场性闸）。
    let (status, wire) = call(
        &fx.app,
        "POST",
        "/api/v1/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "idempotencyKey": "idem-no-token",
            "target": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": fx.profile_revision,
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{wire}");
    assert_eq!(wire["code"], "invalidArgument");

    // HMAC 轨令牌（createProfile 形——ServerSubmissionTokenIssuer 签名）
    // 不是 runtime 注册表的成员 → runtime 核验拒绝。
    let (status, hmac_token) = call(
        &fx.app,
        "POST",
        "/api/v1/submission-tokens",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({ "operation": "createProfile" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{hmac_token}");
    let hmac_key = hmac_token["idempotencyKey"]
        .as_str()
        .expect("key")
        .to_owned();
    assert_ne!(
        hmac_key, "",
        "HMAC token carries a signed payload (startJob/transfer 轨)"
    );
    let (status, wire) = call(
        &fx.app,
        "POST",
        "/api/v1/executions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": hmac_key },
            "idempotencyKey": hmac_key,
            "target": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "expectedConfigRevision": fx.profile_revision,
            "call": { "command": "query", "input": { "sql": "SELECT 1" } },
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "runtime rejects foreign tokens (§9.2 的 409 行): {wire}"
    );
    assert_eq!(wire["code"], "idempotencyExpired");
}

/// cancelExecution：未知执行统一 404（用例层 visible_to）；终态/受理态的
/// 顺序语义由共享层 §7.6 钉子覆盖，此处钉 wire 面（令牌在场性 + 归属）。
#[tokio::test]
async fn t7_cancel_unknown_execution_is_uniform_404() {
    let fx = http_fixture().await;

    let (status, token) = issue_token(&fx.app, &fx.cookie, &fx.csrf, "cancelExecution", None).await;
    // cancelExecution 的 runtime 轨签发缺 handle → 用例层显式拒（400）。
    assert_eq!(status, StatusCode::BAD_REQUEST, "{token}");

    // 持会话签发 cancel 令牌（前端 EditorSessionController 的形状：
    // token('cancelExecution') 带 this.session.handle）。
    let (status, open_token) =
        issue_token(&fx.app, &fx.cookie, &fx.csrf, "openSession", None).await;
    assert_eq!(status, StatusCode::CREATED);
    let open_key = open_token["idempotencyKey"]
        .as_str()
        .expect("key")
        .to_owned();
    let (status, opened) = call(
        &fx.app,
        "POST",
        "/api/v1/sessions",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": open_key },
            "idempotencyKey": open_key,
            "initialTarget": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "owner": { "kind": "editor", "clientInstanceId": "http", "editorSessionId": "e" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let handle = opened["session"]["handle"].clone();
    let (status, cancel_token) = issue_token(
        &fx.app,
        &fx.cookie,
        &fx.csrf,
        "cancelExecution",
        Some(handle),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{cancel_token}");
    let key = cancel_token["idempotencyKey"]
        .as_str()
        .expect("key")
        .to_owned();

    // 未知执行：404 统一文案（visible_to 先于取消语义）。
    let (status, wire) = call(
        &fx.app,
        "POST",
        "/api/v1/executions/exe_nope_1/cancel",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({ "submissionToken": { "idempotencyKey": key } })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{wire}");
    assert_eq!(wire["message"], "execution unavailable");
}

/// aiHost.executeTarget 绿路径：`independent` 声明走桌面同源共享函数
/// （MockDriver 全栈），200 `CommandResult`。
#[tokio::test]
async fn t8_ai_host_execute_target_independent_green_path() {
    let fx = http_fixture().await;

    let (status, result) = call(
        &fx.app,
        "POST",
        "/api/v1/ai-host/execute-target",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "id": fx.connection_id,
            "command": "query",
            "input": { "sql": "SELECT 1", "database": "app" },
            "claim": { "kind": "independent" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    // CommandResult 的 camelCase wire 形状：{ data }。
    assert!(
        result.get("data").is_some(),
        "CommandResult.data present: {result}"
    );
}

/// aiHost.executeTarget 的闸序与 fail-closed 分臂（f1 文件已钉分臂形状，
/// 此处钉全栈闸序：未知连接 → 503「profile unavailable」）。
#[tokio::test]
async fn t9_ai_host_execute_target_gate_order() {
    let fx = http_fixture().await;

    let (status, wire) = call(
        &fx.app,
        "POST",
        "/api/v1/ai-host/execute-target",
        &fx.cookie,
        &fx.csrf,
        Some(serde_json::json!({
            "id": "conn-unknown",
            "command": "query",
            "input": { "sql": "SELECT 1" },
            "claim": { "kind": "independent" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{wire}");
    assert_eq!(wire["code"], "serviceUnavailable");
}
