//! Schema Diff Deploy IPC commands.

pub mod unified_plan;
pub mod job;

use super::error::{CmdExt, CommandError};
use super::sync::compare::diff_table_schemas_ir;
use super::AppState;
use crate::db::SqlTarget;
use crate::schema_diff::deploy::{
    execute_schema_diff_deploy_at as run_schema_diff_deploy, plan_has_destructive, DeployOptions,
    DESTRUCTIVE_CONFIRM_TOKEN,
};
use crate::schema_diff::diff_table_schemas;
use crate::schema_diff::objects::{
    build_routine_trigger_migration_plan_with_components,
    build_sequence_migration_plan_with_components, build_type_migration_plan_with_components,
    build_view_migration_plan_with_components, SchemaObjectSnapshot,
};
use crate::schema_diff::plan::{is_source_unbounded_text, PlanOptions};
use crate::schema_diff::types::TableColumnDiff;
use crate::schema_diff::types::{
    normalize_dialect, resolve_table_for_dialect, uses_schema_scope, ColumnTypeOverride,
    PlanRequirement, SchemaDiffDeployResult, SchemaDiffPlan,
};
use crate::schema_diff::SchemaDiffProfile;
use crate::services::job_registry::{cancel_job, ensure_job, remove_job};
use crate::transfer::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::transfer::ddl::build_create_table_ddl;
use crate::transfer::full_types::fetch_full_column_types;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use tauri::State;

async fn list_schema_views(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
) -> Result<Vec<datazen_driver_api::DatabaseObject>, CommandError> {
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "list_objects",
        serde_json::json!({ "kind": "view" }),
    )
    .await
    .map_err(CommandError::Driver)?;
    result
        .data
        .get("objects")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| CommandError::Internal(format!("invalid view metadata: {error}")))
        .map(|objects| objects.unwrap_or_default())
}

async fn ensure_distinct_schema_scope(
    source_driver: &dyn datazen_driver_api::DatabaseDriver,
    source_handle: &datazen_driver_api::ConnectionHandle,
    source_config: &datazen_driver_api::ConnectionConfig,
    target_driver: &dyn datazen_driver_api::DatabaseDriver,
    target_handle: &datazen_driver_api::ConnectionHandle,
    target_config: &datazen_driver_api::ConnectionConfig,
    source_schema_scope: Option<&str>,
    target_schema_scope: Option<&str>,
) -> Result<(), CommandError> {
    let source_identity = source_driver
        .physical_database_identity(
            source_handle,
            source_config.database.as_deref().unwrap_or_default(),
        )
        .await
        .ok()
        .flatten();
    let target_identity = target_driver
        .physical_database_identity(
            target_handle,
            target_config.database.as_deref().unwrap_or_default(),
        )
        .await
        .ok()
        .flatten();
    let is_sqlserver = crate::schema_diff::types::normalize_dialect(&source_config.database_type)
        == "sqlserver"
        && crate::schema_diff::types::normalize_dialect(&target_config.database_type)
            == "sqlserver";
    let same_physical_database = matches!(
        (source_identity.as_deref(), target_identity.as_deref()),
        (Some(source), Some(target)) if source == target
    );
    let source_schema = source_schema_scope
        .and_then(|schema| (!schema.trim().is_empty()).then_some(schema))
        .or_else(|| {
            source_config
                .schema
                .as_deref()
                .filter(|schema| !schema.trim().is_empty())
        });
    let target_schema = target_schema_scope
        .and_then(|schema| (!schema.trim().is_empty()).then_some(schema))
        .or_else(|| {
            target_config
                .schema
                .as_deref()
                .filter(|schema| !schema.trim().is_empty())
        });
    let (source_schema_identity, target_schema_identity) = if is_sqlserver && same_physical_database
    {
        let source_schema_identity = match source_schema {
            Some(schema) => source_driver
                .schema_scope_identity(
                    source_handle,
                    source_config.database.as_deref().unwrap_or_default(),
                    schema,
                )
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let target_schema_identity = match target_schema {
            Some(schema) => target_driver
                .schema_scope_identity(
                    target_handle,
                    target_config.database.as_deref().unwrap_or_default(),
                    schema,
                )
                .await
                .ok()
                .flatten(),
            None => None,
        };
        (source_schema_identity, target_schema_identity)
    } else {
        (None, None)
    };

    match crate::schema_diff::reviewed::physical_database_scope(
        source_config,
        target_config,
        source_identity.as_deref(),
        target_identity.as_deref(),
        source_schema_scope,
        target_schema_scope,
        source_schema_identity.as_deref(),
        target_schema_identity.as_deref(),
    ) {
        crate::schema_diff::reviewed::PhysicalDatabaseScope::Same => {
            Err(reject_same_schema_scope())
        }
        crate::schema_diff::reviewed::PhysicalDatabaseScope::Different => Ok(()),
        crate::schema_diff::reviewed::PhysicalDatabaseScope::Unknown => {
            Err(reject_unverifiable_schema_scope())
        }
    }
}

/// Object-only plans do not have the unified planner's SQL Server schema
/// mapper. Keep their source and target objects in the same configured schema
/// so a plan cannot render source-schema DDL or drop a same-named object from
/// another target schema.
fn sqlserver_object_scope_requirement(
    source_dialect: &str,
    target_dialect: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    source: &[SchemaObjectSnapshot],
    target: &[SchemaObjectSnapshot],
) -> Option<PlanRequirement> {
    if source_dialect != "sqlserver" || target_dialect != "sqlserver" {
        return None;
    }

    // SQL Server's driver-level default schema is dbo when no explicit schema
    // is configured. Object snapshots always carry their catalog schema.
    let source_scope = source_schema
        .map(str::trim)
        .filter(|schema| !schema.is_empty())
        .unwrap_or("dbo");
    let target_scope = target_schema
        .map(str::trim)
        .filter(|schema| !schema.is_empty())
        .unwrap_or("dbo");
    let source_matches_scope = source.iter().all(|object| {
        object.schema.as_deref() == Some(source_scope)
            && object
                .target_schema
                .as_deref()
                .is_none_or(|schema| schema == source_scope)
    });
    let target_matches_scope = target.iter().all(|object| {
        object.schema.as_deref() == Some(target_scope)
            && object
                .target_schema
                .as_deref()
                .is_none_or(|schema| schema == target_scope)
    });

    if source_scope == target_scope && source_matches_scope && target_matches_scope {
        return None;
    }

    Some(PlanRequirement::Unsupported {
        operation: "sqlserver-object-schema-scope".into(),
        reason: format!(
            "SQL Server object-only plans cannot rewrite object definitions across schemas. Source and target objects must both match the same configured schema; source scope is `{source_scope}`, target scope is `{target_scope}`."
        ),
    })
}

fn apply_sqlserver_object_scope_gate(
    plan: &mut SchemaDiffPlan,
    source_dialect: &str,
    target_dialect: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    source: &[SchemaObjectSnapshot],
    target: &[SchemaObjectSnapshot],
) {
    if let Some(requirement) = sqlserver_object_scope_requirement(
        source_dialect,
        target_dialect,
        source_schema,
        target_schema,
        source,
        target,
    ) {
        plan.requirements.push(requirement);
        plan.statements.clear();
    }
}

fn reject_same_schema_scope() -> CommandError {
    CommandError::Validation("Source and target must identify different database scopes".into())
}

fn reject_unverifiable_schema_scope() -> CommandError {
    CommandError::Validation(
        "Cannot verify that source and target are different database scopes; the driver must provide physical database identity".into(),
    )
}

async fn fetch_schema_view(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    object: &datazen_driver_api::DatabaseObject,
) -> Result<SchemaObjectSnapshot, CommandError> {
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "get_object_ddl",
        serde_json::json!({
            "kind": "view",
            "name": object.name,
            "schema": object.schema,
        }),
    )
    .await
    .map_err(CommandError::Driver)?;
    let definition = result
        .data
        .get("ddl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let mysql_view_metadata = result
        .data
        .get("viewMetadata")
        .cloned()
        .map(serde_json::from_value::<datazen_driver_api::MySqlViewMetadata>)
        .transpose()
        .map_err(|error| {
            CommandError::Internal(format!("invalid MySQL view creation metadata: {error}"))
        })?;
    if definition.is_empty() {
        return Err(CommandError::Validation(format!(
            "View {} disappeared while it was being inspected",
            object.name
        )));
    }
    let mut snapshot =
        SchemaObjectSnapshot::view(object.schema.as_deref(), &object.name, &definition);
    snapshot.mysql_view_metadata = mysql_view_metadata;
    Ok(snapshot)
}

async fn list_schema_objects(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    kind: datazen_driver_api::ObjectKind,
) -> Result<Vec<datazen_driver_api::DatabaseObject>, CommandError> {
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "list_objects",
        serde_json::json!({ "kind": kind.as_str() }),
    )
    .await
    .map_err(CommandError::Driver)?;
    result
        .data
        .get("objects")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            CommandError::Internal(format!("invalid {} metadata: {error}", kind.as_str()))
        })
        .map(|objects| objects.unwrap_or_default())
}

async fn fetch_schema_object(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    object: &datazen_driver_api::DatabaseObject,
) -> Result<SchemaObjectSnapshot, CommandError> {
    let kind = datazen_driver_api::ObjectKind::parse(&object.kind).ok_or_else(|| {
        CommandError::Validation(format!("Unsupported schema object kind `{}`", object.kind))
    })?;
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "get_object_ddl",
        serde_json::json!({
            "kind": kind.as_str(),
            "name": object.name,
            "schema": object.schema,
            "signature": object.signature,
            "targetSchema": object.target_schema,
            "targetName": object.target_name,
        }),
    )
    .await
    .map_err(CommandError::Driver)?;
    let definition = result
        .data
        .get("ddl")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let mysql_view_metadata = result
        .data
        .get("viewMetadata")
        .cloned()
        .map(serde_json::from_value::<datazen_driver_api::MySqlViewMetadata>)
        .transpose()
        .map_err(|error| {
            CommandError::Internal(format!("invalid MySQL view creation metadata: {error}"))
        })?;
    if definition.is_empty() {
        return Err(CommandError::Validation(format!(
            "{} {} disappeared while it was being inspected",
            kind.as_str(),
            object.name
        )));
    }
    Ok(match kind {
        datazen_driver_api::ObjectKind::View => {
            let mut snapshot =
                SchemaObjectSnapshot::view(object.schema.as_deref(), &object.name, &definition);
            snapshot.mysql_view_metadata = mysql_view_metadata;
            snapshot
        }
        datazen_driver_api::ObjectKind::Function | datazen_driver_api::ObjectKind::Procedure => {
            SchemaObjectSnapshot::routine(
                kind,
                object.schema.as_deref(),
                &object.name,
                object.signature.as_deref(),
                &definition,
            )
        }
        datazen_driver_api::ObjectKind::Trigger => SchemaObjectSnapshot::trigger(
            object.schema.as_deref(),
            &object.name,
            object.target_schema.as_deref(),
            object.target_name.as_deref().ok_or_else(|| {
                CommandError::Validation(format!("Trigger {} has no target relation", object.name))
            })?,
            &definition,
        ),
        datazen_driver_api::ObjectKind::Sequence => {
            SchemaObjectSnapshot::sequence(object.schema.as_deref(), &object.name, &definition)
        }
        datazen_driver_api::ObjectKind::Type => SchemaObjectSnapshot::type_definition(
            object.schema.as_deref(),
            &object.name,
            &definition,
        ),
        _ => {
            return Err(CommandError::Validation(format!(
                "{} migration is not supported",
                kind.as_str()
            )))
        }
    })
}

