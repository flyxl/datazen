use std::collections::HashSet;

use chrono::Utc;
use thiserror::Error;

use super::types::{
    AlertRule, ChartConfig, Dashboard, DashboardWidget, MonitorSettings, RefreshMode,
    RefreshPolicy, ViewMode,
};
use crate::store::AppSettings;
use crate::store::{AppDb, AppDbError, DashboardRecord, WidgetRecord};

#[derive(Debug, Error)]
pub enum DashboardStoreError {
    #[error("AppDb error: {0}")]
    Db(#[from] AppDbError),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

fn refresh_mode_str(mode: RefreshMode) -> &'static str {
    match mode {
        RefreshMode::Manual => "manual",
        RefreshMode::OnOpen => "onOpen",
        RefreshMode::Interval => "interval",
    }
}

fn parse_refresh_mode(s: &str) -> Result<RefreshMode, DashboardStoreError> {
    match s {
        "manual" => Ok(RefreshMode::Manual),
        "onOpen" => Ok(RefreshMode::OnOpen),
        "interval" => Ok(RefreshMode::Interval),
        other => Err(DashboardStoreError::Parse(format!(
            "invalid refresh_mode: {other}"
        ))),
    }
}

fn view_mode_str(mode: ViewMode) -> &'static str {
    match mode {
        ViewMode::Chart => "chart",
        ViewMode::Table => "table",
    }
}

fn parse_view_mode(s: &str) -> Result<ViewMode, DashboardStoreError> {
    match s {
        "chart" => Ok(ViewMode::Chart),
        "table" => Ok(ViewMode::Table),
        other => Err(DashboardStoreError::Parse(format!(
            "invalid view_mode: {other}"
        ))),
    }
}

fn widget_to_record(widget: &DashboardWidget, dashboard_id: &str, sort_order: i32) -> WidgetRecord {
    let mut refresh = widget.refresh.clone();
    refresh.normalize();
    WidgetRecord {
        id: widget.id.clone(),
        dashboard_id: dashboard_id.to_string(),
        title: widget.title.clone(),
        workflow_id: widget.workflow_id.clone(),
        view_mode: view_mode_str(widget.view_mode).into(),
        chart_config_json: widget
            .chart_config
            .as_ref()
            .and_then(|c| serde_json::to_string(c).ok()),
        layout_x: widget.layout.x,
        layout_y: widget.layout.y,
        layout_w: widget.layout.w,
        layout_h: widget.layout.h,
        refresh_mode: refresh_mode_str(refresh.mode).into(),
        refresh_sec: refresh.refresh_sec,
        alert_json: widget
            .alert
            .as_ref()
            .and_then(|a| serde_json::to_string(a).ok()),
        enabled: widget.enabled,
        sort_order,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    }
}

fn widget_from_record(record: &WidgetRecord) -> Result<DashboardWidget, DashboardStoreError> {
    let chart_config = record
        .chart_config_json
        .as_ref()
        .map(|json| {
            serde_json::from_str::<ChartConfig>(json)
                .map_err(|e| DashboardStoreError::Parse(e.to_string()))
        })
        .transpose()?;
    let alert = record
        .alert_json
        .as_ref()
        .map(|json| {
            serde_json::from_str::<AlertRule>(json)
                .map_err(|e| DashboardStoreError::Parse(e.to_string()))
        })
        .transpose()?;
    Ok(DashboardWidget {
        id: record.id.clone(),
        title: record.title.clone(),
        workflow_id: record.workflow_id.clone(),
        view_mode: parse_view_mode(&record.view_mode)?,
        chart_config,
        layout: super::types::WidgetLayout {
            x: record.layout_x,
            y: record.layout_y,
            w: record.layout_w,
            h: record.layout_h,
        },
        refresh: RefreshPolicy {
            mode: parse_refresh_mode(&record.refresh_mode)?,
            refresh_sec: record.refresh_sec,
        },
        alert,
        enabled: record.enabled,
    })
}

