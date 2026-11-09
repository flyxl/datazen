//! Dashboard 域路由测试（W8 刀3d-2；契约 §6.15）。
//!
//! 夹具 = `workflow_routes` fullstack 同构：真 SQLite 管理库 + membership/
//! login 会话 + MockDriver（query 应答一行 Integer(1)，以 `postgresql`
//! 注册——monitor 授权链的驱动族闸只认 PG/MySQL 词）+ 真租约源/执行
//! 后端/`ServerResourceExecutionHost` + 共享 registry + v14 仪表板存储 +
//! monitor 引擎（真 spawn，arm-not-fire——manual 组件不进调度表，测试
//! 不受 1 秒 poll 干扰）。令牌闸按在场性核对（域路由口径）。
//!
//! monitor 授权的**绿路径**（PG/MySQL 真驱动）在 `dashboard_scheduler_
//! wiring.rs` 以 `authorize` 流程函数直测；本文件钉 HTTP 面的拒绝腿与
//! 读/撤面。

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
    AdminStore, ServerConnections, ServerDashboardRuns, ServerDashboardStore, ServerMonitorGrants,
    ServerProfiles, ServerSettingsStore, ServerWorkflowHistoryStore, ServerWorkflowStore,
};

const SERVER_SECRET: &[u8] = b"w8-dashboard-routes-secret";

// ── 账号与请求助手（workflow_routes 同款） ──────────────────────────────────

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

fn idem(tag: &str) -> String {
    format!("{tag}-{}", uuid::Uuid::new_v4())
}

// ── 夹具 ────────────────────────────────────────────────────────────────────

struct WireFixture {
    app: axum::Router,
    primary: Account,
    peer: Account,
    other_org: Account,
    connection_id: String,
    grants: ServerMonitorGrants,
}

/// 一条最小 workflow 定义（query 一步；`database` 显式——固定目标收集
/// 的前提）。
fn query_workflow_json(id: &str, connection_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": format!("WF {id}"),
        "description": "dashboard route test",
        "steps": [
            { "type": "query", "id": "q1", "sql": "SELECT 1", "database": "app" }
        ],
        "connection": connection_id,
        "visibility": "user"
    })
}

/// 一个挂 manual 刷新组件的仪表板（HTTP 体形状）。
fn dashboard_json(id: &str, workflow_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": format!("Dash {id}"),
        "createdAt": "2026-01-01T00:00:00.000Z",
        "updatedAt": "2026-01-01T00:00:00.000Z",
        "layout": { "cols": 12, "rowHeight": 80 },
        "widgets": [
            {
                "id": "w1",
                "title": "Widget One",
                "workflowId": workflow_id,
                "viewMode": "table",
                "chartConfig": null,
                "layout": { "x": 0, "y": 0, "w": 6, "h": 4 },
                "refresh": { "mode": "manual", "refreshSec": null },
                "alert": null,
                "enabled": true
            }
        ],
        "enabled": true,
        "refreshPaused": false
    })
}

