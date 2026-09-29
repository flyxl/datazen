//! Catalog reads and structured dependency snapshots for the unified plan.

use super::*;
use crate::schema_diff::object_identity::{
    SchemaObjectDependencySnapshot, SchemaObjectIdentity, SequenceDependencyUsage,
    SequenceDependencyUsageKind, TypeDependencyUsage, TypeDependencyUsageKind,
};
use datazen_driver_api::{DatabaseObject, ObjectKind, TableType};
use std::collections::HashSet;

pub(crate) async fn read_target_object_catalog(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    database: &str,
    dialect: &str,
    schema_scope: Option<&str>,
    complete: &mut bool,
) -> (Vec<DatabaseObject>, Vec<SchemaObjectDependencySnapshot>) {
    let metadata_database = super::schema_catalog_database(dialect, Some(database));
    let mut objects = Vec::new();
    for kind in UNIFIED_OBJECT_KINDS {
        match list_schema_objects(driver, handle, kind).await {
            Ok(list) => objects.extend(list),
            Err(error) => {
                *complete = false;
                tracing::warn!(kind = kind.as_str(), %error, "Unified target object catalog is incomplete");
            }
        }
    }
    let mut dependency_objects = objects.clone();
    match driver.get_tables(handle, metadata_database, None).await {
        Ok(tables) => {
            dependency_objects.extend(
                tables
                    .into_iter()
                    .filter(|table| matches!(table.table_type, TableType::Table))
                    .map(|table| DatabaseObject {
                        kind: ObjectKind::Table.as_str().into(),
                        schema: table.schema.or_else(|| {
                            if uses_schema_scope(dialect) {
                                schema_scope.map(str::to_owned)
                            } else {
                                Some(metadata_database.to_owned())
                            }
                        }),
                        name: table.name,
                        signature: None,
                        target_schema: None,
                        target_name: None,
                    }),
            );
        }
        Err(error) => {
            *complete = false;
            tracing::warn!(%error, "Unified target table catalog is incomplete");
        }
    }
    let mut identities = HashSet::new();
    let mut snapshots = Vec::with_capacity(dependency_objects.len());
    for object in &dependency_objects {
        if ObjectKind::parse(&object.kind).is_none() {
            *complete = false;
            continue;
        }
        let identity = object_identity(object);
        if !identities.insert(identity.clone()) {
            *complete = false;
            continue;
        }
        let snapshot = fetch_object_dependency_snapshot(driver, handle, object).await;
        snapshots.push(snapshot.unwrap_or_else(|| SchemaObjectDependencySnapshot {
            identity,
            dependencies: None,
            type_dependency_usages: None,
            sequence_dependency_usages: None,
        }));
    }
    (objects, snapshots)
}

pub(super) async fn fetch_object_snapshot(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    object: &DatabaseObject,
) -> Result<SchemaObjectSnapshot, CommandError> {
    let mut snapshot = fetch_schema_object(driver, handle, object).await?;
    if let Some(dependencies) = fetch_object_dependency_snapshot(driver, handle, object).await {
        snapshot.dependencies = dependencies.dependencies;
        snapshot.sequence_dependency_usages = dependencies.sequence_dependency_usages;
    }
    Ok(snapshot)
}

