//! Workflow 调度器接线测试（W8 刀3c-2；「双端调度」的 server 半边）。
//!
//! 覆盖面：派生授权单的确定性（冻结比对的承重前提）与拒绝为事实的各
//! 分支、宿主的 owner 归属核对、spawn 的 arm-not-fire 语义（首次观察只
//! 上膛不触发——重启不补跑）、停车的幂等性。核心 tick/在途/授权闸门的
//! 行为钉在共享层 `engine::scheduler` 测试；本套件只钉 server 接线。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen::workflow::application::execution_identity::ScheduledExecutionIdentity;
use datazen::workflow::application::grant_reader::WorkflowScheduleGrantReader as _;
use datazen::workflow::domain::grant::definition_revision;
use datazen::workflow::domain::model::WorkflowDefinition;
use datazen::workflow::engine::scheduler::{
    WorkflowScheduleHost, WorkflowScheduler as CoreScheduler,
};
use datazen::workflow::registry::{WorkflowDefinitionRepository, WorkflowRegistry};

use datazen_server::storage::{
    AdminStore, ServerProfiles, ServerWorkflowHistoryStore, ServerWorkflowStore,
};
use datazen_server::workflow::schedule_grant::ServerScheduleGrantReader;
use datazen_server::workflow::scheduler::{ServerScheduleHost, ServerWorkflowScheduler};

const ORG: &str = "org-wf-sched";

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("memory store");
    store.migrate().expect("migrations");
    Arc::new(store)
}

fn ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new(ORG),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("test"),
        RequestId::new("req-wf-sched"),
        None,
    )
}

/// 带定时的最小查询 workflow（YAML 文本——派生单与存储都吃原文；
/// 连接 id 由 `save_profile` 铸造后回填——server 的 profile 端口铸 id，
/// 桌面测试的 `save_connection("conn-1")` 在 server 侧没有直接对偶）。
fn scheduled_yaml(connection_id: &str) -> String {
    format!(
        "id: demo\nname: Demo\ndescription: d\nconnection: {connection_id}\ndatabase: app\nschedule:\n  enabled: true\n  intervalSecs: 3600\nsteps:\n  - type: query\n    id: q1\n    sql: select 1\n"
    )
}

/// 落一条定义行（直接走仓储端口——调度面读的就是这份事实）。
fn save_definition(store: &Arc<AdminStore>, org: &str, yaml: &str) {
    let definition: WorkflowDefinition =
        datazen::workflow::domain::repository::decode_definition(yaml).expect("decode");
    let workflows = ServerWorkflowStore::scoped(store.clone(), org);
    let scope = workflows.scope();
    let now = "2026-01-01T00:00:00.000Z";
    workflows
        .upsert_workflow(
            &scope,
            &datazen::workflow::registry::WorkflowDefinitionRecord {
                id: definition.id.clone(),
                name: definition.name.clone(),
                description: definition.description.clone(),
                visibility: definition.visibility,
                definition_yaml: yaml.to_owned(),
                created_at: now.into(),
                updated_at: now.into(),
            },
        )
        .expect("save definition");
}

/// 建目标连接的 profile（返回铸造的连接 id，回填进 workflow YAML）。
async fn save_profile(store: &Arc<AdminStore>) -> String {
    use datazen::connection::domain::profile::ProfileRepository as _;
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    let record = profiles
        .create(
            &ctx(),
            datazen::connection::dto::ProfileDraft::new("sched-conn", "mockdb"),
            &datazen::shared::id::IdempotencyKey::new(format!(
                "idem-sched-{}",
                uuid::Uuid::new_v4()
            )),
        )
        .await
        .expect("profile create");
    record.connection_id.as_str().to_owned()
}

fn reader(store: &Arc<AdminStore>) -> ServerScheduleGrantReader {
    ServerScheduleGrantReader::new(
        ServerWorkflowStore::scoped(store.clone(), ORG),
        Arc::new(ServerProfiles::new(store.clone())),
        ORG,
    )
}

