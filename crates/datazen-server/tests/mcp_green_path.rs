//! 绿路径刀：MCP seed-example + 七工具的 server 绿路径（fake-driver 常态 CI）。
//!
//! ## 刀的范围（W8-a 绿路径刀）
//!
//! server MCP 面（`src/mcp/`）的 13 工具里，**七工具**是经
//! `read_target_command` 闸序（profile → driver → 命令目录 → 执行）的目标
//! 命令工具：`query` / `list_databases` / `list_tables` / `search_tables` /
//! `get_schema` / `explain_query` / `describe_table`（登记见
//! `stateless_exec.rs` 模块头状态表），外加有状态会话三工具
//! `open_session` / `use_session` / `close_session`。本文件用
//! `datazen_driver_api::mock_driver::MockDriver`（fake-driver 先例：
//! `table_data_routes.rs` 的 `wired_fixture`、桌面 `TestMockDriverFactory`）
//! 驱动同一套生产装配走通绿路径——无任何真方言服务，常态 CI 可跑：
//!
//! 1. **有状态旅程**：open_session → use_session(query) 读回行 →
//!    close_session → 幂等重关 → 关后再用报 SessionLost；
//! 2. **七工具逐一正向**：每工具断言经目标机器（`TargetMachine` +
//!    `ServerResourceExecutionHost` 闸序）返回的结果形状；
//! 3. **seed-example**：契约表 `TOOL_HELPS` 的逐工具 `example` 调用体就是
//!    本刀的 seed（判读依据见下）——全部 13 个 example 原样（占位符替换
//!    后）经真后端跑一遍；
//! 4. **P3 真主体**：per-call 身份（`CallIdentity` → `RequestContext`）驱动
//!    执行面授权——组织内真主体走通闸序，非本组织成员/伪报组织在 token
//!    授权面被拒；
//! 5. **streamable HTTP wire**：同一真后端经 `handle_post` 走
//!    initialize → tools/call 全链，断言 JSON-RPC 包装形状。
//!
//! ## seed-example 判读（本刀的语义登记）
//!
//! 全仓 rg 无 `seed-example` 命名（桌面 seed 数据是 e2e/contract 的 TS 面，
//! 不属 server）。本刀判读：seed-example = `contract.rs` `TOOL_HELPS` 里
//! 逐工具的 `example` 字段——它是 13 工具契约的「示例调用种子」，golden
//! 契约帮助块逐字携带它。测试把这些 example 原样作为 `tools/call` 参数
//! seed 进真后端：占位符 `<uuid>` → 夹具真连接 id、`<opaque-ref>` →
//! open_session 真回执。断言每个 example 都被契约层接受（未知工具 /
//! missing-required-input-property 均为红），可达绿路径的断言到结果形状，
//! 不可达的（空目录 `run_workflow`）断言其诚实应答。
//!
//! `list_tables` 族 example 未携带可选 `database` 时，seed 注入夹具连接的
//! 默认库（契约语义「database optional — defaults to connection default」；
//! 夹具连接默认库即 `app`）。除占位符与该默认库注入外，example 逐字不改。
//!
//! ## 装配
//!
//! 与 bootstrap 同构（`table_data_routes.rs::wired_fixture` 先例的 MCP
//! 同构）：真 SQLite 管理库（profile 行 + 连接目录行 + membership 行）+
//! MockDriver 工厂注册进 `DriverRegistry::register_test_driver/_
//! factory`（`datazen` test-harness 门后面，dev-deps 点亮）+
//! `LegacyResourceAdapter` 资源提供者 + 预算桥 + `ServerDriverExecBackend`
//! + `ServerExecutionPolicy` + `ServerRuntimeLeaseSource` +
//! `ServerResourceExecutionHost` + `StatelessToolExec(TargetMachine)` +
//! `ModulesBackend`。会话连续性沿 `FixedLeaseSource` 先例（g1_w1_gaps）：夹
//! 具恒返同一 runtime，会话/令牌签发与消费落在同一份内存账上——刀0（CM-63）
//! 后主线 `ServerRuntimeLeaseSource` 自身即持当前单元直至吊销，两侧语义同效。
//!
//! serde_json `preserve_order`（bson/mongodb 等驱动传递启用）会让键序随
//! feature 组合漂移——本文件全部做**结构断言**（按键取值），不做字节级
//! JSON 比较。

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::orchestration::{
    RuntimeConnectionUseCases, RuntimeResultSink, SystemRuntimeClock,
};
use datazen::settings::ExecutionSettings;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{
    ClientInstanceId, IdempotencyKey, OrganizationId, PrincipalId, RequestId,
};
use datazen_driver_api::capabilities;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{
    explain_command_definition, ColumnSchema, ConnectionConfig, DatabaseDriver,
    DatabaseDriverFactory, ExplainResult, ForeignKeyInfo, IndexInfo, SslMode, TableInfo,
    TableSchema, TableType, Value as DriverValue,
};
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::{RuntimeLeaseSource, ServerRuntimeLeaseSource};
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::mcp::http::{CallIdentity, HttpMcpState, McpBackend, SessionAuthenticator};
use datazen_server::mcp::runtime::ModulesBackend;
use datazen_server::mcp::stateless_exec::{
    EmptyWorkflowCatalog, ServerRedactedConnections, StatelessToolExec, TargetMachine,
};
use datazen_server::mcp::wire::ToolCallError;
use datazen_server::policy::{PolicyService as ServerPolicyEngine, Role};
use datazen_server::storage::{AdminStore, ServerConnections, ServerProfiles, ServerSettingsStore};

