//! W8 刀3：变更二成对端点（`change-plan-previews` / `change-commits`）
//! 的集成绿路径与护栏测试（`w8-knife3-mutation-pair-design.md` §5 测试
//! 计划的 t1–t8 + t10；t9 探针翻新落在 `table_data_routes.rs`，t11 错误
//! 映射臂钉落在 `table_data.rs` 的 `#[cfg(test)]` 组件模块）。
//!
//! 装配沿 `table_data_routes.rs` 的 `wired_fixture` 同构（真 SQLite 管理
//! 库 + `WireMockFactory` + `LegacyResourceAdapter` + 预算桥 +
//! `ServerDriverExecBackend` + 真 `ServerRuntimeLeaseSource` +
//! `ServerTargetRunner`；manual 分支吃 `FixedLeaseSource` 保会话连续），
//! 两处差异如实登记：
//!
//! - **连接配置参数化** `read_only`（t5 只读闸需要——`wired_fixture`
//!   的 `ConnectionConfig` 硬编码 `read_only: false`）；
//! - **commit 写半边的租约源用真 `ServerRuntimeLeaseSource`**（WIP 刀3
//!   的 `table_data_routes.rs` 已同款：单次 resolve 的实例贯穿提交序列，
//!   CM-63 cell 语义，贴近生产）。
//!
//! 列源走刀1 的命令面（`get_table_schema` 经 MockDriver
//! `command_results` 预录应答，`CommandResult.data = {"schema": …}`）。
//! t10 的偏离登记：设计档 t10 的「`get_session` → SessionLost」断言需要
//! owned session 的 handle，而 `CommitPendingChangesResponse` 不携带它
//! （owned 会话是宿主内部事实）——落成可达断言：`commit_calls() == 1`
//! （事务确实提交走完，`write_exec` 关闭路径在 commit 之后）+ 幂等重放
//! 全链再成功（资源面无残留：新会话新 handle 新事务正常开立）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use datazen_driver_api::TransactionHandle;
use serde_json::{json, Value};
use tower::ServiceExt;

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::{ProfilePatch, ProfileRepository as _};
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::orchestration::{
    RuntimeConnectionUseCases, RuntimeResultSink, SystemRuntimeClock,
};
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, ConfigRevision, ConnectionId, IdempotencyKey};
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{capabilities, DatabaseDriver, DatabaseDriverFactory, SslMode};
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::{RuntimeLeaseSource, ServerRuntimeLeaseSource};
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::execution::target_runner::ServerTargetRunner;
use datazen_server::http::domains::table_data::{table_data_router, TableDataState};
use datazen_server::http::router::{build_router_with_auth, AuthDeps};
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::{
    AdminStore, LoginSessions, ServerConnections, ServerProfiles, ServerSettingsStore,
};

const SERVER_SECRET: &[u8] = b"w3-table-data-server-secret-32-bytes!";

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
/// `table_data_routes.rs::register_login` 同构。
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

/// oneshot 驱动认证链 Router（`table_data_routes.rs::call` 同构）。
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

/// 把 [`MockDriver`] 连同 [`LegacyResourceAdapter`] 包成静态工厂
/// （`table_data_routes.rs::WireMockFactory` 同构）。
struct WireMockFactory {
    driver_id: &'static str,
    driver: Arc<MockDriver>,
}

impl DatabaseDriverFactory for WireMockFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        // 具体型 → trait object 的 `as` coercion（字段位不自动，返回位同样）
        self.driver.clone() as Arc<dyn DatabaseDriver>
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
        let mut capability_set = capabilities::CapabilitySet::default();
        capability_set.stateful_session = capabilities::Availability::Supported;
        capability_set.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
        capability_set.context_observation = capabilities::ContextObservation::Full;
        capability_set.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
        capability_set.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
        Some(Arc::new(TransactionalMockProvider::new(
            Arc::clone(&self.driver),
            self.driver_id,
            "wire-test",
            1,
            shape,
            capability_set,
        )))
    }
}

