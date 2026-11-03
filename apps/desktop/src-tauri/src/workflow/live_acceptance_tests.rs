//! Host-generic Workflow v1 wiring, not a driver SQL dialect test.
//! Run explicitly through run-live-contract.sh --crate datazen --lib-test
//! workflow::live_acceptance_tests. Missing/unreachable live prerequisites fail.
//!
//! # CI 策略（live 与 fake 两条腿）
//!
//! * **live 腿**（下方两个 `#[ignore]` 用例）：需要真实 PostgreSQL/MySQL
//!   夹具（`TEST_PG_*` / `TEST_MYSQL_*` 环境变量 + `dz_fixture_*` 专用库），
//!   只经 `run-live-contract.sh`（`DATAZEN_CONTRACT_REQUIRE_LIVE=1`）在带
//!   live 先决条件的环境执行；常态 CI（`cargo test` / `pnpm test`）靠
//!   `#[ignore]` + 环境变量断言双重保险跳过，不烧真实库。
//! * **fake-driver 腿**（`scheduled_dispatch_journey_with_fake_driver`）：
//!   无网络依赖、常态 CI 可跑——把 live 旅程里可码化的断言（触发语义 →
//!   授权闸 → 服务身份执行上下文 → mock driver 执行 → 审计）全部钉住；
//!   live 腿补齐的只有「真实驱动方言」这一段距离。
use crate::{
    store::DesktopRepository,
    testing::app_state_test_support::{build_app_state, sample_postgres_config},
};
use datazen::execution::runtime::driver_registry::registry::DriverRegistry;
use datazen::workflow::executor::WorkflowExecutor;
use datazen::workflow::model::{StepStatus, WorkflowDefinition};
use futures_util::FutureExt;
use std::sync::Arc;
// Link actual inventory factories even without production driver feature injection.
use datazen_driver_mysql as _;
use datazen_driver_postgres as _;

fn setting(prefix: &str, suffix: &str) -> String {
    std::env::var(format!("{prefix}{suffix}"))
        .unwrap_or_else(|_| panic!("live workflow prerequisite missing: {prefix}{suffix}"))
}

