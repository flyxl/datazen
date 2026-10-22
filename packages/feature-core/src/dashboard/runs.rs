//! Portable dashboard run records and the rules that shape them.
//!
//! A host stores the runs a widget produced as rows
//! ([`WidgetRunRecord`]) and provides them through [`DashboardRunStore`].
//! Which status a row carries, when a row belongs to the widget that was
//! asked for, and how a run is shaped for the frontend are decided here.

use super::records::WidgetRunRecord;
use super::types::{MonitorSettings, RunIndexEntry, WidgetRun, WidgetRunStatus};
use datazen_platform_api::ports::scope::OrganizationScope;

/// Why a dashboard run operation failed.
#[derive(Debug, thiserror::Error)]
pub enum DashboardRunsError {
    #[error("Store error: {0}")]
    Db(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

/// The rows a host stores widget runs as.
///
/// The desktop host implements this over its SQLite store; another host may
/// implement it over its own persistence.
///
/// ## 组织作用域
///
/// 每个方法都带一个 [`OrganizationScope`]：运行记录与仪表板、widget 一样属于
/// 某个组织。越组织访问一律按「不存在」处理（空列表 / `NotFound`），
/// **不得**返回「无权限」类错误（CM-05）。
pub trait DashboardRunStore: Send + Sync {
    /// Store a run row and apply the retention policy.
    fn write_run_row(
        &self,
        scope: &OrganizationScope,
        run: WidgetRunRecord,
        retention_count: u32,
        retention_days: u32,
    ) -> Result<(), DashboardRunsError>;

    /// The stored run rows of a widget, newest first.
    fn list_run_rows(
        &self,
        scope: &OrganizationScope,
        widget_id: &str,
        limit: u32,
    ) -> Result<Vec<WidgetRunRecord>, DashboardRunsError>;

    /// One stored run row.
    fn run_row(
        &self,
        scope: &OrganizationScope,
        run_id: &str,
    ) -> Result<WidgetRunRecord, DashboardRunsError>;
}

/// Read the stored spelling of a run status.
pub fn parse_status(value: &str) -> Result<WidgetRunStatus, DashboardRunsError> {
    match value {
        "ok" => Ok(WidgetRunStatus::Ok),
        "error" => Ok(WidgetRunStatus::Error),
        "timeout" => Ok(WidgetRunStatus::Timeout),
        other => Err(DashboardRunsError::Parse(format!(
            "invalid run status: {other}"
        ))),
    }
}

/// The stored spelling of a run status.
pub fn status_str(status: WidgetRunStatus) -> &'static str {
    match status {
        WidgetRunStatus::Ok => "ok",
        WidgetRunStatus::Error => "error",
        WidgetRunStatus::Timeout => "timeout",
    }
}

/// Turn a run into the row a host stores it as.
pub fn run_to_record(run: &WidgetRun) -> Result<WidgetRunRecord, DashboardRunsError> {
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

/// Turn a stored run row back into a run.
pub fn run_from_record(record: &WidgetRunRecord) -> Result<WidgetRun, DashboardRunsError> {
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

/// Store a run and apply the retention policy of the monitor settings.
pub fn write_run(
    scope: &OrganizationScope,
    store: &dyn DashboardRunStore,
    run: &WidgetRun,
    retention: &MonitorSettings,
) -> Result<(), DashboardRunsError> {
    let record = run_to_record(run)?;
    store.write_run_row(
        scope,
        record,
        retention.run_retention_count,
        retention.run_retention_days,
    )
}

/// The run index of a widget.
pub fn list_run_index(
    scope: &OrganizationScope,
    store: &dyn DashboardRunStore,
    _dashboard_id: &str,
    widget_id: &str,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, DashboardRunsError> {
    let records = store.list_run_rows(scope, widget_id, limit as u32)?;
    Ok(records
        .into_iter()
        .map(|record| RunIndexEntry {
            id: record.id,
            started_at: record.started_at,
            status: parse_status(&record.status).unwrap_or(WidgetRunStatus::Error),
            alert_fired: record.alert_fired,
        })
        .collect())
}

/// One run of a widget.
///
/// A run that belongs to another widget is not found: a widget never sees
/// the runs of its neighbours. A run of another **organization** is not found
/// either — the scope is the organization boundary (CM-05).
pub fn get_run(
    scope: &OrganizationScope,
    store: &dyn DashboardRunStore,
    _dashboard_id: &str,
    widget_id: &str,
    run_id: &str,
) -> Result<WidgetRun, DashboardRunsError> {
    let record = store.run_row(scope, run_id)?;
    if record.widget_id != widget_id {
        return Err(DashboardRunsError::NotFound(run_id.to_string()));
    }
    run_from_record(&record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::OrganizationId;
    use std::sync::Mutex;

    /// 一个**按组织分区**的内存运行存储：多个组织的运行记录并存于同一份数据。
    struct MemoryRunStore {
        rows: Mutex<Vec<(OrganizationId, WidgetRunRecord)>>,
    }

    impl MemoryRunStore {
        fn new() -> Self {
            Self {
                rows: Mutex::new(Vec::new()),
            }
        }
    }

    /// 某个组织的作用域。
    fn scope_of(organization: &str) -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new(organization))
    }

    impl DashboardRunStore for MemoryRunStore {
        fn write_run_row(
            &self,
            scope: &OrganizationScope,
            run: WidgetRunRecord,
            _retention_count: u32,
            _retention_days: u32,
        ) -> Result<(), DashboardRunsError> {
            self.rows
                .lock()
                .unwrap()
                .push((scope.organization_id().clone(), run));
            Ok(())
        }
        fn list_run_rows(
            &self,
            scope: &OrganizationScope,
            widget_id: &str,
            limit: u32,
        ) -> Result<Vec<WidgetRunRecord>, DashboardRunsError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|(owner, row)| {
                    owner == scope.organization_id() && row.widget_id == widget_id
                })
                .take(limit as usize)
                .map(|(_, row)| row.clone())
                .collect())
        }
        fn run_row(
            &self,
            scope: &OrganizationScope,
            run_id: &str,
        ) -> Result<WidgetRunRecord, DashboardRunsError> {
            self.rows
                .lock()
                .unwrap()
                .iter()
                .find(|(owner, row)| owner == scope.organization_id() && row.id == run_id)
                .map(|(_, row)| row.clone())
                .ok_or_else(|| DashboardRunsError::NotFound(run_id.to_string()))
        }
    }