/// 进程内驱动的测试 provider（W8 刀3）：`LegacyResourceAdapter` 的包装——
/// 全方法委托 adapter（acquire/observe/execute/close 与 `wired_fixture`
/// 同构），仅事务三方法覆盖为 MockDriver 真实现。
///
/// 动机（如实登记）：adapter 的 `commit_transaction` 是 legacy 形态裁决
/// （「legacy 驱动在自家命令里解决事务，宿主读不回」——
/// `resource_adapter.rs` 注释），对进程内驱动不成立：生产 PostgreSQL
/// provider 的 `commit_transaction` 就是完整实现（`finish_transaction`）。
/// 变更提交的 owned-session 全链需要 begin→execute→commit 的事务面，
/// 本件即「进程内驱动 provider」的最小测试形（与生产 PG provider 同构）。
struct TransactionalMockProvider {
    inner: LegacyResourceAdapter,
    driver: Arc<MockDriver>,
    txs: std::sync::Mutex<HashMap<String, TransactionHandle>>,
}

impl TransactionalMockProvider {
    fn new(
        driver: Arc<MockDriver>,
        driver_id: &'static str,
        driver_version: &'static str,
        runtime_epoch: u64,
        shape: datazen_driver_api::namespace::NamespaceShape,
        capabilities: capabilities::CapabilitySet,
    ) -> Self {
        let driver_dyn: Arc<dyn DatabaseDriver> = driver.clone() as _;
        Self {
            inner: LegacyResourceAdapter::new(
                driver_dyn,
                driver_id,
                driver_version,
                runtime_epoch,
                shape,
                capabilities,
            ),
            driver,
            txs: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl datazen_driver_api::resource::ResourceProvider for TransactionalMockProvider {
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }

    fn capabilities(&self) -> &datazen_driver_api::capabilities::CapabilityRegistry {
        self.inner.capabilities()
    }

    fn namespace_shape(&self) -> &datazen_driver_api::namespace::NamespaceShape {
        self.inner.namespace_shape()
    }

    fn connection_handle(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
    ) -> Result<datazen_driver_api::ConnectionHandle, datazen_driver_api::resource::ResourceError>
    {
        self.inner.connection_handle(handle)
    }

    async fn describe_resource(
        &self,
        request: &datazen_driver_api::resource::DescribeResourceRequest,
    ) -> Result<
        datazen_driver_api::resource::ResourceDescriptor,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner.describe_resource(request).await
    }

    async fn acquire_resource(
        &self,
        request: &datazen_driver_api::resource::AcquireResourceRequest,
        budget: &Arc<dyn datazen_driver_api::resource::BudgetPort>,
    ) -> Result<
        datazen_driver_api::resource::ResourceHandle,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner.acquire_resource(request, budget).await
    }

    async fn execute_on_resource(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
        execution_id: &datazen_driver_api::QueryExecutionId,
        call: &datazen_driver_api::resource::CommandCall,
        sink: &dyn datazen_driver_api::resource::ResultSink,
    ) -> Result<
        datazen_driver_api::session::ExecutionCompletion,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner
            .execute_on_resource(handle, execution_id, call, sink)
            .await
    }

    async fn observe_session(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
    ) -> Result<
        datazen_driver_api::session::SessionObservation,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner.observe_session(handle).await
    }

    async fn change_context(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
        desired: &datazen_driver_api::namespace::NamespaceTarget,
    ) -> Result<
        datazen_driver_api::session::ContextChangeDisposition,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner.change_context(handle, desired).await
    }

    async fn begin_transaction(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
        options: &datazen_driver_api::session::TransactionOptions,
    ) -> Result<
        datazen_driver_api::session::TransactionObservation,
        datazen_driver_api::resource::ResourceError,
    > {
        let _ = options;
        let connection = self.inner.connection_handle(handle)?;
        let tx = self
            .driver
            .begin_transaction(&connection)
            .await
            .map_err(datazen_driver_api::resource::ResourceError::Driver)?;
        let tx_id = tx.id.clone();
        self.txs
            .lock()
            .expect("mock provider txs")
            .insert(handle.resource_key().to_owned(), tx);
        Ok(datazen_driver_api::session::TransactionObservation::begun(
            tx_id, 0,
        ))
    }

    async fn commit_transaction(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
    ) -> Result<
        datazen_driver_api::session::TransactionObservation,
        datazen_driver_api::resource::ResourceError,
    > {
        let tx = self
            .txs
            .lock()
            .expect("mock provider txs")
            .remove(handle.resource_key())
            .ok_or_else(
                || datazen_driver_api::resource::ResourceError::InvalidResourceState {
                    resource_key: handle.resource_key().to_owned(),
                    operation: "commit_transaction".into(),
                    state: "no transaction open".into(),
                },
            )?;
        self.driver
            .commit(tx)
            .await
            .map_err(datazen_driver_api::resource::ResourceError::Driver)?;
        Ok(
            datazen_driver_api::session::TransactionObservation::finished(
                datazen_driver_api::session::TransactionState::Idle,
                datazen_driver_api::session::EffectOutcome::Completed,
                0,
            ),
        )
    }

    async fn rollback_transaction(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
    ) -> Result<
        datazen_driver_api::session::TransactionObservation,
        datazen_driver_api::resource::ResourceError,
    > {
        let tx = self
            .txs
            .lock()
            .expect("mock provider txs")
            .remove(handle.resource_key())
            .ok_or_else(
                || datazen_driver_api::resource::ResourceError::InvalidResourceState {
                    resource_key: handle.resource_key().to_owned(),
                    operation: "rollback_transaction".into(),
                    state: "no transaction open".into(),
                },
            )?;
        self.driver
            .rollback(tx)
            .await
            .map_err(datazen_driver_api::resource::ResourceError::Driver)?;
        Ok(
            datazen_driver_api::session::TransactionObservation::finished(
                datazen_driver_api::session::TransactionState::Idle,
                datazen_driver_api::session::EffectOutcome::RolledBack,
                0,
            ),
        )
    }

    async fn request_cancel(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
        execution_id: &datazen_driver_api::QueryExecutionId,
    ) -> Result<
        datazen_driver_api::session::CancelReceipt,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner.request_cancel(handle, execution_id).await
    }

    async fn reset_resource(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
        baseline: &datazen_driver_api::resource::Baseline,
    ) -> Result<
        datazen_driver_api::session::ResetDisposition,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner.reset_resource(handle, baseline).await
    }

    async fn close_resource(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
    ) -> Result<
        datazen_driver_api::session::CloseDisposition,
        datazen_driver_api::resource::ResourceError,
    > {
        self.inner.close_resource(handle).await
    }

    async fn close_handle(
        &self,
        handle: &datazen_driver_api::resource::ResourceHandle,
        session_handle: &datazen_driver_api::session::SessionHandleRef,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        self.inner.close_handle(handle, session_handle).await
    }
}

