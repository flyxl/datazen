//! Host-generic Workflow v1 wiring, not a driver SQL dialect test.
//! Run explicitly through run-live-contract.sh --crate datazen --lib-test
//! workflow::live_acceptance. Missing/unreachable live prerequisites fail.
use crate::{
    store::DesktopRepository,
    testing::app_state::{build_app_state, sample_postgres_config},
};
use datazen_feature_core::workflow::executor::WorkflowExecutor;
use datazen_feature_core::workflow::model::{StepStatus, WorkflowDefinition};
use datazen_runtime::driver_registry::registry::DriverRegistry;
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
