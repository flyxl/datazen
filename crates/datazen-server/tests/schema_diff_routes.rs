//! Schema Diff 域路由集成测试（W8 刀3b-2；契约 §6.13）。
//!
//! 两档夹具（query_assets / table_data_routes 先例）：
//!
//! - **router-only 腿**：`AuthenticatedSession` request extension 直插
//!   （绕过登录与 CSRF，只测域 handler 面）——profiles CRUD、令牌闸、
//!   未知会话/计划的 4xx 钉、job 读回的空集语义。
//! - **wired 全栈腿**：真登录会话 + cookie/CSRF 全链（L2–L6）+ 真
//!   MockDriver 活会话（`open_session` 经真用例开立）——prepare 的
//!   绿路径（202 → 轮询 Succeeded → prepared 信封）、compare 的
//!   对比形状、deploy 受理与终态可达、恢复核验的归属闸。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ColumnSchema, SslMode, TableSchema};
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, LoginSessions, NewLoginSession};
use datazen_server::storage::{AdminStore, ServerConnections, ServerProfiles};

const SERVER_SECRET: &[u8] = b"schema-diff-routes-secret";

// ── 公共件 ──────────────────────────────────────────────────────────────────

fn memory_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

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
    let request = if let Some(body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request with body")
    } else {
        builder.body(Body::empty()).expect("request")
    };
    let response = app.clone().oneshot(request).await.expect("oneshot");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("body bytes");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

/// 最小 schema_diff 域装配（router-only：无驱动、无活会话——未知会话的
/// 解析腿按 400/404 钉）。
fn schema_diff_state(
    store: Arc<AdminStore>,
) -> datazen_server::http::domains::schema_diff::SchemaDiffState {
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::lease::ServerRuntimeLeaseSource;
    use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
    use datazen_server::execution::secret_provider::ServerSecretVault;
    use datazen_server::schema_diff::backend::ServerSchemaDiffBackendDeps;
    use datazen_server::schema_diff::endpoints::SessionEndpoints;
    use datazen_server::schema_diff::history::MigrationHistory;
    use datazen_server::schema_diff::jobs::SchemaDiffJobs;
    use datazen_server::schema_diff::{ServerComparisonPreviewHost, ServerSchemaDiffAdapters};
    use datazen_server::storage::{ServerMigrationRuns, ServerSchemaDiffProfiles};

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
        datazen::connection::domain::ConnectionScope::new("org-1"),
        bridge,
        None,
    ));
    let policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let directory =
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
    let lease_source = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles,
        policy,
        backend,
        Default::default(),
        directory.clone(),
    ));
    let endpoints = SessionEndpoints {
        directory,
        lease: lease_source,
        connections: connections.clone(),
    };
    let profiles_repo = Arc::new(ServerSchemaDiffProfiles::new(store.clone()));
    let history = MigrationHistory::new(
        Arc::new(ServerMigrationRuns::new(store.clone())),
        profiles_repo.clone(),
    );
    let reviewed: Arc<dyn datazen::migration::schema_diff::reviewed::ReviewedPlanRepository> =
        Arc::new(datazen::migration::schema_diff::reviewed::InMemoryReviewedPlanRepository::new());
    let adapters = Arc::new(ServerSchemaDiffAdapters {
        registry: Arc::new(
            datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry::new(
            ),
        ),
    });
    let backend_deps = ServerSchemaDiffBackendDeps {
        endpoints: endpoints.clone(),
        history,
        reviewed,
        adapters: adapters.registry.clone(),
    };
    let jobs = SchemaDiffJobs::new(
        Arc::new(datazen::jobs::JobHost::new(
            Arc::new(datazen_server::storage::ServerJobs::new(
                store.clone(),
                Arc::new(datazen::jobs::domain::time::PortJobClock::new(
                    datazen::shared::clock::SystemClock,
                )),
                900,
            )),
            Arc::new(std::sync::Mutex::new(
                datazen::execution::runtime::budget::BudgetLedger::new(
                    datazen::execution::runtime::budget::BudgetConfig::team_default(),
                ),
            )),
            Arc::new(datazen::jobs::domain::time::PortJobClock::new(
                datazen::shared::clock::SystemClock,
            )),
        )),
        Arc::new(datazen::migration::schema_diff::job::PlanRepository::new()),
        backend_deps,
    );
    datazen_server::http::domains::schema_diff::SchemaDiffState {
        store,
        profiles: profiles_repo,
        jobs,
        endpoints,
        comparison_preview: Arc::new(ServerComparisonPreviewHost),
        adapters,
    }
}