/// 会话连续性的测试形（`table_data_routes.rs::FixedLeaseSource` 同构）：
/// runner 的 manual 分支吃固定租约源，target 分支经 host 的真租约源自洽。
struct FixedLeaseSource(Arc<RuntimeConnectionUseCases>);

#[async_trait::async_trait]
impl RuntimeLeaseSource for FixedLeaseSource {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        Ok(self.0.clone())
    }
}

/// 变更二端点的接线夹具（`wired_fixture` 同构 + `read_only` 参数化）。
struct MutationFixture {
    _store: Arc<AdminStore>,
    app: axum::Router,
    primary: Account,
    other_org: Account,
    connection_id: String,
    driver: Arc<MockDriver>,
    profiles: Arc<ServerProfiles>,
    ctx: RequestContext,
    sessions: Arc<dyn datazen::execution::application::use_cases::ConnectionUseCases>,
}

async fn mutation_fixture(read_only: bool, opts: MockDriverOptions) -> MutationFixture {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w8-mutation-pair-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    // 主体的授权基线（设计档 §5 之外的第 8 项发现，如实登记）：写执行
    // 命令（platform.beginTransaction / execute）经 `enqueue` 的 action
    // 判定归 `Write` → `(Write, Execution)` 组织闸 + `(Write, Connection)`
    // 目标闸——RBAC 基线里 OrgMember 的 Execution 只有 Read/Execute/Cancel
    // （rbac.rs 钉子测试逐格钉住，改矩阵 = 改授权基线 = 改测试，非本刀
    // 权限）。写授权是部署授权决策：OrgAdmin / ACL grant / ConnectionOwner
    // 的矩阵调整属产品授权面，登记为开放问题随台账上交。因此本夹具的
    // 提交主体是 OrgAdmin；「OrgMember 提交变更」是前端产品语义问题，
    // 不在本测试文件里裁决。
    let primary = register_login(
        &store,
        "org-1",
        "user-1",
        "sess-w8-mutation-primary",
        Role::OrgAdmin,
    );
    let other_org = register_login(
        &store,
        "org-2",
        "user-2",
        "sess-w8-mutation-org2",
        Role::OrgMember,
    );

    let ctx = RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        // HTTP 面的 SessionIdentity 固定铸造 `http`（router.rs 提取器）；
        // owned-session 的 open_session owner 校验要求 owner 与 ctx 同一
        // 客户端实例，fixture ctx 随之对齐（wired_fixture 同款）。
        ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-w8-mutation"),
        None,
    );

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

    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("wire-conn", "mockdb"),
            &IdempotencyKey::new("idem-w8-mutation-profile"),
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
                read_only,
                pinned: false,
            },
        )
        .await
        .expect("connection config saved");

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
                    .expect("policy"),
            ),
            store.clone(),
            Arc::new(datazen::shared::clock::SystemClock),
        ),
    );
    let directory =
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new());
    let runtime = Arc::new(RuntimeConnectionUseCases::new_with_parts(
        profiles.clone(),
        policy.clone(),
        backend.clone(),
        Arc::new(SystemRuntimeClock(std::time::Instant::now()))
            as Arc<dyn datazen::execution::runtime::gateway::MonotonicClock>,
        sink.clone(),
        directory.clone(),
    ));
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
        real_lease.clone(),
        "org-1",
    ));
    let runner = Arc::new(ServerTargetRunner::new(
        host,
        Arc::new(FixedLeaseSource(runtime.clone())) as Arc<dyn RuntimeLeaseSource>,
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
            profiles: profiles.clone(),
            connections,
            directory,
            sessions: runtime.clone(),
            // commit 写半边的租约源：真租约源贴近生产（单次 resolve 的
            // 实例贯穿提交序列，CM-63 cell 语义）。
            lease: real_lease,
            schema_cache: Arc::new(datazen::schema::cache::SchemaCache::new()),
        })]),
    })
    .expect("router");

    MutationFixture {
        _store: store,
        app,
        primary,
        other_org,
        connection_id,
        driver,
        profiles,
        ctx,
        sessions: runtime.clone(),
    }
}