fn object_selector(object: &datazen_driver_api::DatabaseObject) -> String {
    object
        .schema
        .as_deref()
        .map(|schema| format!("{schema}.{}", object.name))
        .unwrap_or_else(|| object.name.clone())
}

fn object_selector_with_metadata(object: &datazen_driver_api::DatabaseObject) -> String {
    let base = object_selector(object);
    match datazen_driver_api::ObjectKind::parse(&object.kind) {
        Some(datazen_driver_api::ObjectKind::Function)
        | Some(datazen_driver_api::ObjectKind::Procedure) => object
            .signature
            .as_deref()
            .map(|signature| format!("{base}({signature})"))
            .unwrap_or(base),
        Some(datazen_driver_api::ObjectKind::Trigger) => object
            .target_name
            .as_deref()
            .map(|target| {
                let target = object
                    .target_schema
                    .as_deref()
                    .map(|schema| format!("{schema}.{target}"))
                    .unwrap_or_else(|| target.to_owned());
                format!("{base} ON {target}")
            })
            .unwrap_or(base),
        _ => base,
    }
}

fn select_schema_object_pair(
    source: &[datazen_driver_api::DatabaseObject],
    target: &[datazen_driver_api::DatabaseObject],
    requested: &[String],
) -> Result<
    (
        Vec<datazen_driver_api::DatabaseObject>,
        Vec<datazen_driver_api::DatabaseObject>,
    ),
    CommandError,
> {
    let mut source_selected = Vec::new();
    let mut target_selected = Vec::new();
    for raw in requested {
        let name = raw.trim();
        let source_matches = source
            .iter()
            .filter(|object| {
                object.name == name
                    || object_selector(object) == name
                    || object_selector_with_metadata(object) == name
            })
            .collect::<Vec<_>>();
        let target_matches = target
            .iter()
            .filter(|object| {
                object.name == name
                    || object_selector(object) == name
                    || object_selector_with_metadata(object) == name
            })
            .collect::<Vec<_>>();
        if source_matches.len() > 1
            || target_matches.len() > 1
            || (source_matches.is_empty() && target_matches.is_empty())
        {
            return Err(CommandError::Validation(format!(
                "Schema object selector `{name}` must identify exactly one available object"
            )));
        }
        if let Some(object) = source_matches.first() {
            source_selected.push((*object).clone());
        }
        if let Some(object) = target_matches.first() {
            target_selected.push((*object).clone());
        }
    }
    Ok((source_selected, target_selected))
}

#[tauri::command]
pub async fn get_schema_diff_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<SchemaDiffProfile>, CommandError> {
    Ok(state.store.get_schema_diff_profiles().await)
}

#[tauri::command]
pub async fn save_schema_diff_profile(
    state: State<'_, AppState>,
    mut profile: SchemaDiffProfile,
) -> Result<(), CommandError> {
    profile.validate().map_err(CommandError::Validation)?;
    validate_schema_diff_profile_connections(&state, &profile).await?;
    profile.updated_at = chrono::Utc::now();
    state
        .store
        .save_schema_diff_profile(profile)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

async fn validate_schema_diff_profile_connections(
    state: &AppState,
    profile: &SchemaDiffProfile,
) -> Result<(), CommandError> {
    if state
        .store
        .get_connection(&profile.source_connection_id)
        .await
        .is_none()
    {
        return Err(CommandError::Validation(
            "schema diff profile source connection no longer exists".into(),
        ));
    }
    if state
        .store
        .get_connection(&profile.target_connection_id)
        .await
        .is_none()
    {
        return Err(CommandError::Validation(
            "schema diff profile target connection no longer exists".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_schema_diff_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_schema_diff_profile(&profile_id)
        .await
        .map_err(|error| CommandError::Internal(error.to_string()))
}

fn is_table_missing_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("1146")
        || lower.contains("doesn't exist")
        || lower.contains("does not exist")
        || lower.contains("not found")
}

/// Convert a connection's configured database value to the catalog identifier
/// expected by metadata methods. SQLite stores a file path in the connection
/// config while its single default catalog is named `main`.
pub(super) fn schema_catalog_database<'a>(
    database_type: &str,
    configured_database: Option<&'a str>,
) -> &'a str {
    if normalize_dialect(database_type) == "sqlite" {
        "main"
    } else {
        configured_database.unwrap_or_default()
    }
}

pub(super) fn schema_catalog_scope(
    database_type: &str,
    configured_database: Option<&str>,
) -> Option<String> {
    if normalize_dialect(database_type) == "sqlite" {
        Some("main".into())
    } else {
        configured_database.map(str::to_owned)
    }
}

async fn fetch_target_table_schema(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<crate::db::TableSchema, CommandError> {
    let database = schema_catalog_database(&driver.driver_type(), Some(database));
    match driver
        .get_table_schema(handle, table, database, schema)
        .await
    {
        Ok(schema) => Ok(schema),
        Err(e) => {
            let msg = e.to_string();
            if is_table_missing_error(&msg) {
                tracing::info!(%table, "target table does not exist, treating as empty schema");
                Ok(crate::db::TableSchema {
                    table_name: table.to_string(),
                    columns: Vec::new(),
                    primary_keys: Vec::new(),
                    indexes: Vec::new(),
                    foreign_keys: Vec::new(),
                    check_constraints: Vec::new(),
                    table_options: datazen_driver_api::TableOptions::default(),
                })
            } else {
                Err(CommandError::Driver(e))
            }
        }
    }
}

async fn fetch_target_table_dependency_catalog(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    database: &str,
    dialect: &str,
    schema_scope: Option<&str>,
    selected_snapshots: &[(String, crate::db::TableSchema)],
) -> Result<Vec<(String, crate::db::TableSchema)>, CommandError> {
    const CATALOG_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
    let started_at = std::time::Instant::now();
    let metadata_database = schema_catalog_database(dialect, Some(database));
    let (snapshots, table_count) = tokio::time::timeout(CATALOG_READ_TIMEOUT, async {
        let databases = if normalize_dialect(dialect) == "mysql" {
            if !driver
                .has_complete_foreign_key_catalog_visibility(handle)
                .await
                .map_err(CommandError::Driver)?
            {
                return Err(CommandError::Validation(
                    "MySQL target-only table drops require direct global SELECT or ALL PRIVILEGES on *.* with no partial REVOKE so every database dependency can be inspected; use a suitably privileged connection and retry.".into(),
                ));
            }
            let databases = driver
                .get_databases(handle)
                .await
                .map_err(CommandError::Driver)?;
            if !databases.iter().any(|candidate| candidate == database) {
                return Err(CommandError::Validation(
                    "MySQL reported an incomplete database list despite global metadata visibility; target-only table drops are blocked.".into(),
                ));
            }
            databases
        } else {
            vec![metadata_database.to_string()]
        };
        // This must be a catalog read, not a union with individually fetched
        // selected tables: adding selected snapshots here could make an
        // incomplete catalog look complete and hide an inbound FK dependent.
        let mut snapshots = Vec::new();
        let mut seen = HashSet::new();
        let mut table_count = 0usize;

        for catalog_database in databases {
            let tables = driver
                .get_tables(handle, &catalog_database, None)
                .await
                .map_err(CommandError::Driver)?;
            table_count += tables
                .iter()
                .filter(|table| matches!(&table.table_type, datazen_driver_api::TableType::Table))
                .count();
            for table in tables {
                if !matches!(&table.table_type, datazen_driver_api::TableType::Table) {
                    continue;
                }
                let relation = table
                    .schema
                    .as_deref()
                    .filter(|schema| !schema.is_empty())
                    .map(|schema| format!("{schema}.{}", table.name))
                    .unwrap_or_else(|| table.name.clone());
                let table_identity = if normalize_dialect(dialect) == "mysql"
                    && catalog_database != database
                {
                    format!("{catalog_database}.{relation}")
                } else {
                    relation
                };
                let table_identity =
                    dependency_relation_identity(dialect, &table_identity, schema_scope);
                if !seen.insert(table_identity.clone()) {
                    continue;
                }
                let mut schema = fetch_target_table_schema(
                    driver,
                    handle,
                    &table.name,
                    &catalog_database,
                    table.schema.as_deref(),
                )
                .await?;
                if normalize_dialect(dialect) == "mysql" {
                    for foreign_key in &mut schema.foreign_keys {
                        if let Some((referenced_database, referenced_table)) =
                            foreign_key.referenced_table.split_once('.')
                        {
                            if referenced_database == database {
                                foreign_key.referenced_table = referenced_table.to_string();
                            }
                        }
                    }
                }
                snapshots.push((table_identity, schema));
            }
        }
        for (selected_table, _) in selected_snapshots {
            let mut selected_identity =
                dependency_relation_identity(dialect, selected_table, schema_scope);
            if normalize_dialect(dialect) == "mysql" {
                if let Some((selected_database, relation)) = selected_identity.split_once('.') {
                    if selected_database == database {
                        selected_identity = relation.to_string();
                    }
                }
            }
            if snapshots
                .iter()
                .any(|(identity, _)| identity == &selected_identity)
            {
                continue;
            }

            if uses_schema_scope(dialect) && !selected_table.contains('.') {
                let candidates = snapshots
                    .iter()
                    .filter(|(identity, _)| {
                        identity.rsplit('.').next() == Some(selected_table.as_str())
                            && schema_scope
                                .map(str::trim)
                                .filter(|schema| !schema.is_empty())
                                .map(|schema| identity.starts_with(&format!("{schema}.")))
                                .unwrap_or(true)
                    })
                    .collect::<Vec<_>>();
                match candidates.as_slice() {
                    [_] => continue,
                    [] => {
                        return Err(CommandError::Validation(format!(
                            "Selected target table `{selected_table}` is absent from the complete target dependency catalog"
                        )));
                    }
                    _ => {
                        return Err(CommandError::Validation(format!(
                            "Selected target table `{selected_table}` has an ambiguous schema identity in the complete target dependency catalog"
                        )));
                    }
                }
            }

            return Err(CommandError::Validation(format!(
                "Selected target table `{selected_table}` is absent from the complete target dependency catalog"
            )));
        }

        Ok::<_, CommandError>((snapshots, table_count))
    })
    .await
    .map_err(|_| {
        CommandError::Internal(
            "Reading the target foreign-key dependency catalog exceeded 30 seconds; scope the target selection or retry after reducing catalog load.".into(),
        )
    })??;

    tracing::info!(
        table_count,
        elapsed_ms = started_at.elapsed().as_millis(),
        "Schema Diff target dependency catalog read"
    );
    Ok(snapshots)
}

fn resolve_profile_table(dialect: &str, table: &str, schema: Option<&str>) -> String {
    let Some(schema) = schema.map(str::trim).filter(|value| !value.is_empty()) else {
        return resolve_table_for_dialect(dialect, table);
    };
    let relation = table
        .trim()
        .rsplit_once('.')
        .map(|(_, name)| name)
        .unwrap_or_else(|| table.trim());
    format!("{schema}.{relation}")
}

fn dependency_relation_identity(dialect: &str, table: &str, schema_scope: Option<&str>) -> String {
    let table = table.trim();
    if uses_schema_scope(dialect) && !table.contains('.') {
        if let Some(schema) = schema_scope.filter(|value| !value.trim().is_empty()) {
            return format!("{}.{}", schema.trim(), table);
        }
    }
    table.to_string()
}

fn resolve_reviewed_table_snapshot(
    dialect: &str,
    table: &str,
    database_scope: &str,
    schema_scope: Option<&str>,
) -> (String, String, Option<String>) {
    if uses_schema_scope(dialect) {
        if let Some((schema, relation)) = table.rsplit_once('.') {
            return (
                relation.to_string(),
                database_scope.to_string(),
                Some(schema.to_string()),
            );
        }
    }
    if normalize_dialect(dialect) == "mysql" {
        if let Some((database, relation)) = table.rsplit_once('.') {
            return (relation.to_string(), database.to_string(), None);
        }
    }
    (
        table.to_string(),
        database_scope.to_string(),
        schema_scope.map(str::to_owned),
    )
}

fn has_table_drop_statement(plan: &SchemaDiffPlan) -> bool {
    plan.statements.iter().any(|statement| {
        statement
            .summary
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("DROP TABLE")
    })
}

fn validate_target_dependency_catalog(
    reviewed: &[(String, crate::db::TableSchema)],
    current: &[(String, crate::db::TableSchema)],
) -> Result<(), String> {
    fn catalog_map<'a>(
        snapshots: &'a [(String, crate::db::TableSchema)],
    ) -> Result<BTreeMap<&'a str, &'a crate::db::TableSchema>, String> {
        let mut catalog = BTreeMap::new();
        for (identity, snapshot) in snapshots {
            if catalog.insert(identity.as_str(), snapshot).is_some() {
                return Err(format!(
                    "Target dependency catalog contains duplicate relation identity `{identity}`"
                ));
            }
        }
        Ok(catalog)
    }

    let reviewed = catalog_map(reviewed)?;
    let current = catalog_map(current)?;
    if reviewed.len() != current.len()
        || reviewed
            .keys()
            .any(|identity| !current.contains_key(identity))
    {
        let added = current
            .keys()
            .filter(|identity| !reviewed.contains_key(**identity))
            .copied()
            .collect::<Vec<_>>();
        let removed = reviewed
            .keys()
            .filter(|identity| !current.contains_key(**identity))
            .copied()
            .collect::<Vec<_>>();
        return Err(format!(
            "Target dependency catalog changed after review (new relations: [{}]; removed relations: [{}]); compare again before dropping tables",
            added.join(", "),
            removed.join(", ")
        ));
    }

    for (identity, reviewed_snapshot) in reviewed {
        let current_snapshot = current
            .get(identity)
            .ok_or_else(|| format!("Target relation `{identity}` disappeared after review"))?;
        crate::schema_diff::reviewed::validate_snapshot(
            identity,
            reviewed_snapshot,
            current_snapshot,
        )?;
    }
    Ok(())
}

/// Prepare a DDL deploy plan (source = desired → target).
#[tauri::command]
pub async fn prepare_schema_diff_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Option<Vec<String>>,
    target_only_table_names: Option<Vec<String>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
) -> Result<job::SchemaDiffPrepareEnvelope, CommandError> {
    let target_table_names = target_table_names.unwrap_or_else(|| table_names.clone());
    job::run_prepare_job(
        &state,
        crate::schema_diff::job::PrepareRequest::Table {
            source_db_session_id,
            target_db_session_id,
            table_names,
            target_table_names,
            target_only_table_names: target_only_table_names.unwrap_or_default(),
            source_schema,
            target_schema,
            allow_destructive,
            include_indexes,
            type_overrides: type_overrides.unwrap_or_default(),
        },
    )
    .await
}

