//! Alert notification channels: desktop, webhook, email stub.

use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::dashboard::types::{AlertChannel, DashboardWidget, MonitorSettings, WidgetRun};

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

pub use datazen_feature_core::dashboard::notifications::{AlertPayload, PendingNotification};
pub struct AlertChannelState {
    core: datazen_feature_core::dashboard::notifications::AlertChannelState,
    http_client: reqwest::Client,
}
impl AlertChannelState {
    pub fn new() -> Self {
        Self {
            core: datazen_feature_core::dashboard::notifications::AlertChannelState::new(),
            http_client: reqwest::Client::builder()
                .timeout(WEBHOOK_TIMEOUT)
                .build()
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "Unable to configure webhook client");
                    reqwest::Client::new()
                }),
        }
    }
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }
    pub fn process_run_state(
        &mut self,
        dashboard_id: &str,
        dashboard_name: &str,
        widget: &DashboardWidget,
        run: &WidgetRun,
    ) -> Vec<PendingNotification> {
        self.core
            .process_run_state(dashboard_id, dashboard_name, widget, run)
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
                        tracing::warn!(
                            error = %crate::log_redact::redact_secrets_for_log(&e.to_string()),
                            url = %crate::log_redact::redact_url_for_log(url),
                            "webhook alert failed"
                        );
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
