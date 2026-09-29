//! One reviewed, deterministic plan across tables and schema objects.
//!
//! The caller supplies only backend-read snapshots plus the complete target
//! catalog identity set. This module builds one typed operation graph and does
//! not render any SQL until every dependency has been proved.

use super::{
    dependencies,
    ir::diff_to_operations,
    object_identity::SchemaObjectDependencySnapshot,
    object_identity::SchemaObjectIdentity,
    objects::SchemaObjectSnapshot,
    operations::MigrationOperation,
    types::*,
    unified_objects::{diff_schema_objects_to_operations, operation_node_key},
    unified_scope::{map_source_objects_for_target_scope, MySqlViewScopeContext},
    unified_sequence::{render_sequence_phase, split_owned_sequence_operations},
    unified_validation::{
        empty_plan, operation_creates_or_replaces_identity, operation_drops_identity,
        operation_identity, unsupported, validate_custom_type_drops, validate_object_dependencies,
        validate_table_catalog_dependencies, validate_table_dependencies,
        validate_table_drop_catalog,
    },
};
use crate::db::TableSchema;
use datazen_driver_api::{MigrationCapabilities, MigrationRenderer};
use std::collections::{BTreeSet, HashMap};

fn apply_type_overrides(
    table: &str,
    operations: &mut [MigrationOperation],
    overrides: &[ColumnTypeOverride],
    dialect: &str,
) {
    if overrides.is_empty() {
        return;
    }
    let resolved_table = resolve_table_for_dialect(dialect, table);
    let target_type = |column: &str| {
        overrides
            .iter()
            .find(|override_| {
                resolve_table_for_dialect(dialect, &override_.table) == resolved_table
                    && override_.column == column
            })
            .map(|override_| override_.target_type.clone())
    };
    for operation in operations {
        match operation {
            MigrationOperation::CreateTable {
                table: operation_table,
                columns,
                ..
            } if resolve_table_for_dialect(dialect, operation_table) == resolved_table => {
                for column in columns {
                    if let Some(override_type) = target_type(&column.name) {
                        column.data_type = override_type;
                    }
                }
            }
            MigrationOperation::AddColumn {
                table: operation_table,
                column,
            } if resolve_table_for_dialect(dialect, operation_table) == resolved_table => {
                if let Some(override_type) = target_type(&column.name) {
                    column.data_type = override_type;
                }
            }
            MigrationOperation::AlterColumnType {
                table: operation_table,
                column,
                to,
                ..
            } if resolve_table_for_dialect(dialect, operation_table) == resolved_table => {
                if let Some(override_type) = target_type(column) {
                    *to = override_type;
                }
            }
            _ => {}
        }
    }
}

