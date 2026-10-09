//! Driver-owned schema-object mapping into the target database/schema scope.

use super::{
    object_identity::SchemaObjectIdentity, objects::SchemaObjectSnapshot, types::PlanRequirement,
};
use datazen_driver_api::TableSchema;
use datazen_driver_api::{
    DatabaseObject, MigrationRenderer, MySqlViewMetadata, ObjectKind, SchemaObjectScopeDependency,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MySqlViewScopeContext {
    pub current_user: String,
    pub character_set_client: String,
    pub collation_connection: String,
}

pub(super) fn map_source_objects_for_target_scope(
    source: &[SchemaObjectSnapshot],
    table_pairs: &[(String, TableSchema, TableSchema)],
    target_dialect: &str,
    configured_source_scope: Option<&str>,
    target_scope: Option<&str>,
    target_view_context: Option<&MySqlViewScopeContext>,
    renderer: &dyn MigrationRenderer,
) -> (Vec<SchemaObjectSnapshot>, Vec<PlanRequirement>) {
    let mut mapped = Vec::with_capacity(source.len());
    let mut requirements = Vec::new();
    if !matches!(target_dialect, "mysql" | "postgresql" | "sqlserver") {
        return (source.to_vec(), requirements);
    }
    for object in source {
        let Some(source_scope) = configured_source_scope else {
            if target_scope.is_some() {
                requirements.push(unsupported(
                    object,
                    "The configured source schema/database scope is unavailable, so object DDL cannot be mapped into the target scope.",
                ));
            }
            mapped.push(object.clone());
            continue;
        };
        if object.schema.as_deref() != Some(source_scope) {
            requirements.push(unsupported(
                object,
                format!(
                    "Source object scope `{}` does not match configured source scope `{source_scope}`; external or mismatched objects cannot be migrated through this plan.",
                    object.schema.as_deref().unwrap_or_default()
                ),
            ));
            mapped.push(object.clone());
            continue;
        }
        let Some(target_scope) = target_scope else {
            requirements.push(unsupported(
                object,
                "The target schema/database scope is unknown; object DDL cannot be safely rendered.",
            ));
            mapped.push(object.clone());
            continue;
        };
        if target_dialect == "mysql" && object.kind == ObjectKind::View {
            let metadata_error = object
                .mysql_view_metadata
                .as_ref()
                .ok_or_else(|| {
                    "The source driver did not provide MySQL view creation metadata; migration cannot preserve its algorithm, columns, security, or collation semantics.".to_string()
                })
                .and_then(|metadata| {
                    let context = target_view_context.ok_or_else(|| {
                        "The target connection could not prove its MySQL view creation context; compare again with session metadata visibility.".to_string()
                    })?;
                    validate_view_creation_semantics(metadata, context)
                });
            if let Err(reason) = metadata_error {
                requirements.push(unsupported(object, reason));
                mapped.push(object.clone());
                continue;
            }
        }
        if source_scope == target_scope {
            mapped.push(object.clone());
            continue;
        }
        if target_dialect != "mysql" || object.kind != ObjectKind::View {
            requirements.push(unsupported(
                object,
                format!(
                    "Cross-scope migration is only proven for MySQL views; `{}` remains fail-closed.",
                    object.kind.as_str()
                ),
            ));
            mapped.push(object.clone());
            continue;
        }
        match map_mysql_view(
            object,
            table_pairs,
            source_scope,
            target_scope,
            target_view_context,
            renderer,
        ) {
            Ok(snapshot) => mapped.push(snapshot),
            Err(reason) => {
                requirements.push(unsupported(object, reason));
                mapped.push(object.clone());
            }
        }
    }
    (mapped, requirements)
}

fn map_mysql_view(
    object: &SchemaObjectSnapshot,
    table_pairs: &[(String, TableSchema, TableSchema)],
    source_scope: &str,
    target_scope: &str,
    target_view_context: Option<&MySqlViewScopeContext>,
    renderer: &dyn MigrationRenderer,
) -> Result<SchemaObjectSnapshot, String> {
    let metadata = object.mysql_view_metadata.as_ref().ok_or_else(|| {
        "The source driver did not provide MySQL view creation metadata; cross-database mapping cannot preserve its security or collation semantics.".to_string()
    })?;
    let target_context = target_view_context.ok_or_else(|| {
        "The target connection could not prove its MySQL view creation context; compare again with catalog and session metadata visibility.".to_string()
    })?;
    validate_view_creation_semantics(metadata, target_context)?;
    let dependencies = object.dependencies.as_ref().ok_or_else(|| {
        "The source driver could not prove a complete view dependency set; enable full catalog visibility and compare again.".to_string()
    })?;
    let mut pairs = Vec::with_capacity(dependencies.len());
    let mut expected_targets = BTreeSet::new();
    for dependency in dependencies {
        if !matches!(dependency.kind, ObjectKind::Table | ObjectKind::View)
            || dependency.name.trim().is_empty()
            || dependency.signature.is_some()
            || dependency.target_schema.is_some()
            || dependency.target_name.is_some()
        {
            return Err(format!(
                "View dependency `{}` is outside the proven MySQL table/view scope-mapping subset.",
                dependency.display_key()
            ));
        }
        let target = if dependency.schema.as_deref() == Some(source_scope) {
            let target_name = if dependency.kind == ObjectKind::Table {
                let matches = table_pairs
                    .iter()
                    .filter(|(_, source, _)| source.table_name == dependency.name)
                    .collect::<Vec<_>>();
                if matches.len() > 1 {
                    return Err(format!(
                        "View dependency `{}` matches multiple selected table mappings.",
                        dependency.display_key()
                    ));
                }
                matches
                    .first()
                    .map(|(_, _, target)| target.table_name.as_str())
                    .unwrap_or(&dependency.name)
            } else {
                &dependency.name
            };
            SchemaObjectIdentity {
                kind: dependency.kind,
                schema: Some(target_scope.to_owned()),
                name: target_name.to_owned(),
                signature: None,
                target_schema: None,
                target_name: None,
            }
        } else {
            dependency.clone()
        };
        if !expected_targets.insert(target.clone()) {
            return Err(format!(
                "Multiple source view dependencies map to the same target identity `{}`.",
                target.display_key()
            ));
        }
        pairs.push(SchemaObjectScopeDependency {
            source: to_database_object(dependency),
            target: to_database_object(&target),
        });
    }

    let mapping = renderer
        .map_schema_object_scope(
            ObjectKind::View,
            source_scope,
            target_scope,
            &object.definition,
            &pairs,
        )?
        .ok_or_else(|| {
            "The MySQL renderer does not provide a verified view scope-mapping implementation."
                .to_string()
        })?;
    if mapping.definition.trim().is_empty() {
        return Err("The MySQL renderer returned an empty mapped view definition".into());
    }
    let mapped_dependencies = mapping
        .dependencies
        .iter()
        .map(from_database_object)
        .collect::<Option<BTreeSet<_>>>()
        .ok_or_else(|| {
            "The MySQL renderer returned an invalid mapped dependency identity".to_string()
        })?;
    if mapped_dependencies != expected_targets
        || mapping.dependencies.len() != expected_targets.len()
    {
        return Err(
            "The MySQL renderer returned dependencies that do not exactly match the proven source-to-target identity mapping.".into(),
        );
    }

    let mut mapped = object.clone();
    mapped.schema = Some(target_scope.to_owned());
    mapped.definition = mapping.definition;
    mapped.dependencies = Some(mapped_dependencies.into_iter().collect());
    Ok(mapped)
}

fn validate_view_creation_semantics(
    metadata: &MySqlViewMetadata,
    target_context: &MySqlViewScopeContext,
) -> Result<(), String> {
    if !metadata.algorithm.eq_ignore_ascii_case("UNDEFINED")
        || metadata.has_explicit_column_list
        || !metadata.security_type.eq_ignore_ascii_case("DEFINER")
        || !metadata.check_option.eq_ignore_ascii_case("NONE")
    {
        return Err(
            "MySQL view migration only preserves ALGORITHM=UNDEFINED, no explicit column list, SECURITY_TYPE=DEFINER, and CHECK_OPTION=NONE; this view has non-default creation semantics.".into(),
        );
    }
    if metadata.definer != target_context.current_user {
        return Err(
            "MySQL view definer differs from the target connection user; mapping would change view access semantics.".into(),
        );
    }
    if metadata.character_set_client != target_context.character_set_client
        || metadata.collation_connection != target_context.collation_connection
    {
        return Err(
            "MySQL view creation character set or collation differs from the target connection; mapping would change literal semantics.".into(),
        );
    }
    Ok(())
}

pub fn validate_mysql_view_snapshot_creation_semantics(
    object: &SchemaObjectSnapshot,
    target_context: Option<&MySqlViewScopeContext>,
) -> Result<(), String> {
    let metadata = object.mysql_view_metadata.as_ref().ok_or_else(|| {
        "The driver did not provide MySQL view creation metadata; migration or rollback cannot preserve algorithm, columns, security, or collation semantics.".to_string()
    })?;
    let context = target_context.ok_or_else(|| {
        "The target connection could not prove its MySQL view creation context; compare again with session metadata visibility.".to_string()
    })?;
    validate_view_creation_semantics(metadata, context)
}

fn to_database_object(identity: &SchemaObjectIdentity) -> DatabaseObject {
    DatabaseObject {
        kind: identity.kind.as_str().to_owned(),
        schema: identity.schema.clone(),
        name: identity.name.clone(),
        signature: identity.signature.clone(),
        target_schema: identity.target_schema.clone(),
        target_name: identity.target_name.clone(),
    }
}

fn from_database_object(object: &DatabaseObject) -> Option<SchemaObjectIdentity> {
    Some(SchemaObjectIdentity {
        kind: ObjectKind::parse(&object.kind)?,
        schema: object.schema.clone(),
        name: object.name.clone(),
        signature: object.signature.clone(),
        target_schema: object.target_schema.clone(),
        target_name: object.target_name.clone(),
    })
}

fn unsupported(object: &SchemaObjectSnapshot, reason: impl Into<String>) -> PlanRequirement {
    PlanRequirement::Unsupported {
        operation: object.identity().display_key(),
        reason: reason.into(),
    }
}
