//! Workflow 域路由测试（W8 刀3c-2；契约 §6.14）。
//!
//! 夹具 = `execution_routes` fullstack 同构：真 SQLite 管理库 + membership/
//! login 会话 + MockDriver（query 应答）+ 真租约源/执行后端/
//! `ServerResourceExecutionHost`（与 bootstrap 同一装配链，只去掉无关域）。
//! 令牌闸按在场性核对（域路由口径；HMAC 签发面在 transfer 域测试覆盖），
//! 测试体直接携带 `submissionToken` 形状。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use datazen_driver_api::SslMode;

use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, LoginSessions, NewLoginSession};
use datazen_server::storage::{
    AdminStore, ServerConnections, ServerProfiles, ServerSettingsStore, ServerWorkflowHistoryStore,
    ServerWorkflowStore,
};

const SERVER_SECRET: &[u8] = b"w8-workflow-routes-secret";

// ── 账号与请求助手（execution_routes 同款） ─────────────────────────────────

struct Account {
    cookie: String,
    csrf: String,
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn future_timestamp(hours: u64) -> String {
    (chrono::Utc::now() + chrono::Duration::hours(hours as i64)).to_rfc3339()
}

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

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf_token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, cookie);
    if let Some(token) = csrf_token {
        builder = builder.header(csrf::CSRF_HEADER, token);
    }
    let request = match body {
        Some(value) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string()))
            .expect("request"),
        None => builder.body(Body::empty()).expect("request"),
    };
    let response = app.clone().oneshot(request).await.expect("in-flight");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("body bytes");
    let text = String::from_utf8_lossy(&bytes);
    let json = if text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text.into_owned()))
    };
    (status, json)
}

/// 幂等键随机铸造（令牌在场性核对的绑定面）。
fn idem(tag: &str) -> String {
    format!("{tag}-{}", uuid::Uuid::new_v4())
}

// ── 夹具 ────────────────────────────────────────────────────────────────────

struct WireFixture {
    app: axum::Router,
    primary: Account,
    other_org: Account,
    connection_id: String,
    history: ServerWorkflowHistoryStore,
}

/// 一条最小 workflow 定义（JSON 体——与桌面 IPC 的 `WorkflowDefinition`
/// 同一 serde 词汇）。
fn query_workflow_json(id: &str, connection_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": format!("WF {id}"),
        "description": "route test",
        "steps": [
            { "type": "query", "id": "q1", "sql": "SELECT 1" }
        ],
        "connection": connection_id,
        "visibility": "user"
    })
}

fn data_only_workflow_json(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": format!("WF {id}"),
        "description": "data only",
        "steps": [
            {
                "type": "transform", "id": "t1",
                "from": "[{\"a\":\"x\",\"amount\":10},{\"a\":\"y\",\"amount\":30}]",
                "add_columns": [{ "name": "taxed", "expr": "amount * 1.1" }]
            },
            {
                "type": "merge", "id": "m1",
                "sources": [
                    { "source": "steps.t1.rows", "columns": {}, "add": { "src": "CALC" } },
                    { "source": "[{\"a\":\"z\",\"amount\":1}]", "columns": {}, "add": { "src": "LIT" } }
                ]
            }
        ],
        "visibility": "user"
    })
}