// ── 预录与请求体 helper ──────────────────────────────────────────────────

/// `get_table_schema` 的 `CommandResult.data`：`{"schema": <TableSchema>}`
/// （`resource_adapter.rs` + `schema_catalog_commands.rs` 的既有形状）。
/// 列 serde camelCase；`id` 列主键（`primaryKeys` 字段优先路径）。
fn schema_payload() -> Value {
    json!({
        "tableName": "users",
        "columns": [
            {"name": "id", "dataType": "integer", "nullable": false,
             "defaultValue": null, "comment": null,
             "isPrimaryKey": true, "isAutoIncrement": true},
            {"name": "email", "dataType": "varchar", "nullable": false,
             "defaultValue": null, "comment": null,
             "isPrimaryKey": false, "isAutoIncrement": false},
        ],
        "primaryKeys": ["id"],
        "indexes": [],
        "foreignKeys": [],
    })
}

/// t1/t2/t3(c)/t5(preview 腿)/t6/t7/t10 的驱动预录：参数化写生效 +
/// affected=1 + 列源 schema 应答。
fn writable_opts() -> MockDriverOptions {
    let mut opts = MockDriverOptions::default();
    opts.parameterized_writes = true;
    opts.execute_rows_affected = 1;
    opts.command_results.insert(
        "get_table_schema".to_owned(),
        json!({ "schema": schema_payload() }),
    );
    opts
}

fn update_change() -> Value {
    json!({
        "rowIdentity": {"id": 1},
        "originalValues": {"email": "a@b.c"},
        "currentValues": {"email": "z@b.c"},
        "changedColumns": ["email"],
        "deleteMarked": false,
    })
}

fn preview_body(connection_id: &str) -> Value {
    json!({
        "context": {
            "connectionId": connection_id,
            "dbSessionId": "wire-change-session",
            "driverType": "mockdb",
            "database": "app",
            "schema": null,
            "table": "users",
        },
        "changes": [update_change()],
    })
}

fn commit_body(plan: &Value) -> Value {
    json!({
        "dbSessionId": "wire-change-session",
        "plan": plan,
        "fingerprint": plan["fingerprint"],
    })
}

const PREVIEW_URI: &str = "/api/v1/table-data/change-plan-previews";
const COMMIT_URI: &str = "/api/v1/table-data/change-commits";

// ── t1：preview 绿路径 ──────────────────────────────────────────────────