const SERVER_SECRET: &[u8] = b"mcp-green-path-server-secret-32b!";

// ── fake-driver 装配（table_data_routes.rs `WireMockFactory` 的 MCP 同构）──

/// 把 [`MockDriver`] 连同 [`LegacyResourceAdapter`] 包成静态工厂
/// （`register_test_factory` 吃 `&'static dyn DatabaseDriverFactory`）。
/// 命名空间形状：Database 在场（可选）、Catalog 缺席、Schema 在场（可选）
/// ——七工具的目标规范化按此形状裁决。
struct GreenMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn DatabaseDriver>,
}

impl DatabaseDriverFactory for GreenMockFactory {
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
            "mcp-green-path",
            1,
            shape,
            capabilities,
        )))
    }
}

/// 会话连续性的测试形（`g1_w1_gaps.rs` 先例）：恒返回**同一** runtime 实例，
/// open/use/close 三次调用落在同一份内存账上。刀0（CM-63）后主线的
/// `ServerRuntimeLeaseSource` 自身即「持当前单元直至吊销」（未吊销窗口内
/// resolve 恒同一 `Arc`），本形与该语义同效；执行后端与共享 sink/directory
/// 仍与主线同构。
struct FixedLeaseSource(Arc<RuntimeConnectionUseCases>);

#[async_trait::async_trait]
impl RuntimeLeaseSource for FixedLeaseSource {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
        Ok(self.0.clone())
    }
}

/// seed 夹具的示例数据集：两列一行的 users 表 + 两库两表 + EXPLAIN 计划。
/// `explain_command_definition` 显式进 `extra_commands`——explain 是会话绑
/// 定命令，驱动默认定义不含它（`execute_standard_sql_command` 的 explain
/// 分支以「驱动登记了 explain」为前提）。
fn green_options() -> MockDriverOptions {
    let columns = vec![
        ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: true,
        },
        ColumnSchema {
            name: "email".into(),
            data_type: "text".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
    ];
    MockDriverOptions {
        columns: columns.clone(),
        primary_keys: vec!["id".into()],
        query_rows: vec![vec![
            Some(DriverValue::Integer(42)),
            Some(DriverValue::String("ada@example.test".into())),
        ]],
        count_total: 2,
        databases: vec!["app".into(), "archive".into()],
        tables: vec![
            TableInfo {
                name: "users".into(),
                schema: Some("public".into()),
                table_type: TableType::Table,
                row_count: Some(2),
            },
            TableInfo {
                name: "audit_log".into(),
                schema: Some("public".into()),
                table_type: TableType::View,
                row_count: None,
            },
        ],
        table_schema: Some(TableSchema {
            table_name: "users".into(),
            columns,
            primary_keys: vec!["id".into()],
            indexes: vec![IndexInfo {
                name: "users_pkey".into(),
                columns: vec!["id".into()],
                is_unique: true,
                is_primary: true,
                index_type: "btree".into(),
            }],
            foreign_keys: vec![ForeignKeyInfo {
                name: "fk_users_org".into(),
                columns: vec!["org_id".into()],
                referenced_table: "orgs".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "CASCADE".into(),
                deferrability: Default::default(),
            }],
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }),
        explain_plan: ExplainResult {
            plan_text: "Seq Scan on users".into(),
            plan_json: None,
            plan_tree: None,
            total_cost: None,
            estimated_rows: Some(2),
        },
        extra_commands: vec![explain_command_definition()],
        // MockDriver 的 extra_commands echo 分支会遮蔽真 explain 驱动方法，
        // 直接编程命令应答（output_schema 要求 planText；与 opts.explain_plan
        // 同一内容）。
        command_results: HashMap::from([(
            "explain".to_string(),
            serde_json::json!({
                "planText": "Seq Scan on users",
                "planJson": null,
                "totalCost": null,
                "estimatedRows": 2,
            }),
        )]),
        ..MockDriverOptions::default()
    }
}

