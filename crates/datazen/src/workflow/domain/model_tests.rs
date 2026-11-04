//! Workflow 模型词汇的解析 / 序列化测试。
//!
//! 2026-11-02 归位（原 `crates/datazen-desktop/tests/workflow_tests.rs` 的
//! tc01/03/08/09/11/12——F6-c1 目录拆分时该文件随旧 src-tauri 迁入宿主，
//! 但这些用例只测 `datazen::workflow::model` 的纯词汇，零宿主依赖）：
//! 测试住在被测本体旁边，随每条 feature 腿编译（`--no-run`）、由无选择器
//! 腿执行。用例名沿用描述式命名（与 `executor_tests.rs` 同风格），原始
//! tcNN 编号以行内注释保留对照——跨库真 DB 用例（tc02/04/05/06/07/13）
//! 留在宿主 `src-tauri/tests/workflow_db_tests.rs`（需本地 PG/MySQL）。
//!
//! 既有五例（YAML/serde 往返与 schedule 兼容）自 model.rs 内联测试区
//! 迁来——姊妹文件是本域惯例（`executor_tests.rs` 同款，见其文件头），
//! 让 model.rs 保持可读。

use super::*;

// ─── 跨库工作流定义 YAML（原 tc01）──────────────────────────────────────────

#[test]
fn cross_db_workflow_yaml_parses() {
    let yaml = include_str!("../../../../scripts/test-cross-db-workflow.yaml");
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

// ─── 执行结果的结构化序列化（原 tc03）───────────────────────────────────────

#[test]
fn structured_result_serialization_round_trips() {
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

// ─── 连接变量引用（原 tc08）─────────────────────────────────────────────────

#[test]
fn multiple_steps_can_reference_the_same_connection_variable() {
    let yaml = r#"
id: connection-variable-test
name: Connection Variable
description: Connection variable reference
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

// ─── 旧版 YAML 向后兼容（原 tc09）───────────────────────────────────────────

#[test]
fn legacy_workflow_yaml_without_new_fields_still_deserializes() {
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

// ─── IN 子句通配模板（原 tc11）──────────────────────────────────────────────

#[test]
fn wildcard_template_for_in_clause_survives_parsing() {
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

// ─── condition + foreach 嵌套（原 tc12）─────────────────────────────────────

#[test]
fn nested_condition_foreach_parses() {
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

// ─── 以下五例自 model.rs 内联测试区原样迁来（零改动）────────────────────────

#[test]
fn legacy_query_yaml_still_deserializes() {
    let step: WorkflowStep = serde_yaml::from_str("type: query\nid: q\nsql: SELECT 1\n").unwrap();
    assert!(matches!(step, WorkflowStep::Query { .. }));
}

#[test]
fn command_yaml_deserializes() {
    let yaml =
        "type: command\nid: find\ncommand: find\nconnection: mongo\ninput:\n  collection: users\n";
    let step: WorkflowStep = serde_yaml::from_str(yaml).unwrap();
    match step {
        WorkflowStep::Command {
            command,
            connection,
            input,
            ..
        } => {
            assert_eq!(command, "find");
            assert_eq!(connection.as_deref(), Some("mongo"));
            assert_eq!(input["collection"], "users");
        }
        _ => panic!("expected Command"),
    }
}

#[test]
fn workflow_connection_is_optional() {
    let workflow: WorkflowDefinition =
        serde_yaml::from_str("id: w\nname: W\ndescription: d\nsteps: []\n").unwrap();
    assert_eq!(workflow.connection, None);
    assert_eq!(workflow.schedule, None);
}

#[test]
fn workflow_schedule_yaml_deserializes() {
    let yaml = "id: w\nname: W\ndescription: d\nsteps: []\nschedule:\n  enabled: true\n  interval_secs: 120\n";
    let workflow: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    let schedule = workflow.schedule.unwrap();
    assert!(schedule.enabled);
    assert_eq!(schedule.interval_secs, Some(120));
}

#[test]
fn workflow_schedule_accepts_camel_case_interval() {
    let json = r#"{"id":"w","name":"W","description":"d","steps":[],"schedule":{"enabled":true,"intervalSecs":45}}"#;
    let workflow: WorkflowDefinition = serde_json::from_str(json).unwrap();
    assert_eq!(workflow.schedule.unwrap().interval_secs, Some(45));
}