/// Prepare a plan using schema overrides stored in a migration profile.
///
/// Interactive callers inherit the live connection schema. Persisted profiles
/// carry both source and target schema, so their runner must keep those scopes
/// explicit when it creates the reviewed immutable plan.
pub(crate) async fn prepare_schema_diff_profile_plan_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_only_table_names: Vec<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<SchemaDiffPlan, CommandError> {
    prepare_schema_diff_plan_with_schemas_impl(
        state,
        source_db_session_id,
        target_db_session_id,
        table_names.clone(),
        table_names,
        target_only_table_names,
        allow_destructive,
        include_indexes,
        type_overrides,
        source_schema,
        target_schema,
    )
    .await
}

pub(crate) async fn prepare_schema_diff_plan_with_schemas_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    table_names: Vec<String>,
    target_table_names: Vec<String>,
    target_only_table_names: Vec<String>,
    allow_destructive: bool,
    include_indexes: Option<bool>,
    type_overrides: Option<Vec<ColumnTypeOverride>>,
    source_schema_override: Option<String>,
    target_schema_override: Option<String>,
) -> Result<SchemaDiffPlan, CommandError> {
    tracing::info!(
        %source_db_session_id,
        %target_db_session_id,
        tables = table_names.len(),
        allow_destructive,
        "prepare_schema_diff_plan"
    );

    if table_names.is_empty() && target_only_table_names.is_empty() {
        return Err(CommandError::Validation(
            "table_names and target_only_table_names must not both be empty".into(),
        ));
    }
    if table_names.len() != target_table_names.len() {
        return Err(CommandError::Validation(
            "table_names and target_table_names must have the same length".into(),
        ));
    }

    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;

    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    let source_schema_scope = source_schema_override
        .as_deref()
        .or(src_config.schema.as_deref());
    let target_schema_scope = target_schema_override
        .as_deref()
        .or(tgt_config.schema.as_deref());
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config)
        && source_schema_scope.map(str::trim) == target_schema_scope.map(str::trim)
    {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_diff_plan")?;
    let target_dependency_schema_scope = if uses_schema_scope(&tgt_config.database_type) {
        target_schema_scope.or(tgt_driver.default_schema())
    } else {
        None
    };
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        source_schema_scope,
        target_schema_scope,
    )
    .await?;

    let mut pairs = Vec::new();
    for (table, target_table) in table_names.iter().zip(&target_table_names) {
        let src_table = resolve_profile_table(
            &src_config.database_type,
            table,
            source_schema_override.as_deref(),
        );
        let tgt_table = resolve_profile_table(
            &tgt_config.database_type,
            target_table,
            target_schema_override.as_deref(),
        );
        let src_schema = src_driver
            .get_table_schema(
                &src_handle,
                &src_table,
                schema_catalog_database(&src_config.database_type, src_config.database.as_deref()),
                source_schema_override
                    .as_deref()
                    .or(src_config.schema.as_deref()),
            )
            .await
            .cmd_err("prepare_schema_diff_plan")?;
        let tgt_schema = fetch_target_table_schema(
            tgt_driver.as_ref(),
            &tgt_handle,
            &tgt_table,
            tgt_config.database.as_deref().unwrap_or_default(),
            target_schema_override
                .as_deref()
                .or(tgt_config.schema.as_deref()),
        )
        .await
        .cmd_err("prepare_schema_diff_plan")?;
        // DDL in the plan targets the target dialect, so the pair's table identifier must be
        // target-resolved. Using `table` here leaks the source's schema qualification into the
        // target DDL (e.g. `public.table` on MySQL) and breaks deploy.
        pairs.push((tgt_table, src_schema, tgt_schema));
    }

    let mut target_only_snapshots = Vec::new();
    let mut target_only_tables = Vec::new();
    let source_target_tables = pairs
        .iter()
        .map(|(table, _, _)| table.as_str())
        .collect::<HashSet<_>>();
    let mut seen_target_only_tables = HashSet::new();
    for table in &target_only_table_names {
        let tgt_table = resolve_profile_table(
            &tgt_config.database_type,
            table,
            target_schema_override.as_deref(),
        );
        // Target-only selection is an explicit destructive request. Read the
        // real target schema and fail closed if the selected table disappeared;
        // do not turn it into an empty source schema sentinel.
        let tgt_schema = tgt_driver
            .get_table_schema(
                &tgt_handle,
                &tgt_table,
                schema_catalog_database(&tgt_config.database_type, tgt_config.database.as_deref()),
                target_schema_override
                    .as_deref()
                    .or(tgt_config.schema.as_deref()),
            )
            .await
            .cmd_err("prepare_schema_diff_plan")?;
        if source_target_tables.contains(tgt_table.as_str()) {
            return Err(CommandError::Validation(format!(
                "Table `{tgt_table}` cannot be selected as both a source table and target-only table"
            )));
        }
        if !seen_target_only_tables.insert(tgt_table.clone()) {
            return Err(CommandError::Validation(format!(
                "Target-only table `{tgt_table}` was selected more than once"
            )));
        }
        target_only_tables.push(tgt_table.clone());
        target_only_snapshots.push((tgt_table, tgt_schema));
    }

    let mut selected_target_snapshots = pairs
        .iter()
        .map(|(table, _, target)| (table.clone(), target.clone()))
        .collect::<Vec<_>>();
    selected_target_snapshots.extend(target_only_snapshots.iter().cloned());

    let (target_dependency_catalog, target_dependency_catalog_error) =
        if allow_destructive && !target_only_tables.is_empty() {
            match fetch_target_table_dependency_catalog(
                tgt_driver.as_ref(),
                &tgt_handle,
                tgt_config.database.as_deref().unwrap_or_default(),
                &tgt_config.database_type,
                target_dependency_schema_scope,
                &selected_target_snapshots,
            )
            .await
            {
                Ok(catalog) => (Some(catalog), None),
                Err(error) => (None, Some(error.to_string())),
            }
        } else {
            (None, None)
        };

    let src_d = normalize_dialect(&src_config.database_type);
    let tgt_d = normalize_dialect(&tgt_config.database_type);
    let include_indexes = include_indexes.unwrap_or(true);

    let mut plan = if src_d != tgt_d {
        state
            .sync_adapters
            .ensure_pair(&src_config.database_type, &tgt_config.database_type)
            .map_err(CommandError::Validation)?;
        let src_adapter: Arc<dyn SyncSourceAdapter> = state
            .sync_adapters
            .get_source(&src_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing source sync adapter".into()))?;
        let tgt_adapter: Arc<dyn SyncTargetAdapter> = state
            .sync_adapters
            .get_target(&tgt_config.database_type)
            .ok_or_else(|| CommandError::Validation("missing target sync adapter".into()))?;

        let tgt_is_mysql = tgt_d == "mysql";
        let mapper = |table: &str, source_type: &str, col_name: &str| -> Result<String, String> {
            for (tgt_tbl, src, _) in pairs.iter().filter(|(t, _, _)| t == table) {
                let matching_col = src
                    .columns
                    .iter()
                    .find(|c| c.name == col_name && c.data_type == source_type)
                    .or_else(|| src.columns.iter().find(|c| c.name == col_name));
                if let Some(col) = matching_col {
                    // 1. User explicit column type overrides take highest priority
                    if let Some(ref overrides) = type_overrides {
                        if let Some(ov) = overrides.iter().find(|o| {
                            resolve_profile_table(
                                &tgt_config.database_type,
                                &o.table,
                                target_schema_override.as_deref(),
                            ) == *tgt_tbl
                                && o.column == col_name
                        }) {
                            return Ok(ov.target_type.clone());
                        }
                    }

                    // 2. MySQL target: unbounded text columns default to pre-filled suggested VARCHAR(255)
                    if tgt_is_mysql && is_source_unbounded_text(&col.data_type) {
                        return Ok("VARCHAR(255)".into());
                    }

                    let ir = src_adapter.column_to_ir(col, Some(source_type));
                    if col.default_value.is_some()
                        && !tgt_adapter.allows_column_default(&ir.ir_type)
                    {
                        if let Some(fallback) = tgt_adapter.default_capable_type_for(&ir.ir_type) {
                            return Ok(tgt_adapter.ir_type_to_native(&fallback));
                        }
                    }
                    return Ok(tgt_adapter.ir_type_to_native(&ir.ir_type));
                }
            }
            Err(format!(
                "cannot map type `{source_type}` for column `{col_name}`"
            ))
        };

        crate::schema_diff::plan::build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
            &pairs,
            &target_only_tables,
            target_dependency_catalog.as_deref(),
            &src_d,
            &tgt_d,
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            )),
            target_dependency_schema_scope,
            PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: Some(&mapper),
                cross_dialect: true,
            },
        )
    } else {
        crate::schema_diff::plan::build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
            &pairs,
            &target_only_tables,
            target_dependency_catalog.as_deref(),
            &src_d,
            &tgt_d,
            Some(schema_catalog_database(
                &tgt_config.database_type,
                tgt_config.database.as_deref(),
            )),
            target_dependency_schema_scope,
            PlanOptions {
                allow_destructive,
                include_indexes,
                type_mapper: None,
                cross_dialect: false,
            },
        )
    };

    if let Some(error) = target_dependency_catalog_error.as_deref() {
        for requirement in &mut plan.requirements {
            if let crate::schema_diff::types::PlanRequirement::Unsupported { operation, reason } =
                requirement
            {
                if operation == "target-only-table-drop-order" {
                    *reason = format!(
                        "Could not inspect the target foreign-key dependency catalog ({error}). Confirm catalog access, select dependent tables, or remove their foreign keys before dropping a parent."
                    );
                }
            }
        }
        plan.statements.clear();
    }

    let has_complete_target_dependency_catalog =
        allow_destructive && !target_only_tables.is_empty() && target_dependency_catalog.is_some();
    let frozen_target_snapshots = target_dependency_catalog
        .clone()
        .unwrap_or_else(|| selected_target_snapshots.clone());

    if has_complete_target_dependency_catalog {
        crate::schema_diff::reviewed::freeze_with_dependency_catalog(
            &mut plan,
            target_db_session_id,
            &tgt_handle,
            &tgt_config,
            frozen_target_snapshots,
            schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
            target_schema_scope.map(str::to_owned),
            target_dependency_schema_scope.map(str::to_owned),
        )
        .await;
    } else {
        crate::schema_diff::reviewed::freeze(
            &mut plan,
            target_db_session_id,
            &tgt_handle,
            &tgt_config,
            frozen_target_snapshots,
            schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
            target_schema_scope.map(str::to_owned),
        )
        .await;
    }

    tracing::info!(
        statements = plan.statements.len(),
        warnings = plan.warnings.len(),
        "prepare_schema_diff_plan OK"
    );
    Ok(plan)
}