/// MockDriver 预录含主键 schema → `POST /change-plan-previews` → 200：
/// `planId` 非空、`fingerprint` 非空、`configRevision` = profile revision
/// （初始 1，十进制字符串 CM-01）、`updates[0].sqlTemplate` 含 UPDATE、
/// `table` 回显上下文。
#[tokio::test]
async fn t1_a_preview_answers_a_fingerprinted_plan_through_the_column_source() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (status, body) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["planId"].as_str().expect("planId").len() > 0,
        "plan id is minted: {body}"
    );
    let fingerprint = body["fingerprint"].as_str().expect("fingerprint");
    assert!(
        !fingerprint.is_empty(),
        "fingerprint rides the plan: {body}"
    );
    assert_eq!(
        body["configRevision"],
        json!("1"),
        "the plan stamps the preview-time profile revision (decimal string): {body}"
    );
    let updates = body["updates"].as_array().expect("updates");
    assert_eq!(updates.len(), 1, "one update statement: {body}");
    assert!(
        updates[0]["sqlTemplate"]
            .as_str()
            .expect("sql")
            .contains("UPDATE"),
        "the driver-rendered preview mentions UPDATE: {body}"
    );
    assert_eq!(
        body["table"]["table"],
        json!("users"),
        "table echoes: {body}"
    );
    assert_eq!(
        body["table"]["connectionId"],
        json!(fx.connection_id),
        "{body}"
    );
}

// ── t2：commit 绿路径（owned-session 全链）────────────────────────────────

/// t1 的 plan 原样提交 → 200：`affectedRows == 1`、`statements[0]` 的
/// operation == "update" 且 rowIdentity 逐字段一致；驱动恰好被
/// `execute_with_params` 调用一次、事务恰好提交一次（t10 可达断言）。
#[tokio::test]
async fn t2_a_commit_executes_the_plan_through_an_owned_session() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&plan)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["affectedRows"], json!(1), "{body}");
    let statements = body["statements"].as_array().expect("statements");
    assert_eq!(statements.len(), 1, "{body}");
    assert_eq!(statements[0]["operation"], json!("update"), "{body}");
    assert_eq!(
        statements[0]["rowIdentity"],
        json!({"id": 1}),
        "row identity round trips: {body}"
    );
    assert_eq!(
        fx.driver.execute_with_params_calls(),
        1,
        "exactly one parameterized write ran"
    );
    assert_eq!(fx.driver.commit_calls(), 1, "the transaction committed");
}

// ── t3：成对不变量 + manual 明拒 ─────────────────────────────────────────

/// (b) 体带 `manualHandle` → 501 `capabilityUnsupported`（拒绝先于一切执行
/// 触碰：驱动零参数化写调用），文案指引 owned-session 形态；(c)
/// `manualHandle: null` 与缺省同义 → 同一条 owned-session 绿路径。
#[tokio::test]
async fn t3_a_manual_handle_is_refused_before_any_execution_touch() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    let mut body = commit_body(&plan);
    body["manualHandle"] = json!({"dbSessionId": "wire-change-session", "runtimeEpoch": "1"});
    let (status, answer) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{answer}");
    assert_eq!(answer["code"], json!("capabilityUnsupported"), "{answer}");
    let message = answer["message"].as_str().expect("message");
    assert!(message.contains("manual"), "{answer}");
    assert!(message.contains("owned-session"), "{answer}");
    assert_eq!(
        fx.driver.execute_with_params_calls(),
        0,
        "the refusal precedes every execution touch"
    );
}

#[tokio::test]
async fn t3_an_explicit_null_manual_handle_means_owned_session() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    let mut body = commit_body(&plan);
    body["manualHandle"] = Value::Null;
    let (status, answer) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["affectedRows"], json!(1), "{answer}");
}

// ── t4：指纹/不可变计划护栏 ──────────────────────────────────────────────

/// 篡改 `currentValues` 保留原 fingerprint → 400 指纹失配（不可变计划重建
/// 重按发现漂移）；`configRevision` 加一 → 400 CAS stalePlan（重读列之前
/// 拒绝）。两组都断言驱动零参数化写调用。
#[tokio::test]
async fn t4_a_tampered_plan_is_refused_and_never_touches_the_write_path() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;

    // 腿 1：篡改行内容、保留原 fingerprint → 不可变重建发现漂移。
    let mut tampered = plan.clone();
    tampered["updates"][0]["currentValues"]["email"] = json!("hijacked@evil.c");
    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&tampered)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("fingerprint is stale or was modified"),
        "{body}"
    );
    assert_eq!(fx.driver.execute_with_params_calls(), 0, "{body}");

    // 腿 2：plan 的 configRevision 漂移 → 共享层 CAS 在重读列之前拒绝。
    let mut drifted = plan.clone();
    drifted["configRevision"] = json!("2");
    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&drifted)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("Connection configuration changed"),
        "{body}"
    );
    assert_eq!(fx.driver.execute_with_params_calls(), 0, "{body}");
}