async fn put_workflow(
    app: &axum::Router,
    account: &Account,
    id: &str,
    definition: serde_json::Value,
) -> StatusCode {
    let key = idem("seed");
    let mut body = definition.clone();
    if let (Some(target), Some(source)) = (body.as_object_mut(), definition.as_object()) {
        target.insert(
            "submissionToken".into(),
            serde_json::json!({ "idempotencyKey": key }),
        );
        target.insert("idempotencyKey".into(), serde_json::json!(key));
        for (k, v) in source {
            target.insert(k.clone(), v.clone());
        }
    }
    let (status, body) = call(
        app,
        "PUT",
        &format!("/api/v1/workflows/{id}"),
        &account.cookie,
        Some(&account.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "seed workflow: {body}");
    status
}

async fn put_dashboard(
    app: &axum::Router,
    account: &Account,
    dashboard: serde_json::Value,
) -> serde_json::Value {
    let id = dashboard["id"].as_str().expect("id").to_owned();
    let key = idem("dash");
    let mut body = dashboard.clone();
    if let Some(target) = body.as_object_mut() {
        target.insert(
            "submissionToken".into(),
            serde_json::json!({ "idempotencyKey": key }),
        );
        target.insert("idempotencyKey".into(), serde_json::json!(key));
    }
    let (status, body) = call(
        app,
        "PUT",
        &format!("/api/v1/dashboards/{id}"),
        &account.cookie,
        Some(&account.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "seed dashboard: {body}");
    body
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

    let primary = register_login(&store, "org-1", "user-1", "sess-dash-primary");
    let peer = register_login(&store, "org-1", "user-1b", "sess-dash-peer");
    let other_org = register_login(&store, "org-2", "user-2", "sess-dash-other");

    let ctx = datazen::shared::context::RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        datazen::shared::id::ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-dash-routes"),
        None,
    );

    // MockDriver 以 `postgresql` 注册（monitor 授权链的驱动族闸只认
    // PG/MySQL 词；注册键是字符串，驱动本体是 mock——CI 无真 PG 的
    // 替身面）。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let mut opts = datazen_driver_api::mock_driver::MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(datazen_driver_api::Value::Integer(1))]];
    let driver = datazen_driver_api::mock_driver::MockDriver::new("mockdb", opts);
    registry
        .register_test_driver("postgresql", driver.clone())
        .await;
    struct RouteMockFactory {
        driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    }
    impl datazen_driver_api::DatabaseDriverFactory for RouteMockFactory {
        fn driver_id(&self) -> &'static str {
            "postgresql"
        }
        fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
            Arc::clone(&self.driver)
        }
        fn resource_provider(
            &self,
        ) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
            use datazen_driver_api::capabilities;
            use datazen_driver_api::namespace::NamespaceShape;
            use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
            let shape = NamespaceShape {
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
                ..NamespaceShape::default()
            };
            let mut caps = capabilities::CapabilitySet::default();
            caps.stateful_session = capabilities::Availability::Supported;
            caps.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
            caps.context_observation = capabilities::ContextObservation::Full;
            caps.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
            caps.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
            Some(Arc::new(LegacyResourceAdapter::new(
                Arc::clone(&self.driver),
                "postgresql",
                "dashboard-routes",
                1,
                shape,
                caps,
            )))
        }
    }
    registry
        .register_test_factory(
            "postgresql",
            Box::leak(Box::new(RouteMockFactory { driver })),
        )
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
            datazen::connection::dto::ProfileDraft::new("dash-conn", "postgresql"),
            &datazen::shared::id::IdempotencyKey::new("idem-dash-routes-profile"),
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
                name: "dash routes conn".into(),
                database_type: "postgresql".into(),
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

    // Workflow / dashboard 域装配（bootstrap 同构）。
    let workflow_store = ServerWorkflowStore::scoped(store.clone(), "org-1");
    let registry_core = Arc::new(datazen::workflow::registry::WorkflowRegistry::new(
        Arc::new(workflow_store.clone()),
        Vec::new(),
    ));
    let history = ServerWorkflowHistoryStore::scoped(store.clone(), "org-1");
    let dashboard_store = ServerDashboardStore::scoped(store.clone(), "org-1");
    let dashboard_runs = ServerDashboardRuns::scoped(store.clone(), "org-1");
    let monitor_grants = ServerMonitorGrants::scoped(store.clone(), "org-1");
    let settings = Arc::new(ServerSettingsStore::scoped("org-1", store.clone()));

    let monitor_scheduler = Arc::new(
        datazen_server::dashboard::scheduler::ServerMonitorScheduler::spawn(Arc::new(
            datazen_server::dashboard::scheduler::ServerMonitorHost::new(
                dashboard_store.clone(),
                dashboard_runs.clone(),
                monitor_grants.clone(),
                workflow_store.clone(),
                registry_core.clone(),
                settings.clone(),
                profiles.clone(),
                resource_host.clone(),
                "org-1",
            ),
        )),
    );

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
        wapp_assets: None,
        domain_routers: Some(vec![
            datazen_server::http::domains::workflow::workflow_router(
                datazen_server::http::domains::workflow::WorkflowState {
                    registry: registry_core.clone(),
                    history,
                    execution: resource_host.clone(),
                    profiles: profiles.clone(),
                    dashboard_refs: dashboard_store.clone(),
                    organization_id: "org-1".to_owned(),
                },
            ),
            datazen_server::http::domains::dashboard::dashboard_router(
                datazen_server::http::domains::dashboard::DashboardState {
                    dashboards: dashboard_store.clone(),
                    runs: dashboard_runs.clone(),
                    grants: monitor_grants.clone(),
                    workflows: workflow_store.clone(),
                    registry: registry_core.clone(),
                    profiles: profiles.clone(),
                    connections: connections.clone(),
                    resource_host: resource_host.clone(),
                    settings: settings.clone(),
                    monitor: monitor_scheduler.clone(),
                    manual_gate: Arc::new(tokio::sync::Mutex::new(())),
                    organization_id: "org-1".to_owned(),
                },
            ),
        ]),
    })
    .expect("router");

    WireFixture {
        app,
        primary,
        peer,
        other_org,
        connection_id,
        grants: monitor_grants,
    }
}

