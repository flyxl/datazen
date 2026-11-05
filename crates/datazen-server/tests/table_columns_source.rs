//! W8 刀1：[`ServerTableColumnSource`]（命令面列源备料件）的集成测试（t1–t6）。
//!
//! 装配沿 `table_data_routes.rs` 的 `wired_fixture` 先例：真 SQLite 管理库
//! （v1..=v9 迁移）+ `WireMockFactory`（MockDriver + `LegacyResourceAdapter`
//! 静态工厂）+ `ServerDriverExecBackend` + 真 `ServerResourceExecutionHost`
//! （prepare 闸序 = profile 存在 → 未禁用 → revision CAS → 连接配置 →
//! 驱动可用 → 目标规范化）+ `ServerTargetRunner` target 分支。变更二端点
//! 本身 fail-closed 501（刀3 才接线），因此用例在**组件边界**直接调用
//! `table_columns`——prepare → token → execute → poll → collect 的执行栈
//! 全部为真件；t7（8 条 501 探针不改一字）由既有
//! `table_data_routes::driver_bound_table_data_routes_fail_closed_as_not_implemented`
//! 探针照旧覆盖，本文件不重复。
//!
//! 覆盖面（设计档 §2.6）：
//! - **t1 命中路径**：预录含主键 schema → 全栈返回 `CachedColumns`，列序 /
//!   data_type / nullable / primary_keys 逐字段断言（主键 `primaryKeys`
//!   字段优先一条腿 + `is_primary_key` 列标记回退一条腿）。
//! - **t2 空列显式报错**：0 列应答 → `Validation`，文案含表名（端口硬规则
//!   「空答案 ≠ 无主键」的可执行钉）。
//! - **t3 schema 级过滤**：`has_schema_level() == false` 的驱动 + 上下文带
//!   schema → 命令 input 不携带 `schema` 键（录音 runner 直接观察；反向腿：
//!   schema-aware 驱动全栈收到 schema 并成功——丢弃不是无脑抹除）。
//! - **t4 未知驱动** → `NotConfigured("Driver unavailable")`。
//! - **t5 revision CAS 漂移** → `prepare` 拒绝 → `Validation`
//!   （`ConfigRevisionMismatch` 桶经 `From<ApiError>` 映射）。
//! - **t6 产物形状防御**：data.schema 缺失 / outputs 无 commandResult 块 /
//!   空 outputs → `Validation("Table schema unavailable for '<table>'…")`，
//!   绝不出「看似成功」的空列答案。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value as Json};

use datazen::connection::domain::catalog::{ConnectionRepository as _, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository as _;
use datazen::connection::dto::ProfileDraft;
use datazen::execution::application::orchestration::RuntimeResultSink;
use datazen::execution::runtime::directory::InMemorySessionDirectory;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::shared::clock::SystemClock;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{
    ClientInstanceId, Counter, IdempotencyKey, OrganizationId, PrincipalId, RequestId,
};
use datazen::table_browsing::api::{TargetBinding, TargetCommandRunner};
use datazen::table_editing::change_set::RowChangeTableContext;
use datazen::table_editing::{TableColumnSource, TableEditingError};
use datazen_driver_api::capabilities;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{
    ColumnSchema, ConnectionConfig, DatabaseDriver, DatabaseDriverFactory, SslMode, TableSchema,
};
use datazen_server::execution::budget::ServerBudgetCoordinator;
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::ServerRuntimeLeaseSource;
use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::execution::table_columns::ServerTableColumnSource;
use datazen_server::execution::target_runner::ServerTargetRunner;
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::{AdminStore, ServerConnections, ServerProfiles, ServerSettingsStore};

const SERVER_SECRET: &[u8] = b"w8-colsrc-server-secret-32-bytes!";

// ── 夹具（table_data_routes.rs 的 wired_fixture 同构，去 HTTP 面）──────────

/// 把 [`MockDriver`] 连同 [`LegacyResourceAdapter`] 包成静态工厂
/// （`register_test_factory` 吃 `&'static dyn DatabaseDriverFactory`；
/// `schema_level` 决定命名空间形状是否声明 Schema 层——schema-less 驱动
/// 连形状都不携带 schema 层，`reject_unknown_layers` 才不会误伤）。
struct WireMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn DatabaseDriver>,
    schema_level: bool,
}

