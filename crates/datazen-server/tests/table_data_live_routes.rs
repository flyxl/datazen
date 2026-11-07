//! W8 刀2 测试欠账补齐：tableData 活句柄面四读端点（`resolveForeignKeyTargets`
//! / `listInboundForeignKeys` / `searchTableValues` / `resolveResultOrigin`，
//! 路由与状态表见 `http/domains/table_data.rs`，实现在
//! `http/domains/table_data_live.rs`）的集成测试 **t1–t10**。
//!
//! 本文件清偿 #193（W8 刀2，已合并）勘误所记欠账：刀2 台账声称的集成测试
//! `tests/table_data_live_routes.rs` 从未创建，刀4（#195）台账勘误如实登记
//! 「刀2 四读端点（FK 二/值搜索/origin）至今零专项测试覆盖；欠账登记，随后
//! 续测试刀补齐」——本文件即那笔补齐，用例规格逐字对齐台账 t1–t10：
//!
//! - **t1 认证链 401 ×4**：四个读端点未认证各得 401。
//! - **t2 FK 跳转绿路径**：源表 FK 元数据 → 目标校验 → 探针命中，
//!   断言 `exists:true` + candidates。
//! - **t3 引用行已不在 = `exists:false` 成功答案（不是错误）**。
//! - **t4 未知约束 → `grid.fk.unknownConstraint:` 前缀 400**。
//! - **t5 反向目录扫描**（listInboundForeignKeys）：orders→users 约束条目 +
//!   `truncated` 如实（≤200 次 schema 读的早停）。
//! - **t6 值搜索计数**：COUNT(*) 单格语句经 MockDriver count 分支。
//! - **t7 空值 → 400**。
//! - **t8 origin 绿路径**：MockDriver 契约 Undeterminable → 200 只读裁决
//!   `unknownOrigin`，且定位键半边走 SchemaCache 目录读。
//! - **t9 跨组织借出·FK 面**：bind_session 的 authorize 层 → 404 文案
//!   「DB session … is unavailable」。
//! - **t10 跨组织借出·值搜索面**：`borrow_live_for_session` → 404 固定文案
//!   （未知 id 与越组织两种防枚举口径都钉住）。
//!
//! 装配与 `table_data_routes.rs::wired_fixture` 同构 + **真租约源**：真
//! SQLite 管理库（v1..=v9 迁移）+ 真 membership/登录会话行 + 真 cookie/CSRF
//! （L2–L4 全链）+ `WireMockFactory`（MockDriver + `LegacyResourceAdapter`）
//! + 预算桥 + `ServerDriverExecBackend` + `ServerLiveConnectionSource` 经
//! `with_live_source` 接进真 `ServerRuntimeLeaseSource`——活句柄借出只读
//! backend 的 by_session 反向索引，**绝不新建物理连接**。夹具先 resolve 一次
//! cell 拿当前 runtime，再经真 `open_session` 用例开会话：签发/登记/借出读
//! 同一实例事实（CM-63 cell 语义；`result_cell_routes.rs` 刀4 夹具同构）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::idempotency::IdempotentOperation;
use datazen::execution::application::orchestration::RuntimeResultSink;
use datazen::execution::application::requests::OpenSessionRequest;
use datazen::execution::target::NamespaceTarget;
use datazen::execution::ExecutionTarget;
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::id::{
    ClientInstanceId, ConnectionId, EditorSessionId, IdempotencyKey, OrganizationId, PrincipalId,
    RequestId,
};
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{
    capabilities, ColumnSchema, DatabaseDriver, DatabaseDriverFactory, ForeignKeyDeferrability,
    ForeignKeyInfo, SslMode, TableInfo, TableSchema, TableType, Value as DriverValue,
};
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::{RuntimeLeaseSource as _, ServerRuntimeLeaseSource};
use datazen_server::execution::live_source::ServerLiveConnectionSource;
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::execution::target_runner::ServerTargetRunner;
use datazen_server::http::domains::table_data::{table_data_router, TableDataState};
use datazen_server::http::middleware::authorize::NOT_VISIBLE_MESSAGE as NOT_VISIBLE_SENTINEL;
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::{
    AdminStore, LoginSessions, ServerConnections, ServerProfiles, ServerSettingsStore,
};

const SERVER_SECRET: &[u8] = b"w8-live-routes-server-secret-32b!";

