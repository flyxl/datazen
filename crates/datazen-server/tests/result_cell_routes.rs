//! W8 刀4：result-cell 二成对端点（`result-cell-change-previews` /
//! `result-cell-commits`）的集成绿路径与护栏测试
//! （`w8-knife4-result-cell-design.md` §5 测试计划的 t1–t9；探针收窄落在
//! `table_data_routes.rs`——刀4 后域内 fail-closed 面清零，501 探针退役）。
//!
//! 装配沿 `mutation_pair_routes.rs` 的夹具同构（真 SQLite 管理库 +
//! `WireMockFactory` + `TransactionalMockProvider` + 预算桥 +
//! `ServerDriverExecBackend` + 真 `ServerRuntimeLeaseSource` +
//! `ServerTargetRunner`），新增「先开会话再在会话里跑 query」的纪律：
//! 结果绑定（`RuntimeResultBinding`）来自 execute_in_session 的执行台账
//! （共享 sink，跨请求可读），preview / commit 用 `dbSessionId` +
//! `executionId` 指认它——桌面请求体里的进程内 binding 的 server 替换
//! （设计档 §2）。
//!
//! 偏离与裁决登记（如实）：
//!
//! - **t8「驱动零写调用」的可达断言形**：歧义写（affected=2）的拒绝发生
//!   在语句**已跑**之后（affected 数正是驱动报的），而原会话无事务包裹
//!   （事务语义随会话；autocommit 会话上单语句已生效——桌面写路径同权，
//!   文案指示「inspect the target before retrying」）。可达断言是：恰好
//!   **一次**驱动写调用（拒绝后零补偿重试、零追加写）+ 400
//!   `grid.commit.affectedMismatch`。
//! - **只读连接上 preview 也 400**（设计档 §3 把只读预检放进 ①~⑤ 共同
//!   前闸）：result-cell 预览的出口是**可写裁决**，只读连接上它是死答案；
//!   与刀3 变更预览「不设 read_only 闸」不同源（那是命令面预览，不查
//!   可写性），码表同族（`grid.result.writeDenied`）。
//! - **提交主体是 OrgAdmin**（刀3 同一条授权基线发现）：原会话
//!   `execute_in_session` 写命令经 enqueue 的 action 判定归 `Write` →
//!   `(Write, Execution)` 组织闸 + `(Write, Connection)` 目标闸；RBAC
//!   基线里 OrgMember 的 Execution 只有 Read/Execute/Cancel。写授权是
//!   部署授权决策，矩阵调整属产品授权面（刀3 台账已登记的开放问题）。

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
    RuntimeConnectionUseCases, RuntimeResultSink,
};
use datazen::execution::application::requests::{
    CommandCall, ExecuteInSessionRequest, OpenSessionRequest,
};
use datazen::execution::target::NamespaceTarget;
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::id::{ClientInstanceId, ConfigRevision, ConnectionId, IdempotencyKey};
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{
    capabilities, DatabaseDriver, DatabaseDriverFactory, SslMode, Value as DriverValue,
};
use datazen_server::csrf::{self, CsrfSecret};
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::{RuntimeLeaseSource, ServerRuntimeLeaseSource};
use datazen_server::execution::live_source::ServerLiveConnectionSource;
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
/// `mutation_pair_routes.rs::register_login` 同构。
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

/// oneshot 驱动认证链 Router（`mutation_pair_routes.rs::call` 同构）。
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

/// 把 [`MockDriver`] 连同 [`TransactionalMockProvider`] 包成静态工厂
/// （`mutation_pair_routes.rs::WireMockFactory` 同构）。
struct WireMockFactory {
    driver_id: &'static str,
    driver: Arc<MockDriver>,
}

impl DatabaseDriverFactory for WireMockFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
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

/// 进程内驱动的测试 provider（刀3 同构：全方法委托 adapter，事务三方法
/// 覆盖为 MockDriver 真实现）。result-cell 写半边不开关事务（事务语义随
/// 会话），但生产 provider 同构保留装配一致性。
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