impl DatabaseDriverFactory for WireMockFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::clone(&self.driver)
    }

    fn driver_id(&self) -> &'static str {
        self.driver_id
    }

    fn resource_provider(&self) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
        let mut levels = vec![
            datazen_driver_api::namespace::NamespaceLevel::present(
                datazen_driver_api::namespace::NamespaceLevelKind::Database,
                false,
            ),
            datazen_driver_api::namespace::NamespaceLevel::absent(
                datazen_driver_api::namespace::NamespaceLevelKind::Catalog,
            ),
        ];
        if self.schema_level {
            levels.push(datazen_driver_api::namespace::NamespaceLevel::present(
                datazen_driver_api::namespace::NamespaceLevelKind::Schema,
                false,
            ));
        }
        let shape = datazen_driver_api::namespace::NamespaceShape {
            levels,
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
            "colsrc-test",
            1,
            shape,
            capabilities,
        )))
    }
}

/// 全栈夹具产物：真 runner（prepare 闸序 → token → execute → poll →
/// collect）+ 驱动注册表 + 请求上下文 + 夹具已读的 profile config_revision。
struct ColumnSourceFixture {
    registry: Arc<DriverRegistry>,
    runner: Arc<dyn TargetCommandRunner>,
    ctx: RequestContext,
    /// profile 创建时铸造的 config_revision（happy path 的 CAS 基准）。
    revision: Counter,
    connection_id: String,
}

impl ColumnSourceFixture {
    /// 组件铸造：与刀3 handler 调用点同款组装（runner/registry 同实例，
    /// ctx 是请求真主体，revision 是已读 config_revision）。
    fn source(&self) -> ServerTableColumnSource {
        ServerTableColumnSource::new(
            self.runner.clone(),
            self.registry.clone(),
            self.ctx.clone(),
            self.revision,
        )
    }
}

async fn fixture(opts: MockDriverOptions, schema_level: bool) -> ColumnSourceFixture {
    let dir: PathBuf = std::env::temp_dir().join(format!("dz-w8-colsrc-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply v1..=v9");

    // 授权引擎的角色基线（authorize 只查 membership，无 cookie 链——组件
    // 边界直调，ctx 即请求真主体）。
    PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership");

    let ctx = RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("http"),
        RequestId::new("req-w8-colsrc"),
        None,
    );

    let registry = Arc::new(DriverRegistry::new());
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
                schema_level,
            })),
        )
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    let record = profiles
        .create(
            &ctx,
            ProfileDraft::new("wire-conn", "mockdb"),
            &IdempotencyKey::new("idem-w8-colsrc-profile"),
        )
        .await
        .expect("profile create");
    let connection_id = record.connection_id.as_str().to_owned();
    let revision = record.config_revision.counter();
    let scope = ConnectionScope::new("org-1");
    connections
        .save(
            &scope,
            ConnectionConfig {
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
    let policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy engine")),
        store.clone(),
        Arc::new(SystemClock),
    ));
    let directory = Arc::new(InMemorySessionDirectory::new());
    // 资源宿主持真租约源：prepare 的 resolve 自洽（闸序 + 用例面同源）。
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
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        real_lease.clone(),
        "org-1",
    ));
    let runner: Arc<dyn TargetCommandRunner> = Arc::new(ServerTargetRunner::new(host, real_lease));

    ColumnSourceFixture {
        registry,
        runner,
        ctx,
        revision,
        connection_id,
    }
}

/// 轻量夹具：只装配驱动注册表（组件前两步消费面），runner 由用例自带
/// （录音桩，见 [`RecordingRunner`]）。
async fn registry_with(opts: MockDriverOptions) -> Arc<DriverRegistry> {
    let registry = Arc::new(DriverRegistry::new());
    registry
        .register_test_driver("mockdb", MockDriver::new("mockdb", opts))
        .await;
    registry
}

// ── 观察桩与应答铸造 ─────────────────────────────────────────────────────

/// 录音桩 runner：捕获 `(command, input, binding 形状)` 并回放预录 outputs。
/// 组件步 3/4 的直接观察面（真实驱动无法回看命令 input 的键形状）。
struct RecordingRunner {
    captured: Mutex<Vec<RunRecord>>,
    answer: Mutex<Vec<Json>>,
}

struct RunRecord {
    command: String,
    input: Json,
    object: Option<(String, String)>,
    namespace_schema: Option<String>,
    manual_handle_present: bool,
    revision: u64,
}

