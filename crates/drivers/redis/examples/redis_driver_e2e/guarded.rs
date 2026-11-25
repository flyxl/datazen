//! Exercise guarded edits against Redis; an independent client verifies the stored values.

use serde_json::{json, Value};

use super::Journey;

async fn case(journey: &Journey<'_>, operation: &str, payload: Value, applied: bool) {
    assert_eq!(
        journey.command(operation, payload.clone()).await["applied"],
        applied,
        "{operation}: {payload}"
    );
}

pub async fn run(j: &mut Journey<'_>, with_json: bool) {
    let string = "contract:guard:string";
    for (operation, payload, applied) in [
        (
            "set_string",
            json!({"key":string,"value":"initial","createOnly":true}),
            true,
        ),
        (
            "set_string",
            json!({"key":string,"value":"not written","createOnly":true}),
            false,
        ),
        (
            "set_ttl",
            json!({"key":string,"expectedType":"list","ttlSeconds":120}),
            false,
        ),
        (
            "set_ttl",
            json!({"key":string,"expectedType":"string","ttlSeconds":120}),
            true,
        ),
        (
            "set_string",
            json!({"key":string,"expectedValue":"stale","value":"not written"}),
            false,
        ),
        (
            "set_string",
            json!({"key":string,"expectedValue":"initial","value":"new"}),
            true,
        ),
        (
            "rename",
            json!({"key":string,"newKey":"contract:catalog","noOverwrite":true}),
            false,
        ),
        (
            "rename",
            json!({"key":string,"newKey":"contract:guard:renamed","noOverwrite":true}),
            true,
        ),
    ] {
        case(j, operation, payload, applied).await;
    }
    let renamed = "contract:guard:renamed";
    assert_eq!(j.read::<String>("GET", &[renamed]).await, "new");
    assert_eq!(j.read::<u64>("EXISTS", &[string]).await, 0);
    assert!((1..=120).contains(&j.read::<i64>("TTL", &[renamed]).await));
    case(
        j,
        "set_ttl",
        json!({"key":renamed,"expectedType":"string","ttlSeconds":-1}),
        true,
    )
    .await;
    assert_eq!(j.read::<i64>("TTL", &[renamed]).await, -1);
    assert!(j
        .driver
        .execute_command(
            j.handle,
            "set_ttl",
            json!({
                "key":renamed, "expectedType":"string", "ttlSeconds":0
            })
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("positive seconds"));

    let hash = "contract:guard:hash";
    let list = "contract:guard:list";
    let set = "contract:guard:set";
    let sorted = "contract:guard:sorted";
    let stream = "contract:guard:stream";
    assert_eq!(j.read::<u64>("HSET", &[hash, "one", "before"]).await, 1);
    assert_eq!(j.read::<u64>("RPUSH", &[list, "before"]).await, 1);
    assert_eq!(j.read::<u64>("SADD", &[set, "before"]).await, 1);
    assert_eq!(j.read::<u64>("ZADD", &[sorted, "1", "one"]).await, 1);
    assert_eq!(
        j.read::<String>("XADD", &[stream, "1-0", "state", "before"])
            .await,
        "1-0"
    );
    for (operation, payload, applied) in [
        (
            "hash_del",
            json!({"key":hash,"guardedField":"one","expectedValue":"before"}),
            false,
        ),
        (
            "hash_set",
            json!({"key":hash,"field":"two","value":"kept","createFieldOnly":true}),
            true,
        ),
        (
            "hash_set",
            json!({"key":hash,"field":"two","value":"discarded","createFieldOnly":true}),
            false,
        ),
        (
            "hash_set",
            json!({"key":hash,"field":"one","expectedValue":"before","value":"updated"}),
            true,
        ),
        (
            "hash_del",
            json!({"key":hash,"guardedField":"one","expectedValue":"before"}),
            false,
        ),
        (
            "hash_del",
            json!({"key":hash,"guardedField":"one","expectedValue":"updated"}),
            true,
        ),
        (
            "list_pop",
            json!({"key":list,"expectedHead":"before"}),
            false,
        ),
        (
            "list_push",
            json!({"key":list,"appendExistingOnly":true,"appendValue":"kept"}),
            true,
        ),
        (
            "list_set",
            json!({"key":list,"index":0,"expectedValue":"before","value":"updated"}),
            true,
        ),
        (
            "list_pop",
            json!({"key":list,"expectedHead":"before"}),
            false,
        ),
        (
            "list_pop",
            json!({"key":list,"expectedHead":"updated"}),
            true,
        ),
        (
            "set_remove",
            json!({"key":set,"guardedMember":"before"}),
            false,
        ),
        ("set_add", json!({"key":set,"existingMember":"kept"}), true),
        (
            "set_remove",
            json!({"key":set,"guardedMember":"before"}),
            true,
        ),
        (
            "zset_remove",
            json!({"key":sorted,"guardedMember":"one","expectedScore":"1"}),
            false,
        ),
        (
            "zset_add",
            json!({"key":sorted,"existingMember":"two","newScore":"2.5","createMemberOnly":true}),
            true,
        ),
        (
            "zset_add",
            json!({"key":sorted,"existingMember":"one","newScore":"90","createMemberOnly":true}),
            false,
        ),
        (
            "zset_add",
            json!({"key":sorted,"existingMember":"one","expectedScore":"5","newScore":"3"}),
            false,
        ),
        (
            "zset_add",
            json!({"key":sorted,"existingMember":"one","expectedScore":"1","newScore":"3"}),
            true,
        ),
        (
            "zset_remove",
            json!({"key":sorted,"guardedMember":"one","expectedScore":"1"}),
            false,
        ),
        (
            "zset_remove",
            json!({"key":sorted,"guardedMember":"one","expectedScore":"3"}),
            true,
        ),
        (
            "xadd",
            json!({"key":string,"existingField":"state","fieldValue":"discarded"}),
            false,
        ),
        (
            "xadd",
            json!({"key":stream,"existingField":"state","fieldValue":"kept"}),
            true,
        ),
        (
            "xgroup_create",
            json!({"key":stream,"group":"guarded","existingStreamOnly":true}),
            true,
        ),
        (
            "xgroup_create",
            json!({"key":stream,"group":"guarded","existingStreamOnly":true}),
            false,
        ),
    ] {
        case(j, operation, payload, applied).await;
    }
    assert_eq!(
        j.read::<Vec<String>>("HGETALL", &[hash]).await,
        ["two", "kept"]
    );
    assert_eq!(
        j.read::<Vec<String>>("LRANGE", &[list, "0", "-1"]).await,
        ["kept"]
    );
    assert_eq!(j.read::<Vec<String>>("SMEMBERS", &[set]).await, ["kept"]);
    assert_eq!(
        j.read::<Vec<String>>("ZRANGE", &[sorted, "0", "-1", "WITHSCORES"])
            .await,
        ["two", "2.5"]
    );
    assert_eq!(j.read::<u64>("XLEN", &[stream]).await, 2);
    assert_eq!(j.read::<u64>("EXISTS", &[string]).await, 0);

    if with_json {
        let key = "contract:guard:json";
        assert_eq!(
            j.read::<String>("JSON.SET", &[key, "$", "{\"state\":\"before\"}"])
                .await,
            "OK"
        );
        case(
            j,
            "json_set",
            json!({"key":key,"expectedDocument":"{}","value":"{}"}),
            false,
        )
        .await;
        let previous = j.read::<String>("JSON.GET", &[key]).await;
        case(
            j,
            "json_set",
            json!({
                "key":key,"expectedDocument":previous,"value":"{\"state\":\"updated\"}"
            }),
            true,
        )
        .await;
        assert_eq!(
            j.read::<String>("JSON.GET", &[key]).await,
            "{\"state\":\"updated\"}"
        );
    }
}