/// Prepare a reviewed migration plan for selected views.
///
/// The source and target definitions are read by the backend from live object
/// metadata. The client supplies only qualified object selectors; it never
/// supplies replacement SQL. The returned plan can be deployed through the
/// existing `execute_schema_diff_deploy` command and is guarded by the same
/// one-shot identity and target-snapshot checks as table plans.
#[tauri::command]
pub async fn prepare_schema_view_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
        ));
    }
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_view_plan")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_view_plan")?;
    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_view_plan")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_view_plan")?;
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_views(src_driver.as_ref(), &src_handle).await?;
    let target_available = list_schema_views(tgt_driver.as_ref(), &tgt_handle).await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;

    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_view(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_view(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let target_mysql_view_scope_context = if src_dialect == "mysql" && tgt_dialect == "mysql" {
        unified_plan::read_mysql_view_scope_context(tgt_driver.as_ref(), &tgt_handle).await
    } else {
        None
    };
    let Some(renderer) = tgt_driver.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = tgt_driver.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_view_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    if tgt_dialect == "mysql" {
        for object in source_snapshots.iter().chain(&target_snapshots) {
            if let Err(reason) =
                crate::schema_diff::unified_scope::validate_mysql_view_snapshot_creation_semantics(
                    object,
                    target_mysql_view_scope_context.as_ref(),
                )
            {
                plan.requirements
                    .push(crate::schema_diff::types::PlanRequirement::Unsupported {
                        operation: object.identity().display_key(),
                        reason,
                    });
            }
        }
        if src_config.database.as_deref() != tgt_config.database.as_deref() {
            plan.requirements.push(crate::schema_diff::types::PlanRequirement::Unsupported {
                operation: "mysql-view-scope".into(),
                reason: "Cross-database MySQL view migration requires the unified dependency-reviewed plan; this object-only plan cannot prove source-to-target relation mapping.".into(),
            });
        }
        if !plan.requirements.is_empty() {
            plan.statements.clear();
        }
    }
    crate::schema_diff::reviewed::freeze_with_objects_and_mysql_view_context(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
        target_mysql_view_scope_context,
    )
    .await;
    Ok(plan)
}

/// Prepare a reviewed same-dialect plan for PostgreSQL/MySQL functions,
/// procedures, or triggers. The backend owns object DDL retrieval so callers
/// cannot inject replacement SQL into the migration plan.
#[tauri::command]
pub async fn prepare_schema_routine_trigger_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    kind: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    let kind = datazen_driver_api::ObjectKind::parse(&kind).ok_or_else(|| {
        CommandError::Validation("kind must be function, procedure, or trigger".into())
    })?;
    if !matches!(
        kind,
        datazen_driver_api::ObjectKind::Function
            | datazen_driver_api::ObjectKind::Procedure
            | datazen_driver_api::ObjectKind::Trigger
    ) {
        return Err(CommandError::Validation(
            "kind must be function, procedure, or trigger".into(),
        ));
    }
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
        ));
    }
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_routine_trigger_plan")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_routine_trigger_plan")?;
    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_routine_trigger_plan")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_routine_trigger_plan")?;
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_objects(src_driver.as_ref(), &src_handle, kind).await?;
    let target_available = list_schema_objects(tgt_driver.as_ref(), &tgt_handle, kind).await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;
    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_object(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_object(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let Some(renderer) = tgt_driver.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = tgt_driver.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_routine_trigger_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    crate::schema_diff::reviewed::freeze_with_objects(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
    )
    .await;
    Ok(plan)
}

/// Prepare a reviewed same-dialect plan for PostgreSQL sequences. The client
/// supplies only qualified selectors; both source and target DDL are read
/// from the live driver catalog and frozen for the one-shot deploy gate.
#[tauri::command]
pub async fn prepare_schema_sequence_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
        ));
    }
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_sequence_plan")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_sequence_plan")?;
    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_sequence_plan")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_sequence_plan")?;
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_objects(
        src_driver.as_ref(),
        &src_handle,
        datazen_driver_api::ObjectKind::Sequence,
    )
    .await?;
    let target_available = list_schema_objects(
        tgt_driver.as_ref(),
        &tgt_handle,
        datazen_driver_api::ObjectKind::Sequence,
    )
    .await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;
    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_object(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_object(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let Some(renderer) = tgt_driver.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = tgt_driver.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_sequence_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    crate::schema_diff::reviewed::freeze_with_objects(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
    )
    .await;
    Ok(plan)
}

/// Prepare a reviewed same-dialect plan for user-defined types. The client
/// supplies only qualified selectors; both source and target DDL are read
/// from the live driver catalog and frozen for the one-shot deploy gate.
#[tauri::command]
pub async fn prepare_schema_type_plan(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    object_names: Vec<String>,
    allow_destructive: bool,
) -> Result<SchemaDiffPlan, CommandError> {
    if object_names.is_empty() {
        return Err(CommandError::Validation(
            "object_names must not be empty".into(),
        ));
    }
    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_type_plan")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_type_plan")?;
    if tgt_config.read_only {
        return Err(CommandError::Validation(
            "Target connection is read-only".into(),
        ));
    }
    if crate::schema_diff::reviewed::same_endpoint(&src_config, &tgt_config) {
        return Err(CommandError::Validation(
            "Source and target must identify different database scopes".into(),
        ));
    }
    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("prepare_schema_type_plan")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("prepare_schema_type_plan")?;
    ensure_distinct_schema_scope(
        src_driver.as_ref(),
        &src_handle,
        &src_config,
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_config,
        None,
        None,
    )
    .await?;
    let source_available = list_schema_objects(
        src_driver.as_ref(),
        &src_handle,
        datazen_driver_api::ObjectKind::Type,
    )
    .await?;
    let target_available = list_schema_objects(
        tgt_driver.as_ref(),
        &tgt_handle,
        datazen_driver_api::ObjectKind::Type,
    )
    .await?;
    let (source_selected, target_selected) =
        select_schema_object_pair(&source_available, &target_available, &object_names)?;
    let mut source_snapshots = Vec::with_capacity(source_selected.len());
    for object in &source_selected {
        source_snapshots.push(fetch_schema_object(src_driver.as_ref(), &src_handle, object).await?);
    }
    let mut target_snapshots = Vec::with_capacity(target_selected.len());
    for object in &target_selected {
        target_snapshots.push(fetch_schema_object(tgt_driver.as_ref(), &tgt_handle, object).await?);
    }
    let src_dialect = normalize_dialect(&src_config.database_type);
    let tgt_dialect = normalize_dialect(&tgt_config.database_type);
    let Some(renderer) = tgt_driver.migration_renderer() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration rendering",
            tgt_config.database_type
        )));
    };
    let Some(capabilities) = tgt_driver.migration_capabilities() else {
        return Err(CommandError::Validation(format!(
            "Driver {} does not expose schema migration capabilities",
            tgt_config.database_type
        )));
    };
    let mut plan = build_type_migration_plan_with_components(
        &source_snapshots,
        &target_snapshots,
        &src_dialect,
        &tgt_dialect,
        allow_destructive,
        renderer.as_ref(),
        capabilities.as_ref(),
    );
    apply_sqlserver_object_scope_gate(
        &mut plan,
        &src_dialect,
        &tgt_dialect,
        src_config.schema.as_deref(),
        tgt_config.schema.as_deref(),
        &source_snapshots,
        &target_snapshots,
    );
    crate::schema_diff::reviewed::freeze_with_objects(
        &mut plan,
        target_db_session_id,
        &tgt_handle,
        &tgt_config,
        Vec::new(),
        target_snapshots,
        schema_catalog_scope(&tgt_config.database_type, tgt_config.database.as_deref()),
        tgt_config.schema.clone(),
    )
    .await;
    Ok(plan)
}