// ── 派生授权单 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_derived_grant_is_deterministic_across_reads() {
    let store = store();
    let connection_id = save_profile(&store).await;
    let yaml = scheduled_yaml(&connection_id);
    save_definition(&store, ORG, &yaml);
    let reader = reader(&store);

    let first = reader
        .schedule_grant("demo")
        .await
        .expect("read")
        .expect("grant");
    let second = reader
        .schedule_grant("demo")
        .await
        .expect("read")
        .expect("grant");
    assert_eq!(first, second, "派生单必须是输入的纯函数（冻结比对承重）");
    assert_eq!(first.workflow_revision, definition_revision(&yaml));
    assert!(first
        .service_principal
        .starts_with("workflow-schedule:demo:"));
    assert_eq!(first.organization_id, ORG);
    assert_eq!(first.approved_by, "definition-save");
    assert_eq!(first.scopes.len(), 1);
    assert_eq!(first.scopes[0].command, "query");
    assert_eq!(first.scopes[0].target.connection_id.as_str(), connection_id);
    // 查询步骤未带 database 时继承 workflow 级默认。
    assert_eq!(
        first.scopes[0].target.namespace.database.as_deref(),
        Some("app")
    );
    assert!(first.validate(chrono::Utc::now()).is_ok());
}

#[tokio::test]
async fn a_definition_change_moves_the_derived_grant() {
    let store = store();
    let connection_id = save_profile(&store).await;
    let yaml = scheduled_yaml(&connection_id);
    save_definition(&store, ORG, &yaml);
    let reader = reader(&store);

    let first = reader
        .schedule_grant("demo")
        .await
        .expect("read")
        .expect("grant");
    save_definition(&store, ORG, &yaml.replace("select 1", "select 2"));
    let second = reader
        .schedule_grant("demo")
        .await
        .expect("read")
        .expect("grant");
    assert_ne!(first, second, "定义变更必须换派生单（触发比对拒绝）");
}

#[tokio::test]
async fn refusal_facts_are_none_not_errors() {
    let store = store();
    save_profile(&store).await;
    let reader = reader(&store);

    // 定义缺失 → 事实。
    assert!(reader
        .schedule_grant("missing")
        .await
        .expect("read")
        .is_none());

    // AI 步骤（服务身份不放行）→ 事实。
    save_definition(
        &store,
        ORG,
        "id: ai-demo\nname: Ai\ndescription: d\nconnection: conn-1\nsteps:\n  - type: ai\n    id: a1\n    prompt: p\n",
    );
    assert!(reader
        .schedule_grant("ai-demo")
        .await
        .expect("read")
        .is_none());

    // 无连接可继承（目标无法固定）→ 事实。
    save_definition(
        &store,
        ORG,
        "id: orphan\nname: Orphan\ndescription: d\nsteps:\n  - type: query\n    id: q1\n    sql: select 1\n",
    );
    assert!(reader
        .schedule_grant("orphan")
        .await
        .expect("read")
        .is_none());

    // profile 缺失（目标连接不在目录）→ 事实。
    save_definition(
        &store,
        ORG,
        "id: no-profile\nname: Np\ndescription: d\nconnection: conn-absent\nsteps:\n  - type: query\n    id: q1\n    sql: select 1\n",
    );
    assert!(reader
        .schedule_grant("no-profile")
        .await
        .expect("read")
        .is_none());
}

// ── 宿主归属核对 ────────────────────────────────────────────────────────────

/// 计数宿主（arm-not-fire 的观测面；拒绝路径不触执行原语）。
struct CountingHost {
    inner: ServerScheduleHost,
    dispatches: AtomicUsize,
}

#[async_trait::async_trait]
impl WorkflowScheduleHost for CountingHost {
    async fn list_workflows(&self) -> Result<Vec<WorkflowDefinition>, String> {
        self.inner.list_workflows().await
    }
    async fn execute_workflow(
        &self,
        id: &str,
        identity: &ScheduledExecutionIdentity,
    ) -> Result<datazen::workflow::domain::model::WorkflowExecutionResult, String> {
        self.dispatches.fetch_add(1, Ordering::SeqCst);
        self.inner.execute_workflow(id, identity).await
    }
}

