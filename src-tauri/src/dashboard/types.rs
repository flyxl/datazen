use serde::{Deserialize, Serialize};

pub const MIN_REFRESH_SEC: u32 = 30;

pub fn clamp_refresh_sec(n: u32) -> u32 {
    n.max(MIN_REFRESH_SEC)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AggregationType {
    None,
    Sum,
    Avg,
    Count,
    Min,
    Max,
    DistinctCount,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChartSortBy {
    XAsc,
    XDesc,
    YAsc,
    YDesc,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChartConfig {
    pub chart_type: ChartType,
    pub x_axis: Option<String>,
    pub y_axes: Vec<String>,
    pub group_by: Option<String>,
    pub aggregation: AggregationType,
    pub sort_by: ChartSortBy,
    pub show_legend: bool,
    pub show_grid: bool,
    pub show_values: bool,
    pub color_scheme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLayout {
    pub cols: u32,
    pub row_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetLayout {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertMetricKind {
    Column,
    Aggregation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertMetricAgg {
    Last,
    Max,
    Min,
    Avg,
    Sum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertMetric {
    pub kind: AlertMetricKind,
    pub column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agg: Option<AlertMetricAgg>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertOperator {
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Gte,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Lte,
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertChannel {
    Desktop,
    Webhook,
    Email,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertRule {
    pub metric: AlertMetric,
    pub op: AlertOperator,
    pub threshold: f64,
    #[serde(default = "default_cooldown_sec")]
    pub cooldown_sec: u32,
    pub channels: Vec<AlertChannel>,
}

fn default_cooldown_sec() -> u32 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidget {
    pub id: String,
    pub title: String,
    pub config_id: String,
    pub sql: String,
    pub chart_config: ChartConfig,
    pub layout: WidgetLayout,
    pub refresh_sec: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<AlertRule>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub layout: DashboardLayout,
    pub widgets: Vec<DashboardWidget>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WidgetRunStatus {
    Ok,
    Error,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetRun {
    pub id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: WidgetRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub row_count: u32,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_fired: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert_value: Option<f64>,
}

/// SMTP settings reserved for phase 2.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorEmailSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSettings {
    pub tray_enabled: bool,
    pub close_to_tray: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_webhook_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<MonitorEmailSettings>,
    pub max_concurrent_queries: u32,
    pub export_include_dashboard_runs: bool,
    pub run_retention_count: u32,
    pub run_retention_days: u32,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            tray_enabled: true,
            close_to_tray: true,
            default_webhook_url: None,
            email: None,
            max_concurrent_queries: 2,
            export_include_dashboard_runs: true,
            run_retention_count: 200,
            run_retention_days: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_refresh_sec_enforces_minimum() {
        assert_eq!(clamp_refresh_sec(5), 30);
        assert_eq!(clamp_refresh_sec(30), 30);
        assert_eq!(clamp_refresh_sec(120), 120);
    }

    #[test]
    fn monitor_settings_default() {
        let settings = MonitorSettings::default();
        assert_eq!(settings.max_concurrent_queries, 2);
        assert_eq!(settings.run_retention_count, 200);
        assert_eq!(settings.run_retention_days, 30);
        assert!(settings.export_include_dashboard_runs);
    }
}
