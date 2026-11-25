//! Verify that the live MONITOR stream stays bounded and releases its subscription.

use std::time::Duration;

use serde_json::json;

use super::Journey;

pub async fn run(j: &mut Journey<'_>) {
    let monitor = j.command("monitor_start", json!({"bufferSize":1})).await;
    let id = monitor["monitorId"].as_str().unwrap();
    for index in 0..18 {
        let key = format!("contract:monitor:row:{index:02}");
        assert_eq!(j.read::<Option<String>>("GET", &[&key]).await, None);
    }
    let oversized = format!("contract:monitor:{}", "q".repeat(1100));
    assert_eq!(j.read::<Option<String>>("GET", &[&oversized]).await, None);
    let keys = (0..21)
        .map(|i| format!("contract:monitor:args:{i}"))
        .collect::<Vec<_>>();
    let reply: Vec<Option<String>> = redis::cmd("MGET")
        .arg(&keys)
        .query_async(&mut j.oracle)
        .await
        .unwrap();
    assert_eq!(reply, vec![None; keys.len()]);

    let visible = tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let snapshot = j
                .command("monitor_get_buffer", json!({"monitorId":id}))
                .await;
            let rows = snapshot["events"].as_array().unwrap();
            if rows.iter().any(|row| row["command"] == "MGET") {
                break rows.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("monitor must publish the real MGET");
    assert_eq!(visible.len(), 10); // bufferSize clamps to at least ten
    assert!(!visible
        .iter()
        .any(|row| row["args"].to_string().contains("row:00")));
    let truncated = visible
        .iter()
        .find_map(|row| row["args"][0].as_str().filter(|arg| arg.ends_with('…')))
        .expect("long Redis argument must have a visible truncation marker");
    assert_eq!(truncated.chars().count(), 1025);
    assert!(oversized.starts_with(truncated.strip_suffix('…').unwrap()));
    let rows = visible.iter().find(|row| row["command"] == "MGET").unwrap();
    assert_eq!(rows["args"], json!(&keys[..16]));
    assert_eq!(rows["db"], 0);
    assert!(rows["timestamp"].as_f64().unwrap() > 0.0);
    assert_eq!(
        j.command("monitor_stop", json!({"monitorId":id})).await["ok"],
        true
    );
    let stopped = j
        .driver
        .execute_command(j.handle, "monitor_stop", json!({"monitorId":id}))
        .await
        .unwrap_err();
    assert!(stopped.to_string().contains("monitor not found"));
}