/// 保存一条 workflow（走真 PUT 路由——夹具装配断言前的入库通道）。
async fn put_workflow(
    app: &axum::Router,
    account: &Account,
    id: &str,
    definition: serde_json::Value,
) -> StatusCode {
    let key = idem("seed");
    let (status, body) = call(
        app,
        "PUT",
        &format!("/api/v1/workflows/{id}"),
        &account.cookie,
        Some(&account.csrf),
        Some(
            serde_json::json!({
                "submissionToken": { "idempotencyKey": key },
                "idempotencyKey": key,
                "definition": definition,
            })
            .as_object()
            .map(|extra| {
                let mut merged = extra.clone();
                if let Some(obj) = definition.as_object() {
                    for (k, v) in obj {
                        merged.insert(k.clone(), v.clone());
                    }
                }
                serde_json::Value::Object(merged)
            })
            .unwrap_or_default(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "seed save: {body}");
    status
}

async fn wired_fixture() -> WireFixture {
    use datazen_server::execution::budget::ServerBudgetCoordinator;
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::lease::ServerRuntimeLeaseSource;
    use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
    use datazen_server::execution::publisher::execution_result_publisher;
    use datazen_server::execution::resource_host::ServerResourceExecutionHost;
    use datazen_server::execution::secret_provider::ServerSecretVault;

    let store = Arc::new(AdminStore::open_in_memory().expect("memory store"));
    store.migrate().expect("migrations");

    let primary = register_login(&store, "org-1", "user-1", "sess-wf-primary");
    let other_org = register_login(&store, "org-2", "user-2", "sess-wf-other");

    let ctx = datazen::shared::context::RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        datazen::shared::id::ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-wf-routes"),
        None,
    );

    // MockDriver：query 应答一行 Integer(1)。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let mut opts = datazen_driver_api::mock_driver::MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(datazen_driver_api::Value::Integer(1))]];
    let driver = datazen_driver_api::mock_driver::MockDriver::new("mockdb", opts);
    registry
        .register_test_driver("mockdb", driver.clone())
        .await;
    struct RouteMockFactory {
        driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    }
    impl datazen_driver_api::DatabaseDriverFactory for RouteMockFactory {
        fn driver_id(&self) -> &'static str {
            "mockdb"
        }
        fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
            Arc::clone(&self.driver)
        }
        // resource provider 合同（execution_routes RouteMockFactory 同款）：
        // resource_host.prepare 的闸门需要命名空间形状与能力快照。
        fn resource_provider(
            &self,
        ) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
            use datazen_driver_api::capabilities;
            use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
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
                "mockdb",
                "workflow-routes",
                1,
                shape,
                caps,
            )))
        }
    }
    registry
        .register_test_factory("mockdb", Box::leak(Box::new(RouteMockFactory { driver })))
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    use datazen::connection::domain::catalog::ConnectionRepository as _;
    use datazen::connection::domain::profile::ProfileRepository as _;
    let record = profiles
        .create(
            &ctx,
            datazen::connection::dto::ProfileDraft::new("wf-routes-conn", "mockdb"),
            &datazen::shared::id::IdempotencyKey::new("idem-wf-routes-profile"),
        )
        .await
        .expect("profile create");
    let connection_id = record.connection_id.as_str().to_owned();
    let scope = datazen::connection::domain::ConnectionScope::new("org-1");
    connections
        .save(
            &scope,
            datazen_driver_api::ConnectionConfig {
                id: connection_id.clone(),
                name: "wf routes conn".into(),
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

    // 执行栈（bootstrap 同构：预算桥 → 执行后端 → 发布钩子 → 租约源 →
    // 资源宿主）。
    let bridge = Arc::new(BudgetPortBridge::new(
        Arc::new(ServerBudgetCoordinator::new(
            8,
            10_000,
            datazen::shared::id::OrganizationId::new("org-1"),
        )),
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
    let sink = datazen::execution::application::orchestration::RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let lease_source = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles.clone(),
        policy,
        backend,
        sink,
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new()),
    ));
    let resource_host = Arc::new(ServerResourceExecutionHost::new(
        connections.clone(),
        profiles.clone(),
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        lease_source,
        "org-1",
    ));

    // Workflow 域装配（bootstrap 同构：共享 registry 架在 scoped store 上）。
    let workflow_store = ServerWorkflowStore::scoped(store.clone(), "org-1");
    let registry_core = Arc::new(datazen::workflow::registry::WorkflowRegistry::new(
        Arc::new(workflow_store),
        Vec::new(),
    ));
    let history = ServerWorkflowHistoryStore::scoped(store.clone(), "org-1");

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
        domains: None,
        mcp: None,
        domain_routers: Some(vec![
            datazen_server::http::domains::workflow::workflow_router(
                datazen_server::http::domains::workflow::WorkflowState {
                    registry: registry_core,
                    history: history.clone(),
                    execution: resource_host,
                    profiles,
                    organization_id: "org-1".to_owned(),
                },
            ),
        ]),
    })
    .expect("router");

    WireFixture {
        app,
        primary,
        other_org,
        connection_id,
        history,
    }
}

// ── t1：定义 CRUD 往返 ──────────────────────────────────────────────────────

