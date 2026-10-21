//! Portable dashboard archive format, validation and secret stripping.
use super::types::{AlertChannel, Dashboard};
use crate::error::FeatureError as DashboardExportError;
use crate::workflow::model::{WorkflowDefinition, WorkflowVisibility};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

/// The workflows a dashboard references, embedded so the dashboard travels.
///
/// Referenced workflows are always embedded: the hidden workflows a SQL
/// widget owns are required for the dashboard to work somewhere else. The
/// lookup is the host's — it reads the stored definition of a workflow.
pub fn select_embedded_workflows<F, E>(
    dashboard: &Dashboard,
    mut workflow_of: F,
) -> Result<Vec<WorkflowDefinition>, E>
where
    F: FnMut(&str) -> Result<WorkflowDefinition, E>,
{
    let mut seen = HashSet::new();
    let mut workflows = Vec::new();

    for widget in &dashboard.widgets {
        if !seen.insert(widget.workflow_id.clone()) {
            continue;
        }
        let definition = workflow_of(&widget.workflow_id)?;
        // Always embed referenced workflows; hidden ones are required for portability.
        if definition.visibility == WorkflowVisibility::DashboardHidden
            || definition.visibility == WorkflowVisibility::User
        {
            workflows.push(definition);
        }
    }

    Ok(workflows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::{
        DashboardLayout, DashboardWidget, RefreshMode, RefreshPolicy, ViewMode, WidgetLayout,
    };
    use crate::workflow::model::{WorkflowStep, WorkflowVisibility as WfVisibility};

    fn widget(id: &str, workflow_id: &str) -> DashboardWidget {
        DashboardWidget {
            id: id.into(),
            title: "Widget".into(),
            workflow_id: workflow_id.into(),
            view_mode: ViewMode::Table,
            chart_config: None,
            layout: WidgetLayout {
                x: 0,
                y: 0,
                w: 4,
                h: 3,
            },
            refresh: RefreshPolicy {
                mode: RefreshMode::Manual,
                refresh_sec: None,
            },
            alert: None,
            enabled: true,
        }
    }

    fn definition(id: &str, visibility: WfVisibility) -> WorkflowDefinition {
        WorkflowDefinition {
            database: None,
            id: id.into(),
            name: id.into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: None,
            steps: vec![WorkflowStep::Query {
                id: "q1".into(),
                sql: "SELECT 1".into(),
                connection: None,
                database: None,
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility,
        }
    }

    fn dashboard(widgets: Vec<DashboardWidget>) -> Dashboard {
        Dashboard {
            id: "d1".into(),
            name: "Board".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets,
            enabled: true,
            refresh_paused: false,
        }
    }

    #[test]
    fn a_referenced_workflow_is_embedded_once() {
        let board = dashboard(vec![widget("w1", "wf-1"), widget("w2", "wf-1")]);
        let embedded = select_embedded_workflows(&board, |id| {
            Ok::<_, String>(definition(id, WfVisibility::User))
        })
        .unwrap();
        assert_eq!(embedded.len(), 1);
        assert_eq!(embedded[0].id, "wf-1");
    }

    #[test]
    fn a_hidden_workflow_travels_with_the_dashboard() {
        let board = dashboard(vec![widget("w1", "wf-hidden")]);
        let embedded = select_embedded_workflows(&board, |id| {
            Ok::<_, String>(definition(id, WfVisibility::DashboardHidden))
        })
        .unwrap();
        assert_eq!(embedded.len(), 1);
    }

    #[test]
    fn a_missing_workflow_fails_the_export() {
        let board = dashboard(vec![widget("w1", "wf-gone")]);
        let error = select_embedded_workflows(&board, |id| {
            Err::<WorkflowDefinition, String>(format!("workflow {id} is missing"))
        })
        .unwrap_err();
        assert!(error.contains("wf-gone"));
    }
}
