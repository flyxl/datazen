use std::time::Duration;

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, SslMode, Value};
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
    if topology == "cluster" {
        assert_eq!(driver.get_databases(&handle).await.unwrap(), ["db0"]);
        let counts = journey.command("db_sizes", json!({})).await;
        assert_eq!(
            counts,
            json!([{"db":0,"keys":journey.read::<u64>("DBSIZE", &[]).await}])
        );
        let invalid = driver
            .execute_command(
                &handle,
                "exec",
                json!({
                    "dbIndex":1,"commands":"PING","nodeAddr":addr,
                }),
            )
            .await
            .unwrap_err();
        assert!(
            invalid.to_string().contains("only supports DB 0"),
            "{invalid}"
        );
        let independent_id: String = journey.read("CLUSTER", &["MYID"]).await;
        let reported = journey.command("cluster_nodes", json!({})).await;
        assert!(
            reported["nodes"].as_array().unwrap().iter().any(|node| {
                node["id"] == independent_id && node["addr"] == addr && node["role"] == "master"
            }),
            "{reported}"
        );
    }

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
    if topology == "sentinel" {
        let mut previous = None;
        for operation in ["info", "get_key", "get_key_raw", "key_probe"] {
            let id = driver.query(&handle, "CLIENT ID").await.unwrap();
            let Some(Value::Integer(id)) = id.rows[0][0] else {
                panic!("Redis did not return a connection ID: {id:?}");
            };
            assert_ne!(previous, Some(id));
            previous = Some(id);
            // Close only this disposable driver's socket; leave the other test
            // fixtures and the independent oracle connected.
            assert_eq!(
                journey
                    .read::<u64>("CLIENT", &["KILL", "ID", &id.to_string()])
                    .await,
                1,
            );
            let result = journey
                .command(
                    operation,
                    json!({"section":"server","key":"contract:topology"}),
                )
                .await;
            match operation {
                "info" => assert!(result.as_str().unwrap().contains("redis_version:")),
                "get_key" => assert_eq!(result["value"]["value"], "sentinel"),
                "get_key_raw" => assert_eq!(result["rawB64"], "c2VudGluZWw="),
                "key_probe" => assert_eq!(result["exists"], true),
                _ => unreachable!(),
            }
        }
    }
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
    plaintext_negotiation(driver, &routed, master_port, "sentinel").await;
    if let (Ok(auth_port), Ok(password)) = (
        std::env::var("REDIS_E2E_AUTH_SENTINEL_PORT"),
        std::env::var("REDIS_E2E_AUTH_SENTINEL_PASSWORD"),
    ) {
        authenticated_sentinel(driver, &routed, &auth_port, &password).await;
    }

    let mut routed = base.clone();
    routed.port = Some(cluster);
    routed.options = json!({"topology":"cluster","clusterNodes":[format!("127.0.0.1:{cluster}")]})
        .as_object()
        .cloned();
    inspect(driver, &routed, cluster, "cluster", proof).await;
    plaintext_negotiation(driver, &routed, cluster, "cluster").await;
    metadata_acl(driver, &routed, cluster, proof).await;
}

async fn plaintext_negotiation(
    driver: &dyn DatabaseDriver,
    config: &ConnectionConfig,
    port: u16,
    topology: &str,
) {
    let mut config = config.clone();
    config.connection_timeout = 2;
    config.ssl_mode = SslMode::Require;
    assert!(
        driver.test_connection(&config).await.is_err(),
        "{topology}: TLS Require reached a plaintext fixture"
    );
    config.ssl_mode = SslMode::Prefer;
    let handle = driver
        .connect(&config)
        .await
        .unwrap_or_else(|error| panic!("{topology} TLS preference fallback: {error}"));
    let client = redis::Client::open(format!("redis://127.0.0.1:{port}/0")).unwrap();
    let mut j = Journey {
        driver,
        handle: &handle,
        oracle: client.get_multiplexed_async_connection().await.unwrap(),
    };
    assert!(j
        .command("info", json!({"section":"server"}))
        .await
        .is_string());
    let channel = format!("contract:prefer:{topology}");
    let sub = j
        .command("pubsub_subscribe", json!({"channels":[channel]}))
        .await;
    assert_eq!(j.read::<u64>("PUBLISH", &[&channel, "negotiated"]).await, 1);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let stats = j.command("pubsub_stats", json!({})).await;
            if stats["byChannel"][&channel] == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{topology}: subscriber did not receive the real event"));
    j.command("pubsub_unsubscribe", json!({"subscriptionId":sub}))
        .await;
    let monitor = j.command("monitor_start", json!({})).await;
    let id = monitor["monitorId"].as_str().unwrap();
    let marker = format!("contract:monitor:prefer:{topology}");
    let _: Option<String> = j.read("GET", &[&marker]).await;
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let events = j
                .command("monitor_get_buffer", json!({"monitorId":id}))
                .await;
            if events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["args"][0] == marker)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("MONITOR must reconnect using the same negotiated transport");
    j.command("monitor_stop", json!({"monitorId":id})).await;
    driver.disconnect(handle).await.unwrap();
}

