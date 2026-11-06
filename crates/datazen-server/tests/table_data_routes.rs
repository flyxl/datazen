//! W3-b（Wave6 第三刀）：tableData 域生产点亮——命名视图服务端存储 +
//! 读/预览/变更路由的真路由 oneshot 契约测试。
//!
//! 装配沿 `g1_cm68_replacement_full.rs` 先例：`build_router_with_auth` +
//! `domain_routers` 槽挂真 `table_data_router` + 真 SQLite 文件库（v1..=v9
//! 迁移，`schema_version` 断言钉住 v9）+ 真 membership/login 会话行 + 真
//! cookie/CSRF 头（L2–L4 全链）+ 真实时钟戳（硬编码日期会随真实时间腐烂
//! 成 401，g1 冻结项先例）。
//!
//! 覆盖面：
//!
//! - **命名视图 §6.9 三腿（真实现，无驱动依赖，全绿）**：空目录 200 `[]`、
//!   save 铸 id 201 / 已知 id 更新 200、delete 未知 id 幂等 200 返剩余、
//!   scope 绑定持久 connectionId（真表行 + 新适配器实例回读）、跨组织
//!   不可见（CM-05，含跨组织 DELETE 不产生副作用）、跨主体不可见、
//!   application 错误码 wire（`grid.view.nameExists` → 409 /
//!   `grid.view.invalid` → 400，与 WR-C3 契约腿逐字一致）。
//! - **⛔ fail-closed 登记**：driver 依赖的 tableData 路由（§6.7/§6.8）
//!   恒 501 `capabilityUnsupported`，文案显式声明「没有执行任何语句、
//!   没有构建任何计划、没有写任何东西」。
//! - **MockDriver 读路径：接不上，不硬凑（登记）**——真路由 handler 是
//!   fail-closed 501；W8-a P2 起 `TableDataState` 已注入 server 运行器
//!   （`TableDataState.runner`）与驱动注册表（`.registry`），但十一端点
//!   接线尚未发生，handler 依旧不读它们——本夹具给空注册表 + 永不应答的
//!   占位运行器。登记为后续接线刀（501 → 真用例接线时换真运行器 +
//!   fake-driver）。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use tower::ServiceExt;

use datazen::named_views::NamedViewRepository;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::table_browsing::api::{TargetBinding, TargetCommandRunner};
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::http::domains::table_data::{table_data_router, TableDataState};
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::named_views::ServerNamedViewStore;
use datazen_server::storage::{AdminStore, LoginSessions};
use serde_json::Value as Json;

const SERVER_SECRET: &[u8] = b"w3-table-data-server-secret-32-bytes!";

/// 十一端点接线前的占位运行器（W8-a P2 形状填充）：handler 尚不读
/// `TableDataState.runner`，本件永不应答——若未来某处误调，显式拒绝而不是
/// 佯装成功（fail-closed 与生产同则）。
struct FixtureRunner;

#[async_trait]
impl TargetCommandRunner for FixtureRunner {
    async fn run(
        &self,
        _context: &RequestContext,
        _binding: &TargetBinding,
        _command: &str,
        _input: Json,
    ) -> Result<Vec<Json>, ApiError> {
        Err(ApiError::new(
            ApiErrorCode::CapabilityUnsupported,
            "tableData endpoints are not wired in this fixture yet",
        ))
    }
}

/// 真实时钟戳（g1 冻结项先例）：登录会话的过期面以真实时钟判定，
/// 硬编码日期会随真实时间腐烂成 401。
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn future_timestamp(hours: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::hours(hours))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

struct Account {
    cookie: String,
    csrf: String,
}

/// membership 行（login_sessions 的外键目标，g2 冻结项先例）经
/// `PolicyService` 的管理用例落行 + 真登录会话行（L2）+ 真 CSRF 令牌
/// （L4，与 router 共用同一 server_secret）。
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
            auth_time: now_rfc3339(),
            issued_at: now_rfc3339(),
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

