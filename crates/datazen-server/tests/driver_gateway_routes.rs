//! driverGateway 过渡域路由的集成测试（WR-C2 契约面 §6.9；W3 刀）。
//!
//! 形状与 `table_data_routes.rs` 同款：fixture 复用 `build_router_with_auth`
//! 的真认证链（真登录会话 / CSRF / L4）+ `domain_routers` 统一通道；状态
//! 面是 f7_wiring `target_machine` 夹具的同构投影（真存储 + 空驱动注册表
//! + 租约源 + `ServerResourceExecutionHost`，与 bootstrap 同一构造序）。
//! v1 空驱动目录下每一条断言都是**闸序真跑出来的应答**：驱动闸（503
//! 「Driver unavailable」）先于任何策略求值；registry 点亮后绿路径（命令
//! 编排 + 全量执行半边）自动可达，测试面不需要重写。
//!
//! 覆盖面（按域文件模块头状态表）：
//! - `get_driver_commands`：参数闸（缺参/双参）+ registry 直取的诚实 404；
//! - `get_connection_commands`：防枚举 404（未知/越组织，CM-05）+ 注册会
//!   话的驱动闸 503；
//! - `execute_driver_command`：体解析闸 + 缺 command 闸 + driverType 直取
//!   的诚实 404（W8-b 起 registry 点亮前绿路径随 registry 自动可达）+ 会话
//!   绑定路径的 profile/驱动闸 503；
//! - `execute_driver_command_stream`：SQL 命令收窄闸 + 缺 dbSessionId 闸 +
//!   同款闸序（驱动闸先于订阅登记——闸不过不建流，域文件模块头）；
//! - 认证链钉：未认证 401、无 CSRF 的 POST 403（L2/L4 由
//!   `build_router_with_auth` 保证）。

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use axum::response::Response;
use tower::ServiceExt;

use datazen::connection::domain::catalog::ConnectionRepository as _;
use datazen::connection::domain::catalog::ConnectionScope;
use datazen::connection::domain::profile::ProfileRepository;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::orchestration::RuntimeResultSink;
use datazen::execution::domain::session_directory::SessionOwner;
use datazen::shared::audit::AuditSink;
use datazen::shared::clock::{Clock, SystemClock};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::id::{
    ClientInstanceId, ConnectionId, DbSessionId, OrganizationId, PrincipalId, RequestId,
    RuntimeEpoch, Timestamp, WorkerId,
};

use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::execution::ephemeral::EphemeralSessions;
use datazen_server::execution::lease::ServerRuntimeLeaseSource;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::http::domains::driver_gateway::{driver_gateway_router, DriverGatewayState};
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::{
    AdminStore, LoginSessions, ServerAudit, ServerConnections, ServerProfiles, ServerSettingsStore,
};

const SERVER_SECRET: &[u8] = b"w3-driver-gateway-secret-32-bytes!";
const NOW: &str = "2026-01-01T00:00:00.000Z";

/// 真实时钟戳（g1 冻结项先例）：登录会话的过期面以真实时钟判定，
/// 硬编码日期会随真实时间腐烂成 401。
fn future_timestamp(hours: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

struct Account {
    cookie: String,
    csrf: String,
}

fn store() -> Arc<AdminStore> {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w3-driver-gw-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");
    store
}

fn context(org: &str, principal: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(org),
        PrincipalId::new(principal),
        None,
        ClientInstanceId::new("dg-test"),
        RequestId::new("req-dg-1"),
        None,
    )
}

fn policy_for(
    store: &Arc<AdminStore>,
) -> datazen_server::execution::policy_adapter::ServerExecutionPolicy {
    let engine =
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine"));
    datazen_server::execution::policy_adapter::ServerExecutionPolicy::new(
        engine,
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    )
}

fn coordinator(node_limit: u64) -> Arc<datazen_server::execution::budget::ServerBudgetCoordinator> {
    Arc::new(
        datazen_server::execution::budget::ServerBudgetCoordinator::new(
            node_limit,
            10_000,
            OrganizationId::new("org-1"),
        ),
    )
}

