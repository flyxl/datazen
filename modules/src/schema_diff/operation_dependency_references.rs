//! Exact relation and custom-type reference validation for migration operations.

use super::dependency_graph::DependencyGraphError;
use super::operations::MigrationOperation;

pub(super) fn foreign_key_references_table(
    foreign_key: &datazen_driver_api::ForeignKeyInfo,
    table: &str,
) -> bool {
    table_reference_matches(&foreign_key.referenced_table, table)
}

pub(super) fn foreign_key_uses_column(
    foreign_key_table: &str,
    foreign_key: &datazen_driver_api::ForeignKeyInfo,
    table: &str,
    column: &str,
) -> bool {
    if op_table_name_matches(foreign_key_table, table) {
        foreign_key.columns.iter().any(|value| value == column)
    } else if foreign_key_references_table(foreign_key, table) {
        foreign_key
            .referenced_columns
            .iter()
            .any(|value| value == column)
    } else {
        false
    }
}

pub(super) fn op_table_name_matches(table: &str, referenced_table: &str) -> bool {
    table_reference_matches(referenced_table, table)
}

/// A dependency edge is valid only when the renderer supplied the same
/// relation identity on both sides. Basename-only matches are considered
/// possible candidates for diagnostics, never proof of identity.
pub(super) fn table_reference_matches(reference: &str, candidate: &str) -> bool {
    reference == candidate
}

fn table_reference_may_match(reference: &str, candidate: &str) -> bool {
    reference == candidate || reference.rsplit('.').next() == candidate.rsplit('.').next()
}

pub(super) fn trigger_targets_table(
    trigger: &datazen_driver_api::MigrationTrigger,
    table: &str,
) -> bool {
    let target = trigger_target_identity(trigger);
    table_reference_matches(&target, table)
}

fn trigger_target_identity(trigger: &datazen_driver_api::MigrationTrigger) -> String {
    trigger
        .target_schema
        .as_deref()
        .filter(|schema| !schema.is_empty())
        .map(|schema| format!("{schema}.{}", trigger.target_name))
        .unwrap_or_else(|| trigger.target_name.clone())
}

fn type_reference_parts(raw: &str) -> Vec<String> {
    let mut type_name = raw.trim();
    while let Some(stripped) = type_name.strip_suffix("[]") {
        type_name = stripped.trim_end();
    }
    if let Some(open) = type_name.find('(') {
        type_name = type_name[..open].trim_end();
    }
    type_name
        .split('.')
        .map(|part| part.trim().trim_matches('"').trim_matches('`').to_owned())
        .collect()
}

pub(super) fn column_uses_type(
    column: &crate::schema_diff::types::ColumnSnapshot,
    type_def: &datazen_driver_api::MigrationType,
) -> bool {
    let actual = type_reference_parts(&column.data_type);
    let expected = type_def
        .schema
        .as_deref()
        .filter(|schema| !schema.is_empty())
        .map(|schema| vec![schema.to_owned(), type_def.name.clone()])
        .unwrap_or_else(|| vec![type_def.name.clone()]);
    match (actual.as_slice(), expected.as_slice()) {
        ([actual_name], [expected_name]) => actual_name == expected_name,
        ([actual_name], [expected_schema, expected_name]) => {
            expected_schema.is_empty() && actual_name == expected_name
        }
        ([actual_schema, actual_name], [expected_schema, expected_name]) => {
            actual_schema == expected_schema && actual_name == expected_name
        }
        _ => false,
    }
}

fn matching_selected_types(
    column: &crate::schema_diff::types::ColumnSnapshot,
    operations: &[MigrationOperation],
) -> std::collections::BTreeSet<String> {
    operations
        .iter()
        .filter_map(|operation| match operation {
            MigrationOperation::CreateType { type_definition }
            | MigrationOperation::ReplaceType {
                desired: type_definition,
                ..
            } if column_type_may_match(column, type_definition) => Some(format!(
                "{}.{}",
                type_definition.schema.as_deref().unwrap_or_default(),
                type_definition.name
            )),
            _ => None,
        })
        .collect()
}

fn matching_selected_types_exact(
    column: &crate::schema_diff::types::ColumnSnapshot,
    operations: &[MigrationOperation],
) -> std::collections::BTreeSet<String> {
    operations
        .iter()
        .filter_map(|operation| match operation {
            MigrationOperation::CreateType { type_definition }
            | MigrationOperation::ReplaceType {
                desired: type_definition,
                ..
            } if column_uses_type(column, type_definition) => Some(format!(
                "{}.{}",
                type_definition.schema.as_deref().unwrap_or_default(),
                type_definition.name
            )),
            _ => None,
        })
        .collect()
}

