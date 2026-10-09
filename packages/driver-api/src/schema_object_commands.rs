//! Driver Command definitions and execution for schema object browsers.

use serde_json::{json, Value as JsonValue};

mod dependencies;
use dependencies::parse_object_dependency_catalog;
mod mysql_view_metadata;
#[cfg(test)]
use mysql_view_metadata::mysql_show_create_view_check_option;
use mysql_view_metadata::{
    metadata_value_matches, missing_show_view_metadata, mysql_show_create_view_for_parser,
    mysql_view_definer_identity, mysql_view_query_bodies_match, required_view_metadata_field,
};

use crate::command::{
    CommandAccessLevel, CommandCategory, CommandResult, DriverCommandDefinition,
    DriverCommandMetadata,
};
use crate::schema_dependencies::{
    mysql_dependency_grants_are_complete, mysql_table_dependencies_sql,
    mysql_table_dependency_grants_are_complete, postgres_function_dependencies_sql,
    postgres_sequence_dependencies_sql, postgres_table_dependencies_sql,
    postgres_trigger_dependencies_sql, postgres_type_dependencies_sql, view_dependencies_sql,
    SchemaObjectDependencies, SequenceDependencyUsageKind, MYSQL_DEPENDENCY_GRANTS_SQL,
    MYSQL_UDF_CATALOG_SQL,
};
use crate::schema_objects::{
    list_objects_sql_for_database, list_privileges_sql, object_ddl_sql_with_metadata,
    DatabaseObject, ObjectKind, PrivilegeGrant,
};
use crate::sql_target::SqlTarget;
use crate::traits::DatabaseDriver;
use crate::types::{ColumnInfo, DriverError, QueryResult, Value};
use crate::ConnectionHandle;
use sqlparser::{
    ast::{CreateViewSecurity, Statement},
    dialect::MySqlDialect,
    parser::Parser,
};

