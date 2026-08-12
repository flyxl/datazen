use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::types::{MonitorSettings, WidgetRun, WidgetRunStatus};

pub const RUNS_DIR: &str = "dashboard-runs";
pub const INDEX_FILE: &str = "index.jsonl";
pub const MAX_RUN_ROWS: usize = 500;

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
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

fn widget_runs_dir(data_dir: &Path, dashboard_id: &str, widget_id: &str) -> PathBuf {
    data_dir.join(RUNS_DIR).join(dashboard_id).join(widget_id)
}

fn index_path(data_dir: &Path, dashboard_id: &str, widget_id: &str) -> PathBuf {
    widget_runs_dir(data_dir, dashboard_id, widget_id).join(INDEX_FILE)
}

fn parse_started_at(value: &str) -> Result<DateTime<Utc>, DashboardRunsError> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| DashboardRunsError::Parse(format!("invalid started_at `{value}`: {e}")))
}

fn run_file_path(
    data_dir: &Path,
    dashboard_id: &str,
    widget_id: &str,
    started_at: &str,
    run_id: &str,
) -> Result<PathBuf, DashboardRunsError> {
    let dt = parse_started_at(started_at)?;
    Ok(widget_runs_dir(data_dir, dashboard_id, widget_id)
        .join(format!("{:04}", dt.year()))
        .join(format!("{:02}", dt.month()))
        .join(format!("{run_id}.json")))
}

fn cap_run_rows(run: &mut WidgetRun) {
    if run.rows.len() > MAX_RUN_ROWS {
        run.rows.truncate(MAX_RUN_ROWS);
    }
}

fn read_index_entries(path: &Path) -> Result<Vec<RunIndexEntry>, DashboardRunsError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: RunIndexEntry = serde_json::from_str(line)
            .map_err(|e| DashboardRunsError::Parse(format!("index line {}: {e}", line_no + 1)))?;
        entries.push(entry);
    }
    Ok(entries)
}

fn write_index_entries(path: &Path, entries: &[RunIndexEntry]) -> Result<(), DashboardRunsError> {
    let parent = path.parent().ok_or_else(|| {
        DashboardRunsError::Io(io::Error::new(io::ErrorKind::NotFound, "no parent"))
    })?;
    std::fs::create_dir_all(parent)?;

    let tmp_path = parent.join(format!(".{INDEX_FILE}.tmp"));
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        for entry in entries {
            serde_json::to_writer(&mut file, entry)
                .map_err(|e| DashboardRunsError::Parse(e.to_string()))?;
            file.write_all(b"\n")?;
        }
    }
    std::fs::rename(tmp_path, path)?;
    Ok(())
}

fn append_index_entry(path: &Path, entry: &RunIndexEntry) -> Result<(), DashboardRunsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut file, entry)
        .map_err(|e| DashboardRunsError::Parse(e.to_string()))?;
    file.write_all(b"\n")?;
    Ok(())
}

fn write_run_file(path: &Path, run: &WidgetRun) -> Result<(), DashboardRunsError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let parent = path.parent().ok_or_else(|| {
        DashboardRunsError::Io(io::Error::new(io::ErrorKind::NotFound, "no parent"))
    })?;
    let file_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        DashboardRunsError::Io(io::Error::new(io::ErrorKind::InvalidInput, "bad name"))
    })?;
    let tmp_path = parent.join(format!(".{file_name}.tmp"));

    let content =
        serde_json::to_string_pretty(run).map_err(|e| DashboardRunsError::Parse(e.to_string()))?;
    std::fs::write(&tmp_path, content.as_bytes())?;
    std::fs::rename(tmp_path, path)?;
    Ok(())
}

fn prune_runs(
    data_dir: &Path,
    dashboard_id: &str,
    widget_id: &str,
    retention: &MonitorSettings,
) -> Result<(), DashboardRunsError> {
    let index = index_path(data_dir, dashboard_id, widget_id);
    let mut entries = read_index_entries(&index)?;
    if entries.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    let max_age = Duration::days(retention.run_retention_days as i64);

    entries.retain(|entry| {
        if retention.run_retention_days == 0 {
            return true;
        }
        parse_started_at(&entry.started_at)
            .map(|started| now.signed_duration_since(started) <= max_age)
            .unwrap_or(false)
    });

    entries.sort_by(|a, b| {
        parse_started_at(&b.started_at)
            .unwrap_or(DateTime::UNIX_EPOCH)
            .cmp(&parse_started_at(&a.started_at).unwrap_or(DateTime::UNIX_EPOCH))
    });

    let keep_count = retention.run_retention_count as usize;
    let kept: Vec<RunIndexEntry> = if keep_count == 0 {
        Vec::new()
    } else {
        entries.into_iter().take(keep_count).collect()
    };

    let kept_ids: std::collections::HashSet<&str> = kept.iter().map(|e| e.id.as_str()).collect();

    let all_entries = read_index_entries(&index)?;
    for entry in all_entries {
        if kept_ids.contains(entry.id.as_str()) {
            continue;
        }
        if let Ok(path) = run_file_path(
            data_dir,
            dashboard_id,
            widget_id,
            &entry.started_at,
            &entry.id,
        ) {
            let _ = std::fs::remove_file(path);
        }
    }

    let mut kept_chronological = kept;
    kept_chronological.sort_by(|a, b| {
        parse_started_at(&a.started_at)
            .unwrap_or(DateTime::UNIX_EPOCH)
            .cmp(&parse_started_at(&b.started_at).unwrap_or(DateTime::UNIX_EPOCH))
    });

    write_index_entries(&index, &kept_chronological)
}

