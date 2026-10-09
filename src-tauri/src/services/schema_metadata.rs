//! Host metadata policy, entered only after Driver Command access checks.
mod columns;

use std::collections::HashSet;
use std::sync::Arc;

use datazen_driver_api::schema_metadata::{
    ColumnsReadResult, ListCatalogOutput, MetadataReadError, MetadataReadErrorCode,
    MetadataRefreshScope, ReadColumnsOutput, RelationColumns, RelationRef, RelationSchema,
    RelationSummary,
};
use datazen_driver_api::{
    validate_schema_target, CommandAccessLevel, CommandCategory, CommandResult, ConnectionHandle,
    DatabaseDriver, DriverCommandDefinition, DriverCommandMetadata, DriverError, SchemaScope,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::commands::{AppState, CommandError};

pub fn is_metadata_command(command: &str) -> bool {
    matches!(
        command,
        "list_catalog"
            | "read_relation_columns"
            | "read_relation_schema"
            | "refresh_schema_metadata"
    )
}

pub fn command_definitions() -> Vec<DriverCommandDefinition> {
    let relation = json!({
        "type": "object",
        "properties": {
            "database": { "type": "string" },
            "schema": { "type": ["string", "null"] },
            "name": { "type": "string" }
        },
        "required": ["database", "schema", "name"]
    });
    [
        (
            "list_catalog",
            json!({
                "type": "object", "properties": {
                    "database": { "type": "string" },
                    "schema": { "type": ["string", "null"] }
                }, "required": ["database", "schema"]
            }),
        ),
        (
            "read_relation_columns",
            json!({
                "type": "object", "properties": {
                    "relations": { "type": "array", "items": relation.clone() }
                }, "required": ["relations"]
            }),
        ),
        (
            "read_relation_schema",
            json!({
                "type": "object", "properties": { "relation": relation },
                "required": ["relation"]
            }),
        ),
        (
            "refresh_schema_metadata",
            json!({
                "type": "object", "properties": { "scope": { "type": "object" } },
                "required": ["scope"]
            }),
        ),
    ]
    .into_iter()
    .map(|(id, input_schema)| DriverCommandDefinition {
        id: id.into(),
        name: id.into(),
        description: None,
        input_schema,
        output_schema: None,
        permissions: vec!["driver.query".into()],
        metadata: DriverCommandMetadata::new(CommandCategory::Query, CommandAccessLevel::Read)
            .hide_from_workflow(),
    })
    .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogInput {
    database: String,
    schema: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ColumnsInput {
    relations: Vec<RelationRef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaInput {
    relation: RelationRef,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefreshInput {
    scope: MetadataRefreshScope,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, CommandError> {
    serde_json::from_value(value)
        .map_err(|e| CommandError::Validation(format!("Invalid metadata request: {e}")))
}

fn validate_relation(
    driver: &dyn DatabaseDriver,
    relation: &RelationRef,
) -> Result<(), CommandError> {
    if relation.database.trim().is_empty() {
        return Err(CommandError::Validation("Database is required".into()));
    }
    if relation
        .schema
        .as_deref()
        .is_some_and(|schema| schema.trim().is_empty())
    {
        return Err(CommandError::Validation(
            "Schema must be null or a non-blank name".into(),
        ));
    }
    if relation.name.trim().is_empty() {
        return Err(CommandError::Validation("Relation name is required".into()));
    }
    validate_schema_target(
        driver,
        &relation.database,
        relation.schema.as_deref(),
        SchemaScope::ExactSchema,
    )?;
    Ok(())
}

pub async fn execute(
    state: &AppState,
    db_session_id: &str,
    driver: &Arc<dyn DatabaseDriver>,
    handle: &ConnectionHandle,
    command: &str,
    input: Value,
) -> Result<CommandResult, CommandError> {
    let data = match command {
        "refresh_schema_metadata" => {
            let input: RefreshInput = decode(input)?;
            match input.scope {
                MetadataRefreshScope::Session => {
                    state.schema_cache.clear_connection(db_session_id).await
                }
                MetadataRefreshScope::Database { database } => {
                    if database.trim().is_empty() {
                        return Err(CommandError::Validation("Database is required".into()));
                    }
                    state
                        .schema_cache
                        .invalidate(db_session_id, &database, None)
                        .await;
                }
                MetadataRefreshScope::Relation { relation } => {
                    validate_relation(driver.as_ref(), &relation)?;
                    state
                        .schema_cache
                        .invalidate_relation(
                            db_session_id,
                            &relation.database,
                            relation.schema.as_deref(),
                            &relation.name,
                        )
                        .await;
                }
            }
            json!({ "revision": state.schema_cache.generation() })
        }
        "list_catalog" => {
            let input: CatalogInput = decode(input)?;
            if input.database.trim().is_empty() {
                return Err(CommandError::Validation("Database is required".into()));
            }
            validate_schema_target(
                driver.as_ref(),
                &input.database,
                input.schema.as_deref(),
                SchemaScope::AnySchema,
            )?;
            // Preserve driver-owned hierarchical catalog commands as well as
            // the standard trait-backed list_tables implementation.
            let result = driver
                .execute_command(
                    handle,
                    "list_tables",
                    json!({
                        "database": input.database, "schema": input.schema,
                    }),
                )
                .await?;
            let tables = datazen_driver_api::parse_tables_from_command(&result.data);
            let mut schemas: Vec<String> = tables.iter().filter_map(|t| t.schema.clone()).collect();
            schemas.sort();
            schemas.dedup();
            let relations = tables
                .into_iter()
                .filter(|t| !t.name.is_empty())
                .map(|t| RelationSummary {
                    relation: RelationRef {
                        database: input.database.clone(),
                        schema: t.schema,
                        name: t.name,
                    },
                    kind: t.table_type,
                    row_count: t.row_count,
                })
                .collect();
            serde_json::to_value(ListCatalogOutput {
                database: input.database,
                schemas,
                relations,
            })?
        }
        "read_relation_columns" => {
            let input: ColumnsInput = decode(input)?;
            serde_json::to_value(
                columns::read(state, db_session_id, driver, handle, input.relations).await?,
            )?
        }
        "read_relation_schema" => {
            let input: SchemaInput = decode(input)?;
            validate_relation(driver.as_ref(), &input.relation)?;
            let definition = state
                .schema_cache
                .get_table_schema(
                    db_session_id,
                    &input.relation.database,
                    input.relation.schema.as_deref(),
                    &input.relation.name,
                    driver,
                    handle,
                )
                .await?;
            json!({ "value": RelationSchema { relation: input.relation, definition } })
        }
        _ => {
            return Err(CommandError::Validation(
                "Unsupported metadata command".into(),
            ))
        }
    };
    Ok(CommandResult::new(data))
}

#[cfg(test)]
mod tests;
