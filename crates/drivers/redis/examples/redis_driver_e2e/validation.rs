use datazen_driver_api::DriverError;
use serde_json::json;

use super::Journey;

pub async fn run(journey: &mut Journey<'_>) {
    for (command, payload, detail) in [
        (
            "set_string",
            json!({"dbIndex":4294967296_u64,"key":"contract:catalog","value":"would-wrap"}),
            "dbIndex",
        ),
        (
            "set_string",
            json!({"dbIndex":-1,"key":"contract:catalog","value":"would-wrap"}),
            "dbIndex",
        ),
        (
            "set_string",
            json!({"key":"contract:catalog","value":null}),
            "'value'",
        ),
        (
            "set_string_raw",
            json!({"key":"contract:catalog","data_b64":"not base64!"}),
            "invalid base64",
        ),
        (
            "list_set",
            json!({"key":"contract:list","expectedValue":"x","value":"y"}),
            "integer 'index'",
        ),
        (
            "set_add",
            json!({"key":"contract:set","members":"not an array"}),
            "'members'",
        ),
        (
            "zset_add",
            json!({"key":"contract:zset","members":[{"member":"x","score":true}]}),
            "boolean",
        ),
        ("rename", json!({"key":"contract:catalog"}), "'newKey'"),
        (
            "set_ttl",
            json!({"key":"contract:catalog"}),
            "'ttlSeconds' or 'expireAt'",
        ),
        (
            "batch_set_ttl",
            json!({"keys":["contract:catalog"]}),
            "'ttlSeconds'",
        ),
        (
            "batch_rename_prefix",
            json!({"newPrefix":"contract:new:"}),
            "'oldPrefix'",
        ),
        (
            "batch_rename_prefix",
            json!({"oldPrefix":"contract:old:"}),
            "'newPrefix'",
        ),
        ("flush_db", json!({"allowFlush":"true"}), "boolean"),
        ("pubsub_unsubscribe", json!({}), "'subscriptionId'"),
        (
            "restore_keys",
            json!({"entries":"not an array"}),
            "sequence",
        ),
    ] {
        let error = journey
            .driver
            .execute_command(journey.handle, command, payload)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, DriverError::InvalidConfig(reason) if reason.contains(detail)),
            "{command}: {error}"
        );
    }

    // Confirm failure cases did not change or remove the original server-side key.
    assert_eq!(
        journey.read::<String>("GET", &["contract:catalog"]).await,
        "written"
    );
    let raw = journey
        .command(
            "get_key_raw",
            json!({"db_index":0,"key":"contract:catalog","with_memory":true}),
        )
        .await;
    assert_eq!(raw["keyType"], "string");
    assert!(raw["memoryBytes"].is_number() || raw["memBytes"].is_number());
    for name in ["get_key", "get_key_raw"] {
        let missing = "contract:validation:missing";
        let error = journey
            .driver
            .execute_command(journey.handle, name, json!({"key":missing}))
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("key not found: {missing}")),
            "{error}"
        );
        let denied = journey
            .driver
            .execute_command(
                journey.handle,
                name,
                json!({"key":"contract:catalog","dbIndex":99_999}),
            )
            .await
            .unwrap_err();
        assert!(
            denied.to_string().contains("DB index is out of range"),
            "{denied}"
        );
    }
    let empty = journey
        .command(
            "list_index",
            json!({"key":"contract:does-not-exist","index":0}),
        )
        .await;
    assert_eq!(empty, serde_json::Value::Null);
}
