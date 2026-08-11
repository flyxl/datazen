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

/// Validate the generic structural part of a command's JSON Schema.
/// Driver-specific semantic validation remains inside the driver.
pub fn validate_command_input(
    definition: &DriverCommandDefinition,
    input: &JsonValue,
) -> Result<(), String> {
    if definition.input_schema.get("type").and_then(JsonValue::as_str) == Some("object")
        && !input.is_object()
    {
        return Err(format!("Command '{}' input must be an object", definition.id));
    }

    if let Some(required) = definition
        .input_schema
        .get("required")
        .and_then(JsonValue::as_array)
    {
        for field in required.iter().filter_map(JsonValue::as_str) {
            if input.get(field).is_none() {
                return Err(format!(
                    "Command '{}' input is missing required field '{}'",
                    definition.id, field
                ));
            }
        }
    }

    Ok(())
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
        permissions: vec!["driver.query".into()],
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
            "properties": { "sql": { "type": "string" } },
            "required": ["sql"]
        }),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "rowsAffected": { "type": "integer" } }
        })),
        permissions: vec!["driver.execute".into()],
    }
}

/// Access level required by a Driver Command, derived from its declared permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandAccessLevel {
    Read,
    Write,
    HighRisk,
}

const HIGH_RISK_TOKENS: &[&str] = &["flush", "drop", "restore", "exec", "truncate", "destroy"];
const WRITE_TOKENS: &[&str] = &[
    "execute", "write", "delete", "del", "set", "update", "insert", "add", "rename", "push",
    "pop", "remove", "ack", "create", "publish", "ttl", "reset", "xadd", "xack",
];

fn permission_tokens(definition: &DriverCommandDefinition) -> Vec<String> {
    let mut tokens = Vec::new();
    for value in definition
        .permissions
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(definition.id.as_str()))
    {
        tokens.extend(value.split(|c: char| !c.is_ascii_alphanumeric()).filter(|t| !t.is_empty()).map(|t| t.to_ascii_lowercase()));
    }
    tokens
}

/// Classify a command from its id and declared permission identifiers.
pub fn required_access_level(definition: &DriverCommandDefinition) -> CommandAccessLevel {
    let tokens = permission_tokens(definition);
    if tokens.iter().any(|t| HIGH_RISK_TOKENS.contains(&t.as_str())) {
        return CommandAccessLevel::HighRisk;
    }
    if tokens.iter().any(|t| WRITE_TOKENS.contains(&t.as_str())) {
        return CommandAccessLevel::Write;
    }
    if definition.permissions.is_empty() && definition.id != "query" {
        return CommandAccessLevel::Write;
    }
    CommandAccessLevel::Read
}

/// Reject commands that need a higher access level than the caller was granted.
pub fn check_command_access(
    definition: &DriverCommandDefinition,
    granted: CommandAccessLevel,
) -> Result<(), String> {
    let required = required_access_level(definition);
    if required > granted {
        return Err(format!(
            "Command '{}' is not allowed at the current permission level",
            definition.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_query_definition_is_schema_driven() {
        let definition = query_command_definition();
        assert_eq!(definition.id, "query");
        assert_eq!(definition.name, "Query");
        assert_eq!(definition.input_schema["type"], "object");
        assert_eq!(definition.input_schema["required"], serde_json::json!(["sql"]));
        assert_eq!(definition.input_schema["properties"]["sql"]["type"], "string");
        assert_eq!(definition.input_schema["properties"]["limit"]["minimum"], 1);
        assert!(definition.output_schema.is_none());
        assert_eq!(definition.permissions, vec!["driver.query"]);
    }

    #[test]
    fn standard_execute_definition_is_schema_driven() {
        let definition = execute_command_definition();
        assert_eq!(definition.id, "execute");
        assert_eq!(definition.name, "Execute");
        assert_eq!(definition.input_schema["required"], serde_json::json!(["sql"]));
        assert_eq!(definition.output_schema.as_ref().unwrap()["type"], "object");
        assert_eq!(definition.permissions, vec!["driver.execute"]);
    }

    #[test]
    fn command_input_validation_accepts_valid_input() {
        let definition = query_command_definition();
        assert!(validate_command_input(&definition, &serde_json::json!({"sql": "SELECT 1"})).is_ok());
    }

    #[test]
    fn command_input_validation_rejects_non_object() {
        let definition = query_command_definition();
        let error = validate_command_input(&definition, &serde_json::json!("SELECT 1")).unwrap_err();
        assert!(error.contains("must be an object"));
    }

    #[test]
    fn command_input_validation_rejects_missing_required_field() {
        let definition = query_command_definition();
        let error = validate_command_input(&definition, &serde_json::json!({})).unwrap_err();
        assert!(error.contains("missing required field 'sql'"));
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
        assert!(definition.permissions.is_empty());
    }

    #[test]
    fn command_result_preserves_arbitrary_json() {
        let payload = serde_json::json!({"rows": [{"id": 1}], "metadata": {"source": "mysql"}});
        assert_eq!(CommandResult::new(payload.clone()).data, payload);
    }

    #[test]
    fn command_result_round_trips_as_json() {
        let result = CommandResult::new(serde_json::json!({ "rowsAffected": 3 }));
        let encoded = serde_json::to_value(&result).unwrap();
        let decoded: CommandResult = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.data["rowsAffected"], 3);
    }

    #[test]
    fn query_is_read_and_execute_is_write() {
        assert_eq!(required_access_level(&query_command_definition()), CommandAccessLevel::Read);
        assert_eq!(required_access_level(&execute_command_definition()), CommandAccessLevel::Write);
    }

    #[test]
    fn redis_style_permissions_classify_risk() {
        let info = DriverCommandDefinition {
            id: "info".into(),
            name: "Info".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            permissions: vec!["redis:allow-info".into()],
        };
        let set = DriverCommandDefinition {
            id: "set_string".into(),
            name: "Set".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            permissions: vec!["redis:allow-set-string".into()],
        };
        let flush = DriverCommandDefinition {
            id: "flush_db".into(),
            name: "Flush".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            permissions: vec!["redis:allow-flush-db".into()],
        };
        assert_eq!(required_access_level(&info), CommandAccessLevel::Read);
        assert_eq!(required_access_level(&set), CommandAccessLevel::Write);
        assert_eq!(required_access_level(&flush), CommandAccessLevel::HighRisk);
        assert!(check_command_access(&flush, CommandAccessLevel::Write).is_err());
        assert!(check_command_access(&set, CommandAccessLevel::Write).is_ok());
        assert!(check_command_access(&info, CommandAccessLevel::Read).is_ok());
    }
}
