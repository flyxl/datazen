//! Generic Driver Command API.
//!
//! Commands are the extension point for driver-specific operations. The host
//! uses [`DriverCommandDefinition`] for discovery and [`CommandResult`] as the
//! transport-neutral execution result.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

fn default_true() -> bool {
    true
}

/// Access level required by a Driver Command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandAccessLevel {
    Read,
    Write,
    HighRisk,
}

/// High-level grouping used by Workflow UI and docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CommandCategory {
    #[default]
    Query,
    Mutate,
    Admin,
    Observe,
    PubSub,
    Stream,
    Io,
}

/// Declarative native save dialog attached to a command (host thin shell).
///
/// Drivers cannot pop OS dialogs themselves: instead a command returns its
/// payload as `{ <fileNameField>: string, <dataBase64Field>: base64-bytes }`
/// and declares this spec. When the host executes the command through an
/// interactive `execute_driver_command` call it — generically, without any
/// driver-specific branch — shows a native save dialog seeded by
/// `fileNameField`, writes the decoded bytes to the picked location and
/// replaces the result data with `{ <resultPathField>: savedPath | null }`
/// (`null` when the user cancelled).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverSaveDialogSpec {
    /// Key in the command result data holding the suggested file name.
    pub file_name_field: String,
    /// Key in the command result data holding base64-encoded file bytes.
    pub data_base64_field: String,
    /// Dialog filter label, e.g. "SQLite Database".
    pub filter_name: String,
    /// Allowed/advertised extensions (without dot), e.g. ["db", "sqlite"].
    pub extensions: Vec<String>,
    /// Key written back into the result data with the saved absolute path.
    pub result_path_field: String,
}

/// Optional command metadata. Missing fields deserialize to workflow/UI-enabled defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCommandMetadata {
    #[serde(default)]
    pub category: CommandCategory,
    /// Explicit risk. When omitted, derived from permissions and command id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<CommandAccessLevel>,
    #[serde(default = "default_true")]
    pub workflow: bool,
    #[serde(default = "default_true")]
    pub ui: bool,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
    /// When false, the host may execute this command without a live Connection.
    #[serde(default = "default_true")]
    pub requires_connection: bool,
    /// When set, the host thin shell saves the command's returned bytes through
    /// a native save dialog (see [`DriverSaveDialogSpec`]). Commands declaring
    /// this are interactive-only: MCP / workflow / headless callers are
    /// rejected before execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_dialog: Option<DriverSaveDialogSpec>,
}

impl Default for DriverCommandMetadata {
    fn default() -> Self {
        Self {
            category: CommandCategory::Query,
            risk: None,
            workflow: true,
            ui: true,
            deprecated: false,
            replaced_by: None,
            requires_connection: true,
            save_dialog: None,
        }
    }
}

impl DriverCommandMetadata {
    pub fn new(category: CommandCategory, risk: CommandAccessLevel) -> Self {
        Self {
            category,
            risk: Some(risk),
            ..Self::default()
        }
    }

    pub fn hide_from_workflow(mut self) -> Self {
        self.workflow = false;
        self
    }

    pub fn unbound(mut self) -> Self {
        self.requires_connection = false;
        self
    }

    /// Declare that the host must save this command's byte payload through a
    /// native save dialog (generic mechanism — see [`DriverSaveDialogSpec`]).
    pub fn save_dialog(mut self, spec: DriverSaveDialogSpec) -> Self {
        self.save_dialog = Some(spec);
        self
    }
}

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
    #[serde(default)]
    pub metadata: DriverCommandMetadata,
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
    if definition
        .input_schema
        .get("type")
        .and_then(JsonValue::as_str)
        == Some("object")
        && !input.is_object()
    {
        return Err(format!(
            "Command '{}' input must be an object",
            definition.id
        ));
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

/// Optional F7 targeting fields injected by the host into SQL command inputs
/// (serde-default on the wire: absent fields keep legacy behavior).
fn sql_target_input_schema_properties() -> serde_json::Map<String, JsonValue> {
    serde_json::json!({
        "database": {
            "description": "Optional target database (F7): drivers that support dialect-aware qualification rewrite unqualified table references; others ignore it",
            "type": ["string", "null"]
        },
        "schema": {
            "description": "Optional target schema (F7, PG-family engines): inlined as qualified name when the driver supports it",
            "type": ["string", "null"]
        }
    })
    .as_object()
    .expect("static json object")
    .clone()
}

/// Build the standard SQL `query_stream` command definition.
pub fn query_stream_command_definition() -> DriverCommandDefinition {
    let mut properties = sql_target_input_schema_properties();
    properties.insert(
        "sql".into(),
        serde_json::json!({ "type": "string" }),
    );
    properties.insert(
        "limit".into(),
        serde_json::json!({ "type": ["integer", "null"], "minimum": 1 }),
    );
    properties.insert(
        "params".into(),
        serde_json::json!({
            "description": "Named or positional bind values substituted by the host before execution",
            "type": ["object", "array", "null"]
        }),
    );
    DriverCommandDefinition {
        id: "query_stream".into(),
        name: "Query Stream".into(),
        description: Some("Execute a SQL query and stream result rows in batches".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": ["sql"]
        }),
        output_schema: None,
        permissions: vec!["driver.query".into()],
        metadata: DriverCommandMetadata::new(CommandCategory::Stream, CommandAccessLevel::Read),
    }
}