#[tokio::test]
async fn definitions_crud_roundtrip_through_the_http_face() {
    let fx = wired_fixture().await;

    // PUT 保存（带令牌）→ 204。
    let key = idem("save");
    let (status, body) = call(
        &fx.app,
        "PUT",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "id": "wf-crud",
            "name": "WF crud",
            "description": "d",
            "steps": [ { "type": "query", "id": "q1", "sql": "SELECT 1" } ],
            "connection": fx.connection_id,
            "visibility": "user"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "save: {body}");

    // 无令牌保存 → 400（在场性闸）。
    let (status, body) = call(
        &fx.app,
        "PUT",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "id": "wf-crud",
            "name": "WF crud",
            "description": "d",
            "steps": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "token gate: {body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("submissionToken"),
        "{body}"
    );

    // 令牌与 idempotencyKey 不绑定 → 400。
    let (status, _) = call(
        &fx.app,
        "PUT",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": "other-key" },
            "idempotencyKey": "this-key",
            "id": "wf-crud", "name": "n", "description": "", "steps": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "token binding");

    // 体 id 与路径 id 不一致 → 400。
    let key = idem("mismatch");
    let (status, _) = call(
        &fx.app,
        "PUT",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "id": "wf-other", "name": "n", "description": "", "steps": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "id mismatch");

    // 列表（user-可见面）。
    let (status, list) = call(
        &fx.app,
        "GET",
        "/api/v1/workflows",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let items = list.as_array().expect("list");
    assert!(items.iter().any(|item| item["id"] == "wf-crud"), "{list}");
    let saved = items
        .iter()
        .find(|item| item["id"] == "wf-crud")
        .expect("saved item");
    assert_eq!(saved["name"], "WF crud");
    assert_eq!(saved["scheduled"], false);

    // 单读（存储列胜过文档）。
    let (status, definition) = call(
        &fx.app,
        "GET",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{definition}");
    assert_eq!(definition["id"], "wf-crud");
    assert_eq!(definition["name"], "WF crud");
    assert_eq!(definition["steps"][0]["sql"], "SELECT 1");

    // YAML 原文（{ yaml } 包装）。
    let (status, yaml_body) = call(
        &fx.app,
        "GET",
        "/api/v1/workflows/wf-crud/yaml",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{yaml_body}");
    let yaml = yaml_body["yaml"].as_str().expect("yaml string");
    assert!(yaml.contains("id: wf-crud"), "YAML 原文应含 id：{yaml}");

    // 未知 id 读 → 404。
    let (status, _) = call(
        &fx.app,
        "GET",
        "/api/v1/workflows/missing",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 删除 → 204；再读 → 404；再删（幂等）→ 204。
    let (status, _) = call(
        &fx.app,
        "DELETE",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &fx.app,
        "GET",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &fx.app,
        "DELETE",
        "/api/v1/workflows/wf-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "幂等删除");
}

// ── t2：save-yaml 双模编辑器轨道 ───────────────────────────────────────────

#[tokio::test]
async fn save_yaml_parses_validates_and_persists() {
    let fx = wired_fixture().await;
    let yaml = "id: wf-yaml\nname: FromYaml\ndescription: d\nsteps:\n  - type: query\n    id: q1\n    sql: SELECT 1\n";

    let key = idem("yaml");
    let (status, definition) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/from-yaml",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "yaml": yaml
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{definition}");
    assert_eq!(definition["id"], "wf-yaml");
    assert_eq!(definition["name"], "FromYaml");

    // 非法 id（词表规则在共享层）→ 400。
    let key = idem("yaml-bad");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/from-yaml",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "yaml": "id: bad id!\nname: Bad\nsteps: []\n"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // 坏 YAML → 400。
    let key = idem("yaml-broken");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/from-yaml",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "yaml": "::: not yaml ["
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

// ── t3：认证与归属闸 ────────────────────────────────────────────────────────

#[tokio::test]
async fn unauthenticated_requests_are_rejected_before_the_domain() {
    let fx = wired_fixture().await;
    for (method, uri) in [
        ("GET", "/api/v1/workflows"),
        ("GET", "/api/v1/workflows/x"),
        ("GET", "/api/v1/workflow-runs"),
        ("POST", "/api/v1/workflows/x/execute"),
    ] {
        let (status, _) = call(&fx.app, method, uri, "", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
async fn another_organization_is_a_uniform_404() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-org",
        query_workflow_json("wf-org", &fx.connection_id),
    )
    .await;

    // 归属闸：org-2 的请求对 org-1 的域面一律 404（aiHost 同款合并口径）。
    for (method, uri) in [
        ("GET", "/api/v1/workflows"),
        ("GET", "/api/v1/workflows/wf-org"),
        ("GET", "/api/v1/workflows/wf-org/yaml"),
        ("GET", "/api/v1/workflow-runs"),
        ("POST", "/api/v1/workflows/reload"),
    ] {
        let (status, _) = call(
            &fx.app,
            method,
            uri,
            &fx.other_org.cookie,
            Some(&fx.other_org.csrf),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

// ── t4：执行面 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn execute_query_workflow_returns_the_full_result_and_records_history() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-exec",
        query_workflow_json("wf-exec", &fx.connection_id),
    )
    .await;

    let key = idem("exec");
    let (status, result) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/wf-exec/execute",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key,
            "variables": {},
            "connectionId": null
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["success"], true, "{result}");
    assert_eq!(result["steps"][0]["stepType"], "query");
    assert_eq!(result["steps"][0]["status"], "success", "{result}");

    // 运行历史落账（执行应答前 best-effort 完成）。
    let (status, runs) = call(
        &fx.app,
        "GET",
        "/api/v1/workflow-runs?workflowId=wf-exec",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{runs}");
    let runs = runs.as_array().expect("runs");
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0]["workflowId"], "wf-exec");
    assert_eq!(runs[0]["success"], true);

    // 单读（整档记录）。
    let run_id = runs[0]["id"].as_str().expect("run id").to_owned();
    let (status, record) = call(
        &fx.app,
        "GET",
        &format!("/api/v1/workflow-runs/{run_id}"),
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{record}");
    assert_eq!(record["workflowName"], "WF wf-exec");
    assert_eq!(record["result"]["success"], true);

    // 未知 run id → 404。
    let (status, _) = call(
        &fx.app,
        "GET",
        "/api/v1/workflow-runs/run-missing",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn execute_data_only_workflow_runs_without_a_database() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-data",
        data_only_workflow_json("wf-data"),
    )
    .await;

    let key = idem("data");
    let (status, result) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/wf-data/execute",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["success"], true, "{result}");
    // merge 拼接 2 + 1 行。
    let merge = &result["steps"][1]["result"];
    assert_eq!(merge["rows_count"], 3, "{result}");
}

#[tokio::test]
async fn execute_unknown_workflow_is_404_and_ai_steps_fail_closed() {
    let fx = wired_fixture().await;

    // 未知 workflow → 404（防枚举固定文案）。
    let key = idem("missing");
    let (status, _) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/missing/execute",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // AI 步骤 → 400 fail-closed（server 形态无 AI 供应商面）。
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-ai",
        serde_json::json!({
            "id": "wf-ai",
            "name": "WF ai",
            "description": "",
            "steps": [ { "type": "ai", "id": "a1", "prompt": "hello" } ],
            "visibility": "user"
        }),
    )
    .await;
    let key = idem("ai");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/wf-ai/execute",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    // 执行器默认错误策略：步骤失败标记后继续，应答是 200 + success:false
    // （桌面 IPC 同语义——步骤失败不是命令错误）；fail-closed 文案钉在
    // 步骤的 error 里。
    assert_eq!(status, StatusCode::OK, "AI fail-closed：{body}");
    assert_eq!(body["success"], false, "{body}");
    assert_eq!(body["steps"][0]["status"], "failed", "{body}");
    let message = body["steps"][0]["error"].as_str().unwrap_or_default();
    assert!(message.contains("AI steps are not served"), "{body}");
}

// ── t5：reload 与历史管理 ──────────────────────────────────────────────────

#[tokio::test]
async fn reload_is_a_noop_204_and_history_clears_by_workflow() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-a",
        query_workflow_json("wf-a", &fx.connection_id),
    )
    .await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-b",
        query_workflow_json("wf-b", &fx.connection_id),
    )
    .await;

    // reload：库后端 no-op → 204。
    let (status, _) = call(
        &fx.app,
        "POST",
        "/api/v1/workflows/reload",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 两次执行落两条历史。
    for tag in ["first", "second"] {
        let key = idem(tag);
        let (status, _) = call(
            &fx.app,
            "POST",
            "/api/v1/workflows/wf-a/execute",
            &fx.primary.cookie,
            Some(&fx.primary.csrf),
            Some(serde_json::json!({
                "submissionToken": { "idempotencyKey": key },
                "idempotencyKey": key
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // 按 workflow 清空 → { cleared: 2 }。
    let (status, body) = call(
        &fx.app,
        "DELETE",
        "/api/v1/workflow-runs?workflowId=wf-a",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cleared"], 2, "{body}");

    // 全量列表为空；再次清空 → { cleared: 0 }（no-op 的成功可被调用方
    // 据行数拒绝）。
    let (status, runs) = call(
        &fx.app,
        "GET",
        "/api/v1/workflow-runs",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(runs.as_array().map(Vec::len), Some(0));
    let (status, body) = call(
        &fx.app,
        "DELETE",
        "/api/v1/workflow-runs",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cleared"], 0);
}

// ── 未用词汇防线（导入由夹具的真装配链消费） ─────────────────────────