/// Execute a reviewed schema diff plan on the target connection.
async fn fail_schema_diff_deploy<T>(
    state: &AppState,
    run: crate::store::MigrationRunRecord,
    error: CommandError,
) -> Result<T, CommandError> {
    crate::commands::history::finish_migration_run(state, run, false, false, 0, 1, 0, "unknown")
        .await;
    Err(error)
}

#[tauri::command]
pub async fn execute_schema_diff_deploy(
    state: State<'_, AppState>,
    target_db_session_id: String,
    plan: SchemaDiffPlan,
    use_transaction: Option<bool>,
    require_rollback: Option<bool>,
    confirm_destructive: Option<String>,
    job_id: Option<String>,
    target_database: Option<String>,
    target_schema: Option<String>,
    profile: Option<crate::store::MigrationProfileRef>,
    plan_id: Option<String>,
    selection_revision: Option<u64>,
) -> Result<SchemaDiffDeployResult, CommandError> {
    let apply_request = crate::schema_diff::job::ApplyRequest {
        target_db_session_id: target_db_session_id.clone(),
        use_transaction: use_transaction.unwrap_or(true),
        require_rollback: require_rollback.unwrap_or(false),
        confirm_destructive,
        job_id,
        target_database: target_database.clone(),
        target_schema: target_schema.clone(),
        profile: profile.map(|p| (p.id, p.revision)),
    };
    let (effective_plan_id, effective_selection_revision) = match (plan_id, selection_revision) {
        (Some(plan_id), Some(sel_rev)) => (plan_id, sel_rev),
        _ => {
            // 兼容路径：客户端直接携带计划正文 ⇒ 先注册进 PlanStore 再跑 apply Job。
            job::register_plan_for_apply(&state, plan, &target_db_session_id)?
        }
    };
    job::run_apply_job(&state, &effective_plan_id, effective_selection_revision, apply_request).await
}

