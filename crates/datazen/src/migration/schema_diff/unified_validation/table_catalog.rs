//! Table catalog dependency validation used by unified schema plans.

use super::*;

#[allow(clippy::too_many_arguments)]
pub fn validate_table_catalog_dependencies(
    source_table_dependencies: &[SchemaObjectDependencySnapshot],
    operations: &[MigrationOperation],
    target_catalog: &BTreeSet<SchemaObjectIdentity>,
    identities_created: &HashMap<SchemaObjectIdentity, usize>,
    identities_dropped: &HashMap<SchemaObjectIdentity, usize>,
    node_keys: &[String],
    extra_edges: &mut Vec<(usize, usize)>,
    requirements: &mut Vec<PlanRequirement>,
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) {
    for snapshot in source_table_dependencies {
        let dependent_indices = operations
            .iter()
            .enumerate()
            .filter_map(|(index, operation)| {
                (operation_identity(operation, dialect, target_schema, target_database).as_ref()
                    == Some(&snapshot.identity))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        if dependent_indices.is_empty() {
            continue;
        }
        let Some(dependencies) = snapshot.dependencies.as_ref() else {
            requirements.push(unsupported(
                snapshot.identity.display_key(),
                format!(
                    "The source driver could not prove all dependencies of `{}`. Enable complete catalog visibility or remove this table from the unified plan.",
                    snapshot.identity.display_key()
                ),
            ));
            continue;
        };
        for dependency in dependencies {
            if dependency.kind == ObjectKind::Type
                && type_dependency_fully_replaced(
                    snapshot,
                    dependency,
                    operations,
                    dialect,
                    target_schema,
                    target_database,
                )
            {
                continue;
            }
            if let Some(prerequisite) = identities_created.get(dependency) {
                for dependent in &dependent_indices {
                    if prerequisite != dependent {
                        extra_edges.push((*prerequisite, *dependent));
                    }
                }
                continue;
            }
            if identities_dropped.contains_key(dependency) {
                requirements.push(unsupported(
                    node_keys
                        .get(dependent_indices[0])
                        .map(String::as_str)
                        .unwrap_or("table"),
                    format!(
                        "Source table `{}` depends on `{}`, which is also selected for removal.",
                        snapshot.identity.display_key(),
                        dependency.display_key()
                    ),
                ));
                continue;
            }
            if target_catalog.contains(dependency) {
                continue;
            }
            let basename_matches = target_catalog
                .iter()
                .filter(|candidate| {
                    candidate.kind == dependency.kind && candidate.name == dependency.name
                })
                .map(SchemaObjectIdentity::display_key)
                .collect::<Vec<_>>();
            let reason = if basename_matches.is_empty() {
                format!(
                    "Source table `{}` requires `{}`, which is absent from the complete target snapshot and selected plan.",
                    snapshot.identity.display_key(),
                    dependency.display_key()
                )
            } else {
                format!(
                    "Source table `{}` requires exact identity `{}`, but the target has only ambiguous basename candidates: {}.",
                    snapshot.identity.display_key(),
                    dependency.display_key(),
                    basename_matches.join(", ")
                )
            };
            requirements.push(unsupported(
                node_keys
                    .get(dependent_indices[0])
                    .map(String::as_str)
                    .unwrap_or("table"),
                reason,
            ));
        }
    }
}

fn type_dependency_fully_replaced(
    snapshot: &SchemaObjectDependencySnapshot,
    dependency: &SchemaObjectIdentity,
    operations: &[MigrationOperation],
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) -> bool {
    let Some(usages) = snapshot.type_dependency_usages.as_ref() else {
        return false;
    };
    let matching_usages = usages
        .iter()
        .filter(|usage| &usage.dependency == dependency)
        .collect::<Vec<_>>();
    if matching_usages.is_empty() {
        return false;
    }
    matching_usages.iter().all(|usage| {
        let (TypeDependencyUsageKind::ColumnType, Some(column_name)) =
            (usage.usage, usage.column_name.as_deref())
        else {
            return false;
        };
        let final_type = operations.iter().find_map(|operation| {
            if operation_identity(operation, dialect, target_schema, target_database).as_ref()
                != Some(&snapshot.identity)
            {
                return None;
            }
            match operation {
                MigrationOperation::CreateTable { columns, .. } => columns
                    .iter()
                    .find(|column| column.name == column_name)
                    .map(|column| column.data_type.as_str()),
                MigrationOperation::AddColumn { column, .. } if column.name == column_name => {
                    Some(column.data_type.as_str())
                }
                MigrationOperation::AlterColumnType { column, to, .. } if column == column_name => {
                    Some(to.as_str())
                }
                _ => None,
            }
        });
        final_type.is_some_and(|final_type| !type_name_matches(final_type, dependency, dialect))
    })
}

pub fn validate_table_drop_catalog(
    operations: &[MigrationOperation],
    target_dependency_tables: &[(String, TableSchema)],
    extra_edges: &mut Vec<(usize, usize)>,
    requirements: &mut Vec<PlanRequirement>,
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) {
    let drops = operations
        .iter()
        .enumerate()
        .filter_map(|(index, operation)| match operation {
            MigrationOperation::DropTable { table } => Some((
                index,
                table_identity(table, dialect, target_schema, target_database),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    if drops.is_empty() {
        return;
    }
    for (child_name, child_schema) in target_dependency_tables {
        let child = table_identity(child_name, dialect, target_schema, target_database);
        for foreign_key in &child_schema.foreign_keys {
            let referenced = table_identity(
                &foreign_key.referenced_table,
                dialect,
                target_schema,
                target_database,
            );
            let Some((parent_index, _)) = drops.iter().find(|(_, parent)| *parent == referenced)
            else {
                continue;
            };
            if let Some((child_index, _)) = drops.iter().find(|(_, dropped)| *dropped == child) {
                extra_edges.push((*child_index, *parent_index));
                continue;
            }
            let has_selected_fk_drop = operations.iter().enumerate().any(|(_, operation)| {
                matches!(operation, MigrationOperation::DropForeignKey { table, foreign_key: selected }
                    if table_identity(table, dialect, target_schema, target_database) == child
                        && selected.name == foreign_key.name
                        && table_identity(&selected.referenced_table, dialect, target_schema, target_database) == referenced)
            });
            if !has_selected_fk_drop {
                requirements.push(unsupported(
                    referenced.display_key(),
                    format!("Unselected target table `{}` has foreign key `{}` to selected drop `{}`. Select the dependent table or remove its foreign key before dropping the parent.", child.display_key(), foreign_key.name, referenced.display_key()),
                ));
            }
        }
    }
}

pub fn validate_custom_type_drops(
    operations: &[MigrationOperation],
    target_dependency_tables: &[(String, TableSchema)],
    extra_edges: &mut Vec<(usize, usize)>,
    requirements: &mut Vec<PlanRequirement>,
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) {
    let type_drops = operations
        .iter()
        .enumerate()
        .filter_map(|(index, operation)| match operation {
            MigrationOperation::DropType { type_definition } => Some((
                index,
                SchemaObjectIdentity {
                    kind: ObjectKind::Type,
                    schema: type_definition.schema.clone(),
                    name: type_definition.name.clone(),
                    signature: None,
                    target_schema: None,
                    target_name: None,
                },
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    if type_drops.is_empty() {
        return;
    }
    if target_dependency_tables.is_empty() {
        for (_, identity) in &type_drops {
            requirements.push(unsupported(
                identity.display_key(),
                format!("The complete target table catalog is required to prove no columns depend on `{}`. Retry with catalog visibility or keep the type drop out of this plan.", identity.display_key()),
            ));
        }
        return;
    }

    for (table_index, (table, schema)) in target_dependency_tables.iter().enumerate() {
        let table_id = table_identity(table, dialect, target_schema, target_database);
        for column in &schema.columns {
            let matching_drops = type_drops
                .iter()
                .filter(|(_, identity)| type_name_matches(&column.data_type, identity, dialect))
                .collect::<Vec<_>>();
            if matching_drops.is_empty() {
                continue;
            }
            if matching_drops.len() > 1 {
                requirements.push(unsupported(
                    table_id.display_key(),
                    format!("Column `{}` on `{}` uses unqualified type `{}` that matches multiple selected type drops; qualify the column type or resolve the ambiguity before comparing again.", column.name, table_id.display_key(), column.data_type),
                ));
                continue;
            }
            let (type_drop_index, type_identity) = matching_drops[0];
            let dependent_operation =
                operations
                    .iter()
                    .enumerate()
                    .find(|(_, operation)| match operation {
                        MigrationOperation::DropTable { table: dropped } => {
                            table_identity(dropped, dialect, target_schema, target_database)
                                == table_id
                        }
                        MigrationOperation::DropColumn {
                            table: dropped,
                            column: dropped_column,
                        } => {
                            dropped_column.name == column.name
                                && table_identity(dropped, dialect, target_schema, target_database)
                                    == table_id
                        }
                        _ => false,
                    });
            if let Some((dependent_index, _)) = dependent_operation {
                extra_edges.push((dependent_index, *type_drop_index));
            } else {
                requirements.push(unsupported(
                    type_identity.display_key(),
                    format!("Unchanged target column `{}.{}` depends on selected type drop `{}`. Keep the type or include a table/column operation that removes the dependency first.", table_id.display_key(), column.name, type_identity.display_key()),
                ));
            }
            let _ = table_index;
        }
    }
}