impl RecordingRunner {
    fn new(answer: Vec<Json>) -> Self {
        Self {
            captured: Mutex::new(Vec::new()),
            answer: Mutex::new(answer),
        }
    }
}

#[async_trait]
impl TargetCommandRunner for RecordingRunner {
    async fn run(
        &self,
        _context: &RequestContext,
        binding: &TargetBinding,
        command: &str,
        input: Json,
    ) -> Result<Vec<Json>, datazen::shared::error::ApiError> {
        self.captured.lock().expect("captured").push(RunRecord {
            command: command.to_owned(),
            input,
            object: binding
                .target
                .object
                .as_ref()
                .map(|object| (object.kind.clone(), object.name.clone())),
            namespace_schema: binding.target.namespace.schema.clone(),
            manual_handle_present: binding.manual_handle.is_some(),
            revision: binding.revision.get(),
        });
        Ok(self.answer.lock().expect("answer").clone())
    }
}

/// 真实产物形状：非 query 命令的产物是 `{"kind":"commandResult","data":
/// <CommandResult.data>}`，`get_table_schema` 的 `data = {"schema": <TableSchema>}`
/// （`resource_adapter.rs` + `schema_catalog_commands.rs` 的既有形状）。
fn command_result_output(schema: &TableSchema) -> Json {
    json!({
        "kind": "commandResult",
        "data": { "schema": serde_json::to_value(schema).expect("schema serializes") },
    })
}

fn column(name: &str, data_type: &str, nullable: bool, pk: bool, autoinc: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable,
        default_value: None,
        comment: None,
        is_primary_key: pk,
        is_auto_increment: autoinc,
    }
}

/// t1 的预录 schema：`primaryKeys` 字段优先路径（["id"] 与列标记一致）。
fn schema_with_primary_keys_field() -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: vec![
            column("id", "integer", false, true, true),
            column("email", "varchar", false, false, false),
            column("note", "text", true, false, false),
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

/// t1 回退腿的预录 schema：`primaryKeys` 字段为空，有效主键回退
/// `is_primary_key` 列标记（`TableSchema::effective_primary_keys` 规则）。
fn schema_with_marked_columns_only() -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: vec![
            column("id", "integer", false, true, false),
            column("email", "varchar", false, false, false),
        ],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

/// t2 的预录 schema：0 列（驱动应答形状合法但列集为空）。
fn empty_columns_schema() -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: vec![],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn fixture_ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("http"),
        RequestId::new("req-w8-colsrc-light"),
        None,
    )
}

/// 变更上下文的最小合法体（`validate_table_context` 已保证 database 非空；
/// 列源不再兜底）。
fn table_context(connection_id: &str, driver_type: &str, schema: Option<&str>) -> RowChangeTableContext {
    RowChangeTableContext {
        connection_id: connection_id.to_owned(),
        db_session_id: "wire-change-session".to_owned(),
        driver_type: driver_type.to_owned(),
        database: Some("app".to_owned()),
        schema: schema.map(str::to_owned),
        table: "users".to_owned(),
    }
}

fn source_of(
    runner: Arc<dyn TargetCommandRunner>,
    registry: Arc<DriverRegistry>,
    revision: Counter,
) -> ServerTableColumnSource {
    ServerTableColumnSource::new(runner, registry, fixture_ctx(), revision)
}

// ── t1：命中路径 ─────────────────────────────────────────────────────────

/// 预录含主键 schema → 全栈（prepare 闸序 → token → execute → poll →
/// collect → 解析 → 映射）返回 `CachedColumns`：列序 / data_type / nullable /
/// auto_increment 逐字段一致，primary_keys 走 `primaryKeys` 字段优先，
/// table_name 与请求表名一致。
#[tokio::test]
async fn t1_columns_arrive_field_by_field_through_the_full_stack() {
    let mut opts = MockDriverOptions::default();
    opts.table_schema = Some(schema_with_primary_keys_field());
    let fx = fixture(opts, false).await;
    let columns = fx
        .source()
        .table_columns(&table_context(&fx.connection_id, "mockdb", None))
        .await
        .expect("the schema round trip answers columns");

    assert_eq!(columns.table_name, "users", "table name rides through");
    let names: Vec<&str> = columns.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "email", "note"], "column order preserved");
    assert_eq!(columns.columns[0].data_type, "integer");
    assert!(!columns.columns[0].nullable);
    assert!(columns.columns[0].is_auto_increment);
    assert_eq!(columns.columns[1].data_type, "varchar");
    assert!(!columns.columns[1].nullable);
    assert!(columns.columns[2].nullable, "note is nullable");
    assert_eq!(
        columns.primary_keys,
        vec!["id".to_owned()],
        "the primaryKeys field wins"
    );
}

