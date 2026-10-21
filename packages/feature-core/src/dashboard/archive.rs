//! Portable dashboard archive format, validation and secret stripping.
use super::types::{AlertChannel, Dashboard};
use crate::error::FeatureError as DashboardExportError;
use crate::workflow::model::WorkflowDefinition;
use serde::{Deserialize, Serialize};
pub const FORMAT: &str = "datazen.dashboard";
pub const VERSION: u32 = 2;
#[derive(Debug, Serialize, Deserialize)]
struct DashboardFile {
    format: String,
    version: u32,
    dashboard: Dashboard,
    #[serde(
        default,
        rename = "embeddedWorkflows",
        skip_serializing_if = "Vec::is_empty"
    )]
    embedded_workflows: Vec<WorkflowDefinition>,
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

pub fn parse_dashboard_file_inner(
    bytes: &[u8],
) -> Result<(Dashboard, Vec<WorkflowDefinition>), DashboardExportError> {
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
    if version != 1 && version != 2 {
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

    let embedded = if version >= 2 {
        let value = root
            .get("embeddedWorkflows")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        serde_json::from_value(value)
            .map_err(|e| DashboardExportError::Validation(e.to_string()))?
    } else {
        Vec::new()
    };

    Ok((dashboard, embedded))
}

pub fn serialize_dashboard_file(
    dashboard: &Dashboard,
    embedded_workflows: Vec<WorkflowDefinition>,
) -> Result<String, DashboardExportError> {
    let mut dashboard = dashboard.clone();
    sanitize_dashboard(&mut dashboard);
    let file = DashboardFile {
        format: FORMAT.into(),
        version: VERSION,
        dashboard,
        embedded_workflows,
    };
    serde_json::to_string_pretty(&file)
        .map_err(|error| DashboardExportError::Validation(error.to_string()))
}
