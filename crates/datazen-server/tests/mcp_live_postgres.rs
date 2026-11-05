//! 真方言腿（#[ignore] 环境侧验收）：postgres 上的同款 MCP 绿路径。
//!
//! 本文件是 `mcp_green_path.rs`（MockDriver 常态 CI 腿）的真方言镜像：
//! 同一生产装配（profile → driver → 命令目录 → 执行闸序 + 有状态会话三
//! 工具），驱动从 inventory 解析真 `datazen-driver-postgres`（`driver-postgres`
//! feature 链接闭包），目标是一台**真实 PostgreSQL**。不在常态 CI 跑——
//! 测试恒 `#[ignore]`，且连接事实从环境变量取、缺省即早退。
//!
//! ## 启用方式（run-live）
//!
//! ```text
//! export DATAZEN_LIVE_PG_HOST=127.0.0.1      # 必填，缺省即跳过
//! export DATAZEN_LIVE_PG_PORT=5432           # 可选，缺省 5432
//! export DATAZEN_LIVE_PG_DATABASE=app        # 可选，缺省 app
//! export DATAZEN_LIVE_PG_USER=app            # 可选，缺省 app
//! export DATAZEN_LIVE_PG_PASSWORD=app        # 可选，缺省 app
//! cargo test -p datazen-server --features mcp,driver-postgres \
//!   --test mcp_live_postgres -- --ignored --nocapture
//! ```
//!
//! 常态 CI：`[[test]] required-features = ["mcp", "driver-postgres"]`——不开
//! feature 连编译都不发生；开了 feature（CI 全 feature 腿）也因 `#[ignore]`
//! 跳过。green_options/MockDriver 与本文件无关：这里的「驱动可用」事实来自
//! 真链接闭包，连接/会话事实来自真 PostgreSQL。

use std::sync::Arc;

use serde_json::{json, Value};

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::orchestration::{
    RuntimeConnectionUseCases, RuntimeResultSink, SystemRuntimeClock,
};
use datazen::mcp::application::tools::{RedactedConnectionSource, WorkflowCatalog};
use datazen::settings::ExecutionSettings;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, IdempotencyKey, OrganizationId, PrincipalId, RequestId};
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::{RuntimeLeaseSource, ServerRuntimeLeaseSource};
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::mcp::http::{CallIdentity, McpBackend};
use datazen_server::mcp::runtime::ModulesBackend;
use datazen_server::mcp::stateless_exec::{
    EmptyWorkflowCatalog, ServerRedactedConnections, StatelessToolExec, TargetMachine,
};
use datazen_server::mcp::wire::ToolCallError;
use datazen_server::policy::{PolicyService as ServerPolicyEngine, Role};
use datazen_server::storage::{AdminStore, ServerConnections, ServerProfiles, ServerSettingsStore};

const SERVER_SECRET: &[u8] = b"mcp-live-postgres-secret-32b!";

/// 连接事实（环境变量；HOST 缺省 = 环境未准备 → 测试早退）。
struct LivePg {
    host: String,
    port: u16,
    database: String,
    user: String,
    password: String,
}

fn live_pg() -> Option<LivePg> {
    let host = std::env::var("DATAZEN_LIVE_PG_HOST").ok()?;
    Some(LivePg {
        host,
        port: std::env::var("DATAZEN_LIVE_PG_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5432),
        database: std::env::var("DATAZEN_LIVE_PG_DATABASE").unwrap_or_else(|_| "app".into()),
        user: std::env::var("DATAZEN_LIVE_PG_USER").unwrap_or_else(|_| "app".into()),
        password: std::env::var("DATAZEN_LIVE_PG_PASSWORD").unwrap_or_else(|_| "app".into()),
    })
}

/// 会话连续性测试形（`mcp_green_path.rs` 同构；g1 CM-63 先例）。
struct FixedLeaseSource(Arc<RuntimeConnectionUseCases>);

#[async_trait::async_trait]
impl RuntimeLeaseSource for FixedLeaseSource {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        Ok(self.0.clone())
    }
}

