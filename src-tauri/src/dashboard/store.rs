use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::types::{clamp_refresh_sec, Dashboard, MonitorSettings};
use crate::store::AppSettings;

pub const DASHBOARDS_FILE: &str = "dashboards.json";

#[derive(Debug, Error)]
pub enum DashboardStoreError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

fn dashboards_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DASHBOARDS_FILE)
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<(), DashboardStoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| DashboardStoreError::Io(io::Error::new(io::ErrorKind::NotFound, "no parent")))?;
    std::fs::create_dir_all(parent)?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DashboardStoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, "bad name")))?;
    let tmp_path = parent.join(format!(".{file_name}.tmp"));

    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

fn load_all(data_dir: &Path) -> Result<Vec<Dashboard>, DashboardStoreError> {
    let path = dashboards_path(data_dir);
    if !path.exists() {
        write_atomic(&path, b"[]")?;
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)?;
    serde_json::from_str(&content).map_err(|e| DashboardStoreError::Parse(e.to_string()))
}

fn save_all(data_dir: &Path, dashboards: &[Dashboard]) -> Result<(), DashboardStoreError> {
    let content = serde_json::to_string_pretty(dashboards)
        .map_err(|e| DashboardStoreError::Parse(e.to_string()))?;
    write_atomic(&dashboards_path(data_dir), content.as_bytes())
}

fn normalize_dashboard(dashboard: &mut Dashboard) {
    for widget in &mut dashboard.widgets {
        widget.refresh_sec = clamp_refresh_sec(widget.refresh_sec);
    }
}

pub fn list_dashboards(data_dir: &Path) -> Result<Vec<Dashboard>, DashboardStoreError> {
    load_all(data_dir)
}

pub fn get_dashboard(data_dir: &Path, id: &str) -> Result<Dashboard, DashboardStoreError> {
    load_all(data_dir)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| DashboardStoreError::NotFound(id.to_string()))
}

pub fn save_dashboard(data_dir: &Path, mut dashboard: Dashboard) -> Result<(), DashboardStoreError> {
    normalize_dashboard(&mut dashboard);
    let mut all = load_all(data_dir)?;
    if let Some(pos) = all.iter().position(|d| d.id == dashboard.id) {
        all[pos] = dashboard;
    } else {
        all.push(dashboard);
    }
    save_all(data_dir, &all)
}

pub fn delete_dashboard(data_dir: &Path, id: &str) -> Result<(), DashboardStoreError> {
    let mut all = load_all(data_dir)?;
    let len_before = all.len();
    all.retain(|d| d.id != id);
    if all.len() == len_before {
        return Err(DashboardStoreError::NotFound(id.to_string()));
    }
    save_all(data_dir, &all)
}

pub fn load_monitor_settings(settings: &AppSettings) -> MonitorSettings {
    settings.monitor.clone()
}