/// MCP 后端夹具：生产装配 + MockDriver 全栈。每个测试独立内存库。
struct McpFixture {
    backend: Arc<ModulesBackend>,
    /// 连接 id（profile 行与连接目录行两半齐备）。
    connection_id: String,
    _store: Arc<AdminStore>,
}

async fn mcp_fixture(opts: MockDriverOptions) -> McpFixture {
    let store = Arc::new(AdminStore::open_in_memory().expect("admin store"));
    store.migrate().expect("migrations apply v1..=v9");

    // membership 行（执行面 PolicyService 的 RBAC 基线事实来源）。
    let engine = Arc::new(
        ServerPolicyEngine::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine"),
    );
    engine
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership org-1");
    engine
        .create_membership("org-2", "user-2", Role::OrgMember, "idp:user-2")
        .expect("membership org-2");

    // 驱动注册表：懒形 + 测试驱动 + 测试工厂（生产缺省形态零依赖边的对偶
    // 面；`register_test_driver` 供 `get` 直取，`register_test_factory` 供
    // `resource_provider` 闸）。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let driver = MockDriver::new("mockdb", opts);
    registry
        .register_test_driver("mockdb", driver.clone())
        .await;
    registry
        .register_test_factory(
            "mockdb",
            Box::leak(Box::new(GreenMockFactory {
                driver_id: "mockdb",
                driver: driver.clone(),
            })),
        )
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    // profile 行 + 连接目录行（同一连接 id 两半都齐，闸序可达驱动层）。
    let ctx = RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("test"),
        RequestId::new("req-mcp-green-fixture"),
        None,
    );
    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("mcp-green-conn", "mockdb"),
            &IdempotencyKey::new("idem-mcp-green-profile"),
        )
        .await
        .expect("profile create");
    let connection_id = record.connection_id.as_str().to_owned();
    let scope = ConnectionScope::new("org-1");
    connections
        .save(
            &scope,
            ConnectionConfig {
                id: connection_id.clone(),
                name: "mcp green conn".into(),
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

    // 执行后端 + 结果发布钩子 + 共享 sink/directory（bootstrap 同构纪律）。
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
    // 会话面单例（FixedLeaseSource 先例）：open/use/close 同一 runtime。
    let runtime = Arc::new(RuntimeConnectionUseCases::new_with_parts(
        profiles.clone(),
        policy.clone(),
        backend.clone(),
        Arc::new(SystemRuntimeClock(std::time::Instant::now()))
            as Arc<dyn datazen::execution::runtime::gateway::MonotonicClock>,
        sink.clone(),
        directory.clone(),
    ));
    // 资源宿主（七工具闸序）持真租约源——prepare 的 resolve 自洽（闸序 +
    // 用例面同源于 PreparedExecution）。
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
    // 七工具目标命令机器 + 会话三工具租约源 → MCP 后端。
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

    McpFixture {
        backend: Arc::new(mcp_backend),
        connection_id,
        _store: store,
    }
}

/// L2 登录会话身份（传输层真主体；P3 语义的输入面）。
fn identity(org: &str, principal: &str) -> CallIdentity {
    CallIdentity {
        organization_id: org.to_owned(),
        principal_id: principal.to_owned(),
        request_id: format!("req-mcp-green-{principal}"),
    }
}

/// 成功调用：返回解析后的 JSON 结果（结构断言，不做字节级比较）。
async fn call_tool_ok(
    backend: &ModulesBackend,
    identity: &CallIdentity,
    tool: &str,
    arguments: Value,
) -> Value {
    match backend.call_tool(identity, tool, arguments).await {
        Ok(text) => serde_json::from_str(&text)
            .unwrap_or_else(|panic| panic!("tool {tool} returned non-JSON text: {panic}")),
        Err(error) => panic!(
            "tool {tool} should take the green path, got: {}",
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

/// 期望失败调用：返回错误并附上工具名便于诊断。
async fn call_tool_err(
    backend: &ModulesBackend,
    identity: &CallIdentity,
    tool: &str,
    arguments: Value,
) -> ToolCallError {
    let text = arguments.to_string();
    match backend.call_tool(identity, tool, arguments).await {
        Ok(reply) => {
            panic!("tool {tool} should fail, but answered green with: {reply} (args {text})")
        }
        Err(error) => error,
    }
}

/// 成功调用：返回原始文本应答（describe_table 等文本型工具不保证 JSON）。
async fn call_tool_ok_text(
    backend: &ModulesBackend,
    identity: &CallIdentity,
    tool: &str,
    arguments: Value,
) -> String {
    match backend.call_tool(identity, tool, arguments).await {
        Ok(text) => text,
        Err(error) => panic!(
            "tool {tool} should take the green path, got: {}",
            render(&error)
        ),
    }
}

fn state_wire(state: datazen::execution::ExecutionState) -> Value {
    serde_json::to_value(state).expect("state serializes")
}

// ── 1. 有状态旅程：open → use(query) 读回行 → close → 幂等 → SessionLost ──

#[tokio::test]
async fn the_stateful_session_journey_opens_executes_reads_and_closes() {
    let fx = mcp_fixture(green_options()).await;
    let who = identity("org-1", "user-1");

    // open_session：不透明引用 + 状态 + TTL 面齐备。
    let opened = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "open_session",
        json!({
            "connection_id": fx.connection_id,
            "database": "app",
            "ttl_ms": 600_000,
        }),
    )
    .await;
    let reference = opened["clientSessionRef"]
        .as_str()
        .expect("open_session mints a non-empty opaque ref")
        .to_owned();
    assert!(!reference.is_empty());
    assert_ne!(reference, fx.connection_id, "the ref is opaque, not an id");
    assert_eq!(
        opened["state"],
        json!("ready"),
        "a freshly opened session reports its live SessionState (open 回执的 state 是会话态，不是执行态)"
    );
    assert!(
        opened["contextRevision"].is_string(),
        "context revision stamped (CM-01 十进制字符串): {opened}"
    );
    assert_eq!(opened["expiresInMs"], json!(600_000), "ttl echoed in ms");

    // use_session(query)：真闸序（token → 提交 → 等终态 → 读产物块）。
    let used = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "use_session",
        json!({
            "client_session_ref": reference,
            "command": "query",
            "input": {"sql": "SELECT id, email FROM users"},
        }),
    )
    .await;
    assert_eq!(
        used["state"],
        state_wire(datazen::execution::ExecutionState::Succeeded)
    );
    assert!(
        used["executionId"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "execution id present: {used}"
    );
    assert_eq!(
        used["rowsTruncated"],
        json!(false),
        "a two-cell row is nowhere near the 200-row cap"
    );
    let rows = used["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 1, "the mock answers exactly one row: {used}");
    assert_eq!(
        rows[0][0],
        json!(42),
        "the mock's seeded cell reaches the report"
    );
    assert_eq!(rows[0][1], json!("ada@example.test"));
    assert!(
        used["artifactIds"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty()),
        "row batches arrived via published artifacts: {used}"
    );

    // close_session：确认释放（跑过命令的会话必须显式选 rollbackAndClose：
    // 适配器事务观测不可证无事务，默认 requireNoTransaction 会被闸拒绝——
    // 这正是 transactionResolutionRequired 的设计语义：客户端必须裁决）。
    let closed = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "close_session",
        json!({ "client_session_ref": reference, "mode": "rollbackAndClose" }),
    )
    .await;
    assert_eq!(
        closed,
        json!({"closed": true}),
        "a confirmed close: {closed}"
    );

    // 幂等重关：不区分未知/过期/异主，一律 closed=false（§4.2）。
    let reclosed = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "close_session",
        json!({ "client_session_ref": reference }),
    )
    .await;
    assert_eq!(reclosed, json!({"closed": false}));

    // 关后再用：SessionLost 家族裸文案（不附帮助块）。
    let lost = call_tool_err(
        fx.backend.as_ref(),
        &who,
        "use_session",
        json!({
            "client_session_ref": reference,
            "command": "query",
            "input": {"sql": "SELECT 1"},
        }),
    )
    .await;
    match lost {
        ToolCallError::Plain(reason) => {
            assert!(
                reason.starts_with("SessionLost"),
                "the closed ref answers SessionLost, got: {reason}"
            );
        }
        other => panic!(
            "a use on a closed ref must be the bare SessionLost family, got: {}",
            render(&other)
        ),
    }
}

// ── 2. 七工具逐一正向（目标机器闸序）────────────────────────────────────

/// 七工具 = `stateless_exec.rs` 状态表的目标命令工具集合。
const SEVEN_TARGET_COMMAND_TOOLS: [&str; 7] = [
    "query",
    "list_databases",
    "list_tables",
    "search_tables",
    "get_schema",
    "explain_query",
    "describe_table",
];

#[tokio::test]
async fn each_of_the_seven_target_command_tools_answers_through_the_target_machine() {
    let fx = mcp_fixture(green_options()).await;
    let who = identity("org-1", "user-1");
    let connection_id = fx.connection_id.clone();

    // query：SQL 守卫 → 目标机器 → MockDriver 行。
    let query = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "query",
        json!({
            "connection_id": connection_id,
            "sql": "SELECT id, email FROM users",
            "limit": 50,
        }),
    )
    .await;
    let statement = &query["results"][0];
    assert_eq!(
        statement["sql"],
        json!("SELECT id, email FROM users"),
        "the statement is echoed verbatim: {query}"
    );
    assert_eq!(statement["columns"][0]["name"], json!("id"));
    assert_eq!(statement["columns"][0]["dataType"], json!("integer"));
    assert_eq!(statement["rows"][0][0], json!(42));
    assert_eq!(statement["rows"][0][1], json!("ada@example.test"));

    // list_databases：脱敏目录 → 驱动 get_databases。
    let databases = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "list_databases",
        json!({ "connection_id": connection_id }),
    )
    .await;
    let names: Vec<&str> = databases
        .as_array()
        .expect("list_databases answers an array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(names, vec!["app", "archive"], "{databases}");

    // list_tables：驱动 get_tables（camelCase 投影）。
    let tables = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "list_tables",
        json!({ "connection_id": connection_id, "database": "app" }),
    )
    .await;
    let tables = tables.as_array().expect("list_tables answers an array");
    let names: Vec<&str> = tables.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names, vec!["users", "audit_log"], "{tables:?}");
    assert_eq!(tables[0]["tableType"], json!("table"));
    assert_eq!(tables[0]["rowCount"], json!(2));

    // search_tables：模式大小写不敏感子串 + 上限 + 三元组计数。
    let search = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "search_tables",
        json!({
            "connection_id": connection_id,
            "pattern": "AUD",
            "limit": 20,
        }),
    )
    .await;
    assert_eq!(search["pattern"], json!("AUD"));
    assert_eq!(search["totalTables"], json!(2), "{search}");
    assert_eq!(search["totalMatches"], json!(1), "{search}");
    assert_eq!(search["matched"][0]["name"], json!("audit_log"));

    // get_schema：完整结构包一层数组（列/主键/索引/外键）。
    let schema = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "get_schema",
        json!({
            "connection_id": connection_id,
            "database": "app",
            "table": "users",
        }),
    )
    .await;
    let schema = schema.as_array().expect("get_schema wraps one schema");
    assert_eq!(schema.len(), 1, "{schema:?}");
    assert_eq!(schema[0]["tableName"], json!("users"));
    assert_eq!(schema[0]["primaryKeys"], json!(["id"]));
    assert_eq!(schema[0]["columns"][1]["name"], json!("email"));
    assert_eq!(
        schema[0]["foreignKeys"][0]["referencedTable"],
        json!("orgs")
    );

    // explain_query：驱动 EXPLAIN 计划。
    let explain = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "explain_query",
        json!({
            "connection_id": connection_id,
            "sql": "SELECT * FROM users",
        }),
    )
    .await;
    assert_eq!(explain["planText"], json!("Seq Scan on users"), "{explain}");

    // describe_table：人类可读描述（列 + 主键 + 外键面）。文本型工具，
    // 不经 JSON 解析。
    let description = call_tool_ok_text(
        fx.backend.as_ref(),
        &who,
        "describe_table",
        json!({
            "connection_id": connection_id,
            "database": "app",
            "table": "users",
        }),
    )
    .await;
    assert!(description.contains("Table: users"), "{description}");
    assert!(description.contains("id integer PK"), "{description}");
    assert!(description.contains("Primary Key: (id)"), "{description}");
    assert!(
        description.contains("fk_users_org"),
        "the FK surface reaches the description: {description}"
    );

    // 登记面：七工具与契约表逐一对齐（防名单漂移）。
    for tool in SEVEN_TARGET_COMMAND_TOOLS {
        assert!(
            datazen_server::mcp::contract::MCP_ALL_TOOLS.contains(&tool),
            "{tool} must stay on the 13-tool contract"
        );
    }
}