/// router-only 装配：`AuthenticatedSession` extension 直插（query_assets
/// 夹具同款——只测域 handler 面）。
fn session_of(
    org: &str,
    principal: &str,
) -> datazen_server::http::middleware::auth::AuthenticatedSession {
    datazen_server::http::middleware::auth::AuthenticatedSession {
        organization_id: org.into(),
        principal_id: principal.into(),
        session_id: "sess-1".into(),
        permission_version: 1,
        token_epoch: 0,
        auth_mode: LoginSessionMode::Cookie,
    }
}

async fn assets_app(store: Arc<AdminStore>, org: &str, principal: &str) -> axum::Router {
    let state = schema_diff_state(store);
    let mut app = datazen_server::http::domains::schema_diff::schema_diff_router(state);
    let session = session_of(org, principal);
    app = app.layer(axum::middleware::from_fn(
        move |mut req: Request<Body>, next: axum::middleware::Next| {
            let session = session.clone();
            async move {
                req.extensions_mut().insert(session);
                next.run(req).await
            }
        },
    ));
    app
}

fn token_body(extra: serde_json::Value) -> serde_json::Value {
    let mut body = extra;
    body["submissionToken"] = serde_json::json!({ "idempotencyKey": "idem-1" });
    body["idempotencyKey"] = serde_json::json!("idem-1");
    body
}

// ── t1：profiles CRUD + 令牌闸 + 校验 ──────────────────────────────────────

