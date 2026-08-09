//! Alert notification channels: desktop, webhook, email stub.

use std::collections::HashMap;
use std::time::Instant;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::dashboard::alert::{alert_op_str, CooldownBook};
use crate::dashboard::types::{
    AlertChannel, AlertRule, DashboardWidget, MonitorSettings, WidgetRun, WidgetRunStatus,
};

const CONSECUTIVE_FAILURE_THRESHOLD: u32 = 3;

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertPayload {
    pub dashboard_id: String,
    pub dashboard_name: String,
    pub widget_id: String,
    pub widget_title: String,
    pub value: f64,
    pub threshold: f64,
    pub op: String,
    pub at: String,
}

impl AlertPayload {
    pub fn from_run(
        dashboard_id: &str,
        dashboard_name: &str,
        widget: &DashboardWidget,
        rule: &AlertRule,
        run: &WidgetRun,
    ) -> Self {
        Self {
            dashboard_id: dashboard_id.to_string(),
            dashboard_name: dashboard_name.to_string(),
            widget_id: widget.id.clone(),
            widget_title: widget.title.clone(),
            value: run.alert_value.unwrap_or(0.0),
            threshold: rule.threshold,
            op: alert_op_str(rule.op).to_string(),
            at: run.finished_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlertEdge {
    Fired,
    Recovered,
}

/// Tracks alert edges, cooldowns, and consecutive query failures for channel dispatch.
pub struct AlertChannelState {
    cooldown: CooldownBook,
    prev_alert_fired: HashMap<(String, String), bool>,
    consecutive_failures: HashMap<(String, String), u32>,
    failure_warning_sent: HashMap<(String, String), bool>,
}

impl AlertChannelState {
    pub fn new() -> Self {
        Self {
            cooldown: CooldownBook::new(),
            prev_alert_fired: HashMap::new(),
            consecutive_failures: HashMap::new(),
            failure_warning_sent: HashMap::new(),
        }
    }

    pub async fn process_run(
        &mut self,
        app: Option<&AppHandle>,
        settings: &MonitorSettings,
        dashboard_id: &str,
        dashboard_name: &str,
        widget: &DashboardWidget,
        run: &WidgetRun,
    ) {
        let key = (dashboard_id.to_string(), widget.id.clone());
        let now = Instant::now();

        self.track_query_failures(app, dashboard_name, widget, run, &key)
            .await;

        let Some(rule) = widget.alert.as_ref() else {
            return;
        };

        let current_fired = run.status == WidgetRunStatus::Ok && run.alert_fired == Some(true);
        let prev_fired = self.prev_alert_fired.get(&key).copied().unwrap_or(false);
        self.prev_alert_fired.insert(key.clone(), current_fired);

        let edge = if !prev_fired && current_fired {
            Some(AlertEdge::Fired)
        } else if prev_fired && !current_fired {
            Some(AlertEdge::Recovered)
        } else {
            None
        };

        let Some(edge) = edge else {
            return;
        };

        if edge == AlertEdge::Fired {
            let cooldown = rule.cooldown_sec as u64;
            if !self.cooldown.should_notify(&widget.id, cooldown, now) {
                return;
            }
            self.cooldown.mark_sent(&widget.id, now);
        }

        let payload = AlertPayload::from_run(dashboard_id, dashboard_name, widget, rule, run);
        let (title, body) = match edge {
            AlertEdge::Fired => (
                format!("Alert: {}", widget.title),
                format!(
                    "{}: {} {} {}",
                    dashboard_name, payload.value, payload.op, payload.threshold
                ),
            ),
            AlertEdge::Recovered => (
                format!("Recovered: {}", widget.title),
                format!(
                    "{}: {} is back within threshold ({})",
                    dashboard_name, payload.value, payload.threshold
                ),
            ),
        };

        dispatch_channels(app, settings, rule, &payload, &title, &body).await;
    }

    async fn track_query_failures(
        &mut self,
        app: Option<&AppHandle>,
        dashboard_name: &str,
        widget: &DashboardWidget,
        run: &WidgetRun,
        key: &(String, String),
    ) {
        let failed = run.status != WidgetRunStatus::Ok;
        if failed {
            let count = self
                .consecutive_failures
                .entry(key.clone())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            if *count >= CONSECUTIVE_FAILURE_THRESHOLD
                && !self.failure_warning_sent.get(key).copied().unwrap_or(false)
            {
                let title = format!("Query failed: {}", widget.title);
                let body = format!(
                    "{}: {} consecutive query failures",
                    dashboard_name, count
                );
                if let Some(app) = app {
                    notify_desktop(app, &title, &body).await;
                }
                self.failure_warning_sent.insert(key.clone(), true);
                tracing::warn!(
                    dashboard_id = %key.0,
                    widget_id = %key.1,
                    failures = *count,
                    "monitor widget consecutive query failures"
                );
            }
        } else {
            self.consecutive_failures.remove(key);
            self.failure_warning_sent.remove(key);
        }
    }
}

impl Default for AlertChannelState {
    fn default() -> Self {
        Self::new()
    }
}

async fn dispatch_channels(
    app: Option<&AppHandle>,
    settings: &MonitorSettings,
    rule: &AlertRule,
    payload: &AlertPayload,
    title: &str,
    body: &str,
) {
    for channel in &rule.channels {
        match channel {
            AlertChannel::Desktop => {
                if let Some(app) = app {
                    notify_desktop(app, title, body).await;
                }
            }
            AlertChannel::Webhook => {
                if let Some(url) = settings.default_webhook_url.as_deref() {
                    if let Err(e) = notify_webhook(url, payload).await {
                        tracing::warn!(error = %e, url = %url, "webhook alert failed");
                    }
                } else {
                    tracing::debug!("webhook channel configured but default_webhook_url is unset");
                }
            }
            AlertChannel::Email => {
                notify_email_stub(payload);
            }
        }
    }
}

pub async fn notify_desktop(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show()
    {
        tracing::warn!(error = %e, "desktop notification failed");
    }
}

pub async fn notify_webhook(url: &str, payload: &AlertPayload) -> Result<(), reqwest::Error> {
    reqwest::Client::new()
        .post(url)
        .json(payload)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub fn notify_email_stub(payload: &AlertPayload) {
    tracing::info!(
        channel = "email",
        dashboard_id = %payload.dashboard_id,
        widget_id = %payload.widget_id,
        "stub; not sent"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::{
        AlertMetric, AlertMetricAgg, AlertMetricKind, AlertOperator, ChartConfig, ChartType,
        WidgetLayout,
    };

    fn sample_widget() -> DashboardWidget {
        DashboardWidget {
            id: "w1".into(),
            title: "CPU".into(),
            config_id: "cfg-1".into(),
            sql: "SELECT 1".into(),
            chart_config: ChartConfig {
                chart_type: ChartType::Line,
                x_axis: None,
                y_axes: vec![],
                group_by: None,
                aggregation: crate::dashboard::types::AggregationType::None,
                sort_by: crate::dashboard::types::ChartSortBy::None,
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
            alert: None,
            enabled: true,
        }
    }

    fn sample_rule() -> AlertRule {
        AlertRule {
            metric: AlertMetric {
                kind: AlertMetricKind::Column,
                column: "v".into(),
                agg: Some(AlertMetricAgg::Last),
            },
            op: AlertOperator::Gt,
            threshold: 90.0,
            cooldown_sec: 300,
            channels: vec![AlertChannel::Webhook],
        }
    }

    #[test]
    fn alert_payload_serializes_expected_camel_case_fields() {
        let widget = sample_widget();
        let rule = sample_rule();
        let run = WidgetRun {
            id: "run-1".into(),
            dashboard_id: "d1".into(),
            widget_id: "w1".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: "2026-01-01T00:01:00Z".into(),
            status: WidgetRunStatus::Ok,
            error: None,
            row_count: 1,
            columns: vec!["v".into()],
            rows: vec![vec![serde_json::json!(95.0)]],
            alert_fired: Some(true),
            alert_value: Some(95.0),
        };

        let payload = AlertPayload::from_run("d1", "Ops Board", &widget, &rule, &run);
        let json = serde_json::to_value(&payload).unwrap();
        let obj = json.as_object().unwrap();

        assert_eq!(obj.get("dashboardId").and_then(|v| v.as_str()), Some("d1"));
        assert_eq!(
            obj.get("dashboardName").and_then(|v| v.as_str()),
            Some("Ops Board")
        );
        assert_eq!(obj.get("widgetId").and_then(|v| v.as_str()), Some("w1"));
        assert_eq!(obj.get("widgetTitle").and_then(|v| v.as_str()), Some("CPU"));
        assert_eq!(obj.get("value").and_then(|v| v.as_f64()), Some(95.0));
        assert_eq!(obj.get("threshold").and_then(|v| v.as_f64()), Some(90.0));
        assert_eq!(obj.get("op").and_then(|v| v.as_str()), Some(">"));
        assert_eq!(
            obj.get("at").and_then(|v| v.as_str()),
            Some("2026-01-01T00:01:00Z")
        );
    }

    #[test]
    fn alert_payload_json_keys_match_webhook_contract() {
        let payload = AlertPayload {
            dashboard_id: "d1".into(),
            dashboard_name: "Ops".into(),
            widget_id: "w1".into(),
            widget_title: "CPU".into(),
            value: 1.0,
            threshold: 2.0,
            op: ">=".into(),
            at: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"dashboardId\""));
        assert!(json.contains("\"dashboardName\""));
        assert!(json.contains("\"widgetId\""));
        assert!(json.contains("\"widgetTitle\""));
        assert!(json.contains("\"threshold\""));
    }
}