// ── 3. seed-example：契约 example 全量过真后端 ─────────────────────────────

/// 逐字取契约 example 的 `arguments`（`contract.rs` `TOOL_HELPS`）。
fn example_arguments(tool: &str) -> Value {
    let help = datazen_server::mcp::contract::lookup(tool)
        .unwrap_or_else(|| panic!("contract help for {tool}"));
    let example: Value = serde_json::from_str(help.example)
        .unwrap_or_else(|panic| panic!("example of {tool} must be JSON: {panic}"));
    example["arguments"].clone()
}

/// seed 占位符替换：`<uuid>` → 夹具真连接 id；`<opaque-ref>` → 真回执。
/// 契约可选属性 `database` 缺席时注入夹具连接默认库（契约语义
/// 「defaults to connection default」；注入判据读契约表 props，非硬编码）。
/// 除占位符与该默认库注入外，example 逐字不改。
fn seed_arguments(
    tool: &str,
    arguments: &Value,
    connection_id: &str,
    reference: &str,
    database: &str,
) -> Value {
    let mut seeded = arguments.clone();
    if let Some(object) = seeded.as_object_mut() {
        for value in object.values_mut() {
            if value.as_str() == Some("<uuid>") {
                *value = json!(connection_id);
            }
            if value.as_str() == Some("<opaque-ref>") {
                *value = json!(reference);
            }
        }
        let takes_optional_database = datazen_server::mcp::contract::lookup(tool)
            .is_some_and(|help| help.props.iter().any(|(key, _)| *key == "database"));
        if takes_optional_database && !object.contains_key("database") {
            object.insert("database".into(), json!(database));
        }
    }
    seeded
}

