use super::*;
use crate::operations::MigrationOperation;

#[test]
fn ordering_places_create_before_index_and_drop() {
    let ops = vec![
        MigrationOperation::DropColumn {
            table: "t".into(),
            column: crate::types::ColumnSnapshot {
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
            index: datazen_driver_api::IndexInfo {
                name: "idx".into(),
                columns: vec!["id".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
            },
        },
        MigrationOperation::AddColumn {
            table: "t".into(),
            column: crate::types::ColumnSnapshot {
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
    let sorted = resolve_dependencies(ops.clone());
    let add = sorted
        .iter()
        .position(|operation| matches!(operation, MigrationOperation::AddColumn { .. }))
        .unwrap();
    let index = sorted
        .iter()
        .position(|operation| matches!(operation, MigrationOperation::CreateIndex { .. }))
        .unwrap();
    assert!(add < index);
    assert_eq!(
        sorted,
        resolve_dependencies(ops.into_iter().rev().collect())
    );
}

pub(super) fn snap(name: &str) -> crate::types::ColumnSnapshot {
    crate::types::ColumnSnapshot {
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

#[test]
fn explicit_table_drop_dependencies_place_dependent_before_referenced_table() {
    let ops = vec![
        MigrationOperation::DropTable {
            table: "a_parent".into(),
        },
        MigrationOperation::DropTable {
            table: "z_child".into(),
        },
    ];
    let dependencies = vec![("z_child".into(), "a_parent".into())];
    let sorted = try_resolve_dependencies_with_table_drop_edges(&ops, &dependencies).unwrap();

    assert!(matches!(sorted[0], MigrationOperation::DropTable { ref table } if table == "z_child"));
    assert!(
        matches!(sorted[1], MigrationOperation::DropTable { ref table } if table == "a_parent")
    );
}

#[test]
fn cyclic_table_drop_dependencies_fail_closed() {
    let ops = vec![
        MigrationOperation::DropTable {
            table: "a_parent".into(),
        },
        MigrationOperation::DropTable {
            table: "z_child".into(),
        },
    ];
    let dependencies = vec![
        ("z_child".into(), "a_parent".into()),
        ("a_parent".into(), "z_child".into()),
    ];
    let error = try_resolve_dependencies_with_table_drop_edges(&ops, &dependencies)
        .expect_err("a dependency cycle must not produce drop SQL");

    assert!(error.contains("cycle"), "{error}");
}

#[test]
fn sequence_operations_have_deterministic_create_replace_drop_order() {
    let make = |name: &str| {
        datazen_driver_api::MigrationSequence {
            schema: Some("public".into()),
            name: name.into(),
            definition: format!(
                "CREATE SEQUENCE \"public\".\"{name}\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE;"
            ),
        }
    };
    let create = MigrationOperation::CreateSequence {
        sequence: make("s"),
    };
    let replace = MigrationOperation::ReplaceSequence {
        current: make("s"),
        desired: make("s"),
    };
    let drop = MigrationOperation::DropSequence {
        sequence: make("s"),
    };
    let sorted = resolve_dependencies(vec![drop.clone(), create.clone(), replace.clone()]);
    assert_eq!(sorted, vec![create, replace, drop]);
}

#[test]
fn test_tester_foreign_key_removal_precedes_every_referenced_structure_change() {
    let foreign_key = datazen_driver_api::ForeignKeyInfo {
        name: "orders_user_id_fk".into(),
        columns: vec!["user_id".into()],
        referenced_table: "users".into(),
        referenced_columns: vec!["id".into()],
        on_update: "CASCADE".into(),
        on_delete: "RESTRICT".into(),
        deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
    };
    let drop_fk = MigrationOperation::DropForeignKey {
        table: "orders".into(),
        foreign_key: foreign_key.clone(),
    };
    let candidates = vec![
        MigrationOperation::DropColumn {
            table: "orders".into(),
            column: snap("user_id"),
        },
        MigrationOperation::DropPrimaryKey {
            table: "users".into(),
            columns: vec!["id".into()],
        },
        MigrationOperation::AlterColumnType {
            table: "orders".into(),
            column: "user_id".into(),
            from: "integer".into(),
            to: "bigint".into(),
        },
        MigrationOperation::SetNullable {
            table: "orders".into(),
            column: "user_id".into(),
            nullable: true,
        },
        MigrationOperation::SetDefault {
            table: "orders".into(),
            column: "user_id".into(),
            from: None,
            to: Some("0".into()),
        },
        MigrationOperation::SetComment {
            table: "orders".into(),
            column: "user_id".into(),
            from: None,
            to: Some("owner".into()),
        },
        MigrationOperation::SetAutoIncrement {
            table: "orders".into(),
            column: "user_id".into(),
            from: false,
            to: true,
        },
        MigrationOperation::DropIndex {
            table: "orders".into(),
            index: datazen_driver_api::IndexInfo {
                name: "orders_user_id_idx".into(),
                columns: vec!["user_id".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
            },
        },
        MigrationOperation::DropTable {
            table: "users".into(),
        },
    ];

    for change in candidates {
        let sorted = resolve_dependencies(vec![change.clone(), drop_fk.clone()]);
        let fk_position = sorted.iter().position(|operation| operation == &drop_fk);
        let change_position = sorted.iter().position(|operation| operation == &change);
        assert!(
            matches!((fk_position, change_position), (Some(fk), Some(change)) if fk < change),
            "foreign key must be removed before {change:?}; got {sorted:?}"
        );
    }
}
