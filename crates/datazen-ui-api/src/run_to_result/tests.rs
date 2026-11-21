use super::*;
use crate::WidgetRun;
use serde_json::json;

fn run() -> WidgetRun {
    match serde_json::from_value(json!({
        "id": "r1",
        "dashboardId": "d1",
        "widgetId": "w1",
        "workflowId": "wf1",
        "startedAt": "2026-01-01T00:00:00Z",
        "finishedAt": "2026-01-01T00:00:01Z",
        "status": "ok",
        "rowCount": 2,
        "columns": ["id", "name"],
        "rows": [[1, "a"], [2, null]]
    })) {
        Ok(value) => value,
        Err(error) => panic!("fixture must decode: {error}"),
    }
}

#[test]
fn the_adapted_result_carries_the_run_columns_and_rows() {
    let adapted = widget_run_to_statement_result(&run());
    assert_eq!(adapted.sql, "");
    assert_eq!(adapted.execution_time_ms, 0);
    assert_eq!(adapted.columns.len(), 2);
    assert_eq!(adapted.columns[0].name, "id");
    assert_eq!(adapted.columns[0].data_type, "unknown");
    assert!(adapted.columns[0].nullable);
    assert_eq!(adapted.columns[1].name, "name");
    assert_eq!(adapted.rows.len(), 2);
    assert_eq!(adapted.rows[0][0], Some(json!(1)));
    assert_eq!(adapted.rows[0][1], Some(json!("a")));
    assert_eq!(adapted.rows[1][1], Some(json!(null)));
}

#[test]
fn absent_legs_are_left_empty() {
    let adapted = widget_run_to_statement_result(&run());
    assert_eq!(adapted.source, None);
    assert_eq!(adapted.runtime_binding, None);
    assert_eq!(adapted.provenance, None);
    assert_eq!(adapted.rows_affected, None);
    assert_eq!(adapted.truncated, None);
    assert_eq!(adapted.pinned, None);
    let empty = WidgetRun {
        columns: Vec::new(),
        rows: Vec::new(),
        ..run()
    };
    let adapted_empty = widget_run_to_statement_result(&empty);
    assert!(adapted_empty.columns.is_empty());
    assert!(adapted_empty.rows.is_empty());
    assert_eq!(adapted_empty.sql, "");
}

#[test]
fn crate_root_reexports_the_adapter() {
    let adapted = crate::widget_run_to_statement_result(&run());
    assert_eq!(adapted.sql, "");
}