// ── t1：仪表板 CRUD 往返 ────────────────────────────────────────────────────

#[tokio::test]
async fn dashboard_crud_roundtrip_through_the_http_face() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-crud",
        query_workflow_json("wf-crud", &fx.connection_id),
    )
    .await;

    // 空列表。
    let (status, body) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().expect("array").len(), 0);

    // PUT 保存（带令牌）→ 200 存储后回读。
    let saved = put_dashboard(&fx.app, &fx.primary, dashboard_json("dash-crud", "wf-crud")).await;
    assert_eq!(saved["id"], "dash-crud");
    assert_eq!(saved["widgets"].as_array().expect("widgets").len(), 1);
    // updatedAt 服务端权威（保存时铸造，不再是指定的旧值）。
    assert_ne!(saved["updatedAt"], "2026-01-01T00:00:00.000Z");

    // 令牌缺席 → 400。
    let (status, body) = call(
        &fx.app,
        "PUT",
        "/api/v1/dashboards/dash-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(dashboard_json("dash-crud", "wf-crud")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("submissionToken"));

    // id/路径不一致 → 400。
    let key = idem("mismatch");
    let mut mismatched = dashboard_json("dash-other", "wf-crud");
    if let Some(target) = mismatched.as_object_mut() {
        target.insert(
            "submissionToken".into(),
            serde_json::json!({ "idempotencyKey": key }),
        );
        target.insert("idempotencyKey".into(), serde_json::json!(key));
    }
    let (status, body) = call(
        &fx.app,
        "PUT",
        "/api/v1/dashboards/dash-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(mismatched),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["message"].as_str().unwrap().contains("does not match"));

    // GET 单读 + 引用查询。
    let (status, body) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-crud",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Dash dash-crud");

    let (status, refs) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/workflow-refs?workflowId=wf-crud",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refs}");
    assert_eq!(refs.as_array().expect("refs").len(), 1);
    assert_eq!(refs[0]["dashboardId"], "dash-crud");
    assert_eq!(refs[0]["widgetId"], "w1");

    // PATCH refresh-paused（带令牌）→ 204 → 回读可见。
    let key = idem("pause");
    let (status, body) = call(
        &fx.app,
        "PATCH",
        "/api/v1/dashboards/dash-crud/refresh-paused",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({ "paused": true, "submissionToken": { "idempotencyKey": key }, "idempotencyKey": key })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (_, body) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-crud",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(body["refreshPaused"], true);

    // 跨组织读 → 404 合并口径；未认证 → 401。
    let (status, _) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-crud",
        &fx.other_org.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-crud",
        "cookie-less",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // DELETE → 204 → 再读 404；再删（未知 id）→ 404（桌面 NotFound 语义）。
    let (status, body) = call(
        &fx.app,
        "DELETE",
        "/api/v1/dashboards/dash-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, _) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-crud",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &fx.app,
        "DELETE",
        "/api/v1/dashboards/dash-crud",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── t2：组件创建（SQL → 隐藏工作流） ───────────────────────────────────────

#[tokio::test]
async fn widget_creation_from_sql_and_hidden_sql_update() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-create",
        query_workflow_json("wf-create", &fx.connection_id),
    )
    .await;
    put_dashboard(
        &fx.app,
        &fx.primary,
        dashboard_json("dash-create", "wf-create"),
    )
    .await;

    // from-sql 绿路径：200 + CreateWidgetResult（新 widget + 隐藏工作流）。
    let key = idem("from-sql");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-create/widgets/from-sql",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "connectionId": fx.connection_id,
            "sql": "SELECT 42 AS answer",
            "title": "Answer",
            "viewMode": "table",
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let widget_id = body["widget"]["id"].as_str().expect("widget id").to_owned();
    let hidden_workflow = body["widget"]["workflowId"]
        .as_str()
        .expect("hidden wf")
        .to_owned();
    assert!(
        hidden_workflow.starts_with("dash-wf-"),
        "hidden workflow minted: {hidden_workflow}"
    );
    assert_eq!(
        body["dashboard"]["widgets"]
            .as_array()
            .expect("widgets")
            .len(),
        2
    );

    // 隐藏工作流不进用户列表面（dashboardHidden 私有面）。
    let (_, list) = call(
        &fx.app,
        "GET",
        "/api/v1/workflows",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    let visible: Vec<&str> = list
        .as_array()
        .expect("list")
        .iter()
        .map(|item| item["id"].as_str().expect("id"))
        .collect();
    assert!(
        !visible.contains(&hidden_workflow.as_str()),
        "hidden stays private"
    );

    // updateHiddenWidgetSql → 204。
    let key = idem("hidden-sql");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/hidden-widget-sql",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "workflowId": hidden_workflow,
            "connectionId": fx.connection_id,
            "sql": "SELECT 43 AS answer",
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    // from-workflow：未知 workflow → 400（共享校验文案）。
    let key = idem("from-wf");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-create/widgets/from-workflow",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "workflowId": "wf-missing",
            "viewMode": "chart",
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // from-sql：未知仪表板 → 404。
    let key = idem("from-sql-404");
    let (status, _) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-missing/widgets/from-sql",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "connectionId": fx.connection_id,
            "sql": "SELECT 1",
            "viewMode": "table",
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 未知 widget id 仅供 t3 复用断言（这里钉创建幂等性：同 key 二次提交
    // 由令牌闸语义处理——域面只核在场性，重复执行不在本钉范围）。
    assert!(!widget_id.is_empty());
}

// ── t3：手工执行与运行历史 ──────────────────────────────────────────────────

#[tokio::test]
async fn run_widget_green_path_and_run_history() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-run",
        query_workflow_json("wf-run", &fx.connection_id),
    )
    .await;
    put_dashboard(&fx.app, &fx.primary, dashboard_json("dash-run", "wf-run")).await;

    // POST run（带令牌体）→ 200 WidgetRun（同步全量结果）。
    let key = idem("run");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-run/widgets/w1/run",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["widgetId"], "w1");
    let run_id = body["id"].as_str().expect("run id").to_owned();

    // 运行历史：列表 + 单读。
    let (status, index) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-run/widgets/w1/runs?limit=10",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{index}");
    assert_eq!(index.as_array().expect("index").len(), 1);
    assert_eq!(index[0]["id"], run_id.as_str());

    let (status, run) = call(
        &fx.app,
        "GET",
        &format!("/api/v1/dashboards/dash-run/widgets/w1/runs/{run_id}"),
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(run["rowCount"], 1);

    // 未知运行 → 404；别的 widget 读同一条 → 404（归属核对）。
    let (status, _) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-run/widgets/w1/runs/run-missing",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &fx.app,
        "GET",
        &format!("/api/v1/dashboards/dash-run/widgets/w2/runs/{run_id}"),
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 未知 widget 执行 → 404；未知仪表板 → 404；跨组织 → 404。
    let key = idem("run-404");
    let (status, _) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-run/widgets/w-missing/run",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let key = idem("run-404b");
    let (status, _) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-missing/widgets/w1/run",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let key = idem("run-xorg");
    let (status, _) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-run/widgets/w1/run",
        &fx.other_org.cookie,
        Some(&fx.other_org.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── t4：monitor 授权面（读/撤 + 拒绝腿） ───────────────────────────────────

#[tokio::test]
async fn monitor_grant_read_revoke_and_rejection_faces() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-grant",
        query_workflow_json("wf-grant", &fx.connection_id),
    )
    .await;
    put_dashboard(
        &fx.app,
        &fx.primary,
        dashboard_json("dash-grant", "wf-grant"),
    )
    .await;

    // 无授权单 → 200 null。
    let (status, body) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.is_null());

    // authorize 拒绝腿：manual 组件（非 interval）→ 400 状态闸文案。
    let key = idem("authz-manual");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "ttlSecs": 3600,
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("interval schedule"),
        "state gate wording: {body}"
    );

    // ttl 越界 → 400。
    let key = idem("authz-ttl");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "ttlSecs": 10,
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["message"].as_str().unwrap().contains("60..86400"));

    // 未知 widget → 400（授权流程的「widget missing」校验文案）。
    let key = idem("authz-404");
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/dashboards/dash-grant/widgets/w-missing/schedule-grant",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(serde_json::json!({
            "ttlSecs": 3600,
            "submissionToken": { "idempotencyKey": key },
            "idempotencyKey": key
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["message"].as_str().unwrap().contains("widget missing"));

    // 直落一条授权单（存储面；approved_by = primary）→ 读面/撤面。
    let grant = datazen::monitor::domain::MonitorGrant {
        id: "g-routes-1".into(),
        dashboard_id: "dash-grant".into(),
        widget_id: "w1".into(),
        workflow_id: "wf-grant".into(),
        workflow_revision: "rev-1".into(),
        organization_id: "org-1".into(),
        service_principal: "monitor-service:g-routes-1".into(),
        approved_by: "user-1".into(),
        scopes: vec![datazen::monitor::domain::MonitorScope {
            target: datazen::execution::target::ExecutionTarget::new(
                datazen::shared::id::ConnectionId::new(fx.connection_id.clone()),
                datazen::execution::target::NamespaceTarget::database("app"),
            ),
            config_revision: 1,
            command: "query".into(),
        }],
        actions: vec!["connect".into(), "query".into()],
        read_only: true,
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        revocation_version: 1,
        revoked: false,
    };
    fx.grants.save(&grant).expect("grant seeded");

    // 批准人可读。
    let (status, body) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], "g-routes-1");

    // 同组织第二主体读 → 400 grant owner required。
    let (status, body) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.peer.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("grant owner required"));

    // 非批准人撤 → 400；批准人撤 → 204；再读 → revoked 翻转。
    let (status, body) = call(
        &fx.app,
        "DELETE",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.peer.cookie,
        Some(&fx.peer.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = call(
        &fx.app,
        "DELETE",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let (status, body) = call(
        &fx.app,
        "GET",
        "/api/v1/dashboards/dash-grant/widgets/w1/schedule-grant",
        &fx.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revoked"], true);

    // 无单撤销 = 幂等 no-op。
    let (status, _) = call(
        &fx.app,
        "DELETE",
        "/api/v1/dashboards/dash-grant/widgets/w-none/schedule-grant",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

// ── t5：workflow 删除引用闸（§6.14 欠账销账） ───────────────────────────────

#[tokio::test]
async fn workflow_delete_blocked_by_dashboard_reference() {
    let fx = wired_fixture().await;
    put_workflow(
        &fx.app,
        &fx.primary,
        "wf-ref",
        query_workflow_json("wf-ref", &fx.connection_id),
    )
    .await;
    put_dashboard(&fx.app, &fx.primary, dashboard_json("dash-ref", "wf-ref")).await;

    // 被引用 → 400 桌面同文案。
    let (status, body) = call(
        &fx.app,
        "DELETE",
        "/api/v1/workflows/wf-ref",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("Workflow in use by dashboards"),
        "gate wording: {body}"
    );
    assert!(
        body["message"].as_str().unwrap().contains("Widget One"),
        "reference summary names the widget: {body}"
    );

    // 删除仪表板后 → 204。
    let (status, _) = call(
        &fx.app,
        "DELETE",
        "/api/v1/dashboards/dash-ref",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &fx.app,
        "DELETE",
        "/api/v1/workflows/wf-ref",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
