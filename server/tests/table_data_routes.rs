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
//!   fail-closed 501；用例面（`read_table_page`）的 `TargetCommandRunner`
//!   server 适配尚不存在，测试侧仿一个驱动进不了真路由的执行半边。登记
//!   为后续切片（`TargetCommandRunner` server 实现 + 驱动注册表点开）。

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use tower::ServiceExt;

use datazen::named_views::NamedViewRepository;
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::http::domains::table_data::{table_data_router, TableDataState};
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::named_views::ServerNamedViewStore;
use datazen_server::storage::{AdminStore, LoginSessions};

const SERVER_SECRET: &[u8] = b"w3-table-data-server-secret-32-bytes!";

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
async fn fixture() -> (Arc<AdminStore>, axum::Router, Account, Account, Account) {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w3-table-data-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    let primary = register_login(&store, "org-1", "user-1", "sess-w3-primary");
    let same_org_other_principal = register_login(&store, "org-1", "user-1b", "sess-w3-1b");
    let other_org = register_login(&store, "org-2", "user-2", "sess-w3-org2");

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
/// （v9 迁移随之断言），list 原样回读。
#[tokio::test]
async fn a_view_saved_without_an_id_is_minted_with_one() {
    let (store, app, primary, _same_org, _other_org) = fixture().await;
    assert_eq!(
        store.schema_version().expect("schema version"),
        9,
        "v9 named_views migration applied"
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

// ── ⛔ fail-closed 登记（§6.7/§6.8）────────────────────────────────────────

/// driver 依赖的 tableData 路由恒 501 `capabilityUnsupported`，文案显式
/// 声明无副作用（读一条、值搜索一条探针；全表见域路由模块头状态表）。
#[tokio::test]
async fn driver_bound_table_data_routes_fail_closed_as_not_implemented() {
    let (_store, app, primary, _same_org, _other_org) = fixture().await;
    for (uri, method_name) in [
        ("/api/v1/table-data/reads", "getTableData"),
        ("/api/v1/table-data/value-searches", "searchTableValues"),
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
