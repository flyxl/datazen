//! Pub/Sub subscribe loop, publish helper, and subscription registry.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use redis::aio::ConnectionLike;
use redis::AsyncCommands;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::connect::{open_pubsub_connection, ConnectionPlan, RedisLiveConn};
use crate::redis_driver::RedisDriver;
use tauri::Emitter;

const EVENT_NAME: &str = "redis-pubsub-message";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisPubSubMessageEvent {
    pub connection_id: String,
    pub subscription_id: String,
    pub channel: String,
    pub payload: String,
    pub ts: u64,
}

struct SubscriptionEntry {
    connection_id: String,
    handle: JoinHandle<()>,
}

struct SubscriptionRegistry {
    subs: HashMap<String, SubscriptionEntry>,
}

static REGISTRY: OnceLock<Mutex<SubscriptionRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<SubscriptionRegistry> {
    REGISTRY.get_or_init(|| {
        Mutex::new(SubscriptionRegistry {
            subs: HashMap::new(),
        })
    })
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Validate subscribe arguments (channels and/or patterns required; no empty names).
pub fn validate_subscribe_args(channels: &[String], patterns: &[String]) -> Result<(), String> {
    let ch: Vec<&str> = channels
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let pat: Vec<&str> = patterns
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if ch.is_empty() && pat.is_empty() {
        return Err("at least one channel or pattern is required".into());
    }
    Ok(())
}

fn normalize_names(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn message_payload(msg: &redis::Msg) -> String {
    msg.get_payload::<String>()
        .or_else(|_| {
            msg.get_payload::<Vec<u8>>()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        })
        .unwrap_or_default()
}

pub async fn publish<C>(conn: &mut C, channel: &str, message: &str) -> Result<u64, String>
where
    C: AsyncCommands + ConnectionLike + Send,
{
    let channel = channel.trim();
    if channel.is_empty() {
        return Err("channel is required".into());
    }
    conn.publish(channel, message)
        .await
        .map_err(|e| e.to_string())
}

pub async fn publish_on_live(
    live: &mut RedisLiveConn,
    channel: &str,
    message: &str,
) -> Result<u64, String> {
    crate::with_redis_conn!(live, |conn| publish(conn, channel, message).await)
}

async fn remove_subscription(subscription_id: &str) {
    let mut reg = registry().lock().await;
    reg.subs.remove(subscription_id);
}

pub async fn unsubscribe(subscription_id: &str) -> Result<(), String> {
    let mut reg = registry().lock().await;
    let entry = reg
        .subs
        .remove(subscription_id)
        .ok_or_else(|| format!("subscription not found: {subscription_id}"))?;
    entry.handle.abort();
    Ok(())
}

pub async fn cleanup_connection_subscriptions(connection_id: &str) {
    let mut reg = registry().lock().await;
    let ids: Vec<String> = reg
        .subs
        .iter()
        .filter(|(_, e)| e.connection_id == connection_id)
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if let Some(entry) = reg.subs.remove(&id) {
            entry.handle.abort();
        }
    }
}

pub async fn start_subscription<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    driver: Arc<RedisDriver>,
    connection_id: String,
    channels: Vec<String>,
    patterns: Vec<String>,
) -> Result<String, String> {
    validate_subscribe_args(&channels, &patterns)?;

    let plan = driver.connection_plan(&connection_id).await.map_err(|e| e.to_string())?;
    let channels = normalize_names(&channels);
    let patterns = normalize_names(&patterns);
    if channels.is_empty() && patterns.is_empty() {
        return Err("at least one channel or pattern is required".into());
    }

    let subscription_id = uuid::Uuid::new_v4().to_string();
    let sub_id_for_task = subscription_id.clone();
    let conn_id_for_task = connection_id.clone();
    let sub_id_log = sub_id_for_task.clone();

    let handle = tokio::spawn(async move {
        let result = run_subscribe_loop(
            &plan,
            &channels,
            &patterns,
            move |channel, payload| {
                let event = RedisPubSubMessageEvent {
                    connection_id: conn_id_for_task.clone(),
                    subscription_id: sub_id_for_task.clone(),
                    channel,
                    payload,
                    ts: now_millis(),
                };
                let _ = app.emit(EVENT_NAME, &event);
            },
        )
        .await;
        if let Err(e) = result {
            tracing::warn!(
                subscription_id = %sub_id_log,
                error = %e,
                "redis pubsub subscription ended with error"
            );
        }
        remove_subscription(&sub_id_log).await;
    });

    let mut reg = registry().lock().await;
    reg.subs.insert(
        subscription_id.clone(),
        SubscriptionEntry {
            connection_id,
            handle,
        },
    );

    Ok(subscription_id)
}

async fn run_subscribe_loop<F>(
    plan: &ConnectionPlan,
    channels: &[String],
    patterns: &[String],
    mut on_message: F,
) -> Result<(), String>
where
    F: FnMut(String, String) + Send,
{
    let mut pubsub = open_pubsub_connection(plan)
        .await
        .map_err(|e| e.to_string())?;

    for ch in channels {
        pubsub
            .subscribe(ch)
            .await
            .map_err(|e| format!("SUBSCRIBE {ch}: {e}"))?;
    }
    for pat in patterns {
        pubsub
            .psubscribe(pat)
            .await
            .map_err(|e| format!("PSUBSCRIBE {pat}: {e}"))?;
    }

    let mut stream = pubsub.on_message();
    while let Some(msg) = stream.next().await {
        let channel = msg.get_channel_name().to_string();
        let payload = message_payload(&msg);
        on_message(channel, payload);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_subscribe_requires_channels_or_patterns() {
        assert!(validate_subscribe_args(&[], &[]).is_err());
        assert!(validate_subscribe_args(&["  ".into()], &[]).is_err());
    }

    #[test]
    fn validate_subscribe_accepts_channels() {
        assert!(validate_subscribe_args(&["news".into()], &[]).is_ok());
    }

    #[test]
    fn validate_subscribe_accepts_patterns() {
        assert!(validate_subscribe_args(&[], &["news.*".into()]).is_ok());
    }

    #[test]
    fn validate_subscribe_ignores_blank_entries() {
        assert!(validate_subscribe_args(&["  ".into(), "a".into()], &[]).is_ok());
    }

    #[tokio::test]
    async fn unsubscribe_unknown_id_returns_error() {
        let err = unsubscribe("nonexistent-subscription-id").await.unwrap_err();
        assert!(err.contains("subscription not found"));
    }

    #[tokio::test]
    async fn cleanup_connection_subscriptions_is_noop_when_empty() {
        cleanup_connection_subscriptions("conn-does-not-exist").await;
    }
}