const FK_TARGETS_URI: &str = "/api/v1/table-data/foreign-key-targets";
const INBOUND_FKS_URI: &str = "/api/v1/table-data/inbound-foreign-keys";
const VALUE_SEARCHES_URI: &str = "/api/v1/table-data/value-searches";
const RESULT_ORIGINS_URI: &str = "/api/v1/table-data/result-origins";

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

/// membership 行 + 真登录会话行（L2）+ 真 CSRF 令牌（L4）——
/// `table_data_routes.rs::register_login` 同构（真实时钟戳，g1 冻结项先例）。
fn register_login(
    store: &Arc<AdminStore>,
    org: &str,
    principal: &str,
    session_id: &str,
    role: Role,
) -> Account {
    PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership(org, principal, role, &format!("idp:{principal}"))
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

/// oneshot 驱动认证链 Router（`table_data_routes.rs::call` 同款）。
async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf_token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
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
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, json)
}

// ── MockDriver 装配（table_data_routes.rs::WireMockFactory 同款）────────────

/// 把 [`MockDriver`] 连同 [`LegacyResourceAdapter`] 包成静态工厂
/// （`register_test_factory` 吃 `&'static dyn DatabaseDriverFactory`）。
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
            "live-routes-test",
            1,
            shape,
            capabilities,
        )))
    }
}

// ── 目录元数据 helper（t2/t3/t4/t5 的 FK 面）────────────────────────────────

fn column(name: &str, data_type: &str, primary: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable: false,
        default_value: None,
        comment: None,
        is_primary_key: primary,
        is_auto_increment: false,
    }
}

fn table_schema(
    name: &str,
    columns: Vec<ColumnSchema>,
    primary_keys: Vec<String>,
    foreign_keys: Vec<ForeignKeyInfo>,
) -> TableSchema {
    TableSchema {
        table_name: name.to_owned(),
        columns,
        primary_keys,
        indexes: Vec::new(),
        foreign_keys,
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

fn foreign_key(
    name: &str,
    columns: &[&str],
    referenced_table: &str,
    referenced_columns: &[&str],
) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.to_owned(),
        columns: columns.iter().map(|name| name.to_string()).collect(),
        referenced_table: referenced_table.to_owned(),
        referenced_columns: referenced_columns
            .iter()
            .map(|name| name.to_string())
            .collect(),
        on_update: "NO ACTION".to_owned(),
        on_delete: "NO ACTION".to_owned(),
        deferrability: ForeignKeyDeferrability::Unknown,
    }
}

/// 源表 `users`：`role_id → roles.id`（t2/t3/t4 的跳转源、t5 的被引用表）。
fn users_schema() -> TableSchema {
    table_schema(
        "users",
        vec![
            column("id", "integer", true),
            column("role_id", "integer", false),
        ],
        vec!["id".to_owned()],
        vec![foreign_key("fk_users_role", &["role_id"], "roles", &["id"])],
    )
}

/// 跳转目标表 `roles`：`id` 主键 + `label`。
fn roles_schema() -> TableSchema {
    table_schema(
        "roles",
        vec![
            column("id", "integer", true),
            column("label", "varchar", false),
        ],
        vec!["id".to_owned()],
        Vec::new(),
    )
}

/// 引用表 `orders`：`user_id → users.id`（t5 的反向目录条目）。
fn orders_schema() -> TableSchema {
    table_schema(
        "orders",
        vec![
            column("order_id", "integer", true),
            column("user_id", "integer", false),
        ],
        vec!["order_id".to_owned()],
        vec![foreign_key(
            "fk_orders_user",
            &["user_id"],
            "users",
            &["id"],
        )],
    )
}

/// FK 二条的驱动预录：绑定二形（MockDriver 的占位符词汇挂在
/// `parameterized_writes` 旗后——无旗时内联形状需要字面量语法，mockdb 未
/// 声明，语句构建会失败）+ `app` 库的 users/roles 表 schema。
fn fk_opts(query_rows: Vec<Vec<Option<DriverValue>>>) -> MockDriverOptions {
    let mut opts = MockDriverOptions::default();
    opts.parameterized_writes = true;
    opts.query_rows = query_rows;
    let mut app_tables = HashMap::new();
    app_tables.insert("users".to_owned(), users_schema());
    app_tables.insert("roles".to_owned(), roles_schema());
    opts.table_schemas_by_database
        .insert("app".to_owned(), app_tables);
    opts
}

