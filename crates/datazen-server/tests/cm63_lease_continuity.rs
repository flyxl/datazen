//! CM-63：跨 resolve 会话连续性（刀0，`w8-mutation-knife0-1-design.md` §1）。
//!
//! 病灶（修复前）：主线 `ServerRuntimeLeaseSource::resolve()` 每调用现构造
//! 新的 `RuntimeConnectionUseCases`——sessions/tokens/keyring/receipts 全是
//! 实例私有账，请求 A 开的会话对请求 B 必然 `SessionNotFound`；测试夹具
//! （`table_data_routes.rs` 的 `FixedLeaseSource`）此前靠「单例租约源」绕开
//! 这份失联。刀0 = 把夹具已经采用的语义落进生产租约源：租约源持「当前
//! runtime 单元，直至吊销」。
//!
//! 本文件断言全部跨「两个独立的 `LeaseConnectionUseCases` 实例」（每个
//! 用例调用都走一次真实 `ServerRuntimeLeaseSource::resolve()`，等价于两个
//! 独立 HTTP 请求的 resolve）：
//!
//! - **t1 会话连续**：A 开会话 → B `get_session` → `Ok`，connection_id /
//!   config_revision / namespace 一致。
//! - **t2 关闭幂等连续**：A 开 → B close → C 再 close 回放同一回执
//!   （tombstone 跨 resolve 可见）。
//! - **t3 令牌签发/消费连续**：A issue（ExecuteInSession）→ B
//!   `execute_in_session` 消费成功（tokens map + keyring 同实例的双层验证）。
//! - **t4 执行视图跨 resolve**：A 提交 target 执行 → B `get_execution` /
//!   `read_artifact_chunk` 正常（共享 sink 旧行为已好，钉住）。
//! - **t5 吊销语义**：`revoke()` 后 resolve 返回新实例，旧 dbSessionId 在
//!   新实例上 `get_session_by_id` → `SessionNotFound`（「旧会话按 SessionLost
//!   语义失效」的可执行钉）。
//! - **t6 estimate 端到端**：HTTP 面走**真** lease source（不再是
//!   FixedLeaseSource 的一条腿）：POST sessions 开会话 → POST
//!   `/api/v1/table-data/row-count-estimates` → 200。
//! - **t7 既有 fail-closed 探针不回归**：不改 `table_data_routes.rs` 一字，
//!   全量跑既有测试确认（不在本文件）。
//!
//! 装配沿 `table_data_routes.rs` 的 `wired_fixture` 模式：真 SQLite 管理库 +
//! `WireMockFactory`（MockDriver + `LegacyResourceAdapter` 资源提供者）+
//! `ServerDriverExecBackend` + **真** `ServerRuntimeLeaseSource` +
//! `LeaseConnectionUseCases`；共享层（`datazen` crate）零改动。

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request as HttpRequest, StatusCode};
use tower::ServiceExt;

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
// 用例方法与租约端口都是 trait 面——方法调用要求 trait 在作用域内。
use datazen::execution::application::ConnectionUseCases as _;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::idempotency::IdempotentOperation;
use datazen::execution::application::requests::{
    CloseMode, CloseSessionRequest, CommandCall, ExecuteAtTargetRequest, ExecuteInSessionRequest,
    OpenSessionRequest,
};
use datazen::execution::target::NamespaceTarget;
use datazen::execution::ExecutionTarget;
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::ApiErrorCode;
use datazen::shared::id::{
    ClientInstanceId, ConnectionId, Counter, EditorSessionId, ExecutionId, IdempotencyKey,
    OrganizationId, PrincipalId, RequestId,
};
use datazen_driver_api::capabilities;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{DatabaseDriver, DatabaseDriverFactory, SslMode, Value};
use datazen_server::execution::lease::{RuntimeLeaseSource as _, ServerRuntimeLeaseSource};
use datazen_server::execution::lease_use_cases::LeaseConnectionUseCases;
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::{AdminStore, ServerConnections, ServerProfiles};

const SERVER_SECRET: &[u8] = b"cm63-lease-continuity-secret-32b!";

// ── MockDriver 装配（table_data_routes.rs WireMockFactory 同款）──────────────

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
            "cm63-test",
            1,
            shape,
            capabilities,
        )))
    }
}

// ── 核心夹具（t1–t5：进程内用例面，无 HTTP 路由）────────────────────────────

struct CoreFixture {
    _dir: PathBuf,
    /// 管理库句柄（保持行存活）。
    _store: Arc<AdminStore>,
    /// 真 `ServerRuntimeLeaseSource`——被测件本体。
    lease: Arc<ServerRuntimeLeaseSource>,
    connection_id: String,
    ctx: RequestContext,
}

