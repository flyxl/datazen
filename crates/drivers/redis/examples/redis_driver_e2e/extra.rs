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
    let hash = "contract:detail:hash";
    assert_eq!(
        journey
            .read::<u64>("HSET", &[hash, "a", "1", "b", "2"])
            .await,
        2
    );
    let detail = journey.command("get_key", json!({"key":hash})).await;
    assert_eq!(detail["value"], json!({"a":"1","b":"2"}));
    let raw = journey
        .command("get_key_raw", json!({"key":hash,"with_memory":true}))
        .await;
    assert_eq!(raw["logicalLen"], 2);
    assert_eq!(raw["rawB64"], Value::Null);
    assert_eq!(
        raw["memBytes"],
        journey.read::<u64>("MEMORY", &["USAGE", hash]).await
    );

    let large_key = "contract:detail:large";
    let data = "x".repeat(5 * 1024 * 1024 + 1);
    assert_eq!(
        journey.read::<String>("SET", &[large_key, &data]).await,
        "OK"
    );
    let raw = journey
        .command("get_key_raw", json!({"key":large_key}))
        .await;
    assert_eq!(raw["logicalLen"], data.len());
    assert_eq!(raw["truncated"], true);
    assert_eq!(raw["rawB64"], Value::Null);
    assert_eq!(journey.read::<u64>("DEL", &[large_key]).await, 1);

    let stream = "contract:detail:stream";
    let first = journey
        .read::<String>("XADD", &[stream, "1-0", "field", "first"])
        .await;
    let second = journey
        .read::<String>("XADD", &[stream, "2-0", "field", "second", "other", "data"])
        .await;
    let detail = journey.command("get_key", json!({"key":stream})).await;
    assert_eq!(
        detail["value"],
        json!([
            {"id":first,"fields":{"field":"first"}},
            {"id":second,"fields":{"field":"second","other":"data"}},
        ])
    );
    assert_eq!(
        journey.read::<u64>("XDEL", &[stream, "1-0", "2-0"]).await,
        2
    );
    let empty = journey.command("get_key", json!({"key":stream})).await;
    assert_eq!(empty["value"], json!([]));

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

pub async fn tree(j: &mut Journey<'_>) {
    let prefix = "contract:tree?[literal]*:";
    let string = format!("{prefix}plain");
    let expiring = format!("{prefix}temporary");
    let folder_a = format!("{prefix}folder/a");
    let folder_b = format!("{prefix}folder/b");
    let folder_c = format!("{prefix}other|c");
    let hash = format!("{prefix}hash");
    for (key, val) in [
        (&string, "hello"),
        (&expiring, "expires"),
        (&folder_a, "A"),
        (&folder_b, "B"),
        (&folder_c, "C"),
    ] {
        assert_eq!(j.read::<String>("SET", &[key, val]).await, "OK");
    }
    assert_eq!(j.read::<u64>("EXPIRE", &[&expiring, "120"]).await, 1);
    assert_eq!(j.read::<u64>("HSET", &[&hash, "field", "value"]).await, 1);

    let page = j
        .command(
            "list_children",
            json!({
                "prefix":prefix, "sep":"/|", "count":256, "with_memory":true, "no_ttl_only":true
            }),
        )
        .await;
    let children = page["children"].as_array().unwrap();
    assert_eq!(page["cursor"], 0);
    assert_eq!(page["truncated"], false);
    assert_eq!(page["dbsize"], j.read::<u64>("DBSIZE", &[]).await);
    assert_eq!(children.len(), 4, "{page}");
    assert_eq!(
        children[0],
        json!({"kind":"folder","prefix":format!("{prefix}folder/"),"count":2})
    );
    assert_eq!(
        children[1],
        json!({"kind":"folder","prefix":format!("{prefix}other|"),"count":1})
    );
    assert_eq!(children[2]["key"], hash);
    assert_eq!(children[2]["keyType"], "hash");
    assert_eq!(children[2]["logicalLen"], 1);
    assert_eq!(children[3]["key"], string);
    assert_eq!(children[3]["keyType"], "string");
    assert_eq!(children[3]["logicalLen"], 5);
    assert_eq!(children[3]["ttl"], -1);
    assert_eq!(
        children[3]["memBytes"],
        j.read::<u64>("MEMORY", &["USAGE", &string]).await
    );

    let only_hash = j
        .command(
            "list_children",
            json!({
                "prefix":prefix,"sep":"/|","key_type":"HASH","count":256,
            }),
        )
        .await;
    let children = only_hash["children"].as_array().unwrap();
    assert_eq!(children.len(), 1, "{only_hash}");
    assert_eq!(children[0]["key"], hash);
    assert!(children[0]["memBytes"].is_null());

    for i in 0..40 {
        let key = format!("contract:budget:{i:02}");
        assert_eq!(j.read::<String>("SET", &[&key, "fixture"]).await, "OK");
    }
    let bounded = j
        .command(
            "list_children",
            json!({
                "prefix":"contract:budget:","count":256,"budget":1
            }),
        )
        .await;
    assert_eq!(bounded["truncated"], true);
    assert_ne!(bounded["cursor"], 0);
    assert_eq!(bounded["consumed"], 10); // minimum Redis SCAN COUNT
    let complete = j
        .command(
            "list_children",
            json!({
                "prefix":"contract:budget:","count":256,"budget":256
            }),
        )
        .await;
    assert_eq!(complete["cursor"], 0);
    assert_eq!(complete["children"].as_array().unwrap().len(), 40);
}

pub async fn batch(j: &mut Journey<'_>) {
    let from = "contract:batch:from:";
    let to = "contract:batch:to:";
    let key = format!("{from}valid");
    let missing = format!("{from}missing");
    let unrelated = "contract:batch:untouched";
    assert_eq!(
        j.read::<String>("SET", &[&key, "stored", "EX", "120"])
            .await,
        "OK"
    );
    assert_eq!(
        j.read::<String>("SET", &[unrelated, "untouched"]).await,
        "OK"
    );
    let ttl = j
        .command(
            "batch_set_ttl",
            json!({"keys":[key,missing],"ttl_seconds":-1}),
        )
        .await;
    assert_eq!(ttl["updated"], 1);
    assert_eq!(ttl["errors"][0]["key"], missing);
    assert_eq!(j.read::<i64>("PTTL", &[&key]).await, -1);
    let rename = j
        .command(
            "batch_rename_prefix",
            json!({
                "old_prefix":from,"new_prefix":to,"keys":[key,missing,unrelated]
            }),
        )
        .await;
    assert_eq!(rename["renamed"], 1);
    assert_eq!(rename["errors"][0]["key"], missing);
    assert_eq!(j.read::<u64>("EXISTS", &[&key]).await, 0);
    assert_eq!(
        j.read::<String>("GET", &[&format!("{to}valid")]).await,
        "stored"
    );
    assert_eq!(j.read::<String>("GET", &[unrelated]).await, "untouched");
}