/// 反向目录的驱动预录：`users`（被引用表）+ `orders`（引用表）+ 任意填充表
/// （把扫描推过 200 上限用）。填充表不在 schema 表里 → 空 schema、无 FK。
fn inbound_opts(extra_tables: Vec<String>) -> MockDriverOptions {
    let mut opts = MockDriverOptions::default();
    let mut tables = vec![
        TableInfo {
            name: "users".to_owned(),
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        },
        TableInfo {
            name: "orders".to_owned(),
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        },
    ];
    for name in extra_tables {
        tables.push(TableInfo {
            name,
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        });
    }
    opts.tables = tables;
    let mut app_tables = HashMap::new();
    app_tables.insert("users".to_owned(), users_schema());
    app_tables.insert("orders".to_owned(), orders_schema());
    opts.table_schemas_by_database
        .insert("app".to_owned(), app_tables);
    opts
}

/// 值搜索的驱动预录：绑定二形（同 FK 探针——COUNT(*) 语句的占位符走同一份
/// 方言词汇）。
fn search_opts() -> MockDriverOptions {
    let mut opts = MockDriverOptions::default();
    opts.parameterized_writes = true;
    opts
}

// ── 接线夹具（wired_fixture 同构 + 真租约源 + 真活连接源）───────────────────

struct LiveFixture {
    _dir: PathBuf,
    _store: Arc<AdminStore>,
    app: axum::Router,
    primary: Account,
    other_org: Account,
    connection_id: String,
    driver: Arc<MockDriver>,
    ctx: RequestContext,
    /// 真 `open_session` 用例面（签发/登记/借出读同一 runtime 实例）。
    sessions: Arc<dyn datazen::execution::application::use_cases::ConnectionUseCases>,
}

async fn live_fixture(opts: MockDriverOptions) -> LiveFixture {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w8-live-routes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    let primary = register_login(
        &store,
        "org-1",
        "user-1",
        "sess-w8-live-primary",
        Role::OrgMember,
    );
    let other_org = register_login(
        &store,
        "org-2",
        "user-2",
        "sess-w8-live-org2",
        Role::OrgMember,
    );

    let ctx = RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        // HTTP 面的 SessionIdentity 固定铸造 `http`（router.rs 提取器）；
        // open_session 的 owner 校验与借出的 ctx 过滤都要求同一客户端实例。
        ClientInstanceId::new("http"),
        RequestId::new("req-w8-live"),
        None,
    );

    // 驱动注册表：懒形 + 测试驱动 + 测试工厂（wired_fixture 同款）。
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
        ServerSecretVault::new(store.clone(), b"w8-live-secret".to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    // profile 行 + 连接目录行（同一连接 id 两半都齐；database `app` 是
    // FK/值搜索档案回退梯子的落点）。
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("live-conn", "mockdb"),
            &IdempotencyKey::new("idem-w8-live-profile"),
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
                name: "live conn".into(),
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
                OrganizationId::new("org-1"),
            ),
        ),
        OrganizationId::new("org-1"),
    ));
    let backend = Arc::new(ServerDriverExecBackend::new(
        registry.clone(),
        connections.clone(),
        profiles.clone(),
        scope.clone(),
        bridge,
        None,
    ));
    let sink = RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let directory =
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
    // 刀4 夹具装配（生产 bootstrap 同构）：活连接出向端口适配器吃 backend
    // 的 clone（by_session 反向索引随 backend.open 同帧写入，两份 Arc 必须
    // 指向同一 backend 实例）；租约源 `.with_live_source` 接线后 resolve 一次
    // 拿当前单元——会话开立 / 借出读同一实例（CM-63 cell 语义）。
    let live_source = Arc::new(ServerLiveConnectionSource::new(
        backend.clone(),
        connections.clone() as Arc<dyn datazen::connection::domain::catalog::ConnectionRepository>,
        registry.clone(),
        scope,
    ));
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
    let runtime = lease_source.resolve().await.expect("runtime unit resolves");
    let directory_face: Arc<dyn datazen::execution::SessionDirectory> =
        lease_source.session_directory();
    let host = Arc::new(ServerResourceExecutionHost::new(
        connections.clone(),
        profiles.clone(),
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        lease_source.clone(),
        "org-1",
    ));
    let runner = Arc::new(ServerTargetRunner::new(host, lease_source.clone()));

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
        domain_routers: Some(vec![table_data_router(TableDataState {
            store: store.clone(),
            registry,
            runner,
            profiles,
            connections,
            directory: directory_face,
            sessions: runtime.clone(),
            // 四读端点的活句柄借出租约源：真租约源 + 真活连接源（缺省
            // unwired 是 fail-closed 503，这里贴近生产接线）。
            lease: lease_source,
            schema_cache: Arc::new(datazen::schema::cache::SchemaCache::new()),
        })]),
    })
    .expect("router");

    LiveFixture {
        _dir: dir,
        _store: store,
        app,
        primary,
        other_org,
        connection_id,
        driver,
        ctx,
        sessions: runtime,
    }
}

