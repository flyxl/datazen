//! Dashboard 监控引擎接线测试（W8 刀3d-2；`dashboard::scheduler`）。
//!
//! 夹具与 `dashboard_routes` 同构（MockDriver 以 `postgresql` 注册——
//! 授权链驱动族闸只认 PG/MySQL 词）。覆盖面：
//!
//! - **授权绿路径**（`dashboard::grants::authorize` 流程函数直测）：
//!   interval 组件 + 固定只读查询 + 显式目标 → 授权单落库（规范化目标、
//!   revision 冻结、批准人 = 会话主体）；
//! - **调度执行绿路径**：手工构造的授权单走 `MonitorHost::
//!   execute_scheduled_widget` —— 定义 revision 复验 + `GrantScopedHost`
//!   作用域执行（真租约/resource host/MockDriver）+ 运行落账（v14 表）；
//! - **拒绝面**：无授权单的 `scheduled_authorization` 失败关闭；过期
//!   授权单拒绝；workflow revision 漂移拒绝；
//! - **生命周期**：spawn 后未停车 / stop 幂等（共享 `MonitorEngine` 的
//!   stop/is_stopped 可见性）。

use std::sync::Arc;

use datazen_driver_api::SslMode;

use datazen_server::dashboard::grants::{self, GrantFlowError};
use datazen_server::dashboard::scheduler::{ServerMonitorHost, ServerMonitorScheduler};
use datazen_server::execution::resource_host::ServerResourceExecutionHost;
use datazen_server::policy::{PolicyService, Role};
use datazen_server::storage::login_sessions::{LoginSessionMode, LoginSessions, NewLoginSession};
use datazen_server::storage::{
    AdminStore, ServerConnections, ServerDashboardRuns, ServerDashboardStore, ServerMonitorGrants,
    ServerProfiles, ServerSettingsStore, ServerWorkflowStore,
};

use datazen::monitor::application::MonitorHost as _;
use datazen::monitor::domain::MonitorGrant;
use datazen::shared::context::RequestContext;

const SERVER_SECRET: &[u8] = b"w8-dashboard-scheduler-secret";

struct Fixture {
    dashboards: ServerDashboardStore,
    runs: ServerDashboardRuns,
    grants: ServerMonitorGrants,
    workflows: ServerWorkflowStore,
    registry: Arc<datazen::workflow::registry::WorkflowRegistry>,
    profiles: Arc<ServerProfiles>,
    connections: Arc<ServerConnections>,
    resource_host: Arc<ServerResourceExecutionHost>,
    host: Arc<ServerMonitorHost>,
    scheduler: Arc<ServerMonitorScheduler>,
    approver: RequestContext,
    connection_id: String,
}