/// 组装执行域三件套 + 真租约源（bootstrap 同构；行/库/驱动全真）。
async fn core_fixture(opts: MockDriverOptions) -> CoreFixture {
    use datazen::execution::application::orchestration::RuntimeResultSink;
    use datazen::execution::runtime::directory::InMemorySessionDirectory;
    use datazen_server::execution::budget::ServerBudgetCoordinator;
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
    use datazen_server::execution::publisher::execution_result_publisher;
    use datazen_server::execution::secret_provider::ServerSecretVault;

    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-cm63-core-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    // 授权基线：org-1/user-1 是 OrgMember（进程内用例面无登录会话链）。
    PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership");

    let ctx = RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("cm63-client"),
        RequestId::new("req-cm63-core"),
        None,
    );

    // 驱动注册表：懒形 + 测试驱动 + 测试工厂（生产缺省形态零依赖边的对偶面）。
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
        ServerSecretVault::new(store.clone(), b"cm63-secret".to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    // profile 行 + 连接目录行（同一连接 id 两半都齐，闸序可达驱动层）。
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("cm63-conn", "mockdb"),
            &IdempotencyKey::new("idem-cm63-profile"),
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
                name: "cm63 conn".into(),
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
    let sink = RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(
            PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine"),
        ),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let directory =
        Arc::new(InMemorySessionDirectory::new());
    let lease = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles.clone(),
        policy,
        backend,
        sink,
        directory.clone(),
    ));

    CoreFixture {
        _dir: dir,
        _store: store,
        lease,
        connection_id,
        ctx,
    }
}

/// 两个独立用例面实例（等价两个独立 HTTP 请求的 resolve 链）。
fn lease_cases(fx: &CoreFixture) -> Arc<LeaseConnectionUseCases> {
    Arc::new(LeaseConnectionUseCases::new(fx.lease.clone()))
}

fn editor_target(connection_id: &str) -> ExecutionTarget {
    ExecutionTarget::new(
        ConnectionId::new(connection_id.to_owned()),
        NamespaceTarget::new(Some("app".to_owned()), None, None, Vec::new()),
    )
}

/// 经真 `open_session` 用例开立会话（预签发幂等令牌，用例层核验）。
async fn open_session(
    fx: &CoreFixture,
    cases: &LeaseConnectionUseCases,
    editor_session: &str,
) -> datazen::execution::OpenSessionReceipt {
    let token = cases
        .issue_submission_token(&fx.ctx, IdempotentOperation::OpenSession, None)
        .await
        .expect("submission token issued");
    cases
        .open_session(
            &fx.ctx,
            OpenSessionRequest::new(
                editor_target(&fx.connection_id),
                OwnerRef::Editor {
                    client_instance_id: fx.ctx.client_instance_id.clone(),
                    editor_session_id: EditorSessionId::new(editor_session),
                },
                token.idempotency_key,
            ),
        )
        .await
        .expect("session opens through the real use case")
}