const SCHEMA_OBJECT_COMMANDS: &[&str] = &[
    "list_objects",
    "get_object_ddl",
    "get_object_dependencies",
    "list_privileges",
];

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
                        "enum": ["table", "view", "function", "procedure", "trigger", "sequence", "type"],
                        "description": "Object kind to list"
                    },
                    "database": { "type": ["string", "null"], "description": "Database that owns the objects (optional)" }
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
                                "name": { "type": "string" },
                                "signature": { "type": ["string", "null"] },
                                "targetSchema": { "type": ["string", "null"] },
                                "targetName": { "type": ["string", "null"] }
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
                        "enum": ["table", "view", "function", "procedure", "trigger", "sequence", "type"],
                        "description": "Object kind"
                    },
                    "name": { "type": "string", "description": "Object name" },
                    "schema": { "type": ["string", "null"], "description": "Schema name (optional)" },
                    "database": { "type": ["string", "null"], "description": "Database that owns the object (optional)" },
                    "signature": { "type": ["string", "null"], "description": "Routine identity arguments (PostgreSQL overload disambiguation)" },
                    "targetSchema": { "type": ["string", "null"], "description": "Trigger target schema (optional)" },
                    "targetName": { "type": ["string", "null"], "description": "Trigger target relation (optional)" }
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
            id: "get_object_dependencies".into(),
            name: "Get Object Dependencies".into(),
            description: Some(
                "Return direct structured schema dependencies and whether the catalog is complete"
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["table", "view", "function", "procedure", "trigger", "sequence", "type"] },
                    "name": { "type": "string" },
                    "schema": { "type": ["string", "null"] },
                    "signature": { "type": ["string", "null"] },
                    "targetSchema": { "type": ["string", "null"] },
                    "targetName": { "type": ["string", "null"] }
                },
                "required": ["kind", "name"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "complete": { "type": "boolean" },
                    "dependencies": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string" },
                                "schema": { "type": ["string", "null"] },
                                "name": { "type": "string" },
                                "signature": { "type": ["string", "null"] },
                                "targetSchema": { "type": ["string", "null"] },
                                "targetName": { "type": ["string", "null"] }
                            },
                            "required": ["kind", "name"]
                        }
                    },
                    "typeDependencyUsages": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "dependency": {
                                    "type": "object",
                                    "properties": {
                                        "kind": { "type": "string" },
                                        "schema": { "type": ["string", "null"] },
                                        "name": { "type": "string" },
                                        "signature": { "type": ["string", "null"] }
                                    },
                                    "required": ["kind", "name"]
                                },
                                "usage": { "type": "string", "enum": ["column_type", "expression", "constraint"] },
                                "columnName": { "type": "string" }
                            },
                            "required": ["dependency", "usage"]
                        }
                    },
                    "sequenceDependencyUsages": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "sequence": {
                                    "type": "object",
                                    "properties": {
                                        "kind": { "type": "string" },
                                        "schema": { "type": ["string", "null"] },
                                        "name": { "type": "string" }
                                    },
                                    "required": ["kind", "name"]
                                },
                                "ownerTable": {
                                    "type": "object",
                                    "properties": {
                                        "kind": { "type": "string" },
                                        "schema": { "type": ["string", "null"] },
                                        "name": { "type": "string" }
                                    },
                                    "required": ["kind", "name"]
                                },
                                "columnName": { "type": "string" },
                                "usage": { "type": "string", "enum": ["column_default", "owned_by"] }
                            },
                            "required": ["sequence", "ownerTable", "columnName", "usage"]
                        }
                    }
                },
                "required": ["complete", "dependencies"]
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
            let database = input.get("database").and_then(JsonValue::as_str);
            let Some(sql) = list_objects_sql_for_database(db_type, parsed, database) else {
                return Ok(CommandResult::new(json!({ "objects": [] })));
            };
            let result = query_in_target_database(driver, db_type, handle, &sql, database).await?;
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
            if name.trim().is_empty() {
                return Err(DriverError::InvalidConfig("name must not be empty".into()));
            }
            let schema = input.get("schema").and_then(|v| v.as_str());
            let signature = input.get("signature").and_then(|v| v.as_str());
            let target_schema = input.get("targetSchema").and_then(|v| v.as_str());
            let target_name = input.get("targetName").and_then(|v| v.as_str());
            let database = input.get("database").and_then(JsonValue::as_str);
            let object_schema = if crate::schema_objects::dialect_family(db_type) == "mysql" {
                schema.or(database)
            } else {
                schema
            };
            let Some(sql) = object_ddl_sql_with_metadata(
                db_type,
                parsed,
                name,
                object_schema,
                signature,
                target_schema,
                target_name,
            ) else {
                return Err(DriverError::InvalidConfig(
                    "This database type does not expose object DDL".into(),
                ));
            };
            let result = query_in_target_database(driver, db_type, handle, &sql, database).await?;
            let ddl = extract_object_ddl_checked(&result)?;
            let view_metadata = if parsed == ObjectKind::View
                && crate::schema_objects::dialect_family(db_type) == "mysql"
            {
                let show_create_sql =
                    crate::schema_objects::mysql_show_create_view_sql(name, object_schema);
                let show_create = driver.query(handle, &show_create_sql).await?;
                Some(extract_mysql_view_metadata(&result, &show_create)?)
            } else {
                None
            };
            let mut response = json!({ "ddl": ddl });
            if let Some(metadata) = view_metadata {
                response["viewMetadata"] = serde_json::to_value(metadata).map_err(|error| {
                    DriverError::QueryFailed(format!("serialize MySQL view metadata: {error}"))
                })?;
            }
            Ok(CommandResult::new(response))
        }
        "get_object_dependencies" => {
            let result = execute_object_dependencies(driver, db_type, handle, &input).await;
            Ok(CommandResult::new(serde_json::to_value(result).map_err(
                |error| DriverError::QueryFailed(format!("serialize dependency catalog: {error}")),
            )?))
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

async fn query_in_target_database<D: DatabaseDriver + ?Sized>(
    driver: &D,
    db_type: &str,
    handle: &ConnectionHandle,
    sql: &str,
    database: Option<&str>,
) -> Result<QueryResult, DriverError> {
    if crate::schema_objects::dialect_family(db_type) == "postgresql" {
        driver
            .query_at(handle, sql, SqlTarget::new(database, None))
            .await
    } else {
        driver.query(handle, sql).await
    }
}

async fn execute_object_dependencies<D: DatabaseDriver + ?Sized>(
    driver: &D,
    db_type: &str,
    handle: &ConnectionHandle,
    input: &JsonValue,
) -> SchemaObjectDependencies {
    let Some(kind) = input["kind"].as_str().and_then(ObjectKind::parse) else {
        return SchemaObjectDependencies::incomplete();
    };
    let Some(name) = input["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return SchemaObjectDependencies::incomplete();
    };
    let schema = input["schema"]
        .as_str()
        .map(str::trim)
        .filter(|schema| !schema.is_empty());
    let target_schema = input["targetSchema"]
        .as_str()
        .map(str::trim)
        .filter(|schema| !schema.is_empty());
    let target_name = input["targetName"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let signature = input["signature"].as_str();
    let family = crate::schema_objects::dialect_family(db_type);
    let sql = match (family, kind) {
        (_, ObjectKind::View) => view_dependencies_sql(db_type, name, schema),
        ("postgresql", ObjectKind::Trigger) => Some(postgres_trigger_dependencies_sql(
            name,
            schema,
            target_schema,
            target_name,
        )),
        ("postgresql", ObjectKind::Function) => {
            Some(postgres_function_dependencies_sql(name, schema, signature))
        }
        ("mysql", ObjectKind::Table) => Some(mysql_table_dependencies_sql(name, schema)),
        ("postgresql", ObjectKind::Table) => Some(postgres_table_dependencies_sql(name, schema)),
        ("postgresql", ObjectKind::Sequence) => {
            Some(postgres_sequence_dependencies_sql(name, schema))
        }
        ("postgresql", ObjectKind::Type) => Some(postgres_type_dependencies_sql(name, schema)),
        // MySQL routines/triggers and PostgreSQL procedure bodies can have
        // opaque dependencies. Trigger targets are separately represented as
        // structured metadata and added by the Host planner.
        _ => None,
    };
    let Some(sql) = sql else {
        return SchemaObjectDependencies::incomplete();
    };
    let required_sequence_usage = match (family, kind) {
        ("postgresql", ObjectKind::Table) => Some(SequenceDependencyUsageKind::ColumnDefault),
        ("postgresql", ObjectKind::Sequence) => Some(SequenceDependencyUsageKind::OwnedBy),
        _ => None,
    };
    let Ok(query_result) = driver.query(handle, &sql).await else {
        return SchemaObjectDependencies::incomplete();
    };
    let Ok((
        selected_count,
        unsupported_count,
        dependencies,
        type_dependency_usages,
        type_usage_complete,
        sequence_dependency_usages,
        sequence_usage_complete,
    )) = parse_object_dependency_catalog(
        &query_result,
        family == "postgresql" && kind == ObjectKind::Table,
        required_sequence_usage,
    )
    else {
        return SchemaObjectDependencies::incomplete();
    };
    let mut complete = selected_count == Some(1)
        && unsupported_count == Some(0)
        && type_usage_complete
        && sequence_usage_complete;

    if family == "mysql" && kind == ObjectKind::View {
        complete = complete && mysql_dependency_catalog_visibility(driver, handle).await;
    }
    if family == "mysql" && kind == ObjectKind::Table {
        complete = complete && mysql_table_dependency_catalog_visibility(driver, handle).await;
    }

    SchemaObjectDependencies {
        complete,
        dependencies,
        type_dependency_usages,
        sequence_dependency_usages: required_sequence_usage
            .and_then(|_| complete.then_some(sequence_dependency_usages)),
    }
}

async fn mysql_dependency_catalog_visibility<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
) -> bool {
    let Ok(grants_result) = driver.query(handle, MYSQL_DEPENDENCY_GRANTS_SQL).await else {
        return false;
    };
    let grant_strings = grants_result
        .rows
        .iter()
        .filter_map(|row| value_as_string(row.first().and_then(Option::as_ref)))
        .collect::<Vec<_>>();
    if !mysql_dependency_grants_are_complete(&grant_strings) {
        return false;
    }
    let Ok(udf_result) = driver.query(handle, MYSQL_UDF_CATALOG_SQL).await else {
        return false;
    };
    let Some(index) = column_index(&udf_result.columns, &["udf_count"]) else {
        return false;
    };
    udf_result
        .rows
        .first()
        .and_then(|row| value_as_string(row.get(index).and_then(Option::as_ref)))
        .and_then(|count| count.parse::<i64>().ok())
        == Some(0)
}

async fn mysql_table_dependency_catalog_visibility<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
) -> bool {
    let Ok(grants_result) = driver.query(handle, MYSQL_DEPENDENCY_GRANTS_SQL).await else {
        return false;
    };
    let grant_strings = grants_result
        .rows
        .iter()
        .filter_map(|row| value_as_string(row.first().and_then(Option::as_ref)))
        .collect::<Vec<_>>();
    mysql_table_dependency_grants_are_complete(&grant_strings)
}