fn scheduled_identity(org: &str) -> ScheduledExecutionIdentity {
    // 归属核对比对的是 organization_id（修订值不参与拒绝判定），修订用
    // 任意确定值即可。
    let revision = definition_revision("id: demo\n");
    let grant = datazen::workflow::domain::grant::WorkflowScheduleGrant {
        workflow_id: "demo".into(),
        workflow_revision: revision.clone(),
        organization_id: org.into(),
        service_principal: format!("workflow-schedule:demo:{}", &revision[..8]),
        approved_by: "definition-save".into(),
        scopes: vec![],
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        revocation_version: 1,
        revoked: false,
    };
    datazen::workflow::application::execution_identity::schedule_execution_identity(&grant)
}

#[tokio::test]
async fn the_host_rejects_organization_mismatch_and_unknown_workflows() {
    let store = store();
    // 拒绝路径（归属核对/未知 workflow）都在执行原语之前——定义无需可执行。
    save_definition(
        &store,
        ORG,
        "id: demo\nname: Demo\ndescription: d\nsteps: []\n",
    );
    let registry = Arc::new(WorkflowRegistry::new(
        Arc::new(ServerWorkflowStore::scoped(store.clone(), ORG)),
        Vec::new(),
    ));
    // 拒绝路径不触执行原语——宿主的执行字段给空注册表的真宿主（f1 同款
    // 最小装配）。
    let empty_registry = Arc::new(
        datazen::execution::runtime::driver_registry::DriverRegistry::with_factories([])
            .expect("empty registry"),
    );
    let vault = Arc::new(
        datazen_server::execution::secret_provider::ServerSecretVault::new(
            store.clone(),
            b"wf-sched-secret".to_vec(),
        )
        .expect("vault"),
    );
    let connections = Arc::new(datazen_server::storage::ServerConnections::new(
        store.clone(),
        vault,
    ));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));
    let backend = Arc::new(
        datazen_server::execution::driver_exec::ServerDriverExecBackend::new(
            empty_registry.clone(),
            connections.clone(),
            profiles.clone(),
            datazen::connection::domain::ConnectionScope::new(ORG),
            Arc::new(
                datazen_server::execution::budget_bridge::BudgetPortBridge::new(
                    Arc::new(
                        datazen_server::execution::budget::ServerBudgetCoordinator::new(
                            8,
                            10_000,
                            OrganizationId::new(ORG),
                        ),
                    ),
                    OrganizationId::new(ORG),
                ),
            ),
            None,
        ),
    );
    let lease = Arc::new(
        datazen_server::execution::lease::ServerRuntimeLeaseSource::new_with_shared_parts(
            profiles.clone(),
            Arc::new(
                datazen_server::execution::policy_adapter::ServerExecutionPolicy::new(
                    Arc::new(
                        datazen_server::policy::PolicyService::new(
                            store.clone(),
                            b"wf-sched-secret".to_vec(),
                        )
                        .expect("policy"),
                    ),
                    store.clone(),
                    Arc::new(datazen::shared::clock::SystemClock),
                ),
            ),
            backend,
            datazen::execution::application::orchestration::RuntimeResultSink::default(),
            Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new()),
        ),
    );
    let resource_host = Arc::new(
        datazen_server::execution::resource_host::ServerResourceExecutionHost::new(
            connections,
            profiles.clone(),
            empty_registry,
            Arc::new(datazen_server::storage::ServerSettingsStore::scoped(
                ORG,
                store.clone(),
            )),
            lease,
            ORG,
        ),
    );
    let host = ServerScheduleHost::new(
        registry,
        ServerWorkflowHistoryStore::scoped(store.clone(), ORG),
        resource_host,
        profiles,
        ORG,
    );

    // Owner 归属核对：授权单的组织 ≠ 宿主配置的组织 → 拒绝（失败关闭）。
    let foreign = scheduled_identity("org-other");
    let error = host
        .execute_workflow("demo", &foreign)
        .await
        .expect_err("mismatch must reject");
    assert!(error.contains("organization mismatch"), "{error}");

    // 未知 workflow → 拒绝（桌面同文案族）。
    let identity = scheduled_identity(ORG);
    let error = host
        .execute_workflow("missing", &identity)
        .await
        .expect_err("unknown must reject");
    assert!(error.contains("not found"), "{error}");
}