#[tokio::test]
async fn every_contract_example_seed_takes_its_honest_green_path() {
    let fx = mcp_fixture(green_options()).await;
    let who = identity("org-1", "user-1");
    let tools = datazen_server::mcp::contract::MCP_ALL_TOOLS;

    // 会话 example 的 <opaque-ref> 用同一旅程的真实回执：open → use → close。
    let opened = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "open_session",
        seed_arguments(
            "open_session",
            &example_arguments("open_session"),
            &fx.connection_id,
            "",
            "app",
        ),
    )
    .await;
    let reference = opened["clientSessionRef"]
        .as_str()
        .expect("the open_session example seeds a live ref")
        .to_owned();

    for tool in tools
        .iter()
        .copied()
        .filter(|tool| *tool != "describe_table")
    {
        let seeded = seed_arguments(
            tool,
            &example_arguments(tool),
            &fx.connection_id,
            &reference,
            "app",
        );
        match tool {
            // 空目录的诚实应答：参数类「不存在」，example 参数本身过契约。
            "run_workflow" => {
                let error = call_tool_err(fx.backend.as_ref(), &who, tool, seeded).await;
                match error {
                    ToolCallError::WithHelp { reason, .. } => {
                        assert!(
                            reason.contains("not found"),
                            "an empty catalog answers 'not found', got: {reason}"
                        );
                    }
                    other => {
                        panic!(
                            "run_workflow's example must map to invalid_params, got: {}",
                            render(&other)
                        )
                    }
                }
            }
            // 其余 12 个 example 全绿（可达结果形状的断到形状）。
            _ => {
                let result = call_tool_ok(fx.backend.as_ref(), &who, tool, seeded).await;
                match tool {
                    "list_connections" => {
                        let entries = result.as_array().expect("a redacted directory");
                        assert!(
                            entries.iter().any(|e| e["id"] == json!(fx.connection_id)),
                            "the fixture connection is listed redacted: {result}"
                        );
                        assert!(
                            entries.iter().all(|e| e.get("password").is_none()),
                            "credentials never reach the projection: {result}"
                        );
                    }
                    "list_workflows" => {
                        assert_eq!(
                            result,
                            json!([]),
                            "server v1 has no workflow storage; the empty directory is the truth"
                        );
                    }
                    "open_session" | "use_session" | "close_session" => {
                        // 旅程回执在上方 open/use/close 已断形状；此处钉住
                        // close 的确认面。
                        if tool == "close_session" {
                            assert_eq!(result, json!({"closed": true}));
                        } else if tool == "use_session" {
                            assert_eq!(
                                result["state"],
                                state_wire(datazen::execution::ExecutionState::Succeeded)
                            );
                        }
                    }
                    "query" => {
                        assert_eq!(result["results"][0]["rows"][0][0], json!(42), "{result}");
                    }
                    "list_databases" => {
                        assert_eq!(result, json!(["app", "archive"]), "{result}");
                    }
                    "list_tables" => {
                        assert_eq!(result[0]["name"], json!("users"), "{result}");
                    }
                    "search_tables" => {
                        // 契约 example 的 pattern 是 "user"（contract.rs）：
                        // contains 匹配 "users"，"audit_log" 不含该子串。
                        assert_eq!(result["matched"][0]["name"], json!("users"));
                        assert_eq!(result["totalTables"], json!(2));
                    }
                    "get_schema" => {
                        assert_eq!(result[0]["tableName"], json!("users"), "{result}");
                    }
                    "explain_query" => {
                        assert_eq!(result["planText"], json!("Seq Scan on users"));
                    }
                    "describe_table" => unreachable!("asserted separately (text answer)"),
                    other => panic!("unhandled contract tool {other}"),
                }
            }
        }
    }

    // use_session 的 example 已随 seed 跑过 query；close example
    // （rollbackAndClose）已关同一引用：重关是幂等 false（§4.2）。
    let reclosed = call_tool_ok(
        fx.backend.as_ref(),
        &who,
        "close_session",
        json!({ "client_session_ref": reference }),
    )
    .await;
    assert_eq!(
        reclosed,
        json!({"closed": false}),
        "a closed ref stays closed without leaking its history"
    );
}