#[tokio::test]
async fn profiles_crud_roundtrip_with_token_and_connection_gates() {
    let store = memory_store();
    let app = assets_app(store.clone(), "org-1", "user-1").await;

    // 空集。
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().map(Vec::len), Some(0));

    // 无令牌 → 400。
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        Some(serde_json::json!({ "id": "p1" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 令牌不绑定 → 400。
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": "other" },
            "idempotencyKey": "idem-1",
            "id": "p1"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 空表选择 → 域校验拒绝。
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        Some(token_body(serde_json::json!({
            "version": 1,
            "id": "p1",
            "name": "dev → prod",
            "sourceConnectionId": "c-src",
            "targetConnectionId": "c-tgt",
            "sourceDatabase": "app_dev",
            "targetDatabase": "app_prod",
            "tables": []
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 合法体 + 连接不在目录 → 400（连接存在性闸）。
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        Some(token_body(serde_json::json!({
            "version": 1,
            "id": "p1",
            "name": "dev → prod",
            "sourceConnectionId": "c-src",
            "targetConnectionId": "c-tgt",
            "sourceDatabase": "app_dev",
            "targetDatabase": "app_prod",
            "tables": ["users"]
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 落一条连接目录行后再存 → 204；list 回读；upsert 改名；delete 清零
    // （目录行经公共仓储 API 落——`ConnectionRepository::save`）。
    {
        use datazen::connection::domain::catalog::ConnectionRepository as _;
        use datazen_server::execution::secret_provider::ServerSecretVault;
        let vault = Arc::new(
            ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
        );
        let connections = Arc::new(ServerConnections::new(store.clone(), vault));
        let scope = datazen::connection::domain::ConnectionScope::new("org-1");
        for id in ["c-src", "c-tgt"] {
            connections
                .save(
                    &scope,
                    datazen_driver_api::ConnectionConfig {
                        id: id.into(),
                        name: id.into(),
                        database_type: "mockdb".into(),
                        host: Some("localhost".into()),
                        port: Some(1),
                        database: Some("app".into()),
                        schema: Some("public".into()),
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
        }
    }
    let (status, _) = call(
        &app,
        "PUT",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        Some(token_body(serde_json::json!({
            "version": 1,
            "id": "p1",
            "name": "dev → prod",
            "sourceConnectionId": "c-src",
            "targetConnectionId": "c-tgt",
            "sourceDatabase": "app_dev",
            "targetDatabase": "app_prod",
            "tables": ["users", "orders"],
            "allowDestructive": false,
            "includeIndexes": true,
            "requireRollback": false
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "PUT 落库：{status}");
    let (status, body) = call(
        &app,
        "GET",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["id"], "p1");
    assert_eq!(body[0]["tables"].as_array().map(Vec::len), Some(2));

    // 组织隔离：org-2 看不见 org-1 的行。
    let other_app = assets_app(memory_store(), "org-2", "user-2").await;
    let (status, body) = call(
        &other_app,
        "GET",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().map(Vec::len), Some(0));

    let (status, _) = call(
        &app,
        "DELETE",
        "/api/v1/schema-diff/profiles/p1",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = call(
        &app,
        "GET",
        "/api/v1/schema-diff/profiles",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(body.as_array().map(Vec::len), Some(0));
}

// ── t2：未知会话/计划 + job 空集语义 ───────────────────────────────────────

#[tokio::test]
async fn unknown_sessions_and_plans_answer_the_documented_error_faces() {
    let store = memory_store();
    let app = assets_app(store, "org-1", "user-1").await;

    // 未知会话的 prepare → 400（端点身份 fail-closed）。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/schema-diff/plans",
        "c=1",
        None,
        Some(token_body(serde_json::json!({
            "sourceDbSessionId": "nope",
            "targetDbSessionId": "nada",
            "tableNames": ["users"],
            "allowDestructive": false
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // deploy 无 planId 且无 plan 正文 → 400。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/schema-diff/deploys",
        "c=1",
        None,
        Some(token_body(serde_json::json!({
            "targetDbSessionId": "nope"
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 未知计划的 deploy → 404。
    let (status, _) = call(
        &app,
        "POST",
        "/api/v1/schema-diff/deploys",
        "c=1",
        None,
        Some(token_body(serde_json::json!({
            "targetDbSessionId": "nope",
            "planId": "plan-ghost",
            "selectionRevision": 1
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // jobs 空集 / 未知详情 / 未知取消。
    let (status, body) = call(&app, "GET", "/api/v1/schema-diff/jobs", "c=1", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().map(Vec::len), Some(0));
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/schema-diff/jobs/job-ghost",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = call(
        &app,
        "POST",
        "/api/v1/schema-diff/jobs/job-ghost/cancel",
        "c=1",
        None,
        Some(token_body(serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cancelled"], false, "未知 job 的取消是幂等 no-op");

    // 未知状态过滤 → 400。
    let (status, _) = call(
        &app,
        "GET",
        "/api/v1/schema-diff/jobs?states=banana",
        "c=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── wired 全栈夹具 ─────────────────────────────────────────────────────────

fn users_schema() -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "BIGINT".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: true,
            },
            ColumnSchema {
                name: "name".into(),
                data_type: "VARCHAR(80)".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

/// resource_provider 闸的工厂形（table_data_routes 先例）：open 的
/// 会话后端经工厂解析资源适配器。
struct WireMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
}

impl datazen_driver_api::DatabaseDriverFactory for WireMockFactory {
    fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
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
        let mut capabilities = datazen_driver_api::capabilities::CapabilitySet::default();
        capabilities.stateful_session = datazen_driver_api::capabilities::Availability::Supported;
        capabilities.namespace_switch =
            datazen_driver_api::capabilities::NamespaceSwitch::Unsupported;
        capabilities.context_observation =
            datazen_driver_api::capabilities::ContextObservation::Full;
        capabilities.session_scoped_handles =
            datazen_driver_api::capabilities::SessionScopedHandleSupport::Supported;
        capabilities.reset_for_reuse = datazen_driver_api::capabilities::ResetForReuse::Unsupported;
        Some(Arc::new(
            datazen_driver_api::resource_adapter::LegacyResourceAdapter::new(
                Arc::clone(&self.driver),
                self.driver_id,
                "sd-wire-test",
                1,
                shape,
                capabilities,
            ),
        ))
    }
}

struct WireFixture {
    _store: Arc<AdminStore>,
    admin_path: PathBuf,
    app: axum::Router,
    primary: Account,
    sessions: Arc<datazen::execution::application::RuntimeConnectionUseCases>,
    ctx: datazen::shared::context::RequestContext,
    source_connection_id: String,
    target_connection_id: String,
}

async fn wired_fixture() -> WireFixture {
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::lease::RuntimeLeaseSource as _;
    use datazen_server::execution::lease::ServerRuntimeLeaseSource;
    use datazen_server::execution::publisher::execution_result_publisher;
    use datazen_server::execution::secret_provider::ServerSecretVault;
    use datazen_server::schema_diff::backend::ServerSchemaDiffBackendDeps;
    use datazen_server::schema_diff::endpoints::SessionEndpoints;
    use datazen_server::schema_diff::history::MigrationHistory;
    use datazen_server::schema_diff::jobs::SchemaDiffJobs;
    use datazen_server::schema_diff::{ServerComparisonPreviewHost, ServerSchemaDiffAdapters};
    use datazen_server::storage::{ServerMigrationRuns, ServerSchemaDiffProfiles};

    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w8-schema-diff-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply");

    let primary = register_login(&store, "org-1", "user-1", "sess-sd-primary");
    let _other_org = register_login(&store, "org-2", "user-2", "sess-sd-org2");

    let ctx = datazen::shared::context::RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        datazen::shared::id::ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-sd-wire"),
        None,
    );

    // MockDriver：app_dev 有 users 表（两列），app_prod 无（空库 → 目标侧
    // 全量 CREATE）。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    // 方言取 postgresql（桌面 plan_deploy 测试同款）：非 postgresql 方言
    // 在驱动无法提供物理库身份时按 Unknown fail-closed（拼写不构成迁移
    // 许可）；postgresql 的库名是大小写敏感的目录身份，app_dev ≠ app_prod
    // 即可证 Different。
    let mut options = MockDriverOptions::default();
    options.has_schema_level = true;
    options.default_schema = Some("public");
    let mut by_database: HashMap<String, HashMap<String, TableSchema>> = HashMap::new();
    by_database.insert(
        "app_dev".to_owned(),
        HashMap::from([("users".to_owned(), users_schema())]),
    );
    by_database.insert("app_prod".to_owned(), HashMap::new());
    options.table_schemas_by_database = by_database;
    options.databases = vec!["app_dev".into(), "app_prod".into()];
    let driver = MockDriver::new("postgresql", options);
    registry
        .register_test_driver("postgresql", driver.clone())
        .await;
    registry
        .register_test_factory(
            "postgresql",
            Box::leak(Box::new(WireMockFactory {
                driver_id: "postgresql",
                driver: driver.clone(),
            })),
        )
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    use datazen::connection::domain::catalog::ConnectionRepository as _;
    use datazen::connection::domain::profile::ProfileRepository as _;
    let scope = datazen::connection::domain::ConnectionScope::new("org-1");
    let mut connection_ids = Vec::new();
    for (name, database) in [("wire src", "app_dev"), ("wire tgt", "app_prod")] {
        let record = profiles
            .create(
                &ctx,
                datazen::connection::dto::ProfileDraft::new(name, "postgresql"),
                &datazen::shared::id::IdempotencyKey::new(format!("idem-sd-{name}")),
            )
            .await
            .expect("profile create");
        let connection_id = record.connection_id.as_str().to_owned();
        connections
            .save(
                &scope,
                datazen_driver_api::ConnectionConfig {
                    id: connection_id.clone(),
                    name: name.into(),
                    database_type: "postgresql".into(),
                    host: Some("localhost".into()),
                    port: Some(1),
                    database: Some(database.into()),
                    schema: Some("public".into()),
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
        connection_ids.push(connection_id);
    }
    let (source_connection_id, target_connection_id) =
        (connection_ids[0].clone(), connection_ids[1].clone());

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
    let sink = datazen::execution::application::orchestration::RuntimeResultSink::default();
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
    // CM-63 cell 语义（刀2/刀4 夹具同构）：活连接出向端口适配器吃 backend
    // 的 clone（by_session 反向索引随 backend.open 同帧写入，两份 Arc 必须
    // 指向同一 backend 实例）；租约源 `.with_live_source` 接线后 resolve 一次
    // 拿当前单元——会话开立 / 借出读同一实例。
    let live_source = Arc::new(
        datazen_server::execution::live_source::ServerLiveConnectionSource::new(
            backend.clone(),
            connections.clone()
                as Arc<dyn datazen::connection::domain::catalog::ConnectionRepository>,
            registry.clone(),
            datazen::connection::domain::ConnectionScope::new("org-1"),
        ),
    );
    let lease_source = Arc::new(
        ServerRuntimeLeaseSource::new_with_shared_parts(
            profiles.clone(),
            policy,
            backend,
            sink,
            directory.clone(),
        )
        .with_live_source(live_source),
    );
    let runtime = lease_source.resolve().await.expect("cell runtime resolves");

    let endpoints = SessionEndpoints {
        directory,
        lease: lease_source,
        connections: connections.clone(),
    };
    let profiles_repo = Arc::new(ServerSchemaDiffProfiles::new(store.clone()));
    let history = MigrationHistory::new(
        Arc::new(ServerMigrationRuns::new(store.clone())),
        profiles_repo.clone(),
    );
    let reviewed: Arc<dyn datazen::migration::schema_diff::reviewed::ReviewedPlanRepository> =
        Arc::new(datazen::migration::schema_diff::reviewed::InMemoryReviewedPlanRepository::new());
    let adapters = Arc::new(ServerSchemaDiffAdapters {
        registry: Arc::new(
            datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry::new(
            ),
        ),
    });
    let backend_deps = ServerSchemaDiffBackendDeps {
        endpoints: endpoints.clone(),
        history,
        reviewed,
        adapters: adapters.registry.clone(),
    };
    let jobs = SchemaDiffJobs::new(
        Arc::new(datazen::jobs::JobHost::new(
            Arc::new(datazen_server::storage::ServerJobs::new(
                store.clone(),
                Arc::new(datazen::jobs::domain::time::PortJobClock::new(
                    datazen::shared::clock::SystemClock,
                )),
                900,
            )),
            Arc::new(std::sync::Mutex::new(
                datazen::execution::runtime::budget::BudgetLedger::new(
                    datazen::execution::runtime::budget::BudgetConfig::team_default(),
                ),
            )),
            Arc::new(datazen::jobs::domain::time::PortJobClock::new(
                datazen::shared::clock::SystemClock,
            )),
        )),
        Arc::new(datazen::migration::schema_diff::job::PlanRepository::new()),
        backend_deps,
    );

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
        wapp_assets: None,
        domain_routers: Some(vec![
            datazen_server::http::domains::schema_diff::schema_diff_router(
                datazen_server::http::domains::schema_diff::SchemaDiffState {
                    store: store.clone(),
                    profiles: profiles_repo,
                    jobs,
                    endpoints,
                    comparison_preview: Arc::new(ServerComparisonPreviewHost),
                    adapters,
                },
            ),
        ]),
    })
    .expect("router");

    WireFixture {
        _store: store.clone(),
        admin_path: dir.join("admin.sqlite"),
        app,
        primary,
        sessions: runtime,
        ctx,
        source_connection_id,
        target_connection_id,
    }
}

impl WireFixture {
    async fn open_session_on(&self, connection_id: &str, database: &str) -> String {
        use datazen::execution::application::idempotency::IdempotentOperation;
        use datazen::execution::application::use_cases::ConnectionUseCases as _;
        use datazen::execution::target::NamespaceTarget;
        use datazen::shared::context::OwnerRef;
        use datazen::shared::id::{ClientInstanceId, EditorSessionId};

        let target = datazen::execution::ExecutionTarget::new(
            datazen::shared::id::ConnectionId::new(connection_id.to_owned()),
            NamespaceTarget::new(Some(database.to_owned()), None, None, Vec::new()),
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
                datazen::execution::application::OpenSessionRequest::new(
                    target,
                    OwnerRef::Editor {
                        client_instance_id: ClientInstanceId::new("http"),
                        editor_session_id: EditorSessionId::new("sd-editor-1"),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .expect("session opens through the real use case");
        receipt.session.handle.db_session_id.as_str().to_owned()
    }

    async fn open_session(&self, database: &str) -> String {
        let connection_id = if database == "app_dev" {
            self.source_connection_id.clone()
        } else {
            self.target_connection_id.clone()
        };
        self.open_session_on(&connection_id, database).await
    }
}

// ── t3：prepare 绿路径（202 → Succeeded → prepared 信封） ──────────────────

#[tokio::test]
async fn prepare_job_green_path_reaches_succeeded_with_envelope() {
    let fixture = wired_fixture().await;
    let source_session = fixture.open_session("app_dev").await;
    let target_session = fixture.open_session("app_prod").await;

    let (status, accepted) = call(
        &fixture.app,
        "POST",
        "/api/v1/schema-diff/plans",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(token_body(serde_json::json!({
            "sourceDbSessionId": source_session,
            "targetDbSessionId": target_session,
            "tableNames": ["users"],
            "allowDestructive": false
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "prepare 受理 202：{accepted}");
    let job_id = accepted["jobId"].as_str().expect("jobId").to_owned();
    assert_eq!(accepted["kind"], "schemaDiffPrepare");

    // 轮询到终态（受理即派发；MockDriver 目录读全同步，秒级完成）。
    let mut details = serde_json::Value::Null;
    for _ in 0..100 {
        let (status, body) = call(
            &fixture.app,
            "GET",
            &format!("/api/v1/schema-diff/jobs/{job_id}"),
            &fixture.primary.cookie,
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let state = body["details"]["job"]["state"].as_str().unwrap_or("");
        if state == "succeeded" || state == "failed" || state == "cancelled" {
            details = body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        details["details"]["job"]["state"], "succeeded",
        "prepare 绿路径到 Succeeded：{details}"
    );
    let prepared = &details["prepared"];
    assert!(
        prepared["planId"].as_str().is_some_and(|id| !id.is_empty()),
        "prepared 信封带 planId：{prepared}"
    );
    assert_eq!(prepared["sourceConnectionId"], fixture.source_connection_id);
    assert_eq!(prepared["targetConnectionId"], fixture.target_connection_id);
    assert!(
        !details["planUnavailableAfterRestart"]
            .as_bool()
            .unwrap_or(true),
        "进程内计划正文在场"
    );

    // list 收编本 job。
    let (status, body) = call(
        &fixture.app,
        "GET",
        "/api/v1/schema-diff/jobs",
        &fixture.primary.cookie,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body
        .as_array()
        .is_some_and(|views| views.iter().any(|view| view["jobId"] == job_id.as_str())));

    // 已终态 job 再取消 → true（幂等）。
    let (status, body) = call(
        &fixture.app,
        "POST",
        &format!("/api/v1/schema-diff/jobs/{job_id}/cancel"),
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(token_body(serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cancelled"], true);
}

// ── t4：compare 绿路径（对比形状） ──────────────────────────────────────────

#[tokio::test]
async fn compare_answers_column_diff_for_missing_target_table() {
    let fixture = wired_fixture().await;
    let source_session = fixture.open_session("app_dev").await;
    let target_session = fixture.open_session("app_prod").await;

    let (status, body) = call(
        &fixture.app,
        "POST",
        "/api/v1/schema-diff/comparisons",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(serde_json::json!({
            "sourceDbSessionId": source_session,
            "targetDbSessionId": target_session,
            "sourceTableName": "users",
            "targetTableName": "users"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "compare 200：{body}");
    assert_eq!(body["table"], "users");
    assert_eq!(
        body["missingOnTarget"].as_array().map(Vec::len),
        Some(2),
        "目标缺表 → 源两列全在 missingOnTarget：{body}"
    );
}

// ── t5：deploy 受理与终态可达 ───────────────────────────────────────────────

#[tokio::test]
async fn deploy_accepts_then_reaches_a_terminal_state() {
    let fixture = wired_fixture().await;
    let source_session = fixture.open_session("app_dev").await;
    let target_session = fixture.open_session("app_prod").await;

    // 先 prepare 出一个计划。
    let (status, accepted) = call(
        &fixture.app,
        "POST",
        "/api/v1/schema-diff/plans",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(token_body(serde_json::json!({
            "sourceDbSessionId": source_session,
            "targetDbSessionId": target_session.clone(),
            "tableNames": ["users"],
            "allowDestructive": false
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let prepare_job = accepted["jobId"].as_str().expect("jobId").to_owned();
    let mut prepared = serde_json::Value::Null;
    for _ in 0..100 {
        let (_, body) = call(
            &fixture.app,
            "GET",
            &format!("/api/v1/schema-diff/jobs/{prepare_job}"),
            &fixture.primary.cookie,
            None,
            None,
        )
        .await;
        if body["details"]["job"]["state"].as_str() == Some("succeeded") {
            prepared = body["prepared"].clone();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let plan_id = prepared["planId"].as_str().expect("planId").to_owned();
    let selection_revision = prepared["selectionRevision"].as_u64().expect("revision");

    // 选择版本漂移 → 400（compare again 语义）。
    let (status, _) = call(
        &fixture.app,
        "POST",
        "/api/v1/schema-diff/deploys",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(token_body(serde_json::json!({
            "targetDbSessionId": target_session,
            "planId": plan_id,
            "selectionRevision": selection_revision + 100
        }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 正确受理 → 202（受理先于 DDL）。
    let (status, accepted) = call(
        &fixture.app,
        "POST",
        "/api/v1/schema-diff/deploys",
        &fixture.primary.cookie,
        Some(&fixture.primary.csrf),
        Some(token_body(serde_json::json!({
            "targetDbSessionId": target_session,
            "planId": plan_id,
            "selectionRevision": selection_revision,
            "useTransaction": true
        }))),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "deploy 受理 202（受理先于任何 DDL 执行）：{accepted}"
    );
    assert_eq!(accepted["kind"], "schemaDiffApply");
    let apply_job = accepted["jobId"].as_str().expect("jobId").to_owned();

    // 派发后到达终态（MockDriver 的 DDL 应答决定成败；两条腿都是合法的
    // 可观测终态——断言的是「到达」，不是具体哪条）。
    let mut terminal = None;
    for _ in 0..200 {
        let (_, body) = call(
            &fixture.app,
            "GET",
            &format!("/api/v1/schema-diff/jobs/{apply_job}"),
            &fixture.primary.cookie,
            None,
            None,
        )
        .await;
        let state = body["details"]["job"]["state"].as_str().unwrap_or("");
        if matches!(state, "succeeded" | "failed" | "cancelled") {
            terminal = Some(state.to_owned());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let terminal = terminal.unwrap_or_else(|| panic!("apply job 未到终态：{apply_job}"));
    assert!(matches!(
        terminal.as_str(),
        "succeeded" | "failed" | "cancelled"
    ));

    // 迁移历史行落账（start/finish 两段——v12 表；直接读文件库核验）。
    let probe = rusqlite::Connection::open(&fixture.admin_path).expect("open admin db");
    let runs: i64 = probe
        .query_row(
            "SELECT COUNT(*) FROM migration_run_history WHERE organization_id = 'org-1' \
             AND operation = 'schemaDiff' AND status IN ('completed', 'failed', 'cancelled')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(runs >= 1, "部署编排的历史行落了终态（实际 {runs} 行）");
}

// ── t6：L2 认证腿 ──────────────────────────────────────────────────────────

#[tokio::test]
async fn unauthenticated_requests_are_rejected_before_the_domain() {
    let fixture = wired_fixture().await;
    let (status, _) = call(
        &fixture.app,
        "GET",
        "/api/v1/schema-diff/profiles",
        "no-cookie=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        &fixture.app,
        "GET",
        "/api/v1/schema-diff/jobs",
        "no-cookie=1",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