struct LiveFixture {
    backend: Arc<ModulesBackend>,
    connection_id: String,
    database: String,
    _store: Arc<AdminStore>,
}

/// 生产装配 + 真 postgres 驱动（inventory 懒形注册表）。
async fn live_fixture(pg: &LivePg) -> LiveFixture {
    let store = Arc::new(AdminStore::open_in_memory().expect("admin store"));
    store.migrate().expect("migrations apply v1..=v9");

    let engine = Arc::new(
        ServerPolicyEngine::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine"),
    );
    engine
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership");

    // 真 registry：inventory 懒形（bootstrap 同款）。driver-postgres feature
    // 链接闭包内，`get("postgres")` 经 inventory 工厂解析真驱动。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    let ctx = RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("test"),
        RequestId::new("req-mcp-live-pg"),
        None,
    );
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("mcp-live-pg", "postgres"),
            &IdempotencyKey::new("idem-mcp-live-pg-profile"),
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
                name: "mcp live pg".into(),
                database_type: "postgres".into(),
                host: Some(pg.host.clone()),
                port: Some(pg.port),
                database: Some(pg.database.clone()),
                schema: None,
                username: Some(pg.user.clone()),
                password: Some(pg.password.clone()),
                ssl_mode: datazen_driver_api::SslMode::Prefer,
                connection_timeout: 10,
                max_pool_size: 4,
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
        Arc::new(
            datazen_server::execution::budget::ServerBudgetCoordinator::new(
                4,
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
        scope,
        bridge,
        None,
    ));
    let sink = RuntimeResultSink::default();
    backend
        .set_publisher(execution_result_publisher(sink.clone()))
        .expect("publisher installed");
    let policy = Arc::new(ServerExecutionPolicy::new(
        engine,
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
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
        directory,
    ));
    let host = Arc::new(ServerResourceExecutionHost::new(
        connections.clone(),
        profiles.clone(),
        registry,
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        real_lease,
        "org-1",
    ));
    let stateless = StatelessToolExec::new(
        Arc::new(ServerRedactedConnections::new(connections, "org-1")),
        Arc::new(EmptyWorkflowCatalog),
        vec![connection_id.clone()],
    )
    .with_target_machine(TargetMachine {
        host,
        // storage fresh-install 的 execution 缺省形状（`ExecutionSettings`
        // 无 Default，字段全列；settings.rs 装配面同值）。
        settings: ExecutionSettings {
            limit_select_results: false,
            query_result_limit: 5000,
            safe_mode: true,
            connection_pool_size: 10,
            mcp_disabled_tools: Vec::new(),
            mcp_permission_mode: "safe_write".into(),
            mcp_allowed_connection_ids: Vec::new(),
            driver_settings: serde_json::Map::new(),
            ai_strict_egress: true,
        },
    });
    let mcp_backend = ModulesBackend::new(
        Arc::new(FixedLeaseSource(runtime)),
        vec![connection_id.clone()],
        datazen::mcp::domain::permission::McpPermissionMode::SafeWrite,
    )
    .with_stateless(Arc::new(stateless));

    LiveFixture {
        backend: Arc::new(mcp_backend),
        connection_id,
        database: pg.database.clone(),
        _store: store,
    }
}

fn identity() -> CallIdentity {
    CallIdentity {
        organization_id: "org-1".to_owned(),
        principal_id: "user-1".to_owned(),
        request_id: "req-mcp-live-pg".to_owned(),
    }
}

async fn call_tool_ok(backend: &ModulesBackend, tool: &str, arguments: Value) -> Value {
    match backend.call_tool(&identity(), tool, arguments).await {
        Ok(text) => serde_json::from_str(&text)
            .unwrap_or_else(|panic| panic!("tool {tool} returned non-JSON text: {panic}")),
        Err(error) => panic!(
            "tool {tool} should take the live green path, got: {}",
            render(&error)
        ),
    }
}

