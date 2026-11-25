use std::time::Duration;

use datazen_driver_api::{ConnectionConfig, DatabaseDriver};
use serde_json::json;

use super::Journey;

async fn inspect(
    driver: &dyn DatabaseDriver,
    config: &ConnectionConfig,
    port: u16,
    topology: &str,
    proof: &str,
) {
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/0")).unwrap();
    let oracle = client.get_multiplexed_async_connection().await.unwrap();
    driver
        .test_connection(config)
        .await
        .unwrap_or_else(|error| panic!("{topology} connection probe failed: {error}"));
    let handle = driver.connect(config).await.unwrap();
    let mut journey = Journey {
        driver,
        handle: &handle,
        oracle,
    };
    assert_eq!(
        journey.read::<String>("GET", &["contract:fixture"]).await,
        proof,
        "refuse to edit any Redis node other than the disposable fixture"
    );
    let addr = format!("127.0.0.1:{port}");
    let info = journey
        .command("info", json!({"section":"server","nodeAddr":addr}))
        .await;
    assert!(info.as_str().unwrap().contains("redis_version"));
    let filtered = journey
        .command(
            "info_filtered",
            json!({"section":"server","search":"redis_version","node_addr":addr}),
        )
        .await;
    assert_eq!(filtered["matchedEntries"], 1);
    let unpinned = journey.command("info", json!({"section":"server"})).await;
    assert!(unpinned.as_str().unwrap().contains("redis_version:"));
    let filtered = journey
        .command(
            "info_filtered",
            json!({"section":"server","search":"REDIS_VERSION"}),
        )
        .await;
    assert_eq!(filtered["matchedEntries"], 1);
    assert!(filtered["sections"][0]["entries"][0]["value"].is_string());
    let whole = journey.command("info", json!({})).await;
    assert!(whole.as_str().unwrap().contains("# Server"));

    let exec = journey
        .command("exec", json!({"commands":"PING","nodeAddr":addr}))
        .await;
    assert_eq!(exec["results"][0]["value"], "PONG");

    journey
        .command(
            "set_string",
            json!({"key":"contract:topology","value":topology}),
        )
        .await;
    let probe = journey
        .command("key_probe", json!({"key":"contract:topology"}))
        .await;
    assert!(probe.is_object());
    let distribution = journey
        .command("type_distribution", json!({"sampleLimit":128}))
        .await;
    assert!(
        distribution.to_string().contains("string"),
        "{distribution}"
    );
    let top = journey.command("memory_sample", json!({"limit":200})).await;
    assert!(
        top["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["key"] == "contract:topology"),
        "{top}"
    );

    let mon = journey
        .command("monitor_start", json!({"bufferSize":30}))
        .await;
    let id = mon["monitorId"].as_str().unwrap();
    assert_eq!(
        journey.read::<String>("GET", &["contract:topology"]).await,
        topology
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let buffer = journey
                .command("monitor_get_buffer", json!({"monitorId":id}))
                .await;
            if buffer["events"].as_array().unwrap().iter().any(|e| {
                e["command"]
                    .as_str()
                    .is_some_and(|cmd| cmd.eq_ignore_ascii_case("GET"))
                    && e.to_string().contains("contract:topology")
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("receive the independent fixture GET");
    assert_eq!(
        journey
            .command("monitor_stop", json!({"monitorId":id}))
            .await["ok"],
        true
    );
    assert!(driver
        .execute_command(&handle, "monitor_get_buffer", json!({"monitorId":id}))
        .await
        .is_err());
    driver.disconnect(handle).await.unwrap();
}

pub async fn run(
    driver: &dyn DatabaseDriver,
    base: &ConnectionConfig,
    master_port: u16,
    sentinel: u16,
    cluster: u16,
    proof: &str,
) {
    let mut routed = base.clone();
    routed.options = json!({"topology":"sentinel","sentinelMasterName":"native-test",
                            "sentinelNodes":[format!("127.0.0.1:{sentinel}")]})
    .as_object()
    .cloned();
    inspect(driver, &routed, master_port, "sentinel", proof).await;

    let mut routed = base.clone();
    routed.port = Some(cluster);
    routed.options = json!({"topology":"cluster","clusterNodes":[format!("127.0.0.1:{cluster}")]})
        .as_object()
        .cloned();
    inspect(driver, &routed, cluster, "cluster", proof).await;
}
