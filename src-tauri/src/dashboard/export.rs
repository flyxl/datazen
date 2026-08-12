//! Single-file dashboard import/export (`.datazen-dashboard.json`).

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::store::{list_dashboards, save_dashboard, DashboardStoreError};
use super::types::{AlertChannel, Dashboard};

pub const FORMAT: &str = "datazen.dashboard";
pub const VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum DashboardExportError {
    #[error("validation: {0}")]
    Validation(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("store error: {0}")]
    Store(#[from] DashboardStoreError),
}

#[derive(Debug, Serialize, Deserialize)]
struct DashboardFile {
    format: String,
    version: u32,
    dashboard: Dashboard,
}

fn strip_webhook_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("defaultWebhookUrl");
            map.remove("default_webhook_url");
            map.remove("webhookUrl");
            map.remove("webhook_url");
            for v in map.values_mut() {
                strip_webhook_fields(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                strip_webhook_fields(v);
            }
        }
        _ => {}
    }
}

fn sanitize_dashboard(dashboard: &mut Dashboard) {
    for widget in &mut dashboard.widgets {
        if let Some(alert) = &mut widget.alert {
            alert.channels.retain(|c| *c != AlertChannel::Webhook);
        }
    }
}

/// Parse and validate a single-file dashboard export. Strips accidental webhook fields.
pub fn parse_dashboard_file(bytes: &[u8]) -> Result<Dashboard, DashboardExportError> {
    let mut root: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| DashboardExportError::Validation(e.to_string()))?;

    strip_webhook_fields(&mut root);

    let format = root
        .get("format")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DashboardExportError::Validation("missing format".into()))?;
    if format != FORMAT {
        return Err(DashboardExportError::Validation(format!(
            "unsupported format: {format}"
        )));
    }

    let version = root
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| DashboardExportError::Validation("missing version".into()))?;
    if version != u64::from(VERSION) {
        return Err(DashboardExportError::Validation(format!(
            "unsupported version: {version}"
        )));
    }

    let dashboard_value = root
        .get("dashboard")
        .ok_or_else(|| DashboardExportError::Validation("missing dashboard".into()))?
        .clone();

    let mut dashboard: Dashboard = serde_json::from_value(dashboard_value)
        .map_err(|e| DashboardExportError::Validation(e.to_string()))?;

    sanitize_dashboard(&mut dashboard);
    Ok(dashboard)
}

/// Serialize a dashboard to the single-file export format (no secrets / webhook URLs).
pub fn export_dashboard_json(dashboard: &Dashboard) -> Result<String, DashboardExportError> {
    let mut sanitized = dashboard.clone();
    sanitize_dashboard(&mut sanitized);
    let file = DashboardFile {
        format: FORMAT.into(),
        version: VERSION,
        dashboard: sanitized,
    };
    serde_json::to_string_pretty(&file).map_err(|e| DashboardExportError::Validation(e.to_string()))
}

/// Import a dashboard file; assigns a new id when the id already exists locally.
pub fn import_dashboard(data_dir: &Path, bytes: &[u8]) -> Result<Dashboard, DashboardExportError> {
    let mut dashboard = parse_dashboard_file(bytes)?;

    let existing = list_dashboards(data_dir)?;
    if existing.iter().any(|d| d.id == dashboard.id) {
        dashboard.id = uuid::Uuid::new_v4().to_string();
    }

    save_dashboard(data_dir, dashboard.clone())?;
    Ok(dashboard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::store::{get_dashboard, save_dashboard};
    use crate::dashboard::types::{
        AggregationType, AlertChannel, AlertMetric, AlertMetricKind, AlertOperator, AlertRule,
        ChartConfig, ChartSortBy, ChartType, DashboardLayout, DashboardWidget, WidgetLayout,
    };

    fn sample_dashboard() -> Dashboard {
        Dashboard {
            id: "dash-export-1".into(),
            name: "Export Test".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets: vec![DashboardWidget {
                id: "w1".into(),
                title: "Widget".into(),
                config_id: "cfg-keep".into(),
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
                alert: Some(AlertRule {
                    metric: AlertMetric {
                        kind: AlertMetricKind::Column,
                        column: "v".into(),
                        agg: None,
                    },
                    op: AlertOperator::Gt,
                    threshold: 1.0,
                    cooldown_sec: 300,
                    channels: vec![AlertChannel::Desktop, AlertChannel::Webhook],
                }),
                enabled: true,
            }],
            enabled: true,
        }
    }

    #[test]
    fn import_dashboard_json_rejects_bad_format() {
        let err = parse_dashboard_file(br#"{"format":"nope"}"#).unwrap_err();
        assert!(err.to_string().contains("format"));
    }

    #[test]
    fn export_strips_webhook_channel_and_keeps_config_id() {
        let json = export_dashboard_json(&sample_dashboard()).unwrap();
        assert!(!json.contains("webhook"));
        assert!(json.contains("cfg-keep"));
        let parsed = parse_dashboard_file(json.as_bytes()).unwrap();
        assert_eq!(parsed.widgets[0].config_id, "cfg-keep");
        assert!(parsed.widgets[0]
            .alert
            .as_ref()
            .unwrap()
            .channels
            .iter()
            .all(|c| *c != AlertChannel::Webhook));
    }

    #[test]
    fn import_assigns_new_id_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let dash = sample_dashboard();
        save_dashboard(dir.path(), dash.clone()).unwrap();

        let json = export_dashboard_json(&dash).unwrap();
        let imported = import_dashboard(dir.path(), json.as_bytes()).unwrap();
        assert_ne!(imported.id, dash.id);
        assert_eq!(list_dashboards(dir.path()).unwrap().len(), 2);
    }

    #[test]
    fn import_roundtrip_persists_dashboard() {
        let dir = tempfile::tempdir().unwrap();
        let dash = sample_dashboard();
        let json = export_dashboard_json(&dash).unwrap();
        let imported = import_dashboard(dir.path(), json.as_bytes()).unwrap();
        let loaded = get_dashboard(dir.path(), &imported.id).unwrap();
        assert_eq!(loaded.name, dash.name);
        assert_eq!(loaded.widgets[0].config_id, "cfg-keep");
    }
}