/// 真路由装配：真 SQLite 文件库（v9 含在内）+ `table_data_router` 进
/// `domain_routers` 槽。返回库里三个账号：主账号（org-1/user-1）、同组织
/// 异主体（org-1/user-1b）、异组织（org-2/user-2）。
///
/// W8-a 接线刀起 `TableDataState` 需要会话面：本夹具装配最小租约链
/// （f7_wiring_tests 同构；空注册表 + 占位 runner——本夹具只测命名视图与
/// 501 探针，读三端点的绿路径在下方 `wired_fixture`）。
async fn fixture() -> (Arc<AdminStore>, axum::Router, Account, Account, Account) {
    use datazen::execution::application::orchestration::RuntimeResultSink;
    use datazen::execution::runtime::directory::InMemorySessionDirectory;
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::lease::ServerRuntimeLeaseSource;
    use datazen_server::execution::lease_use_cases::LeaseConnectionUseCases;
    use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
    use datazen_server::execution::secret_provider::ServerSecretVault;
    use datazen_server::storage::{ServerConnections, ServerProfiles};

    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w3-table-data-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    let primary = register_login(&store, "org-1", "user-1", "sess-w3-primary");
    let same_org_other_principal = register_login(&store, "org-1", "user-1b", "sess-w3-1b");
    let other_org = register_login(&store, "org-2", "user-2", "sess-w3-org2");

    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    let bridge = Arc::new(BudgetPortBridge::new(
        Arc::new(
            datazen_server::execution::budget::ServerBudgetCoordinator::new(
                8,
                10_000,
                datazen::shared::id::OrganizationId::new("org-1"),
            ),
        ),
        datazen::shared::id::OrganizationId::new("org-1"),
    ));
    let backend = Arc::new(ServerDriverExecBackend::new(
        registry.clone(),
        connections.clone(),
        profiles.clone(),
        datazen::connection::domain::catalog::ConnectionScope::new("org-1"),
        bridge,
        None,
    ));
    let policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let lease_source = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles,
        policy,
        backend,
        RuntimeResultSink::default(),
        Arc::new(InMemorySessionDirectory::new()),
    ));
    let lease_cases = Arc::new(LeaseConnectionUseCases::new(lease_source));

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
        domain_routers: Some(vec![table_data_router(TableDataState {
            store: store.clone(),
            registry,
            runner: Arc::new(FixtureRunner),
            profiles: Arc::new(datazen_server::storage::ServerProfiles::new(store.clone())),
            connections,
            directory: Arc::new(InMemorySessionDirectory::new()),
            sessions: lease_cases,
        })]),
    })
    .expect("router");
    (store, router, primary, same_org_other_principal, other_org)
}

/// oneshot 驱动认证链 Router：带 cookie（+ 可选 CSRF 头）与可选 JSON 体。
/// 返回 `(status, json)`；空体回 `Null`（g1 先例同款）。
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

/// §6.9 的 `{ view }` 保存体：空 id 请求铸造。
fn view_draft(
    name: &str,
    connection_id: &str,
    database: serde_json::Value,
    schema: serde_json::Value,
    table: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": "",
        "name": name,
        "scope": {
            "connectionId": connection_id,
            "database": database,
            "schema": schema,
            "table": table,
        },
        "createdAt": now_rfc3339(),
    })
}

async fn save(
    app: &axum::Router,
    account: &Account,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    call(
        app,
        "PUT",
        "/api/v1/named-views",
        &account.cookie,
        Some(&account.csrf),
        Some(serde_json::json!({ "view": body })),
    )
    .await
}

// ── §6.9 命名视图（✅ 真实现）──────────────────────────────────────────────

/// 空目录：GET 无 scope 参数 → 200 `[]`（缺省列全部）。
#[tokio::test]
async fn named_views_list_an_empty_directory() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, serde_json::json!([]), "a fresh directory is empty");
}

/// save 铸 id：空 id → 201，铸造非空 id、时间戳齐备，记录真实落库
/// （v10 迁移随之断言），list 原样回读。
#[tokio::test]
async fn a_view_saved_without_an_id_is_minted_with_one() {
    let (store, app, primary, _same_org, _other_org) = fixture().await;
    assert_eq!(
        store.schema_version().expect("schema version"),
        10,
        "v10 org-memory migration applied"
    );

    let draft = view_draft(
        "Alpha",
        "conn-1",
        serde_json::json!("app"),
        serde_json::json!(null),
        "orders",
    );
    let (status, body) = save(&app, &primary, draft).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a minted save answers 201: {body}"
    );
    let minted_id = body["id"].as_str().expect("minted id").to_owned();
    assert!(!minted_id.is_empty(), "the backend mints a non-empty id");
    assert_eq!(body["name"], "Alpha");
    assert_eq!(body["scope"]["connectionId"], "conn-1");
    assert!(body["createdAt"].is_string(), "createdAt stamped");
    assert!(body["updatedAt"].is_string(), "updatedAt stamped");

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let views = body.as_array().expect("a NamedView array");
    assert_eq!(views.len(), 1, "the minted record is stored");
    assert_eq!(views[0]["id"], minted_id);
    assert_eq!(views[0]["scope"]["table"], "orders");
}

