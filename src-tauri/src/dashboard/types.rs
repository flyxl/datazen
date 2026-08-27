use serde::{Deserialize, Serialize};

pub const MIN_REFRESH_SEC: u32 = 30;
#[allow(dead_code)]
pub const REFRESH_WARN_BELOW_SEC: u32 = 60;

pub fn clamp_refresh_sec(n: u32) -> u32 {
    n.max(MIN_REFRESH_SEC)
}

/// Non-blocking UI warning when interval refresh is denser than this threshold.
#[allow(dead_code)]
pub fn should_warn_refresh_sec(refresh_sec: u32) -> bool {
    refresh_sec < REFRESH_WARN_BELOW_SEC
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

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
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
        }
    }
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
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    Chart,
    Table,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RefreshMode {
    Manual,
    OnOpen,
    Interval,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RefreshPolicy {
    pub mode: RefreshMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_sec: Option<u32>,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            mode: RefreshMode::Manual,
            refresh_sec: None,
        }
    }
}

impl RefreshPolicy {
    pub fn normalize(&mut self) {
        if self.mode == RefreshMode::Interval {
            self.refresh_sec = Some(clamp_refresh_sec(
                self.refresh_sec.unwrap_or(MIN_REFRESH_SEC),
            ));
        } else {
            self.refresh_sec = None;
        }
    }

    pub fn interval_secs(&self) -> Option<u32> {
        match self.mode {
            RefreshMode::Interval => Some(clamp_refresh_sec(
                self.refresh_sec.unwrap_or(MIN_REFRESH_SEC),
            )),
            _ => None,
        }
    }
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
    pub workflow_id: String,
    pub view_mode: ViewMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart_config: Option<ChartConfig>,
    pub layout: WidgetLayout,
    pub refresh: RefreshPolicy,
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
    #[serde(default)]
    pub refresh_paused: bool,
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
    pub workflow_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWorkflowRef {
    pub workflow_id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub dashboard_name: String,
    pub widget_title: String,
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
    fn should_warn_refresh_sec_below_threshold() {
        assert!(should_warn_refresh_sec(30));
        assert!(should_warn_refresh_sec(59));
        assert!(!should_warn_refresh_sec(60));
        assert!(!should_warn_refresh_sec(120));
    }

    #[test]
    fn refresh_policy_normalizes_interval() {
        let mut policy = RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(5),
        };
        policy.normalize();
        assert_eq!(policy.refresh_sec, Some(30));
    }

    #[test]
    fn refresh_policy_clears_sec_for_manual() {
        let mut policy = RefreshPolicy {
            mode: RefreshMode::Manual,
            refresh_sec: Some(60),
        };
        policy.normalize();
        assert_eq!(policy.refresh_sec, None);
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