    fn sample_run(id: &str, widget_id: &str) -> WidgetRun {
        WidgetRun {
            id: id.into(),
            dashboard_id: "d1".into(),
            widget_id: widget_id.into(),
            workflow_id: "wf1".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: "2026-01-01T00:00:01Z".into(),
            status: WidgetRunStatus::Ok,
            error: None,
            row_count: 1,
            columns: vec!["v".into()],
            rows: vec![vec![serde_json::json!(1)]],
            alert_fired: None,
            alert_value: None,
        }
    }

    #[test]
    fn write_run_roundtrip() {
        let store = MemoryRunStore::new();
        let scope = scope_of("org-a");
        let run = sample_run("run-7", "w1");
        write_run(&scope, &store, &run, &MonitorSettings::default()).unwrap();
        let loaded = get_run(&scope, &store, "d1", "w1", "run-7").unwrap();
        assert_eq!(loaded.id, run.id);
        assert_eq!(loaded.columns, run.columns);
    }

    #[test]
    fn list_run_index_returns_entries() {
        let store = MemoryRunStore::new();
        let scope = scope_of("org-a");
        for index in 0..3 {
            write_run(
                &scope,
                &store,
                &sample_run(&format!("run-{index}"), "w1"),
                &MonitorSettings::default(),
            )
            .unwrap();
        }
        let index = list_run_index(&scope, &store, "d1", "w1", 10).unwrap();
        assert_eq!(index.len(), 3);
        assert_eq!(index[0].status, WidgetRunStatus::Ok);
    }

    #[test]
    fn a_run_of_another_widget_is_not_found() {
        let store = MemoryRunStore::new();
        let scope = scope_of("org-a");
        write_run(
            &scope,
            &store,
            &sample_run("run-1", "w1"),
            &MonitorSettings::default(),
        )
        .unwrap();
        assert!(matches!(
            get_run(&scope, &store, "d1", "w2", "run-1"),
            Err(DashboardRunsError::NotFound(_))
        ));
    }

    #[test]
    fn run_status_spellings_round_trip() {
        for status in [
            WidgetRunStatus::Ok,
            WidgetRunStatus::Error,
            WidgetRunStatus::Timeout,
        ] {
            assert_eq!(parse_status(status_str(status)).unwrap(), status);
        }
        assert!(parse_status("nope").is_err());
    }

    // ─── 组织隔离 ───

    /// 别的组织的运行记录读不到：索引为空、单条 `NotFound`（CM-05）。
    #[test]
    fn the_runs_of_another_organization_are_not_found() {
        let store = MemoryRunStore::new();
        let owner = scope_of("org-a");
        let other = scope_of("org-b");
        write_run(
            &owner,
            &store,
            &sample_run("run-1", "w1"),
            &MonitorSettings::default(),
        )
        .unwrap();

        assert!(list_run_index(&other, &store, "d1", "w1", 10)
            .unwrap()
            .is_empty());
        assert!(matches!(
            get_run(&other, &store, "d1", "w1", "run-1"),
            Err(DashboardRunsError::NotFound(_))
        ));
    }

    /// 两个组织的运行记录互不可见。
    #[test]
    fn two_organizations_keep_their_own_runs_apart() {
        let store = MemoryRunStore::new();
        let a = scope_of("org-a");
        let b = scope_of("org-b");
        write_run(
            &a,
            &store,
            &sample_run("run-a", "w1"),
            &MonitorSettings::default(),
        )
        .unwrap();
        write_run(
            &b,
            &store,
            &sample_run("run-b", "w1"),
            &MonitorSettings::default(),
        )
        .unwrap();

        let index_a = list_run_index(&a, &store, "d1", "w1", 10).unwrap();
        let index_b = list_run_index(&b, &store, "d1", "w1", 10).unwrap();
        assert_eq!(index_a.len(), 1);
        assert_eq!(index_b.len(), 1);
        assert_eq!(index_a[0].id, "run-a");
        assert_eq!(index_b[0].id, "run-b");
    }
}
