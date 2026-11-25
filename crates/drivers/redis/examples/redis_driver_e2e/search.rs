use serde_json::json;

use super::Journey;

pub async fn run(journey: &mut Journey<'_>) {
    let prefix = "contract:search:";
    for i in 0..40 {
        journey
            .command(
                "set_string",
                json!({
                    "key":format!("{prefix}{i:02}"), "value":format!("ordinary value {i}")
                }),
            )
            .await;
    }
    journey
        .command(
            "set_string_raw",
            json!({
                "key":"contract:search:binary", "dataB64":"UHJlZml4OnT/Cg=="
            }),
        )
        .await;
    journey
        .command(
            "hash_set",
            json!({
                "key":"contract:search:hash", "field":"name", "value":"aggregate should not be read"
            }),
        )
        .await;

    let value = journey
        .command(
            "scan_values",
            json!({
                "query":"\\xFF","mode":"all","pattern":"contract:search:*","count":256
            }),
        )
        .await;
    assert_eq!(value["cursor"], 0);
    assert_eq!(value["matched"].as_array().unwrap().len(), 1);
    assert_eq!(value["matched"][0]["key"], "contract:search:binary");
    assert_eq!(value["matched"][0]["matchedIn"], "value");
    let by_key = journey
        .command(
            "scan_values",
            json!({
                "query":"HASH","mode":"key","pattern":"contract:search:*","count":256
            }),
        )
        .await;
    assert_eq!(by_key["matched"][0]["key"], "contract:search:hash");
    assert_eq!(by_key["matched"][0]["matchedIn"], "key");

    let started = journey
        .command(
            "scan_values",
            json!({
                "query":"ordinary","mode":"value","pattern":"contract:search:*","count":1
            }),
        )
        .await;
    assert!(!started["done"].as_bool().unwrap());
    let task = started["taskId"].as_str().unwrap();
    let cursor = started["cursor"].as_u64().unwrap();
    assert_ne!(cursor, 0);
    assert_eq!(
        journey
            .command("scan_abort", json!({"taskId":"different"}))
            .await["aborted"],
        false
    );
    assert_eq!(
        journey.command("scan_abort", json!({"taskId":task})).await["aborted"],
        true
    );
    let cancelled = journey
        .command(
            "scan_values",
            json!({
                "query":"ordinary", "taskId":task, "cursor":cursor
            }),
        )
        .await;
    assert_eq!(cancelled["cancelled"], true);
    assert_eq!(cancelled["done"], true);
    assert_eq!(cancelled["matched"], json!([]));

    let bounded = journey
        .command(
            "scan_values",
            json!({
                "query":"ordinary", "mode":"value","pattern":"contract:search:*",
                "maxKeys":1,"byteBudget":4,"perValuePeek":4,"count":256
            }),
        )
        .await;
    assert_eq!(bounded["limitHit"], true);
    assert_eq!(bounded["matched"], json!([]));
}
