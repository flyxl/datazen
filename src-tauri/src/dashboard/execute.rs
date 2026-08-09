//! One-shot widget SQL execution for dashboard monitor runs.
//!
//! Uses a dedicated `ConnectionManager::connect` handle — never `get_or_connect` —
//! so UI session pools are not stolen. Task 7 will introduce a `monitor:{config_id}`
//! registry to reuse monitor handles across scheduled refreshes; Task 8 may refine
//! disconnect/lifecycle policy.

use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::dashboard::runs::{write_run, MAX_RUN_ROWS};
use crate::dashboard::store::load_monitor_settings;
use crate::dashboard::types::{DashboardWidget, WidgetRun, WidgetRunStatus};
use crate::db::{StatementResult, Value};
use crate::services::ConnectionManager;
use crate::store::AppSettings;

#[derive(Debug, thiserror::Error)]
pub enum DashboardExecuteError {
    #[error("Connection error: {0}")]
    Connection(#[from] crate::services::connection_manager::ConnectionError),

    #[error("Driver error: {0}")]
    Driver(#[from] crate::db::DriverError),

    #[error("Runs error: {0}")]
    Runs(#[from] crate::dashboard::runs::DashboardRunsError),
}

fn cell_to_json(value: &Option<Value>) -> serde_json::Value {
    match value {
        None => serde_json::Value::Null,
        Some(v) => match v {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::json!(b),
            Value::Integer(n) => serde_json::json!(n),
            Value::Float(f) => serde_json::json!(f),
            Value::String(s) => serde_json::json!(s),
            Value::Bytes(b) => serde_json::json!(b),
            Value::Timestamp(ts) => serde_json::json!(ts),
            Value::Json(j) => j.clone(),
        },
    }
}

fn statement_to_run_fields(stmt: &StatementResult) -> (Vec<String>, Vec<Vec<serde_json::Value>>, u32) {
    let columns: Vec<String> = stmt.columns.iter().map(|c| c.name.clone()).collect();
    let rows: Vec<Vec<serde_json::Value>> = stmt
        .rows
        .iter()
        .map(|row| row.iter().map(cell_to_json).collect())
        .collect();
    let row_count = rows.len() as u32;
    (columns, rows, row_count)
}

/// Execute a widget query once using a dedicated monitor connection handle.
pub async fn execute_widget_once(
    connection_manager: &ConnectionManager,
    data_dir: &Path,
    settings: &AppSettings,
    dashboard_id: &str,
    widget: &DashboardWidget,
) -> Result<WidgetRun, DashboardExecuteError> {
    let started_at = Utc::now();
    let run_id = Uuid::new_v4().to_string();

    // Dedicated handle — do not reuse UI sessions via get_or_connect.
    let connection_id = connection_manager.connect(&widget.config_id).await?;
    let query_result = async {
        let (driver, handle) = connection_manager
            .get_connection(&connection_id)
            .await
            .map_err(DashboardExecuteError::Connection)?;

        let limit = Some(MAX_RUN_ROWS as u32);
        driver
            .query_multi(&handle, &widget.sql, limit)
            .await
            .map_err(DashboardExecuteError::Driver)
    }
    .await;

    let _ = connection_manager.disconnect(&connection_id).await;

    let finished_at = Utc::now();

    let run = match query_result {
        Ok(multi) => {
            let results = multi.results;
            let stmt = results
                .iter()
                .find(|r| !r.columns.is_empty() || !r.rows.is_empty() || r.rows_affected.is_some())
                .or_else(|| results.first());

            match stmt {
                Some(stmt) => {
                    let (columns, rows, row_count) = statement_to_run_fields(stmt);
                    WidgetRun {
                        id: run_id,
                        dashboard_id: dashboard_id.to_string(),
                        widget_id: widget.id.clone(),
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
                None => WidgetRun {
                    id: run_id,
                    dashboard_id: dashboard_id.to_string(),
                    widget_id: widget.id.clone(),
                    started_at: started_at.to_rfc3339(),
                    finished_at: finished_at.to_rfc3339(),
                    status: WidgetRunStatus::Ok,
                    error: None,
                    row_count: 0,
                    columns: Vec::new(),
                    rows: Vec::new(),
                    alert_fired: None,
                    alert_value: None,
                },
            }
        }
        Err(err) => WidgetRun {
            id: run_id,
            dashboard_id: dashboard_id.to_string(),
            widget_id: widget.id.clone(),
            started_at: started_at.to_rfc3339(),
            finished_at: finished_at.to_rfc3339(),
            status: WidgetRunStatus::Error,
            error: Some(err.to_string()),
            row_count: 0,
            columns: Vec::new(),
            rows: Vec::new(),
            alert_fired: None,
            alert_value: None,
        },
    };

    let retention = load_monitor_settings(settings);
    write_run(data_dir, &run, &retention)?;

    Ok(run)
}
