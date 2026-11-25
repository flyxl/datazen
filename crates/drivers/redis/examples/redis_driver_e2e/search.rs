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

    let mut cursor = 0;
    let mut task = None;
    let mut observed = Vec::new();
    for _ in 0..256 {
        let page = journey
            .command(
                "scan_values",
                json!({
                    "query":"ordinary", "mode":"value", "pattern":"contract:search:*",
                    "count":3, "cursor":cursor, "task_id":task
                }),
            )
            .await;
        let page_task = page["taskId"].as_str().unwrap();
        if let Some(prior) = task.as_deref() {
            assert_eq!(page_task, prior);
        }
        task = Some(page_task.to_owned());
        observed.extend(page["matched"].as_array().unwrap().iter().map(|hit| {
            assert_eq!(hit["matchedIn"], "value");
            hit["key"].as_str().unwrap().to_owned()
        }));
        cursor = page["cursor"].as_u64().unwrap();
        if cursor == 0 {
            assert_eq!(page["done"], true);
            break;
        }
    }
    assert_eq!(cursor, 0);
    observed.sort();
    observed.dedup();
    assert_eq!(
        observed,
        (0..40)
            .map(|i| format!("{prefix}{i:02}"))
            .collect::<Vec<_>>()
    );

    let wide = "contract:search:wide";
    let body = format!(
        "{}Mark\0ß{} literal \\xZZ",
        "a".repeat(180),
        "b".repeat(180)
    );
    assert_eq!(journey.read::<String>("SET", &[wide, &body]).await, "OK");
    for query in ["mark\\x00\\xc3\\x9f", "MARK\\x00\\xC3\\x9F"] {
        let result = journey
            .command(
                "scan_values",
                json!({
                    "query":query,"pattern":wide,"mode":"value","count":256,
                }),
            )
            .await;
        let row = &result["matched"][0];
        assert_eq!(row["key"], wide);
        let preview = row["preview"].as_str().unwrap();
        assert!(
            preview.starts_with('…') && preview.ends_with('…'),
            "{preview}"
        );
        assert!(preview.contains("Mark\0ß"));
    }
    let literal = journey
        .command(
            "scan_values",
            json!({
                "query":"\\xZZ","pattern":wide,"mode":"value","count":256,
            }),
        )
        .await;
    assert_eq!(literal["matched"][0]["key"], wide);
    let empty = journey
        .command(
            "scan_values",
            json!({
                "query":"","pattern":wide,"mode":"value","count":256,
            }),
        )
        .await;
    let preview = empty["matched"][0]["preview"].as_str().unwrap();
    assert!(preview.starts_with('a') && preview.ends_with('…'));
    assert_eq!(journey.read::<u64>("DEL", &[wide]).await, 1);

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