impl LiveFixture {
    /// 经真 `open_session` 用例开立会话，返回 dbSessionId。先 resolve 的
    /// cell runtime 就是签发者——borrow_live_for_session / bind_session 的
    /// 借出读同一实例的登记账（刀0 cell 语义）。
    async fn open_session(&self) -> String {
        let target = ExecutionTarget::new(
            ConnectionId::new(self.connection_id.clone()),
            NamespaceTarget::new(Some("app".to_owned()), None, None, Vec::new()),
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
                        client_instance_id: ClientInstanceId::new("http"),
                        editor_session_id: EditorSessionId::new("live-editor-1"),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .expect("session opens through the real use case");
        receipt.session.handle.db_session_id.as_str().to_owned()
    }
}

// ── t1 认证链 401 ×4 ────────────────────────────────────────────────────────

/// 四个活句柄面读端点都在认证链内：无 cookie 的 POST 在 L2 被逐个 401 拒绝
/// （`table_data_routes.rs` 同一条探针，从读三端点扩展到刀2 四端点）。
#[tokio::test]
async fn t1_the_four_live_read_endpoints_sit_inside_the_auth_chain() {
    let fx = live_fixture(MockDriverOptions::default()).await;
    for uri in [
        FK_TARGETS_URI,
        INBOUND_FKS_URI,
        VALUE_SEARCHES_URI,
        RESULT_ORIGINS_URI,
    ] {
        let (status, _) = call(&fx.app, "POST", uri, "", None, Some(json!({}))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "unauthenticated {uri}");
    }
}

// ── t2 FK 跳转绿路径 ────────────────────────────────────────────────────────

/// 绿路径全链：真 open_session → `bind_session` 借出活句柄（真租约源 →
/// `ServerLiveConnectionSource` 的 by_session 反查）→ 源表 FK 元数据
/// （`users.role_id → roles.id`）→ 目标校验（roles 主键/列档案）→ 探针命中。
/// 200 `exists:true` + candidates 一行 `[7]` + 目标档案逐字段。
#[tokio::test]
async fn t2_an_fk_jump_resolves_the_target_and_probes_it_over_a_live_borrow() {
    let fx = live_fixture(fk_opts(vec![vec![Some(DriverValue::Integer(7))]])).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        FK_TARGETS_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
            "columns": ["role_id"],
            "values": [7],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["exists"], json!(true), "the probe hits: {body}");
    assert_eq!(body["candidates"], json!([[7]]), "{body}");
    assert_eq!(body["truncated"], json!(false), "{body}");
    assert_eq!(body["target"]["table"], json!("roles"), "{body}");
    assert_eq!(body["target"]["database"], json!("app"), "{body}");
    assert_eq!(body["target"]["schema"], Value::Null, "{body}");
    assert_eq!(body["target"]["columns"], json!(["id"]), "{body}");
    assert_eq!(body["target"]["dataTypes"], json!(["integer"]), "{body}");
    assert_eq!(body["target"]["primaryKeys"], json!(["id"]), "{body}");
    assert_eq!(
        body["target"]["unique"],
        json!(true),
        "the referenced column is the target's primary key: {body}"
    );
    assert_eq!(body["target"]["candidateLimit"], json!(50), "{body}");
}

// ── t3 引用行已不在 = exists:false 成功答案 ─────────────────────────────────

/// 探针空行集（query_rows 预录为空）：目标表可读但引用行已不在——200
/// `exists:false` + 空 candidates，**不是错误**（fk_navigation.rs 的头号
/// 规则，活句柄面同判）。
#[tokio::test]
async fn t3_a_missing_referenced_row_answers_exists_false_as_a_success() {
    let fx = live_fixture(fk_opts(Vec::new())).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        FK_TARGETS_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
            "columns": ["role_id"],
            "values": [7],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "exists:false is an answer: {body}");
    assert_eq!(body["exists"], json!(false), "{body}");
    assert_eq!(body["candidates"], json!([]), "{body}");
    assert_eq!(body["truncated"], json!(false), "{body}");
    assert_eq!(
        body["target"]["table"],
        json!("roles"),
        "the target is still resolved and named: {body}"
    );
}

// ── t4 未知约束 → grid.fk.unknownConstraint: 前缀 400 ───────────────────────

