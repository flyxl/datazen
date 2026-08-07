//! Integration tests for workflow cross-database features.
//!
//! Requires local PostgreSQL and MySQL instances with test data
//! from `scripts/setup-workflow-testdata.sh`.
//!
//! Run with: `cargo test -p datazen --test workflow_tests -- --nocapture`
//! Skip if databases are unavailable (tests check connectivity first).

use std::path::PathBuf;

fn load_env() -> Option<DbConfig> {
    let env_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".env");

    if !env_path.exists() {
        eprintln!("⏭  Skipping workflow tests: .env not found");
        return None;
    }

    let content = std::fs::read_to_string(&env_path).ok()?;
    let mut cfg = DbConfig::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "TEST_PG_HOST" => cfg.pg_host = v.trim().to_string(),
                "TEST_PG_PORT" => cfg.pg_port = v.trim().parse().unwrap_or(5432),
                "TEST_PG_USER" => cfg.pg_user = v.trim().to_string(),
                "TEST_PG_PASSWORD" => cfg.pg_password = v.trim().to_string(),
                "TEST_PG_DATABASE" => cfg.pg_database = v.trim().to_string(),
                "TEST_MYSQL_HOST" => cfg.mysql_host = v.trim().to_string(),
                "TEST_MYSQL_PORT" => cfg.mysql_port = v.trim().parse().unwrap_or(3306),
                "TEST_MYSQL_USER" => cfg.mysql_user = v.trim().to_string(),
                "TEST_MYSQL_PASSWORD" => cfg.mysql_password = v.trim().to_string(),
                "TEST_MYSQL_DATABASE" => cfg.mysql_database = v.trim().to_string(),
                _ => {}
            }
        }
    }
    Some(cfg)
}

#[derive(Default)]
struct DbConfig {
    pg_host: String,
    pg_port: u16,
    pg_user: String,
    pg_password: String,
    pg_database: String,
    mysql_host: String,
    mysql_port: u16,
    mysql_user: String,
    mysql_password: String,
    mysql_database: String,
}

impl DbConfig {
    fn pg_url(&self) -> String {
        if self.pg_password.is_empty() {
            format!(
                "postgres://{}@{}:{}/{}",
                self.pg_user, self.pg_host, self.pg_port, self.pg_database
            )
        } else {
            format!(
                "postgres://{}:{}@{}:{}/{}",
                self.pg_user, self.pg_password, self.pg_host, self.pg_port, self.pg_database
            )
        }
    }

    fn mysql_url(&self) -> String {
        if self.mysql_password.is_empty() {
            format!(
                "mysql://{}@{}:{}/{}",
                self.mysql_user, self.mysql_host, self.mysql_port, self.mysql_database
            )
        } else {
            format!(
                "mysql://{}:{}@{}:{}/{}",
                self.mysql_user, self.mysql_password, self.mysql_host, self.mysql_port,
                self.mysql_database
            )
        }
    }
}

// ─── TC-01: Cross-DB Workflow YAML Parsing ─────────────────────────────────────

use datazen::workflow::workflows::{WorkflowDefinition, WorkflowStep};

#[test]
fn tc01_cross_db_workflow_yaml_parses() {
    let yaml = include_str!("../../scripts/test-cross-db-workflow.yaml");
    let workflow: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(workflow.id, "order-logistics");
    assert!(workflow.timeout_secs.is_some());
    assert!(workflow.error_handling.is_some());

    let conn_vars: Vec<_> = workflow
        .variables
        .iter()
        .filter(|v| v.var_type == "connection")
        .collect();
    assert_eq!(conn_vars.len(), 2, "Should have 2 connection variables");

    assert!(workflow.steps.len() >= 2, "Should have at least 2 steps");
    match &workflow.steps[0] {
        WorkflowStep::Query { connection, .. } => {
            assert!(
                connection.is_some(),
                "First query should have per-step connection"
            );
        }
        _ => panic!("First step should be Query"),
    }
}

// ─── TC-02: Cross-DB Query + Template Resolution ────────────────────────────

