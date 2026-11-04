//! The rows a host stores dashboards, widgets and widget runs as.
//!
//! These are the storage vocabulary of the dashboard feature: the desktop
//! host persists them in SQLite, another host may persist the very same rows
//! in its own store. The rules that turn a row into a dashboard — and a
//! dashboard back into rows — live in [`super::repository`] and [`super::runs`].

use serde::{Deserialize, Serialize};

/// A persisted dashboard row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardRecord {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub layout_cols: u32,
    pub layout_row_height: u32,
    pub enabled: bool,
    pub refresh_paused: bool,
}

/// A persisted widget row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRecord {
    pub id: String,
    pub dashboard_id: String,
    pub title: String,
    pub workflow_id: String,
    pub view_mode: String,
    pub chart_config_json: Option<String>,
    pub layout_x: u32,
    pub layout_y: u32,
    pub layout_w: u32,
    pub layout_h: u32,
    pub refresh_mode: String,
    pub refresh_sec: Option<u32>,
    pub alert_json: Option<String>,
    pub enabled: bool,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// A persisted widget run row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRunRecord {
    pub id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub workflow_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub error: Option<String>,
    pub row_count: u32,
    pub columns_json: String,
    pub rows_json: String,
    pub variables_json: Option<String>,
    pub alert_fired: Option<bool>,
    pub alert_value: Option<f64>,
}