/// 源表元数据可读但请求指认的列组不构成任何外键：400，文案携带
/// `grid.fk.unknownConstraint:` 前缀（前端分类契约，第一词保持码本身）。
#[tokio::test]
async fn t4_an_unknown_constraint_is_rejected_with_the_grid_fk_code() {
    let fx = live_fixture(fk_opts(Vec::new())).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        FK_TARGETS_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
            "columns": ["ghost_column"],
            "values": [1],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let message = body["message"].as_str().expect("message");
    assert!(
        message.starts_with("grid.fk.unknownConstraint:"),
        "the first word stays the wire code: {message}"
    );
    assert!(message.contains("ghost_column"), "{message}");
}

// ── t5 反向目录扫描 ─────────────────────────────────────────────────────────

/// `listInboundForeignKeys`：小目录里 `orders → users` 的约束条目逐字段 +
/// `truncated:false` 如实；大目录（201 张填充表把扫描推过 200 上限）里
/// `truncated:true` + `scannedTables:200` 如实早停，先扫到的 orders 条目
/// 仍在——诚实部分答案，不是静默全集。
#[tokio::test]
async fn t5_the_inbound_catalog_scans_the_schema_and_reports_truncation_honestly() {
    // 小目录：恰好一条反向引用，扫描完整。
    let fx = live_fixture(inbound_opts(Vec::new())).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        INBOUND_FKS_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["table"], json!("users"), "{body}");
    assert_eq!(body["database"], json!("app"), "{body}");
    let references = body["references"].as_array().expect("references");
    assert_eq!(
        references.len(),
        1,
        "orders→users is the only entry: {body}"
    );
    assert_eq!(references[0]["table"], json!("orders"), "{body}");
    assert_eq!(
        references[0]["constraintName"],
        json!("fk_orders_user"),
        "{body}"
    );
    assert_eq!(references[0]["columns"], json!(["user_id"]), "{body}");
    assert_eq!(references[0]["referencedColumns"], json!(["id"]), "{body}");
    assert_eq!(body["scannedTables"], json!(1), "{body}");
    assert_eq!(body["truncated"], json!(false), "{body}");

    // 大目录：扫描在 200 张表处如实早停。
    let fillers: Vec<String> = (0..201).map(|index| format!("filler_{index}")).collect();
    let fx = live_fixture(inbound_opts(fillers)).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        INBOUND_FKS_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["scannedTables"],
        json!(200),
        "the cap is honest: {body}"
    );
    assert_eq!(body["truncated"], json!(true), "{body}");
    let references = body["references"].as_array().expect("references");
    assert_eq!(
        references.len(),
        1,
        "what was scanned before the cap is not lost: {body}"
    );
    assert_eq!(references[0]["table"], json!("orders"), "{body}");
}

// ── t6 值搜索计数 ───────────────────────────────────────────────────────────

/// `searchTableValues` 绿路径：活句柄借出 → 档案回退 → 共享层构建的
/// COUNT(*) 单格语句经 MockDriver 的 count 分支应答 → 200 `matches` 携带
/// 计数。恰好一条语句到达驱动（查询日志为证）。
#[tokio::test]
async fn t6_a_value_search_counts_matches_through_the_driver_count_branch() {
    let mut opts = search_opts();
    opts.count_total_value = Some(DriverValue::Integer(42));
    let fx = live_fixture(opts).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        VALUE_SEARCHES_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
            "columns": ["email", "name"],
            "value": "needle",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["table"], json!("users"), "{body}");
    assert_eq!(body["searchedColumns"], json!(["email", "name"]), "{body}");
    assert_eq!(
        body["matches"],
        json!(42),
        "the count rides the wire: {body}"
    );

    let log = fx.driver.query_log();
    assert_eq!(
        log.len(),
        1,
        "exactly one statement reaches the driver: {log:?}"
    );
    assert!(
        log[0].contains("COUNT(*)"),
        "the count statement: {:?}",
        log[0]
    );
    assert!(log[0].to_uppercase().contains("USERS"), "{:?}", log[0]);
}

// ── t7 空值 → 400 ───────────────────────────────────────────────────────────

/// 信封预检：空白搜索值在借出之前被 400 `value is required` 拒绝
/// （镜像桌面 required 文案；handler 侧预检先于一切会话触碰）。
#[tokio::test]
async fn t7_a_blank_search_value_is_refused_with_a_400() {
    let fx = live_fixture(search_opts()).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        VALUE_SEARCHES_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
            "columns": ["email"],
            "value": "   ",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["message"], json!("value is required"), "{body}");
}