async fn cross_driver(first: &str, second: &str) {
    assert_eq!(
        std::env::var("DATAZEN_CONTRACT_REQUIRE_LIVE").as_deref(),
        Ok("1"),
        "run through the strict live contract runner"
    );
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(
        DesktopRepository::init_with_path(temp.path(), crate::store::KeyBackend::File)
            .await
            .expect("temporary store"),
    );
    let registry = Arc::new(DriverRegistry::new());
    for (id, prefix, driver) in [
        ("pg", "TEST_PG_", "postgresql"),
        ("my", "TEST_MYSQL_", "mysql"),
    ] {
        let mut config = sample_postgres_config(id);
        config.database_type = driver.into();
        config.host = Some(setting(prefix, "HOST"));
        config.port = Some(
            setting(prefix, "PORT")
                .parse()
                .expect("live port must be numeric"),
        );
        let database = setting(prefix, "DATABASE");
        assert!(
            database.starts_with("dz_fixture_"),
            "dedicated fixture database required"
        );
        config.database = Some(database);
        config.username = Some(setting(prefix, "USER"));
        config.password = Some(std::env::var(format!("{prefix}PASSWORD")).unwrap_or_default());
        config.ssl_mode = datazen_driver_api::SslMode::Disable;
        store
            .save_connection(config)
            .await
            .expect("persist fixture profile");
    }
    let state = build_app_state(store, registry);
    // Different data behind the same table name proves the override reached
    // the other physical database, rather than merely reporting its profile name.
    let table = format!("dz_wf_{}", uuid::Uuid::new_v4().simple());
    let setup_steps: Vec<_> = [("pg", 37), ("my", 5)]
        .into_iter()
        .flat_map(|(connection, value)| {
            [
                format!("CREATE TABLE {table} (value INTEGER NOT NULL)"),
                format!("INSERT INTO {table} (value) VALUES ({value})"),
            ]
            .into_iter()
            .enumerate()
            .map(move |(i, sql)| {
                serde_json::json!({
                    "type":"command", "id":format!("{connection}_{i}"),
                    "connection":connection, "command":"execute", "input":{"sql":sql},
                })
            })
        })
        .collect();
    let setup: WorkflowDefinition = serde_json::from_value(serde_json::json!({
        "id":"live-setup", "name":"Fixture setup", "description":"Live fixture", "steps":setup_steps,
    }))
    .unwrap();
    let journey = std::panic::AssertUnwindSafe(async {
        let setup_result = WorkflowExecutor::execute(&setup, &state, None, &serde_json::json!({}))
            .await
            .expect("fixture orchestration completes");
        // Existing YAML, legacy Query + Command normalization, connection inheritance
        // and override. The second SQL cannot parse if first-step substitution fails.
        let yaml = format!(
            r#"
id: live-cross-driver
name: Live cross-driver
description: Live wiring acceptance
connection: {first}
steps:
  - type: query
    id: seed
    sql: SELECT value FROM {table}
  - type: command
    id: finish
    connection: {second}
    command: query
    input:
      sql: "SELECT {{{{steps.seed.rows[0].value}}}} + value AS value FROM {table}"
"#
        );
        let workflow: WorkflowDefinition = serde_yaml::from_str(&yaml).expect("existing v1 YAML");
        assert!(workflow.version.is_none());
        let result = WorkflowExecutor::execute(&workflow, &state, None, &serde_json::json!({}))
            .await
            .expect("workflow orchestration completes");
        (setup_result, result)
    })
    .catch_unwind()
    .await;
    // Default Safe Mode remains enabled for setup and the acceptance workflow.
    // Only this disposable fixture store permits DROP of its exact UUID table.
    let original_settings = state.store.get_settings().await;
    let mut cleanup_settings = original_settings.clone();
    cleanup_settings.safe_mode = false;
    state
        .store
        .save_settings(cleanup_settings)
        .await
        .expect("fixture cleanup settings");
    let cleanup: WorkflowDefinition = serde_json::from_value(serde_json::json!({
        "id":"live-cleanup", "name":"Fixture cleanup", "description":"Live fixture", "error_handling":{"strategy":"skip"}, "steps":[
            {"type":"command", "id":"pg", "connection":"pg", "command":"execute", "input":{"sql":format!("DROP TABLE IF EXISTS {table}")}},
            {"type":"command", "id":"my", "connection":"my", "command":"execute", "input":{"sql":format!("DROP TABLE IF EXISTS {table}")}},
        ],
    })).unwrap();
    let cleanup_result = WorkflowExecutor::execute(&cleanup, &state, None, &serde_json::json!({}))
        .await
        .expect("cleanup orchestration completes");
    state
        .store
        .save_settings(original_settings)
        .await
        .expect("restore fixture safety settings");
    let teardown_ok = cleanup_result.success
        && cleanup_result.steps.len() == 2
        && cleanup_result
            .steps
            .iter()
            .all(|step| step.status == StepStatus::Success);
    // Teardown precedes all result assertions and also runs after a panicked
    // orchestration future; no failing step can skip the final cleanup phase.
    let (setup_result, result) = match journey {
        Ok(results) => results,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    eprintln!(
        "LIVE_STAGE direction={first}->{second} setup={} query={:?} command={:?} teardown={}",
        setup_result.success,
        result.steps.first().map(|step| &step.status),
        result.steps.get(1).map(|step| &step.status),
        teardown_ok
    );
    assert!(setup_result.success, "live fixture setup failed");
    assert!(teardown_ok, "live fixture cleanup failed");
    // Do not echo runtime errors/connection details on failures.
    assert!(result.success, "live cross-driver workflow failed");
    assert_eq!(result.steps.len(), 2);
    for (step, expected) in result
        .steps
        .iter()
        .zip([if first == "pg" { 37 } else { 5 }, 42])
    {
        assert_eq!(step.status, StepStatus::Success);
        let value = &step.result.as_ref().expect("query result")["rows"][0]["value"];
        assert_eq!(
            value.as_i64(),
            Some(expected),
            "live result/template mismatch"
        );
    }
}

#[tokio::test]
#[ignore = "requires strict PostgreSQL and MySQL live fixtures"]
async fn postgres_to_mysql_output_feeds_next_sql() {
    cross_driver("pg", "my").await;
}
#[tokio::test]
#[ignore = "requires strict PostgreSQL and MySQL live fixtures"]
async fn mysql_to_postgres_output_feeds_next_sql() {
    cross_driver("my", "pg").await;
}

/// Fake-driver 完整旅程（非 live 依赖，常态 CI 可跑）：
/// **触发 → 授权闸 → 服务身份上下文 → 执行 → 审计**。
///
/// 与上方两个 `#[ignore]` live 用例互补：live 腿证明「真实 PostgreSQL/MySQL
/// 驱动」的方言旅程；本用例用 mock driver 走**同一条派发链**（桌面授权单
/// 派生器 → 本体校验 → 服务身份执行上下文 → 调度宿主实际调用的执行入口），
/// 把不依赖真实驱动环境的断言全部钉住。
#[tokio::test]
async fn scheduled_dispatch_journey_with_fake_driver() {
    use datazen::shared::audit::{AuditOutcome, AuditPrincipalKind};
    use datazen::shared::clock::SystemClock;
    use datazen::workflow::application::execution_identity::schedule_execution_identity;
    use datazen::workflow::application::grant_reader::WorkflowScheduleGrantReader as _;
    use datazen::workflow::scheduler::{due_decision, DueDecision};
    use std::time::{Duration, Instant};
    use tauri::Manager as _;

    // ── 触发：v1 时钟语义（首次观察只上膛，到点触发）──
    let now = Instant::now();
    assert_eq!(due_decision(None, now, 60), DueDecision::Arm, "首次观察只上膛");
    assert!(
        !datazen::workflow::scheduler::is_workflow_due(None, now, 60),
        "上膛 tick 不派发"
    );
    assert!(
        datazen::workflow::scheduler::is_workflow_due(
            Some(now - Duration::from_secs(61)),
            Instant::now(),
            60
        ),
        "到点触发"
    );

    // ── 装配：TestAppState + mock driver，带 schedule 的定义保存 = 批准事实 ──
    let test = crate::testing::app_state_test_support::TestAppState::with_resource_provider(
        crate::testing::mock_driver::MockDriverOptions::default(),
    )
    .await;
    test.save_connection("conn-1").await;
    let (state, _keepalive) = test.into_state();
    let scope = state.organization_scope().expect("fixture scope");
    let yaml = "id: demo\nname: Demo\ndescription: fake-driver journey\nconnection: conn-1\ndatabase: app\nsteps:\n  - type: query\n    id: seed\n    sql: SELECT 1\n";
    state
        .workflow_registry
        .save_workflow_yaml(&scope, yaml)
        .await
        .expect("definition save (the approval fact) persists");
    let app = tauri::test::mock_app();
    app.manage(state);

    // ── 授权闸：桌面派生器按既有持久化形状派生授权单，本体校验放行 ──
    let grant = super::schedule_grant::DesktopScheduleGrantReader::new(app.handle().clone())
        .schedule_grant("demo")
        .await
        .expect("grant port reads the persisted facts")
        .expect("a scheduled definition yields a grant");
    assert!(
        grant.validate(chrono::Utc::now()).is_ok(),
        "派生单必须过授权闸的本体校验"
    );

    // ── 服务身份执行上下文：owner 归属 + 服务主体 + 无委托 ──
    let identity = schedule_execution_identity(&grant);
    assert_eq!(identity.service_principal(), grant.service_principal);
    assert!(identity.service_principal().starts_with("workflow-schedule:demo:"));
    assert_eq!(identity.organization_id(), grant.organization_id);
    assert_eq!(identity.workflow_id(), "demo");
    assert!(identity.delegation_id().is_none(), "服务身份不走委托链路");

    // ── 执行：调度宿主实际调用的同一条执行入口（mock driver 旅程）──
    let state = app.state::<crate::commands::AppState>();
    let result = crate::commands::workflow_execute_impl(
        state.inner(),
        "demo".into(),
        serde_json::json!({}),
        None,
    )
    .await
    .expect("scheduled dispatch executes through the shared entry");
    assert!(result.success, "fake-driver journey completes: {:?}", result.error);
    assert!(
        result.steps.iter().all(|step| step.status == StepStatus::Success),
        "every step succeeds through the fake driver"
    );

    // ── 审计：主体 kind=Service、principal=服务身份、归属授权单组织、无委托 ──
    let event = identity.audit_event(
        "workflow_scheduled_dispatch",
        AuditOutcome::Succeeded,
        SystemClock::default().now(),
    );
    assert_eq!(event.principal_kind, AuditPrincipalKind::Service);
    assert_eq!(event.principal_kind.as_str(), "service");
    assert_eq!(
        event.principal_id.as_ref().map(|p| p.as_str()),
        Some(identity.service_principal()),
        "审计主体必须是服务身份，而不是批准者或桌面用户"
    );
    assert_eq!(event.organization_id.as_str(), identity.organization_id());
    assert!(event.delegation_id.is_none());
    assert!(event.client_instance_id.is_none(), "调度不在请求周期内");
}
