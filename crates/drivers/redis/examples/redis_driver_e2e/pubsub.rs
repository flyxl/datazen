//! Exercise the production driver event outlet using messages from real Redis.

use std::{sync::Arc, time::Duration};

use datazen_driver_api::{DriverEvent, DriverEventOutlet, DriverEventSink};
use datazen_driver_redis::RedisPubSubOutlet;
use serde_json::json;
use tokio::sync::mpsc;

use super::Journey;

struct Sink(mpsc::UnboundedSender<DriverEvent>);

impl DriverEventSink for Sink {
    fn emit(&self, event: &DriverEvent) {
        let _ = self.0.send(event.clone());
    }
}

pub async fn run(j: &mut Journey<'_>, config_id: &str) {
    let (sender, mut events) = mpsc::unbounded_channel();
    RedisPubSubOutlet.install(Arc::new(Sink(sender)));
    let invalid = j
        .driver
        .execute_command(
            j.handle,
            "pubsub_subscribe",
            json!({"channels":[" "],"patterns":[" "]}),
        )
        .await
        .unwrap_err();
    assert!(invalid.to_string().contains("channel or pattern"));
    let invalid = j
        .driver
        .execute_command(
            j.handle,
            "pubsub_publish",
            json!({"channel":" ", "message":"never sent"}),
        )
        .await
        .unwrap_err();
    assert!(invalid.to_string().contains("channel is required"));

    let id = j
        .command(
            "pubsub_subscribe",
            json!({
                "channels":[" contract:event:text ",""],
                "patterns":[" contract:event:binary:* "]
            }),
        )
        .await;
    let id = id.as_str().unwrap();
    let listed = j.command("pubsub_list_subscriptions", json!({})).await;
    assert_eq!(
        listed["subscriptions"],
        json!([{
            "subscriptionId":id,"channels":["contract:event:text"],"patterns":["contract:event:binary:*"]
        }])
    );
    assert_eq!(
        j.command(
            "pubsub_publish",
            json!({
                "channel":" contract:event:text ","message":"真实 Redis"
            })
        )
        .await,
        1
    );
    let binary_channel = "contract:event:binary:1";
    let count: u64 = redis::cmd("PUBLISH")
        .arg(binary_channel)
        .arg(&[0xff_u8, b'!'][..])
        .query_async(&mut j.oracle)
        .await
        .unwrap();
    assert_eq!(count, 1);
    for (channel, payload) in [
        ("contract:event:text", "真实 Redis"),
        (binary_channel, "�!"),
    ] {
        let event = tokio::time::timeout(Duration::from_secs(4), events.recv())
            .await
            .expect("Redis message must reach host event sink")
            .unwrap();
        assert_eq!(event.name, "redis-pubsub-message");
        assert_eq!(event.payload["connectionId"], config_id);
        assert_eq!(event.payload["subscriptionId"], id);
        assert_eq!(event.payload["channel"], channel);
        assert_eq!(event.payload["payload"], payload);
        assert!(event.payload["ts"].as_u64().unwrap() > 0);
    }
    let stats = j.command("pubsub_stats", json!({})).await;
    assert_eq!(stats["totalMessages"], 2);
    assert_eq!(stats["byChannel"][binary_channel], 1);
    assert_eq!(stats["byChannel"]["contract:event:text"], 1);
    assert_eq!(
        j.command("pubsub_unsubscribe", json!({"subscription_id":id}))
            .await["ok"],
        true
    );
    assert_eq!(
        j.command("pubsub_list_subscriptions", json!({})).await["subscriptions"],
        json!([])
    );
    let gone = j
        .driver
        .execute_command(j.handle, "pubsub_unsubscribe", json!({"subscriptionId":id}))
        .await
        .unwrap_err();
    assert!(gone.to_string().contains("subscription not found"));
    let ids_in = |line: &str| {
        line.lines()
            .filter_map(|row| {
                row.split_whitespace()
                    .find_map(|field| field.strip_prefix("id="))
                    .map(str::to_owned)
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    let before = ids_in(
        &j.read::<String>("CLIENT", &["LIST", "TYPE", "PUBSUB"])
            .await,
    );
    let reconnect_channel = "contract:event:disconnect";
    j.command("pubsub_subscribe", json!({"channels":[reconnect_channel]}))
        .await;
    let after = ids_in(
        &j.read::<String>("CLIENT", &["LIST", "TYPE", "PUBSUB"])
            .await,
    );
    let ids: Vec<_> = after.difference(&before).map(String::as_str).collect();
    assert_eq!(
        ids.len(),
        1,
        "only this fixture's just-opened subscription may be closed"
    );
    assert_eq!(j.read::<u64>("CLIENT", &["KILL", "ID", ids[0]]).await, 1);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let active = j.command("pubsub_list_subscriptions", json!({})).await;
            if active["subscriptions"] == json!([]) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a socket Redis closed must disappear from subscription state");
    assert_eq!(
        j.read::<u64>("PUBLISH", &[reconnect_channel, "not delivered"])
            .await,
        0
    );
}
