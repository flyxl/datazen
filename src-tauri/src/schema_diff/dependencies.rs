//! Dependency ordering for migration operations.

use super::operations::MigrationOperation;

fn op_table(op: &MigrationOperation) -> &str {
    match op {
        MigrationOperation::CreateTable { table, .. }
        | MigrationOperation::AddColumn { table, .. }
        | MigrationOperation::DropColumn { table, .. }
        | MigrationOperation::AlterColumnType { table, .. }
        | MigrationOperation::SetNullable { table, .. }
        | MigrationOperation::SetDefault { table, .. }
        | MigrationOperation::SetComment { table, .. }
        | MigrationOperation::SetAutoIncrement { table, .. }
        | MigrationOperation::AddPrimaryKey { table, .. }
        | MigrationOperation::DropPrimaryKey { table, .. }
        | MigrationOperation::CreateIndex { table, .. }
        | MigrationOperation::DropIndex { table, .. } => table,
    }
}

fn sort_bucket(bucket: &mut [MigrationOperation]) {
    bucket.sort_by(|a, b| {
        op_table(a)
            .cmp(op_table(b))
            .then_with(|| a.key().cmp(&b.key()))
    });
}

pub fn resolve_dependencies(ops: Vec<MigrationOperation>) -> Vec<MigrationOperation> {
    let mut create_tables = Vec::new();
    let mut adds = Vec::new();
    let mut alters = Vec::new();
    let mut drops = Vec::new();
    let mut indexes = Vec::new();
    for op in ops {
        match op {
            MigrationOperation::CreateTable { .. } => create_tables.push(op),
            MigrationOperation::AddColumn { .. } | MigrationOperation::AddPrimaryKey { .. } => {
                adds.push(op)
            }
            MigrationOperation::AlterColumnType { .. }
            | MigrationOperation::SetNullable { .. }
            | MigrationOperation::SetDefault { .. }
            | MigrationOperation::SetComment { .. }
            | MigrationOperation::SetAutoIncrement { .. } => alters.push(op),
            MigrationOperation::CreateIndex { .. } => indexes.push(op),
            MigrationOperation::DropIndex { .. }
            | MigrationOperation::DropPrimaryKey { .. }
            | MigrationOperation::DropColumn { .. } => drops.push(op),
        }
    }
    sort_bucket(&mut create_tables);
    sort_bucket(&mut adds);
    sort_bucket(&mut alters);
    sort_bucket(&mut indexes);
    sort_bucket(&mut drops);
    create_tables
        .into_iter()
        .chain(adds)
        .chain(alters)
        .chain(indexes)
        .chain(drops)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_diff::operations::MigrationOperation;

    #[test]
    fn ordering_places_create_before_index_and_drop() {
        let ops = vec![
            MigrationOperation::DropColumn {
                table: "t".into(),
                column: crate::schema_diff::types::ColumnSnapshot {
                    name: "old".into(),
                    data_type: "int".into(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            },
            MigrationOperation::CreateIndex {
                table: "t".into(),
                index: crate::db::IndexInfo {
                    name: "idx".into(),
                    columns: vec!["id".into()],
                    is_unique: false,
                    is_primary: false,
                    index_type: "btree".into(),
                },
            },
            MigrationOperation::AddColumn {
                table: "t".into(),
                column: crate::schema_diff::types::ColumnSnapshot {
                    name: "new".into(),
                    data_type: "int".into(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            },
        ];
        let sorted = resolve_dependencies(ops);
        assert!(matches!(sorted[0], MigrationOperation::AddColumn { .. }));
        assert!(matches!(sorted[1], MigrationOperation::CreateIndex { .. }));
        assert!(matches!(sorted[2], MigrationOperation::DropColumn { .. }));
    }

    fn snap(name: &str) -> crate::schema_diff::types::ColumnSnapshot {
        crate::schema_diff::types::ColumnSnapshot {
            name: name.into(),
            data_type: "int".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    #[test]
    fn same_bucket_ops_are_sorted_by_table_and_key() {
        let ops = vec![
            MigrationOperation::SetDefault {
                table: "b".into(),
                column: "z".into(),
                from: None,
                to: Some("1".into()),
            },
            MigrationOperation::SetDefault {
                table: "a".into(),
                column: "y".into(),
                from: None,
                to: Some("0".into()),
            },
            MigrationOperation::SetDefault {
                table: "a".into(),
                column: "x".into(),
                from: None,
                to: Some("0".into()),
            },
        ];
        let sorted = resolve_dependencies(ops.clone());
        let again = resolve_dependencies(ops);
        assert_eq!(sorted, again);
        assert!(
            matches!(&sorted[0], MigrationOperation::SetDefault { table, column, .. } if table == "a" && column == "x")
        );
        assert!(
            matches!(&sorted[1], MigrationOperation::SetDefault { table, column, .. } if table == "a" && column == "y")
        );
        assert!(
            matches!(&sorted[2], MigrationOperation::SetDefault { table, column, .. } if table == "b" && column == "z")
        );
    }

    #[test]
    fn drop_bucket_sorts_deterministically() {
        let ops = vec![
            MigrationOperation::DropColumn {
                table: "t".into(),
                column: snap("z"),
            },
            MigrationOperation::DropColumn {
                table: "t".into(),
                column: snap("a"),
            },
        ];
        let sorted = resolve_dependencies(ops);
        assert!(
            matches!(&sorted[0], MigrationOperation::DropColumn { column, .. } if column.name == "a")
        );
        assert!(
            matches!(&sorted[1], MigrationOperation::DropColumn { column, .. } if column.name == "z")
        );
    }
}
