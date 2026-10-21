//! Portable dashboard run records and the rules that shape them.
//!
//! A host stores the runs a widget produced as rows
//! ([`WidgetRunRecord`]) and provides them through [`DashboardRunStore`].
//! Which status a row carries, when a row belongs to the widget that was
//! asked for, and how a run is shaped for the frontend are decided here.

use super::records::WidgetRunRecord;
use super::types::{MonitorSettings, RunIndexEntry, WidgetRun, WidgetRunStatus};

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
pub trait DashboardRunStore: Send + Sync {
    /// Store a run row and apply the retention policy.
    fn write_run_row(
        &self,
        run: WidgetRunRecord,
        retention_count: u32,
        retention_days: u32,
    ) -> Result<(), DashboardRunsError>;

    /// The stored run rows of a widget, newest first.
    fn list_run_rows(
        &self,
        widget_id: &str,
        limit: u32,
    ) -> Result<Vec<WidgetRunRecord>, DashboardRunsError>;

    /// One stored run row.
    fn run_row(&self, run_id: &str) -> Result<WidgetRunRecord, DashboardRunsError>;
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
    store: &dyn DashboardRunStore,
    run: &WidgetRun,
    retention: &MonitorSettings,
) -> Result<(), DashboardRunsError> {
    let record = run_to_record(run)?;
    store.write_run_row(
        record,
        retention.run_retention_count,
        retention.run_retention_days,
    )
}

/// The run index of a widget.
pub fn list_run_index(
    store: &dyn DashboardRunStore,
    _dashboard_id: &str,
    widget_id: &str,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, DashboardRunsError> {
    let records = store.list_run_rows(widget_id, limit as u32)?;
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
/// the runs of its neighbours.
pub fn get_run(
    store: &dyn DashboardRunStore,
    _dashboard_id: &str,
    widget_id: &str,
    run_id: &str,
) -> Result<WidgetRun, DashboardRunsError> {
    let record = store.run_row(run_id)?;
    if record.widget_id != widget_id {
        return Err(DashboardRunsError::NotFound(run_id.to_string()));
    }
    run_from_record(&record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MemoryRunStore {
        rows: Mutex<Vec<WidgetRunRecord>>,
    }

    impl MemoryRunStore {
        fn new() -> Self {
            Self {
                rows: Mutex::new(Vec::new()),
            }
        }
    }

    impl DashboardRunStore for MemoryRunStore {
        fn write_run_row(
            &self,
            run: WidgetRunRecord,
            _retention_count: u32,
            _retention_days: u32,
        ) -> Result<(), DashboardRunsError> {
            self.rows.lock().unwrap().push(run);
            Ok(())
        }
        fn list_run_rows(
            &self,
            widget_id: &str,
            limit: u32,
        ) -> Result<Vec<WidgetRunRecord>, DashboardRunsError> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|row| row.widget_id == widget_id)
                .take(limit as usize)
                .cloned()
                .collect())
        }
        fn run_row(&self, run_id: &str) -> Result<WidgetRunRecord, DashboardRunsError> {
            self.rows
                .lock()
                .unwrap()
                .iter()
                .find(|row| row.id == run_id)
                .cloned()
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
        let run = sample_run("run-7", "w1");
        write_run(&store, &run, &MonitorSettings::default()).unwrap();
        let loaded = get_run(&store, "d1", "w1", "run-7").unwrap();
        assert_eq!(loaded.id, run.id);
        assert_eq!(loaded.columns, run.columns);
    }

    #[test]
    fn list_run_index_returns_entries() {
        let store = MemoryRunStore::new();
        for index in 0..3 {
            write_run(
                &store,
                &sample_run(&format!("run-{index}"), "w1"),
                &MonitorSettings::default(),
            )
            .unwrap();
        }
        let index = list_run_index(&store, "d1", "w1", 10).unwrap();
        assert_eq!(index.len(), 3);
        assert_eq!(index[0].status, WidgetRunStatus::Ok);
    }

    #[test]
    fn a_run_of_another_widget_is_not_found() {
        let store = MemoryRunStore::new();
        write_run(
            &store,
            &sample_run("run-1", "w1"),
            &MonitorSettings::default(),
        )
        .unwrap();
        assert!(matches!(
            get_run(&store, "d1", "w2", "run-1"),
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
}
