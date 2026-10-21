//! Host bindings for the shared dashboard run store.
//!
//! The desktop host keeps the run rows in its SQLite store; the rules that
//! decide what a run looks like live in
//! `datazen_feature_core::dashboard::runs`.

use crate::store::{AppDb, AppDbError};
use datazen_feature_core::dashboard::records::WidgetRunRecord;
use datazen_feature_core::dashboard::runs::{DashboardRunStore, DashboardRunsError};

fn run_error(error: AppDbError) -> DashboardRunsError {
    DashboardRunsError::Db(error.to_string())
}

impl DashboardRunStore for AppDb {
    fn write_run_row(
        &self,
        run: WidgetRunRecord,
        retention_count: u32,
        retention_days: u32,
    ) -> Result<(), DashboardRunsError> {
        self.write_run(run, retention_count, retention_days)
            .map_err(run_error)
    }

    fn list_run_rows(
        &self,
        widget_id: &str,
        limit: u32,
    ) -> Result<Vec<WidgetRunRecord>, DashboardRunsError> {
        self.list_run_index(widget_id, limit).map_err(run_error)
    }

    fn run_row(&self, run_id: &str) -> Result<WidgetRunRecord, DashboardRunsError> {
        self.get_run(run_id).map_err(run_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_feature_core::dashboard::runs::{get_run, list_run_index, write_run};
    use datazen_feature_core::dashboard::types::{MonitorSettings, WidgetRun, WidgetRunStatus};

    fn open_db() -> std::sync::Arc<AppDb> {
        let db = AppDb::open_in_memory().unwrap();
        db.upsert_workflow(&crate::store::WorkflowRecord {
            id: "wf1".into(),
            name: "WF".into(),
            description: String::new(),
            visibility: crate::store::WorkflowVisibility::User,
            definition_yaml: "id: wf1\nname: WF\nsteps: []\n".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
        .unwrap();
        db.upsert_dashboard(&crate::store::DashboardRecord {
            id: "d1".into(),
            name: "Dash".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
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
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
        .unwrap();
        db
    }

    fn sample_run(i: u32) -> WidgetRun {
        let started_at = chrono::Utc::now().to_rfc3339();
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
        write_run(&*db, &run, &settings).unwrap();
        let loaded = get_run(&*db, "d1", "w1", "run-7").unwrap();
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
            write_run(&*db, &sample_run(i), &settings).unwrap();
        }
        let idx = list_run_index(&*db, "d1", "w1", 10).unwrap();
        assert_eq!(idx.len(), 3);
    }
}
