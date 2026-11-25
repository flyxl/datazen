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
