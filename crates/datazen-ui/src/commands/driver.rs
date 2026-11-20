//! Port of `packages/frontend/src/commands/driver.ts` (T279) — the
//! generic driver-command IPC gateway (the driver-sdk
//! `ipc/driverCommands` thin re-export the frontend import path rides).
//!
//! Four methods over the bound transport, payloads byte-identical to
//! the previous direct invokes (§7.2's ruling): the two command-list
//! reads, the execute, and the stream execute (whose `Channel` push
//! callback is the host's event edge — the engine shapes the request;
//! the host wires the channel).
//!
//! `DriverCommandDefinition` is T254's single source (the hooks module
//! that first consumed it); the `execute` path is the adapter's own
//! [`DesktopAdapter::execute_driver_command`] — this module's
//! [`ExecuteDriverCommandRequest`] is the wire shape that call accepts.
//!
//! Known deltas: none behavioural — every command name and payload key
//! is the driver-sdk source's verbatim.

/// One shaped transport call (command name + payload).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DriverCommandRequest {
    pub(crate) command: &'static str,
    pub(crate) value: serde_json::Value,
}

impl DriverCommandRequest {
    fn native(command: &'static str, value: serde_json::Value) -> Self {
        Self { command, value }
    }
}

/// React `ExecuteDriverCommandRequest` — the execute face's bag (the
/// optional halves are the F1 database pin and the F7 schema).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ExecuteDriverCommand {
    /// Runtime db session id (required for session-bound commands).
    pub db_session_id: Option<String>,
    pub driver_type: Option<String>,
    pub command: String,
    pub input: serde_json::Value,
    /// F1: optional explicit database pin — the host switches the
    /// session before executing (session-bound commands only).
    pub database: Option<String>,
    /// F7: optional target schema (PG-family). Rewrite-capable drivers
    /// inline it as a qualified name; others ignore it.
    pub schema: Option<String>,
}

impl ExecuteDriverCommand {
    /// The `{ request }` wire bag `execute` sends.
    pub(crate) fn wire(&self) -> serde_json::Value {
        serde_json::json!({ "request": self })
    }
}

impl serde::Serialize for ExecuteDriverCommand {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        // JS drops `undefined` keys: the optional halves are absent, not
        // null, when never set.
        let mut map = serializer.serialize_map(None)?;
        if let Some(session) = &self.db_session_id {
            map.serialize_entry("dbSessionId", session)?;
        }
        if let Some(driver_type) = &self.driver_type {
            map.serialize_entry("driverType", driver_type)?;
        }
        map.serialize_entry("command", &self.command)?;
        map.serialize_entry("input", &self.input)?;
        if let Some(database) = &self.database {
            map.serialize_entry("database", database)?;
        }
        if let Some(schema) = &self.schema {
            map.serialize_entry("schema", schema)?;
        }
        map.end()
    }
}

/// `driverCommands.getConnectionCommands(dbSessionId)`.
pub(crate) fn get_connection_commands_request(db_session_id: &str) -> DriverCommandRequest {
    DriverCommandRequest::native(
        "get_connection_commands",
        serde_json::json!({ "dbSessionId": db_session_id }),
    )
}

/// `driverCommands.getDriverCommands(driverType)`.
pub(crate) fn get_driver_commands_request(driver_type: &str) -> DriverCommandRequest {
    DriverCommandRequest::native("get_driver_commands", serde_json::json!({ "driverType": driver_type }))
}

/// `driverCommands.execute(request)` → `execute_driver_command` with
/// the `{ request }` bag (the receipt is `{ data }`).
pub(crate) fn execute_request(request: &ExecuteDriverCommand) -> DriverCommandRequest {
    DriverCommandRequest::native("execute_driver_command", request.wire())
}

/// The stream face's caller options (the parts the channel edge does
/// not carry).
pub(crate) struct ExecuteDriverCommandStream<'a> {
    pub db_session_id: &'a str,
    pub command: &'a str,
    pub input: serde_json::Value,
    /// F1: optional explicit database pin, applied before streaming.
    pub database: Option<&'a str>,
    /// F7: optional target schema (PG-family).
    pub schema: Option<&'a str>,
    pub apply_result_limit: Option<bool>,
    pub record_history: Option<bool>,
}

/// `driverCommands.executeStream(request)` →
/// `execute_driver_command_stream`: the request bag normalizes the
/// optional database/schema to null (the stream face always sends the
/// keys), `applyResultLimit`/`recordHistory` ride beside the bag, and
/// `onEvent` is the host's channel edge (named in the payload).
pub(crate) fn execute_stream_request(stream: &ExecuteDriverCommandStream<'_>) -> DriverCommandRequest {
    DriverCommandRequest::native(
        "execute_driver_command_stream",
        serde_json::json!({
            "request": {
                "dbSessionId": stream.db_session_id,
                "command": stream.command,
                "input": stream.input,
                "database": stream.database,
                "schema": stream.schema,
            },
            "onEvent": "channel",
            "applyResultLimit": stream.apply_result_limit,
            "recordHistory": stream.record_history,
        }),
    )
}
