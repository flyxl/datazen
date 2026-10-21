//! Host bindings for the shared dashboard run store.
//!
//! The desktop host keeps the run rows in its SQLite store; the rules that
//! decide what a run looks like live in
//! `datazen_feature_core::dashboard::runs`.
//!
//! ## 组织作用域
//!
//! 桌面是**单租户**：实现只承认自己那一个组织——别的组织的作用域按「不存在」
//! 处理（空列表 / NotFound），不返回「无权限」类错误（CM-05）。

use crate::platform::scope::is_local_scope;
use crate::store::{AppDb, AppDbError};
use datazen_feature_core::dashboard::records::WidgetRunRecord;
use datazen_feature_core::dashboard::runs::{DashboardRunStore, DashboardRunsError};
use datazen_platform_api::ports::scope::OrganizationScope;

fn run_error(error: AppDbError) -> DashboardRunsError {
    DashboardRunsError::Db(error.to_string())
}

impl DashboardRunStore for AppDb {
    fn write_run_row(
        &self,
        scope: &OrganizationScope,
        run: WidgetRunRecord,
        retention_count: u32,
        retention_days: u32,
    ) -> Result<(), DashboardRunsError> {
        if !is_local_scope(scope) {
            return Err(DashboardRunsError::NotFound(run.id.clone()));
        }
        self.write_run(run, retention_count, retention_days)
            .map_err(run_error)
    }

    fn list_run_rows(
        &self,
        scope: &OrganizationScope,
        widget_id: &str,
        limit: u32,
    ) -> Result<Vec<WidgetRunRecord>, DashboardRunsError> {
        if !is_local_scope(scope) {
            return Ok(Vec::new());
        }
        self.list_run_index(widget_id, limit).map_err(run_error)
    }

    fn run_row(
        &self,
        scope: &OrganizationScope,
        run_id: &str,
    ) -> Result<WidgetRunRecord, DashboardRunsError> {
        if !is_local_scope(scope) {
            return Err(DashboardRunsError::NotFound(run_id.to_string()));
        }
        self.get_run(run_id).map_err(run_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_feature_core::dashboard::runs::{get_run, list_run_index, write_run};
    use datazen_feature_core::dashboard::types::{MonitorSettings, WidgetRun, WidgetRunStatus};
    use datazen_platform_api::id::OrganizationId;

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

    /// 桌面自己的组织作用域。
    fn local_scope() -> OrganizationScope {
        crate::platform::local_organization_scope()
    }

    /// 别的组织的作用域。
    fn foreign_scope() -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new("another-organization"))
    }

    #[test]
    fn write_run_roundtrip() {
        let db = open_db();
        let settings = MonitorSettings::default();
        let run = sample_run(7);
        write_run(&local_scope(), &*db, &run, &settings).unwrap();
        let loaded = get_run(&local_scope(), &*db, "d1", "w1", "run-7").unwrap();
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
            write_run(&local_scope(), &*db, &sample_run(i), &settings).unwrap();
        }
        let idx = list_run_index(&local_scope(), &*db, "d1", "w1", 10).unwrap();
        assert_eq!(idx.len(), 3);
    }

    /// 别的组织读不到本组织的运行记录（CM-05：按「不存在」处理，不是「无权限」）。
    #[test]
    fn the_runs_of_another_organization_are_not_found() {
        let db = open_db();
        let settings = MonitorSettings::default();
        write_run(&local_scope(), &*db, &sample_run(7), &settings).unwrap();

        assert!(list_run_index(&foreign_scope(), &*db, "d1", "w1", 10)
            .unwrap()
            .is_empty());
        assert!(matches!(
            get_run(&foreign_scope(), &*db, "d1", "w1", "run-7"),
            Err(DashboardRunsError::NotFound(_))
        ));
    }
}