// ── 4. P3 真主体：per-call 身份驱动执行面授权 ─────────────────────────────────

#[tokio::test]
async fn the_per_call_identity_drives_the_authorization_face() {
    let fx = mcp_fixture(green_options()).await;

    // 正向半边：组织内真主体（CallIdentity → RequestContext）走通闸序。
    let member = identity("org-1", "user-1");
    let opened = call_tool_ok(
        fx.backend.as_ref(),
        &member,
        "open_session",
        json!({
            "connection_id": fx.connection_id,
            "database": "app",
        }),
    )
    .await;
    let reference = opened["clientSessionRef"]
        .as_str()
        .expect("an in-org member opens a session")
        .to_owned();

    // P3 反向钉子：同一连接、同一 client_instance，主体不是 ctx 组织的
    // membership 事实 → token 授权面否决（合成/伪造主体都拿不到 membership）。
    // server v1 的执行域按部署组织装配（bootstrap v1 裁决，闸序读半边的
    // 组织口径来自装配配置），per-call 主体的权威面就是这条授权——它拒绝
    // 即拒绝，没有任何合成主体可以代赎。
    for outsider in [identity("org-1", "user-2"), identity("org-2", "user-1")] {
        let error = call_tool_err(
            fx.backend.as_ref(),
            &outsider,
            "open_session",
            json!({
                "connection_id": fx.connection_id,
                "database": "app",
            }),
        )
        .await;
        match error {
            ToolCallError::Internal(reason) => {
                assert!(
                    reason.contains("permission denied"),
                    "a non-member principal is denied by the authorization face, got: {reason}"
                );
            }
            other => {
                panic!(
                    "open_session by {} identity must fail at the authorization face",
                    render(&other)
                )
            }
        }
    }

    // 对照组：同一组织真主体的会话照常执行（P3 的正向半边随行）。
    let used = call_tool_ok(
        fx.backend.as_ref(),
        &member,
        "use_session",
        json!({
            "client_session_ref": reference,
            "command": "query",
            "input": {"sql": "SELECT id FROM users"},
        }),
    )
    .await;
    assert_eq!(
        used["state"],
        state_wire(datazen::execution::ExecutionState::Succeeded)
    );
    assert_eq!(used["rows"][0][0], json!(42));

    // 会话收尾，不留悬挂资源。
    let closed = call_tool_ok(
        fx.backend.as_ref(),
        &member,
        "close_session",
        json!({ "client_session_ref": reference, "mode": "rollbackAndClose" }),
    )
    .await;
    assert_eq!(closed, json!({"closed": true}));
}

