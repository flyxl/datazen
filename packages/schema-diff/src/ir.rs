//! Schema Diff IR: convert snapshots into dialect-neutral operations.

use super::{compare::diff_indexes, operations::MigrationOperation, types::ColumnChange};
use datazen_driver_api::TypeNormalizer;
use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo, TableSchema};

pub fn diff_to_operations(
    table: &str,
    source: &TableSchema,
    target: &TableSchema,
    normalizer: Option<&dyn TypeNormalizer>,
) -> Vec<MigrationOperation> {
    let diff = super::compare::diff_table_schemas(table, source, target, normalizer);
    let mut ops = Vec::new();

    let is_new_table = target.columns.is_empty() && !source.columns.is_empty();

    if is_new_table {
        ops.push(MigrationOperation::CreateTable {
            table: table.into(),
            columns: source
                .columns
                .iter()
                .map(super::compare::column_snapshot)
                .collect(),
            primary_keys: source.effective_primary_keys(),
            table_options: source.table_options.clone(),
        });
    } else if source.columns.is_empty() && !target.columns.is_empty() {
        // A missing desired table is represented explicitly so a target-only
        // table cannot be reduced to a sequence of column drops. The renderer
        // emits plain DROP TABLE without CASCADE; the deploy gate therefore
        // requires destructive approval and never advertises DDL rollback.
        ops.push(MigrationOperation::DropTable {
            table: table.into(),
        });
        return ops;
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

        let source_pks = source.effective_primary_keys();
        let target_pks = target.effective_primary_keys();
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

    let source_by_name = source
        .foreign_keys
        .iter()
        .map(|foreign_key| (foreign_key.name.as_str(), foreign_key))
        .collect::<std::collections::HashMap<_, _>>();
    let target_by_name = target
        .foreign_keys
        .iter()
        .map(|foreign_key| (foreign_key.name.as_str(), foreign_key))
        .collect::<std::collections::HashMap<_, _>>();

    for (name, source_foreign_key) in &source_by_name {
        match target_by_name.get(name) {
            None => ops.push(MigrationOperation::AddForeignKey {
                table: table.into(),
                foreign_key: (*source_foreign_key).clone(),
            }),
            Some(target_foreign_key)
                if !foreign_key_definition_equal(source_foreign_key, target_foreign_key) =>
            {
                ops.push(MigrationOperation::DropForeignKey {
                    table: table.into(),
                    foreign_key: (*target_foreign_key).clone(),
                });
                ops.push(MigrationOperation::AddForeignKey {
                    table: table.into(),
                    foreign_key: (*source_foreign_key).clone(),
                });
            }
            Some(_) => {}
        }
    }

    for (name, target_foreign_key) in &target_by_name {
        if !source_by_name.contains_key(name) {
            ops.push(MigrationOperation::DropForeignKey {
                table: table.into(),
                foreign_key: (*target_foreign_key).clone(),
            });
        }
    }

    for constraint in &diff.missing_check_constraints {
        ops.push(MigrationOperation::AddCheckConstraint {
            table: table.into(),
            constraint: datazen_driver_api::CheckConstraint {
                name: constraint.name.clone(),
                expression: constraint.expression.clone(),
            },
        });
    }
    for constraint in &diff.extra_check_constraints {
        ops.push(MigrationOperation::DropCheckConstraint {
            table: table.into(),
            constraint: datazen_driver_api::CheckConstraint {
                name: constraint.name.clone(),
                expression: constraint.expression.clone(),
            },
        });
    }

    if !is_new_table {
        if let Some(table_options) = diff.table_options {
            ops.push(MigrationOperation::SetTableOptions {
                table: table.into(),
                from: table_options.target,
                to: table_options.source,
            });
        }
    }

    ops
}

fn foreign_key_definition_equal(left: &ForeignKeyInfo, right: &ForeignKeyInfo) -> bool {
    left.columns == right.columns
        && left.referenced_table == right.referenced_table
        && left.referenced_columns == right.referenced_columns
        && normalize_action(&left.on_update) == normalize_action(&right.on_update)
        && normalize_action(&left.on_delete) == normalize_action(&right.on_delete)
        && left.deferrability != ForeignKeyDeferrability::Unknown
        && left.deferrability == right.deferrability
}

fn normalize_action(action: &str) -> String {
    let normalized = action.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        "NO ACTION".into()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ColumnSchema;
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
            check_constraints: vec![],
            table_options: Default::default(),
        }
    }

    #[test]
    fn produces_neutral_operations() {
        let s = schema(vec![col("id"), col("name")]);
        let t = schema(vec![col("id")]);
        let ops = diff_to_operations("t", &s, &t, None);
        assert!(matches!(&ops[0], MigrationOperation::AddColumn { .. }));
    }

    #[test]
    fn new_table_creates_table_with_pks_and_no_redundant_add_pk() {
        let mut s = schema(vec![col("id"), col("name")]);
        s.primary_keys = vec!["id".into()];
        let t = schema(vec![]);
        let ops = diff_to_operations("t", &s, &t, None);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], MigrationOperation::CreateTable { primary_keys, .. } if primary_keys == &["id"])
        );
        assert!(!ops
            .iter()
            .any(|op| matches!(op, MigrationOperation::AddPrimaryKey { .. })));
        assert!(!ops
            .iter()
            .any(|op| matches!(op, MigrationOperation::DropPrimaryKey { .. })));
    }

    #[test]
    fn new_table_carries_table_options_in_create_operation() {
        let mut source = schema(vec![col("id")]);
        source.table_options.engine = Some("InnoDB".into());
        source.table_options.charset = Some("utf8mb4".into());
        let target = schema(vec![]);

        let operations = diff_to_operations("items", &source, &target, None);

        assert_eq!(operations.len(), 1);
        assert!(matches!(
            &operations[0],
            MigrationOperation::CreateTable { table_options, .. }
                if table_options.engine.as_deref() == Some("InnoDB")
                    && table_options.charset.as_deref() == Some("utf8mb4")
        ));
        assert!(!operations
            .iter()
            .any(|operation| matches!(operation, MigrationOperation::SetTableOptions { .. })));
    }

    #[test]
    fn new_table_from_column_flags_creates_table_with_pks() {
        let mut id = col("id");
        id.is_primary_key = true;
        let s = schema(vec![id, col("name")]);
        let t = schema(vec![]);
        let ops = diff_to_operations("t", &s, &t, None);
        assert_eq!(ops.len(), 1);
        assert!(
            matches!(&ops[0], MigrationOperation::CreateTable { primary_keys, .. } if primary_keys == &["id"])
        );
        assert!(!ops
            .iter()
            .any(|op| matches!(op, MigrationOperation::AddPrimaryKey { .. })));
    }

    #[test]
    fn target_only_table_is_one_explicit_drop_table_operation() {
        let source = schema(vec![]);
        let mut target = schema(vec![col("id")]);
        target.primary_keys = vec!["id".into()];
        target.indexes.push(datazen_driver_api::IndexInfo {
            name: "idx_id".into(),
            columns: vec!["id".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });

        let ops = diff_to_operations("archive", &source, &target, None);

        assert_eq!(
            ops,
            vec![MigrationOperation::DropTable {
                table: "archive".into()
            }]
        );
    }

    #[test]
    fn drop_table_does_not_emit_cascade_or_reconstructible_column_drops() {
        let source = schema(vec![]);
        let target = schema(vec![col("id"), col("payload")]);

        let ops = diff_to_operations("audit", &source, &target, None);

        assert!(
            matches!(ops.as_slice(), [MigrationOperation::DropTable { table }] if table == "audit")
        );
        assert!(!ops
            .iter()
            .any(|op| matches!(op, MigrationOperation::DropColumn { .. })));
    }

    #[test]
    fn primary_key_change_generates_drop_and_add() {
        let mut s = schema(vec![col("id"), col("user_id")]);
        s.primary_keys = vec!["user_id".into()];
        let mut t = schema(vec![col("id"), col("user_id")]);
        t.primary_keys = vec!["id".into()];
        let ops = diff_to_operations("t", &s, &t, None);
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
        let ops = diff_to_operations("t", &s, &t, None);
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::DropPrimaryKey { columns, .. } if columns == &["id"])));
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::AddPrimaryKey { columns, .. } if columns == &["user_id"])));
    }

    #[test]
    fn postgres_type_alias_does_not_emit_alter_column_type() {
        use datazen_driver_postgres::PostgresTypeNormalizer;
        let normalizer = PostgresTypeNormalizer;
        let mut src_col = col("id");
        src_col.data_type = "int4".into();
        let mut tgt_col = col("id");
        tgt_col.data_type = "integer".into();
        let s = schema(vec![src_col]);
        let t = schema(vec![tgt_col]);
        let ops = diff_to_operations("t", &s, &t, Some(&normalizer));
        assert!(!ops
            .iter()
            .any(|op| matches!(op, MigrationOperation::AlterColumnType { .. })));
    }

    #[test]
    fn index_definition_change_generates_drop_and_create_ops() {
        use datazen_driver_api::IndexInfo;
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
        let ops = diff_to_operations("t", &s, &t, None);
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::DropIndex { index, .. } if index.columns == vec!["id", "email"])));
        assert!(ops.iter().any(|op| matches!(op, MigrationOperation::CreateIndex { index, .. } if index.columns == vec!["email"])));
    }

    #[test]
    fn foreign_key_definition_change_generates_drop_and_create_ops() {
        let mut source = schema(vec![col("user_id")]);
        source.foreign_keys.push(ForeignKeyInfo {
            name: "fk_user".into(),
            columns: vec!["user_id".into()],
            referenced_table: "users".into(),
            referenced_columns: vec!["id".into()],
            on_update: "CASCADE".into(),
            on_delete: "CASCADE".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        });
        let mut target = source.clone();
        target.foreign_keys[0].on_delete = "RESTRICT".into();
        let ops = diff_to_operations("orders", &source, &target, None);
        assert!(ops.iter().any(|op| matches!(
            op,
            MigrationOperation::DropForeignKey { foreign_key, .. }
                if foreign_key.on_delete == "RESTRICT"
        )));
        assert!(ops.iter().any(|op| matches!(
            op,
            MigrationOperation::AddForeignKey { foreign_key, .. }
                if foreign_key.on_delete == "CASCADE"
        )));
    }

    #[test]
    fn foreign_key_deferrability_change_generates_drop_and_create_ops() {
        let mut source = schema(vec![col("user_id")]);
        source.foreign_keys.push(ForeignKeyInfo {
            name: "fk_user".into(),
            columns: vec!["user_id".into()],
            referenced_table: "users".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::DeferrableInitiallyDeferred,
        });
        let mut target = source.clone();
        target.foreign_keys[0].deferrability =
            ForeignKeyDeferrability::DeferrableInitiallyImmediate;

        let ops = diff_to_operations("orders", &source, &target, None);
        assert!(ops.iter().any(|op| matches!(
            op,
            MigrationOperation::DropForeignKey { foreign_key, .. }
                if foreign_key.deferrability == ForeignKeyDeferrability::DeferrableInitiallyImmediate
        )));
        assert!(ops.iter().any(|op| matches!(
            op,
            MigrationOperation::AddForeignKey { foreign_key, .. }
                if foreign_key.deferrability == ForeignKeyDeferrability::DeferrableInitiallyDeferred
        )));
    }

    #[test]
    fn unknown_foreign_key_deferrability_is_not_assumed_equal() {
        let mut source = schema(vec![col("user_id")]);
        source.foreign_keys.push(ForeignKeyInfo {
            name: "fk_user".into(),
            columns: vec!["user_id".into()],
            referenced_table: "users".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::Unknown,
        });
        let ops = diff_to_operations("orders", &source, &source.clone(), None);

        assert!(ops
            .iter()
            .any(|op| matches!(op, MigrationOperation::DropForeignKey { .. })));
        assert!(ops.iter().any(|op| matches!(
            op,
            MigrationOperation::AddForeignKey { foreign_key, .. }
                if foreign_key.deferrability == ForeignKeyDeferrability::Unknown
        )));
    }

    #[test]
    fn check_constraint_add_and_change_generate_reviewable_operations() {
        let mut source = schema(vec![col("id")]);
        source
            .check_constraints
            .push(datazen_driver_api::CheckConstraint {
                name: "users_age_check".into(),
                expression: "age >= 0".into(),
            });
        let mut target = schema(vec![col("id")]);
        target
            .check_constraints
            .push(datazen_driver_api::CheckConstraint {
                name: "users_age_check".into(),
                expression: "age > 0".into(),
            });
        let ops = diff_to_operations("users", &source, &target, None);
        assert!(ops.iter().any(|op| matches!(
            op,
            MigrationOperation::DropCheckConstraint { constraint, .. }
                if constraint.expression == "age > 0"
        )));
        assert!(ops.iter().any(|op| matches!(
            op,
            MigrationOperation::AddCheckConstraint { constraint, .. }
                if constraint.expression == "age >= 0"
        )));
    }
}
