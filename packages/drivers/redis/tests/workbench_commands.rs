//! Link-time contract for the two Redis Workbench P0 commands
//! (`type_distribution`, `key_object_info`).
//!
//! Wave 2 writes its TypeScript types straight from these field names, so the
//! registration surface is asserted here rather than against a live server:
//!
//! * both commands appear in `command_definitions()` with an input schema that
//!   matches the parameters the dispatch arm actually parses,
//! * both classify as read-only observations, so neither is caught by the
//!   SafeMode write gate,
//! * both are reachable from `execute_command` — they fail with a *connection*
//!   error, not `Unsupported`, which is what distinguishes a wired arm from a
//!   missing one.
//!
//! Behaviour (sample clamping, `truncated`, per-field degradation) is covered by
//! the scripted `ConnectionLike` in `src/ops/workbench/tests.rs`.

use datazen_driver_api::{
    required_access_level, validate_command_input, CommandAccessLevel, CommandCategory,
    ConnectionHandle, DatabaseDriver, DriverCommandDefinition, DriverError,
};
use datazen_driver_redis::RedisDriver;
use serde_json::{json, Value};

fn definition(command: &str) -> DriverCommandDefinition {
    RedisDriver::new()
        .command_definitions()
        .into_iter()
        .find(|def| def.id == command)
        .unwrap_or_else(|| panic!("'{command}' must be registered in command_definitions()"))
}

fn handle() -> ConnectionHandle {
    ConnectionHandle {
        id: "workbench-contract-test".into(),
        pool_id: "workbench-contract-test".into(),
    }
}

fn declares(def: &DriverCommandDefinition, field: &str) -> bool {
    def.input_schema
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|props| props.contains_key(field))
}

#[test]
fn type_distribution_input_schema_matches_the_parsed_params() {
    let def = definition("type_distribution");
    // The dispatch arm reads `sampleLimit` (camelCase) and the shared prologue
    // reads `dbIndex`; both must be declared, and neither may be required.
    assert!(declares(&def, "sampleLimit"), "got {:?}", def.input_schema);
    assert!(declares(&def, "dbIndex"));
    assert_eq!(def.input_schema["required"], json!([]));
    assert_eq!(
        def.input_schema["properties"]["sampleLimit"]["type"],
        "integer"
    );
    // The clamp is documented instead of declared as `maximum`, so an oversized
    // window stays a clamp rather than a rejection.
    assert!(def.input_schema["properties"]["sampleLimit"]["maximum"].is_null());
    assert_eq!(validate_command_input(&def, &json!({})), Ok(()));
}

#[test]
fn key_object_info_input_schema_matches_the_parsed_params() {
    let def = definition("key_object_info");
    assert!(declares(&def, "key"));
    assert!(declares(&def, "dbIndex"));
    // `req_str(…, "key")` errors without it, so the schema has to say so.
    assert_eq!(def.input_schema["required"], json!(["key"]));
    assert_eq!(
        validate_command_input(&def, &json!({ "key": "a:b" })),
        Ok(())
    );
    assert!(
        validate_command_input(&def, &json!({}))
            .is_err_and(|msg| msg.contains("required field 'key'")),
        "schema must flag the missing key the same way the driver does"
    );
}

#[test]
fn workbench_commands_are_read_only_observations() {
    for command in ["type_distribution", "key_object_info"] {
        let def = definition(command);
        assert_eq!(
            def.metadata.category,
            CommandCategory::Observe,
            "{command} must not fall through to the Mutate default"
        );
        assert_eq!(
            required_access_level(&def),
            CommandAccessLevel::Read,
            "{command} is sampled, never written: it must stay behind the SafeMode write gate"
        );
        assert!(!def.permissions.is_empty(), "{command} needs a permission");
    }
}

#[tokio::test]
async fn workbench_commands_reach_the_dispatch_table() {
    let driver = RedisDriver::new();
    let inputs = vec![
        (
            "type_distribution",
            json!({ "dbIndex": 1, "sampleLimit": 50 }),
        ),
        (
            "key_object_info",
            json!({ "dbIndex": 1, "key": "app:user:1" }),
        ),
    ];
    for (command, input) in inputs {
        let err = driver
            .execute_command(&handle(), command, input)
            .await
            .expect_err("no connection is registered, so the command cannot succeed");
        assert!(
            matches!(err, DriverError::ConnectionFailed(_)),
            "{command} must reach its arm and fail on the connection, got {err:?}"
        );
    }

    // Contrast case: an id that is not in the table fails a different way.
    let unknown = driver
        .execute_command(&handle(), "no_such_probe", json!({}))
        .await
        .expect_err("unknown commands must not be silently accepted");
    assert!(
        matches!(unknown, DriverError::Unsupported(_)),
        "got {unknown:?}"
    );
}
