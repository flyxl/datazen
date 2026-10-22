//! Pub/Sub subscribe loop, publish helper, and subscription registry.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use redis::aio::ConnectionLike;
use redis::AsyncCommands;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::connect::{open_pubsub_connection, ConnectionPlan};
use crate::driver::RedisDriver;

#[allow(dead_code)] // event name reserved for the Redis plugin Pub/Sub sink
pub const EVENT_NAME: &str = "redis-pubsub-message";

pub type PubSubEmitter = std::sync::Arc<dyn Fn(RedisPubSubMessageEvent) + Send + Sync>;

static PUBSUB_EMITTER: OnceLock<PubSubEmitter> = OnceLock::new();

/// Install the frontend event sink. Called once from the Redis plugin setup hook.
#[allow(dead_code)]
pub fn set_pubsub_emitter(emitter: PubSubEmitter) {
    let _ = PUBSUB_EMITTER.set(emitter);
}

fn emit_pubsub_message(event: RedisPubSubMessageEvent) {
    if let Some(emitter) = PUBSUB_EMITTER.get() {
        emitter(event);
    }
}

/// A Pub/Sub message pushed to the frontend (`redis-pubsub-message`).
///
/// ## Which id the payload carries
///
/// `connectionId` is the **persisted connection config id** (the host's
/// `connectionId`, `naming.md` §1) — *not* the driver's own runtime handle
/// (`ConnectionHandle.pool_id`, `redis_<uuid>`) and *not* the host's runtime
/// session id (`dbSessionId`, `dbs_<hex>`). Those are three different id
/// spaces, and the frontend can only match the first one: it is the id the
/// Redis tab of a saved connection is filed under.
///
/// The event answers "**which connection does this message belong to**" —
/// `naming.md` §5 files that under `connectionId` ("UI 连接归属"). The
/// *call* side is the other half: `pubsub_subscribe` / `pubsub_unsubscribe` /
/// `pubsub_publish` are operations on an already-established session, so they
/// are invoked with `dbSessionId` ("操作已建立连接会话").
///
/// Putting the driver handle here — as this payload used to — makes the field
/// a name the value cannot answer to: the frontend compares it against an id
/// it never receives, the filter never matches, and every message is dropped
/// without a word.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisPubSubMessageEvent {
    /// Persisted connection config id (the host's `connectionId`).
    pub connection_id: String,
    pub subscription_id: String,
    pub channel: String,
    pub payload: String,
    pub ts: u64,
}

/// Serializable info about an active subscription, returned to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    pub subscription_id: String,
    pub channels: Vec<String>,
    pub patterns: Vec<String>,
}

/// Aggregated Pub/Sub message statistics for a connection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PubSubStats {
    pub total_messages: u64,
    pub by_channel: HashMap<String, u64>,
}

struct SubscriptionEntry {
    /// The driver's own runtime handle for the session that owns this
    /// subscription (`ConnectionHandle.pool_id`). **Not** a `connectionId` and
    /// **not** a `dbSessionId` — it is the driver's private id space, and it is
    /// what the registry is keyed by.
    #[allow(dead_code)]
    pool_id: String,
    channels: Vec<String>,
    patterns: Vec<String>,
    handle: JoinHandle<()>,
}

struct SubscriptionRegistry {
    subs: HashMap<String, SubscriptionEntry>,
    /// Per-session message counters: driver runtime handle (`pool_id`) ->
    /// (total, by_channel)
    stats: HashMap<String, (u64, HashMap<String, u64>)>,
}