/// Build the standard SQL `query` command definition.
pub fn query_command_definition() -> DriverCommandDefinition {
    let mut properties = sql_target_input_schema_properties();
    properties.insert(
        "sql".into(),
        serde_json::json!({ "type": "string" }),
    );
    properties.insert(
        "limit".into(),
        serde_json::json!({ "type": ["integer", "null"], "minimum": 1 }),
    );
    properties.insert(
        "params".into(),
        serde_json::json!({
            "description": "Named or positional bind values substituted by the host before execution",
            "type": ["object", "array", "null"]
        }),
    );
    DriverCommandDefinition {
        id: "query".into(),
        name: "Query".into(),
        description: Some("Execute a SQL query and return its result".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": ["sql"]
        }),
        output_schema: None,
        permissions: vec!["driver.query".into()],
        metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read),
    }
}

/// Build the standard SQL `execute` command definition.
pub fn execute_command_definition() -> DriverCommandDefinition {
    let mut properties = sql_target_input_schema_properties();
    properties.insert(
        "sql".into(),
        serde_json::json!({ "type": "string" }),
    );
    DriverCommandDefinition {
        id: "execute".into(),
        name: "Execute".into(),
        description: Some("Execute a SQL statement".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": ["sql"]
        }),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "rowsAffected": { "type": "integer" } }
        })),
        permissions: vec!["driver.execute".into()],
        metadata: DriverCommandMetadata::new(CommandCategory::Mutate, CommandAccessLevel::Write),
    }
}

fn set_sql_title(mut definition: DriverCommandDefinition, title: &str) -> DriverCommandDefinition {
    if let Some(sql) = definition
        .input_schema
        .get_mut("properties")
        .and_then(|properties| properties.get_mut("sql"))
    {
        sql["title"] = JsonValue::String(title.into());
        sql["description"] = JsonValue::String(title.into());
    }
    definition
}

/// `query` with a driver-specific description and `sql` field title.
pub fn query_command_definition_for(
    description: &str,
    statement_title: &str,
) -> DriverCommandDefinition {
    let mut definition = query_command_definition();
    definition.description = Some(description.into());
    set_sql_title(definition, statement_title)
}

/// `execute` with a driver-specific description and `sql` field title.
pub fn execute_command_definition_for(
    description: &str,
    statement_title: &str,
) -> DriverCommandDefinition {
    let mut definition = execute_command_definition();
    definition.description = Some(description.into());
    set_sql_title(definition, statement_title)
}

/// Query + execute for non-SQL drivers that still use the `sql` input field.
pub fn statement_command_definitions(
    query_description: &str,
    execute_description: &str,
    statement_title: &str,
) -> Vec<DriverCommandDefinition> {
    vec![
        query_command_definition_for(query_description, statement_title),
        execute_command_definition_for(execute_description, statement_title),
    ]
}

/// Query-only drivers (writes are not exposed as commands).
pub fn query_only_command_definitions(
    description: &str,
    statement_title: &str,
) -> Vec<DriverCommandDefinition> {
    vec![query_command_definition_for(description, statement_title)]
}

const HIGH_RISK_TOKENS: &[&str] = &["flush", "drop", "restore", "exec", "truncate", "destroy"];
const WRITE_TOKENS: &[&str] = &[
    "execute", "write", "delete", "del", "set", "update", "insert", "add", "rename", "push", "pop",
    "remove", "ack", "create", "publish", "ttl", "reset", "xadd", "xack",
];

fn permission_tokens(definition: &DriverCommandDefinition) -> Vec<String> {
    let mut tokens = Vec::new();
    for value in definition
        .permissions
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(definition.id.as_str()))
    {
        tokens.extend(
            value
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|t| !t.is_empty())
                .map(|t| t.to_ascii_lowercase()),
        );
    }
    tokens
}

