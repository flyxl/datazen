//! Data Sync 执行面服务层集成测试（W8 刀3e-4a；契约 §6.16 的执行面）。
//!
//! 直驱 [`SyncExecution`]（HTTP 路由随刀3e-4b 点亮，届时补 wired 路由腿；
//! 本刀钉服务层语义）。夹具与 `schema_diff_routes.rs` 的 wired 腿同构：
//! 真 `ServerDriverExecBackend` + 租约源 + 活会话（`open_session` 经真用例
//! 开立）——桌面 `jobs_contract_tests.rs` 的 `pair()` 同款双拼写技巧：
//! 源注册在 `postgres`、目标注册在 `postgresql`（同族两拼写 = 两个注册键
//! = 两个各持不同行集的 MockDriver）。
//!
//! 钉住的语义（桌面 IPC 契约的逐条对齐）：
//!
//! - inspect：配对分类 + 自同步拒绝 + 会话不可见（防枚举同文案）；
//! - compare：durable prepare Job 冻结 ChangeSet → 复核投影（insert 计数）
//!   → SQL 预览有界非空；
//! - execute：受理 → 终态 → `committed`（applied=行数）+ 重放回执（同
//! plan 不二次写）+ 迁移运行历史落账（v12，operation=dataSync）；
//! - revalidate / check_sync_conflicts：复核投影 + 冻结计数比对；
//! - verify_recovery：非 PendingVerification 拒绝；置挂起后核验结论 =
//!   RequireManualReview（证据记录，永不续写）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{SslMode, TableInfo, TableType, Value};
use datazen_server::execution::budget_bridge::BudgetPortBridge;
use datazen_server::execution::driver_exec::ServerDriverExecBackend;
use datazen_server::execution::lease::RuntimeLeaseSource as _;
use datazen_server::execution::lease::ServerRuntimeLeaseSource;
use datazen_server::execution::publisher::execution_result_publisher;
use datazen_server::execution::secret_provider::ServerSecretVault;
use datazen_server::schema_diff::endpoints::SessionEndpoints;
use datazen_server::storage::migration_runs::ServerMigrationRuns;
use datazen_server::storage::sync_assets::{ServerSyncProfiles, ServerSyncTasks};
use datazen_server::storage::{AdminStore, ServerConnections, ServerProfiles};
use datazen_server::sync::execution::{
    SyncExecution, SyncExecutionError, SyncInspectRequest, SyncPrepareRequest, SyncRecoveryRequest,
};
use datazen_server::sync::runtime_state::SyncRuntimeState;

const SERVER_SECRET: &[u8] = b"sync-execution-secret";

/// inventory 工厂随链接期注册：测试二进制必须显式触碰驱动 crate 的符号
/// （dev-dep 只保证在依赖图里，不保证进符号表——链接器会把未引用的
/// 静态注册裁掉；datazen 自己的 `force_link_driver_sync_adapters` 同款）。
#[inline(never)]
fn force_link_sync_adapters() {
    let _ = std::any::type_name::<datazen_driver_postgres::PgSyncAdapter>();
}

/// 桌面 `jobs_contract_tests::mock_options` 同款：一页后断流（否则 handler
/// 见到同一页两次会以「未严格递增」拒绝）+ 参数化写 + 按键等值过滤复核读
/// （否则双读对每个键都答第一行，合法 Insert 会被误判为目标漂移）。
fn sync_mock_options(rows: &[(i64, &str)]) -> MockDriverOptions {
    MockDriverOptions {
        empty_keyset_after_cursor: true,
        parameterized_writes: true,
        filter_rows_by_key_equality: true,
        execute_rows_affected: 1,
        count_total: rows.len() as i64,
        query_rows: rows
            .iter()
            .map(|(id, name)| {
                vec![
                    Some(Value::Integer(*id)),
                    Some(Value::String((*name).to_string())),
                ]
            })
            .collect(),
        databases: vec!["app".into()],
        tables: vec![TableInfo {
            name: "users".into(),
            schema: None,
            table_type: TableType::Table,
            row_count: Some(rows.len() as i64),
        }],
        server_version: "PostgreSQL 16".into(),
        ..Default::default()
    }
}

