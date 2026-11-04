//! Link-time contract for the key-tree budget track (W3-B): `key_probe` plus
//! the `budget` parameter shared by `scan_keys` / `list_children` /
//! `count_matching`.
//!
//! Wave 4's tree UI reads these names straight from `command_definitions()`,
//! so the registration surface is asserted here — no live server needed:
//!
//! * all four commands appear in `command_definitions()` with an input schema
//!   that matches the parameters the dispatch arm actually parses (`budget`
//!   declared on the three scan commands, never required, clamped not
//!   rejected);
//! * `key_probe` classifies exactly like `get_key` (Observe / read / same
//!   permission), so the attribute probe sits behind the same gates;
//! * all four are reachable from `execute_command` — they fail with a
//!   *connection* error, not `Unsupported`, which is what distinguishes a wired
//!   arm from a missing one.
//!
//! Behaviour (budget ledger, exact-key short circuit, per-page batches, slot
//! addressing) is covered by the scripted connection double in
//! `src/ops/tree/scan/tests.rs`.

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
        id: "tree-budget-contract-test".into(),
        pool_id: "tree-budget-contract-test".into(),
    }
}

fn declares(def: &DriverCommandDefinition, field: &str) -> bool {
    def.input_schema
        .get("properties")
        .and_then(Value::as_object)
        .is_some_and(|props| props.contains_key(field))
}

fn required(def: &DriverCommandDefinition) -> Vec<String> {
    def.input_schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn key_probe_is_registered_like_get_key() {
    let probe = definition("key_probe");
    let get_key = definition("get_key");
    assert_eq!(
        probe.metadata.category,
        CommandCategory::Observe,
        "key_probe must not fall through to the Mutate default"
    );
    assert_eq!(
        required_access_level(&probe),
        CommandAccessLevel::Read,
        "an attribute probe never writes: it must stay behind the SafeMode write gate"
    );
    assert_eq!(
        probe.permissions, get_key.permissions,
        "the probe reads attributes only, so it rides get_key's permission exactly"
    );
    assert!(declares(&probe, "key"));
    assert!(declares(&probe, "dbIndex"));
    assert_eq!(
        required(&probe),
        ["key"],
        "req_str(…, \"key\") needs the schema to say so"
    );
    assert_eq!(
        validate_command_input(&probe, &json!({ "dbIndex": 1, "key": "a:b" })),
        Ok(())
    );
    assert!(
        validate_command_input(&probe, &json!({}))
            .is_err_and(|msg| msg.contains("required field 'key'")),
        "schema must flag the missing key the same way the driver does"
    );
}

#[test]
fn the_three_scan_commands_declare_budget_without_requiring_it() {
    for command in ["scan_keys", "list_children", "count_matching"] {
        let def = definition(command);
        assert!(
            declares(&def, "budget"),
            "{command} must declare the budget parameter its dispatch arm parses: {:?}",
            def.input_schema
        );
        assert_eq!(
            def.input_schema["properties"]["budget"]["type"], "integer",
            "the 10k/50k/200k/1M ladder is whole COUNT units, not a float"
        );
        assert!(
            !required(&def).iter().any(|field| field == "budget"),
            "{command}: an absent budget means 'derive it from DBSIZE', so it can never be required"
        );
        // The clamp is documented, not declared: an oversized request is clamped
        // into the hard cap, never rejected — asserting no `maximum` here keeps
        // that a driver behaviour instead of a schema validation error.
        assert!(def.input_schema["properties"]["budget"]["maximum"].is_null());
        let mut oversized = json!({ "budget": 1_000_000_000 });
        // …validated alongside each command's own required fields, so only the
        // budget itself could fail.
        match command {
            "list_children" => oversized["prefix"] = json!("app:"),
            "count_matching" => oversized["pattern"] = json!("app:*"),
            _ => {}
        }
        assert_eq!(
            validate_command_input(&def, &oversized),
            Ok(()),
            "an oversized budget must validate and then clamp in the ledger"
        );
    }

    // Pre-existing required fields keep their exact sets (append-only contract).
    assert!(required(&definition("scan_keys")).is_empty());
    assert_eq!(required(&definition("list_children")), ["prefix"]);
    assert_eq!(required(&definition("count_matching")), ["pattern"]);

    // And list_children gained withMemory alongside budget (dispatch parses it).
    assert!(declares(&definition("list_children"), "withMemory"));
}

#[test]
fn key_tree_commands_are_read_only_observations() {
    for command in ["scan_keys", "list_children", "count_matching", "key_probe"] {
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
async fn key_tree_commands_reach_the_dispatch_table() {
    let driver = RedisDriver::new();
    let inputs = vec![
        ("scan_keys", json!({ "dbIndex": 1, "pattern": "app:*" })),
        ("list_children", json!({ "dbIndex": 1, "prefix": "app:" })),
        (
            "count_matching",
            json!({ "dbIndex": 1, "pattern": "app:*", "budget": 50_000 }),
        ),
        ("key_probe", json!({ "dbIndex": 1, "key": "app:user:1" })),
    ];
    for (command, input) in inputs {
        let err = driver
            .execute_command(&handle(), command, input.clone())
            .await
            .expect_err("no connection is registered, so the command cannot succeed");
        assert!(
            matches!(err, DriverError::ConnectionFailed(_)),
            "{command} must reach its arm and fail on the connection, got {err:?}"
        );
    }

    // Contrast case: an id that is not in the table fails a different way.
    let unknown = driver
        .execute_command(&handle(), "no_such_tree_command", json!({}))
        .await
        .expect_err("unknown commands must not be silently accepted");
    assert!(
        matches!(unknown, DriverError::Unsupported(_)),
        "got {unknown:?}"
    );
}
