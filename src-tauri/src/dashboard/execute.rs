//! One-shot widget SQL execution for dashboard monitor runs.
//!
//! Uses [`MonitorConnectionRegistry`] — never UI session pools via `get_or_connect`.

use std::path::Path;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::dashboard::runs::{write_run, MAX_RUN_ROWS};
use crate::dashboard::store::load_monitor_settings;
use crate::dashboard::types::{DashboardWidget, WidgetRun, WidgetRunStatus};
use crate::db::{StatementResult, Value};
use crate::monitor::MonitorConnectionRegistry;
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

fn build_error_run(
    run_id: String,
    dashboard_id: &str,
    widget_id: &str,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    error: &str,
) -> WidgetRun {
    WidgetRun {
        id: run_id,
        dashboard_id: dashboard_id.to_string(),
        widget_id: widget_id.to_string(),
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

/// Build and persist an error-status run using monitor retention settings.
pub(crate) fn persist_error_run(
    data_dir: &Path,
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
        &widget.id,
        started_at,
        finished_at,
        error,
    );
    let retention = load_monitor_settings(settings);
    write_run(data_dir, &run, &retention)?;
    Ok(run)
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

/// Execute a widget query once using the monitor connection registry.
pub async fn execute_widget_once(
    monitor_connections: &MonitorConnectionRegistry,
    data_dir: &Path,
    settings: &AppSettings,
    dashboard_id: &str,
    widget: &DashboardWidget,
) -> Result<WidgetRun, DashboardExecuteError> {
    let started_at = Utc::now();
    let run_id = Uuid::new_v4().to_string();

    let query_result = async {
        let (driver, handle) = monitor_connections
            .get_or_connect_monitor(&widget.config_id)
            .await
            .map_err(DashboardExecuteError::Connection)?;

        let limit = Some(MAX_RUN_ROWS as u32);
        driver
            .query_multi(&handle, &widget.sql, limit)
            .await
            .map_err(DashboardExecuteError::Driver)
    }
    .await;

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
        Err(err) => build_error_run(
            run_id,
            dashboard_id,
            &widget.id,
            started_at,
            finished_at,
            &err.to_string(),
        ),
    };

    let retention = load_monitor_settings(settings);
    write_run(data_dir, &run, &retention)?;

    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::runs::{get_run, list_run_index};
    use crate::dashboard::types::{
        AggregationType, ChartConfig, ChartSortBy, ChartType, WidgetLayout,
    };
    use crate::store::AppSettings;

    fn sample_widget() -> DashboardWidget {
        DashboardWidget {
            id: "w1".into(),
            title: "Test".into(),
            config_id: "cfg-missing".into(),
            sql: "SELECT 1".into(),
            chart_config: ChartConfig {
                chart_type: ChartType::Line,
                x_axis: None,
                y_axes: vec![],
                group_by: None,
                aggregation: AggregationType::None,
                sort_by: ChartSortBy::None,
                show_legend: true,
                show_grid: true,
                show_values: false,
                color_scheme: "default".into(),
            },
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 4,
                h: 3,
            },
            refresh_sec: 60,
            alert: None,
            enabled: true,
        }
    }

    #[test]
    fn persist_error_run_writes_index_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let widget = sample_widget();
        let started_at = Utc::now();
        let finished_at = started_at + chrono::Duration::seconds(1);
        let settings = AppSettings::default();

        let run = persist_error_run(
            dir.path(),
            &settings,
            "d1",
            &widget,
            "run-connect-err",
            started_at,
            finished_at,
            "Connection error: config not found",
        )
        .unwrap();

        assert_eq!(run.status, WidgetRunStatus::Error);
        assert_eq!(
            run.error.as_deref(),
            Some("Connection error: config not found")
        );

        let index = list_run_index(dir.path(), "d1", "w1", 10).unwrap();
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].id, "run-connect-err");
        assert_eq!(index[0].status, WidgetRunStatus::Error);

        let loaded = get_run(dir.path(), "d1", "w1", "run-connect-err").unwrap();
        assert_eq!(loaded.status, WidgetRunStatus::Error);
        assert_eq!(loaded.error, run.error);
    }
}
