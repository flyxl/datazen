//! Driver Command definitions and execution for schema object browsers.

use serde_json::{json, Value as JsonValue};

use crate::command::{
    CommandAccessLevel, CommandCategory, CommandResult, DriverCommandDefinition,
    DriverCommandMetadata,
};
use crate::schema_objects::{
    list_objects_sql, list_privileges_sql, object_ddl_sql, DatabaseObject, ObjectKind,
    PrivilegeGrant,
};
use crate::traits::DatabaseDriver;
use crate::types::{ColumnInfo, DriverError, QueryResult, Value};
use crate::ConnectionHandle;

const SCHEMA_OBJECT_COMMANDS: &[&str] = &["list_objects", "get_object_ddl", "list_privileges"];

pub fn is_schema_object_command(command: &str) -> bool {
    SCHEMA_OBJECT_COMMANDS.contains(&command)
}

pub fn schema_object_command_definitions() -> Vec<DriverCommandDefinition> {
    vec![
        DriverCommandDefinition {
            id: "list_objects".into(),
            name: "List Schema Objects".into(),
            description: Some(
                "List routines, triggers, sequences, or types in the current database".into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["function", "procedure", "trigger", "sequence", "type"],
                        "description": "Object kind to list"
                    }
                },
                "required": ["kind"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "objects": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string" },
                                "schema": { "type": ["string", "null"] },
                                "name": { "type": "string" }
                            },
                            "required": ["kind", "name"]
                        }
                    }
                },
                "required": ["objects"]
            })),
            permissions: vec!["driver.query".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read)
                .hide_from_workflow(),
        },
        DriverCommandDefinition {
            id: "get_object_ddl".into(),
            name: "Get Object DDL".into(),
            description: Some("Return CREATE/definition SQL for a schema object".into()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["function", "procedure", "trigger", "sequence", "type"],
                        "description": "Object kind"
                    },
                    "name": { "type": "string", "description": "Object name" },
                    "schema": { "type": ["string", "null"], "description": "Schema name (optional)" }
                },
                "required": ["kind", "name"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "ddl": { "type": "string" }
                },
                "required": ["ddl"]
            })),
            permissions: vec!["driver.query".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read)
                .hide_from_workflow(),
        },
        DriverCommandDefinition {
            id: "list_privileges".into(),
            name: "List Privileges".into(),
            description: Some("List user/role privilege grants".into()),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "grants": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "grantee": { "type": "string" },
                                "objectSchema": { "type": ["string", "null"] },
                                "objectName": { "type": "string" },
                                "privilege": { "type": "string" }
                            },
                            "required": ["grantee", "objectName", "privilege"]
                        }
                    }
                },
                "required": ["grants"]
            })),
            permissions: vec!["driver.query".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read)
                .hide_from_workflow(),
        },
    ]
}

pub async fn execute_schema_object_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    db_type: &str,
    handle: &ConnectionHandle,
    command: &str,
    input: JsonValue,
) -> Result<CommandResult, DriverError> {
    match command {
        "list_objects" => {
            let kind_raw = input["kind"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("kind is required".into()))?;
            let parsed = ObjectKind::parse(kind_raw).ok_or_else(|| {
                DriverError::InvalidConfig(format!("Unknown object kind: {kind_raw}"))
            })?;
            let Some(sql) = list_objects_sql(db_type, parsed) else {
                return Ok(CommandResult::new(json!({ "objects": [] })));
            };
            let result = driver.query(handle, &sql).await?;
            let objects = parse_object_list(&result, parsed.as_str())?;
            Ok(CommandResult::new(json!({ "objects": objects })))
        }
        "get_object_ddl" => {
            let kind_raw = input["kind"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("kind is required".into()))?;
            let parsed = ObjectKind::parse(kind_raw).ok_or_else(|| {
                DriverError::InvalidConfig(format!("Unknown object kind: {kind_raw}"))
            })?;
            let name = input["name"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("name is required".into()))?;
            let schema = input.get("schema").and_then(|v| v.as_str());
            let Some(sql) = object_ddl_sql(db_type, parsed, name, schema) else {
                return Err(DriverError::InvalidConfig(
                    "This database type does not expose object DDL".into(),
                ));
            };
            let result = driver.query(handle, &sql).await?;
            let ddl = extract_object_ddl(&result);
            Ok(CommandResult::new(json!({ "ddl": ddl })))
        }
        "list_privileges" => {
            let Some(sql) = list_privileges_sql(db_type) else {
                return Ok(CommandResult::new(json!({ "grants": [] })));
            };
            let result = driver.query(handle, &sql).await?;
            let grants = parse_privilege_list(&result)?;
            Ok(CommandResult::new(json!({ "grants": grants })))
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported schema object command: {other}"
        ))),
    }
}