/// membership 行（login_sessions 外键目标）+ 真登录会话行（L2）+ 真 CSRF
/// 令牌（L4，与 router 共用同一 server_secret）——table_data_routes 同款。
fn register_login(
    store: &Arc<AdminStore>,
    org: &str,
    principal: &str,
    session_id: &str,
) -> Account {
    PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership(org, principal, Role::OrgMember, &format!("idp:{principal}"))
        .expect("membership");
    LoginSessions::new(store.clone())
        .create(NewLoginSession {
            organization_id: org.to_owned(),
            session_id: session_id.to_owned(),
            principal_id: principal.to_owned(),
            auth_time: NOW.to_owned(),
            issued_at: NOW.to_owned(),
            absolute_expires_at: future_timestamp(24),
            idle_expires_at: future_timestamp(8),
            permission_version: 1,
            client_binding: "ua|client-1".to_owned(),
            mode: LoginSessionMode::Cookie,
            native_secret_hash: None,
        })
        .expect("live login session");
    let csrf_secret = CsrfSecret::new(SERVER_SECRET.to_vec()).expect("csrf secret");
    let csrf_token = csrf::mint(&csrf_secret, session_id).expect("mint csrf token");
    Account {
        cookie: format!("__Host-dz_session={session_id}; __Host-dz_csrf={csrf_token}"),
        csrf: csrf_token,
    }
}

/// 与 bootstrap 同构的最小执行链：存储 + 空驱动注册表 + 预算桥 + 执行
/// 后端 + 租约源 + `ServerResourceExecutionHost`（f7 `target_machine`
/// 夹具的同构投影；registry 一个实例两处共享）。
fn gateway_state(store: &Arc<AdminStore>) -> DriverGatewayState {
    let vault = Arc::new(
        datazen_server::execution::secret_provider::ServerSecretVault::new(
            store.clone(),
            b"driver-gateway-vault-secret".to_vec(),
        )
        .expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    let registry = Arc::new(
        datazen::execution::runtime::driver_registry::DriverRegistry::with_factories([])
            .expect("empty registry is a legal catalog"),
    );
    let bridge = Arc::new(
        datazen_server::execution::budget_bridge::BudgetPortBridge::new(
            coordinator(8),
            OrganizationId::new("org-1"),
        ),
    );
    let backend = Arc::new(
        datazen_server::execution::driver_exec::ServerDriverExecBackend::new(
            registry.clone(),
            connections.clone(),
            profiles.clone(),
            ConnectionScope::new("org-1"),
            bridge,
            None,
        ),
    );
    let lease = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles.clone(),
        Arc::new(policy_for(store)),
        backend,
        RuntimeResultSink::default(),
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new()),
    ));
    let host = Arc::new(ServerResourceExecutionHost::new(
        connections,
        profiles.clone(),
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        lease,
        "org-1",
    ));
    DriverGatewayState {
        registry,
        host,
        directory: Arc::new(
            datazen::execution::runtime::directory::InMemorySessionDirectory::new(),
        ),
        settings: datazen_server::storage::default_shared_settings().execution,
        // W8-b 装配面：本套件钉的是闸序（事件面未配置 = sse None），审计落
        // 真存储（审计断言随命令历史面测试补），时钟与 TTL 来源同 bootstrap。
        sse: None,
        audit: Arc::new(ServerAudit::new(store.clone())) as Arc<dyn AuditSink>,
        clock: Arc::new(SystemClock) as Arc<dyn Clock>,
        ephemeral: Arc::new(EphemeralSessions::new()),
        now_unix: Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or(0)
        }),
    }
}

/// 共享 fixture：同一路由 + 同一 gateway 状态（目录登记与请求走同一
/// 实例——目录闸按归属核对，共享实例让断言纯粹）。
struct SharedFixture {
    store: Arc<AdminStore>,
    app: axum::Router,
    primary: Account,
    other_org: Account,
    state: DriverGatewayState,
}

async fn shared_fixture() -> SharedFixture {
    let store = store();
    let state = gateway_state(&store);
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
        domain_routers: Some(vec![driver_gateway_router(state.clone())]),
    })
    .expect("router");
    let primary = register_login(&store, "org-1", "user-1", "sess-dg-primary");
    let other_org = register_login(&store, "org-2", "user-2", "sess-dg-org2");
    SharedFixture {
        store,
        app: router,
        primary,
        other_org,
        state,
    }
}

/// 独立 fixture（无目录登记需求的路由用）：真路由装配 + 两个账号。
async fn fixture() -> (axum::Router, Account, Account) {
    let fixture = shared_fixture().await;
    (fixture.app, fixture.primary, fixture.other_org)
}

