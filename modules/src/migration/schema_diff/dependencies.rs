//! Compatibility entry points for Schema Diff operation dependency planning.

use super::operations::MigrationOperation;

pub fn retain_dependency_closed(
    all: &[MigrationOperation],
    selected: &mut Vec<MigrationOperation>,
) {
    super::operation_dependencies::retain_dependency_closed(all, selected)
}

pub(super) fn try_resolve_dependencies(
    ops: &[MigrationOperation],
) -> Result<Vec<MigrationOperation>, String> {
    super::operation_dependencies::try_resolve_dependencies(ops)
}

/// Resolve the normal operation graph plus explicit target-table drop edges.
/// Each pair is `(dependent_table, referenced_table)`: the dependent table
/// must be dropped before the table it references.
pub(super) fn try_resolve_dependencies_with_table_drop_edges(
    ops: &[MigrationOperation],
    dependent_before_referenced: &[(String, String)],
) -> Result<Vec<MigrationOperation>, String> {
    super::operation_dependencies::try_resolve_dependencies_with_table_drop_edges(
        ops,
        dependent_before_referenced,
    )
}

pub(super) fn try_resolve_dependencies_with_operation_edges(
    ops: &[MigrationOperation],
    prerequisite_before_dependent: &[(usize, usize)],
) -> Result<Vec<MigrationOperation>, String> {
    super::operation_dependencies::try_resolve_dependencies_with_operation_edges(
        ops,
        prerequisite_before_dependent,
    )
}

pub fn resolve_dependencies(ops: Vec<MigrationOperation>) -> Vec<MigrationOperation> {
    super::operation_dependencies::resolve_dependencies(ops)
}
