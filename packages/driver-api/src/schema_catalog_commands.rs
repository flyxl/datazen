//! Driver Command definitions and execution for schema catalog browsing.
//!
//! These commands delegate to [`DatabaseDriver`] trait methods (`get_databases`,
//! `get_tables`, `get_table_schema`). Host GUI IPC, Workflow, and MCP converge
//! on this path via `execute_driver_command`.

use serde_json::{json, Value as JsonValue};

use crate::command::{
    CommandAccessLevel, CommandCategory, CommandResult, DriverCommandDefinition,
    DriverCommandMetadata,
};
use crate::traits::DatabaseDriver;
use crate::types::{DriverError, TableInfo, TableSchema};
use crate::ConnectionHandle;

const SCHEMA_CATALOG_COMMANDS: &[&str] = &["list_databases", "list_tables", "get_table_schema"];

pub fn is_schema_catalog_command(command: &str) -> bool {
    SCHEMA_CATALOG_COMMANDS.contains(&command)
}

pub fn schema_catalog_command_definitions() -> Vec<DriverCommandDefinition> {
    vec![
        DriverCommandDefinition {
            id: "list_databases".into(),
            name: "List Databases".into(),
            description: Some("List logical databases (or namespaces) on the connection".into()),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "databases": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["databases"]
            })),
            permissions: vec!["driver.query".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read)
                .hide_from_workflow(),
        },
        DriverCommandDefinition {
            id: "list_tables".into(),
            name: "List Tables".into(),
            description: Some("List tables and views in a database".into()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "database": {
                        "type": "string",
                        "description": "Target database (or namespace)"
                    }
                },
                "required": ["database"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "tables": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "schema": { "type": ["string", "null"] },
                                "tableType": { "type": "string" },
                                "rowCount": { "type": ["integer", "null"] }
                            },
                            "required": ["name", "tableType"]
                        }
                    }
                },
                "required": ["tables"]
            })),
            permissions: vec!["driver.query".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read)
                .hide_from_workflow(),
        },
        DriverCommandDefinition {
            id: "get_table_schema".into(),
            name: "Get Table Schema".into(),
            description: Some(
                "Return full table schema (columns, indexes, foreign keys)".into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "table": {
                        "type": "string",
                        "description": "Table name"
                    }
                },
                "required": ["table"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "schema": {
                        "type": "object",
                        "description": "Full TableSchema payload"
                    }
                },
                "required": ["schema"]
            })),
            permissions: vec!["driver.query".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read)
                .hide_from_workflow(),
        },
    ]
}

/// Dispatch schema catalog commands after standard SQL commands are ruled out.
pub async fn try_execute_schema_catalog_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    command: &str,
    input: JsonValue,
) -> Result<Option<CommandResult>, DriverError> {
    if !is_schema_catalog_command(command) {
        return Ok(None);
    }
    Ok(Some(
        execute_schema_catalog_command(driver, handle, command, input).await?,
    ))
}

pub async fn execute_schema_catalog_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    command: &str,
    input: JsonValue,
) -> Result<CommandResult, DriverError> {
    match command {
        "list_databases" => {
            let databases = driver.get_databases(handle).await?;
            Ok(CommandResult::new(json!({ "databases": databases })))
        }
        "list_tables" => {
            let database = input["database"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?;
            let tables = driver.get_tables(handle, database).await?;
            Ok(CommandResult::new(json!({ "tables": tables })))
        }
        "get_table_schema" => {
            let table = input["table"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
            let schema = driver.get_table_schema(handle, table).await?;
            Ok(CommandResult::new(json!({ "schema": schema })))
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported schema catalog command: {other}"
        ))),
    }
}

pub fn parse_databases_from_command(data: &JsonValue) -> Vec<String> {
    data.get("databases")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn parse_tables_from_command(data: &JsonValue) -> Vec<TableInfo> {
    data.get("tables")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn parse_table_schema_from_command(data: &JsonValue) -> Option<TableSchema> {
    data.get("schema")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TableType;

    #[test]
    fn command_definitions_include_schema_catalog_commands() {
        let defs = schema_catalog_command_definitions();
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"list_databases"));
        assert!(ids.contains(&"list_tables"));
        assert!(ids.contains(&"get_table_schema"));
    }

    #[test]
    fn parse_databases_from_command_maps_array() {
        let data = json!({ "databases": ["app", "analytics"] });
        assert_eq!(
            parse_databases_from_command(&data),
            vec!["app".to_string(), "analytics".to_string()]
        );
    }

    #[test]
    fn parse_tables_from_command_maps_table_info() {
        let data = json!({
            "tables": [{
                "name": "users",
                "schema": "public",
                "tableType": "table",
                "rowCount": 42
            }]
        });
        let tables = parse_tables_from_command(&data);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "users");
        assert_eq!(tables[0].schema.as_deref(), Some("public"));
        assert!(matches!(tables[0].table_type, TableType::Table));
        assert_eq!(tables[0].row_count, Some(42));
    }

    #[test]
    fn parse_table_schema_from_command_maps_schema() {
        let data = json!({
            "schema": {
                "tableName": "users",
                "columns": [{
                    "name": "id",
                    "dataType": "integer",
                    "nullable": false,
                    "defaultValue": null,
                    "comment": null,
                    "isPrimaryKey": true,
                    "isAutoIncrement": false
                }],
                "primaryKeys": ["id"],
                "indexes": [],
                "foreignKeys": []
            }
        });
        let schema = parse_table_schema_from_command(&data).unwrap();
        assert_eq!(schema.table_name, "users");
        assert_eq!(schema.columns.len(), 1);
        assert_eq!(schema.columns[0].name, "id");
        assert!(schema.columns[0].is_primary_key);
    }

    #[test]
    fn is_schema_catalog_command_recognizes_ids() {
        assert!(is_schema_catalog_command("list_databases"));
        assert!(is_schema_catalog_command("list_tables"));
        assert!(is_schema_catalog_command("get_table_schema"));
        assert!(!is_schema_catalog_command("list_objects"));
    }
}
