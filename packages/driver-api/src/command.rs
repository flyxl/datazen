//! Generic Driver Command API.
//!
//! Commands are the extension point for driver-specific operations. The host
//! uses [`DriverCommandDefinition`] for discovery and [`CommandResult`] as the
//! transport-neutral execution result.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Metadata exposed by a driver for a command it supports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCommandDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema describing the command input object.
    pub input_schema: JsonValue,
    pub output_schema: Option<JsonValue>,
    /// Existing DataZen permission identifiers required by this command.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Transport-neutral result returned by a Driver Command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub data: JsonValue,
}

impl CommandResult {
    pub fn new(data: JsonValue) -> Self {
        Self { data }
    }
}

/// Build the standard SQL `query` command definition.
pub fn query_command_definition() -> DriverCommandDefinition {
    DriverCommandDefinition {
        id: "query".into(),
        name: "Query".into(),
        description: Some("Execute a SQL query and return its result".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "sql": { "type": "string" },
                "limit": { "type": ["integer", "null"], "minimum": 1 }
            },
            "required": ["sql"]
        }),
        output_schema: None,
        permissions: Vec::new(),
    }
}

/// Build the standard SQL `execute` command definition.
pub fn execute_command_definition() -> DriverCommandDefinition {
    DriverCommandDefinition {
        id: "execute".into(),
        name: "Execute".into(),
        description: Some("Execute a SQL statement".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "sql": { "type": "string" }
            },
            "required": ["sql"]
        }),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "rowsAffected": { "type": "integer" } }
        })),
        permissions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_query_definition_is_schema_driven() {
        let definition = query_command_definition();
        assert_eq!(definition.id, "query");
        assert_eq!(definition.name, "Query");
        assert_eq!(definition.input_schema["required"], serde_json::json!(["sql"]));
        assert_eq!(definition.input_schema["properties"]["sql"]["type"], "string");
    }

    #[test]
    fn standard_execute_definition_is_schema_driven() {
        let definition = execute_command_definition();
        assert_eq!(definition.id, "execute");
        assert_eq!(definition.input_schema["required"], serde_json::json!(["sql"]));
        assert_eq!(definition.output_schema.as_ref().unwrap()["type"], "object");
    }

    #[test]
    fn command_result_round_trips_as_json() {
        let result = CommandResult::new(serde_json::json!({ "rowsAffected": 3 }));
        let encoded = serde_json::to_value(&result).unwrap();
        assert_eq!(encoded["data"]["rowsAffected"], 3);

        let decoded: CommandResult = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.data["rowsAffected"], 3);
    }
}