/// 落一个真 profile + 连接配置行（f7 驱动闸测试同款全字段；返回
/// connection_id 供会话登记——profile 闸先于驱动闸，没有这行就到不了
/// 「Driver unavailable」）。
async fn create_profile_and_config(store: &Arc<AdminStore>, name: &str) -> String {
    let profiles = ServerProfiles::new(store.clone());
    let record = profiles
        .create(
            &context("org-1", "user-1"),
            ProfileDraft::new(name, "postgres"),
            &datazen::shared::id::IdempotencyKey::new(format!("idem-{name}")),
        )
        .await
        .expect("profile create");
    let vault = Arc::new(
        datazen_server::execution::secret_provider::ServerSecretVault::new(
            store.clone(),
            b"driver-gateway-vault-secret".to_vec(),
        )
        .expect("secret vault"),
    );
    let connections = ServerConnections::new(store.clone(), vault);
    connections
        .save(
            &ConnectionScope::new("org-1"),
            datazen_driver_api::ConnectionConfig {
                id: record.connection_id.as_str().to_owned(),
                name: name.to_owned(),
                database_type: "postgres".to_owned(),
                host: Some("127.0.0.1".to_owned()),
                port: Some(5432),
                database: Some("app".to_owned()),
                schema: None,
                username: None,
                password: None,
                ssl_mode: datazen_driver_api::SslMode::Disable,
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
        .expect("catalog save");
    record.connection_id.as_str().to_owned()
}

/// 在 gateway 的目录里登记一个活会话（org-1/user-1 的归属事实）。
async fn register_session(state: &DriverGatewayState, connection_id: &str) -> String {
    let owner = SessionOwner {
        db_session_id: DbSessionId::new("dbs-dg-1".to_string()),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new("user-1"),
        connection_id: ConnectionId::new(connection_id.to_string()),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("dg-test"),
            purpose: "driver-gateway-route".into(),
        },
        worker_id: WorkerId::new("local-runtime"),
        runtime_epoch: RuntimeEpoch::new("1"),
        resource_epoch: 1,
        last_business_activity: Timestamp::new(NOW),
    };
    let handle = state.directory.register(owner).await.expect("register");
    handle.db_session_id.as_str().to_owned()
}

/// oneshot 驱动认证链 Router：带 cookie（+ 可选 CSRF 头）与可选 JSON 体。
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
    let response: Response = app.clone().oneshot(request).await.expect("oneshot");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body bytes");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

// ── GET /commands：参数闸与目录闸 ─────────────────────────────────────────

#[tokio::test]
async fn commands_refuse_a_request_without_either_parameter() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/driver-gateway/commands",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "invalidArgument", "{body}");
}

#[tokio::test]
async fn commands_refuse_both_parameters_at_once() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/driver-gateway/commands?driverType=postgres&dbSessionId=dbs-dg-1",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("mutually exclusive"),
        "{body}"
    );
}

/// `get_driver_commands`：v1 空注册表下 registry 直取的真实答案（桌面同
/// 文案）；不是 501——能力在，是目录里没有这个驱动。
#[tokio::test]
async fn driver_type_commands_answer_the_honest_registry_lookup() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/driver-gateway/commands?driverType=postgres",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "notFound", "{body}");
    assert_eq!(body["message"], "Driver not found: postgres", "{body}");
}

#[tokio::test]
async fn db_session_commands_answer_the_anti_enumeration_gate_for_unknown_sessions() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/driver-gateway/commands?dbSessionId=no-such-session",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body["message"], NOT_VISIBLE_MESSAGE,
        "the anti-enumeration sentence is shared with the sessions domain: {body}"
    );
}

/// 跨组织不可见（CM-05）：org-2 对 org-1 已登记的会话拿到同一条 404。
#[tokio::test]
async fn db_session_commands_refuse_cross_org_callers() {
    let fixture = shared_fixture().await;
    let _ = register_session(&fixture.state, "conn-dg-1").await;
    let (status, body) = call(
        &fixture.app,
        "GET",
        "/api/v1/driver-gateway/commands?dbSessionId=dbs-dg-1",
        &fixture.other_org.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], NOT_VISIBLE_MESSAGE, "{body}");
}

// ── POST /executions：体闸与直取闸 ────────────────────────────────────────

#[tokio::test]
async fn executions_reject_a_body_that_cannot_parse() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/executions",
        &primary.cookie,
        Some(&primary.csrf),
        Some(serde_json::json!({ "command": 42 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn executions_refuse_a_missing_command() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/executions",
        &primary.cookie,
        Some(&primary.csrf),
        Some(serde_json::json!({ "dbSessionId": "dbs-dg-1", "input": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("missing required input property: command"),
        "{body}"
    );
}

#[tokio::test]
async fn executions_refuse_a_request_without_either_binding() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/executions",
        &primary.cookie,
        Some(&primary.csrf),
        Some(serde_json::json!({ "command": "list_databases" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("pass either driverType or dbSessionId"),
        "{body}"
    );
}

/// driverType 直取（无连接执行）：v1 空注册表下是 registry 的诚实 404。
/// W8-b 起 registry 点亮后的绿路径（ephemeral 句柄 + 同一份编排）随驱动
/// 目录自动可达，测试面不需要重写。
#[tokio::test]
async fn direct_driver_executions_answer_the_registry_404() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/executions",
        &primary.cookie,
        Some(&primary.csrf),
        Some(serde_json::json!({
            "driverType": "postgres",
            "command": "list_databases",
            "input": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], "Driver not found: postgres", "{body}");
}

// ── 会话绑定路径：profile/驱动闸（v1 空目录的真实应答）────────────────────

#[tokio::test]
async fn session_bound_executions_run_the_real_driver_gate() {
    let fixture = shared_fixture().await;
    let connection_id = create_profile_and_config(&fixture.store, "dg-driver-gate").await;
    let db_session_id = register_session(&fixture.state, &connection_id).await;
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/driver-gateway/executions",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(serde_json::json!({
            "dbSessionId": db_session_id,
            "command": "list_databases",
            "input": {}
        })),
    )
    .await;
    // 驱动闸（v1 空注册表）真跑出来的 503——先于命令编排。
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("Driver unavailable"),
        "{body}"
    );
}

