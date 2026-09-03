//! Schema Diff IR: convert snapshots into dialect-neutral operations.

use super::{compare::diff_indexes, operations::MigrationOperation, types::ColumnChange};
use crate::db::TableSchema;

/// Table-level PK columns: prefer `primary_keys`, fall back to column flags.
fn effective_primary_keys(schema: &TableSchema) -> Vec<String> {
    if !schema.primary_keys.is_empty() {
        return schema.primary_keys.clone();
    }
    schema
        .columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect()
}

pub fn diff_to_operations(
    table: &str,
    source: &TableSchema,
    target: &TableSchema,
) -> Vec<MigrationOperation> {
    let diff = super::compare::diff_table_schemas(table, source, target);
    let mut ops = Vec::new();

    if target.columns.is_empty() && !source.columns.is_empty() {
        ops.push(MigrationOperation::CreateTable {
            table: table.into(),
            columns: source
                .columns
                .iter()
                .map(super::compare::column_snapshot)
                .collect(),
            primary_keys: source.primary_keys.clone(),
        });
    } else {
        for c in diff.missing_on_target {
            ops.push(MigrationOperation::AddColumn {
                table: table.into(),
                column: c,
            });
        }
        for c in diff.extra_on_target {
            ops.push(MigrationOperation::DropColumn {
                table: table.into(),
                column: c,
            });
        }
        for c in diff.changed {
            for change in c.changes {
                let op = match change {
                    ColumnChange::DataType => MigrationOperation::AlterColumnType {
                        table: table.into(),
                        column: c.name.clone(),
                        from: c.target.data_type.clone(),
                        to: c.source.data_type.clone(),
                    },
                    ColumnChange::Nullable => MigrationOperation::SetNullable {
                        table: table.into(),
                        column: c.name.clone(),
                        nullable: c.source.nullable,
                    },
                    ColumnChange::Default => MigrationOperation::SetDefault {
                        table: table.into(),
                        column: c.name.clone(),
                        from: c.target.default_value.clone(),
                        to: c.source.default_value.clone(),
                    },
                    ColumnChange::Comment => MigrationOperation::SetComment {
                        table: table.into(),
                        column: c.name.clone(),
                        from: c.target.comment.clone(),
                        to: c.source.comment.clone(),
                    },
                    ColumnChange::AutoIncrement => MigrationOperation::SetAutoIncrement {
                        table: table.into(),
                        column: c.name.clone(),
                        from: c.target.is_auto_increment,
                        to: c.source.is_auto_increment,
                    },
                    // Column-level PK flag; table-level ops come from effective_primary_keys diff below.
                    ColumnChange::PrimaryKey => continue,
                };
                ops.push(op);
            }
        }
    }

    let source_pks = effective_primary_keys(source);
    let target_pks = effective_primary_keys(target);
    if source_pks != target_pks {
        if !target_pks.is_empty() {
            ops.push(MigrationOperation::DropPrimaryKey {
                table: table.into(),
                columns: target_pks,
            });
        }
        if !source_pks.is_empty() {
            ops.push(MigrationOperation::AddPrimaryKey {
                table: table.into(),
                columns: source_pks,
            });
        }
    }

    let indexes = diff_indexes(source, target);
    for index in indexes.missing_on_target {
        ops.push(MigrationOperation::CreateIndex {
            table: table.into(),
            index,
        });
    }
    for index in indexes.extra_on_target {
        ops.push(MigrationOperation::DropIndex {
            table: table.into(),
            index,
        });
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ColumnSchema;
    fn col(n: &str) -> ColumnSchema {
        ColumnSchema {
            name: n.into(),
            data_type: "int".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }
    fn schema(c: Vec<ColumnSchema>) -> TableSchema {
        TableSchema {
            table_name: "t".into(),
            columns: c,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    #[test]
    fn produces_neutral_operations() {
        let s = schema(vec![col("id"), col("name")]);
        let t = schema(vec![col("id")]);
        let ops = diff_to_operations("t", &s, &t);
        assert!(matches!(&ops[0], MigrationOperation::AddColumn { .. }));
    }

    #[test]
    fn primary_key_change_generates_drop_and_add() {
        let mut s = schema(vec![col("id"), col("user_id")]);
        s.primary_keys = vec!["user_id".into()];
        let mut t = schema(vec![col("id"), col("user_id")]);
        t.primary_keys = vec!["id".into()];
        let ops = diff_to_operations("t", &s, &t);
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::DropPrimaryKey { columns, .. } if columns == &["id"])));
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::AddPrimaryKey { columns, .. } if columns == &["user_id"])));
    }

    #[test]
    fn primary_key_change_from_column_flags_when_vectors_empty() {
        let mut id = col("id");
        id.is_primary_key = false;
        let mut user_id = col("user_id");
        user_id.is_primary_key = true;
        let s = schema(vec![id, user_id]);
        let mut tgt_id = col("id");
        tgt_id.is_primary_key = true;
        let tgt_user = col("user_id");
        let t = schema(vec![tgt_id, tgt_user]);
        let ops = diff_to_operations("t", &s, &t);
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::DropPrimaryKey { columns, .. } if columns == &["id"])));
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::AddPrimaryKey { columns, .. } if columns == &["user_id"])));
    }

    #[test]
    fn index_definition_change_generates_drop_and_create_ops() {
        use crate::db::IndexInfo;
        let mut s = schema(vec![col("id"), col("email")]);
        s.indexes.push(IndexInfo {
            name: "idx_email".into(),
            columns: vec!["email".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let mut t = schema(vec![col("id"), col("email")]);
        t.indexes.push(IndexInfo {
            name: "idx_email".into(),
            columns: vec!["id".into(), "email".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let ops = diff_to_operations("t", &s, &t);
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::DropIndex { index, .. } if index.columns == vec!["id", "email"])));
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::CreateIndex { index, .. } if index.columns == vec!["email"])));
    }
}