async fn fixture() -> Fixture {
    use datazen_server::execution::budget::ServerBudgetCoordinator;
    use datazen_server::execution::budget_bridge::BudgetPortBridge;
    use datazen_server::execution::driver_exec::ServerDriverExecBackend;
    use datazen_server::execution::lease::ServerRuntimeLeaseSource;
    use datazen_server::execution::policy_adapter::ServerExecutionPolicy;
    use datazen_server::execution::publisher::execution_result_publisher;
    use datazen_server::execution::secret_provider::ServerSecretVault;

    let store = Arc::new(AdminStore::open_in_memory().expect("memory store"));
    store.migrate().expect("migrations");

    PolicyService::new(store.clone(), SERVER_SECRET.to_vec())
        .expect("policy")
        .create_membership("org-1", "user-1", Role::OrgMember, "idp:user-1")
        .expect("membership");
    LoginSessions::new(store.clone())
        .create(NewLoginSession {
            organization_id: "org-1".into(),
            session_id: "sess-sched".into(),
            principal_id: "user-1".into(),
            auth_time: chrono::Utc::now().to_rfc3339(),
            issued_at: chrono::Utc::now().to_rfc3339(),
            absolute_expires_at: (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339(),
            idle_expires_at: (chrono::Utc::now() + chrono::Duration::hours(8)).to_rfc3339(),
            permission_version: 1,
            client_binding: "ua|client-1".into(),
            mode: LoginSessionMode::Cookie,
            native_secret_hash: None,
        })
        .expect("session");

    let approver = RequestContext::new(
        datazen::shared::id::OrganizationId::new("org-1"),
        datazen::shared::id::PrincipalId::new("user-1"),
        None,
        datazen::shared::id::ClientInstanceId::new("http"),
        datazen::shared::id::RequestId::new("req-sched-wiring"),
        None,
    );

    let registry = Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let mut opts = datazen_driver_api::mock_driver::MockDriverOptions::default();
    opts.query_rows = vec![vec![Some(datazen_driver_api::Value::Integer(7))]];
    let driver = datazen_driver_api::mock_driver::MockDriver::new("mockdb", opts);
    registry
        .register_test_driver("postgresql", driver.clone())
        .await;
    struct WiringFactory {
        driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    }
    impl datazen_driver_api::DatabaseDriverFactory for WiringFactory {
        fn driver_id(&self) -> &'static str {
            "postgresql"
        }
        fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
            Arc::clone(&self.driver)
        }
        fn resource_provider(
            &self,
        ) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
            use datazen_driver_api::capabilities;
            use datazen_driver_api::namespace::NamespaceShape;
            use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
            let shape = NamespaceShape {
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
                ..NamespaceShape::default()
            };
            let mut caps = capabilities::CapabilitySet::default();
            caps.stateful_session = capabilities::Availability::Supported;
            caps.namespace_switch = capabilities::NamespaceSwitch::Unsupported;
            caps.context_observation = capabilities::ContextObservation::Full;
            caps.session_scoped_handles = capabilities::SessionScopedHandleSupport::Supported;
            caps.reset_for_reuse = capabilities::ResetForReuse::Unsupported;
            Some(Arc::new(LegacyResourceAdapter::new(
                Arc::clone(&self.driver),
                "postgresql",
                "scheduler-wiring",
                1,
                shape,
                caps,
            )))
        }
    }
    registry
        .register_test_factory("postgresql", Box::leak(Box::new(WiringFactory { driver })))
        .await;

    let vault = Arc::new(
        ServerSecretVault::new(store.clone(), SERVER_SECRET.to_vec()).expect("secret vault"),
    );
    let connections = Arc::new(ServerConnections::new(store.clone(), vault));
    let profiles = Arc::new(ServerProfiles::new(store.clone()));

    use datazen::connection::domain::catalog::ConnectionRepository as _;
    use datazen::connection::domain::profile::ProfileRepository as _;
    let record = profiles
        .create(
            &approver,
            datazen::connection::dto::ProfileDraft::new("sched-conn", "postgresql"),
            &datazen::shared::id::IdempotencyKey::new("idem-sched-profile"),
        )
        .await
        .expect("profile");
    let connection_id = record.connection_id.as_str().to_owned();
    let scope = datazen::connection::domain::ConnectionScope::new("org-1");
    connections
        .save(
            &scope,
            datazen_driver_api::ConnectionConfig {
                id: connection_id.clone(),
                name: "sched conn".into(),
                database_type: "postgresql".into(),
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
        .expect("connection");

    let bridge = Arc::new(BudgetPortBridge::new(
        Arc::new(ServerBudgetCoordinator::new(
            8,
            10_000,
            datazen::shared::id::OrganizationId::new("org-1"),
        )),
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
        .expect("publisher");
    let policy = Arc::new(ServerExecutionPolicy::new(
        Arc::new(PolicyService::new(store.clone(), SERVER_SECRET.to_vec()).expect("policy")),
        store.clone(),
        Arc::new(datazen::shared::clock::SystemClock),
    ));
    let lease_source = Arc::new(ServerRuntimeLeaseSource::new_with_shared_parts(
        profiles.clone(),
        policy,
        backend,
        sink,
        Arc::new(datazen::execution::runtime::directory::InMemorySessionDirectory::new()),
    ));
    let resource_host = Arc::new(ServerResourceExecutionHost::new(
        connections.clone(),
        profiles.clone(),
        registry.clone(),
        Arc::new(ServerSettingsStore::scoped("org-1", store.clone())),
        lease_source,
        "org-1",
    ));

    let workflows = ServerWorkflowStore::scoped(store.clone(), "org-1");
    let wf_registry = Arc::new(datazen::workflow::registry::WorkflowRegistry::new(
        Arc::new(workflows.clone()),
        Vec::new(),
    ));
    let dashboards = ServerDashboardStore::scoped(store.clone(), "org-1");
    let runs = ServerDashboardRuns::scoped(store.clone(), "org-1");
    let grants_store = ServerMonitorGrants::scoped(store.clone(), "org-1");
    let settings = Arc::new(ServerSettingsStore::scoped("org-1", store.clone()));

    let host = Arc::new(ServerMonitorHost::new(
        dashboards.clone(),
        runs.clone(),
        grants_store.clone(),
        workflows.clone(),
        wf_registry.clone(),
        settings,
        profiles.clone(),
        resource_host.clone(),
        "org-1",
    ));
    let scheduler = Arc::new(ServerMonitorScheduler::spawn(host.clone()));

    Fixture {
        dashboards,
        runs,
        grants: grants_store,
        workflows,
        registry: wf_registry,
        profiles,
        connections,
        resource_host,
        host,
        scheduler,
        approver,
        connection_id,
    }
}

/// interval 组件的仪表板（存储面直落）。
fn interval_dashboard(fx: &Fixture, workflow_id: &str) -> datazen::dashboard::types::Dashboard {
    datazen::dashboard::types::Dashboard {
        id: "dash-sched".into(),
        name: "Sched".into(),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        updated_at: "2026-01-01T00:00:00.000Z".into(),
        layout: datazen::dashboard::types::DashboardLayout {
            cols: 12,
            row_height: 80,
        },
        widgets: vec![datazen::dashboard::types::DashboardWidget {
            id: "w-sched".into(),
            title: "Sched Widget".into(),
            workflow_id: workflow_id.into(),
            view_mode: datazen::dashboard::types::ViewMode::Table,
            chart_config: None,
            layout: datazen::dashboard::types::WidgetLayout {
                x: 0,
                y: 0,
                w: 6,
                h: 4,
            },
            refresh: datazen::dashboard::types::RefreshPolicy {
                mode: datazen::dashboard::types::RefreshMode::Interval,
                refresh_sec: Some(30),
            },
            alert: None,
            enabled: true,
        }],
        enabled: true,
        refresh_paused: false,
    }
}

/// 固定只读查询工作流（YAML——存储行原文）。
fn fixed_query_yaml(id: &str, connection_id: &str) -> String {
    format!(
        "id: {id}\nname: WF {id}\ndescription: sched\nconnection: {connection_id}\nsteps:\n  - type: query\n    id: q1\n    sql: SELECT 7 AS v\n    connection: {connection_id}\n    database: app\n"
    )
}

async fn seed_workflow(fx: &Fixture, id: &str) {
    let yaml = fixed_query_yaml(id, &fx.connection_id);
    let definition: datazen::workflow::domain::model::WorkflowDefinition =
        serde_yaml::from_str(&yaml).expect("yaml parses");
    fx.registry
        .save(&fx.dashboards.scope(), &definition)
        .await
        .expect("workflow saved");
}

// ── 授权绿路径 + 调度执行绿路径 ─────────────────────────────────────────────

#[tokio::test]
async fn authorize_and_scheduled_execution_roundtrip() {
    let fx = fixture().await;
    seed_workflow(&fx, "wf-sched").await;
    datazen::dashboard::repository::save_and_fetch(
        &fx.dashboards.scope(),
        &fx.dashboards,
        interval_dashboard(&fx, "wf-sched"),
    )
    .expect("dashboard saved");

    // 授权绿路径：interval + 固定只读查询 + 显式目标 + PG 词。
    let grant = grants::authorize(
        &fx.dashboards,
        &fx.workflows,
        &fx.grants,
        &fx.profiles,
        &fx.connections,
        &fx.resource_host,
        &fx.approver,
        "dash-sched",
        "w-sched",
        3600,
    )
    .await
    .expect("authorize green path");
    assert_eq!(grant.approved_by, "user-1");
    assert_eq!(grant.workflow_id, "wf-sched");
    assert_eq!(grant.scopes.len(), 1, "one step → one scope");
    // 冻结的是规范化目标（数据库词在）。
    assert_eq!(
        grant.scopes[0].target.namespace.database.as_deref(),
        Some("app")
    );
    assert_eq!(grant.scopes[0].command, "query");

    // 授权单落库可读回。
    let stored = fx
        .grants
        .for_widget("dash-sched", "w-sched")
        .expect("read back")
        .expect("present");
    assert_eq!(stored, grant);

    // scheduled_authorization：最新授权单 + 事实核验 → 通过。
    let widget = fx
        .host
        .dashboards()
        .await
        .expect("dashboards")
        .into_iter()
        .find(|d| d.id == "dash-sched")
        .expect("dashboard")
        .widgets
        .into_iter()
        .find(|w| w.id == "w-sched")
        .expect("widget");
    let authorized = fx
        .host
        .scheduled_authorization("dash-sched", &widget)
        .await
        .expect("scheduled authorization");
    assert_eq!(authorized.id, grant.id);

    // 调度执行：真租约 + MockDriver + 作用域比对 + 运行落账。
    let run = fx
        .host
        .execute_scheduled_widget("dash-sched", &widget, &authorized)
        .await
        .expect("scheduled execution");
    assert_eq!(run.status, datazen::dashboard::types::WidgetRunStatus::Ok);
    assert_eq!(run.widget_id, "w-sched");
    assert_eq!(run.row_count, 1);

    // 运行历史可读（v14 表 + 归一化保留策略）。
    let index = datazen::dashboard::runs::list_run_index(
        &fx.dashboards.scope(),
        &fx.runs,
        "dash-sched",
        "w-sched",
        10,
    )
    .expect("index");
    assert_eq!(index.len(), 1);
    assert_eq!(index[0].id, run.id);

    // 收尾：停车（幂等）。
    fx.scheduler.stop();
    assert!(fx.scheduler.is_stopped());
    fx.scheduler.stop();
    assert!(fx.scheduler.is_stopped());
}

// ── 拒绝面 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scheduled_authorization_fails_closed_without_a_grant() {
    let fx = fixture().await;
    seed_workflow(&fx, "wf-no-grant").await;
    datazen::dashboard::repository::save_and_fetch(
        &fx.dashboards.scope(),
        &fx.dashboards,
        interval_dashboard(&fx, "wf-no-grant"),
    )
    .expect("dashboard saved");

    let widget = fx
        .host
        .dashboards()
        .await
        .expect("dashboards")
        .into_iter()
        .find(|d| d.id == "dash-sched")
        .expect("dashboard")
        .widgets
        .into_iter()
        .find(|w| w.id == "w-sched")
        .expect("widget");
    let error = fx
        .host
        .scheduled_authorization("dash-sched", &widget)
        .await
        .expect_err("no grant → fail closed");
    assert!(error.contains("authorization required"));

    fx.scheduler.stop();
}

#[tokio::test]
async fn authorize_rejects_unreadable_sql_and_drifting_definitions() {
    let fx = fixture().await;

    // 写一个带模板 SQL 的工作流（固定目标收集拒绝动态模板）。
    let yaml = format!(
        "id: wf-template\nname: WF Template\ndescription: x\nconnection: {}\nsteps:\n  - type: query\n    id: q1\n    sql: SELECT {{var}} AS v\n    connection: {}\n    database: app\n",
        fx.connection_id, fx.connection_id
    );
    let definition: datazen::workflow::domain::model::WorkflowDefinition =
        serde_yaml::from_str(&yaml).expect("yaml parses");
    fx.registry
        .save(&fx.dashboards.scope(), &definition)
        .await
        .expect("saved");
    datazen::dashboard::repository::save_and_fetch(
        &fx.dashboards.scope(),
        &fx.dashboards,
        interval_dashboard(&fx, "wf-template"),
    )
    .expect("dashboard saved");

    let error = grants::authorize(
        &fx.dashboards,
        &fx.workflows,
        &fx.grants,
        &fx.profiles,
        &fx.connections,
        &fx.resource_host,
        &fx.approver,
        "dash-sched",
        "w-sched",
        3600,
    )
    .await
    .expect_err("templated SQL rejected");
    assert!(
        matches!(error, GrantFlowError::Validation(ref message) if message.contains("read-only SQL") || message.contains("dynamic templates")),
        "wording: {error:?}"
    );

    // 授权后定义漂移：执行面拒绝（revision 复验）。
    seed_workflow(&fx, "wf-drift").await;
    datazen::dashboard::repository::save_and_fetch(
        &fx.dashboards.scope(),
        &fx.dashboards,
        interval_dashboard(&fx, "wf-drift"),
    )
    .expect("dashboard saved");
    let grant = grants::authorize(
        &fx.dashboards,
        &fx.workflows,
        &fx.grants,
        &fx.profiles,
        &fx.connections,
        &fx.resource_host,
        &fx.approver,
        "dash-sched",
        "w-sched",
        3600,
    )
    .await
    .expect("authorize");
    // 改定义（revision 前移）。
    let drifted =
        fixed_query_yaml("wf-drift", &fx.connection_id).replace("SELECT 7 AS v", "SELECT 8 AS v");
    let definition: datazen::workflow::domain::model::WorkflowDefinition =
        serde_yaml::from_str(&drifted).expect("yaml parses");
    fx.registry
        .save(&fx.dashboards.scope(), &definition)
        .await
        .expect("drifted save");

    let widget = fx
        .host
        .dashboards()
        .await
        .expect("dashboards")
        .into_iter()
        .find(|d| d.id == "dash-sched")
        .expect("dashboard")
        .widgets
        .into_iter()
        .find(|w| w.id == "w-sched")
        .expect("widget");
    // 漂移拒绝以**错误运行**落账（共享 `execute_widget_once` 的错误
    // 策略：宿主失败 → error run，不是异常）。
    let run = fx
        .host
        .execute_scheduled_widget("dash-sched", &widget, &grant)
        .await
        .expect("error run recorded");
    assert_eq!(
        run.status,
        datazen::dashboard::types::WidgetRunStatus::Error
    );
    assert!(
        run.error.as_deref().unwrap().contains("revision changed"),
        "wording: {:?}",
        run.error
    );

    fx.scheduler.stop();
}

#[tokio::test]
async fn expired_grants_fail_the_fact_check() {
    let fx = fixture().await;
    seed_workflow(&fx, "wf-expiry").await;
    datazen::dashboard::repository::save_and_fetch(
        &fx.dashboards.scope(),
        &fx.dashboards,
        interval_dashboard(&fx, "wf-expiry"),
    )
    .expect("dashboard saved");

    // 直落一条过期授权单（存储面；验证事实核验的过期腿）。
    let mut grant: MonitorGrant = {
        let mut grant = grants::authorize(
            &fx.dashboards,
            &fx.workflows,
            &fx.grants,
            &fx.profiles,
            &fx.connections,
            &fx.resource_host,
            &fx.approver,
            "dash-sched",
            "w-sched",
            3600,
        )
        .await
        .expect("authorize");
        grant.expires_at = chrono::Utc::now() - chrono::Duration::minutes(1);
        grant
    };
    fx.grants.save(&grant).expect("expired grant stored");
    // 事实核验读回的是存储行（过期）。
    let stored = fx
        .grants
        .for_widget("dash-sched", "w-sched")
        .expect("read")
        .expect("present");
    grant = stored;

    let widget = fx
        .host
        .dashboards()
        .await
        .expect("dashboards")
        .into_iter()
        .find(|d| d.id == "dash-sched")
        .expect("dashboard")
        .widgets
        .into_iter()
        .find(|w| w.id == "w-sched")
        .expect("widget");
    let error = fx
        .host
        .scheduled_authorization("dash-sched", &widget)
        .await
        .expect_err("expired grant rejected");
    assert!(error.contains("revoked or expired"), "wording: {error}");

    fx.scheduler.stop();
}