// ── 5. streamable HTTP wire：同一真后端走 initialize → tools/call 全链 ──

/// 桩认证器：一切请求都视为 org-1/user-1 的 L2 登录会话（本文件只钉 wire
/// 包装形状，身份面已由上两节用真 CallIdentity 钉住）。
struct GreenAuth;

impl SessionAuthenticator for GreenAuth {
    fn authenticate(&self, _headers: &axum::http::HeaderMap) -> Option<CallIdentity> {
        Some(CallIdentity {
            organization_id: "org-1".to_owned(),
            principal_id: "user-1".to_owned(),
            request_id: datazen_server::mcp::http::new_request_id(),
        })
    }
}

async fn rpc_call(
    state: &HttpMcpState,
    session: Option<&str>,
    id: Value,
    name: &str,
    arguments: Value,
) -> (axum::http::StatusCode, Value) {
    let response = datazen_server::mcp::http::handle_post(
        state,
        session,
        Some(identity("org-1", "user-1")),
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments},
        }),
    )
    .await;
    let body = response.body.expect("tools/call answers a body");
    (response.status, body)
}

#[tokio::test]
async fn the_session_journey_keeps_its_wire_shape_over_streamable_http() {
    let fx = mcp_fixture(green_options()).await;
    let state = HttpMcpState::new(fx.backend.clone(), Arc::new(GreenAuth));

    let init = datazen_server::mcp::http::handle_post(
        &state,
        None,
        Some(identity("org-1", "user-1")),
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
    )
    .await;
    assert_eq!(init.status, axum::http::StatusCode::OK);
    let session = init
        .headers
        .iter()
        .find(|(k, _)| k == datazen_server::mcp::wire::MCP_SESSION_ID_HEADER)
        .map(|(_, v)| v.clone())
        .expect("initialize issues Mcp-Session-Id");

    // open_session over wire：text content 包 JSON 结果，isError=false。
    let (status, body) = rpc_call(
        &state,
        Some(&session),
        json!(2),
        "open_session",
        json!({"connection_id": fx.connection_id, "database": "app"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["result"]["isError"], json!(false));
    assert_eq!(body["result"]["content"][0]["type"], json!("text"));
    let opened: Value = serde_json::from_str(
        body["result"]["content"][0]["text"]
            .as_str()
            .expect("text content"),
    )
    .expect("open payload parses");
    let reference = opened["clientSessionRef"].as_str().expect("ref").to_owned();

    // use_session over wire：行经产物读回进 text content。
    let (status, body) = rpc_call(
        &state,
        Some(&session),
        json!(3),
        "use_session",
        json!({
            "client_session_ref": reference,
            "command": "query",
            "input": {"sql": "SELECT id FROM users"},
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["result"]["isError"], json!(false));
    let used: Value =
        serde_json::from_str(body["result"]["content"][0]["text"].as_str().expect("text"))
            .expect("use payload parses");
    assert_eq!(
        used["state"],
        state_wire(datazen::execution::ExecutionState::Succeeded)
    );
    assert_eq!(used["rows"][0][0], json!(42));

    // close_session over wire（跑过命令 → 显式 rollbackAndClose，同上）。
    let (status, body) = rpc_call(
        &state,
        Some(&session),
        json!(4),
        "close_session",
        json!({"client_session_ref": reference, "mode": "rollbackAndClose"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let closed: Value =
        serde_json::from_str(body["result"]["content"][0]["text"].as_str().expect("text"))
            .expect("close payload parses");
    assert_eq!(closed, json!({"closed": true}));

    // 未接线清单的诚实应答照旧：SessionLost 家族裸 invalid_params。
    let (status, body) = rpc_call(
        &state,
        Some(&session),
        json!(5),
        "use_session",
        json!({
            "client_session_ref": reference,
            "command": "query",
            "input": {"sql": "SELECT 1"},
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["error"]["code"], json!(-32602));
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| m.starts_with("SessionLost")),
        "{body}"
    );
}