#[tokio::test]
async fn db_session_commands_run_the_real_driver_gate_on_a_registered_session() {
    let fixture = shared_fixture().await;
    let connection_id = create_profile_and_config(&fixture.store, "dg-commands-gate").await;
    let db_session_id = register_session(&fixture.state, &connection_id).await;
    let (status, body) = call(
        &fixture.app,
        "GET",
        &format!("/api/v1/driver-gateway/commands?dbSessionId={db_session_id}"),
        &fixture.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("Driver unavailable"),
        "{body}"
    );
}

/// profile 闸先于驱动闸：注册会话指向一个不存在的连接 → 连接闸的失败
/// 形态（ConnectionFailed 503）。
#[tokio::test]
async fn session_bound_executions_answer_the_profile_gate_for_a_missing_connection() {
    let fixture = shared_fixture().await;
    let db_session_id = register_session(&fixture.state, "no-such-connection").await;
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/driver-gateway/executions",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(serde_json::json!({
            "dbSessionId": db_session_id,
            "command": "list_databases",
            "input": {}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("Failed to connect"),
        "{body}"
    );
}

// ── POST /stream-executions：SQL 命令收窄 + 同款闸序 ─────────────────────────────

/// 行块词汇只对 SQL 命令成立（桌面流式面同款收窄）：其余命令 400 指回
/// 非流端点；这闸先于 dbSessionId 闸（命令形状是请求自身的错误）。
#[tokio::test]
async fn stream_executions_frame_only_sql_commands() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/stream-executions",
        &primary.cookie,
        Some(&primary.csrf),
        Some(serde_json::json!({ "command": "list_databases", "input": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("stream executions frame SQL results"),
        "{body}"
    );
}

#[tokio::test]
async fn stream_executions_refuse_a_missing_db_session_id() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/stream-executions",
        &primary.cookie,
        Some(&primary.csrf),
        Some(serde_json::json!({ "command": "query", "input": { "sql": "SELECT 1" } })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("missing required input property: dbSessionId"),
        "{body}"
    );
}

#[tokio::test]
async fn stream_executions_run_the_same_gates_as_the_sync_path() {
    let fixture = shared_fixture().await;
    let connection_id = create_profile_and_config(&fixture.store, "dg-stream-gate").await;
    let db_session_id = register_session(&fixture.state, &connection_id).await;
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/driver-gateway/stream-executions",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(serde_json::json!({
            "dbSessionId": db_session_id,
            "command": "query",
            "input": { "sql": "SELECT 1" }
        })),
    )
    .await;
    // 驱动闸先于订阅登记：v1 空目录下这里是 503（闸不过不建流，域文件
    // 模块头）——投递桥（202 + streamId）只在驱动可达之后才该出现。
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("Driver unavailable"),
        "{body}"
    );
}

#[tokio::test]
async fn stream_executions_refuse_unknown_sessions_with_the_shared_404() {
    let (app, primary, _other) = fixture().await;
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/stream-executions",
        &primary.cookie,
        Some(&primary.csrf),
        Some(serde_json::json!({
            "dbSessionId": "no-such-session",
            "command": "query",
            "input": { "sql": "SELECT 1" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], NOT_VISIBLE_MESSAGE, "{body}");
}

// ── 认证链（L2/L4 由 build_router_with_auth 保证，钉两条）────────────────

#[tokio::test]
async fn unauthenticated_requests_never_reach_the_gateway() {
    let (app, _primary, _other) = fixture().await;
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/driver-gateway/commands?driverType=postgres",
        "__Host-dz_session=forged",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_without_csrf_is_rejected_by_the_chain() {
    let (app, primary, _other) = fixture().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/driver-gateway/executions",
        &primary.cookie,
        None, // 缺 CSRF 头
        Some(serde_json::json!({ "command": "list_databases", "dbSessionId": "dbs-dg-1" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
