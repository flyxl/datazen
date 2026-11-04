//! Widget workflow execution, result shaping and alert evaluation.
use crate::dashboard::application::alert::evaluate_run_alert;
use crate::dashboard::domain::types::{DashboardWidget, WidgetRun, WidgetRunStatus};
use crate::workflow::domain::model::WorkflowExecutionResult;
use chrono::{DateTime, Utc};
use std::time::Duration;
use uuid::Uuid;
pub const MAX_RUN_ROWS: usize = 500;
pub const DEFAULT_QUERY_TIMEOUT_SEC: u64 = 60;
#[async_trait::async_trait]
pub trait WidgetExecutionHost: Send + Sync {
    async fn execute_workflow(&self, workflow_id: &str) -> Result<WorkflowExecutionResult, String>;
    async fn persist_run(&self, run: &WidgetRun) -> Result<(), String>;
}
pub fn build_error_run(
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

pub fn build_timeout_run(
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

pub async fn execute_widget_once(
    host: &dyn WidgetExecutionHost,
    dashboard_id: &str,
    widget: &DashboardWidget,
) -> Result<WidgetRun, String> {
    let started_at = Utc::now();
    let run_id = Uuid::new_v4().to_string();
    let timeout_sec = DEFAULT_QUERY_TIMEOUT_SEC;

    let exec_result = tokio::time::timeout(
        Duration::from_secs(timeout_sec),
        host.execute_workflow(&widget.workflow_id),
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

    host.persist_run(&run).await?;

    Ok(run)
}