/// 会话连续性的测试形（`mutation_pair_routes.rs::FixedLeaseSource` 同构）。
struct FixedLeaseSource(Arc<RuntimeConnectionUseCases>);

#[async_trait::async_trait]
impl RuntimeLeaseSource for FixedLeaseSource {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        Ok(self.0.clone())
    }
}

/// result-cell 二端点的接线夹具（刀3 夹具同构 + `read_only` 参数化）。
struct ResultCellFixture {
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

async fn result_cell_fixture(read_only: bool, opts: MockDriverOptions) -> ResultCellFixture {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w8-result-cell-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    // 提交主体是 OrgAdmin（授权基线见模块文档登记；刀3 同一条发现）。
    let primary = register_login(
        &store,
        "org-1",
        "user-1",
        "sess-w8-result-cell-primary",
        Role::OrgAdmin,
    );
    let other_org = register_login(
        &store,
        "org-2",
        "user-2",
        "sess-w8-result-cell-org2",
        Role::OrgMember,
    );

    let ctx = RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        // HTTP 面的 SessionIdentity 固定铸造 `http`（router.rs 提取器）；
        // 会话 open owner 与借出授权要求同一客户端实例（刀3 夹具同款）。
        ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-w8-result-cell"),
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
            &IdempotencyKey::new("idem-w8-result-cell-profile"),
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
        scope.clone(),
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
    // 刀4 夹具装配（生产 bootstrap 同构）：活连接出向端口适配器吃 backend
    // 的 clone（by_session 反向索引随 backend.open 同帧写入，两份 Arc 必须
    // 指向同一 backend 实例）；租约源接线后 resolve 一次拿当前单元 ——
    // 会话开立 / 借出 / 写半边全部落同一实例（开在 X 借在 Y 的
    // 「session unavailable」是实例分裂，不是产品语义）。
    let live_source = Arc::new(ServerLiveConnectionSource::new(
        backend.clone(),
        connections.clone() as Arc<dyn datazen::connection::domain::catalog::ConnectionRepository>,
        registry.clone(),
        scope.clone(),
    ));
    let real_lease = Arc::new(
        ServerRuntimeLeaseSource::new_with_shared_parts(
            profiles.clone(),
            policy,
            backend,
            sink,
            directory.clone(),
        )
        .with_live_source(live_source),
    );
    let runtime = real_lease.resolve().await.expect("runtime unit resolves");
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
        wapp_assets: None,
        domain_routers: Some(vec![table_data_router(TableDataState {
            store: store.clone(),
            registry,
            runner,
            profiles: profiles.clone(),
            connections,
            directory,
            sessions: runtime.clone(),
            // 写半边与借出的租约源：真租约源贴近生产（单次 resolve 的实例
            // 贯穿提交序列，CM-63 cell 语义；刀3 同款）。
            lease: real_lease,
            schema_cache: Arc::new(datazen::schema::cache::SchemaCache::new()),
        })]),
    })
    .expect("router");

    ResultCellFixture {
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

/// `get_table_schema` 的 `CommandResult.data`（刀3 同款：id 主键 + email）。
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

/// 刀4 的驱动预录：参数化写生效 + affected=1 + 命令面 schema 应答 +
/// 目录列/主键（schema cache 半边）+ 目录表（origin 的 relation kind 确认）
/// + 会话查询的行（`SELECT id, email FROM users` → 一行 `[1, "a@b.c"]`）。
fn result_cell_opts() -> MockDriverOptions {
    let mut opts = MockDriverOptions::default();
    opts.parameterized_writes = true;
    opts.execute_rows_affected = 1;
    opts.command_results.insert(
        "get_table_schema".to_owned(),
        json!({ "schema": schema_payload() }),
    );
    opts.columns = vec![
        datazen_driver_api::ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: true,
        },
        datazen_driver_api::ColumnSchema {
            name: "email".into(),
            data_type: "varchar".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
    ];
    opts.primary_keys = vec!["id".to_owned()];
    opts.tables = vec![datazen_driver_api::TableInfo {
        name: "users".into(),
        schema: None,
        table_type: datazen_driver_api::TableType::Table,
        row_count: None,
    }];
    opts.query_rows = vec![vec![
        Some(DriverValue::Integer(1)),
        Some(DriverValue::String("a@b.c".into())),
    ]];
    opts
}

/// 开会话 → 在会话里跑 `SELECT id, email FROM users`（execute_in_session
/// 产物带 binding）。返回（活句柄, executionId）。
async fn open_result_session(
    fx: &ResultCellFixture,
) -> (datazen::execution::SessionHandle, String) {
    let token = fx
        .sessions
        .issue_submission_token(
            &fx.ctx,
            datazen::execution::IdempotentOperation::OpenSession,
            None,
        )
        .await
        .expect("open token");
    let receipt = fx
        .sessions
        .open_session(
            &fx.ctx,
            OpenSessionRequest {
                initial_target: datazen::execution::ExecutionTarget::new(
                    ConnectionId::new(fx.connection_id.clone()),
                    NamespaceTarget::new(Some("app".to_owned()), None, None, Vec::new()),
                ),
                owner: OwnerRef::ClientSession {
                    client_instance_id: ClientInstanceId::new("http"),
                    purpose: "resultCell".to_owned(),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .expect("session opens");
    let handle = receipt.session.handle.clone();
    let revision = receipt.session.context_revision;
    let token = fx
        .sessions
        .issue_submission_token(
            &fx.ctx,
            datazen::execution::IdempotentOperation::ExecuteInSession,
            Some(&handle),
        )
        .await
        .expect("execute token");
    let receipt = fx
        .sessions
        .execute_in_session(
            &fx.ctx,
            ExecuteInSessionRequest {
                handle: handle.clone(),
                expected_context_revision: Some(revision),
                call: CommandCall {
                    command: "query".to_owned(),
                    input: json!({
                        "sql": "SELECT id, email FROM users",
                        "boundParameters": [],
                    }),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .expect("query accepted");
    loop {
        let view = fx
            .sessions
            .get_execution(&fx.ctx, receipt.execution_id.clone())
            .await
            .expect("execution view");
        if view.state.is_terminal() {
            assert_eq!(
                view.state,
                datazen::execution::ExecutionState::Succeeded,
                "the session query succeeds: {view:?}"
            );
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    (handle, receipt.execution_id.as_str().to_owned())
}

fn preview_body(db_session_id: &str, execution_id: &str) -> Value {
    json!({
        "dbSessionId": db_session_id,
        "executionId": execution_id,
        "statementIndex": 0,
        "rowIndex": 0,
        "column": "email",
        "value": "z@b.c",
        "sql": "SELECT id, email FROM users",
        "resultColumns": ["id", "email"],
    })
}

fn commit_body(db_session_id: &str, execution_id: &str, plan: &Value) -> Value {
    json!({
        "dbSessionId": db_session_id,
        "executionId": execution_id,
        "plan": plan,
        "fingerprint": plan["fingerprint"],
    })
}

const PREVIEW_URI: &str = "/api/v1/table-data/result-cell-change-previews";
const COMMIT_URI: &str = "/api/v1/table-data/result-cell-commits";

// ── t1：绿路径（预览形状钉 + 原会话提交）─────────────────────────────────

/// 开会话 → query → preview → 200：`planId` 非空、`fingerprint` 非空、
/// `configRevision` = profile revision（初始 1，十进制字符串）、
/// `updates[0].sqlTemplate` 含 UPDATE、`rowIdentity` 从产物行的主键列值
/// 构造（`{"id": 1}`）、`table` 回显 origin 关系（catalog 缺席回落 profile
/// database `app`）→ commit → 200 `affectedRows == 1`、operation == "update"
/// 且 rowIdentity 逐字段一致；驱动恰好被参数化写调用一次。
#[tokio::test]
async fn t1_the_green_path_previews_from_artifacts_and_commits_in_the_original_session() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let (status, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(handle.db_session_id.as_str(), &execution_id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert!(
        !plan["planId"].as_str().expect("planId").is_empty(),
        "plan id is minted: {plan}"
    );
    let fingerprint = plan["fingerprint"].as_str().expect("fingerprint");
    assert!(
        !fingerprint.is_empty(),
        "fingerprint rides the plan: {plan}"
    );
    assert_eq!(
        plan["configRevision"],
        json!("1"),
        "the plan stamps the preview-time profile revision (decimal string): {plan}"
    );
    let updates = plan["updates"].as_array().expect("updates");
    assert_eq!(updates.len(), 1, "one update statement: {plan}");
    assert!(
        updates[0]["sqlTemplate"]
            .as_str()
            .expect("sql")
            .contains("UPDATE"),
        "the driver-rendered preview mentions UPDATE: {plan}"
    );
    assert_eq!(
        updates[0]["rowIdentity"],
        json!({"id": 1}),
        "row identity comes from the artifact row's primary-key values: {plan}"
    );
    assert_eq!(
        plan["table"]["table"],
        json!("users"),
        "table echoes: {plan}"
    );
    assert_eq!(plan["table"]["database"], json!("app"), "{plan}");
    assert_eq!(
        plan["table"]["connectionId"],
        json!(fx.connection_id),
        "{plan}"
    );

    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(
            handle.db_session_id.as_str(),
            &execution_id,
            &plan,
        )),
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
}

// ── t2：行定位护栏 ──────────────────────────────────────────────────────

/// rowIndex 越界 → 400 `grid.result.rowNotAddressable`；statementIndex 没有
/// 产物行 → 400 同码；列不在 passthrough 投影（不在结果列里）→ 400
/// `grid.result.writeDenied`。三条都断言驱动零写调用（预览不执行语句）。
#[tokio::test]
async fn t2_row_location_guards_refuse_out_of_range_rows_and_non_passthrough_columns() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();

    let mut body = preview_body(&session_id, &execution_id);
    body["rowIndex"] = json!(5);
    let (status, answer) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .expect("message")
            .contains("grid.result.rowNotAddressable"),
        "{answer}"
    );

    let mut body = preview_body(&session_id, &execution_id);
    body["statementIndex"] = json!(3);
    let (status, answer) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .expect("message")
            .contains("grid.result.rowNotAddressable"),
        "{answer}"
    );

    let mut body = preview_body(&session_id, &execution_id);
    body["column"] = json!("nickname");
    let (status, answer) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .expect("message")
            .contains("grid.result.writeDenied"),
        "{answer}"
    );
    assert_eq!(
        fx.driver.execute_with_params_calls(),
        0,
        "the preview never executes a statement"
    );
}

// ── t3：只读连接（两条端点同闸）──────────────────────────────────────────

/// `read_only` 连接：preview 400 `grid.result.writeDenied`（设计档 §3 前闸
/// ③：result-cell 预览的出口是可写裁决，只读连接上是死答案）；commit 400
/// 同码（前闸先于共享层五道闸）；驱动零写调用。
#[tokio::test]
async fn t3_a_read_only_connection_refuses_both_halves_with_the_result_code() {
    let fx = result_cell_fixture(true, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();

    let (status, answer) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&session_id, &execution_id)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .expect("message")
            .contains("grid.result.writeDenied"),
        "{answer}"
    );

    let plan = json!({
        "planId": "plan-x",
        "fingerprint": "fingerprint-x",
        "configRevision": "1",
        "table": {
            "connectionId": fx.connection_id,
            "dbSessionId": session_id,
            "driverType": "mockdb",
            "database": "app",
            "schema": null,
            "table": "users",
        },
        "updates": [],
        "deletes": [],
        "inserts": [],
        "warnings": [],
    });
    let (status, answer) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&session_id, &execution_id, &plan)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .expect("message")
            .contains("grid.result.writeDenied"),
        "{answer}"
    );
    assert_eq!(
        fx.driver.execute_with_params_calls(),
        0,
        "the read-only gate precedes the write path"
    );
}

// ── t4：binding 复验 ────────────────────────────────────────────────────

/// 陌生 `executionId` → 404 固定文案（防枚举口径与四读同一条）；binding
/// 漂移（executionId 的原会话与请求指认的 `dbSessionId` 不是同一个会话）→
/// 409 `contextConflict`（会话尚在、binding 比对失败——诚实区分）。
#[tokio::test]
async fn t4_binding_reverification_refuses_unknown_executions_and_drifted_sessions() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();

    // 腿 1：陌生 executionId → 404 固定文案。
    let (status, answer) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&session_id, "exec-never-was")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");
    assert_eq!(answer["message"], json!(NOT_VISIBLE_SENTINEL), "{answer}");

    // 腿 2：第二个会话（无 query 产物）+ 第一个会话的 executionId →
    // binding 比对失败 → 409。
    let token = fx
        .sessions
        .issue_submission_token(
            &fx.ctx,
            datazen::execution::IdempotentOperation::OpenSession,
            None,
        )
        .await
        .expect("open token");
    let second = fx
        .sessions
        .open_session(
            &fx.ctx,
            OpenSessionRequest {
                initial_target: datazen::execution::ExecutionTarget::new(
                    ConnectionId::new(fx.connection_id.clone()),
                    NamespaceTarget::new(Some("app".to_owned()), None, None, Vec::new()),
                ),
                owner: OwnerRef::ClientSession {
                    client_instance_id: ClientInstanceId::new("http"),
                    purpose: "resultCell".to_owned(),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .expect("second session opens");
    let second_id = second.session.handle.db_session_id.as_str().to_owned();
    let (status, answer) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&second_id, &execution_id)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(answer["code"], json!("contextConflict"), "{answer}");
}

// ── t5：manualHandle 明拒 ───────────────────────────────────────────────

/// 体带 `manualHandle` → 501 `capabilityUnsupported`（拒绝先于一切执行
/// 触碰：驱动零写调用），文案声明写恒在原会话执行；缺省路径即 t1 绿路径。
#[tokio::test]
async fn t5_a_manual_handle_is_refused_before_any_execution_touch() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(handle.db_session_id.as_str(), &execution_id)),
    )
    .await;
    assert_eq!(plan["planId"].as_str().map(str::len), Some(36), "{plan}");
    let mut body = commit_body(handle.db_session_id.as_str(), &execution_id, &plan);
    body["manualHandle"] =
        json!({"dbSessionId": handle.db_session_id.as_str(), "runtimeEpoch": "1"});
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
    assert!(message.contains("original-session"), "{answer}");
    assert_eq!(
        fx.driver.execute_with_params_calls(),
        0,
        "the refusal precedes every execution touch"
    );
}