/// Classify a command from explicit metadata, then permissions / command id.
pub fn required_access_level(definition: &DriverCommandDefinition) -> CommandAccessLevel {
    if let Some(risk) = definition.metadata.risk {
        return risk;
    }
    let tokens = permission_tokens(definition);
    if tokens
        .iter()
        .any(|t| HIGH_RISK_TOKENS.contains(&t.as_str()))
    {
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
        assert_eq!(
            definition.input_schema["required"],
            serde_json::json!(["sql"])
        );
        assert_eq!(
            definition.input_schema["properties"]["sql"]["type"],
            "string"
        );
        assert_eq!(definition.input_schema["properties"]["limit"]["minimum"], 1);
        assert!(definition.output_schema.is_none());
        assert_eq!(definition.permissions, vec!["driver.query"]);
    }

    #[test]
    fn standard_execute_definition_is_schema_driven() {
        let definition = execute_command_definition();
        assert_eq!(definition.id, "execute");
        assert_eq!(definition.name, "Execute");
        assert_eq!(
            definition.input_schema["required"],
            serde_json::json!(["sql"])
        );
        assert_eq!(definition.output_schema.as_ref().unwrap()["type"], "object");
        assert_eq!(definition.permissions, vec!["driver.execute"]);
    }

    #[test]
    fn command_input_validation_accepts_valid_input() {
        let definition = query_command_definition();
        assert!(
            validate_command_input(&definition, &serde_json::json!({"sql": "SELECT 1"})).is_ok()
        );
    }

    #[test]
    fn command_input_validation_rejects_non_object() {
        let definition = query_command_definition();
        let error =
            validate_command_input(&definition, &serde_json::json!("SELECT 1")).unwrap_err();
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
            metadata: DriverCommandMetadata::default(),
        };
        let encoded = serde_json::to_value(&definition).unwrap();
        assert_eq!(encoded["inputSchema"]["type"], "object");
        assert_eq!(encoded["outputSchema"]["type"], "string");
        assert_eq!(
            encoded["permissions"],
            serde_json::json!(["driver.custom.execute"])
        );
        assert_eq!(encoded["metadata"]["workflow"], true);
        assert_eq!(encoded["metadata"]["category"], "query");
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
        assert!(definition.metadata.workflow);
        assert!(!definition.metadata.deprecated);
    }

    #[test]
    fn query_definition_includes_default_metadata() {
        let definition = query_command_definition();
        assert_eq!(definition.metadata.category, CommandCategory::Query);
        assert_eq!(definition.metadata.risk, Some(CommandAccessLevel::Read));
        assert!(definition.metadata.workflow);
        assert!(definition.metadata.ui);
        assert!(definition.metadata.requires_connection);
    }

    #[test]
    fn query_only_definitions_omit_execute() {
        let ids: Vec<_> = query_only_command_definitions("Scan a table", "scan <table>")
            .into_iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(ids, vec!["query"]);
    }

    #[test]
    fn save_dialog_spec_round_trips_and_defaults_to_none() {
        // Metadata without a spec omits the key entirely (backward compatible).
        let plain = serde_json::to_value(DriverCommandMetadata::default()).unwrap();
        assert!(plain.get("saveDialog").is_none());
        assert_eq!(plain["requiresConnection"], serde_json::json!(true));

        let metadata = DriverCommandMetadata::new(CommandCategory::Io, CommandAccessLevel::Write)
            .unbound()
            .hide_from_workflow()
            .save_dialog(DriverSaveDialogSpec {
                file_name_field: "fileName".into(),
                data_base64_field: "dataBase64".into(),
                filter_name: "SQLite Database".into(),
                extensions: vec!["db".into(), "sqlite".into(), "sqlite3".into()],
                result_path_field: "savedPath".into(),
            });
        let encoded = serde_json::to_value(&metadata).unwrap();
        assert_eq!(encoded["requiresConnection"], false);
        assert_eq!(encoded["category"], "io");
        assert_eq!(encoded["workflow"], false);
        assert_eq!(encoded["saveDialog"]["fileNameField"], "fileName");
        assert_eq!(encoded["saveDialog"]["dataBase64Field"], "dataBase64");
        assert_eq!(encoded["saveDialog"]["filterName"], "SQLite Database");
        assert_eq!(
            encoded["saveDialog"]["extensions"],
            serde_json::json!(["db", "sqlite", "sqlite3"])
        );
        assert_eq!(encoded["saveDialog"]["resultPathField"], "savedPath");
        let decoded: DriverCommandMetadata = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, metadata);
    }

    #[test]
    fn explicit_metadata_risk_overrides_permission_tokens() {
        let mut definition = query_command_definition();
        definition.metadata.risk = Some(CommandAccessLevel::HighRisk);
        assert_eq!(
            required_access_level(&definition),
            CommandAccessLevel::HighRisk
        );
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
        assert_eq!(
            required_access_level(&query_command_definition()),
            CommandAccessLevel::Read
        );
        assert_eq!(
            required_access_level(&execute_command_definition()),
            CommandAccessLevel::Write
        );
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
            metadata: DriverCommandMetadata::default(),
        };
        let set = DriverCommandDefinition {
            id: "set_string".into(),
            name: "Set".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            permissions: vec!["redis:allow-set-string".into()],
            metadata: DriverCommandMetadata::default(),
        };
        let flush = DriverCommandDefinition {
            id: "flush_db".into(),
            name: "Flush".into(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            permissions: vec!["redis:allow-flush-db".into()],
            metadata: DriverCommandMetadata::default(),
        };
        assert_eq!(required_access_level(&info), CommandAccessLevel::Read);
        assert_eq!(required_access_level(&set), CommandAccessLevel::Write);
        assert_eq!(required_access_level(&flush), CommandAccessLevel::HighRisk);
        assert!(check_command_access(&flush, CommandAccessLevel::Write).is_err());
        assert!(check_command_access(&set, CommandAccessLevel::Write).is_ok());
        assert!(check_command_access(&info, CommandAccessLevel::Read).is_ok());
    }
}