fn dashboard_to_record(dashboard: &Dashboard) -> DashboardRecord {
    DashboardRecord {
        id: dashboard.id.clone(),
        name: dashboard.name.clone(),
        created_at: dashboard.created_at.clone(),
        updated_at: dashboard.updated_at.clone(),
        layout_cols: dashboard.layout.cols,
        layout_row_height: dashboard.layout.row_height,
        enabled: dashboard.enabled,
        refresh_paused: dashboard.refresh_paused,
    }
}

fn dashboard_from_record(record: &DashboardRecord, widgets: Vec<DashboardWidget>) -> Dashboard {
    Dashboard {
        id: record.id.clone(),
        name: record.name.clone(),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        layout: super::types::DashboardLayout {
            cols: record.layout_cols,
            row_height: record.layout_row_height,
        },
        widgets,
        enabled: record.enabled,
        refresh_paused: record.refresh_paused,
    }
}

pub fn list_dashboards(app_db: &AppDb) -> Result<Vec<Dashboard>, DashboardStoreError> {
    let records = app_db.list_dashboards()?;
    let mut dashboards = Vec::with_capacity(records.len());
    for record in records {
        let widgets = app_db
            .list_widgets(&record.id)?
            .iter()
            .map(widget_from_record)
            .collect::<Result<Vec<_>, _>>()?;
        dashboards.push(dashboard_from_record(&record, widgets));
    }
    Ok(dashboards)
}

pub fn get_dashboard(app_db: &AppDb, id: &str) -> Result<Dashboard, DashboardStoreError> {
    let record = app_db.get_dashboard(id).map_err(|e| match e {
        AppDbError::NotFound(id) => DashboardStoreError::NotFound(id),
        other => DashboardStoreError::Db(other),
    })?;
    let widgets = app_db
        .list_widgets(id)?
        .iter()
        .map(widget_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(dashboard_from_record(&record, widgets))
}

pub fn save_dashboard(app_db: &AppDb, mut dashboard: Dashboard) -> Result<(), DashboardStoreError> {
    for widget in &mut dashboard.widgets {
        widget.refresh.normalize();
    }
    dashboard.updated_at = Utc::now().to_rfc3339();

    let existing = app_db.list_widgets(&dashboard.id).unwrap_or_default();
    let existing_ids: HashSet<_> = existing.iter().map(|w| w.id.as_str()).collect();
    let new_ids: HashSet<_> = dashboard.widgets.iter().map(|w| w.id.as_str()).collect();

    app_db.upsert_dashboard(&dashboard_to_record(&dashboard))?;

    for (idx, widget) in dashboard.widgets.iter().enumerate() {
        let mut record = widget_to_record(widget, &dashboard.id, idx as i32);
        if let Ok(prev) = app_db.get_widget(&widget.id) {
            record.created_at = prev.created_at;
        }
        record.updated_at = dashboard.updated_at.clone();
        app_db.upsert_widget(&record)?;
    }

    for id in existing_ids.difference(&new_ids) {
        app_db.delete_widget(id)?;
    }

    Ok(())
}

pub fn delete_dashboard(app_db: &AppDb, id: &str) -> Result<(), DashboardStoreError> {
    app_db.delete_dashboard(id).map_err(Into::into)
}

pub fn set_dashboard_refresh_paused(
    app_db: &AppDb,
    id: &str,
    paused: bool,
) -> Result<(), DashboardStoreError> {
    app_db
        .set_dashboard_refresh_paused(id, paused)
        .map_err(Into::into)
}

pub fn find_workflow_refs(
    app_db: &AppDb,
    workflow_id: &str,
) -> Result<Vec<super::types::DashboardWorkflowRef>, DashboardStoreError> {
    let refs = app_db.find_workflow_refs(workflow_id)?;
    Ok(refs
        .into_iter()
        .map(|r| super::types::DashboardWorkflowRef {
            workflow_id: r.workflow_id,
            dashboard_id: r.dashboard_id,
            widget_id: r.widget_id,
            dashboard_name: r.dashboard_name,
            widget_title: r.widget_title,
        })
        .collect())
}

pub fn load_monitor_settings(settings: &AppSettings) -> MonitorSettings {
    settings.monitor.clone()
}