/// t1 回退腿：`primaryKeys` 字段为空 → 有效主键回退 `is_primary_key` 列标记
/// （driver-api 既有规则，列源不得另立一套）。
#[tokio::test]
async fn t1_primary_keys_fall_back_to_column_marks() {
    let mut opts = MockDriverOptions::default();
    opts.table_schema = Some(schema_with_marked_columns_only());
    let fx = fixture(opts, false).await;
    let columns = fx
        .source()
        .table_columns(&table_context(&fx.connection_id, "mockdb", None))
        .await
        .expect("the marked column becomes the primary key");
    assert_eq!(columns.primary_keys, vec!["id".to_owned()]);
}

// ── t2：空列显式报错 ─────────────────────────────────────────────────────

/// 驱动应答 0 列 → `Validation` 且文案含表名：空答案不等于「没有主键」
/// （端口硬规则的可执行钉）。
#[tokio::test]
async fn t2_an_empty_column_answer_is_an_explicit_validation_error() {
    let mut opts = MockDriverOptions::default();
    opts.table_schema = Some(empty_columns_schema());
    let fx = fixture(opts, false).await;
    let error = fx
        .source()
        .table_columns(&table_context(&fx.connection_id, "mockdb", None))
        .await
        .expect_err("an empty answer must be an explicit error");
    match error {
        TableEditingError::Validation(message) => {
            assert!(message.contains("users"), "table name in message: {message}");
            assert!(
                message.contains("no columns"),
                "the reason names the empty column set: {message}"
            );
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

// ── t3：schema 级过滤 ────────────────────────────────────────────────────

/// `has_schema_level() == false` 的驱动 + 上下文带 schema → 命令 input 不携带
/// `schema` 键（录音 runner 直接观察键形状；绑定命名空间的 schema 层同样
/// 随过滤为 None——`public.users` 在 MySQL 不是东西）。
#[tokio::test]
async fn t3_a_schema_argument_is_dropped_for_a_schema_less_driver() {
    let opts = {
        let mut opts = MockDriverOptions::default();
        opts.has_schema_level = false;
        opts
    };
    let registry = registry_with(opts).await;
    let runner = Arc::new(RecordingRunner::new(vec![command_result_output(
        &schema_with_primary_keys_field(),
    )]));
    let source = source_of(runner.clone(), registry, Counter::ZERO);
    let columns = source
        .table_columns(&table_context("conn-1", "mockdb", Some("public")))
        .await
        .expect("the filtered schema-less call succeeds");

    assert_eq!(columns.primary_keys, vec!["id".to_owned()]);
    let captured = runner.captured.lock().expect("captured");
    assert_eq!(captured.len(), 1, "one command ran");
    let record = &captured[0];
    assert_eq!(record.command, "get_table_schema");
    assert!(
        record.input.get("schema").is_none(),
        "the schema key must be absent for a schema-less driver: {}",
        record.input
    );
    assert_eq!(record.input["table"], "users");
    assert_eq!(record.input["database"], "app");
    assert_eq!(
        record.revision,
        Counter::ZERO.get(),
        "the binding rides the caller's read revision"
    );
    assert_eq!(
        record.namespace_schema, None,
        "the target namespace carries no schema either"
    );
    // 步 3 的绑定形状：object target（get_table_schema 的硬准入要求）+
    // manual_handle = None（命令面不解析会话）。
    assert_eq!(
        record.object,
        Some(("table".to_owned(), "users".to_owned()))
    );
    assert!(!record.manual_handle_present, "command face has no session");
}

/// 反向腿：schema-aware 驱动 + 上下文带 schema → 全栈成功（schema 原样
/// 送达——mock 的 ExactSchema 校验会在 schema 缺失时拒绝，成功即证明键在）。
#[tokio::test]
async fn t3_a_schema_aware_driver_receives_the_context_schema_end_to_end() {
    let mut opts = MockDriverOptions::default();
    opts.has_schema_level = true;
    opts.table_schema = Some(schema_with_primary_keys_field());
    let fx = fixture(opts, true).await;
    let columns = fx
        .source()
        .table_columns(&table_context(&fx.connection_id, "mockdb", Some("public")))
        .await
        .expect("a schema-aware driver gets the context schema");
    assert_eq!(columns.primary_keys, vec!["id".to_owned()]);
}

// ── t4：未知驱动 ─────────────────────────────────────────────────────────

/// registry 无该 `driver_type` → `NotConfigured("Driver unavailable")`
/// （配置事实，不是调用方的输入错误）。
#[tokio::test]
async fn t4_an_unknown_driver_answers_not_configured() {
    let fx = fixture(MockDriverOptions::default(), false).await;
    let error = fx
        .source()
        .table_columns(&table_context(&fx.connection_id, "nosuchdb", None))
        .await
        .expect_err("an unknown driver must refuse");
    match error {
        TableEditingError::NotConfigured(message) => assert_eq!(message, "Driver unavailable"),
        other => panic!("expected NotConfigured, got {other:?}"),
    }
}

// ── t5：revision CAS 漂移 ────────────────────────────────────────────────

/// binding.revision 与 profile 现值漂移 → `prepare` 的 CAS 拒绝 →
/// `From<ApiError>` 映射进 `Validation`（`ConfigRevisionMismatch` 桶）。
#[tokio::test]
async fn t5_a_revision_drift_is_refused_as_a_validation_error() {
    let fx = fixture(MockDriverOptions::default(), false).await;
    let drifted = Counter::new(fx.revision.get() + 1);
    let source = ServerTableColumnSource::new(
        fx.runner.clone(),
        fx.registry.clone(),
        fx.ctx.clone(),
        drifted,
    );
    let error = source
        .table_columns(&table_context(&fx.connection_id, "mockdb", None))
        .await
        .expect_err("a stale revision must fail closed");
    match error {
        TableEditingError::Validation(message) => assert!(
            message.contains("connection configuration changed"),
            "the prepare CAS copy rides the mapped error: {message}"
        ),
        other => panic!("expected Validation (CAS bucket), got {other:?}"),
    }
}

// ── t6：产物形状防御 ─────────────────────────────────────────────────────

/// `data.schema` 缺失（命令应答形状漂移）→ `Validation("Table schema
/// unavailable for '<table>'…")`，绝不出「看似成功」的空列答案。
#[tokio::test]
async fn t6_a_command_result_without_a_schema_payload_is_a_validation_error() {
    let mut opts = MockDriverOptions::default();
    opts.command_results
        .insert("get_table_schema".to_owned(), json!({ "unrelated": true }));
    let fx = fixture(opts, false).await;
    let error = fx
        .source()
        .table_columns(&table_context(&fx.connection_id, "mockdb", None))
        .await
        .expect_err("a schema-less payload must refuse");
    match error {
        TableEditingError::Validation(message) => assert!(
            message.contains("Table schema unavailable for 'users'"),
            "the desktop-family copy with the table name: {message}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}

/// outputs 无 `commandResult` 块（另一种形状漂移）与空 outputs 序列 → 同族
/// `Validation`。
#[tokio::test]
async fn t6_an_output_without_a_command_result_block_is_a_validation_error() {
    let registry = registry_with(MockDriverOptions::default()).await;
    let runner = Arc::new(RecordingRunner::new(vec![json!({
        "kind": "rows",
        "rows": [],
    })]));
    let source = source_of(runner.clone(), registry.clone(), Counter::ZERO);
    let error = source
        .table_columns(&table_context("conn-1", "mockdb", None))
        .await
        .expect_err("a rows-kind output is not a table schema");
    match error {
        TableEditingError::Validation(message) => assert!(
            message.contains("Table schema unavailable for 'users'"),
            "{message}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }

    // 空 outputs 序列同款（无任何产物块）。
    *runner.answer.lock().expect("answer") = Vec::new();
    let error = source
        .table_columns(&table_context("conn-1", "mockdb", None))
        .await
        .expect_err("no outputs at all is not an answer either");
    match error {
        TableEditingError::Validation(message) => assert!(
            message.contains("Table schema unavailable for 'users'"),
            "{message}"
        ),
        other => panic!("expected Validation, got {other:?}"),
    }
}
