//! Dependency ordering for migration operations.

use super::operations::MigrationOperation;

pub fn resolve_dependencies(ops: Vec<MigrationOperation>) -> Vec<MigrationOperation> {
    let mut create_tables = Vec::new();
    let mut adds = Vec::new();
    let mut alters = Vec::new();
    let mut drops = Vec::new();
    let mut indexes = Vec::new();
    for op in ops {
        match op {
            MigrationOperation::CreateTable { .. } => create_tables.push(op),
            MigrationOperation::AddColumn { .. } | MigrationOperation::AddPrimaryKey { .. } => adds.push(op),
            MigrationOperation::AlterColumnType { .. } | MigrationOperation::SetNullable { .. } |
            MigrationOperation::SetDefault { .. } | MigrationOperation::SetComment { .. } |
            MigrationOperation::SetAutoIncrement { .. } => alters.push(op),
            MigrationOperation::CreateIndex { .. } => indexes.push(op),
            MigrationOperation::DropIndex { .. } | MigrationOperation::DropPrimaryKey { .. } |
            MigrationOperation::DropColumn { .. } => drops.push(op),
        }
    }
    create_tables.into_iter().chain(adds).chain(alters).chain(indexes).chain(drops).collect()
}
