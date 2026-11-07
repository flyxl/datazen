//! Object selection rules for schema diff plans.
//!
//! A plan selects schema objects (views, routines, sequences, types) by the
//! identity the user picked in the tree. These rules are the plan's own
//! contract: an unsupported kind, an empty name, a duplicate identity or an
//! object that the live catalog no longer shows unambiguously must fail the
//! plan instead of silently narrowing it. They live here so the desktop host
//! and a web host enforce the same rules.

use datazen_driver_api::schema_objects::ObjectKind;
use datazen_driver_api::{DatabaseObject, TableInfo, TableType};

use super::object_identity::SchemaObjectIdentity;
use super::objects::SchemaObjectSnapshot;

/// Why an object selection cannot become a plan.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ObjectSelectionError {
    #[error("Unsupported schema object kind `{kind}`")]
    UnsupportedKind { kind: String },
    #[error("Schema object name must not be empty")]
    EmptyName,
    #[error("Table `{identity}` must be selected through the table list")]
    TableSelectedAsObject { identity: String },
    #[error("{side} schema object selection contains a duplicate identity")]
    DuplicateIdentity { side: String },
    #[error(
        "The {side} object `{identity}` is missing or ambiguous in the live catalog; refresh the selection and compare again"
    )]
    MissingOrAmbiguous { side: String, identity: String },
}

/// The identity of a selected schema object.
pub fn object_identity(object: &DatabaseObject) -> SchemaObjectIdentity {
    SchemaObjectIdentity {
        kind: ObjectKind::parse(&object.kind).unwrap_or(ObjectKind::Table),
        schema: object.schema.clone(),
        name: object.name.clone(),
        signature: object.signature.clone(),
        target_schema: object.target_schema.clone(),
        target_name: object.target_name.clone(),
    }
}

/// Validate the selected objects: known kind, non-empty name, no table
/// selected as an object, and no duplicate identity.
pub fn unique_object_selectors(
    objects: &[DatabaseObject],
    side: &str,
) -> Result<Vec<DatabaseObject>, ObjectSelectionError> {
    let mut seen = std::collections::BTreeSet::new();
    let mut selectors = Vec::with_capacity(objects.len());
    for object in objects {
        let kind = ObjectKind::parse(&object.kind).ok_or_else(|| {
            ObjectSelectionError::UnsupportedKind {
                kind: object.kind.clone(),
            }
        })?;
        if object.name.trim().is_empty() {
            return Err(ObjectSelectionError::EmptyName);
        }
        let identity = object_identity(object);
        if kind == ObjectKind::Table {
            return Err(ObjectSelectionError::TableSelectedAsObject {
                identity: identity.display_key(),
            });
        }
        if !seen.insert(identity) {
            return Err(ObjectSelectionError::DuplicateIdentity {
                side: side.to_string(),
            });
        }
        selectors.push(object.clone());
    }
    Ok(selectors)
}

/// Match the selection against the live catalog. An object that is missing or
/// ambiguous fails the plan — the selection must be refreshed.
pub fn resolve_object_selectors(
    selectors: &[DatabaseObject],
    catalog: &[DatabaseObject],
    side: &str,
) -> Result<Vec<DatabaseObject>, ObjectSelectionError> {
    let mut selected = Vec::with_capacity(selectors.len());
    for selector in selectors {
        let wanted = object_identity(selector);
        let matches = catalog
            .iter()
            .filter(|candidate| object_identity(candidate) == wanted)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(ObjectSelectionError::MissingOrAmbiguous {
                side: side.to_string(),
                identity: wanted.display_key(),
            });
        }
        selected.push(matches[0].clone());
    }
    Ok(selected)
}

/// The snapshot of a catalog dependency entry.
pub fn catalog_entry_snapshot(
    entry: &super::object_identity::SchemaObjectDependencySnapshot,
) -> SchemaObjectSnapshot {
    SchemaObjectSnapshot {
        kind: entry.identity.kind,
        schema: entry.identity.schema.clone(),
        name: entry.identity.name.clone(),
        signature: entry.identity.signature.clone(),
        target_schema: entry.identity.target_schema.clone(),
        target_name: entry.identity.target_name.clone(),
        definition: String::new(),
        dependencies: entry.dependencies.clone(),
        sequence_dependency_usages: entry.sequence_dependency_usages.clone(),
        mysql_view_metadata: None,
    }
}

/// The identities of the tables in a catalog, scoped to the plan's dialect.
pub fn table_identities_from_catalog(
    tables: &[TableInfo],
    dialect: &str,
    schema_scope: Option<&str>,
    database: Option<&str>,
) -> Vec<SchemaObjectIdentity> {
    let mut identities = tables
        .iter()
        .filter(|table| matches!(table.table_type, TableType::Table))
        .map(|table| {
            let scope = table.schema.as_deref().or_else(|| {
                if super::types::uses_schema_scope(dialect) {
                    schema_scope
                } else {
                    database
                }
            });
            SchemaObjectIdentity::table(scope, &table.name)
        })
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    identities
}

/// The identity of one target table, scoped to the plan's dialect.
pub fn table_identity_for_target(
    table: &str,
    dialect: &str,
    schema_scope: Option<&str>,
    database: Option<&str>,
) -> SchemaObjectIdentity {
    let (schema, name) = table
        .rsplit_once('.')
        .map(|(schema, name)| (Some(schema), name))
        .unwrap_or((None, table));
    let scope = schema.or_else(|| {
        if super::types::uses_schema_scope(dialect) {
            schema_scope
        } else {
            database
        }
    });
    SchemaObjectIdentity::table(scope, name)
}