static REGISTRY: OnceLock<Mutex<SubscriptionRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<SubscriptionRegistry> {
    REGISTRY.get_or_init(|| {
        Mutex::new(SubscriptionRegistry {
            subs: HashMap::new(),
            stats: HashMap::new(),
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

/// Open a subscription on the session named by `pool_id`.
///
/// `pool_id` is the driver's own runtime handle for the session
/// (`ConnectionHandle.pool_id`). The **event** the subscription produces is
/// attributed to the session's *persisted connection config id* — see
/// [`RedisPubSubMessageEvent`] for why that is the only id the frontend can
/// match.
pub async fn start_subscription(
    driver: &RedisDriver,
    pool_id: String,
    channels: Vec<String>,
    patterns: Vec<String>,
) -> Result<String, String> {
    validate_subscribe_args(&channels, &patterns)?;

    let plan = driver
        .connection_plan(&pool_id)
        .await
        .map_err(|e| e.to_string())?;
    // The id the event carries: the persisted connection config id. It is
    // resolved from the session's own connection record, never guessed from
    // `pool_id` — a driver handle under the name `connectionId` is a value the
    // frontend cannot match.
    let connection_id = driver
        .connection_config_id(&pool_id)
        .await
        .ok_or_else(|| format!("redis connection not found: {pool_id}"))?;
    let channels = normalize_names(&channels);
    let patterns = normalize_names(&patterns);
    if channels.is_empty() && patterns.is_empty() {
        return Err("at least one channel or pattern is required".into());
    }

    let subscription_id = uuid::Uuid::new_v4().to_string();
    let sub_id_for_task = subscription_id.clone();
    let conn_id_for_task = connection_id.clone();
    let sub_id_log = sub_id_for_task.clone();

    let channels_for_loop = channels.clone();
    let patterns_for_loop = patterns.clone();
    let pool_id_for_stats = pool_id.clone();
    let handle = tokio::spawn(async move {
        let result = run_subscribe_loop(
            &plan,
            &channels_for_loop,
            &patterns_for_loop,
            move |channel, payload| {
                // Increment per-session stats
                {
                    // Stats are updated synchronously via blocking task
                    let channel_clone = channel.clone();
                    let pool_clone = pool_id_for_stats.clone();
                    tokio::spawn(async move {
                        let mut reg = registry().lock().await;
                        let entry = reg
                            .stats
                            .entry(pool_clone)
                            .or_insert_with(|| (0, HashMap::new()));
                        entry.0 += 1;
                        *entry.1.entry(channel_clone).or_insert(0) += 1;
                    });
                }
                emit_pubsub_message(RedisPubSubMessageEvent {
                    connection_id: conn_id_for_task.clone(),
                    subscription_id: sub_id_for_task.clone(),
                    channel,
                    payload,
                    ts: now_millis(),
                });
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
            pool_id,
            channels,
            patterns,
            handle,
        },
    );

    Ok(subscription_id)
}

/// List all active subscriptions of one session.
///
/// `pool_id` is the driver's runtime handle for that session — the same id
/// `pubsub_subscribe` was dispatched with.
pub async fn list_active_subscriptions(pool_id: &str) -> Vec<SubscriptionInfo> {
    let reg = registry().lock().await;
    reg.subs
        .iter()
        .filter(|(_, e)| e.pool_id == pool_id)
        .map(|(id, e)| SubscriptionInfo {
            subscription_id: id.clone(),
            channels: e.channels.clone(),
            patterns: e.patterns.clone(),
        })
        .collect()
}

/// Get aggregated message statistics of one session.
///
/// `pool_id` is the driver's runtime handle for that session.
pub async fn pubsub_stats(pool_id: &str) -> PubSubStats {
    let reg = registry().lock().await;
    match reg.stats.get(pool_id) {
        Some((total, by_channel)) => PubSubStats {
            total_messages: *total,
            by_channel: by_channel.clone(),
        },
        None => PubSubStats {
            total_messages: 0,
            by_channel: HashMap::new(),
        },
    }
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
    use datazen_driver_api::ConnectionConfig;
    use datazen_driver_api::DatabaseDriver;
    use datazen_driver_api::SslMode;
    use std::time::Duration;

    /// The wire shape is a cross-end contract: the frontend filters on
    /// `connectionId` and reads `subscriptionId` / `channel` / `payload` / `ts`.
    /// A key that silently changes name breaks the panel without breaking the
    /// build, so the exact key set is pinned here.
    ///
    /// `dbSessionId` is asserted *absent* on purpose: the payload attributes a
    /// message to a **connection**, and a second, session-shaped identity would
    /// be the dual-mode fallback `naming.md` forbids.
    #[test]
    fn pubsub_event_wire_shape_is_camel_case_and_connection_scoped() {
        let event = RedisPubSubMessageEvent {
            connection_id: "conn-1".into(),
            subscription_id: "sub-1".into(),
            channel: "news".into(),
            payload: "hello".into(),
            ts: 1_700_000_000_000,
        };
        let json = serde_json::to_value(&event).expect("the event serializes");
        assert_eq!(
            json,
            serde_json::json!({
                "connectionId": "conn-1",
                "subscriptionId": "sub-1",
                "channel": "news",
                "payload": "hello",
                "ts": 1_700_000_000_000u64,
            })
        );
        assert!(
            json.get("dbSessionId").is_none(),
            "the payload attributes by connection only: {json}"
        );
    }

    /// The attribution survives the whole trip: a real subscription on a real
    /// server emits an event whose `connectionId` is the **persisted
    /// connection config id** the driver was configured with — not the
    /// driver's own runtime handle (`redis_<uuid>`), which the frontend has no
    /// way to match.
    ///
    /// This is the layer that cannot be faked offline: the id travels from
    /// `ConnectionConfig.id` through the driver's connection record into the
    /// emitted payload, and every step of that chain needs a live connection.
    #[tokio::test]
    async fn a_delivered_message_is_attributed_to_the_persisted_connection_id() {
        let server = crate::live_server::live_server_or_skip!();
        server.wait_until_serving().await.expect(
            "the harness starts its own server and reports loudly if that fails; a failure here \
             means the binary is installed but unusable",
        );

        // The emitter is a process-wide one-shot sink. Installing it here is
        // safe because this is the only test in the crate that subscribes.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        set_pubsub_emitter(std::sync::Arc::new(
            move |event: RedisPubSubMessageEvent| {
                let _ = tx.send(event);
            },
        ));

        let config = ConnectionConfig {
            id: "conn-live-1".into(),
            name: "Redis".into(),
            database_type: "redis".into(),
            host: Some("127.0.0.1".into()),
            port: Some(server.port()),
            database: Some("db0".into()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: 10,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        };

        let driver = RedisDriver::new();
        let handle = driver
            .connect(&config)
            .await
            .expect("the live server is serving, so the driver connects");
        let subscription_id =
            start_subscription(&driver, handle.pool_id.clone(), vec!["news".into()], vec![])
                .await
                .expect("the subscription starts");

        // The SUBSCRIBE is sent from the spawned task, so the first publish can
        // still race it. Publish until a message comes back.
        let event = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                driver
                    .plugin_pubsub_publish(&handle.pool_id, "news", "hello")
                    .await
                    .expect("publish reaches the live server");
                if let Ok(Some(event)) =
                    tokio::time::timeout(Duration::from_millis(250), rx.recv()).await
                {
                    break event;
                }
            }
        })
        .await
        .expect("a published message reaches the subscriber");

        assert_eq!(
            event.connection_id, "conn-live-1",
            "the event is attributed to the persisted connection config id"
        );
        assert_ne!(
            event.connection_id, handle.pool_id,
            "the driver's own runtime handle must never stand in for a connection id"
        );
        assert_eq!(event.subscription_id, subscription_id);
        assert_eq!(event.channel, "news");
        assert_eq!(event.payload, "hello");
    }

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
        let err = unsubscribe("nonexistent-subscription-id")
            .await
            .unwrap_err();
        assert!(err.contains("subscription not found"));
    }

    #[tokio::test]
    async fn list_active_subscriptions_empty_for_unknown_connection() {
        let subs = list_active_subscriptions("nonexistent-connection-id").await;
        assert!(subs.is_empty());
    }

    #[tokio::test]
    async fn pubsub_stats_returns_zero_for_unknown_connection() {
        let stats = pubsub_stats("nonexistent-connection-id").await;
        assert_eq!(stats.total_messages, 0);
        assert!(stats.by_channel.is_empty());
    }
}