fn column_type_may_match(
    column: &crate::schema_diff::types::ColumnSnapshot,
    type_def: &datazen_driver_api::MigrationType,
) -> bool {
    let actual = type_reference_parts(&column.data_type);
    actual
        .last()
        .is_some_and(|actual_name| actual_name == &type_def.name)
}

pub(super) fn reject_ambiguous_references(
    ops: &[MigrationOperation],
) -> Result<(), DependencyGraphError<String>> {
    use MigrationOperation::{
        AddColumn, AddForeignKey, CreateTable, CreateTrigger, DropForeignKey, ReplaceTrigger,
    };
    for operation in ops {
        match operation {
            AddForeignKey { foreign_key, .. } | DropForeignKey { foreign_key, .. } => {
                let candidates = ops
                    .iter()
                    .filter_map(|candidate| match candidate {
                        CreateTable { table, .. }
                            if table_reference_may_match(&foreign_key.referenced_table, table) =>
                        {
                            Some(table.as_str())
                        }
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                let exact = candidates
                    .iter()
                    .copied()
                    .filter(|table| table_reference_matches(&foreign_key.referenced_table, table))
                    .collect::<std::collections::BTreeSet<_>>();
                if exact.is_empty() && !candidates.is_empty() {
                    return Err(DependencyGraphError::AmbiguousReference(format!(
                        "foreign key {} references {} but selected table identity is only a basename match ({})",
                        foreign_key.name,
                        foreign_key.referenced_table,
                        candidates.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
                if exact.len() > 1 {
                    return Err(DependencyGraphError::AmbiguousReference(format!(
                        "foreign key {} references {} which matches selected tables {}",
                        foreign_key.name,
                        foreign_key.referenced_table,
                        exact.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
            }
            CreateTrigger { trigger }
            | ReplaceTrigger {
                desired: trigger, ..
            } => {
                let target = trigger_target_identity(trigger);
                let candidates = ops
                    .iter()
                    .filter_map(|candidate| match candidate {
                        CreateTable { table, .. } if table_reference_may_match(&target, table) => {
                            Some(table.as_str())
                        }
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                let exact = candidates
                    .iter()
                    .copied()
                    .filter(|table| table_reference_matches(&target, table))
                    .collect::<std::collections::BTreeSet<_>>();
                if exact.is_empty() && !candidates.is_empty() {
                    return Err(DependencyGraphError::AmbiguousReference(format!(
                        "trigger {} target {} is only a basename match for selected tables ({})",
                        trigger.name,
                        target,
                        candidates.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
                if exact.len() > 1 {
                    return Err(DependencyGraphError::AmbiguousReference(format!(
                        "trigger {} targets {}.{} which matches selected tables {}",
                        trigger.name,
                        trigger.target_schema.as_deref().unwrap_or_default(),
                        trigger.target_name,
                        exact.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
            }
            CreateTable { columns, .. } => {
                for column in columns {
                    let candidates = matching_selected_types(column, ops);
                    let exact = matching_selected_types_exact(column, ops);
                    if exact.is_empty() && !candidates.is_empty() {
                        return Err(DependencyGraphError::AmbiguousReference(format!(
                            "column {} type {} matches selected custom type by basename only ({})",
                            column.name,
                            column.data_type,
                            candidates.into_iter().collect::<Vec<_>>().join(", ")
                        )));
                    }
                    if exact.len() > 1 {
                        return Err(DependencyGraphError::AmbiguousReference(format!(
                            "column {} uses unqualified type {} matching {}",
                            column.name,
                            column.data_type,
                            exact.into_iter().collect::<Vec<_>>().join(", ")
                        )));
                    }
                }
            }
            AddColumn { column, .. } => {
                let candidates = matching_selected_types(column, ops);
                let exact = matching_selected_types_exact(column, ops);
                if exact.is_empty() && !candidates.is_empty() {
                    return Err(DependencyGraphError::AmbiguousReference(format!(
                        "column {} type {} matches selected custom type by basename only ({})",
                        column.name,
                        column.data_type,
                        candidates.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
                if exact.len() > 1 {
                    return Err(DependencyGraphError::AmbiguousReference(format!(
                        "column {} uses unqualified type {} matching {}",
                        column.name,
                        column.data_type,
                        exact.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