pub(super) async fn fetch_object_dependency_snapshot(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    object: &DatabaseObject,
) -> Option<SchemaObjectDependencySnapshot> {
    let result = datazen_driver_api::execute_schema_object_command(
        driver,
        &driver.driver_type(),
        handle,
        "get_object_dependencies",
        serde_json::json!({
            "kind": object.kind,
            "name": object.name,
            "schema": object.schema,
            "signature": object.signature,
            "targetSchema": object.target_schema,
            "targetName": object.target_name,
        }),
    )
    .await
    .ok()?;
    if result
        .data
        .get("complete")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return None;
    }
    let dependencies =
        serde_json::from_value::<Vec<DatabaseObject>>(result.data.get("dependencies")?.clone())
            .ok()?;
    let mut identities = dependencies
        .iter()
        .map(strict_object_identity)
        .collect::<Option<Vec<_>>>()?;
    identities.sort();
    identities.dedup();
    let type_dependency_usages = match result.data.get("typeDependencyUsages") {
        Some(value) => {
            let usages =
                serde_json::from_value::<Vec<TypeDependencyUsageWire>>(value.clone()).ok()?;
            let mut parsed = Vec::with_capacity(usages.len());
            for usage in usages {
                let kind = match usage.usage.as_str() {
                    "column_type" => TypeDependencyUsageKind::ColumnType,
                    "expression" => TypeDependencyUsageKind::Expression,
                    "constraint" => TypeDependencyUsageKind::Constraint,
                    _ => return None,
                };
                let column_name = match kind {
                    TypeDependencyUsageKind::ColumnType => {
                        let column = usage.column_name.filter(|name| !name.trim().is_empty())?;
                        Some(column)
                    }
                    TypeDependencyUsageKind::Expression | TypeDependencyUsageKind::Constraint => {
                        if usage.column_name.is_some() {
                            return None;
                        }
                        None
                    }
                };
                parsed.push(TypeDependencyUsage {
                    dependency: strict_object_identity(&usage.dependency)?,
                    usage: kind,
                    column_name,
                });
            }
            parsed.sort();
            parsed.dedup();
            if parsed.iter().any(|usage| {
                usage.dependency.kind != ObjectKind::Type || !identities.contains(&usage.dependency)
            }) {
                return None;
            }
            Some(parsed)
        }
        None => None,
    };
    let sequence_dependency_usages = match result.data.get("sequenceDependencyUsages") {
        Some(value) => {
            let usages =
                serde_json::from_value::<Vec<SequenceDependencyUsageWire>>(value.clone()).ok()?;
            let mut parsed = Vec::with_capacity(usages.len());
            for usage in usages {
                let sequence = strict_object_identity(&usage.sequence)?;
                let owner_table = strict_object_identity(&usage.owner_table)?;
                let column_name = usage.column_name.filter(|name| !name.trim().is_empty())?;
                if sequence.kind != ObjectKind::Sequence || owner_table.kind != ObjectKind::Table {
                    return None;
                }
                let usage_kind = match usage.usage.as_str() {
                    "column_default" => {
                        if object_identity(object).kind != ObjectKind::Table
                            || owner_table != object_identity(object)
                            || !identities.contains(&sequence)
                        {
                            return None;
                        }
                        SequenceDependencyUsageKind::ColumnDefault
                    }
                    "owned_by" => {
                        if object_identity(object).kind != ObjectKind::Sequence
                            || sequence != object_identity(object)
                            || !identities.contains(&owner_table)
                        {
                            return None;
                        }
                        SequenceDependencyUsageKind::OwnedBy
                    }
                    _ => return None,
                };
                parsed.push(SequenceDependencyUsage {
                    sequence,
                    owner_table,
                    column_name,
                    usage: usage_kind,
                });
            }
            parsed.sort();
            parsed.dedup();
            Some(parsed)
        }
        None => None,
    };
    Some(SchemaObjectDependencySnapshot {
        identity: object_identity(object),
        dependencies: Some(identities),
        type_dependency_usages,
        sequence_dependency_usages,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeDependencyUsageWire {
    dependency: DatabaseObject,
    usage: String,
    column_name: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SequenceDependencyUsageWire {
    sequence: DatabaseObject,
    owner_table: DatabaseObject,
    column_name: Option<String>,
    usage: String,
}

fn strict_object_identity(object: &DatabaseObject) -> Option<SchemaObjectIdentity> {
    Some(SchemaObjectIdentity {
        kind: ObjectKind::parse(&object.kind)?,
        schema: object.schema.clone(),
        name: object.name.clone(),
        signature: object.signature.clone(),
        target_schema: object.target_schema.clone(),
        target_name: object.target_name.clone(),
    })
}