// ── t8 origin 绿路径（Undeterminable → 只读裁决）────────────────────────────

/// `resolveResultOrigin` 绿路径：非 SELECT 语句（START TRANSACTION）经
/// MockDriver 的 origin 契约应答 `Undeterminable`（不是驱动错误）→ 200
/// 只读裁决 `writable:false` + `reason:"unknownOrigin"`（verdict plus a
/// reason, never a bare no）。定位键半边走 `SchemaCache` 目录读：主键
/// `id` 来自目录（`get_columns` 恰好一次——缓存半边的真读证据），
/// `identityColumns` 以 `resultColumns` 里的同名键铸出。
#[tokio::test]
async fn t8_an_undeterminable_origin_answers_a_read_only_verdict_from_the_schema_cache() {
    let mut opts = MockDriverOptions::default();
    opts.columns = vec![
        column("id", "integer", true),
        column("email", "varchar", false),
    ];
    opts.primary_keys = vec!["id".to_owned()];
    let fx = live_fixture(opts).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        RESULT_ORIGINS_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "db_session_id": db_session_id,
            "database": "app",
            "sql": "START TRANSACTION",
            "result_columns": ["id", "email"],
            "table": "users",
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a read-only verdict is still 200: {body}"
    );
    assert_eq!(body["writable"], json!(false), "{body}");
    assert_eq!(body["reason"], json!("unknownOrigin"), "{body}");
    assert_eq!(body["table"], Value::Null, "no relation is claimed: {body}");
    assert_eq!(body["editableColumns"], json!([]), "{body}");
    assert_eq!(
        body["identityColumns"],
        json!([{"column": "id", "sourceColumn": "id"}]),
        "the locating key comes from the catalog cache, not the origin: {body}"
    );
    assert_eq!(
        fx.driver.get_columns_calls(),
        1,
        "the schema cache read the catalog exactly once"
    );
}

// ── t9 跨组织借出·FK 面 ─────────────────────────────────────────────────────

/// org-2 主体借 org-1 的会话跑 FK 跳转：`bind_session` 的授权链
/// （authorize → sessions 账 → ctx 过滤）把跨组织借出判成
/// `SessionNotFound`，适配器映射 404，文案「DB session … is unavailable」
/// ——不是导航规则失败，不带 grid 前缀。
#[tokio::test]
async fn t9_an_fk_jump_across_organizations_answers_the_unavailable_db_session_404() {
    let fx = live_fixture(fk_opts(Vec::new())).await;
    let db_session_id = fx.open_session().await;
    let (status, body) = call(
        &fx.app,
        "POST",
        FK_TARGETS_URI,
        &fx.other_org.cookie,
        Some(&fx.other_org.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
            "columns": ["role_id"],
            "values": [7],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body["message"],
        json!(format!("DB session {db_session_id} is unavailable")),
        "{body}"
    );
}

// ── t10 跨组织借出·值搜索面 ─────────────────────────────────────────────────

/// `borrow_live_for_session` 的可见性闸两种防枚举口径都钉住：越组织
/// （会话存在但不可见）与未知会话 id 同答 404 固定文案
/// 「The addressed resource does not exist or is not visible to you.」
/// ——不区分「不存在」与「不可见」，ID 枚举无从谈起。
#[tokio::test]
async fn t10_a_value_search_across_organizations_answers_the_fixed_not_visible_404() {
    let fx = live_fixture(search_opts()).await;
    let db_session_id = fx.open_session().await;

    // 口径一：越组织——org-2 借 org-1 的会话。
    let (status, body) = call(
        &fx.app,
        "POST",
        VALUE_SEARCHES_URI,
        &fx.other_org.cookie,
        Some(&fx.other_org.csrf),
        Some(json!({
            "dbSessionId": db_session_id,
            "database": "app",
            "table": "users",
            "columns": ["email"],
            "value": "needle",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body["message"].as_str(),
        Some(NOT_VISIBLE_SENTINEL),
        "{body}"
    );

    // 口径二：未知会话 id——同一条固定文案，两种口径不可区分。
    let (status, body) = call(
        &fx.app,
        "POST",
        VALUE_SEARCHES_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "dbSessionId": "no-such-live-session",
            "database": "app",
            "table": "users",
            "columns": ["email"],
            "value": "needle",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body["message"].as_str(),
        Some(NOT_VISIBLE_SENTINEL),
        "{body}"
    );
}