/// 轮询执行到终态（`wait_for_execution` 是具体类型方法，不在
/// `ConnectionUseCases` 端口上——轮询走端口面 `get_execution`，等价独立请求）。
async fn wait_terminal(
    cases: &LeaseConnectionUseCases,
    ctx: &RequestContext,
    execution_id: ExecutionId,
) -> datazen::execution::ExecutionView {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let view = cases
            .get_execution(ctx, execution_id.clone())
            .await
            .expect("execution view is readable");
        if view.state.is_terminal() {
            return view;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "execution never reached a terminal state: {view:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

// ── t1 会话连续 ─────────────────────────────────────────────────────────────

/// A 开的会话，B 查得到：connection_id / config_revision / namespace 一致
/// （修复前：B resolve 出新实例，`session_entry` 必然 `SessionNotFound`）。
#[tokio::test]
async fn t1_a_session_opened_on_one_resolve_is_visible_through_another() {
    let fx = core_fixture(MockDriverOptions::default()).await;
    let a = lease_cases(&fx);
    let b = lease_cases(&fx);

    let receipt = open_session(&fx, a.as_ref(), "cm63-editor-1").await;
    let view = b
        .get_session(&fx.ctx, receipt.session.handle.clone())
        .await
        .expect("the session resolves across independent use-case instances");

    assert_eq!(view.handle, receipt.session.handle);
    assert_eq!(view.connection_id, receipt.session.connection_id);
    assert_eq!(view.config_revision, receipt.session.config_revision);
    assert_eq!(
        view.observed_context.namespace, receipt.session.observed_context.namespace,
        "the observed namespace survives the resolve boundary"
    );
}

// ── t2 关闭幂等连续 ─────────────────────────────────────────────────────────

/// A 开 → B close → C 再 close 回放同一回执（§7.5 幂等关闭：tombstone 跨
/// resolve 可见，重试不会开出第二个会话也不会改写回执）。
#[tokio::test]
async fn t2_an_idempotent_close_replays_the_same_receipt_across_resolves() {
    let fx = core_fixture(MockDriverOptions::default()).await;
    let a = lease_cases(&fx);
    let b = lease_cases(&fx);
    let c = lease_cases(&fx);

    let receipt = open_session(&fx, a.as_ref(), "cm63-editor-2").await;
    let request = CloseSessionRequest::new(receipt.session.handle.clone(), CloseMode::RollbackAndClose);

    let first = b
        .close_session(&fx.ctx, request.clone())
        .await
        .expect("the first close settles");
    assert_eq!(first.db_session_id, receipt.session.handle.db_session_id);

    let replay = c
        .close_session(&fx.ctx, request)
        .await
        .expect("the replayed close is answered by the tombstone");
    assert_eq!(
        replay, first,
        "the idempotent close replays the original receipt across resolves"
    );

    // 关闭后的 get_session 按既有 SessionLost 语义失效（三实例同则）。
    let error = a
        .get_session(&fx.ctx, receipt.session.handle)
        .await
        .expect_err("a closed session is SessionLost on any resolve");
    assert_eq!(error.code, ApiErrorCode::SessionLost);
}

// ── t3 令牌签发/消费连续 ────────────────────────────────────────────────────

/// A 签发 ExecuteInSession 令牌 → B 消费成功：tokens map 与 keyring 必须
/// 同实例（双层验证——修复前令牌在新实例上「不在账 + 验不过签」双失联）。
#[tokio::test]
async fn t3_a_token_issued_on_one_resolve_is_consumable_on_another() {
    let fx = core_fixture(MockDriverOptions::default()).await;
    let a = lease_cases(&fx);
    let b = lease_cases(&fx);

    let opened = open_session(&fx, a.as_ref(), "cm63-editor-3").await;
    let handle = opened.session.handle.clone();
    let token = a
        .issue_submission_token(&fx.ctx, IdempotentOperation::ExecuteInSession, Some(&handle))
        .await
        .expect("in-session token issued");

    let receipt = b
        .execute_in_session(
            &fx.ctx,
            ExecuteInSessionRequest::new(
                handle,
                CommandCall::new("query", serde_json::json!({"sql": "SELECT 1"})),
                token.idempotency_key,
            ),
        )
        .await
        .expect("the token issued on A is consumed on B without loss");

    // 消费不只是「过闸」：执行真的在会话内跑完并落共享 sink。
    let view = wait_terminal(b.as_ref(), &fx.ctx, receipt.execution_id).await;
    assert_eq!(view.state, datazen::execution::ExecutionState::Succeeded);
    assert!(
        !view.artifact_ids.is_empty(),
        "the in-session run published its result into the shared sink"
    );
}

// ── t4 执行视图跨 resolve ───────────────────────────────────────────────────

/// A 提交 target 执行 → B get_execution / read_artifact_chunk 正常
/// （读面走共享 sink，旧行为已好——钉住，防刀0 改动回归）。
#[tokio::test]
async fn t4_an_execution_submitted_on_one_resolve_is_readable_on_another() {
    let mut opts = MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(Value::Integer(7))]];
    let fx = core_fixture(opts).await;
    let a = lease_cases(&fx);
    let b = lease_cases(&fx);

    // 期望配置版本从 profile 事实现读（与生产客户端同口径）。
    let profiles = a.list_connections(&fx.ctx).await.expect("profiles list");
    let revision = profiles
        .iter()
        .find(|p| p.connection_id.as_str() == fx.connection_id)
        .expect("the fixture profile is visible")
        .config_revision;

    let token = a
        .issue_submission_token(&fx.ctx, IdempotentOperation::ExecuteAtTarget, None)
        .await
        .expect("target token issued");
    let receipt = a
        .execute_at_target(
            &fx.ctx,
            ExecuteAtTargetRequest::new(
                editor_target(&fx.connection_id),
                revision,
                CommandCall::new("query", serde_json::json!({"sql": "SELECT 7"})),
                token.idempotency_key,
            ),
        )
        .await
        .expect("the target execution is admitted");

    let view = wait_terminal(b.as_ref(), &fx.ctx, receipt.execution_id).await;
    assert_eq!(view.state, datazen::execution::ExecutionState::Succeeded);
    let artifact_id = view
        .artifact_ids
        .first()
        .expect("a rows-producing execution publishes an artifact")
        .clone();

    // B 的 resolve 链读产物块（共享 sink 的跨 resolve 可见性）。产物形状：
    // target 执行的行集走语句输出（`kind=rows`），块 `rows` 列为空
    // （`publish_statement_output` 形——对照 t3 会话内执行落 `rows` 列）。
    let runtime = fx.lease.resolve().await.expect("lease resolves");
    let chunk = runtime
        .read_artifact_chunk(&fx.ctx, artifact_id, Counter::new(0))
        .await
        .expect("the artifact chunk is readable across resolves");
    assert_eq!(chunk.rows.len(), 0, "{chunk:?}");
    let output = chunk
        .output
        .expect("the target execution published a statement output");
    assert_eq!(output["kind"], "rows", "{output}");
    assert_eq!(output["rows"], serde_json::json!([[7]]), "{output}");
}