/// 已知 id 更新：同一 id 换名 → 200（不是 201），身份保持、集合不增多。
#[tokio::test]
async fn a_save_with_a_known_id_updates_in_place() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    let draft = view_draft(
        "Alpha",
        "conn-1",
        serde_json::json!("app"),
        serde_json::json!(null),
        "orders",
    );
    let (status, body) = save(&app, &primary, draft).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let minted_id = body["id"].as_str().expect("minted id").to_owned();

    let update = serde_json::json!({
        "id": minted_id,
        "name": "Beta",
        "scope": {
            "connectionId": "conn-1",
            "database": "app",
            "schema": null,
            "table": "orders",
        },
        "createdAt": now_rfc3339(),
    });
    let (status, body) = save(&app, &primary, update).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an update answers 200, not 201: {body}"
    );
    assert_eq!(body["id"], minted_id, "the identity is kept");
    assert_eq!(body["name"], "Beta");

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let views = body.as_array().expect("a NamedView array");
    assert_eq!(views.len(), 1, "an update does not add a record");
    assert_eq!(views[0]["name"], "Beta");
}

/// DELETE 未知 id 是幂等 no-op：空目录与有记录两种状态下都 200 返剩余。
#[tokio::test]
async fn a_delete_of_an_unknown_id_is_an_idempotent_no_op() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    let (status, body) = call(
        &app,
        "DELETE",
        "/api/v1/named-views/does-not-exist",
        &primary.cookie,
        Some(&primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        serde_json::json!([]),
        "nothing was there, nothing remains"
    );

    // 有记录时删另一个未知 id：剩余集合原样返回。
    let draft = view_draft(
        "Alpha",
        "conn-1",
        serde_json::json!("app"),
        serde_json::json!(null),
        "orders",
    );
    let (status, body) = save(&app, &primary, draft).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = call(
        &app,
        "DELETE",
        "/api/v1/named-views/still-does-not-exist",
        &primary.cookie,
        Some(&primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let views = body.as_array().expect("remaining collection");
    assert_eq!(views.len(), 1, "the unknown id left no mark");
    assert_eq!(views[0]["name"], "Alpha");
}

/// DELETE 已知 id → 200 返剩余集合（§6.9 的域客户端契约：响应是剩余
/// `NamedView[]`）。
#[tokio::test]
async fn a_delete_returns_the_remaining_collection() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    for name in ["Alpha", "Beta"] {
        let draft = view_draft(
            name,
            "conn-1",
            serde_json::json!("app"),
            serde_json::json!(null),
            "orders",
        );
        let (status, _) = save(&app, &primary, draft).await;
        assert_eq!(status, StatusCode::CREATED, "{name}");
    }
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views?connectionId=conn-1&database=app&table=orders",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let views = body.as_array().expect("a NamedView array");
    assert_eq!(views.len(), 2);
    let alpha_id = views
        .iter()
        .find(|view| view["name"] == "Alpha")
        .expect("Alpha present")["id"]
        .as_str()
        .expect("id")
        .to_owned();

    let (status, body) = call(
        &app,
        "DELETE",
        &format!("/api/v1/named-views/{alpha_id}"),
        &primary.cookie,
        Some(&primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let remaining = body.as_array().expect("remaining collection");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["name"], "Beta");
}

/// scope 绑定持久 connectionId：scope 四元组过滤按 `connectionId`+`table`
/// 同时在场成过滤（权威裁决）；新适配器实例回读证明 scope 列真实落库
/// （含可空的 database/schema 投影）。
#[tokio::test]
async fn the_scope_binding_persists_the_connection_id() {
    let (store, app, primary, _same_org, _other_org) = fixture().await;
    let draft = view_draft(
        "Orders",
        "conn-1",
        serde_json::json!("app"),
        serde_json::json!("public"),
        "orders",
    );
    let (status, _) = save(&app, &primary, draft).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views?connectionId=conn-1&database=app&schema=public&table=orders",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body.as_array().expect("array").len(),
        1,
        "the scope matches"
    );

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views?connectionId=conn-2&database=app&table=orders",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!([]),
        "another connection's scope is empty"
    );

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.as_array().expect("array").len(),
        1,
        "no filter lists all"
    );

    // 新适配器实例（同一 org/principal）回读：scope 投影列真实持久。
    let fresh = ServerNamedViewStore::scoped("org-1", "user-1", store.clone());
    let loaded = fresh.load_views().await.expect("fresh adapter read");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].scope.connection_id, "conn-1");
    assert_eq!(loaded[0].scope.database.as_deref(), Some("app"));
    assert_eq!(loaded[0].scope.schema.as_deref(), Some("public"));
    assert_eq!(loaded[0].scope.table, "orders");
}