#[tokio::test]
async fn tc02_cross_db_query_and_template_resolution() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let mysql_pool = match sqlx::MySqlPool::connect(&cfg.mysql_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to MySQL: {e}");
            return;
        }
    };

    let orders: Vec<(String,)> = sqlx::query_as(
        "SELECT order_id FROM test_orders WHERE uid = 'U001' ORDER BY created_at DESC",
    )
    .fetch_all(&pg_pool)
    .await
    .unwrap();

    assert!(!orders.is_empty(), "U001 should have orders");
    let order_ids: Vec<&str> = orders.iter().map(|r| r.0.as_str()).collect();
    println!("PG orders for U001: {:?}", order_ids);

    let in_clause: String = order_ids
        .iter()
        .map(|id| format!("'{id}'"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT order_id, carrier, tracking_no, status FROM test_logistics WHERE order_id IN ({in_clause})"
    );

    let logistics: Vec<(String, String, String, String)> =
        sqlx::query_as(&sql).fetch_all(&mysql_pool).await.unwrap();

    assert!(
        !logistics.is_empty(),
        "Should find logistics for U001's orders"
    );
    for (oid, carrier, tracking, status) in &logistics {
        println!("  {oid}: {carrier} {tracking} [{status}]");
        assert!(order_ids.contains(&oid.as_str()));
    }

    pg_pool.close().await;
    mysql_pool.close().await;
}

// ─── TC-03: Structured Result Serialization ─────────────────────────────────

#[tokio::test]
async fn tc03_structured_result_serialization() {
    use datazen::workflow::workflows::{WorkflowExecutionResult, StepExecutionResult, StepStatus};

    let result = WorkflowExecutionResult {
        success: true,
        final_output: "done".into(),
        steps: vec![StepExecutionResult {
            step_id: "s1".into(),
            step_type: "query".into(),
            status: StepStatus::Success,
            result: Some(serde_json::json!({
                "rows": [
                    {"order_id": "ORD-001", "amount": 100.0},
                    {"order_id": "ORD-002", "amount": 200.0},
                ],
                "rows_count": 2,
            })),
            execution_time_ms: 10,
            error: None,
            connection_name: Some("PG".into()),
            sql_executed: Some("SELECT 1".into()),
        }],
        total_time_ms: 10,
        error: None,
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"stepId\""));
    assert!(json.contains("\"finalOutput\""));
    assert!(json.contains("\"totalTimeMs\""));
    assert!(json.contains("ORD-001"));

    let parsed: WorkflowExecutionResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.steps.len(), 1);
    assert_eq!(parsed.steps[0].step_id, "s1");
}

// ─── TC-04: Condition with Real Data ────────────────────────────────────────

#[tokio::test]
async fn tc04_condition_with_real_data() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test_orders WHERE uid = 'U001'")
        .fetch_one(&pg_pool)
        .await
        .unwrap();
    assert!(
        count.0 > 0,
        "U001 should have orders, condition should be true"
    );

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test_orders WHERE uid = 'U999'")
        .fetch_one(&pg_pool)
        .await
        .unwrap();
    assert_eq!(
        count.0, 0,
        "U999 should have no orders, condition should be false"
    );

    pg_pool.close().await;
}

// ─── TC-05: ForEach Batch Query ─────────────────────────────────────────────

#[tokio::test]
async fn tc05_foreach_batch_query() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let mysql_pool = match sqlx::MySqlPool::connect(&cfg.mysql_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to MySQL: {e}");
            return;
        }
    };

    let orders: Vec<(String,)> =
        sqlx::query_as("SELECT order_id FROM test_orders ORDER BY id")
            .fetch_all(&pg_pool)
            .await
            .unwrap();

    assert_eq!(orders.len(), 5, "Should have 5 orders total");

    let mut logistics_found = 0;
    for (order_id,) in &orders {
        let result: Vec<(String, String)> = sqlx::query_as(
            "SELECT carrier, status FROM test_logistics WHERE order_id = ?",
        )
        .bind(order_id)
        .fetch_all(&mysql_pool)
        .await
        .unwrap();

        if !result.is_empty() {
            logistics_found += 1;
            println!("  {order_id}: {} [{}]", result[0].0, result[0].1);
        }
    }

    assert_eq!(
        logistics_found, 5,
        "All 5 orders should have logistics entries"
    );

    pg_pool.close().await;
    mysql_pool.close().await;
}

// ─── TC-06: Error Handling — Invalid SQL ────────────────────────────────────

#[tokio::test]
async fn tc06_error_handling_invalid_sql() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let result: Result<Vec<(i64,)>, _> =
        sqlx::query_as("SELECT id FROM non_existent_table_xyz")
            .fetch_all(&pg_pool)
            .await;

    assert!(result.is_err(), "Query to non-existent table should fail");
    let err = result.err().unwrap().to_string();
    println!("Expected error: {err}");

    pg_pool.close().await;
}

// ─── TC-07: Timeout Behavior ────────────────────────────────────────────────