pub fn write_run(
    data_dir: &Path,
    run: &WidgetRun,
    retention: &MonitorSettings,
) -> Result<(), DashboardRunsError> {
    let mut stored = run.clone();
    cap_run_rows(&mut stored);

    let run_path = run_file_path(
        data_dir,
        &stored.dashboard_id,
        &stored.widget_id,
        &stored.started_at,
        &stored.id,
    )?;
    write_run_file(&run_path, &stored)?;

    let index_entry = RunIndexEntry {
        id: stored.id.clone(),
        started_at: stored.started_at.clone(),
        status: stored.status,
        alert_fired: stored.alert_fired,
    };
    append_index_entry(
        &index_path(data_dir, &stored.dashboard_id, &stored.widget_id),
        &index_entry,
    )?;

    prune_runs(data_dir, &stored.dashboard_id, &stored.widget_id, retention)
}

pub fn list_run_index(
    data_dir: &Path,
    dashboard_id: &str,
    widget_id: &str,
    limit: usize,
) -> Result<Vec<RunIndexEntry>, DashboardRunsError> {
    let mut entries = read_index_entries(&index_path(data_dir, dashboard_id, widget_id))?;
    entries.sort_by(|a, b| {
        parse_started_at(&b.started_at)
            .unwrap_or(DateTime::UNIX_EPOCH)
            .cmp(&parse_started_at(&a.started_at).unwrap_or(DateTime::UNIX_EPOCH))
    });
    entries.truncate(limit);
    Ok(entries)
}

pub fn get_run(
    data_dir: &Path,
    dashboard_id: &str,
    widget_id: &str,
    run_id: &str,
) -> Result<WidgetRun, DashboardRunsError> {
    let entries = read_index_entries(&index_path(data_dir, dashboard_id, widget_id))?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == run_id)
        .ok_or_else(|| DashboardRunsError::NotFound(run_id.to_string()))?;

    let path = run_file_path(data_dir, dashboard_id, widget_id, &entry.started_at, run_id)?;
    if !path.exists() {
        return Err(DashboardRunsError::NotFound(run_id.to_string()));
    }

    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(|e| DashboardRunsError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::WidgetRunStatus;

    fn sample_run(i: u32) -> WidgetRun {
        let started_at = Utc::now() - Duration::seconds(i as i64);
        let started_at = started_at.to_rfc3339();
        WidgetRun {
            id: format!("run-{i}"),
            dashboard_id: "d1".into(),
            widget_id: "w1".into(),
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
    fn prune_keeps_retention_count() {
        let dir = tempfile::tempdir().unwrap();
        let settings = MonitorSettings {
            run_retention_count: 3,
            run_retention_days: 30,
            ..MonitorSettings::default()
        };
        for i in 0..5 {
            let run = sample_run(i);
            write_run(dir.path(), &run, &settings).unwrap();
        }
        let idx = list_run_index(dir.path(), "d1", "w1", 100).unwrap();
        assert_eq!(idx.len(), 3);
        assert_eq!(idx[0].id, "run-0");
        assert_eq!(idx[1].id, "run-1");
        assert_eq!(idx[2].id, "run-2");
    }

    #[test]
    fn write_run_caps_rows_at_500() {
        let dir = tempfile::tempdir().unwrap();
        let settings = MonitorSettings::default();
        let mut run = sample_run(0);
        run.rows = (0..600).map(|i| vec![serde_json::json!(i)]).collect();
        run.row_count = 600;
        write_run(dir.path(), &run, &settings).unwrap();
        let loaded = get_run(dir.path(), "d1", "w1", "run-0").unwrap();
        assert_eq!(loaded.rows.len(), 500);
        assert_eq!(loaded.row_count, 600);
    }

    #[test]
    fn get_run_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let settings = MonitorSettings::default();
        let run = sample_run(7);
        write_run(dir.path(), &run, &settings).unwrap();
        let loaded = get_run(dir.path(), "d1", "w1", "run-7").unwrap();
        assert_eq!(loaded, run);
    }

    #[test]
    fn prune_drops_runs_older_than_retention_days() {
        let dir = tempfile::tempdir().unwrap();
        let settings = MonitorSettings {
            run_retention_count: 100,
            run_retention_days: 7,
            ..MonitorSettings::default()
        };

        let mut old = sample_run(0);
        old.id = "old-run".into();
        old.started_at = "2020-01-01T00:00:00Z".into();
        old.finished_at = old.started_at.clone();
        write_run(dir.path(), &old, &settings).unwrap();

        let recent = sample_run(1);
        write_run(dir.path(), &recent, &settings).unwrap();

        let idx = list_run_index(dir.path(), "d1", "w1", 100).unwrap();
        assert_eq!(idx.len(), 1);
        assert_eq!(idx[0].id, "run-1");
        assert!(get_run(dir.path(), "d1", "w1", "old-run").is_err());
    }
}
