use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::types::{MonitorSettings, WidgetRun, WidgetRunStatus};
use crate::store::{AppDb, AppDbError, WidgetRunRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunIndexEntry {
    pub id: String,
    pub started_at: String,
    pub status: WidgetRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_fired: Option<bool>,
}

#[derive(Debug, Error)]
pub enum DashboardRunsError {
    #[error("AppDb error: {0}")]
    Db(#[from] AppDbError),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

pub const MAX_RUN_ROWS: usize = 500;

fn parse_status(s: &str) -> Result<WidgetRunStatus, DashboardRunsError> {
    match s {
        "ok" => Ok(WidgetRunStatus::Ok),
        "error" => Ok(WidgetRunStatus::Error),
        "timeout" => Ok(WidgetRunStatus::Timeout),
        other => Err(DashboardRunsError::Parse(format!(
            "invalid run status: {other}"
        ))),
    }
}

fn status_str(status: WidgetRunStatus) -> &'static str {
    match status {
        WidgetRunStatus::Ok => "ok",
        WidgetRunStatus::Error => "error",
        WidgetRunStatus::Timeout => "timeout",
    }
}

fn run_to_record(run: &WidgetRun) -> Result<WidgetRunRecord, DashboardRunsError> {
    Ok(WidgetRunRecord {
        id: run.id.clone(),
        dashboard_id: run.dashboard_id.clone(),
        widget_id: run.widget_id.clone(),
        workflow_id: run.workflow_id.clone(),
        started_at: run.started_at.clone(),
        finished_at: run.finished_at.clone(),
        status: status_str(run.status).into(),
        error: run.error.clone(),
        row_count: run.row_count,
        columns_json: serde_json::to_string(&run.columns)
            .map_err(|e| DashboardRunsError::Parse(e.to_string()))?,
        rows_json: serde_json::to_string(&run.rows)
            .map_err(|e| DashboardRunsError::Parse(e.to_string()))?,
        variables_json: None,
        alert_fired: run.alert_fired,
        alert_value: run.alert_value,
    })
}

fn run_from_record(record: &WidgetRunRecord) -> Result<WidgetRun, DashboardRunsError> {
    let columns: Vec<String> = serde_json::from_str(&record.columns_json)
        .map_err(|e| DashboardRunsError::Parse(e.to_string()))?;
    let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&record.rows_json)
        .map_err(|e| DashboardRunsError::Parse(e.to_string()))?;
    Ok(WidgetRun {
        id: record.id.clone(),
        dashboard_id: record.dashboard_id.clone(),
        widget_id: record.widget_id.clone(),
        workflow_id: record.workflow_id.clone(),
        started_at: record.started_at.clone(),
        finished_at: record.finished_at.clone(),
        status: parse_status(&record.status)?,
        error: record.error.clone(),
        row_count: record.row_count,
        columns,
        rows,
        alert_fired: record.alert_fired,
        alert_value: record.alert_value,
    })
}

pub fn write_run(
    app_db: &AppDb,
    run: &WidgetRun,
    retention: &MonitorSettings,
) -> Result<(), DashboardRunsError> {
    let record = run_to_record(run)?;
    app_db.write_run(
        record,
        retention.run_retention_count,
        retention.run_retention_days,
    )?;
    Ok(())
}

pub fn list_run_index(
    app_db: &AppDb,
    _dashboard_id: &str,
    widget_id: &str,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, DashboardRunsError> {
    let records = app_db.list_run_index(widget_id, limit as u32)?;
    Ok(records
        .into_iter()
        .map(|r| RunIndexEntry {
            id: r.id,
            started_at: r.started_at,
            status: parse_status(&r.status).unwrap_or(WidgetRunStatus::Error),
            alert_fired: r.alert_fired,
        })
        .collect())
}

pub fn get_run(
    app_db: &AppDb,
    _dashboard_id: &str,
    widget_id: &str,
    run_id: &str,
) -> Result<WidgetRun, DashboardRunsError> {
    let record = app_db.get_run(run_id)?;
    if record.widget_id != widget_id {
        return Err(DashboardRunsError::NotFound(run_id.to_string()));
    }
    run_from_record(&record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::AppDb;
    use chrono::Utc;

    fn open_db() -> std::sync::Arc<AppDb> {
        let db = AppDb::open_in_memory().unwrap();
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
            name: "Dash".into(),
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
            layout_w: 6,
            layout_h: 4,
            refresh_mode: "manual".into(),
            refresh_sec: None,
            alert_json: None,
            enabled: true,
            sort_order: 0,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .unwrap();
        db
    }

    fn sample_run(i: u32) -> WidgetRun {
        let started_at = Utc::now().to_rfc3339();
        WidgetRun {
            id: format!("run-{i}"),
            dashboard_id: "d1".into(),
            widget_id: "w1".into(),
            workflow_id: "wf1".into(),
            started_at: started_at.clone(),
            finished_at: started_at,
            status: WidgetRunStatus::Ok,
            error: None,
            row_count: 1,
            columns: vec!["v".into()],
            rows: vec![vec![serde_json::json!(i)]],
            alert_fired: None,
            alert_value: None,
        }
    }

    #[test]
    fn write_run_roundtrip() {
        let db = open_db();
        let settings = MonitorSettings::default();
        let run = sample_run(7);
        write_run(&db, &run, &settings).unwrap();
        let loaded = get_run(&db, "d1", "w1", "run-7").unwrap();
        assert_eq!(loaded.id, run.id);
        assert_eq!(loaded.columns, run.columns);
    }

    #[test]
    fn list_run_index_returns_entries() {
        let db = open_db();
        let settings = MonitorSettings {
            run_retention_count: 10,
            ..MonitorSettings::default()
        };
        for i in 0..3 {
            write_run(&db, &sample_run(i), &settings).unwrap();
        }
        let idx = list_run_index(&db, "d1", "w1", 10).unwrap();
        assert_eq!(idx.len(), 3);
    }
}
