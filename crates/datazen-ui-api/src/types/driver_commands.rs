//! The driver-command-catalog re-exports the aggregate still owed — ported
//! from React `packages/frontend/src/types/index.ts` (tasklist T211).
//!
//! React's `types/index.ts` re-exports five command-catalog names from
//! `@datazen/backend-client`: [`CommandCategory`],
//! [`CommandAccessLevel`], [`DriverSaveDialogSpec`],
//! [`DriverCommandMetadata`], [`DriverCommandDefinition`]. Source of truth:
//! `packages/backend-client/src/types/driverCommands.ts` — the DTOs of the
//! driver command catalog (which commands a driver exposes, how risky they
//! are, and what a save-dialog command needs). No Rust home existed
//! (driver-api's catalog is the native registry, not these DTOs), so this
//! module is the mirror.
//!
//! The same TS file's query half (`Value`, `ColumnInfo`, `QueryResult`, …)
//! is [`crate::types::query`]; `Value` itself is owned by
//! `connection_types` (T128).
//!
//! Pure contracts — no IO, no state — so the module compiles for wasm32
//! exactly like its `chart_types` sibling.

use serde::{Deserialize, Serialize};

/// React `CommandCategory` — `'query' | 'mutate' | 'admin' | 'observe' |
/// 'pubSub' | 'stream' | 'io'` — what kind of work a driver command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandCategory {
    Query,
    Mutate,
    Admin,
    Observe,
    PubSub,
    Stream,
    Io,
}

impl CommandCategory {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutate => "mutate",
            Self::Admin => "admin",
            Self::Observe => "observe",
            Self::PubSub => "pubSub",
            Self::Stream => "stream",
            Self::Io => "io",
        }
    }
}

/// React `CommandAccessLevel` — `'read' | 'write' | 'highRisk'` — how
/// dangerous a driver command is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandAccessLevel {
    Read,
    Write,
    HighRisk,
}

impl CommandAccessLevel {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::HighRisk => "highRisk",
        }
    }
}

/// React `DriverSaveDialogSpec` — the host-side file dialog a driver
/// command asks for (the field names the driver reads back are part of the
/// contract).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverSaveDialogSpec {
    pub file_name_field: String,
    pub data_base64_field: String,
    pub filter_name: String,
    pub extensions: Vec<String>,
    pub result_path_field: String,
}

/// React `DriverCommandMetadata` — the catalog row of one driver command.
///
/// `risk`, `replacedBy` and `saveDialog` are `?: <T> | null` — null on the
/// wire when set to `None`, absent also accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCommandMetadata {
    pub category: CommandCategory,
    pub risk: Option<CommandAccessLevel>,
    pub workflow: bool,
    pub ui: bool,
    pub deprecated: bool,
    pub replaced_by: Option<String>,
    pub requires_connection: bool,
    pub save_dialog: Option<DriverSaveDialogSpec>,
}

/// React `DriverCommandDefinition` — one command a driver exposes, with its
/// schema bags (`Record<string, unknown>` → opaque [`Value`] — the input
/// schema is a JSON-schema-ish bag the host validates externally, never
/// structurally here).
///
/// `description` is plain `?:` (absent stays absent);
/// `outputSchema` is `?: Record<string, unknown> | null` — null on the
/// wire when set to `None`, absent also accepted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCommandDefinition {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub permissions: Vec<String>,
    pub metadata: DriverCommandMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn command_category_and_access_level_keep_the_react_literals() {
        let categories = [
            (CommandCategory::Query, "query"),
            (CommandCategory::Mutate, "mutate"),
            (CommandCategory::Admin, "admin"),
            (CommandCategory::Observe, "observe"),
            (CommandCategory::PubSub, "pubSub"),
            (CommandCategory::Stream, "stream"),
            (CommandCategory::Io, "io"),
        ];
        for (value, literal) in categories {
            assert_eq!(value.as_str(), literal);
            let parsed: CommandCategory = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let levels = [
            (CommandAccessLevel::Read, "read"),
            (CommandAccessLevel::Write, "write"),
            (CommandAccessLevel::HighRisk, "highRisk"),
        ];
        for (value, literal) in levels {
            assert_eq!(value.as_str(), literal);
            let parsed: CommandAccessLevel = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn a_command_definition_round_trips_its_catalog_row() {
        let definition = DriverCommandDefinition {
            id: "export_csv".into(),
            name: "Export CSV".into(),
            description: Some("Streams a table as CSV".into()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "table": { "type": "string" } },
            }),
            output_schema: None,
            permissions: vec!["read".into()],
            metadata: DriverCommandMetadata {
                category: CommandCategory::Observe,
                risk: None,
                workflow: false,
                ui: true,
                deprecated: false,
                replaced_by: None,
                requires_connection: true,
                save_dialog: Some(DriverSaveDialogSpec {
                    file_name_field: "fileName".into(),
                    data_base64_field: "dataBase64".into(),
                    filter_name: "CSV files".into(),
                    extensions: vec!["csv".into()],
                    result_path_field: "resultPath".into(),
                }),
            },
        };
        let wire = encode(&definition);
        assert_eq!(wire["id"], "export_csv");
        assert_eq!(wire["metadata"]["category"], "observe");
        assert_eq!(wire["metadata"]["saveDialog"]["dataBase64Field"], "dataBase64");
        // `?: T | null` legs keep explicit nulls; plain `?:` stays absent.
        assert_eq!(wire["outputSchema"], serde_json::Value::Null);
        assert!(wire.get("metadata").unwrap().get("replacedBy").unwrap().is_null());
        let back: DriverCommandDefinition = decode(wire);
        assert_eq!(back, definition);
    }
}