// ── t6：配置漂移（原会话模型的 INV-02 语义）────────────────────────────
//
/// preview 之后 profile revision 前进（`compare_and_set` 1→2）→ commit 409
/// `configRevisionMismatch`：**借出闸先于一切**——原会话模型下配置漂移把
/// 会话整体判失效（INV-02：变更时该连接上所有会话都被判失效，SessionView
/// 文档），客户端必须重跑 query 拿新会话新结果。这与刀3 t6 的 400
/// stalePlan（owned-session 每次新开会话，CAS 在共享层）不同层，但同一条
/// 产品规则的两端：漂移后旧资源不可写。驱动零写调用。
#[tokio::test]
async fn t6_a_config_change_invalidates_the_original_session_before_the_write() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&session_id, &execution_id)),
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
        Some(commit_body(&session_id, &execution_id, &plan)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], json!("configRevisionMismatch"), "{body}");
    assert_eq!(
        fx.driver.execute_with_params_calls(),
        0,
        "the drift gate precedes the write path"
    );
}

// ── t7：可见性 ──────────────────────────────────────────────────────────

/// 跨组织 cookie → 404 固定文案（活句柄借出的防枚举裁决）；无 cookie →
/// 401（L2 认证链先于一切）。
#[tokio::test]
async fn t7_cross_org_and_anonymous_callers_are_refused() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();
    let (status, body) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.other_org.cookie,
        Some(&fx.other_org.csrf),
        Some(preview_body(&session_id, &execution_id)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], json!(NOT_VISIBLE_SENTINEL), "{body}");

    let (status, _) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        "",
        None,
        Some(preview_body(&session_id, &execution_id)),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── t8：歧义写拒绝（可达断言形，偏离登记见模块文档）────────────────────