pub(crate) async fn execute_schema_diff_deploy_impl(
    state: &AppState,
    target_db_session_id: String,
    plan: SchemaDiffPlan,
    use_transaction: Option<bool>,
    require_rollback: Option<bool>,
    confirm_destructive: Option<String>,
    job_id: Option<String>,
    target_database: Option<String>,
    target_schema: Option<String>,
    profile: Option<crate::store::MigrationProfileRef>,
) -> Result<SchemaDiffDeployResult, CommandError> {
    crate::commands::history::validate_migration_profile_ref(
        &state,
        "schemaDiff",
        profile.as_ref(),
    )
    .await?;
    let mut history_run =
        crate::commands::history::start_migration_run(&state, "schemaDiff", profile.as_ref()).await;
    history_run.selected_count = plan.statements.len() as u64;
    tracing::info!(
        %target_db_session_id,
        statements = plan.statements.len(),
        ?job_id,
        "execute_schema_diff_deploy"
    );

    let (driver, handle) = match state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("execute_schema_diff_deploy")
    {
        Ok(value) => value,
        Err(error) => {
            return fail_schema_diff_deploy(&state, history_run, error).await;
        }
    };
    let config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("execute_schema_diff_deploy");
    let config = match config {
        Ok(value) => value,
        Err(error) => {
            return fail_schema_diff_deploy(&state, history_run, error).await;
        }
    };
    let owner = match state
        .connection_manager
        .owner_connection_id(&target_db_session_id)
        .await
    {
        Some(value) => value,
        None => {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation("Target connection owner is unavailable".into()),
            )
            .await;
        }
    };
    history_run.target_connection_id = Some(owner.clone());
    let persisted = match state.store.get_connection(&owner).await {
        Some(value) => value,
        None => {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation("Target connection was removed".into()),
            )
            .await;
        }
    };
    if config.read_only || persisted.read_only {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation("Target connection is read-only".into()),
        )
        .await;
    }
    let effective_target_database = target_database
        .as_deref()
        .or(config.database.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let effective_target_schema = target_schema
        .as_deref()
        .or(config.schema.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if require_rollback.unwrap_or(false)
        && (!plan.rollback_completeness.complete
            || !use_transaction.unwrap_or(true)
            || !matches!(
                driver.ddl_atomicity(),
                crate::db::DdlAtomicity::Transactional
            ))
    {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation(
                "Complete rollback requires transactional DDL and a complete rollback plan".into(),
            ),
        )
        .await;
    }
    if !plan.requirements.is_empty() {
        return fail_schema_diff_deploy(
            &state,
            history_run,
            CommandError::Validation(
                "Resolve plan requirements and prepare again before deploying".into(),
            ),
        )
        .await;
    }
    if plan_has_destructive(&plan) {
        let token = confirm_destructive.as_deref().unwrap_or("");
        if token != DESTRUCTIVE_CONFIRM_TOKEN {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation(format!(
                    "destructive plan requires confirm_destructive = \"{DESTRUCTIVE_CONFIRM_TOKEN}\""
                )),
            )
            .await;
        }
    }

    let reviewed = match crate::schema_diff::reviewed::consume(
        &plan,
        &target_db_session_id,
        &handle,
        &config,
        effective_target_database,
        effective_target_schema,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    };
    if has_table_drop_statement(&reviewed.plan) {
        if !reviewed.has_complete_target_dependency_catalog {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation(
                    "Cannot verify the complete target dependency catalog for this table drop; compare again with full catalog visibility".into(),
                ),
            )
            .await;
        }
        let current_catalog = match fetch_target_table_dependency_catalog(
            driver.as_ref(),
            &handle,
            effective_target_database.unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
            &[],
        )
        .await
        {
            Ok(catalog) => catalog,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) =
            validate_target_dependency_catalog(&reviewed.snapshots, &current_catalog)
        {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    } else {
        for (table, snapshot) in &reviewed.snapshots {
            let (relation, snapshot_database, snapshot_schema) = resolve_reviewed_table_snapshot(
                &config.database_type,
                table,
                effective_target_database.unwrap_or_default(),
                effective_target_schema,
            );
            let current = match fetch_target_table_schema(
                driver.as_ref(),
                &handle,
                &relation,
                &snapshot_database,
                snapshot_schema.as_deref(),
            )
            .await
            {
                Ok(value) => value,
                Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
            };
            if let Err(error) =
                crate::schema_diff::reviewed::validate_snapshot(table, snapshot, &current)
            {
                return fail_schema_diff_deploy(
                    &state,
                    history_run,
                    CommandError::Validation(error),
                )
                .await;
            }
        }
    }
    for snapshot in &reviewed.object_snapshots {
        let object = datazen_driver_api::DatabaseObject {
            kind: snapshot.kind.as_str().into(),
            schema: snapshot.schema.clone(),
            name: snapshot.name.clone(),
            signature: snapshot.signature.clone(),
            target_schema: snapshot.target_schema.clone(),
            target_name: snapshot.target_name.clone(),
        };
        let current = match fetch_schema_object(driver.as_ref(), &handle, &object).await {
            Ok(value) => value,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) =
            crate::schema_diff::reviewed::validate_object_snapshot(snapshot, &current)
        {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    }
    if reviewed.has_complete_target_object_catalog {
        let mut current_catalog_complete = true;
        let (_, current_object_catalog) = match unified_plan::read_target_object_catalog(
            driver.as_ref(),
            &handle,
            config.database.as_deref().unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
            &mut current_catalog_complete,
        )
        .await
        {
            value if current_catalog_complete => value,
            _ => {
                return fail_schema_diff_deploy(
                    &state,
                    history_run,
                    CommandError::Validation(
                        "Target object dependency catalog can no longer be proved complete; compare again".into(),
                    ),
                )
                .await
            }
        };
        if let Err(error) = crate::schema_diff::reviewed::validate_object_dependency_catalog(
            &reviewed.object_dependency_catalog,
            &current_object_catalog,
        ) {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
        let current_table_catalog = match unified_plan::read_target_table_identities(
            driver.as_ref(),
            &handle,
            config.database.as_deref().unwrap_or_default(),
            &config.database_type,
            reviewed.target_dependency_schema_scope.as_deref(),
        )
        .await
        {
            Ok(value) => value,
            Err(error) => return fail_schema_diff_deploy(&state, history_run, error).await,
        };
        if let Err(error) = crate::schema_diff::reviewed::validate_table_identity_catalog(
            &reviewed.target_table_identity_catalog,
            &current_table_catalog,
        ) {
            return fail_schema_diff_deploy(&state, history_run, CommandError::Validation(error))
                .await;
        }
    }
    if let Some(expected_context) = reviewed.target_mysql_view_scope_context.as_ref() {
        let current_context =
            unified_plan::read_mysql_view_scope_context(driver.as_ref(), &handle).await;
        if current_context.as_ref() != Some(expected_context) {
            return fail_schema_diff_deploy(
                &state,
                history_run,
                CommandError::Validation(
                    "Target MySQL view creation context changed after review; compare again".into(),
                ),
            )
            .await;
        }
    }
    if let Err(error) = unified_plan::revalidate_source_snapshot(&state, &reviewed).await {
        return fail_schema_diff_deploy(&state, history_run, error).await;
    }
    let plan = reviewed.plan;
    let cancelled = match job_id.as_deref() {
        Some(id) => Some(ensure_job(id).await),
        None => None,
    };

    let opts = DeployOptions {
        use_transaction: use_transaction.unwrap_or(true),
        stop_on_error: true,
    };

    let result = run_schema_diff_deploy(
        driver.as_ref(),
        &handle,
        &plan,
        opts,
        cancelled,
        SqlTarget::new(effective_target_database, effective_target_schema),
    )
    .await;

    if let Some(id) = job_id.as_deref() {
        remove_job(id).await;
    }

    tracing::info!(
        ?result.status,
        executed = result.executed_count,
        "execute_schema_diff_deploy OK"
    );
    let cancelled_outcome = matches!(result.status, crate::schema_diff::DeployStatus::Cancelled);
    let success_outcome = matches!(result.status, crate::schema_diff::DeployStatus::Committed);
    let rollback_outcome = match result.status {
        crate::schema_diff::DeployStatus::RolledBack => "completed",
        crate::schema_diff::DeployStatus::Unknown | crate::schema_diff::DeployStatus::Mixed => {
            "unknown"
        }
        _ => "notRequired",
    };
    crate::commands::history::finish_migration_run(
        &state,
        history_run,
        success_outcome,
        cancelled_outcome,
        result.executed_count as u64,
        result.errors.len() as u64,
        0,
        rollback_outcome,
    )
    .await;
    Ok(result)
}

/// Cancel an in-progress schema diff deploy job.
#[tauri::command]
pub async fn cancel_schema_diff_deploy(job_id: String) -> Result<bool, CommandError> {
    Ok(cancel_job(&job_id).await)
}

/// Compare column-level schema differences for a single table.
pub(crate) async fn compare_table_schemas_impl(
    state: &AppState,
    source_db_session_id: String,
    target_db_session_id: String,
    source_table_name: String,
    target_table_name: String,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%source_db_session_id, %target_db_session_id, %source_table_name, %target_table_name, "compare_table_schemas");

    let src_config = state
        .connection_manager
        .get_session_config(&source_db_session_id)
        .await
        .cmd_err("compare_table_schemas")?;
    let tgt_config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("compare_table_schemas")?;

    let (src_driver, src_handle) = state
        .connection_manager
        .get_session(&source_db_session_id)
        .await
        .cmd_err("compare_table_schemas")?;
    let (tgt_driver, tgt_handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("compare_table_schemas")?;

    let src_table = resolve_table_for_dialect(&src_config.database_type, &source_table_name);
    let tgt_table = resolve_table_for_dialect(&tgt_config.database_type, &target_table_name);

    let src_schema = src_driver
        .get_table_schema(
            &src_handle,
            &src_table,
            schema_catalog_database(&src_config.database_type, src_config.database.as_deref()),
            source_schema.as_deref().or(src_config.schema.as_deref()),
        )
        .await
        .cmd_err("compare_table_schemas")?;
    let tgt_schema = fetch_target_table_schema(
        tgt_driver.as_ref(),
        &tgt_handle,
        &tgt_table,
        tgt_config.database.as_deref().unwrap_or_default(),
        target_schema.as_deref().or(tgt_config.schema.as_deref()),
    )
    .await
    .cmd_err("compare_table_schemas")?;

    // Source = desired: missingOnTarget → ADD, extraOnTarget → DROP.
    // `added`/`removed` kept as aliases for one release.
    let mut source_ddl: Option<String> = None;
    let mut target_ddl: Option<String> = None;
    let mut ir_diff: Option<TableColumnDiff> = None;

    if state
        .sync_adapters
        .ensure_pair(&src_config.database_type, &tgt_config.database_type)
        .is_ok()
    {
        let src_source = state.sync_adapters.get_source(&src_config.database_type);
        let tgt_source = state.sync_adapters.get_source(&tgt_config.database_type);
        let src_target = state.sync_adapters.get_target(&src_config.database_type);
        let tgt_target = state.sync_adapters.get_target(&tgt_config.database_type);

        if let (
            Some(src_adapter),
            Some(tgt_src_adapter),
            Some(src_tgt_adapter),
            Some(tgt_adapter),
        ) = (src_source, tgt_source, src_target, tgt_target)
        {
            let src_full_types = fetch_full_column_types(
                src_adapter.as_ref(),
                src_driver.as_ref(),
                &src_handle,
                &src_table,
            )
            .await
            .ok();
            let tgt_full_types = if tgt_schema.columns.is_empty() {
                None
            } else {
                fetch_full_column_types(
                    tgt_src_adapter.as_ref(),
                    tgt_driver.as_ref(),
                    &tgt_handle,
                    &tgt_table,
                )
                .await
                .ok()
            };

            let src_ir = src_adapter.table_to_ir(&src_schema, src_full_types.as_ref());
            let tgt_ir = tgt_src_adapter.table_to_ir(&tgt_schema, tgt_full_types.as_ref());
            ir_diff = Some(diff_table_schemas_ir(&source_table_name, &src_ir, &tgt_ir));
            source_ddl = Some(build_create_table_ddl(&src_ir, src_tgt_adapter.as_ref()));
            target_ddl = if tgt_ir.columns.is_empty() {
                None
            } else {
                Some(build_create_table_ddl(&tgt_ir, tgt_adapter.as_ref()))
            };
        }
    }

    let src_d = normalize_dialect(&src_config.database_type);
    let tgt_d = normalize_dialect(&tgt_config.database_type);
    let normalizer_holder = if src_d == tgt_d {
        datazen_driver_api::create_driver(&tgt_d).and_then(|d| d.type_normalizer())
    } else {
        None
    };
    let normalizer = normalizer_holder.as_deref();

    let diff = ir_diff.unwrap_or_else(|| {
        diff_table_schemas(&source_table_name, &src_schema, &tgt_schema, normalizer)
    });

    let mut result = serde_json::json!({
        "table": source_table_name,
        "missingOnTarget": diff.missing_on_target,
        "extraOnTarget": diff.extra_on_target,
        "added": diff.added,
        "removed": diff.removed,
        "changed": diff.changed,
    });
    if let Some(ddl) = source_ddl {
        result["sourceDdl"] = serde_json::Value::String(ddl);
    }
    if let Some(ddl) = target_ddl {
        result["targetDdl"] = serde_json::Value::String(ddl);
    }

    tracing::info!(%source_table_name, %target_table_name, "compare_table_schemas OK");
    Ok(result)
}

#[tauri::command]
pub async fn compare_table_schemas(
    state: State<'_, AppState>,
    source_db_session_id: String,
    target_db_session_id: String,
    source_table_name: String,
    target_table_name: String,
    source_schema: Option<String>,
    target_schema: Option<String>,
) -> Result<serde_json::Value, CommandError> {
    compare_table_schemas_impl(
        &state,
        source_db_session_id,
        target_db_session_id,
        source_table_name,
        target_table_name,
        source_schema,
        target_schema,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnSchema, ForeignKeyDeferrability, ForeignKeyInfo, TableInfo, TableType};
    use crate::schema_diff::types::{PlanStatement, RollbackCompleteness, StatementRisk};
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use std::collections::HashMap;

    #[test]
    fn sqlserver_object_only_plan_blocks_cross_schema_statements() {
        let source = vec![SchemaObjectSnapshot::view(
            Some("dbo"),
            "v",
            "SELECT 1 AS value",
        )];
        let target = vec![SchemaObjectSnapshot::view(
            Some("sales"),
            "v",
            "SELECT 2 AS value",
        )];
        let mut plan = SchemaDiffPlan {
            plan_id: None,
            table: "v".into(),
            tables: Vec::new(),
            source_dialect: "sqlserver".into(),
            target_dialect: "sqlserver".into(),
            same_dialect: true,
            statements: vec![PlanStatement {
                sql: "CREATE VIEW [dbo].[v] AS SELECT 1 AS value".into(),
                risk: StatementRisk::Additive,
                rollback_sql: None,
                summary: "create view".into(),
                requires_transaction: false,
            }],
            warnings: Vec::new(),
            requirements: Vec::new(),
            rollback_completeness: RollbackCompleteness {
                complete: false,
                missing: vec!["view rollback is not complete".into()],
            },
            type_suggestions: Vec::new(),
            expected_target_schemas: Vec::new(),
        };

        apply_sqlserver_object_scope_gate(
            &mut plan,
            "sqlserver",
            "sqlserver",
            Some("dbo"),
            Some("sales"),
            &source,
            &target,
        );

        assert!(plan.statements.is_empty());
        assert!(matches!(
            plan.requirements.as_slice(),
            [PlanRequirement::Unsupported { operation, reason }]
                if operation == "sqlserver-object-schema-scope"
                    && reason.contains("cannot rewrite object definitions across schemas")
        ));

        assert!(sqlserver_object_scope_requirement(
            "sqlserver",
            "sqlserver",
            None,
            None,
            &[SchemaObjectSnapshot::view(
                Some("dbo"),
                "v",
                "SELECT 1 AS value",
            )],
            &[SchemaObjectSnapshot::view(
                Some("dbo"),
                "v",
                "SELECT 2 AS value",
            )],
        )
        .is_none());
    }

    fn test_profile() -> SchemaDiffProfile {
        let now = chrono::Utc::now();
        SchemaDiffProfile {
            version: SchemaDiffProfile::CURRENT_VERSION,
            id: "profile-1".into(),
            name: "profile".into(),
            source_connection_id: "source".into(),
            target_connection_id: "target".into(),
            source_database: "app".into(),
            target_database: "app".into(),
            source_schema: None,
            target_schema: None,
            target_only_tables: vec![],
            tables: vec!["users".into()],
            source_objects: vec![],
            target_objects: vec![],
            allow_destructive: false,
            include_indexes: true,
            require_rollback: false,
            type_overrides: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    fn pg_fk(name: &str) -> ForeignKeyInfo {
        ForeignKeyInfo {
            name: name.into(),
            columns: vec!["parent_id".into()],
            referenced_table: "public.parent".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        }
    }

    fn pg_parent_schema(name: &str) -> crate::db::TableSchema {
        crate::db::TableSchema {
            table_name: name.into(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            }],
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    fn pg_child_schema(name: &str, include_fk: bool) -> crate::db::TableSchema {
        crate::db::TableSchema {
            table_name: name.into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                ColumnSchema {
                    name: "parent_id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: if include_fk {
                vec![pg_fk("fk_child_parent")]
            } else {
                Vec::new()
            },
            check_constraints: Vec::new(),
            table_options: Default::default(),
        }
    }

    async fn pg_command_test_sessions(
        options: MockDriverOptions,
    ) -> (crate::testing::app_state::TestAppState, String, String) {
        let test = crate::testing::app_state::TestAppState::with_options(options).await;
        for (id, database) in [("source", "source_db"), ("target", "target_db")] {
            let mut config = crate::testing::app_state::sample_postgres_config(id);
            config.database = Some(database.into());
            config.schema = Some("public".into());
            test.store
                .save_connection(config)
                .await
                .expect("save fixture connection");
        }
        let source_session = test.connect_config("source").await;
        let target_session = test.connect_config("target").await;
        (test, source_session, target_session)
    }

    fn pg_mock_options(
        source_schemas: HashMap<String, crate::db::TableSchema>,
        target_schemas: HashMap<String, crate::db::TableSchema>,
    ) -> MockDriverOptions {
        MockDriverOptions {
            has_schema_level: true,
            default_schema: Some("public"),
            table_schemas_by_database: HashMap::from([
                ("source_db".into(), source_schemas),
                ("target_db".into(), target_schemas),
            ]),
            ..Default::default()
        }
    }

    async fn prepare_pg_table_plan(
        test: &crate::testing::app_state::TestAppState,
        source_session: &str,
        target_session: &str,
        tables: &[&str],
    ) -> SchemaDiffPlan {
        prepare_schema_diff_plan_with_schemas_impl(
            &test.state,
            source_session.to_string(),
            target_session.to_string(),
            tables.iter().map(|table| (*table).into()).collect(),
            tables.iter().map(|table| (*table).into()).collect(),
            Vec::new(),
            false,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("prepare PostgreSQL table plan")
    }

    fn pg_target_drop_options() -> MockDriverOptions {
        let parent_qualified = pg_parent_schema("public.parent");
        let parent_bare = pg_parent_schema("parent");
        let child_qualified = pg_child_schema("public.child", true);
        let child_bare = pg_child_schema("child", true);
        MockDriverOptions {
            has_schema_level: true,
            default_schema: Some("public"),
            tables_by_database: HashMap::from([(
                "target_db".into(),
                vec![
                    target_table("parent", Some("public")),
                    target_table("child", Some("public")),
                ],
            )]),
            table_schemas_by_database: HashMap::from([(
                "target_db".into(),
                HashMap::from([
                    ("parent".into(), parent_bare),
                    ("public.parent".into(), parent_qualified),
                    ("child".into(), child_bare),
                    ("public.child".into(), child_qualified),
                ]),
            )]),
            ..Default::default()
        }
    }

    async fn prepare_pg_target_only_drop_plan(
        test: &crate::testing::app_state::TestAppState,
        source_session: &str,
        target_session: &str,
        tables: &[&str],
    ) -> SchemaDiffPlan {
        prepare_schema_diff_plan_with_schemas_impl(
            &test.state,
            source_session.to_string(),
            target_session.to_string(),
            Vec::new(),
            Vec::new(),
            tables.iter().map(|table| (*table).into()).collect(),
            true,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("prepare PostgreSQL target-only drop plan")
    }

    async fn deploy_pg_reviewed_plan(
        test: &crate::testing::app_state::TestAppState,
        target_session: &str,
        plan: SchemaDiffPlan,
    ) -> Result<crate::schema_diff::SchemaDiffDeployResult, CommandError> {
        execute_schema_diff_deploy_impl(
            &test.state,
            target_session.to_string(),
            plan,
            Some(false),
            Some(false),
            None,
            None,
            None,
            None,
            None,
        )
        .await
    }

    fn target_table(name: &str, schema: Option<&str>) -> TableInfo {
        TableInfo {
            name: name.into(),
            schema: schema.map(str::to_owned),
            table_type: TableType::Table,
            row_count: None,
        }
    }

    #[tokio::test]
    async fn target_dependency_catalog_reads_all_postgres_schemas_and_excludes_views() {
        let options = MockDriverOptions {
            has_schema_level: true,
            tables: vec![
                target_table("parent", Some("public")),
                target_table("child", Some("archive")),
                TableInfo {
                    name: "active_view".into(),
                    schema: Some("public".into()),
                    table_type: TableType::View,
                    row_count: None,
                },
            ],
            table_schemas_by_database: HashMap::from([(
                "target_db".into(),
                HashMap::from([
                    ("parent".into(), pg_parent_schema("parent")),
                    ("child".into(), pg_child_schema("child", true)),
                ]),
            )]),
            ..Default::default()
        };
        let driver = MockDriver::new("postgres", options);
        let handle = datazen_driver_api::ConnectionHandle {
            id: "catalog-session".into(),
            pool_id: "catalog-pool".into(),
        };
        let selected = vec![("public.parent".into(), pg_parent_schema("public.parent"))];
        let catalog = fetch_target_table_dependency_catalog(
            driver.as_ref(),
            &handle,
            "target_db",
            "postgresql",
            Some("public"),
            &selected,
        )
        .await
        .expect("read PostgreSQL dependency catalog");

        assert_eq!(catalog.len(), 2);
        assert!(catalog
            .iter()
            .any(|(identity, _)| identity == "public.parent"));
        let child = catalog
            .iter()
            .find(|(identity, _)| identity == "archive.child")
            .expect("include table in a non-default schema");
        assert_eq!(child.1.foreign_keys[0].referenced_table, "public.parent");
    }

    #[tokio::test]
    async fn mysql_target_dependency_catalog_fails_closed_without_global_visibility() {
        let driver = MockDriver::new("mysql", MockDriverOptions::default());
        let handle = datazen_driver_api::ConnectionHandle {
            id: "mysql-catalog-session".into(),
            pool_id: "mysql-catalog-pool".into(),
        };
        let error = fetch_target_table_dependency_catalog(
            driver.as_ref(),
            &handle,
            "target_db",
            "mysql",
            None,
            &[],
        )
        .await
        .expect_err("unproven server-wide visibility must block target-only drops");
        assert!(error.to_string().contains("direct global SELECT"));
    }

    #[tokio::test]
    async fn mysql_target_dependency_catalog_uses_database_qualified_identities() {
        let mut archive_child = pg_child_schema("child", true);
        archive_child.foreign_keys[0].referenced_table = "archive.parent".into();
        let options = MockDriverOptions {
            databases: vec!["target_db".into(), "archive".into()],
            complete_foreign_key_catalog_visibility: true,
            tables_by_database: HashMap::from([
                (
                    "target_db".into(),
                    vec![target_table("parent", None), target_table("child", None)],
                ),
                (
                    "archive".into(),
                    vec![target_table("parent", None), target_table("child", None)],
                ),
            ]),
            table_schemas_by_database: HashMap::from([
                (
                    "target_db".into(),
                    HashMap::from([
                        ("parent".into(), pg_parent_schema("parent")),
                        ("child".into(), archive_child),
                    ]),
                ),
                (
                    "archive".into(),
                    HashMap::from([
                        ("parent".into(), pg_parent_schema("parent")),
                        ("child".into(), pg_child_schema("child", false)),
                    ]),
                ),
            ]),
            ..Default::default()
        };
        let driver = MockDriver::new("mysql", options);
        let handle = datazen_driver_api::ConnectionHandle {
            id: "mysql-catalog-session".into(),
            pool_id: "mysql-catalog-pool".into(),
        };
        let catalog = fetch_target_table_dependency_catalog(
            driver.as_ref(),
            &handle,
            "target_db",
            "mysql",
            None,
            &[],
        )
        .await
        .expect("read server-wide MySQL dependency catalog");

        assert!(catalog.iter().any(|(identity, _)| identity == "parent"));
        assert!(catalog
            .iter()
            .any(|(identity, _)| identity == "archive.parent"));
        let child = catalog
            .iter()
            .find(|(identity, _)| identity == "child")
            .expect("include selected database child");
        assert_eq!(child.1.foreign_keys[0].referenced_table, "archive.parent");
        assert_eq!(catalog.len(), 4);
    }

    #[tokio::test]
    async fn postgres_create_with_foreign_key_deploy_accepts_unchanged_reviewed_snapshots() {
        let options = pg_mock_options(
            HashMap::from([
                ("public.parent".into(), pg_parent_schema("public.parent")),
                ("public.child".into(), pg_child_schema("public.child", true)),
            ]),
            HashMap::new(),
        );
        let (test, source_session, target_session) = pg_command_test_sessions(options).await;
        let plan = prepare_pg_table_plan(
            &test,
            &source_session,
            &target_session,
            &["public.parent", "public.child"],
        )
        .await;
        assert!(plan.statements.iter().any(|statement| {
            statement.summary.contains("CREATE TABLE")
                && statement.summary.contains("public.parent")
        }));
        assert!(plan.statements.iter().any(|statement| {
            statement.summary.contains("FOREIGN KEY") || statement.sql.contains("FOREIGN KEY")
        }));

        let result = deploy_pg_reviewed_plan(&test, &target_session, plan)
            .await
            .expect("unchanged create/FK plan should deploy");
        assert_eq!(result.status, crate::schema_diff::DeployStatus::Committed);
        assert!(result.executed_count > 0);
        assert!(test.mock.execute_calls() > 0);
    }

    #[tokio::test]
    async fn postgres_add_fk_deploy_accepts_unchanged_reviewed_snapshots() {
        let source_child = pg_child_schema("public.child", true);
        let target_child = pg_child_schema("public.child", false);
        let options = pg_mock_options(
            HashMap::from([
                ("public.parent".into(), pg_parent_schema("public.parent")),
                ("public.child".into(), source_child),
            ]),
            HashMap::from([
                ("public.parent".into(), pg_parent_schema("public.parent")),
                ("parent".into(), pg_parent_schema("parent")),
                ("public.child".into(), target_child.clone()),
                ("child".into(), target_child),
            ]),
        );
        let (test, source_session, target_session) = pg_command_test_sessions(options).await;
        let plan = prepare_pg_table_plan(
            &test,
            &source_session,
            &target_session,
            &["public.parent", "public.child"],
        )
        .await;
        assert!(plan.statements.iter().any(|statement| {
            statement.summary.contains("FOREIGN KEY") || statement.sql.contains("FOREIGN KEY")
        }));

        let result = deploy_pg_reviewed_plan(&test, &target_session, plan)
            .await
            .expect("unchanged add-FK plan should deploy");
        assert_eq!(result.status, crate::schema_diff::DeployStatus::Committed);
        assert!(result.executed_count > 0);
    }

    #[tokio::test]
    async fn postgres_target_mutation_after_review_is_rejected_before_any_write() {
        let target_child = pg_child_schema("public.child", false);
        let options = pg_mock_options(
            HashMap::from([
                ("public.parent".into(), pg_parent_schema("public.parent")),
                ("public.child".into(), pg_child_schema("public.child", true)),
            ]),
            HashMap::from([
                ("public.parent".into(), pg_parent_schema("public.parent")),
                ("parent".into(), pg_parent_schema("parent")),
                ("public.child".into(), target_child.clone()),
                ("child".into(), target_child),
            ]),
        );
        let (test, source_session, target_session) = pg_command_test_sessions(options).await;
        let plan = prepare_pg_table_plan(
            &test,
            &source_session,
            &target_session,
            &["public.parent", "public.child"],
        )
        .await;

        test.mock
            .set_table_schema_for_test("target_db", "child", pg_child_schema("child", true));
        let error = deploy_pg_reviewed_plan(&test, &target_session, plan)
            .await
            .expect_err("a post-review FK change must invalidate the plan");
        assert!(error
            .to_string()
            .contains("Target schema changed for public.child"));
        assert_eq!(test.mock.execute_calls(), 0);
    }

    #[tokio::test]
    async fn postgres_child_parent_drop_revalidates_qualified_catalog_before_deploy() {
        let (test, source_session, target_session) =
            pg_command_test_sessions(pg_target_drop_options()).await;
        let plan = prepare_pg_target_only_drop_plan(
            &test,
            &source_session,
            &target_session,
            &["public.parent", "public.child"],
        )
        .await;
        assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
        assert_eq!(plan.statements.len(), 2);
        let child_drop = plan
            .statements
            .iter()
            .position(|statement| statement.sql.contains("child"))
            .expect("render child drop");
        let parent_drop = plan
            .statements
            .iter()
            .position(|statement| statement.sql.contains("parent"))
            .expect("render parent drop");
        assert!(child_drop < parent_drop, "{:?}", plan.statements);

        let result = execute_schema_diff_deploy_impl(
            &test.state,
            target_session,
            plan,
            Some(false),
            Some(false),
            Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN.to_string()),
            None,
            None,
            None,
            None,
        )
        .await
        .expect("unchanged full target catalog should validate");
        assert_eq!(result.status, crate::schema_diff::DeployStatus::Committed);
        assert_eq!(result.executed_count, 2);
    }

    #[tokio::test]
    async fn postgres_new_dependent_relation_after_review_blocks_drop_before_write() {
        let (test, source_session, target_session) =
            pg_command_test_sessions(pg_target_drop_options()).await;
        let plan = prepare_pg_target_only_drop_plan(
            &test,
            &source_session,
            &target_session,
            &["public.parent", "public.child"],
        )
        .await;
        test.mock
            .add_table_for_test("target_db", target_table("late_child", Some("public")));

        let error = execute_schema_diff_deploy_impl(
            &test.state,
            target_session,
            plan,
            Some(false),
            Some(false),
            Some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN.to_string()),
            None,
            None,
            None,
            None,
        )
        .await
        .expect_err("a new dependency catalog relation must invalidate the drop plan");
        assert!(error
            .to_string()
            .contains("Target dependency catalog changed after review"));
        assert!(error.to_string().contains("public.late_child"));
        assert_eq!(test.mock.execute_calls(), 0);
    }

    #[test]
    fn is_table_missing_error_detects_various_patterns() {
        assert!(is_table_missing_error(
            "Query failed: error returned from database: 1146 (42S02): Table 'datazen_demo.demo_customers' doesn't exist"
        ));
        assert!(is_table_missing_error(
            "relation \"public.demo_customers\" does not exist"
        ));
        assert!(is_table_missing_error("Table 'users' doesn't exist"));
        assert!(is_table_missing_error("Table not found: users"));
        assert!(!is_table_missing_error("Connection refused"));
        assert!(!is_table_missing_error("Syntax error in SQL statement"));
    }

    #[test]
    fn reviewed_relation_snapshot_uses_its_own_schema_scope() {
        assert_eq!(
            resolve_reviewed_table_snapshot("postgresql", "archive.events", "app", Some("public")),
            ("events".into(), "app".into(), Some("archive".into()))
        );
        assert_eq!(
            resolve_reviewed_table_snapshot("postgresql", "events", "app", Some("public")),
            ("events".into(), "app".into(), Some("public".into()))
        );
        assert_eq!(
            resolve_reviewed_table_snapshot("sqlserver", "sales.orders", "app", Some("dbo")),
            ("orders".into(), "app".into(), Some("sales".into()))
        );
        assert_eq!(
            resolve_reviewed_table_snapshot("sqlserver", "orders", "app", Some("dbo")),
            ("orders".into(), "app".into(), Some("dbo".into()))
        );
        assert_eq!(
            resolve_reviewed_table_snapshot("mysql", "events", "app", None),
            ("events".into(), "app".into(), None)
        );
        assert_eq!(
            resolve_reviewed_table_snapshot("mysql", "archive.events", "app", None),
            ("events".into(), "archive".into(), None)
        );
    }

    #[test]
    fn complete_target_dependency_catalog_rejects_new_relations_and_changed_foreign_keys() {
        let table = |name: &str| crate::db::TableSchema {
            table_name: name.into(),
            columns: Vec::new(),
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        };
        let reviewed = vec![("public.parent".into(), table("parent"))];
        let mut current = reviewed.clone();
        current.push(("public.child".into(), table("child")));
        let error = validate_target_dependency_catalog(&reviewed, &current).unwrap_err();
        assert!(error.contains("new relations: [public.child]"), "{error}");

        let mut reviewed = current;
        let mut changed_child = table("child");
        changed_child.foreign_keys.push(crate::db::ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: "public.parent".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: crate::db::ForeignKeyDeferrability::NotDeferrable,
        });
        let changed = vec![
            ("public.parent".into(), table("parent")),
            ("public.child".into(), changed_child),
        ];
        let error = validate_target_dependency_catalog(&reviewed, &changed).unwrap_err();
        assert!(
            error.contains("Target schema changed for public.child"),
            "{error}"
        );
        reviewed.pop();
    }

    #[test]
    fn test_tester_schema_object_selectors_require_unambiguous_identity_and_allow_target_only_drop()
    {
        let source = vec![datazen_driver_api::DatabaseObject {
            kind: "view".into(),
            schema: Some("public".into()),
            name: "active_users".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        }];
        let target = vec![datazen_driver_api::DatabaseObject {
            kind: "view".into(),
            schema: Some("public".into()),
            name: "legacy_users".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        }];
        let (source_selected, target_selected) =
            select_schema_object_pair(&source, &target, &["public.active_users".into()]).unwrap();
        assert_eq!(source_selected.len(), 1);
        assert!(target_selected.is_empty());
        let (source_selected, target_selected) =
            select_schema_object_pair(&source, &target, &["public.legacy_users".into()]).unwrap();
        assert!(source_selected.is_empty());
        assert_eq!(target_selected.len(), 1);

        let ambiguous = vec![
            datazen_driver_api::DatabaseObject {
                kind: "view".into(),
                schema: Some("one".into()),
                name: "same".into(),
                signature: None,
                target_schema: None,
                target_name: None,
            },
            datazen_driver_api::DatabaseObject {
                kind: "view".into(),
                schema: Some("two".into()),
                name: "same".into(),
                signature: None,
                target_schema: None,
                target_name: None,
            },
        ];
        assert!(select_schema_object_pair(&ambiguous, &[], &["same".into()]).is_err());
    }

    #[test]
    fn test_tester_schema_object_selectors_preserve_overloads_and_trigger_relations() {
        let functions = vec![
            datazen_driver_api::DatabaseObject {
                kind: "function".into(),
                schema: Some("public".into()),
                name: "lookup".into(),
                signature: Some("integer".into()),
                target_schema: None,
                target_name: None,
            },
            datazen_driver_api::DatabaseObject {
                kind: "function".into(),
                schema: Some("public".into()),
                name: "lookup".into(),
                signature: Some("text".into()),
                target_schema: None,
                target_name: None,
            },
        ];
        let (source, target) =
            select_schema_object_pair(&functions, &functions, &["public.lookup(integer)".into()])
                .unwrap();
        assert_eq!(source.len(), 1);
        assert_eq!(target.len(), 1);
        assert_eq!(source[0].signature.as_deref(), Some("integer"));
        assert!(select_schema_object_pair(&functions, &functions, &["lookup".into()]).is_err());

        let triggers = vec![
            datazen_driver_api::DatabaseObject {
                kind: "trigger".into(),
                schema: Some("public".into()),
                name: "audit".into(),
                signature: None,
                target_schema: Some("public".into()),
                target_name: Some("orders".into()),
            },
            datazen_driver_api::DatabaseObject {
                kind: "trigger".into(),
                schema: Some("public".into()),
                name: "audit".into(),
                signature: None,
                target_schema: Some("public".into()),
                target_name: Some("invoices".into()),
            },
        ];
        let (source, target) = select_schema_object_pair(
            &triggers,
            &triggers,
            &["public.audit ON public.orders".into()],
        )
        .unwrap();
        assert_eq!(source.len(), 1);
        assert_eq!(target.len(), 1);
        assert_eq!(source[0].target_name.as_deref(), Some("orders"));
        assert!(select_schema_object_pair(&triggers, &triggers, &["audit".into()]).is_err());
    }

    #[tokio::test]
    async fn schema_diff_profile_save_requires_existing_connections() {
        let test = crate::testing::app_state::TestAppState::new().await;
        test.save_connection("source").await;
        let profile = test_profile();
        let error = validate_schema_diff_profile_connections(&test.state, &profile)
            .await
            .expect_err("missing target should be rejected");
        assert!(error.to_string().contains("target connection"));

        test.save_connection("target").await;
        assert!(
            validate_schema_diff_profile_connections(&test.state, &profile)
                .await
                .is_ok()
        );
    }

    #[test]
    fn sqlite_metadata_uses_catalog_name_instead_of_configured_file_path() {
        assert_eq!(
            schema_catalog_database("sqlite", Some("/tmp/source.sqlite")),
            "main"
        );
        assert_eq!(schema_catalog_database("postgresql", Some("app")), "app");
        assert_eq!(schema_catalog_database("sqlserver", Some("app")), "app");
        assert_eq!(schema_catalog_database("mysql", None), "");
        assert_eq!(
            schema_catalog_scope("sqlite", Some("/tmp/target.sqlite")),
            Some("main".into())
        );
        assert_eq!(
            schema_catalog_scope("postgresql", Some("app")),
            Some("app".into())
        );
    }
}
