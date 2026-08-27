//! Widget execution via WorkflowExecutor for dashboard monitor runs.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::commands::AppState;
use crate::dashboard::alert::evaluate_run_alert;
use crate::dashboard::runs::{write_run, MAX_RUN_ROWS};
use crate::dashboard::store::load_monitor_settings;
use crate::dashboard::types::{DashboardWidget, WidgetRun, WidgetRunStatus};
use crate::store::AppDb;
use crate::store::AppSettings;
use crate::workflow::model::WorkflowExecutionResult;
use crate::workflow::WorkflowExecutor;

/// Default per-widget workflow timeout (seconds).
pub const DEFAULT_QUERY_TIMEOUT_SEC: u64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum DashboardExecuteError {
    #[error("Workflow error: {0}")]
    Workflow(String),

    #[error("Runs error: {0}")]
    Runs(#[from] crate::dashboard::runs::DashboardRunsError),

    #[error("Store error: {0}")]
    Store(#[from] crate::dashboard::store::DashboardStoreError),
}

fn build_error_run(
    run_id: String,
    dashboard_id: &str,
    widget: &DashboardWidget,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    error: &str,
) -> WidgetRun {
    WidgetRun {
        id: run_id,
        dashboard_id: dashboard_id.to_string(),
        widget_id: widget.id.clone(),
        workflow_id: widget.workflow_id.clone(),
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        status: WidgetRunStatus::Error,
        error: Some(error.to_string()),
        row_count: 0,
        columns: Vec::new(),
        rows: Vec::new(),
        alert_fired: None,
        alert_value: None,
    }
}

fn build_timeout_run(
    run_id: String,
    dashboard_id: &str,
    widget: &DashboardWidget,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    timeout_sec: u64,
) -> WidgetRun {
    WidgetRun {
        id: run_id,
        dashboard_id: dashboard_id.to_string(),
        widget_id: widget.id.clone(),
        workflow_id: widget.workflow_id.clone(),
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        status: WidgetRunStatus::Timeout,
        error: Some(format!("Workflow timed out after {timeout_sec}s")),
        row_count: 0,
        columns: Vec::new(),
        rows: Vec::new(),
        alert_fired: None,
        alert_value: None,
    }
}

/// Build and persist an error-status run using monitor retention settings.
#[allow(dead_code)]
pub(crate) fn persist_error_run(
    app_db: &AppDb,
    settings: &AppSettings,
    dashboard_id: &str,
    widget: &DashboardWidget,
    run_id: &str,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    error: &str,
) -> Result<WidgetRun, DashboardExecuteError> {
    let run = build_error_run(
        run_id.to_string(),
        dashboard_id,
        widget,
        started_at,
        finished_at,
        error,
    );
    let retention = load_monitor_settings(settings);
    write_run(app_db, &run, &retention)?;
    Ok(run)
}

fn cap_rows(
    columns: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
) -> (Vec<String>, Vec<Vec<serde_json::Value>>, u32) {
    let row_count = rows.len() as u32;
    if rows.len() > MAX_RUN_ROWS {
        (
            columns,
            rows.into_iter().take(MAX_RUN_ROWS).collect(),
            row_count,
        )
    } else {
        (columns, rows, row_count)
    }
}

/// Parse workflow query step JSON or final_output text into tabular data.
pub fn parse_step_result_value(
    value: &serde_json::Value,
) -> Option<(Vec<String>, Vec<Vec<serde_json::Value>>)> {
    if let Some(cols) = value.get("columns").and_then(|c| c.as_array()) {
        let col_names: Vec<String> = cols
            .iter()
            .filter_map(|c| {
                c.as_str()
                    .map(String::from)
                    .or_else(|| c.get("name").and_then(|n| n.as_str()).map(String::from))
            })
            .collect();
        if let Some(rows_val) = value.get("rows").and_then(|r| r.as_array()) {
            let rows = rows_objects_to_matrix(&col_names, rows_val);
            return Some((col_names, rows));
        }
    }

    if let Some(rows_val) = value.get("rows").and_then(|r| r.as_array()) {
        if rows_val.first().and_then(|r| r.as_object()).is_some() {
            let col_names = rows_val
                .first()
                .and_then(|r| r.as_object())
                .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let rows = rows_objects_to_matrix(&col_names, rows_val);
            return Some((col_names, rows));
        }
    }

    None
}

fn rows_objects_to_matrix(
    col_names: &[String],
    rows_val: &[serde_json::Value],
) -> Vec<Vec<serde_json::Value>> {
    rows_val
        .iter()
        .filter_map(|row| {
            let obj = row.as_object()?;
            Some(
                col_names
                    .iter()
                    .map(|name| obj.get(name).cloned().unwrap_or(serde_json::Value::Null))
                    .collect(),
            )
        })
        .collect()
}

/// Parse workflow final_output string into tabular data.
pub fn parse_final_output_as_table(
    output: &str,
) -> Result<(Vec<String>, Vec<Vec<serde_json::Value>>), String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(table) = parse_step_result_value(&value) {
            return Ok(table);
        }
    }

    Err("Could not parse workflow output as table".into())
}