// ── t5：只读连接 ────────────────────────────────────────────────────────

/// `read_only` 连接：preview 仍 200（预览不执行任何语句，不设只读闸——
/// 桌面镜像）；commit 400 `grid.cell.readOnly`（共享层闸先于重读列，驱动
/// 零调用）。
#[tokio::test]
async fn t5_a_read_only_connection_previews_but_never_commits() {
    let fx = mutation_fixture(true, writable_opts()).await;
    let (status, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&plan)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("grid.cell.readOnly"),
        "{body}"
    );
    assert_eq!(
        fx.driver.execute_with_params_calls(),
        0,
        "the read-only gate precedes the column re-read and the write path"
    );
}

// ── t6：提交期 CAS ──────────────────────────────────────────────────────

/// preview 之后 profile revision 前进（`compare_and_set` 1→2）→ 原样
/// commit 400 stalePlan（新 revision ≠ plan.configRevision）；对照腿：
/// 重新 preview（盖新 revision）→ commit 200。
#[tokio::test]
async fn t6_a_config_change_between_preview_and_commit_is_refused_as_stale() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;

    fx.profiles
        .compare_and_set(
            &fx.ctx,
            ConnectionId::new(fx.connection_id.clone()),
            ConfigRevision::new(1),
            ProfilePatch {
                name: Some("renamed-wire-conn".into()),
                ..ProfilePatch::default()
            },
        )
        .await
        .expect("config revision advances");

    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&plan)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("Connection configuration changed"),
        "{body}"
    );
    assert_eq!(fx.driver.execute_with_params_calls(), 0, "{body}");

    // 对照腿：重新 preview 盖新 revision → commit 绿。
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    assert_eq!(plan["configRevision"], json!("2"), "{plan}");
    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&plan)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["affectedRows"], json!(1), "{body}");
}

// ── t7：歧义写拒绝 ──────────────────────────────────────────────────────

/// 驱动应答 affected=2 → 400 `grid.commit.affectedMismatch`（拒绝歧义写），
/// 事务零提交（`commit_calls == 0`——commitTransaction 未达）。
#[tokio::test]
async fn t7_an_ambiguous_write_is_refused_and_the_transaction_never_commits() {
    let mut opts = writable_opts();
    opts.execute_rows_affected = 2;
    let fx = mutation_fixture(false, opts).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&plan)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("grid.commit.affectedMismatch"),
        "{body}"
    );
    assert_eq!(
        fx.driver.commit_calls(),
        0,
        "an ambiguous write never reaches the transaction commit"
    );
}

// ── t8：可见性 ──────────────────────────────────────────────────────────

/// 跨组织 cookie → 404 固定文案（防枚举口径与读端点同一条）；无 cookie →
/// 401（L2 认证链先于一切）。
#[tokio::test]
async fn t8_cross_org_and_anonymous_callers_are_refused() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (status, body) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.other_org.cookie,
        Some(&fx.other_org.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(
        body["message"],
        json!("Connection profile unavailable"),
        "{body}"
    );

    let (status, _) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        "",
        None,
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── t10：会话生命周期（可达断言形，偏离登记见模块文档）────────────────────

/// 幂等重放：同一 plan 原样提交两次都 200（owned-session 每次新开、无
/// 残留——若会话/事务面泄漏，第二次提交的资源面会污染）；提交次数随
/// 之累计为 2（每次提交一个事务，write_exec 的关闭路径在 commit 之后）。
#[tokio::test]
async fn t10_an_owned_session_leaves_no_lingering_state_between_commits() {
    let fx = mutation_fixture(false, writable_opts()).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&fx.connection_id)),
    )
    .await;
    for expected_calls in 1..=2u32 {
        let (status, body) = call(
            &fx.app,
            "POST",
            COMMIT_URI,
            &fx.primary.cookie,
            Some(&fx.primary.csrf),
            Some(commit_body(&plan)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "replay #{expected_calls}: {body}");
        assert_eq!(body["affectedRows"], json!(1), "{body}");
        assert_eq!(
            fx.driver.commit_calls(),
            expected_calls,
            "each commit drives exactly one transaction"
        );
    }
}