// ── t5 吊销语义 ─────────────────────────────────────────────────────────────

/// `revoke()` 后 resolve 返回新实例；旧 dbSessionId 在新实例上
/// `get_session_by_id` → `SessionNotFound`（旧实例整体作废、旧会话按既有
/// SessionLost 语义失效的可执行钉）。
#[tokio::test]
async fn t5_revocation_replaces_the_runtime_and_orphans_its_sessions() {
    let fx = core_fixture(MockDriverOptions::default()).await;
    let a = lease_cases(&fx);

    let before = fx.lease.resolve().await.expect("lease resolves");
    let opened = open_session(&fx, a.as_ref(), "cm63-editor-5").await;
    let db_session_id = opened.session.handle.db_session_id.as_str().to_owned();

    fx.lease.revoke();
    let after = fx.lease.resolve().await.expect("lease resolves after revoke");
    assert!(
        !Arc::ptr_eq(&before, &after),
        "the revocation replaces the current runtime unit"
    );

    let error = after
        .get_session_by_id(&fx.ctx, &db_session_id)
        .await
        .expect_err("the orphaned session is unreachable on the new runtime");
    assert_eq!(error.code, ApiErrorCode::SessionNotFound);
}

// ── t6 estimate 端到端（HTTP 面，真 lease source）───────────────────────────

struct HttpFixture {
    _dir: PathBuf,
    _store: Arc<AdminStore>,
    app: axum::Router,
    connection_id: String,
    /// 真 cookie/CSRF 头（L2–L4 全链，http_fixture 落库的真实登录会话）。
    cookie: String,
    csrf: String,
    /// 与 HTTP `SessionIdentity` 同一主体（org-1/user-1/client `http`）——
    /// 令牌签发面必须与消费面同一主体上下文。
    ctx: RequestContext,
    /// 令牌预签发用例面（同一真租约源）。
    cases: Arc<LeaseConnectionUseCases>,
}

