use datazen_driver_api::ConnectionConfig;
use datazen_driver_redis::set_settings_allow_flush;
use serde_json::json;

use super::Journey;

pub async fn authenticated(journey: &mut Journey<'_>, config: &ConnectionConfig, proof: &str) {
    let password = format!(">{proof}");
    let acl: String = journey
        .read(
            "ACL",
            &[
                "SETUSER",
                "read-fixture",
                "on",
                &password,
                "~contract:*",
                "+PING",
                "+SELECT",
                "+INFO",
                "+GET",
                "+TYPE",
                "+TTL",
                "+STRLEN",
                "+SCAN",
            ],
        )
        .await;
    assert_eq!(acl, "OK");

    let mut restricted = config.clone();
    restricted.username = Some("read-fixture".into());
    restricted.password = Some(proof.into());
    restricted.read_only = true;
    let driver = journey.driver;
    assert!(driver.test_connection(&restricted).await.is_ok());
    let handle = driver.connect(&restricted).await.unwrap();
    assert!(driver
        .get_databases(&handle)
        .await
        .unwrap()
        .contains(&"db0".into()));

    let read = driver.query(&handle, "GET contract:catalog").await.unwrap();
    assert_eq!(read.rows.len(), 1);
    let write = driver
        .execute_command(
            &handle,
            "set_string",
            json!({"key":"contract:catalog","value":"denied","keepTtl":false}),
        )
        .await;
    assert!(write.is_err());
    let untouched: String = journey.read("GET", &["contract:catalog"]).await;
    assert_eq!(untouched, "written");

    driver.disconnect(handle).await.unwrap();
    // Disable the application's read-only guard; the server's ACL must
    // still reject writes and denied commands without mutating the fixture.
    restricted.read_only = false;
    let handle = driver.connect(&restricted).await.unwrap();
    let dump = driver
        .execute_command(&handle, "dump_keys", json!({"keys":["contract:catalog"]}))
        .await
        .unwrap()
        .data;
    assert!(dump["errors"][0]["error"]
        .as_str()
        .unwrap()
        .contains("NOPERM"));
    assert_eq!(
        journey
            .read::<String>("ACL", &["SETUSER", "read-fixture", "+dump"])
            .await,
        "OK"
    );
    let expiry = driver
        .execute_command(&handle, "dump_keys", json!({"keys":["contract:catalog"]}))
        .await
        .unwrap()
        .data;
    assert_eq!(expiry["entries"], json!([]));
    assert!(expiry["errors"][0]["error"]
        .as_str()
        .unwrap()
        .contains("pttl"));
    let denied_commands = [
        ("modules_list", json!({})),
        ("slowlog_get", json!({"count":2})),
        ("slowlog_reset", json!({"confirm":true})),
        ("memory_usage_key", json!({"key":"contract:catalog"})),
        ("memory_sample", json!({})),
        ("type_distribution", json!({"sampleLimit":3})),
        ("stream_lag", json!({"key":"contract:stream","group":"g"})),
        (
            "xrange",
            json!({"key":"contract:stream","start":"-","end":"+"}),
        ),
        (
            "pubsub_publish",
            json!({"channel":"contract:access","message":"not sent"}),
        ),
    ];
    for (name, input) in denied_commands {
        let error = driver
            .execute_command(&handle, name, input)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("NOPERM"), "{name}: {error}");
    }
    assert_eq!(
        journey
            .read::<String>("ACL", &["SETUSER", "read-fixture", "+dbsize", "-scan"])
            .await,
        "OK"
    );
    for name in ["memory_sample", "type_distribution", "stream_overview"] {
        let error = driver
            .execute_command(&handle, name, json!({}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("NOPERM"), "{name}: {error}");
    }
    assert_eq!(
        journey
            .read::<String>("ACL", &["SETUSER", "read-fixture", "-dbsize", "+scan"])
            .await,
        "OK"
    );

    let monitor = driver
        .execute_command(&handle, "monitor_start", json!({}))
        .await
        .unwrap_err();
    assert!(monitor.to_string().contains("NOPERM"), "{monitor}");
    let raw = driver
        .execute_command(
            &handle,
            "get_key_raw",
            json!({
                "key":"contract:catalog","withMemory":true
            }),
        )
        .await
        .unwrap()
        .data;
    assert_eq!(raw["rawB64"], "d3JpdHRlbg==");
    assert_eq!(raw["logicalLen"], 7);
    assert!(
        raw["memBytes"].is_null(),
        "MEMORY was not allowed by the fixture ACL"
    );
    for (name, input) in [
        (
            "set_string",
            json!({"key":"contract:catalog","value":"never written"}),
        ),
        (
            "scan_values",
            json!({"pattern":"contract:catalog","query":"written","mode":"value"}),
        ),
    ] {
        let error = driver
            .execute_command(&handle, name, input)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("NOPERM"), "{name}: {error}");
    }
    let key_hits = driver
        .execute_command(
            &handle,
            "scan_values",
            json!({
                "pattern":"contract:catalog","query":"catalog","mode":"key"
            }),
        )
        .await
        .unwrap()
        .data;
    assert_eq!(key_hits["matched"][0]["key"], "contract:catalog");
    assert_eq!(
        journey
            .read::<String>("ACL", &["SETUSER", "read-fixture", "-type"])
            .await,
        "OK"
    );
    let type_denial = driver
        .execute_command(
            &handle,
            "scan_values",
            json!({
                "pattern":"contract:catalog","query":"written","mode":"value"
            }),
        )
        .await
        .unwrap_err();
    assert!(type_denial.to_string().contains("NOPERM"), "{type_denial}");
    assert_eq!(
        journey
            .read::<String>("ACL", &["SETUSER", "read-fixture", "+type", "-get"])
            .await,
        "OK"
    );
    // Metadata permission is not value permission, for either string or a
    // collection. A refusal must not be reported as an empty result.
    for key in [
        "contract:catalog",
        "contract:detail:hash",
        "contract:list",
        "contract:set",
        "contract:zset",
        "contract:stream",
    ] {
        let error = driver
            .execute_command(&handle, "get_key", json!({"key":key}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("NOPERM"), "{key}: {error}");
    }
    let raw = driver
        .execute_command(&handle, "get_key_raw", json!({"key":"contract:catalog"}))
        .await
        .unwrap_err();
    assert!(raw.to_string().contains("NOPERM"), "{raw}");
    assert_eq!(
        journey
            .read::<String>("ACL", &["SETUSER", "read-fixture", "+get", "-ttl"])
            .await,
        "OK"
    );
    for op in ["get_key", "get_key_raw"] {
        let error = driver
            .execute_command(&handle, op, json!({"key":"contract:catalog"}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("NOPERM"), "{op}: {error}");
    }
    assert_eq!(
        journey
            .read::<String>("ACL", &["SETUSER", "read-fixture", "+ttl", "-strlen"])
            .await,
        "OK"
    );
    let refused = driver
        .execute_command(&handle, "get_key_raw", json!({"key":"contract:catalog"}))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("NOPERM"), "{refused}");
    let value: String = journey.read("GET", &["contract:catalog"]).await;
    assert_eq!(value, "written");
    driver.disconnect(handle).await.unwrap();

    restricted.password = Some("invalid-fixture-password".into());
    assert!(driver.test_connection(&restricted).await.is_err());
    assert!(driver.connect(&restricted).await.is_err());
    let deleted: u64 = journey.read("ACL", &["DELUSER", "read-fixture"]).await;
    assert_eq!(deleted, 1);
}