/// 驱动应答 affected=2 → 400 `grid.commit.affectedMismatch`；断言恰好一次
/// 驱动写调用（拒绝发生一次、零补偿重试零追加写——原会话 autocommit 上
/// 歧义写已生效，与桌面同权，文案指示检查目标）。
#[tokio::test]
async fn t8_an_ambiguous_write_is_refused_after_exactly_one_driver_call() {
    let mut opts = result_cell_opts();
    opts.execute_rows_affected = 2;
    let fx = result_cell_fixture(false, opts).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&session_id, &execution_id)),
    )
    .await;
    let (status, body) = call(
        &fx.app,
        "POST",
        COMMIT_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(commit_body(&session_id, &execution_id, &plan)),
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
        fx.driver.execute_with_params_calls(),
        1,
        "exactly one write ran (the ambiguous one); the refusal never retries or writes again"
    );
}

// ── t9：幂等重放全链 + 原会话存活 ────────────────────────────────────────

/// 同一 plan 原样提交两次都 200（原会话不被提交半边关闭——第二次提交仍落
/// 同一会话，驱动写调用累计 2）；提交间 get_session 仍活 + 结果仍可再预览
/// （binding 复验仍过：execution 台账与原会话都还在）。
#[tokio::test]
async fn t9_the_original_session_survives_commits_and_replays_stay_green() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();
    let (_, plan) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&session_id, &execution_id)),
    )
    .await;
    for expected_calls in 1..=2u32 {
        let (status, body) = call(
            &fx.app,
            "POST",
            COMMIT_URI,
            &fx.primary.cookie,
            Some(&fx.primary.csrf),
            Some(commit_body(&session_id, &execution_id, &plan)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "replay #{expected_calls}: {body}");
        assert_eq!(body["affectedRows"], json!(1), "{body}");
        assert_eq!(
            fx.driver.execute_with_params_calls(),
            expected_calls,
            "each commit drives exactly one write in the original session"
        );
        // 提交半边不关原会话：get_session 仍活（客户端的活会话）。
        let view = fx
            .sessions
            .get_session(&fx.ctx, handle.clone())
            .await
            .expect("the original session stays alive");
        assert_eq!(
            view.handle.db_session_id.as_str(),
            session_id,
            "the same session serves the next edit"
        );
    }
    // 结果网格仍可编辑：第二次提交后再预览一次（binding 复验 + origin
    // 重读 + 行定位全链再绿）。
    let (status, _) = call(
        &fx.app,
        "POST",
        PREVIEW_URI,
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(preview_body(&session_id, &execution_id)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the result is still editable");
}

#[tokio::test]
async fn probe_origin_endpoint() {
    let fx = result_cell_fixture(false, result_cell_opts()).await;
    let (handle, _execution_id) = open_result_session(&fx).await;
    let session_id = handle.db_session_id.as_str().to_owned();
    let (status, body) = call(
        &fx.app,
        "POST",
        "/api/v1/table-data/result-origins",
        &fx.primary.cookie,
        Some(&fx.primary.csrf),
        Some(json!({
            "db_session_id": session_id,
            "database": null,
            "schema": null,
            "sql": "SELECT id, email FROM users",
            "result_columns": ["id", "email"],
            "table": null,
        })),
    )
    .await;
    println!("PROBE origin: {status} {body}");
}
