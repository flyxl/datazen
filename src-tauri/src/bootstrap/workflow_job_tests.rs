use super::*;
use datazen_platform_api::dto::job::JobFilter;
async fn state(database_type: &str) -> (tempfile::TempDir, AppState) {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::init_with_path(temp.path()).await.unwrap());
    let config = crate::db::ConnectionConfig {
        id: "pg".into(),
        name: "synthetic".into(),
        database_type: database_type.into(),
        host: None,
        port: None,
        database: Some("app".into()),
        schema: Some("approved".into()),
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 1,
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
    };
    store.save_connection(config).await.unwrap();
    let state = finish_app_state(
        store,
        Arc::new(init_drivers()),
        Arc::new(SyncAdapterRegistry::new()),
        None,
    );
    (temp, state)
}
async fn rejected(yaml: &str, code: &str) {
    let (_temp, state) = state("postgresql").await;
    assert_rejected_before_admission(&state, yaml, code).await;
}
async fn assert_rejected_before_admission(state: &AppState, yaml: &str, code: &str) {
    let definition: crate::workflow::WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    let error = crate::workflow::job::execute(state, &definition, &serde_json::json!({}))
        .await
        .unwrap_err();
    assert!(error.contains(code), "unexpected rejection class");
    let ctx = state
        .platform_require()
        .unwrap()
        .begin_call()
        .context()
        .clone();
    assert!(
        state
            .desktop_job_host
            .list(&ctx, JobFilter::default())
            .await
            .unwrap()
            .is_empty(),
        "preflight precedes durable admission and resources"
    );
}
#[tokio::test]
async fn mysql_transaction_plans_fail_closed_before_job_admission() {
    let (_temp, state) = state("mysql").await;
    for sql in [
        "SELECT 2",
        "INSERT INTO items(id) VALUES (1)",
        "UPDATE items SET id = 2",
        "DELETE FROM items",
    ] {
        let yaml = format!("id: wf\nname: Frozen\ndescription: ''\nversion: '2'\nconnection: pg\ndatabase: app\nsteps:\n  - type: query\n    id: independent\n    sql: SELECT 1\nblocks:\n  - type: transactionBlock\n    id: tx\n    steps:\n      - type: query\n        id: write\n        sql: {sql}\n");
        assert_rejected_before_admission(&state, &yaml, "CapabilityUnsupported").await;
    }
}
#[tokio::test]
async fn unproven_postgres_plan_rejects_before_prior_steps_or_blocks() {
    // Schema qualification is forbidden in input, even if it looks approved.
    // The driver binds every accepted bare relation itself after dialect parsing.
    rejected("id: wf\nname: Frozen\ndescription: ''\nversion: '2'\nconnection: pg\ndatabase: app\nsteps:\n  - type: query\n    id: independent\n    sql: SELECT 1\nblocks:\n  - type: sessionBlock\n    id: prior\n    steps:\n      - type: query\n        id: earlier\n        sql: SELECT 2\n  - type: transactionBlock\n    id: tx\n    steps:\n      - type: command\n        id: write\n        command: execute\n        input:\n          sql: UPDATE other.items SET id = 3\n", "UnsupportedPlan").await;
}
#[tokio::test]
async fn workflow_conflicting_block_target_rejects_before_job_admission() {
    rejected("id: wf\nname: Frozen\ndescription: ''\nversion: '2'\nconnection: pg\ndatabase: app\nsteps: []\nblocks:\n  - type: sessionBlock\n    id: retained\n    steps:\n      - type: query\n        id: first\n        sql: SELECT 1\n      - type: query\n        id: conflict\n        connection: pg\n        database: other\n        sql: SELECT 2\n","conflict").await;
}