/// `ToolCallError` 的文本化（wire 层未派生 Debug/Display；诊断消息用）。
fn render(error: &ToolCallError) -> String {
    match error {
        ToolCallError::WithHelp { reason, .. } => format!("with_help({reason})"),
        ToolCallError::Plain(reason) => format!("plain({reason})"),
        ToolCallError::Internal(reason) => format!("internal({reason})"),
    }
}

fn state_wire(state: datazen::execution::ExecutionState) -> Value {
    serde_json::to_value(state).expect("state serializes")
}

/// 有状态旅程（真方言）：open_session → use_session(SELECT 1) → close_session。
#[tokio::test]
#[ignore = "requires a live PostgreSQL; set DATAZEN_LIVE_PG_* and run with --ignored"]
async fn live_postgres_session_journey_opens_queries_and_closes() {
    let Some(pg) = live_pg() else {
        eprintln!("skip: DATAZEN_LIVE_PG_HOST is not set");
        return;
    };
    let fx = live_fixture(&pg).await;
    let backend = fx.backend.as_ref();

    let opened = call_tool_ok(
        backend,
        "open_session",
        json!({
            "connection_id": fx.connection_id,
            "database": fx.database,
        }),
    )
    .await;
    let reference = opened["clientSessionRef"]
        .as_str()
        .expect("a live open_session mints a ref")
        .to_owned();
    assert_eq!(
        opened["state"],
        state_wire(datazen::execution::ExecutionState::Succeeded)
    );

    let used = call_tool_ok(
        backend,
        "use_session",
        json!({
            "client_session_ref": reference,
            "command": "query",
            "input": {"sql": "SELECT 1"},
        }),
    )
    .await;
    assert_eq!(
        used["state"],
        state_wire(datazen::execution::ExecutionState::Succeeded),
        "{used}"
    );
    let rows = used["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 1, "SELECT 1 answers one row: {used}");
    assert_eq!(rows[0][0], json!(1), "the live cell is 1: {used}");

    let closed = call_tool_ok(
        backend,
        "close_session",
        json!({ "client_session_ref": reference }),
    )
    .await;
    assert_eq!(closed, json!({"closed": true}), "{closed}");
}

/// 无状态七工具的 query 腿（真方言）：同一闸序在真驱动上走通。
#[tokio::test]
#[ignore = "requires a live PostgreSQL; set DATAZEN_LIVE_PG_* and run with --ignored"]
async fn live_postgres_query_tool_answers_through_the_target_machine() {
    let Some(pg) = live_pg() else {
        eprintln!("skip: DATAZEN_LIVE_PG_HOST is not set");
        return;
    };
    let fx = live_fixture(&pg).await;

    let query = call_tool_ok(
        fx.backend.as_ref(),
        "query",
        json!({
            "connection_id": fx.connection_id,
            "sql": "SELECT 1",
            "limit": 10,
        }),
    )
    .await;
    let statement = &query["results"][0];
    assert_eq!(statement["rows"][0][0], json!(1), "{query}");
}

/// 错误通道（真方言）：未知引用按 SessionLost 家族应答，不泄漏存在性。
#[tokio::test]
#[ignore = "requires a live PostgreSQL; set DATAZEN_LIVE_PG_* and run with --ignored"]
async fn live_postgres_use_session_on_a_bogus_ref_is_session_lost() {
    let Some(pg) = live_pg() else {
        eprintln!("skip: DATAZEN_LIVE_PG_HOST is not set");
        return;
    };
    let fx = live_fixture(&pg).await;
    match fx
        .backend
        .call_tool(
            &identity(),
            "use_session",
            json!({
                "client_session_ref": "bogus-live-ref",
                "command": "query",
                "input": {"sql": "SELECT 1"},
            }),
        )
        .await
    {
        Err(ToolCallError::Plain(reason)) => {
            assert!(
                reason.starts_with("SessionLost"),
                "a bogus ref answers SessionLost, got: {reason}"
            );
        }
        other => panic!(
            "a bogus ref must be the bare SessionLost family, got: {}",
            render(&other)
        ),
    }
}
