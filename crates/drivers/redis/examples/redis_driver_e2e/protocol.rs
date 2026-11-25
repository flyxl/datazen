use datazen_driver_api::Value;
use serde_json::json;

use super::Journey;

pub async fn run(journey: &mut Journey<'_>) {
    let driver = journey.driver;
    let handle = journey.handle;
    let hash = "contract:protocol:hash";
    for (field, value) in [("one", "un"), ("two", "deux"), ("three", "trois")] {
        journey
            .command("hash_set", json!({"key":hash,"field":field,"value":value}))
            .await;
    }
    let members = driver
        .query(handle, "HGETALL contract:protocol:hash")
        .await
        .unwrap();
    assert_eq!(members.rows.len(), 3);
    assert_eq!(members.columns[0].name, "field");
    let array = driver
        .query(handle, "LRANGE contract:list 0 100")
        .await
        .unwrap();
    assert!(!array.rows.is_empty());

    let stream = "contract:protocol:stream";
    let id = journey
        .command("xadd", json!({"key":stream,"fields":{"field":"first"}}))
        .await;
    assert!(id["id"].as_str().is_some_and(|value| value.contains('-')));
    journey
        .command(
            "xgroup_create",
            json!({"key":stream,"group":"g","startId":"0"}),
        )
        .await;

    driver
        .query(
            handle,
            "XREADGROUP GROUP g worker COUNT 1 STREAMS contract:protocol:stream >",
        )
        .await
        .unwrap();
    let before = journey
        .command("stream_overview", json!({"key":stream}))
        .await;
    assert!(before.is_object());

    // Redis itself negotiates RESP3; subsequent commands go through the same
    // public driver connection and carry the real protocol's map responses.
    driver.query(handle, "HELLO 3").await.unwrap();
    let info_text = driver.query(handle, "INFO server").await.unwrap();
    assert!(
        matches!(&info_text.rows[0][0], Some(Value::String(info)) if info.contains("redis_version"))
    );
    let read_many = driver
        .query(
            handle,
            "MGET contract:catalog contract:absent contract:resource",
        )
        .await
        .unwrap();
    assert_eq!(read_many.rows.len(), 3);
    assert_eq!(read_many.rows[0][1], Some(Value::String("written".into())));
    assert_eq!(read_many.rows[1][1], Some(Value::Null));
    let quoted = driver
        .query(handle, r#"ECHO "引号 \"here\"""#)
        .await
        .unwrap();
    assert_eq!(
        quoted.rows[0][0],
        Some(Value::String("引号 \"here\"".into()))
    );

    let groups = journey.command("xinfo_groups", json!({"key":stream})).await;
    assert_eq!(groups[0]["name"], "g");
    let readers = journey
        .command("xinfo_consumers", json!({"key":stream,"group":"g"}))
        .await;
    assert_eq!(readers[0]["name"], "worker");
    let info = journey
        .command("stream_overview", json!({"key":stream}))
        .await;
    assert!(info.is_object());
    let pending = journey
        .command(
            "xpending",
            json!({"key":stream,"group":"g","consumer":"worker","count":3}),
        )
        .await;
    assert!(pending.to_string().contains("worker"));
    let frames = journey.command("exec", json!({"commands":"HGETALL contract:protocol:hash\nGET contract:absent\nLLEN contract:list"})).await;
    assert_eq!(frames["results"][0]["resultType"], "map");
    assert_eq!(frames["results"][1]["resultType"], "nil");
    assert_eq!(frames["results"][2]["resultType"], "scalar");
    let page = journey
        .command(
            "scan_keys",
            json!({"pattern":"contract:protocol:*","count":100,"withMemory":true}),
        )
        .await;
    assert!(page["keys"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["key"] == hash));
    let acknowledged = journey
        .command("xack", json!({"key":stream,"group":"g","ids":[id["id"]]}))
        .await;
    assert_eq!(acknowledged, 1);
    let empty = journey
        .command("xpending", json!({"key":stream,"group":"g","count":3}))
        .await;
    assert_eq!(empty["entries"], json!([]));
    driver.query(handle, "HELLO 2").await.unwrap();
    let verify: Vec<String> = journey.read("HMGET", &[hash, "one", "two", "three"]).await;
    assert_eq!(verify, ["un", "deux", "trois"]);
}
