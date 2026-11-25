use datazen_driver_api::{ConnectionConfig, DriverError, SslMode};
use serde_json::json;

use super::Journey;

pub async fn run(journey: &mut Journey<'_>, base: &ConnectionConfig) {
    let driver = journey.driver;
    for topology in [
        json!({"topology":"sentinel","sentinelNodes":["127.0.0.1:1"]}),
        json!({"topology":"sentinel","sentinelMasterName":"fixture"}),
        json!({"topology":"cluster","clusterNodes":["malformed:port"]}),
    ] {
        let mut config = base.clone();
        config.options = topology.as_object().cloned();
        assert!(matches!(
            driver.connect(&config).await,
            Err(DriverError::InvalidConfig(_))
        ));
    }
    let mut invalid_db = base.clone();
    invalid_db.database = Some("not-a-logical-db".into());
    assert!(matches!(
        driver.connect(&invalid_db).await,
        Err(DriverError::InvalidConfig(_))
    ));

    let mut tls = base.clone();
    tls.connection_timeout = 1;
    tls.ssl_mode = SslMode::Require;
    assert!(
        driver.connect(&tls).await.is_err(),
        "Require must not accept a plaintext server"
    );

    // This disposable server has no TLS listener. Prefer, unlike Require,
    // may negotiate plaintext when its bounded TLS connection probe fails.
    tls.ssl_mode = SslMode::Prefer;
    assert!(driver.test_connection(&tls).await.is_ok());
    let handle = driver.connect(&tls).await.expect("allowed TLS fallback");
    assert!(driver.get_server_info(&handle).await.is_ok());
    let subscription = driver
        .execute_command(
            &handle,
            "pubsub_subscribe",
            json!({"channels":["contract:prefer"]}),
        )
        .await
        .unwrap()
        .data;
    let id = subscription.as_str().unwrap();
    let count: u64 = journey
        .read("PUBLISH", &["contract:prefer", "fixture-message"])
        .await;
    assert_eq!(count, 1);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let stats = driver
                .execute_command(&handle, "pubsub_stats", json!({}))
                .await
                .unwrap()
                .data;
            if stats["totalMessages"] == 1 {
                assert_eq!(stats["byChannel"]["contract:prefer"], 1);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("receive the fixture message");
    driver
        .execute_command(&handle, "pubsub_unsubscribe", json!({"subscriptionId":id}))
        .await
        .unwrap();
    driver.disconnect(handle).await.unwrap();
}

pub async fn legacy(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    config: &ConnectionConfig,
    port: u16,
    proof: &str,
) {
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/0")).unwrap();
    let mut oracle = client.get_multiplexed_async_connection().await.unwrap();
    let witness: String = redis::cmd("GET")
        .arg("contract:fixture")
        .query_async(&mut oracle)
        .await
        .unwrap();
    assert_eq!(witness, proof);
    let mut config = config.clone();
    config.port = Some(port);
    let info = driver.test_connection(&config).await.unwrap();
    assert!(
        info.server_version.starts_with("5."),
        "Expected an isolated Redis 5 fixture: {info:?}"
    );
    let handle = driver.connect(&config).await.unwrap();
    let mut j = Journey {
        driver,
        handle: &handle,
        oracle,
    };
    let expiring = "contract:legacy:expiring";
    assert_eq!(
        j.read::<String>("SET", &[expiring, "before", "PX", "120000"])
            .await,
        "OK"
    );
    let before = j.read::<i64>("PTTL", &[expiring]).await;
    let updated = j
        .command("set_string", json!({"key":expiring,"value":"after"}))
        .await;
    assert_eq!(
        updated,
        json!({"ok":true,"keepTtl":true,"keepTtlFallback":true})
    );
    assert_eq!(j.read::<String>("GET", &[expiring]).await, "after");
    let after = j.read::<i64>("PTTL", &[expiring]).await;
    assert!((1..=before).contains(&after), "{before} -> {after}");
    let persistent = "contract:legacy:persistent";
    let written = j
        .command("set_string_raw", json!({"key":persistent,"dataB64":"AP8="}))
        .await;
    assert_eq!(written["keepTtlFallback"], true);
    assert_eq!(j.read::<i64>("PTTL", &[persistent]).await, -1);
    assert_eq!(j.read::<Vec<u8>>("GET", &[persistent]).await, [0, 255]);
    driver.disconnect(handle).await.unwrap();
}
