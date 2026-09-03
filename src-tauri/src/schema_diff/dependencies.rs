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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_diff::operations::MigrationOperation;

    #[test]
    fn ordering_places_create_before_index_and_drop() {
        let ops = vec![
            MigrationOperation::DropColumn { table:"t".into(), column: crate::schema_diff::types::ColumnSnapshot { name:"old".into(), data_type:"int".into(), nullable:true, default_value:None, comment:None, is_primary_key:false, is_auto_increment:false } },
            MigrationOperation::CreateIndex { table:"t".into(), index: crate::db::IndexInfo { name:"idx".into(), columns:vec!["id".into()], is_unique:false, is_primary:false, index_type:"btree".into() } },
            MigrationOperation::AddColumn { table:"t".into(), column: crate::schema_diff::types::ColumnSnapshot { name:"new".into(), data_type:"int".into(), nullable:true, default_value:None, comment:None, is_primary_key:false, is_auto_increment:false } },
        ];
        let sorted = resolve_dependencies(ops);
        assert!(matches!(sorted[0], MigrationOperation::AddColumn { .. }));
        assert!(matches!(sorted[1], MigrationOperation::CreateIndex { .. }));
        assert!(matches!(sorted[2], MigrationOperation::DropColumn { .. }));
    }
}