/// 跨组织不可见（CM-05）：org-2 读到空目录；org-2 对 org-1 的视图 id 发
/// DELETE 是无操作（不产生副作用）；org-1 的记录原样保留。
#[tokio::test]
async fn named_views_are_invisible_across_organizations() {
    let (_store, app, primary, _same_org, other_org) = fixture().await;
    let draft = view_draft(
        "Alpha",
        "conn-1",
        serde_json::json!("app"),
        serde_json::json!(null),
        "orders",
    );
    let (status, body) = save(&app, &primary, draft).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let minted_id = body["id"].as_str().expect("minted id").to_owned();

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views",
        &other_org.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!([]),
        "another org sees an empty directory"
    );

    let (status, body) = call(
        &app,
        "DELETE",
        &format!("/api/v1/named-views/{minted_id}"),
        &other_org.cookie,
        Some(&other_org.csrf),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "cross-org delete is a no-op: {body}"
    );
    assert_eq!(
        body,
        serde_json::json!([]),
        "the id is not in their collection"
    );

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views",
        &primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body.as_array().expect("array").len(),
        1,
        "the cross-org delete had no side effect"
    );
}

/// 跨主体不可见（§6.9 per principal + scope）：同组织的第二个主体读到
/// 空目录。
#[tokio::test]
async fn named_views_are_scoped_per_principal() {
    let (_store, app, primary, same_org_other_principal, _other_org) = fixture().await;
    let draft = view_draft(
        "Alpha",
        "conn-1",
        serde_json::json!("app"),
        serde_json::json!(null),
        "orders",
    );
    let (status, _) = save(&app, &primary, draft).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/named-views",
        &same_org_other_principal.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!([]),
        "another principal sees nothing"
    );
}