fn value_as_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Integer(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn value_as_string_allow_empty(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Integer(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn column_index(columns: &[ColumnInfo], names: &[&str]) -> Option<usize> {
    columns
        .iter()
        .position(|c| names.iter().any(|n| c.name.eq_ignore_ascii_case(n)))
}

fn value_as_ddl_text(value: Option<&Value>) -> Option<String> {
    match value {
        // Some MySQL catalog text columns are exposed with a binary wire type.
        // DDL is text, so accept only valid UTF-8 and never replace invalid bytes.
        Some(Value::Bytes(bytes)) => std::str::from_utf8(bytes).ok().map(str::to_owned),
        value => value_as_string(value),
    }
}

fn extract_mysql_view_metadata(
    result: &QueryResult,
    show_create: &QueryResult,
) -> Result<crate::schema_scope_mapping::MySqlViewMetadata, DriverError> {
    // SHOW CREATE VIEW is authoritative for creation options and returns the
    // character-set context in the same result. INFORMATION_SCHEMA still
    // supplies the renderer's original query body, but every shared field and
    // the parsed query must match so two catalog moments cannot be combined.
    let catalog_field = |name: &str| required_view_metadata_field(result, name);
    let create_view = extract_object_ddl_checked(show_create)?;
    let (parser_ddl, check_option) = mysql_show_create_view_for_parser(&create_view)?;
    let statements = Parser::parse_sql(&MySqlDialect {}, &parser_ddl).map_err(|error| {
        DriverError::QueryFailed(format!("parse MySQL SHOW CREATE VIEW result: {error}"))
    })?;
    if statements.len() != 1 {
        return Err(DriverError::QueryFailed(
            "MySQL SHOW CREATE VIEW returned an unexpected statement count".into(),
        ));
    }
    let Statement::CreateView {
        columns,
        params,
        query,
        ..
    } = &statements[0]
    else {
        return Err(DriverError::QueryFailed(
            "MySQL SHOW CREATE VIEW did not return a CREATE VIEW definition".into(),
        ));
    };
    let body = extract_object_ddl_checked(result)?;
    let body_statements = Parser::parse_sql(&MySqlDialect {}, &body).map_err(|error| {
        DriverError::QueryFailed(format!("parse MySQL VIEW_DEFINITION body: {error}"))
    })?;
    let body_query = match body_statements.as_slice() {
        [Statement::Query(query)] => query,
        _ => {
            return Err(DriverError::QueryFailed(
                "MySQL VIEW_DEFINITION did not return exactly one SELECT query".into(),
            ));
        }
    };
    let source_database = required_view_metadata_field(result, "view_schema")?;
    if !mysql_view_query_bodies_match(body_query, query, &source_database) {
        return Err(DriverError::QueryFailed(
            "MySQL VIEW_DEFINITION and SHOW CREATE VIEW describe different query bodies or database identities".into(),
        ));
    }

    let params = params.as_ref().ok_or_else(|| {
        DriverError::QueryFailed(
            "MySQL SHOW CREATE VIEW omitted creation parameters needed for a consistent snapshot"
                .into(),
        )
    })?;
    let algorithm = params
        .algorithm
        .as_ref()
        .map(ToString::to_string)
        .ok_or_else(|| missing_show_view_metadata("ALGORITHM"))?
        .to_ascii_uppercase();
    let definer = params
        .definer
        .as_ref()
        .map(mysql_view_definer_identity)
        .transpose()?
        .ok_or_else(|| missing_show_view_metadata("DEFINER"))?;
    let security_type = params
        .security
        .as_ref()
        .map(|security| match security {
            CreateViewSecurity::Definer => "DEFINER",
            CreateViewSecurity::Invoker => "INVOKER",
        })
        .ok_or_else(|| missing_show_view_metadata("SQL SECURITY"))?;
    let character_set_client = required_view_metadata_field(show_create, "character_set_client")?;
    let collation_connection = required_view_metadata_field(show_create, "collation_connection")?;

    let catalog_definer = catalog_field("view_definer")?;
    let catalog_security = catalog_field("view_security_type")?;
    let catalog_check_option = catalog_field("view_check_option")?;
    let catalog_character_set = catalog_field("view_character_set_client")?;
    let catalog_collation = catalog_field("view_collation_connection")?;
    if !metadata_value_matches(&definer, &catalog_definer, false)
        || !metadata_value_matches(security_type, &catalog_security, true)
        || !metadata_value_matches(&check_option, &catalog_check_option, true)
        || !metadata_value_matches(&character_set_client, &catalog_character_set, true)
        || !metadata_value_matches(&collation_connection, &catalog_collation, true)
    {
        return Err(DriverError::QueryFailed(
            "MySQL INFORMATION_SCHEMA.VIEWS and SHOW CREATE VIEW creation metadata disagree; retry the schema comparison".into(),
        ));
    }

    Ok(crate::schema_scope_mapping::MySqlViewMetadata {
        algorithm,
        definer,
        security_type: security_type.into(),
        check_option,
        character_set_client,
        collation_connection,
        has_explicit_column_list: !columns.is_empty(),
    })
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
    let signature_idx = column_index(
        &result.columns,
        &["signature", "identity_arguments", "identityArguments"],
    );
    let target_schema_idx = column_index(
        &result.columns,
        &["target_schema", "targetSchema", "event_object_schema"],
    );
    let target_name_idx = column_index(
        &result.columns,
        &["target_name", "targetName", "event_object_table", "Table"],
    );
    let mut out = Vec::new();
    for row in &result.rows {
        let Some(name) = value_as_string(row.get(name_idx).and_then(|v| v.as_ref())) else {
            continue;
        };
        let schema = schema_idx.and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref())));
        let signature = signature_idx
            .and_then(|i| value_as_string_allow_empty(row.get(i).and_then(|v| v.as_ref())));
        let target_schema =
            target_schema_idx.and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref())));
        let target_name =
            target_name_idx.and_then(|i| value_as_string(row.get(i).and_then(|v| v.as_ref())));
        out.push(DatabaseObject {
            kind: kind.into(),
            schema,
            name,
            signature,
            target_schema,
            target_name,
        });
    }
    Ok(out)
}

pub fn extract_object_ddl(result: &QueryResult) -> String {
    let ddl_idx = column_index(
        &result.columns,
        &[
            "ddl",
            "Create View",
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
        .and_then(|row| value_as_ddl_text(row.get(idx).and_then(|v| v.as_ref())))
        .unwrap_or_default()
}

/// Extract exactly one non-empty DDL value. A missing object and an ambiguous
/// overload are both errors so callers cannot accidentally execute a partial
/// or unrelated definition.
pub fn extract_object_ddl_checked(result: &QueryResult) -> Result<String, DriverError> {
    if result.rows.len() > 1 {
        return Err(DriverError::QueryFailed(
            "Object DDL lookup returned multiple definitions; provide the full identity".into(),
        ));
    }
    let ddl = extract_object_ddl(result);
    if ddl.trim().is_empty() {
        return Err(DriverError::QueryFailed(
            "Object was not found or its DDL is unavailable".into(),
        ));
    }
    Ok(ddl)
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
mod tests;