pub fn extract_table_from_execution(
    result: &WorkflowExecutionResult,
) -> Result<(Vec<String>, Vec<Vec<serde_json::Value>>), String> {
    parse_final_output_as_table(&result.final_output)
}

/// Execute a widget once via WorkflowExecutor (does not write workflow_history).
pub async fn execute_widget_once(
    app_state: &AppState,
    app_db: &AppDb,
    settings: &AppSettings,
    dashboard_id: &str,
    widget: &DashboardWidget,
) -> Result<WidgetRun, DashboardExecuteError> {
    let started_at = Utc::now();
    let run_id = Uuid::new_v4().to_string();
    let timeout_sec = DEFAULT_QUERY_TIMEOUT_SEC;

    let workflow = app_state
        .workflow_registry
        .get(&widget.workflow_id)
        .await
        .ok_or_else(|| {
            DashboardExecuteError::Workflow(format!("Workflow '{}' not found", widget.workflow_id))
        })?;

    let exec_result = tokio::time::timeout(
        Duration::from_secs(timeout_sec),
        WorkflowExecutor::execute(&workflow, app_state, None, &serde_json::json!({})),
    )
    .await;

    let finished_at = Utc::now();

    let mut run = match exec_result {
        Ok(Ok(result)) if result.success => match extract_table_from_execution(&result) {
            Ok((columns, rows)) => {
                let (columns, rows, row_count) = cap_rows(columns, rows);
                WidgetRun {
                    id: run_id,
                    dashboard_id: dashboard_id.to_string(),
                    widget_id: widget.id.clone(),
                    workflow_id: widget.workflow_id.clone(),
                    started_at: started_at.to_rfc3339(),
                    finished_at: finished_at.to_rfc3339(),
                    status: WidgetRunStatus::Ok,
                    error: None,
                    row_count,
                    columns,
                    rows,
                    alert_fired: None,
                    alert_value: None,
                }
            }
            Err(e) => build_error_run(run_id, dashboard_id, widget, started_at, finished_at, &e),
        },
        Ok(Ok(result)) => build_error_run(
            run_id,
            dashboard_id,
            widget,
            started_at,
            finished_at,
            &result.error.unwrap_or_else(|| "Workflow failed".into()),
        ),
        Ok(Err(e)) => build_error_run(run_id, dashboard_id, widget, started_at, finished_at, &e),
        Err(_elapsed) => build_timeout_run(
            run_id,
            dashboard_id,
            widget,
            started_at,
            finished_at,
            timeout_sec,
        ),
    };

    evaluate_run_alert(&mut run, widget);

    let retention = load_monitor_settings(settings);
    write_run(app_db, &run, &retention)?;

    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::runs::{get_run, list_run_index};
    use crate::dashboard::types::{RefreshMode, RefreshPolicy, ViewMode, WidgetLayout};
    use crate::store::AppSettings;
    use crate::workflow::model::{StepExecutionResult, StepStatus, WorkflowExecutionResult};

    fn sample_widget() -> DashboardWidget {
        DashboardWidget {
            id: "w1".into(),
            title: "Test".into(),
            workflow_id: "wf-missing".into(),
            view_mode: ViewMode::Chart,
            chart_config: None,
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 4,
                h: 3,
            },
            refresh: RefreshPolicy::default(),
            alert: None,
            enabled: true,
        }
    }

    #[test]
    fn persist_error_run_writes_index() {
        let db = crate::store::AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&crate::store::WorkflowRecord {
            id: "wf1".into(),
            name: "WF".into(),
            description: String::new(),
            visibility: crate::store::WorkflowVisibility::User,
            definition_yaml: "id: wf1\nname: WF\nsteps: []\n".into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
        db.upsert_dashboard(&crate::store::DashboardRecord {
            id: "d1".into(),
            name: "D".into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            layout_cols: 12,
            layout_row_height: 80,
            enabled: true,
            refresh_paused: false,
        })
        .unwrap();
        db.upsert_widget(&crate::store::WidgetRecord {
            id: "w1".into(),
            dashboard_id: "d1".into(),
            title: "W".into(),
            workflow_id: "wf1".into(),
            view_mode: "chart".into(),
            chart_config_json: None,
            layout_x: 0,
            layout_y: 0,
            layout_w: 4,
            layout_h: 3,
            refresh_mode: "manual".into(),
            refresh_sec: None,
            alert_json: None,
            enabled: true,
            sort_order: 0,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
        let widget = sample_widget();
        let started_at = Utc::now();
        let finished_at = started_at + chrono::Duration::seconds(1);
        let settings = AppSettings::default();

        let run = persist_error_run(
            &db,
            &settings,
            "d1",
            &widget,
            "run-connect-err",
            started_at,
            finished_at,
            "Workflow error: not found",
        )
        .unwrap();

        assert_eq!(run.status, WidgetRunStatus::Error);
        let index = list_run_index(&db, "d1", "w1", 10).unwrap();
        assert_eq!(index.len(), 1);
        let loaded = get_run(&db, "d1", "w1", "run-connect-err").unwrap();
        assert_eq!(loaded.status, WidgetRunStatus::Error);
    }

    #[test]
    fn parse_step_result_value_from_query_shape() {
        let value = serde_json::json!({
            "rows": [
                {"id": 1, "name": "alice"},
                {"id": 2, "name": "bob"}
            ],
            "columns": [
                {"name": "id", "data_type": "int"},
                {"name": "name", "data_type": "text"}
            ]
        });
        let (cols, rows) = parse_step_result_value(&value).unwrap();
        assert_eq!(cols, vec!["id", "name"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], serde_json::json!(1));
    }

    #[test]
    fn parse_final_output_as_table_from_json() {
        let output = r#"{
            "rows": [{"v": 42}],
            "columns": [{"name": "v"}]
        }"#;
        let (cols, rows) = parse_final_output_as_table(output).unwrap();
        assert_eq!(cols, vec!["v"]);
        assert_eq!(rows[0][0], serde_json::json!(42));
    }

    #[test]
    fn extract_table_from_execution_uses_final_output_only() {
        let step_table = StepExecutionResult {
            step_id: "q1".into(),
            step_type: "query".into(),
            status: StepStatus::Success,
            result: Some(serde_json::json!({
                "rows": [{"n": 7}],
                "columns": [{"name": "n"}]
            })),
            execution_time_ms: 1,
            error: None,
            connection_name: None,
            sql_executed: None,
        };

        let invalid_final = WorkflowExecutionResult {
            success: true,
            final_output: "not json".into(),
            steps: vec![step_table.clone()],
            total_time_ms: 1,
            error: None,
        };
        assert!(extract_table_from_execution(&invalid_final).is_err());

        let valid_final = WorkflowExecutionResult {
            success: true,
            final_output: r#"{"rows":[{"v":42}],"columns":[{"name":"v"}]}"#.into(),
            steps: vec![step_table],
            total_time_ms: 1,
            error: None,
        };
        let (cols, rows) = extract_table_from_execution(&valid_final).unwrap();
        assert_eq!(cols, vec!["v"]);
        assert_eq!(rows[0][0], serde_json::json!(42));
    }

    #[test]
    fn timeout_run_status_serializes_as_lowercase() {
        let started_at = Utc::now();
        let finished_at = started_at + chrono::Duration::seconds(60);
        let widget = sample_widget();
        let run = build_timeout_run(
            "run-timeout".into(),
            "d1",
            &widget,
            started_at,
            finished_at,
            DEFAULT_QUERY_TIMEOUT_SEC,
        );
        assert_eq!(run.status, WidgetRunStatus::Timeout);
        let json = serde_json::to_string(&run).unwrap();
        assert!(json.contains("\"status\":\"timeout\""));
    }
}