async fn metadata_acl(
    driver: &dyn DatabaseDriver,
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
    let user = "native-metadata";
    let password = format!(">{proof}");
    let created: String = redis::cmd("ACL")
        .arg(&[
            "SETUSER",
            user,
            "on",
            &password,
            "~contract:*",
            "+@all",
            "-memory",
            "-pttl",
            "-object",
            "-exists",
            "-del",
            "-expire",
        ])
        .query_async(&mut oracle)
        .await
        .unwrap();
    assert_eq!(created, "OK");
    let mut config = config.clone();
    config.username = Some(user.into());
    config.password = Some(proof.into());
    let handle = driver.connect(&config).await.unwrap();
    let mut j = Journey {
        driver,
        handle: &handle,
        oracle,
    };

    let key = "contract:topology";
    let probe = j.command("key_probe", json!({"key":key})).await;
    assert_eq!(
        probe,
        json!({"exists":true,"type":"string","ttlMs":-1,"memoryBytes":null})
    );
    let missing = j
        .command("key_probe", json!({"key":"contract:metadata-absent"}))
        .await;
    assert_eq!(
        missing,
        json!({"exists":false,"type":null,"ttlMs":-2,"memoryBytes":null})
    );
    let children = j
        .command(
            "list_children",
            json!({"prefix":"contract:topology","withMemory":true}),
        )
        .await;
    assert_eq!(children["children"][0]["key"], key);
    assert_eq!(children["children"][0]["logicalLen"], "cluster".len());
    assert!(children["children"][0]["memBytes"].is_null());
    let deletion = j
        .command("batch_delete_pattern", json!({"pattern":key}))
        .await;
    assert_eq!(deletion["deleted"], 0);
    assert_eq!(deletion["errors"][0]["key"], key);
    assert!(deletion["errors"][0]["error"]
        .as_str()
        .unwrap()
        .contains("NOPERM"));
    let expiry = j
        .command("batch_set_ttl", json!({"keys":[key],"ttlSeconds":60}))
        .await;
    assert_eq!(expiry["updated"], 0);
    assert!(expiry["errors"][0]["error"]
        .as_str()
        .unwrap()
        .contains("NOPERM"));
    assert_eq!(j.read::<String>("GET", &[key]).await, "cluster");
    assert_eq!(j.read::<i64>("PTTL", &[key]).await, -1);
    let sample = j.command("memory_sample", json!({"limit":1})).await;
    assert_eq!(sample["samples"].as_array().unwrap().len(), 1);
    assert_eq!(sample["samples"][0]["bytes"], 0);
    assert!(sample["samples"][0]["ttlMs"].is_null());
    assert_eq!(sample["truncated"], true);

    assert_eq!(
        j.read::<String>("ACL", &["SETUSER", user, "+exists", "-type"])
            .await,
        "OK"
    );
    let probe = j.command("key_probe", json!({"key":key})).await;
    assert_eq!(probe["exists"], true);
    assert!(probe["type"].is_null());
    let details = j.command("key_object_info", json!({"key":key})).await;
    assert_eq!(details["missing"], false);
    assert_eq!(details["ttlMs"], -1);
    assert!(details["type"].is_null());
    let distribution = j
        .command("type_distribution", json!({"sampleLimit":1}))
        .await;
    assert_eq!(distribution["sampled"], 0);
    assert_eq!(distribution["truncated"], true);
    assert_eq!(
        j.read::<String>("ACL", &["SETUSER", user, "-exists"]).await,
        "OK"
    );
    let error = driver
        .execute_command(&handle, "key_probe", json!({"key":key}))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("redis_key_probe_unreadable"),
        "{error}"
    );
    assert_eq!(j.read::<u64>("ACL", &["DELUSER", user]).await, 1);
    driver.disconnect(handle).await.unwrap();
}

async fn authenticated_sentinel(
    driver: &dyn DatabaseDriver,
    config: &ConnectionConfig,
    auth_port: &str,
    password: &str,
) {
    let mut test = config.clone();
    test.options.as_mut().unwrap().insert(
        "sentinelNodes".into(),
        json!([format!("127.0.0.1:{auth_port}")]),
    );
    assert!(
        driver.test_connection(&test).await.is_err(),
        "unauthenticated access to protected fixture must fail"
    );
    test.options
        .as_mut()
        .unwrap()
        .insert("sentinelNodePassword".into(), json!(password));
    let info = driver.test_connection(&test).await.unwrap();
    assert!(!info.server_version.is_empty());
    let handle = driver.connect(&test).await.unwrap();
    let result = driver
        .query(&handle, "GET contract:topology")
        .await
        .unwrap();
    assert_eq!(result.rows[0][0], Some(Value::String("sentinel".into())));
    driver.disconnect(handle).await.unwrap();
}
