//! Alert notification channels: desktop, webhook, email stub.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::dashboard::alert::{alert_op_str, CooldownBook};
use crate::dashboard::types::{
    AlertChannel, AlertRule, DashboardWidget, MonitorSettings, WidgetRun, WidgetRunStatus,
};

const CONSECUTIVE_FAILURE_THRESHOLD: u32 = 3;
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Notification computed under lock; dispatched after the lock is released.
#[derive(Debug, Clone, PartialEq)]
pub enum PendingNotification {
    Alert {
        payload: AlertPayload,
        title: String,
        body: String,
        channels: Vec<AlertChannel>,
    },
    QueryFailureWarning {
        title: String,
        body: String,
    },
}

/// Tracks alert edges, cooldowns, and consecutive query failures for channel dispatch.
pub struct AlertChannelState {
    cooldown: CooldownBook,
    prev_alert_fired: HashMap<(String, String), bool>,
    consecutive_failures: HashMap<(String, String), u32>,
    failure_warning_sent: HashMap<(String, String), bool>,
    http_client: reqwest::Client,
}

impl AlertChannelState {
    pub fn new() -> Self {
        Self {
            cooldown: CooldownBook::new(),
            prev_alert_fired: HashMap::new(),
            consecutive_failures: HashMap::new(),
            failure_warning_sent: HashMap::new(),
            http_client: reqwest::Client::builder()
                .timeout(WEBHOOK_TIMEOUT)
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Updates alert/failure state under lock. Returns notifications to send afterward.
    pub fn process_run_state(
        &mut self,
        dashboard_id: &str,
        dashboard_name: &str,
        widget: &DashboardWidget,
        run: &WidgetRun,
    ) -> Vec<PendingNotification> {
        let key = (dashboard_id.to_string(), widget.id.clone());
        let now = Instant::now();
        let mut out = Vec::new();

        if let Some((title, body)) = self.track_query_failures(dashboard_name, widget, run, &key) {
            out.push(PendingNotification::QueryFailureWarning { title, body });
        }

        let Some(rule) = widget.alert.as_ref() else {
            return out;
        };

        if run.status != WidgetRunStatus::Ok {
            return out;
        }

        let Some(current_fired) = run.alert_fired else {
            return out;
        };

        let prev_fired = self.prev_alert_fired.get(&key).copied().unwrap_or(false);
        self.prev_alert_fired.insert(key, current_fired);

        let edge = if !prev_fired && current_fired {
            Some(AlertEdge::Fired)
        } else if prev_fired && !current_fired {
            Some(AlertEdge::Recovered)
        } else {
            None
        };

        let Some(edge) = edge else {
            return out;
        };

        if edge == AlertEdge::Fired {
            let cooldown = rule.cooldown_sec as u64;
            if !self.cooldown.should_notify(&widget.id, cooldown, now) {
                return out;
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

        out.push(PendingNotification::Alert {
            payload,
            title,
            body,
            channels: rule.channels.clone(),
        });
        out
    }

    fn track_query_failures(
        &mut self,
        dashboard_name: &str,
        widget: &DashboardWidget,
        run: &WidgetRun,
        key: &(String, String),
    ) -> Option<(String, String)> {
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
                self.failure_warning_sent.insert(key.clone(), true);
                tracing::warn!(
                    dashboard_id = %key.0,
                    widget_id = %key.1,
                    failures = *count,
                    "monitor widget consecutive query failures"
                );
                return Some((
                    format!("Query failed: {}", widget.title),
                    format!("{}: {} consecutive query failures", dashboard_name, count),
                ));
            }
        } else {
            self.consecutive_failures.remove(key);
            self.failure_warning_sent.remove(key);
        }
        None
    }
}

impl Default for AlertChannelState {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn dispatch_notifications(
    app: Option<&AppHandle>,
    settings: &MonitorSettings,
    client: &reqwest::Client,
    notifications: &[PendingNotification],
) {
    for notification in notifications {
        match notification {
            PendingNotification::QueryFailureWarning { title, body } => {
                if let Some(app) = app {
                    notify_desktop(app, title, body).await;
                }
            }
            PendingNotification::Alert {
                payload,
                title,
                body,
                channels,
            } => {
                dispatch_channels(app, settings, client, channels, payload, title, body).await;
            }
        }
    }
}

async fn dispatch_channels(
    app: Option<&AppHandle>,
    settings: &MonitorSettings,
    client: &reqwest::Client,
    channels: &[AlertChannel],
    payload: &AlertPayload,
    title: &str,
    body: &str,
) {
    for channel in channels {
        match channel {
            AlertChannel::Desktop => {
                if let Some(app) = app {
                    notify_desktop(app, title, body).await;
                }
            }
            AlertChannel::Webhook => {
                if let Some(url) = settings.default_webhook_url.as_deref() {
                    if let Err(e) = notify_webhook(client, url, payload).await {
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
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!(error = %e, "desktop notification failed");
    }
}

pub async fn notify_webhook(
    client: &reqwest::Client,
    url: &str,
    payload: &AlertPayload,
) -> Result<(), reqwest::Error> {
    client
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

    fn widget_with_alert(cooldown_sec: u32) -> DashboardWidget {
        let mut widget = sample_widget();
        let mut rule = sample_rule();
        rule.cooldown_sec = cooldown_sec;
        widget.alert = Some(rule);
        widget
    }

    fn make_run(status: WidgetRunStatus, alert_fired: Option<bool>) -> WidgetRun {
        WidgetRun {
            id: "run-1".into(),
            dashboard_id: "d1".into(),
            widget_id: "w1".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            finished_at: "2026-01-01T00:01:00Z".into(),
            status,
            error: if status == WidgetRunStatus::Ok {
                None
            } else {
                Some("query failed".into())
            },
            row_count: 1,
            columns: vec!["v".into()],
            rows: vec![vec![serde_json::json!(95.0)]],
            alert_fired,
            alert_value: Some(95.0),
        }
    }

    fn alert_notifications(notifications: &[PendingNotification]) -> Vec<&PendingNotification> {
        notifications
            .iter()
            .filter(|n| matches!(n, PendingNotification::Alert { .. }))
            .collect()
    }

    #[test]
    fn alert_payload_serializes_expected_camel_case_fields() {
        let widget = sample_widget();
        let rule = sample_rule();
        let run = make_run(WidgetRunStatus::Ok, Some(true));

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

    #[test]
    fn fire_edge_emits_alert_notification() {
        let mut state = AlertChannelState::new();
        let widget = widget_with_alert(300);

        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(false)),
        );
        assert!(alert_notifications(&notes).is_empty());

        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(true)),
        );
        let alerts = alert_notifications(&notes);
        assert_eq!(alerts.len(), 1);
        assert!(
            matches!(alerts[0], PendingNotification::Alert { title, .. } if title.starts_with("Alert:"))
        );
    }

    #[test]
    fn query_error_does_not_recover_while_alerting() {
        let mut state = AlertChannelState::new();
        let widget = widget_with_alert(300);

        state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(true)),
        );

        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Error, None),
        );
        assert!(alert_notifications(&notes).is_empty());

        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(false)),
        );
        let alerts = alert_notifications(&notes);
        assert_eq!(alerts.len(), 1);
        assert!(
            matches!(alerts[0], PendingNotification::Alert { title, .. } if title.starts_with("Recovered:"))
        );
    }

    #[test]
    fn ok_with_none_alert_fired_leaves_state_unchanged() {
        let mut state = AlertChannelState::new();
        let widget = widget_with_alert(300);

        state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(true)),
        );

        let notes =
            state.process_run_state("d1", "Board", &widget, &make_run(WidgetRunStatus::Ok, None));
        assert!(alert_notifications(&notes).is_empty());

        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(false)),
        );
        assert_eq!(alert_notifications(&notes).len(), 1);
    }

    #[test]
    fn consecutive_failures_emit_warning_on_third_streak() {
        let mut state = AlertChannelState::new();
        let widget = sample_widget();

        for _ in 0..2 {
            let notes = state.process_run_state(
                "d1",
                "Board",
                &widget,
                &make_run(WidgetRunStatus::Error, None),
            );
            assert!(!notes
                .iter()
                .any(|n| matches!(n, PendingNotification::QueryFailureWarning { .. })));
        }

        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Error, None),
        );
        assert!(notes.iter().any(|n| matches!(
            n,
            PendingNotification::QueryFailureWarning { title, .. } if title.starts_with("Query failed:")
        )));

        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Error, None),
        );
        assert!(!notes
            .iter()
            .any(|n| matches!(n, PendingNotification::QueryFailureWarning { .. })));
    }

    #[test]
    fn cooldown_blocks_second_fire() {
        let mut state = AlertChannelState::new();
        let widget = widget_with_alert(300);

        state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(false)),
        );
        state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(true)),
        );

        state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(false)),
        );
        let notes = state.process_run_state(
            "d1",
            "Board",
            &widget,
            &make_run(WidgetRunStatus::Ok, Some(true)),
        );
        assert!(alert_notifications(&notes).is_empty());
    }
}
