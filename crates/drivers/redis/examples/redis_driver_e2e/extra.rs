use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::Journey;

pub async fn archive_and_metadata(journey: &mut Journey<'_>) {
    for (key, kind) in [
        ("contract:catalog", "string"),
        ("contract:stream", "stream"),
        ("contract:list", "list"),
        ("contract:set", "set"),
        ("contract:zset", "zset"),
    ] {
        let actual: String = journey.read("TYPE", &[key]).await;
        assert_eq!(actual, kind);
        let schema = journey
            .driver
            .get_table_schema(journey.handle, key, "db0", None)
            .await
            .unwrap();
        assert!(!schema.columns.is_empty(), "{kind}");
        let detail = journey.command("get_key", json!({"key":key})).await;
        assert!(detail.is_object(), "{kind}: {detail}");
    }
    let key = "contract:archive";
    journey
        .command(
            "set_string",
            json!({"key":key,"value":"snapshot","keepTtl":false}),
        )
        .await;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 100;
    journey
        .command("set_ttl", json!({"key":key,"expireAt":stamp}))
        .await;
    let ttl: i64 = journey.read("TTL", &[key]).await;
    assert!((1..=100).contains(&ttl));
    let dump = journey
        .command("dump_keys", json!({"keys":[key,"contract:absent"]}))
        .await;
    assert_eq!(dump["entries"].as_array().unwrap().len(), 1);
    assert_eq!(dump["errors"].as_array().unwrap().len(), 1);
    let entries = dump["entries"].clone();
    let archived_ttl = entries[0]["ttlSeconds"].as_i64().unwrap();
    assert!((archived_ttl - ttl).abs() <= 1);
    let blocked = journey
        .command("restore_keys", json!({"entries":entries,"replace":false}))
        .await;
    assert_eq!(blocked["restored"], 0);
    journey
        .command(
            "set_string",
            json!({"key":key,"value":"newer","keepTtl":false}),
        )
        .await;
    let restored = journey
        .command("restore_keys", json!({"entries":entries,"replace":true}))
        .await;
    assert_eq!(restored["restored"], 1);
    assert_eq!(journey.read::<String>("GET", &[key]).await, "snapshot");
    let restored_ttl = journey.read::<i64>("TTL", &[key]).await;
    assert!((archived_ttl - 1..=archived_ttl).contains(&restored_ttl));
    assert_eq!(
        journey.command("delete_keys", json!({"keys":[key]})).await,
        1
    );
}

pub async fn json_document(journey: &mut Journey<'_>) {
    let key = "contract:json";
    let modules = journey.command("modules_list", json!({})).await;
    assert!(modules.to_string().to_ascii_lowercase().contains("rejson"));
    let created = json!({"first":{"name":"fixture"},"second":[1,2]});
    journey
        .command(
            "json_set",
            json!({"key":key,"path":"$","value":created.to_string()}),
        )
        .await;
    let fetched = journey
        .command("json_get", json!({"key":key,"path":"$","raw":true}))
        .await;
    assert_eq!(fetched["value"], created);
    assert_eq!(
        serde_json::from_str::<Value>(fetched["rawText"].as_str().unwrap()).unwrap(),
        created
    );
    let selected = journey
        .command("json_get", json!({"key":key,"path":"$.second","raw":false}))
        .await;
    assert_eq!(selected["value"], json!([[1, 2]]));
    assert_eq!(selected["rawText"], Value::Null);
    for (command, payload) in [
        ("json_get", json!({"key":" ","path":"$"})),
        ("json_set", json!({"key":key,"path":" ","value":"1"})),
        ("json_set", json!({"key":key,"path":"$","value":"invalid"})),
        ("json_del", json!({"key":key,"path":" "})),
    ] {
        assert!(journey
            .driver
            .execute_command(journey.handle, command, payload)
            .await
            .is_err());
    }
    let deleted = journey
        .command("json_del", json!({"key":key,"path":"$.second"}))
        .await;
    assert_eq!(deleted["deleted"], 1);
    let found: String = journey.read("JSON.GET", &[key]).await;
    assert_eq!(
        serde_json::from_str::<Value>(&found).unwrap(),
        json!({"first":{"name":"fixture"}})
    );
    let absent = journey
        .command(
            "json_get",
            json!({"key":"contract:absent","path":"$","raw":true}),
        )
        .await;
    assert_eq!(absent["value"], Value::Null);
}
