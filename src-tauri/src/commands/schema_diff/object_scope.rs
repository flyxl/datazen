//! Schema object scope gating and object selection helpers.
//!
//! The unified planner resolves schema scope before reading source and target
//! objects, and these helpers also resolve the qualified selectors the client
//! sends for views, routines, sequences and types.

use super::{CommandError, PlanRequirement, SchemaDiffPlan, SchemaObjectSnapshot};

pub(super) async fn ensure_distinct_schema_scope(
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
pub(super) fn sqlserver_object_scope_requirement(
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

pub(super) fn apply_sqlserver_object_scope_gate(
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

pub(super) async fn list_schema_objects(
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

pub(super) async fn fetch_schema_object(
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

pub(super) fn select_schema_object_pair(
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