/// 名字唯一性是 scope 内大小写不敏感：同 scope 撞名 → 409 +
/// `grid.view.nameExists`（application 契约码进 wire 的 `code` 字段，
/// WR-C3 契约腿逐字一致）；另一 scope 同名合法。
#[tokio::test]
async fn a_name_conflict_inside_one_scope_is_refused_with_the_application_code() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    let (status, _) = save(
        &app,
        &primary,
        view_draft(
            "Alpha",
            "conn-1",
            serde_json::json!("app"),
            serde_json::json!(null),
            "orders",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = save(
        &app,
        &primary,
        view_draft(
            "alpha",
            "conn-1",
            serde_json::json!("app"),
            serde_json::json!(null),
            "orders",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["code"], "grid.view.nameExists",
        "the application contract code rides the wire"
    );
    assert!(body["message"].is_string());

    // 另一 scope 的同名合法（scope 之间是不同的「表」）。
    let (status, _) = save(
        &app,
        &primary,
        view_draft(
            "Alpha",
            "conn-2",
            serde_json::json!("app"),
            serde_json::json!(null),
            "orders",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

/// 非法草稿（空名字）→ 400 + `grid.view.invalid`。
#[tokio::test]
async fn an_invalid_draft_is_refused_with_the_application_code() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    let (status, body) = save(
        &app,
        &primary,
        view_draft(
            "   ",
            "conn-1",
            serde_json::json!("app"),
            serde_json::json!(null),
            "orders",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "grid.view.invalid");
}

// ── ⛔ fail-closed 登记（§6.7/§6.8 剩余八端点）──────────────────────────────

/// 变更与 FK/值搜索路由恒 501 `capabilityUnsupported`，文案显式声明无副作用
/// 且携带逐端口收窄原因（读三端点已真实现，见下方绿路径腿）。
#[tokio::test]
async fn driver_bound_table_data_routes_fail_closed_as_not_implemented() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    for (uri, method_name) in [
        ("/api/v1/table-data/value-searches", "searchTableValues"),
        (
            "/api/v1/table-data/foreign-key-targets",
            "resolveForeignKeyTargets",
        ),
        ("/api/v1/table-data/change-commits", "commitPendingChanges"),
        ("/api/v1/table-data/result-origins", "resolveResultOrigin"),
    ] {
        let (status, body) = call(
            &app,
            "POST",
            uri,
            &primary.cookie,
            Some(&primary.csrf),
            Some(serde_json::json!({ "connectionId": "conn-1" })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{uri}: {body}");
        assert_eq!(body["code"], "capabilityUnsupported", "{uri}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains(method_name), "{uri}: {message}");
        assert!(
            message.contains("No statement ran"),
            "the copy must rule out silent side effects: {message}"
        );
    }
}

/// 命名视图面在认证链内：无 cookie 的请求在 L2 被 401 拒绝，不泄露目录。
#[tokio::test]
async fn the_named_view_face_sits_inside_the_auth_chain() {
    let (_store, app, _primary, _same_org, _other_org) = fixture().await;
    let (status, _) = call(&app, "GET", "/api/v1/named-views", "", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── ✅ 读三端点真实现（W8-a 接线刀）─────────────────────────────────────────
//
// 装配与 bootstrap 同构（`f7_wiring_tests` 先例 + 桌面 fake-driver 腿同源）：
// 真 SQLite 管理库（profile 行 + 连接目录行）+ MockDriver 工厂注册进
// `DriverRegistry::register_test_factory`（`datazen` test-harness 门后面）
// + `LegacyResourceAdapter` 资源提供者（共享层件：命令 → MockDriver 应答 →
// `kind: rows` / `commandResult` 产物）+ 预算桥 + `ServerDriverExecBackend`
// + `ServerRuntimeLeaseSource` + `ServerTargetRunner`。结果发布钩子与租约
// 源共享同一 `RuntimeResultSink`（bootstrap 既有纪律）。会话经真
// `open_session` 用例开立（目录 = 租约源同一实例）。

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::orchestration::RuntimeResultSink;
use datazen::execution::application::requests::OpenSessionRequest;
use datazen::execution::target::NamespaceTarget;
use datazen::shared::context::OwnerRef;
use datazen::shared::id::{ClientInstanceId, EditorSessionId, IdempotencyKey};
use datazen_driver_api::capabilities;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory, SslMode, Value};

/// sessions 域同一条固定文案（防 ID 枚举口径的断言基准）。
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE as NOT_VISIBLE_MESSAGE_SENTINEL;

/// 把 [`MockDriver`] 连同 [`LegacyResourceAdapter`] 包成静态工厂
/// （`register_test_factory` 吃 `&'static dyn DatabaseDriverFactory`；
/// desktop `TestMockDriverFactory` 的 server 端最小同构）。
struct WireMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn DatabaseDriver>,
}

impl DatabaseDriverFactory for WireMockFactory {
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
        let mut capabilities = capabilities::CapabilitySet::default();
        capabilities.stateful_session = capabilities::Availability::Supported;
        capabilities.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
        capabilities.context_observation = capabilities::ContextObservation::Full;
        capabilities.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
        capabilities.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
        Some(Arc::new(LegacyResourceAdapter::new(
            Arc::clone(&self.driver),
            self.driver_id,
            "wire-test",
            1,
            shape,
            capabilities,
        )))
    }
}

/// 会话连续性的测试形（`g1_w1_gaps.rs` 先例）：主线
/// `ServerRuntimeLeaseSource` 每 resolve 新建 runtime 私有的 sessions/tokens
/// map——issue/consume 跨 resolve 必然失联（g1 CM-63 登记）。夹具把
/// 「未吊销的 runtime」定为单例，固定租约源恒返同一实例，会话/令牌的
/// 签发与消费落在同一份内存账上；执行后端（SessionBackend）与共享
/// sink/directory 仍与主线同构。
struct FixedLeaseSource(Arc<datazen::execution::application::RuntimeConnectionUseCases>);

#[async_trait::async_trait]
impl datazen_server::execution::lease::RuntimeLeaseSource for FixedLeaseSource {
    async fn resolve(
        &self,
    ) -> Result<Arc<datazen::execution::application::RuntimeConnectionUseCases>, String> {
        Ok(self.0.clone())
    }
}

/// 接线夹具：真 runner + 真闸序 + MockDriver 全栈。
struct WireFixture {
    _store: Arc<AdminStore>,
    app: axum::Router,
    primary: Account,
    other_org: Account,
    connection_id: String,
    sessions: Arc<dyn datazen::execution::application::use_cases::ConnectionUseCases>,
    ctx: RequestContext,
}

async fn wired_fixture(opts: MockDriverOptions) -> WireFixture {
    use datazen::execution::application::orchestration::{
        RuntimeConnectionUseCases, SystemRuntimeClock,
    };
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::lease::ServerRuntimeLeaseSource;
    use datazen_server::execution::publisher::execution_result_publisher;
    use datazen_server::execution::resource_host::ServerResourceExecutionHost;
    use datazen_server::execution::secret_provider::ServerSecretVault;
    use datazen_server::execution::target_runner::ServerTargetRunner;
    use datazen_server::storage::{ServerConnections, ServerProfiles, ServerSettingsStore};

    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w8-table-data-wire-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    let primary = register_login(&store, "org-1", "user-1", "sess-w8-primary");
    let other_org = register_login(&store, "org-2", "user-2", "sess-w8-org2");

    let ctx = RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        // HTTP 面的 SessionIdentity 固定铸造 `http`（router.rs 提取器）；
        // open_session 的 owner 校验（check_owner）要求 owner 与 ctx 同一
        // 客户端实例，fixture ctx 随之对齐。
        ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-w8-wire"),
        None,
    );

    // 驱动注册表：懒形 + 测试驱动 + 测试工厂（生产缺省形态零依赖边的对偶
    // 面；`register_test_driver` 供 `get_sql_driver_by_name` 直取，
    // `register_test_factory` 供 `resource_provider` 闸）。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let driver = MockDriver::new("mockdb", opts);
    registry
        .register_test_driver("mockdb", driver.clone())
        .await;
    registry
        .register_test_factory(
            "mockdb",
            Box::leak(Box::new(WireMockFactory {
                driver_id: "mockdb",
                driver: driver.clone(),
            })),
        )
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), b"w8-wire-secret".to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    // profile 行 + 连接目录行（同一连接 id 两半都齐，闸序可达驱动层）。
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("wire-conn", "mockdb"),
            &IdempotencyKey::new("idem-w8-wire-profile"),
        )
        .await
        .expect("profile create");
    let connection_id = record.connection_id.as_str().to_owned();
    let scope = ConnectionScope::new("org-1");
    connections
        .save(
            &scope,
            datazen_driver_api::ConnectionConfig {
                id: connection_id.clone(),
                name: "wire conn".into(),
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

    // 执行后端 + 结果发布钩子 + 共享 sink/directory（bootstrap 同构）。
    let bridge = Arc::new(BudgetPortBridge::new(
        Arc::new(
            datazen_server::execution::budget::ServerBudgetCoordinator::new(
                8,
                10_000,
                datazen::shared::id::OrganizationId::new("org-1"),
            ),
        ),
        datazen::shared::id::OrganizationId::new("org-1"),
    ));
    let backend = Arc::new(ServerDriverExecBackend::new(
        registry.clone(),
        connections.clone(),
        profiles.clone(),
        scope,
        bridge,
        None,
    ));
    let sink = RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let policy = Arc::new(
        datazen_server::execution::policy_adapter::ServerExecutionPolicy::new(
            Arc::new(
                datazen_server::policy::PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
                    .expect("policy engine"),
            ),
            store.clone(),
            Arc::new(datazen::shared::clock::SystemClock),
        ),
    );
    let directory =
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
    // 会话面单例（FixedLeaseSource 先例注释）：签发/消费同一 runtime。
    let runtime = Arc::new(RuntimeConnectionUseCases::new_with_parts(
        profiles.clone(),
        policy.clone(),
        backend.clone(),
        Arc::new(SystemRuntimeClock(std::time::Instant::now()))
            as Arc<dyn datazen::execution::runtime::gateway::MonotonicClock>,
        sink.clone(),
        directory.clone(),
    ));
    // 资源宿主（target 分支闸序）持真租约源——prepare 的 resolve 自洽
    // （闸序 + 用例面同源于 PreparedExecution）。
    let real_lease = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles.clone(),
        policy,
        backend,
        sink,
        directory.clone(),
    ));
    let host = Arc::new(ServerResourceExecutionHost::new(
        connections.clone(),
        profiles.clone(),
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        real_lease,
        "org-1",
    ));
    // runner 的 manual 分支吃固定租约源（会话连续性），target 分支经 host
    // 的真租约源自洽。
    let runner = Arc::new(ServerTargetRunner::new(
        host,
        Arc::new(FixedLeaseSource(runtime.clone()))
            as Arc<dyn datazen_server::execution::lease::RuntimeLeaseSource>,
    ));

    let app = build_router_with_auth(AuthDeps {
        store: store.clone(),
        policy: Arc::new(
            datazen_server::policy::PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
                .expect("policy"),
        ),
        server_secret: SERVER_SECRET.to_vec(),
        idle_ttl_seconds: 3600,
        rate_limit: None,
        sse: None,
        oidc: None,
        domains: None,
        mcp: None,
        domain_routers: Some(vec![table_data_router(TableDataState {
            store: store.clone(),
            registry,
            runner,
            profiles,
            connections,
            directory,
            sessions: runtime.clone(),
        })]),
    })
    .expect("router");

    WireFixture {
        _store: store,
        app,
        primary,
        other_org,
        connection_id,
        sessions: runtime,
        ctx,
    }
}

impl WireFixture {
    /// 经真 `open_session` 用例开立会话，返回 dbSessionId。幂等键走
    /// 预签发令牌（用例层核验签名，_sessions 域同一令牌面）。
    async fn open_session(&self, namespace_database: &str) -> String {
        use datazen::execution::application::idempotency::IdempotentOperation;
        let target = datazen::execution::ExecutionTarget::new(
            datazen::shared::id::ConnectionId::new(self.connection_id.clone()),
            NamespaceTarget::new(Some(namespace_database.to_owned()), None, None, Vec::new()),
        );
        let token = self
            .sessions
            .issue_submission_token(&self.ctx, IdempotentOperation::OpenSession, None)
            .await
            .expect("submission token issued");
        let receipt = self
            .sessions
            .open_session(
                &self.ctx,
                OpenSessionRequest::new(
                    target,
                    OwnerRef::Editor {
                        // HTTP 面的 SessionIdentity 固定铸造 `http` 客户端
                        // 实例（router.rs 提取器）；会话归属必须与之一致，
                        // 否则 session_entry 的 ctx 过滤判 SessionNotFound。
                        client_instance_id: ClientInstanceId::new("http"),
                        editor_session_id: EditorSessionId::new("wire-editor-1"),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .expect("session opens through the real use case");
        receipt.session.handle.db_session_id.as_str().to_owned()
    }

    /// `getTableData` 的最小合法体（camelCase 契约；dbSessionId 是共享层
    /// 上下文校验的必填字段——桌面网格读携带同一上下文词汇）。
    fn read_body(&self, database: &str) -> serde_json::Value {
        serde_json::json!({
            "context": {
                "connectionId": self.connection_id,
                "dbSessionId": "wire-grid-session",
                "driverType": "mockdb",
                "database": database,
                "schema": null,
                "table": "users",
            },
            "page": 1,
            "pageSize": 50,
        })
    }
}

/// 读三端点在认证链内：无 cookie 的读在 L2 被 401 拒绝。
#[tokio::test]
async fn the_wired_read_face_sits_inside_the_auth_chain() {
    let fixture = wired_fixture(MockDriverOptions::default()).await;
    let (status, _) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/reads",
        "",
        None,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// `getTableData` 绿路径：真 runner target 分支（prepare 闸序 → CAS → 执行
/// → 轮询 → 产物收集）经 MockDriver 走通，200 `TableDataResult` 携带行。
/// `skipCount` 的裁决：MockDriver 的 `query_multi` 是单语句实现，双语句
/// batch（count + rows）只会应答 COUNT 拦截——桌面网格同款限制，与生产
/// 驱动的 batch 能力无关；行集绿路径因此以 skip_count 读证明。
#[tokio::test]
async fn a_table_read_runs_through_the_real_runner_and_answers_rows() {
    let mut opts = MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]];
    let fixture = wired_fixture(opts).await;
    let mut body = fixture.read_body("app");
    body["skipCount"] = serde_json::json!(true);
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/reads",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rows = body["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 2, "the page rows arrive: {body}");
    assert_eq!(rows[0][0], serde_json::json!(1), "{body}");
    assert!(
        body["columns"].as_array().expect("columns").len() > 0,
        "the schema round trip through the runner answers columns: {body}"
    );
    assert_eq!(
        body["totalRowsKind"],
        serde_json::Value::Null,
        "skip_count reuses the caller's total: {body}"
    );
}

/// 未知连接的读 → 404（profile 闸先于一切执行面触碰）。
#[tokio::test]
async fn a_read_of_an_unknown_connection_is_refused_with_not_found() {
    let mut opts = MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(Value::Integer(1))]];
    let fixture = wired_fixture(opts).await;
    let mut body = fixture.read_body("app");
    body["context"]["connectionId"] = serde_json::json!("no-such-connection");
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/reads",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], "Connection profile unavailable", "{body}");
}

/// 驱动类型漂移的读 → 400（profile 存在但请求声称的 driverType 与目录
/// 不符——desktop「Table connection context changed」同款拒绝）。
#[tokio::test]
async fn a_read_whose_driver_type_drifted_is_refused() {
    let fixture = wired_fixture(MockDriverOptions::default()).await;
    let mut body = fixture.read_body("app");
    body["context"]["driverType"] = serde_json::json!("postgres");
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/reads",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        body["message"], "Table connection context changed",
        "{body}"
    );
}

/// `previewTableReadSql` 绿路径：同一闸序，200 预览体（数据语句 + 计数
/// 语句 + 精确计数 + kind），且不执行任何语句（MockDriver query 计数为 0
/// 无法直接断言——预览用例本身不触 runner 执行，体形状即证据）。
#[tokio::test]
async fn a_sql_preview_assembles_statements_without_running_them() {
    let fixture = wired_fixture(MockDriverOptions::default()).await;
    let body = serde_json::json!({
        "context": {
            "connectionId": fixture.connection_id,
            "dbSessionId": "wire-grid-session",
            "driverType": "mockdb",
            "database": "app",
            "schema": null,
            "table": "users",
        },
        "page": 1,
        "pageSize": 50,
    });
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/read-sql-previews",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data_sql = body["dataSql"].as_str().expect("dataSql");
    assert!(data_sql.to_uppercase().contains("SELECT"), "{data_sql}");
    assert!(data_sql.to_uppercase().contains("USERS"), "{data_sql}");
    assert!(
        body["exactCountSql"].as_str().is_some(),
        "the exact count statement is always present: {body}"
    );
    assert_eq!(body["countKind"], "exact", "{body}");
}

/// `estimateTableRows` 绿路径（会话面）：真 open_session → 目录可见性闸 →
/// get_session → 估算语句经 runner manual 分支在会话内执行 → 200 数字。
#[tokio::test]
async fn an_estimate_runs_in_the_session_and_answers_a_number() {
    let mut opts = MockDriverOptions::default();
    opts.count_estimate_rows = Some(12345);
    let fixture = wired_fixture(opts).await;
    let db_session_id = fixture.open_session("app").await;
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/row-count-estimates",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(serde_json::json!({
            "dbSessionId": db_session_id,
            "table": "users",
            "database": "app",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        serde_json::json!(12345),
        "the estimate rides the wire: {body}"
    );
}