fn value_as_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Integer(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn column_index(columns: &[ColumnInfo], names: &[&str]) -> Option<usize> {
    columns
        .iter()
        .position(|c| names.iter().any(|n| c.name.eq_ignore_ascii_case(n)))
}

pub fn parse_object_list(
    result: &QueryResult,
    kind: &str,
) -> Result<Vec<DatabaseObject>, DriverError> {
    if result.columns.is_empty() && result.rows.is_empty() {
        return Ok(Vec::new());
    }
    let name_idx = column_index(&result.columns, &["name", "Name", "Trigger", "proname"])
        .ok_or_else(|| DriverError::QueryFailed("Object list query missing name column".into()))?;
    let schema_idx = column_index(&result.columns, &["schema", "Db", "nspname"]);
    let mut out = Vec::new();
    for row in &result.rows {
        let Some(name) = value_as_string(row.get(name_idx).and_then(|v| v.as_ref())) else {
            continue;
        };
        let schema = schema_idx.and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref())));
        out.push(DatabaseObject {
            kind: kind.into(),
            schema,
            name,
        });
    }
    Ok(out)
}

pub fn extract_object_ddl(result: &QueryResult) -> String {
    let ddl_idx = column_index(
        &result.columns,
        &[
            "ddl",
            "Create Function",
            "Create Procedure",
            "Create Trigger",
            "SQL Original Statement",
            "pg_get_functiondef",
            "pg_get_triggerdef",
        ],
    )
    .or_else(|| {
        if result.columns.len() >= 2 {
            Some(1)
        } else {
            result.columns.first().map(|_| 0)
        }
    });
    let Some(idx) = ddl_idx else {
        return String::new();
    };
    result
        .rows
        .first()
        .and_then(|row| value_as_string(row.get(idx).and_then(|v| v.as_ref())))
        .unwrap_or_default()
}

pub fn parse_privilege_list(result: &QueryResult) -> Result<Vec<PrivilegeGrant>, DriverError> {
    let grantee_idx = column_index(&result.columns, &["grantee"]);
    let schema_idx = column_index(&result.columns, &["schema", "table_schema"]);
    let name_idx = column_index(&result.columns, &["name", "table_name"]);
    let priv_idx = column_index(&result.columns, &["privilege", "privilege_type"]);
    let mut out = Vec::new();
    for row in &result.rows {
        let Some(grantee) =
            grantee_idx.and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref())))
        else {
            continue;
        };
        let object_name = name_idx
            .and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref())))
            .unwrap_or_default();
        let privilege = priv_idx
            .and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref())))
            .unwrap_or_default();
        out.push(PrivilegeGrant {
            grantee,
            object_schema: schema_idx
                .and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref()))),
            object_name,
            privilege,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ColumnInfo;

    fn col(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
        }
    }

    #[test]
    fn command_definitions_include_schema_object_commands() {
        let defs = schema_object_command_definitions();
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"list_objects"));
        assert!(ids.contains(&"get_object_ddl"));
        assert!(ids.contains(&"list_privileges"));
    }

    #[test]
    fn parse_object_list_maps_name_and_schema() {
        let result = QueryResult {
            columns: vec![col("schema"), col("name")],
            rows: vec![vec![
                Some(Value::String("public".into())),
                Some(Value::String("fn_ok".into())),
            ]],
            rows_affected: None,
            execution_time_ms: 0,
        };
        let objects = parse_object_list(&result, "function").unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].name, "fn_ok");
        assert_eq!(objects[0].schema.as_deref(), Some("public"));
    }

    #[test]
    fn parse_object_list_empty_when_no_columns() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            execution_time_ms: 0,
        };
        assert!(parse_object_list(&result, "function").unwrap().is_empty());
    }

    #[test]
    fn extract_object_ddl_prefers_named_column() {
        let result = QueryResult {
            columns: vec![col("Function"), col("Create Function")],
            rows: vec![vec![
                Some(Value::String("fn_ok".into())),
                Some(Value::String("CREATE FUNCTION fn_ok() ...".into())),
            ]],
            rows_affected: None,
            execution_time_ms: 0,
        };
        let ddl = extract_object_ddl(&result);
        assert!(ddl.contains("CREATE FUNCTION"));
    }

    #[test]
    fn parse_privilege_list_skips_incomplete_rows() {
        let result = QueryResult {
            columns: vec![col("grantee"), col("schema"), col("name"), col("privilege")],
            rows: vec![
                vec![
                    Some(Value::String("alice".into())),
                    Some(Value::String("public".into())),
                    Some(Value::String("users".into())),
                    Some(Value::String("SELECT".into())),
                ],
                vec![None, None, None, None],
            ],
            rows_affected: None,
            execution_time_ms: 0,
        };
        let grants = parse_privilege_list(&result).unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].grantee, "alice");
    }
}