/// wired 全栈夹具：真执行后端 + 租约源 + 活会话 + sync 执行面服务层。
struct ExecFixture {
    _store: Arc<AdminStore>,
    _admin_path: PathBuf,
    ctx: datazen::shared::context::RequestContext,
    sessions: Arc<datazen::execution::application::RuntimeConnectionUseCases>,
    execution: SyncExecution,
    job_host: Arc<datazen::jobs::JobHost>,
    plans: Arc<dyn datazen::migration::data_sync::plan_store::SyncPlanRepository>,
    runs: Arc<ServerMigrationRuns>,
    tasks: Arc<ServerSyncTasks>,
    source_connection_id: String,
    target_connection_id: String,
}

async fn execution_fixture() -> ExecFixture {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("dz-w8-sync-exec-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = Arc::new(AdminStore::open(&dir.join("admin.sqlite")).expect("file store"));
    store.migrate().expect("migrations apply");

    let ctx = datazen::shared::context::RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        datazen::shared::id::ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-sync-exec"),
        None,
    );
    force_link_sync_adapters();

    // 执行策略闸：principal 需要组织成员身份（schema_diff wired 腿的
    // `register_login` 同款前置，减去登录会话——本测试不走 HTTP 面）。
    datazen_server::policy::PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership(
            "org-1",
            "user-1",
            datazen_server::policy::Role::OrgMember,
            "idp:user-1",
        )
        .expect("membership");

    // 源 = `postgres` 拼写（1 行 alice）；目标 = `postgresql` 拼写（空库 →
    // 全量 Insert）。同族两拼写让两个 MockDriver 各持各的行集。
    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let source_driver = MockDriver::new("postgres", sync_mock_options(&[(1, "alice")]));
    registry
        .register_test_driver("postgres", source_driver.clone())
        .await;
    registry
        .register_test_factory(
            "postgres",
            Box::leak(Box::new(WireMockFactory {
                driver_id: "postgres",
                driver: source_driver.clone(),
            })),
        )
        .await;
    let target_driver = MockDriver::new("postgresql", sync_mock_options(&[]));
    registry
        .register_test_driver("postgresql", target_driver.clone())
        .await;
    registry
        .register_test_factory(
            "postgresql",
            Box::leak(Box::new(WireMockFactory {
                driver_id: "postgresql",
                driver: target_driver.clone(),
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
    for (name, database_type, host) in [
        ("exec src", "postgres", "localhost"),
        ("exec tgt", "postgresql", "target.example.invalid"),
    ] {
        let record = profiles
            .create(
                &ctx,
                datazen::connection::dto::ProfileDraft::new(name, database_type),
                &datazen::shared::id::IdempotencyKey::new(format!("idem-sync-{name}")),
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
                    database_type: database_type.into(),
                    host: Some(host.into()),
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
    let sessions = lease_source.resolve().await.expect("cell runtime resolves");

    let endpoints = SessionEndpoints {
        directory,
        lease: lease_source,
        connections: connections.clone(),
    };

    // sync 执行面装配（bootstrap 同构；比较存储根 = 夹具临时目录）。
    let jobs_repo = Arc::new(datazen_server::storage::ServerJobs::new(
        store.clone(),
        Arc::new(datazen::jobs::domain::time::PortJobClock::new(
            datazen::shared::clock::SystemClock,
        )),
        900,
    ));
    let job_host = Arc::new(datazen::jobs::JobHost::new(
        jobs_repo,
        Arc::new(std::sync::Mutex::new(
            datazen::execution::runtime::budget::BudgetLedger::new(
                datazen::execution::runtime::budget::BudgetConfig::team_default(),
            ),
        )),
        Arc::new(datazen::jobs::domain::time::PortJobClock::new(
            datazen::shared::clock::SystemClock,
        )),
    ));
    let plans: Arc<dyn datazen::migration::data_sync::plan_store::SyncPlanRepository> =
        Arc::new(datazen::migration::data_sync::plan_store::InMemorySyncPlanRepository::new());
    let runs = Arc::new(ServerMigrationRuns::new(store.clone()));
    let sync_profiles = Arc::new(ServerSyncProfiles::new(store.clone()));
    let tasks = Arc::new(ServerSyncTasks::new(store.clone()));
    let comparison_root: Arc<
        dyn datazen::migration::data_sync::comparison_store::ComparisonStorageRoot,
    > = Arc::new(
        datazen::migration::data_sync::comparison_store::LocalComparisonStorageRoot::new(
            dir.join("comparisons"),
            datazen::shared::id::OrganizationId::new("org-1"),
        ),
    );
    let execution = SyncExecution::new(
        job_host.clone(),
        plans.clone(),
        SyncRuntimeState::new(),
        endpoints,
        connections,
        Arc::new(
            datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry::new(
            ),
        ),
        comparison_root,
        runs.clone(),
        sync_profiles,
        tasks.clone(),
    );

    ExecFixture {
        _store: store,
        _admin_path: dir.join("admin.sqlite"),
        ctx,
        sessions,
        execution,
        job_host,
        plans,
        runs,
        tasks,
        source_connection_id,
        target_connection_id,
    }
}

/// schema_diff 夹具同款：按 driver_id 重复出租同一 mock 的测试工厂
/// （`resource_provider` 的 LegacyResourceAdapter 形——open 的会话后端经
/// 工厂解析资源适配器）。
struct WireMockFactory {
    driver_id: &'static str,
    driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
}

impl datazen_driver_api::DatabaseDriverFactory for WireMockFactory {
    fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
        self.driver.clone()
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
                "sync-exec-test",
                1,
                shape,
                capabilities,
            ),
        ))
    }
}

impl ExecFixture {
    /// 经真用例开一条活会话（schema_diff `open_session_on` 同款）。
    async fn open_session_on(&self, connection_id: &str) -> String {
        use datazen::execution::application::idempotency::IdempotentOperation;
        use datazen::execution::application::use_cases::ConnectionUseCases as _;
        use datazen::execution::target::NamespaceTarget;
        use datazen::shared::context::OwnerRef;
        use datazen::shared::id::{ClientInstanceId, EditorSessionId};

        let target = datazen::execution::ExecutionTarget::new(
            datazen::shared::id::ConnectionId::new(connection_id.to_owned()),
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
                datazen::execution::application::OpenSessionRequest::new(
                    target,
                    OwnerRef::Editor {
                        client_instance_id: ClientInstanceId::new("http"),
                        editor_session_id: EditorSessionId::new("sync-exec"),
                    },
                    token.idempotency_key,
                ),
            )
            .await
            .expect("session opens");
        receipt.session.handle.db_session_id.as_str().to_owned()
    }
}

/// 桌面 `insert_selection` 同款：users 表全选 Insert。
fn users_insert_selection(
    revision: u64,
) -> datazen::migration::data_sync::plan_store::SyncRunSelection {
    datazen::migration::data_sync::plan_store::SyncRunSelection {
        revision,
        rows: Vec::new(),
        scopes: vec![
            datazen::migration::data_sync::plan_store::SyncTableSelection {
                source_table: "users".into(),
                target_table: "users".into(),
                selection_mode: datazen::migration::data_sync::plan_store::SyncSelectionMode::All,
                operations: vec![datazen::migration::data_sync::ChangeOperation::Insert],
                excluded_rows: Vec::new(),
            },
        ],
    }
}

fn prepare_request(source: &str, target: &str) -> SyncPrepareRequest {
    SyncPrepareRequest {
        source_db_session_id: source.to_owned(),
        target_db_session_id: target.to_owned(),
        tables: Some(vec!["users".to_owned()]),
        job_id: None,
        source_database: Some("app".to_owned()),
        target_database: Some("app".to_owned()),
        source_schema: None,
        target_schema: None,
        options: None,
        filters: None,
        mappings: None,
    }
}

fn inspect_request(source: &str, target: &str) -> SyncInspectRequest {
    SyncInspectRequest {
        source_db_session_id: source.to_owned(),
        target_db_session_id: target.to_owned(),
        source_database: Some("app".to_owned()),
        target_database: Some("app".to_owned()),
        source_schema: None,
        target_schema: None,
        tables: None,
    }
}

fn assert_validation(error: SyncExecutionError, fragment: &str) {
    match error {
        SyncExecutionError::Validation(message) => {
            assert!(
                message.contains(fragment),
                "validation message {message:?} should contain {fragment:?}"
            );
        }
        other => panic!("expected Validation, got {other:?}"),
    }
}

// ── inspect ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn inspect_classifies_tables_and_refuses_self_sync_and_invisible_sessions() {
    let fixture = execution_fixture().await;
    let source = fixture.open_session_on(&fixture.source_connection_id).await;
    let target = fixture.open_session_on(&fixture.target_connection_id).await;

    let results = fixture
        .execution
        .inspect(&fixture.ctx, &inspect_request(&source, &target))
        .await
        .expect("inspect succeeds");
    let users = results
        .iter()
        .find(|table| table.source_table == "users")
        .expect("users table is inspected");
    assert_eq!(
        users.status,
        datazen::migration::data_sync::TableMappingStatus::Matched
    );

    // 自同步拒绝（同一会话两侧）。
    let error = fixture
        .execution
        .inspect(&fixture.ctx, &inspect_request(&source, &source))
        .await
        .expect_err("self-sync is refused");
    assert_validation(error, "self-sync");

    // 未知/越组织会话：防枚举统一文案（sessions 域裁决）。
    let error = fixture
        .execution
        .inspect(&fixture.ctx, &inspect_request("no-such-session", &target))
        .await
        .expect_err("unknown session is refused");
    assert_validation(error, "does not exist or is not visible");
}

// ── compare / SQL 预览 ──────────────────────────────────────────────────────

#[tokio::test]
async fn compare_freezes_change_set_and_sql_preview_stays_bounded() {
    let fixture = execution_fixture().await;
    let source = fixture.open_session_on(&fixture.source_connection_id).await;
    let target = fixture.open_session_on(&fixture.target_connection_id).await;

    let preview = fixture
        .execution
        .compare(&fixture.ctx, &prepare_request(&source, &target))
        .await
        .expect("compare reaches the review projection");
    assert!(!preview.plan_id.is_empty());
    assert_eq!(preview.selection_revision, 1);
    let users = preview
        .tables
        .iter()
        .find(|table| table.source_table == "users")
        .expect("users table is compared");
    assert_eq!(users.insert_count, 1, "alice is missing on the target");
    assert_eq!(users.update_count, 0);
    assert_eq!(users.delete_count, 0);

    let statements = fixture
        .execution
        .generate_sql(
            &fixture.ctx,
            &preview.plan_id,
            users_insert_selection(preview.selection_revision),
            datazen::migration::data_sync::SyncOptions::default(),
        )
        .await
        .expect("SQL preview generates");
    assert!(
        !statements.is_empty(),
        "the reviewed Insert produces at least one statement"
    );

    // 陈旧选区版本 → 拒绝（回比较面）。
    let error = fixture
        .execution
        .generate_sql(
            &fixture.ctx,
            &preview.plan_id,
            users_insert_selection(preview.selection_revision + 1),
            datazen::migration::data_sync::SyncOptions::default(),
        )
        .await
        .expect_err("stale selection is refused");
    assert_validation(error, "selection revision is stale");
}

// ── execute（受理 → 终态 → 回执重放 → 历史落账）──────────────────────────

#[tokio::test]
async fn execute_applies_selection_replays_receipt_and_records_history() {
    let fixture = execution_fixture().await;
    let source = fixture.open_session_on(&fixture.source_connection_id).await;
    let target = fixture.open_session_on(&fixture.target_connection_id).await;

    let preview = fixture
        .execution
        .compare(&fixture.ctx, &prepare_request(&source, &target))
        .await
        .expect("compare freezes the reviewed plan");

    let request = datazen::migration::data_sync::plan_store::SyncRunRequest {
        plan_id: preview.plan_id.clone(),
        selection: users_insert_selection(preview.selection_revision),
        options: datazen::migration::data_sync::SyncOptions::default(),
        job_id: None,
    };
    let response = fixture
        .execution
        .execute(&fixture.ctx, request.clone(), None)
        .await
        .expect("execute reaches a terminal projection");
    assert_eq!(
        response.outcome,
        datazen::migration::data_sync::ExecutionOutcome::Committed
    );
    assert_eq!(response.result.applied, 1, "alice was applied");
    assert!(!response.result.rolled_back);

    // 重放：consumed-plan 回执指向既有 Job——不二次写、不造第二段历史。
    let replay = fixture
        .execution
        .execute(&fixture.ctx, request, None)
        .await
        .expect("replay resolves the durable receipt");
    assert_eq!(
        replay.outcome,
        datazen::migration::data_sync::ExecutionOutcome::Committed
    );

    // 迁移运行历史（v12，operation=dataSync）：running 行被终态覆写。
    let filter = datazen::history::MigrationRunFilter {
        operation: Some("dataSync".into()),
        status: None,
        profile_id: None,
        connection_id: None,
    };
    let page = fixture
        .runs
        .list("org-1", filter, 0, 10)
        .await
        .expect("history lists");
    assert_eq!(page.items.len(), 1, "exactly one dataSync run is recorded");
    assert_eq!(page.items[0].status, "completed");
    assert_eq!(page.items[0].committed_count, 1);
    assert_eq!(page.items[0].outcome, "committed");
}

// ── revalidate / check_sync_conflicts ──────────────────────────────────────

#[tokio::test]
async fn revalidate_and_check_conflicts_project_task_state() {
    let fixture = execution_fixture().await;
    let source = fixture.open_session_on(&fixture.source_connection_id).await;
    let target = fixture.open_session_on(&fixture.target_connection_id).await;

    // 结构未漂移 → 无过期表。
    let report = fixture
        .execution
        .revalidate(&fixture.ctx, &inspect_request(&source, &target))
        .await
        .expect("revalidate re-runs the inspect gates");
    assert_eq!(
        report
            .get("staleTables")
            .and_then(serde_json::Value::as_array),
        Some(&Vec::new()),
        "nothing drifted: {report}"
    );

    // 冲突检查：冻结计数 0 ≠ 当前源侧 1 行 → 冲突。
    let task = datazen::migration::data_sync::task_checkpoint::StoredSyncTask {
        id: "task-conflict".into(),
        source_db_session_id: String::new(),
        target_db_session_id: String::new(),
        source_connection_id: fixture.source_connection_id.clone(),
        target_connection_id: fixture.target_connection_id.clone(),
        source_database: Some("app".into()),
        target_database: Some("app".into()),
        source_schema: None,
        target_schema: None,
        tables: vec!["users".into()],
        completed_tables: Vec::new(),
        current_table: None,
        current_table_offset: 0,
        source_row_counts: HashMap::from([("users".to_owned(), 0u64)]),
        strategy: "full".into(),
        status: "paused".into(),
        error_message: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        resume_state: "unknown".into(),
    };
    let scope = datazen::shared::scope::OrganizationScope {
        organization_id: datazen::shared::id::OrganizationId::new("org-1"),
    };
    fixture
        .tasks
        .save(&scope, task)
        .await
        .expect("task persists");

    let conflicts = fixture
        .execution
        .check_conflicts(&fixture.ctx, "task-conflict")
        .await
        .expect("conflict check reads the frozen counts");
    assert_eq!(
        conflicts
            .get("hasConflicts")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "row count drifted: {conflicts}"
    );
    let entries = conflicts
        .get("conflicts")
        .and_then(serde_json::Value::as_array)
        .expect("conflict entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].get("table").and_then(serde_json::Value::as_str),
        Some("users")
    );

    // 未知任务 → 404 桶（统一 NotFound）。
    let error = fixture
        .execution
        .check_conflicts(&fixture.ctx, "no-such-task")
        .await
        .expect_err("unknown task is not found");
    assert!(matches!(error, SyncExecutionError::NotFound(_)));
}

// ── verify_recovery ────────────────────────────────────────────────────────

#[tokio::test]
async fn verify_recovery_requires_pending_state_and_records_manual_review() {
    let fixture = execution_fixture().await;
    let source = fixture.open_session_on(&fixture.source_connection_id).await;
    let target = fixture.open_session_on(&fixture.target_connection_id).await;

    // 受理一个 apply Job 但不派发（真实挂起路径的前半——dispatch 失败时
    // `mark_pending_verification` 会把非终态 Job 落成 failed + 挂起原因 +
    // PendingVerification 恢复行）。
    let job_id =
        datazen::shared::id::JobId::new(format!("data-sync-apply-{}", uuid::Uuid::new_v4()));
    let definition = datazen::jobs::domain::job::JobDefinition {
        job_id: job_id.clone(),
        organization_id: fixture.ctx.organization_id.clone(),
        kind: "dataSyncApply".into(),
        owner: datazen::shared::context::OwnerRef::ClientSession {
            client_instance_id: fixture.ctx.client_instance_id.clone(),
            purpose: "data-sync:dataSyncApply".into(),
        },
        payload: serde_json::json!({
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
            "planId": "plan-recovery",
            "consumedPlanId": "plan-recovery",
            "selectionRevision": 1,
        }),
        created_at: fixture.job_host.now(),
    };
    fixture
        .job_host
        .ensure_endpoint_services(&[])
        .expect("no endpoints to register");
    fixture
        .job_host
        .accept(
            &fixture.ctx,
            definition,
            &datazen::shared::id::IdempotencyKey::new(format!("data-sync-apply:plan-recovery")),
        )
        .await
        .expect("apply job accepted");

    // 挂起前：Queued 不是恢复候选 → 拒绝。
    let recovery_request = SyncRecoveryRequest {
        job_id: job_id.as_str().to_owned(),
        source_db_session_id: source.clone(),
        target_db_session_id: target.clone(),
        source_database: "app".into(),
        target_database: "app".into(),
        source_schema: None,
        target_schema: None,
    };
    let error = fixture
        .execution
        .verify_recovery(&fixture.ctx, &recovery_request)
        .await
        .expect_err("a queued job is not a recovery candidate");
    assert_validation(error, "not awaiting recovery verification");

    // 置挂起（durable 证据通道）：核验走只读裁决——受理体没有登记
    // recoveryTargets，端点范围不匹配，结论是 RequireManualReview 而非续写。
    fixture
        .job_host
        .repository()
        .mark_pending_verification(&fixture.ctx, &job_id, "dispatchNeedsVerification")
        .await
        .expect("pending verification is recorded");
    let details = fixture
        .execution
        .verify_recovery(&fixture.ctx, &recovery_request)
        .await
        .expect("recovery verification is read-only");
    let verdict = details
        .recovery
        .as_ref()
        .expect("recovery evidence is attached");
    assert_eq!(
        verdict.verdict,
        datazen::jobs::domain::job::JobRecoveryVerdict::RequireManualReview
    );
    assert_eq!(
        verdict.reason_code.as_deref(),
        Some("reauthorizedEndpointScopeMismatch")
    );

    // 非本域 kind 的 job 不是恢复候选（404 桶，统一文案）。
    let other_kind =
        datazen::shared::id::JobId::new(format!("schemaDiffApply-{}", uuid::Uuid::new_v4()));
    let _ = fixture
        .job_host
        .accept(
            &fixture.ctx,
            datazen::jobs::domain::job::JobDefinition {
                job_id: other_kind.clone(),
                organization_id: fixture.ctx.organization_id.clone(),
                kind: "schemaDiffApply".into(),
                owner: datazen::shared::context::OwnerRef::ClientSession {
                    client_instance_id: fixture.ctx.client_instance_id.clone(),
                    purpose: "test".into(),
                },
                payload: serde_json::json!({}),
                created_at: fixture.job_host.now(),
            },
            &datazen::shared::id::IdempotencyKey::new(format!("other-{other_kind}")),
        )
        .await;
    let error = fixture
        .execution
        .verify_recovery(
            &fixture.ctx,
            &SyncRecoveryRequest {
                job_id: other_kind.as_str().to_owned(),
                source_db_session_id: source,
                target_db_session_id: target,
                source_database: "app".into(),
                target_database: "app".into(),
                source_schema: None,
                target_schema: None,
            },
        )
        .await
        .expect_err("other-kind job is not a sync recovery candidate");
    assert!(matches!(error, SyncExecutionError::NotFound(_)));
}

// ── 跨组织隔离 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn another_organization_finds_no_plan_artifact_or_selection() {
    let fixture = execution_fixture().await;
    let source = fixture.open_session_on(&fixture.source_connection_id).await;
    let target = fixture.open_session_on(&fixture.target_connection_id).await;

    let preview = fixture
        .execution
        .compare(&fixture.ctx, &prepare_request(&source, &target))
        .await
        .expect("compare freezes the reviewed plan");

    let other = datazen::shared::context::RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-2"),
        datazen::shared::id::PrincipalId::new("user-2"),
        None,
        datazen::shared::id::ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-sync-exec-other"),
        None,
    );
    // 越组织 peek 按「不存在」处理（CM-05：与真不存在同文案，不泄漏存在性）。
    let error = fixture
        .execution
        .generate_sql(
            &other,
            &preview.plan_id,
            users_insert_selection(preview.selection_revision),
            datazen::migration::data_sync::SyncOptions::default(),
        )
        .await
        .expect_err("cross-org plan access fails closed");
    assert_validation(error, "unknown or has expired");
}