/// 驱动不能估算 → 200 `null`（答案，不是错误；**没有**隐藏的精确计数回退
/// ——估算用例从不跑 `COUNT(*)`）。
#[tokio::test]
async fn an_estimate_of_an_unsupporting_driver_answers_null() {
    // MockDriverOptions::default() 的 count_estimate_rows = None：
    // count_estimate_sql 返回 None，估算语句根本不存在。
    let fixture = wired_fixture(MockDriverOptions::default()).await;
    let db_session_id = fixture.open_session("app").await;
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/row-count-estimates",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(serde_json::json!({
            "dbSessionId": db_session_id,
            "table": "users",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        serde_json::Value::Null,
        "null is the honest answer: {body}"
    );
}

/// 未知/越组织的会话 id → 404 固定文案（sessions 域同一条，防 ID 枚举）。
#[tokio::test]
async fn an_estimate_of_an_unknown_session_is_not_visible() {
    let fixture = wired_fixture(MockDriverOptions::default()).await;
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/row-count-estimates",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(serde_json::json!({
            "dbSessionId": "no-such-session",
            "table": "users",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], NOT_VISIBLE_MESSAGE_SENTINEL, "{body}");
}

/// 越组织主体的估算 → 404（可见性先于会话存活；org-2 看不见 org-1 的会话）。
#[tokio::test]
async fn an_estimate_across_organizations_is_not_visible() {
    let fixture = wired_fixture(MockDriverOptions::default()).await;
    let db_session_id = fixture.open_session("app").await;
    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/table-data/row-count-estimates",
        &fixture.other_org.cookie,
        Some(&fixture.other_org.csrf),
        Some(serde_json::json!({
            "dbSessionId": db_session_id,
            "table": "users",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], NOT_VISIBLE_MESSAGE_SENTINEL, "{body}");
}