/// HTTP 全链夹具：真路由（sessions 域 + tableData 域）+ 真
/// `ServerRuntimeLeaseSource`——**没有** FixedLeaseSource 的一条腿，租约
/// 连续性由刀0 的生产语义承担。
async fn http_fixture(count_estimate_rows: i64) -> HttpFixture {
    use datazen::execution::application::orchestration::RuntimeResultSink;
    use datazen::execution::runtime::directory::InMemorySessionDirectory;
    use datazen_server::csrf::{self, CsrfSecret};
    use datazen_server::execution::budget::ServerBudgetCoordinator;
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
    use datazen_server::execution::publisher::execution_result_publisher;
    use datazen_server::execution::resource_host::ServerResourceExecutionHost;
    use datazen_server::execution::secret_provider::ServerSecretVault;
    use datazen_server::execution::target_runner::ServerTargetRunner;
    use datazen_server::http::domains::sessions::{sessions_router, SessionsState};
    use datazen_server::http::domains::table_data::{table_data_router, TableDataState};
    use datazen_server::http::router::{build_router_with_auth, AuthDeps};
    use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
    use datazen_server::storage::LoginSessions;
    use datazen_server::storage::ServerSettingsStore;

    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-cm63-http-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    // 真 membership 行 + 真登录会话行 + 真 CSRF 令牌（L2–L4 全链；真实时钟戳，
    // g1 冻结项先例——硬编码日期会随真实时间腐烂成 401）。
    PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership");
    let now = chrono::Utc::now();
    let rfc3339 = |value: chrono::DateTime<chrono::Utc>| {
        value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    LoginSessions::new(store.clone())
        .create(NewLoginSession {
            organization_id: "org-1".to_owned(),
            session_id: "sess-cm63-http".to_owned(),
            principal_id: "user-1".to_owned(),
            auth_time: rfc3339(now),
            issued_at: rfc3339(now),
            absolute_expires_at: rfc3339(now + chrono::Duration::hours(24)),
            idle_expires_at: rfc3339(now + chrono::Duration::hours(8)),
            permission_version: 1,
            client_binding: "ua|client-1".to_owned(),
            mode: LoginSessionMode::Cookie,
            native_secret_hash: None,
        })
        .expect("live login session");
    let csrf_secret = CsrfSecret::new(SERVER_SECRET.to_vec()).expect("csrf secret");
    let csrf_token = csrf::mint(&csrf_secret, "sess-cm63-http").expect("mint csrf token");
    let cookie = format!("__Host-dz_session=sess-cm63-http; __Host-dz_csrf={csrf_token}");

    let ctx = RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        // HTTP 面的 SessionIdentity 固定铸造 `http`（router.rs 提取器）。
        ClientInstanceId::new("http"),
        RequestId::new("req-cm63-http"),
        None,
    );

    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let mut opts = MockDriverOptions::default();
    opts.count_estimate_rows = Some(count_estimate_rows);
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
        ServerSecretVault::new(store.clone(), b"cm63-secret".to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("cm63-http-conn", "mockdb"),
            &IdempotencyKey::new("idem-cm63-http-profile"),
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
                name: "cm63 http conn".into(),
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
    let sink = RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let execution_policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(
            PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine"),
        ),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let directory = Arc::new(InMemorySessionDirectory::new());
    // 真 lease source：sessions 域 / tableData 域 / runner / host 全部同源。
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
        domain_routers: Some(vec![
            sessions_router(SessionsState {
                sessions: cases.clone(),
                directory: directory_face.clone(),
            }),
            table_data_router(TableDataState {
                store: store.clone(),
                registry,
                runner,
                profiles,
                connections,
                directory: directory_face,
                sessions: cases.clone(),
            }),
        ]),
    })
    .expect("router");

    HttpFixture {
        _dir: dir,
        _store: store,
        app,
        connection_id,
        cookie,
        csrf: csrf_token,
        ctx,
        cases,
    }
}

/// oneshot 驱动认证链 Router（`table_data_routes.rs` 的 `call` 同款）。
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
        builder = builder.header(datazen_server::csrf::CSRF_HEADER, token);
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

/// HTTP 面 estimate 端到端：POST /api/v1/sessions 开会话（真租约源的
/// resolve 链 #1）→ POST /api/v1/table-data/row-count-estimates（resolve
/// 链 #2+：可见性闸 → get_session → 会话内估算）→ 200。修复前 get_session
/// 在生产口径上必然 `SessionNotFound`（夹具靠 FixedLeaseSource 绕开），刀0
/// 之后真租约源承担连续性。
#[tokio::test]
async fn t6_the_http_estimate_face_runs_on_the_real_lease_source() {
    let fx = http_fixture(12345).await;

    // 幂等令牌经同一用例面预签发（sessions 域 openSession 的令牌列 ✓）。
    let token = fx
        .cases
        .issue_submission_token(&fx.ctx, IdempotentOperation::OpenSession, None)
        .await
        .expect("submission token issued");
    let idempotency_key = token.idempotency_key.as_str().to_owned();

    let (status, opened) = call(
        &fx.app,
        "POST",
        "/api/v1/sessions",
        // 真 cookie/CSRF（http_fixture 落库的真实登录会话）。
        &fx.cookie,
        Some(&fx.csrf),
        Some(serde_json::json!({
            "submissionToken": { "idempotencyKey": idempotency_key },
            "idempotencyKey": idempotency_key,
            "initialTarget": {
                "connectionId": fx.connection_id,
                "namespace": { "database": "app", "catalog": null, "schema": null, "path": [] },
            },
            "owner": {
                "kind": "editor",
                "clientInstanceId": "http",
                "editorSessionId": "cm63-http-editor",
            },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let db_session_id = opened["session"]["handle"]["dbSessionId"]
        .as_str()
        .expect("the open receipt carries the stable handle half")
        .to_owned();

    let (status, estimate) = call(
        &fx.app,
        "POST",
        "/api/v1/table-data/row-count-estimates",
        &fx.cookie,
        Some(&fx.csrf),
        Some(serde_json::json!({
            "dbSessionId": db_session_id,
            "table": "users",
            "database": "app",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{estimate}");
    assert_eq!(
        estimate,
        serde_json::json!(12345),
        "the estimate rides the wire through the real lease source: {estimate}"
    );
}
