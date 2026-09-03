//! Schema Diff IR: convert snapshots into dialect-neutral operations.

use super::{compare::diff_indexes, operations::MigrationOperation, types::ColumnChange};
use crate::db::TableSchema;

pub fn diff_to_operations(table: &str, source: &TableSchema, target: &TableSchema) -> Vec<MigrationOperation> {
    let diff = super::compare::diff_table_schemas(table, source, target);
    let mut ops = Vec::new();

    if target.columns.is_empty() && !source.columns.is_empty() {
        ops.push(MigrationOperation::CreateTable {
            table: table.into(),
            columns: source.columns.iter().map(super::compare::column_snapshot).collect(),
            primary_keys: source.primary_keys.clone(),
        });
    } else {
        for c in diff.missing_on_target {
            ops.push(MigrationOperation::AddColumn { table: table.into(), column: c });
        }
        for c in diff.extra_on_target {
            ops.push(MigrationOperation::DropColumn { table: table.into(), column: c });
        }
        for c in diff.changed {
            for change in c.changes {
                let op = match change {
                    ColumnChange::DataType => MigrationOperation::AlterColumnType { table: table.into(), column: c.name.clone(), from: c.target.data_type.clone(), to: c.source.data_type.clone() },
                    ColumnChange::Nullable => MigrationOperation::SetNullable { table: table.into(), column: c.name.clone(), nullable: c.source.nullable },
                    ColumnChange::Default => MigrationOperation::SetDefault { table: table.into(), column: c.name.clone(), from: c.target.default_value.clone(), to: c.source.default_value.clone() },
                    ColumnChange::Comment => MigrationOperation::SetComment { table: table.into(), column: c.name.clone(), from: c.target.comment.clone(), to: c.source.comment.clone() },
                    ColumnChange::AutoIncrement => MigrationOperation::SetAutoIncrement { table: table.into(), column: c.name.clone(), from: c.target.is_auto_increment, to: c.source.is_auto_increment },
                    ColumnChange::PrimaryKey => continue,
                };
                ops.push(op);
            }
        }
    }

    if source.primary_keys != target.primary_keys {
        if !target.primary_keys.is_empty() { ops.push(MigrationOperation::DropPrimaryKey { table: table.into(), columns: target.primary_keys.clone() }); }
        if !source.primary_keys.is_empty() { ops.push(MigrationOperation::AddPrimaryKey { table: table.into(), columns: source.primary_keys.clone() }); }
    }

    let indexes = diff_indexes(source, target);
    for index in indexes.missing_on_target { ops.push(MigrationOperation::CreateIndex { table: table.into(), index }); }
    for index in indexes.extra_on_target { ops.push(MigrationOperation::DropIndex { table: table.into(), index }); }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnSchema;
    fn col(n: &str) -> ColumnSchema { ColumnSchema { name:n.into(), data_type:"int".into(), nullable:true, default_value:None, comment:None, is_primary_key:false, is_auto_increment:false } }
    fn schema(c: Vec<ColumnSchema>) -> TableSchema { TableSchema { table_name:"t".into(), columns:c, primary_keys:vec![], indexes:vec![], foreign_keys:vec![] } }

    #[test]
    fn produces_neutral_operations() {
        let mut s = schema(vec![col("id"), col("name")]);
        let t = schema(vec![col("id")]);
        let ops = diff_to_operations("t", &s, &t);
        assert!(matches!(&ops[0], MigrationOperation::AddColumn { .. }));
    }
}