#[tokio::test]
async fn tc07_timeout_behavior() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    // Should complete within timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query("SELECT pg_sleep(0.1)").fetch_all(&pg_pool),
    )
    .await;

    assert!(
        result.is_ok(),
        "Short query should complete within timeout"
    );

    // Should timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(50),
        sqlx::query("SELECT pg_sleep(2)").fetch_all(&pg_pool),
    )
    .await;

    assert!(result.is_err(), "Long query should timeout");

    pg_pool.close().await;
}

// ─── TC-08: Connection Reuse (get_or_connect) ───────────────────────────────

#[test]
fn tc08_connection_reuse_logic() {
    // ConnectionManager.get_or_connect is tested via the module's internal tests
    // Here we verify the WorkflowDefinition can reference connection variables
    let yaml = r#"
id: reuse-test
name: Reuse
description: Connection reuse test
variables:
  - name: db
    type: connection
    description: DB
    required: true
steps:
  - type: query
    id: s1
    connection: "{{db}}"
    sql: "SELECT 1"
  - type: query
    id: s2
    connection: "{{db}}"
    sql: "SELECT 2"
"#;
    let workflow: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    match (&workflow.steps[0], &workflow.steps[1]) {
        (
            WorkflowStep::Query { connection: c1, .. },
            WorkflowStep::Query { connection: c2, .. },
        ) => {
            assert_eq!(c1, c2, "Both steps should use the same connection variable");
        }
        _ => panic!("Expected 2 Query steps"),
    }
}

// ─── TC-09: Backward Compatibility ──────────────────────────────────────────

#[test]
fn tc09_backward_compat_old_yaml() {
    let yaml = r#"
id: old-format
name: Old Workflow
description: No new fields
variables:
  - name: query
    type: string
    description: SQL
    required: true
steps:
  - type: query
    id: run
    sql: "{{query}}"
  - type: ai
    id: analyze
    prompt: "Analyze: {{steps.run.result}}"
output:
  format: text
"#;
    let workflow: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    assert!(workflow.timeout_secs.is_none());
    assert!(workflow.error_handling.is_none());
    match &workflow.steps[0] {
        WorkflowStep::Query {
            connection,
            timeout_secs,
            on_error,
            ..
        } => {
            assert!(connection.is_none());
            assert!(timeout_secs.is_none());
            assert!(on_error.is_none());
        }
        _ => panic!("Expected Query"),
    }
}

// ─── TC-10: History Persistence ─────────────────────────────────────────────

#[tokio::test]
async fn tc10_history_persistence() {
    use datazen::workflow::history::WorkflowHistoryManager;
    use datazen::workflow::workflows::{WorkflowExecutionResult, StepExecutionResult, StepStatus};

    let dir = tempfile::tempdir().unwrap();
    let mgr = WorkflowHistoryManager::new(dir.path().to_path_buf());
    mgr.load().await.unwrap();

    let result = WorkflowExecutionResult {
        success: true,
        final_output: "test".into(),
        steps: vec![StepExecutionResult {
            step_id: "s1".into(),
            step_type: "query".into(),
            status: StepStatus::Success,
            result: Some(serde_json::json!({"rows": [{"order_id": "ORD-001"}]})),
            execution_time_ms: 42,
            error: None,
            connection_name: Some("PG".into()),
            sql_executed: Some("SELECT 1".into()),
        }],
        total_time_ms: 100,
        error: None,
    };

    let id = mgr
        .record(
            "workflow-1",
            "Workflow 1",
            &serde_json::json!({"uid": "U001"}),
            &result,
        )
        .await
        .unwrap();
    assert!(!id.is_empty());

    let items = mgr.list(None).await;
    assert_eq!(items.len(), 1);
    assert!(items[0].success);

    let entry = mgr.get(&id).await.unwrap();
    assert_eq!(entry.workflow_name, "Workflow 1");
    assert_eq!(entry.result.steps[0].step_id, "s1");

    // Persist and reload
    drop(mgr);
    let mgr2 = WorkflowHistoryManager::new(dir.path().to_path_buf());
    mgr2.load().await.unwrap();
    assert_eq!(mgr2.list(None).await.len(), 1);

    let removed = mgr2.clear(None).await.unwrap();
    assert_eq!(removed, 1);
    assert_eq!(mgr2.list(None).await.len(), 0);
}

// ─── TC-11: Wildcard Template for IN Clause ─────────────────────────────────