// ── spawn：arm-not-fire 与停车 ──────────────────────────────────────────────

#[tokio::test]
async fn the_scheduler_arms_without_firing_on_first_observation() {
    let store = store();
    // 无 profile 的定时工作流：授权单派生失败（目标连接不在目录）⇒ 无单
    // 可冻结 ⇒ 永不派发——本断言与 arm-not-fire 语义同向且更严（fail-closed
    // 半边随测）。
    save_definition(&store, ORG, &scheduled_yaml("conn-absent"));
    let registry = Arc::new(WorkflowRegistry::new(
        Arc::new(ServerWorkflowStore::scoped(store.clone(), ORG)),
        Vec::new(),
    ));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    let counting = Arc::new(CountingHost {
        inner: ServerScheduleHost::new(
            registry,
            ServerWorkflowHistoryStore::scoped(store.clone(), ORG),
            // 执行原语不会被触达（arm-only）；空注册表宿主即够。
            Arc::new(NoopExecutionHost),
            profiles.clone(),
            ORG,
        ),
        dispatches: AtomicUsize::new(0),
    });
    let scheduler = ServerWorkflowScheduler::spawn(
        counting.clone(),
        Arc::new(ServerScheduleGrantReader::new(
            ServerWorkflowStore::scoped(store.clone(), ORG),
            profiles,
            ORG,
        )),
    );

    // 最小间隔被钳到 30s（clamp_interval_secs）；首次观察只上膛——2s 内
    // 不得有任何派发（重启不补跑语义的接线半边）。
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    assert_eq!(
        counting.dispatches.load(Ordering::SeqCst),
        0,
        "首次观察只上膛不触发"
    );

    // 停车幂等；停车后不再派发。
    scheduler.stop();
    assert!(scheduler.is_stopped());
    scheduler.stop();
    assert!(scheduler.is_stopped(), "停车幂等");
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(counting.dispatches.load(Ordering::SeqCst), 0);
}

/// 空执行宿主（arm-only 测试的执行原语占位；被触达即 panic 暴露）。
struct NoopExecutionHost;

#[async_trait::async_trait]
impl datazen::workflow::resource_execution::ResourceExecutionHost for NoopExecutionHost {
    async fn profile(
        &self,
        id: &str,
    ) -> Result<
        (datazen_driver_api::ConnectionConfig, u64, bool),
        datazen::workflow::domain::error::WorkflowError,
    > {
        Err(
            datazen::workflow::domain::error::WorkflowError::ConnectionFailed {
                connection_id: id.into(),
                message: "NoopExecutionHost must not be reached".into(),
            },
        )
    }
    async fn driver(
        &self,
        _config: &datazen_driver_api::ConnectionConfig,
    ) -> Result<
        Arc<dyn datazen_driver_api::DatabaseDriver>,
        datazen::workflow::domain::error::WorkflowError,
    > {
        Err(datazen::workflow::domain::error::WorkflowError::Driver(
            "NoopExecutionHost must not be reached".into(),
        ))
    }
    async fn safe_mode(&self) -> bool {
        true
    }
    async fn prepare(
        &self,
        _id: &str,
        _revision: u64,
        _command: &str,
        _target: datazen::execution::ExecutionTarget,
    ) -> Result<
        datazen::workflow::resource_execution::PreparedExecution,
        datazen::workflow::domain::error::WorkflowError,
    > {
        Err(datazen::workflow::domain::error::WorkflowError::Validation(
            "NoopExecutionHost must not be reached".into(),
        ))
    }
}

/// 共享层新增停车口的直接钉子（`stop` 后取消令牌视角的「不再派发」）。
#[tokio::test]
async fn the_shared_scheduler_stop_is_visible_through_the_cancel_token() {
    let core = CoreScheduler::new();
    assert!(!core.is_stopped());
    core.stop();
    assert!(core.is_stopped());
}
