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
        assert_eq!(
            definition.description.as_deref(),
            Some("Execute a SQL query and return its result")
        );
        assert_eq!(definition.input_schema["type"], "object");
        assert_eq!(definition.input_schema["required"], serde_json::json!(["sql"]));
        assert_eq!(definition.input_schema["properties"]["sql"]["type"], "string");
        assert_eq!(
            definition.input_schema["properties"]["limit"]["type"],
            serde_json::json!(["integer", "null"])
        );
        assert_eq!(definition.input_schema["properties"]["limit"]["minimum"], 1);
        assert!(definition.output_schema.is_none());
        assert!(definition.permissions.is_empty());
    }

    #[test]
    fn standard_execute_definition_is_schema_driven() {
        let definition = execute_command_definition();

        assert_eq!(definition.id, "execute");
        assert_eq!(definition.name, "Execute");
        assert_eq!(
            definition.description.as_deref(),
            Some("Execute a SQL statement")
        );
        assert_eq!(definition.input_schema["type"], "object");
        assert_eq!(definition.input_schema["required"], serde_json::json!(["sql"]));
        assert_eq!(definition.input_schema["properties"]["sql"]["type"], "string");
        assert_eq!(definition.output_schema.as_ref().unwrap()["type"], "object");
        assert_eq!(
            definition.output_schema.as_ref().unwrap()["properties"]["rowsAffected"]["type"],
            "integer"
        );
        assert!(definition.permissions.is_empty());
    }

    #[test]
    fn command_definition_uses_camel_case_wire_names() {
        let definition = DriverCommandDefinition {
            id: "custom-command".into(),
            name: "Custom Command".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: Some(serde_json::json!({"type": "string"})),
            permissions: vec!["driver.custom.execute".into()],
        };

        let encoded = serde_json::to_value(&definition).unwrap();
        assert_eq!(encoded["inputSchema"]["type"], "object");
        assert_eq!(encoded["outputSchema"]["type"], "string");
        assert_eq!(encoded["permissions"], serde_json::json!(["driver.custom.execute"]));
        assert!(encoded.get("input_schema").is_none());
        assert!(encoded.get("output_schema").is_none());
    }

    #[test]
    fn command_definition_deserializes_without_permissions() {
        let encoded = serde_json::json!({
            "id": "legacy-command",
            "name": "Legacy Command",
            "description": null,
            "inputSchema": {"type": "object"},
            "outputSchema": null
        });

        let definition: DriverCommandDefinition = serde_json::from_value(encoded).unwrap();
        assert_eq!(definition.id, "legacy-command");
        assert!(definition.description.is_none());
        assert!(definition.output_schema.is_none());
        assert!(definition.permissions.is_empty());
    }

    #[test]
    fn command_result_preserves_arbitrary_json() {
        let payload = serde_json::json!({
            "rows": [
                {"id": 1, "name": "Alice"},
                {"id": 2, "name": null}
            ],
            "metadata": {"source": "mysql"}
        });
        let result = CommandResult::new(payload.clone());

        assert_eq!(result.data, payload);
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
