//! The opaque settings bags the Host stores on behalf of code it does not own:
//! `driver_settings` (drivers) and `mcp_client_servers` (MCP client config).
//!
//! The Host's contract is *opacity, not comprehension* — it must persist whatever
//! nested JSON a driver or extension put there without knowing what the keys
//! mean. The extension's own schema and behaviour are asserted in the extension
//! package, which is the only place that can see both the declaration and the
//! effect; a key here is a fixture, not a specification.

use super::super::*;

#[test]
fn intention_actions_roundtrips_in_extension_settings() {
    for enabled in [true, false] {
        let mut settings = AppSettings::default();
        settings.driver_settings.insert(
            "sql-editor-enhanced".into(),
            serde_json::json!({ "intentionActions": enabled, "insertValueHints": true }),
        );
        let parsed: AppSettings =
            serde_json::from_value(serde_json::to_value(settings).unwrap()).unwrap();
        assert_eq!(
            parsed.driver_settings["sql-editor-enhanced"]["intentionActions"],
            enabled
        );
        assert_eq!(
            parsed.driver_settings["sql-editor-enhanced"]["insertValueHints"],
            true
        );
    }
}

#[test]
fn driver_settings_defaults_when_key_missing() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value.as_object_mut().unwrap().remove("driverSettings");
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert!(parsed.driver_settings.is_empty());
}

#[test]
fn mcp_client_servers_defaults_when_key_missing() {
    let mut value = serde_json::to_value(AppSettings::default()).unwrap();
    value.as_object_mut().unwrap().remove("mcpClientServers");
    let parsed: AppSettings = serde_json::from_value(value).unwrap();
    assert!(parsed.mcp_client_servers.is_empty());
}

#[test]
fn driver_settings_roundtrip_opaque() {
    let settings = AppSettings {
        driver_settings: {
            let mut m = serde_json::Map::new();
            m.insert("redis".into(), serde_json::json!({ "allowFlush": true }));
            m
        },
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert!(json.contains("driverSettings"));
    let parsed: AppSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.driver_settings.get("redis").unwrap()["allowFlush"],
        true
    );
}