/// Build a unified reviewed plan. `target_catalog` must contain exact identities
/// from a complete target catalog; `target_objects` must contain all target
/// object metadata needed to prove that opaque dependents are absent.
#[allow(clippy::too_many_arguments)]
pub fn build_unified_schema_diff_plan_with_components(
    table_pairs: &[(String, TableSchema, TableSchema)],
    target_only_tables: &[String],
    source_objects: &[SchemaObjectSnapshot],
    target_objects: &[SchemaObjectSnapshot],
    target_catalog_objects: &[SchemaObjectSnapshot],
    source_table_dependencies: &[SchemaObjectDependencySnapshot],
    target_catalog: &[SchemaObjectIdentity],
    target_catalog_complete: bool,
    target_dependency_tables: &[(String, TableSchema)],
    source_dialect: &str,
    target_dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
    allow_destructive: bool,
    include_indexes: bool,
    type_overrides: &[ColumnTypeOverride],
    renderer: &dyn MigrationRenderer,
    capabilities: &dyn MigrationCapabilities,
) -> SchemaDiffPlan {
    let inferred_source_scope = source_objects
        .first()
        .and_then(|object| object.schema.as_deref());
    build_unified_schema_diff_plan_with_source_scope(
        table_pairs,
        target_only_tables,
        source_objects,
        target_objects,
        target_catalog_objects,
        source_table_dependencies,
        target_catalog,
        target_catalog_complete,
        target_dependency_tables,
        source_dialect,
        target_dialect,
        target_schema,
        target_database,
        allow_destructive,
        include_indexes,
        type_overrides,
        renderer,
        capabilities,
        inferred_source_scope,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_unified_schema_diff_plan_with_source_scope(
    table_pairs: &[(String, TableSchema, TableSchema)],
    target_only_tables: &[String],
    source_objects: &[SchemaObjectSnapshot],
    target_objects: &[SchemaObjectSnapshot],
    target_catalog_objects: &[SchemaObjectSnapshot],
    source_table_dependencies: &[SchemaObjectDependencySnapshot],
    target_catalog: &[SchemaObjectIdentity],
    target_catalog_complete: bool,
    target_dependency_tables: &[(String, TableSchema)],
    source_dialect: &str,
    target_dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
    allow_destructive: bool,
    include_indexes: bool,
    type_overrides: &[ColumnTypeOverride],
    renderer: &dyn MigrationRenderer,
    capabilities: &dyn MigrationCapabilities,
    configured_source_scope: Option<&str>,
    target_view_context: Option<&MySqlViewScopeContext>,
) -> SchemaDiffPlan {
    let source_dialect = normalize_dialect(source_dialect);
    let target_dialect = normalize_dialect(target_dialect);
    let mut requirements = Vec::new();
    let mut warnings = Vec::new();
    let mut labels = BTreeSet::new();
    let mut operations = Vec::new();

    if source_dialect != target_dialect
        && (!source_objects.is_empty() || !target_objects.is_empty())
    {
        requirements.push(unsupported(
            "unified-plan",
            "A unified plan requires matching source and target dialects because object DDL has no proven cross-dialect translation.",
        ));
        return empty_plan(
            &source_dialect,
            &target_dialect,
            labels,
            warnings,
            requirements,
        );
    }
    let target_object_scope = if uses_schema_scope(&target_dialect) {
        target_schema
    } else {
        target_database
    };
    if target_dialect == "mysql" {
        for object in target_objects
            .iter()
            .filter(|object| object.kind == datazen_driver_api::ObjectKind::View)
        {
            if let Err(reason) =
                super::unified_scope::validate_mysql_view_snapshot_creation_semantics(
                    object,
                    target_view_context,
                )
            {
                requirements.push(unsupported(object.identity().display_key(), reason));
            }
        }
    }
    let (planned_source_objects, scope_requirements) = map_source_objects_for_target_scope(
        source_objects,
        table_pairs,
        &target_dialect,
        configured_source_scope,
        target_object_scope,
        target_view_context,
        renderer,
    );
    requirements.extend(scope_requirements);
    if !requirements.is_empty() {
        return empty_plan(
            &source_dialect,
            &target_dialect,
            labels,
            warnings,
            requirements,
        );
    }
    if let Some(target_scope) = target_object_scope {
        for source_object in &planned_source_objects {
            if source_object
                .schema
                .as_deref()
                .is_some_and(|schema| schema != target_scope)
            {
                requirements.push(unsupported(
                    source_object.identity().display_key(),
                    format!(
                        "Source object `{}` is scoped to `{}`, while target scope is `{target_scope}`. Object DDL is not rewritten across schemas; choose matching scopes or migrate this object through a renderer that supports schema mapping.",
                        source_object.identity().display_key(),
                        source_object.schema.as_deref().unwrap_or_default()
                    ),
                ));
            }
        }
        for source_object in &planned_source_objects {
            if target_objects.iter().any(|target_object| {
                target_object.kind == source_object.kind
                    && target_object.name == source_object.name
                    && target_object.signature == source_object.signature
                    && target_object.schema != source_object.schema
            }) {
                requirements.push(unsupported(
                    source_object.identity().display_key(),
                    "Source and target object identities differ by schema; the planner cannot safely map object DDL between scopes.",
                ));
            }
        }
    }
    if !requirements.is_empty() {
        return empty_plan(
            &source_dialect,
            &target_dialect,
            labels,
            warnings,
            requirements,
        );
    }
    if !target_catalog_complete {
        requirements.push(unsupported(
            "target-catalog",
            "The complete target catalog is required to prove unselected dependencies. Grant catalog visibility and compare again.",
        ));
    }

    for (table, source, target) in table_pairs {
        labels.insert(table.clone());
        let mut table_operations = diff_to_operations(table, source, target, None);
        apply_type_overrides(
            table,
            &mut table_operations,
            type_overrides,
            &target_dialect,
        );
        if !include_indexes {
            table_operations.retain(|operation| {
                !matches!(
                    operation,
                    MigrationOperation::CreateIndex { .. } | MigrationOperation::DropIndex { .. }
                )
            });
        }
        if !table_operations.is_empty() {
            if let Some(reason) = super::plan::table_migration_blocker_reason(source, target) {
                requirements.push(unsupported(format!("table:{table}"), reason));
            }
        }
        for operation in &table_operations {
            if let MigrationOperation::AddColumn { table, column } = operation {
                if !column.nullable && column.default_value.is_none() {
                    requirements.push(PlanRequirement::Backfill {
                        table: table.clone(),
                        column: column.name.clone(),
                        reason: "Populate existing rows before enforcing NOT NULL.".into(),
                    });
                }
            }
        }
        operations.extend(table_operations);
    }
    for table in target_only_tables {
        labels.insert(table.clone());
        if allow_destructive {
            operations.push(MigrationOperation::DropTable {
                table: table.clone(),
            });
        } else {
            warnings.push(format!("Skipped destructive operation table:{table}"));
        }
    }

    let object_batch = diff_schema_objects_to_operations(
        &planned_source_objects,
        target_objects,
        &source_dialect,
        &target_dialect,
        allow_destructive,
    );
    labels.extend(object_batch.labels);
    warnings.extend(object_batch.warnings);
    requirements.extend(object_batch.requirements);
    operations.extend(object_batch.operations);
    let mut object_transitions = object_batch.transitions;
    let (sequence_phase_edges, sequence_requirements) = split_owned_sequence_operations(
        &mut operations,
        &mut object_transitions,
        &planned_source_objects,
        target_objects,
        source_table_dependencies,
        &target_dialect,
        target_schema,
        target_database,
    );
    requirements.extend(sequence_requirements);
    if operations.is_empty() && requirements.is_empty() {
        requirements.push(unsupported(
            "unified-plan",
            "Select at least one table or schema object with a difference.",
        ));
    }

    let target_catalog = target_catalog.iter().cloned().collect::<BTreeSet<_>>();
    let node_keys = operations
        .iter()
        .map(operation_node_key)
        .collect::<Vec<_>>();
    let mut operation_indices = HashMap::new();
    for (index, key) in node_keys.iter().enumerate() {
        if operation_indices.insert(key.clone(), index).is_some() {
            requirements.push(unsupported(
                key,
                "The reviewed plan contains duplicate operation identities; remove the duplicate selection and compare again.",
            ));
        }
    }

    let mut identities_created = HashMap::new();
    let mut identities_dropped = HashMap::new();
    for (index, operation) in operations.iter().enumerate() {
        if let Some(identity) =
            operation_identity(operation, &target_dialect, target_schema, target_database)
        {
            if operation_creates_or_replaces_identity(operation) {
                identities_created.insert(identity, index);
            } else if operation_drops_identity(operation) {
                identities_dropped.insert(identity, index);
            }
        }
    }

    let mut extra_edges = Vec::new();
    for (before_key, after_key) in sequence_phase_edges {
        let before = node_keys.iter().position(|key| key == &before_key);
        let after = node_keys.iter().position(|key| key == &after_key);
        match (before, after) {
            (Some(before), Some(after)) => extra_edges.push((before, after)),
            _ => requirements.push(unsupported(
                "sequence-ownership",
                "The sequence ownership phase is missing an exact operation node; compare again before deploying.",
            )),
        }
    }
    validate_table_dependencies(
        &operations,
        &target_catalog,
        &identities_created,
        &identities_dropped,
        &node_keys,
        &mut extra_edges,
        &mut requirements,
        &target_dialect,
        target_schema,
        target_database,
    );
    validate_table_catalog_dependencies(
        source_table_dependencies,
        &operations,
        &target_catalog,
        &identities_created,
        &identities_dropped,
        &node_keys,
        &mut extra_edges,
        &mut requirements,
        &target_dialect,
        target_schema,
        target_database,
    );
    validate_object_dependencies(
        &operations,
        &object_transitions,
        target_catalog_objects,
        &target_catalog,
        &identities_created,
        &identities_dropped,
        &mut extra_edges,
        &mut requirements,
    );
    validate_table_drop_catalog(
        &operations,
        target_dependency_tables,
        &mut extra_edges,
        &mut requirements,
        &target_dialect,
        target_schema,
        target_database,
    );
    validate_custom_type_drops(
        &operations,
        target_dependency_tables,
        &mut extra_edges,
        &mut requirements,
        &target_dialect,
        target_schema,
        target_database,
    );

    let ordered = match dependencies::try_resolve_dependencies_with_operation_edges(
        &operations,
        &extra_edges,
    ) {
        Ok(ordered) => ordered,
        Err(reason) => {
            requirements.push(unsupported(
                "unified-dependency-graph",
                format!("Cannot prove a safe cross-kind operation order: {reason}. Resolve the missing/ambiguous dependency or cycle and compare again."),
            ));
            Vec::new()
        }
    };

    let mut statements = Vec::new();
    if requirements.is_empty() {
        for operation in ordered {
            let operation_key = operation_node_key(&operation);
            let driver_operation = operation.to_driver_api();
            if !capabilities.supports(&driver_operation) {
                requirements.push(unsupported(
                    &operation_key,
                    format!("Target driver `{target_dialect}` does not support this operation."),
                ));
                continue;
            }
            let rendered = match render_sequence_phase(renderer, &operation) {
                Ok(Some(statement)) => Ok(statement),
                Ok(None) => renderer.render(&driver_operation),
                Err(reason) => Err(reason),
            };
            match rendered {
                Ok(statement) => statements.push(PlanStatement {
                    sql: statement.sql,
                    risk: match statement.risk {
                        datazen_driver_api::MigrationRisk::Additive => StatementRisk::Additive,
                        datazen_driver_api::MigrationRisk::Rewrite => StatementRisk::Rewrite,
                        datazen_driver_api::MigrationRisk::Destructive => {
                            StatementRisk::Destructive
                        }
                    },
                    rollback_sql: statement.rollback_sql,
                    summary: statement.summary,
                    requires_transaction: false,
                }),
                Err(reason) => requirements.push(unsupported(
                    &operation_key,
                    format!("Target renderer cannot safely render this operation: {reason}"),
                )),
            }
        }
    }
    if !requirements.is_empty() {
        // A plan with any unresolved dependency or unsupported operation is
        // display-only: it contains no executable prefix.
        statements.clear();
    }
    let missing = statements
        .iter()
        .filter(|statement| statement.rollback_sql.is_none())
        .map(|statement| statement.summary.clone())
        .collect::<Vec<_>>();
    let same_dialect = source_dialect == target_dialect;
    SchemaDiffPlan {
        plan_id: None,
        table: labels.first().cloned().unwrap_or_else(|| "unified".into()),
        tables: labels.into_iter().collect(),
        source_dialect,
        target_dialect,
        same_dialect,
        statements,
        warnings,
        requirements,
        rollback_completeness: RollbackCompleteness {
            complete: missing.is_empty(),
            missing,
        },
        type_suggestions: Vec::new(),
        expected_target_schemas: Vec::new(),
    }
}

#[cfg(test)]
#[path = "unified_tests.rs"]
mod tests;