pub async fn flush_guard(journey: &mut Journey<'_>, proof: &str) {
    let witness: Option<String> = journey.read("GET", &["contract:fixture"]).await;
    assert_eq!(
        witness.as_deref(),
        Some(proof),
        "Refuse to flush anything but this process's private test fixture."
    );
    set_settings_allow_flush(false);
    let command = json!({"dbIndex":3,"allowFlush":true});
    assert!(journey
        .driver
        .execute_command(journey.handle, "flush_db", command.clone())
        .await
        .is_err());
    set_settings_allow_flush(true);
    journey
        .command(
            "set_string",
            json!({"dbIndex":3,"key":"contract:shall-go","value":"fixture"}),
        )
        .await;
    assert!(journey
        .driver
        .execute_command(
            journey.handle,
            "flush_db",
            json!({"dbIndex":3,"allowFlush":false})
        )
        .await
        .is_err());
    journey.command("flush_db", command).await;
    let previous = journey
        .command("scan_keys", json!({"dbIndex":3,"pattern":"contract:*"}))
        .await;
    assert_eq!(previous["keys"], json!([]));
    assert_eq!(
        journey.read::<u64>("EXISTS", &["contract:fixture"]).await,
        1
    );
    journey
        .command("flush_all", json!({"allowFlush":true}))
        .await;
    assert_eq!(
        journey.read::<u64>("EXISTS", &["contract:fixture"]).await,
        0
    );
    set_settings_allow_flush(false);
}