#[test]
fn tc11_wildcard_template_for_in_clause() {
    let yaml = r#"
id: wildcard-test
name: Wildcard Test
description: Test wildcard resolution
steps:
  - type: query
    id: get_orders
    sql: "SELECT order_id FROM orders"
  - type: query
    id: get_logistics
    sql: "SELECT * FROM logistics WHERE order_id IN ({{steps.get_orders.rows.*.order_id}})"
"#;
    let workflow: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(workflow.steps.len(), 2);
    match &workflow.steps[1] {
        WorkflowStep::Query { sql, .. } => {
            assert!(sql.contains("{{steps.get_orders.rows.*.order_id}}"));
        }
        _ => panic!("Expected Query"),
    }
}

// ─── TC-12: Nested Condition + ForEach ──────────────────────────────────────

#[test]
fn tc12_nested_condition_foreach() {
    let yaml = r#"
id: complex-workflow
name: Complex
description: Test condition + foreach nesting
steps:
  - type: query
    id: get_data
    sql: "SELECT * FROM t"
  - type: condition
    id: check
    if: "steps.get_data.rows_count > 0"
    then_steps:
      - type: foreach
        id: loop
        items: "steps.get_data.rows"
        as_var: "row"
        max_iterations: 10
        steps:
          - type: query
            id: detail
            sql: "SELECT * FROM d WHERE id = '{{row.id}}'"
"#;
    let workflow: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(workflow.steps.len(), 2);

    match &workflow.steps[1] {
        WorkflowStep::Condition {
            then_steps,
            else_steps,
            ..
        } => {
            assert_eq!(then_steps.len(), 1);
            assert!(else_steps.is_none());
            match &then_steps[0] {
                WorkflowStep::ForEach {
                    as_var,
                    max_iterations,
                    steps,
                    ..
                } => {
                    assert_eq!(as_var, "row");
                    assert_eq!(*max_iterations, Some(10));
                    assert_eq!(steps.len(), 1);
                }
                _ => panic!("Expected ForEach"),
            }
        }
        _ => panic!("Expected Condition"),
    }
}

// ─── TC-13: Full Cross-DB Data Flow ─────────────────────────────────────────

#[tokio::test]
async fn tc13_full_cross_db_data_flow() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let mysql_pool = match sqlx::MySqlPool::connect(&cfg.mysql_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to MySQL: {e}");
            return;
        }
    };

    // 1. Query PG for U001's orders
    let orders: Vec<(String, String, f64)> = sqlx::query_as(
        "SELECT order_id, product_name, amount::float8 FROM test_orders WHERE uid = 'U001' ORDER BY created_at DESC",
    )
    .fetch_all(&pg_pool)
    .await
    .unwrap();

    assert_eq!(orders.len(), 3, "U001 should have 3 orders");

    // 2. Condition check
    assert!(!orders.is_empty());

    // 3. Build IN clause (simulating wildcard template resolution)
    let in_clause: String = orders
        .iter()
        .map(|(oid, _, _)| format!("'{oid}'"))
        .collect::<Vec<_>>()
        .join(",");

    // 4. Query MySQL logistics
    let sql = format!(
        "SELECT order_id, carrier, tracking_no, status FROM test_logistics WHERE order_id IN ({in_clause})"
    );
    let logistics: Vec<(String, String, String, String)> =
        sqlx::query_as(&sql).fetch_all(&mysql_pool).await.unwrap();

    assert_eq!(logistics.len(), 3, "Should find logistics for all 3 orders");

    // 5. Cross-reference validation
    let pg_order_ids: Vec<&str> = orders.iter().map(|(oid, _, _)| oid.as_str()).collect();
    for (oid, carrier, tracking, status) in &logistics {
        assert!(
            pg_order_ids.contains(&oid.as_str()),
            "Logistics order_id {oid} should be in PG results"
        );
        assert!(!carrier.is_empty());
        assert!(!tracking.is_empty());
        assert!(!status.is_empty());
        println!("  Cross-DB verified: {oid} -> {carrier} {tracking} [{status}]");
    }

    // 6. Specific data assertions
    let delivered: Vec<_> = logistics
        .iter()
        .filter(|(_, _, _, s)| s == "delivered")
        .collect();
    assert!(
        !delivered.is_empty(),
        "At least one order should be delivered"
    );

    println!("\n✅ Full cross-DB data flow verified successfully");
    println!(
        "   PG orders: {} -> MySQL logistics: {}",
        orders.len(),
        logistics.len()
    );

    pg_pool.close().await;
    mysql_pool.close().await;
}
