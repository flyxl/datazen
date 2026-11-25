//! T279 audit: the driver command gateway, verified against the
//! driver-sdk source re-read line by line (commands/driver.ts is the
//! thin re-export; the payload shapes below are re-derived from
//! `ipc/driverCommands.ts`).
//!
//! Tester-written: the coder's module is treated as a black box.

use serde_json::json;

use datazen_ui::commands::driver::{
    execute_request, execute_stream_request, get_connection_commands_request,
    get_driver_commands_request, DriverCommandRequest, ExecuteDriverCommand,
    ExecuteDriverCommandStream,
};

fn req(command: &'static str, value: serde_json::Value) -> DriverCommandRequest {
    DriverCommandRequest { command, value }
}

#[test]
fn the_list_faces_carry_their_single_keys() {
    assert_eq!(
        get_connection_commands_request("sess-1"),
        req("get_connection_commands", json!({ "dbSessionId": "sess-1" }))
    );
    assert_eq!(
        get_driver_commands_request("sqlite"),
        req("get_driver_commands", json!({ "driverType": "sqlite" }))
    );
}

/// `execute` wraps the request bag; JS's undefined-dropping means the
/// optional halves are ABSENT when never set (not null).
#[test]
fn execute_wraps_the_bag_and_drops_unset_optionals() {
    // The minimal request: command + input only.
    let request = ExecuteDriverCommand {
        command: "list_tables".to_owned(),
        input: json!({"schema": "public"}),
        ..ExecuteDriverCommand::default()
    };
    assert_eq!(
        execute_request(&request),
        req(
            "execute_driver_command",
            json!({ "request": { "command": "list_tables", "input": {"schema": "public"} } })
        )
    );

    // The full request: every optional key present.
    let request = ExecuteDriverCommand {
        db_session_id: Some("sess-1".to_owned()),
        driver_type: Some("postgresql".to_owned()),
        command: "list_tables".to_owned(),
        input: json!({}),
        database: Some("shop".to_owned()),
        schema: Some("public".to_owned()),
    };
    assert_eq!(
        execute_request(&request),
        req(
            "execute_driver_command",
            json!({ "request": {
                "dbSessionId": "sess-1", "driverType": "postgresql",
                "command": "list_tables", "input": {},
                "database": "shop", "schema": "public",
            } })
        )
    );
}

/// `executeStream` normalizes the optional database/schema to NULL (the
/// stream face always sends the keys) and rides the two stream flags
/// beside the bag.
#[test]
fn the_stream_face_normalizes_and_carries_the_flags() {
    let stream = ExecuteDriverCommandStream {
        db_session_id: "sess-1",
        command: "query",
        input: json!({"sql": "SELECT 1"}),
        database: None,
        schema: None,
        apply_result_limit: Some(false),
        record_history: Some(false),
    };
    assert_eq!(
        execute_stream_request(&stream),
        req(
            "execute_driver_command_stream",
            json!({
                "request": {
                    "dbSessionId": "sess-1", "command": "query",
                    "input": {"sql": "SELECT 1"},
                    "database": null, "schema": null,
                },
                "onEvent": "channel",
                "applyResultLimit": false,
                "recordHistory": false,
            })
        )
    );

    // The explicit halves pass through; absent flags stay null.
    let stream = ExecuteDriverCommandStream {
        db_session_id: "sess-2",
        command: "query",
        input: json!({}),
        database: Some("shop"),
        schema: Some("public"),
        apply_result_limit: None,
        record_history: None,
    };
    let request = execute_stream_request(&stream);
    assert_eq!(request.value["request"]["database"], "shop");
    assert_eq!(request.value["request"]["schema"], "public");
    assert!(request.value["applyResultLimit"].is_null());
    assert!(request.value["recordHistory"].is_null());
}
