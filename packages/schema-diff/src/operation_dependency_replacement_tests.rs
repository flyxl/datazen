use super::*;
use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo, IndexInfo};
fn pk(add: bool) -> MigrationOperation {
    if add {
        MigrationOperation::AddPrimaryKey {
            table: "t".into(),
            columns: vec!["new".into()],
        }
    } else {
        MigrationOperation::DropPrimaryKey {
            table: "t".into(),
            columns: vec!["old".into()],
        }
    }
}
fn idx(create: bool) -> MigrationOperation {
    let index = IndexInfo {
        name: "same_name".into(),
        columns: vec![if create { "new".into() } else { "old".into() }],
        is_unique: create,
        is_primary: false,
        index_type: "btree".into(),
    };
    if create {
        MigrationOperation::CreateIndex {
            table: "t".into(),
            index,
        }
    } else {
        MigrationOperation::DropIndex {
            table: "t".into(),
            index,
        }
    }
}

fn fk(add: bool) -> MigrationOperation {
    let foreign_key = ForeignKeyInfo {
        name: "orders_user_id_fk".into(),
        columns: vec!["user_id".into()],
        referenced_table: "users".into(),
        referenced_columns: vec!["id".into()],
        on_update: "CASCADE".into(),
        on_delete: "RESTRICT".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    };
    if add {
        MigrationOperation::AddForeignKey {
            table: "orders".into(),
            foreign_key,
        }
    } else {
        MigrationOperation::DropForeignKey {
            table: "orders".into(),
            foreign_key,
        }
    }
}
#[test]
fn replacement_order_is_drop_before_create_regardless_of_input() {
    for (add, drop) in [
        (pk(true), pk(false)),
        (idx(true), idx(false)),
        (fk(true), fk(false)),
    ] {
        assert_eq!(
            resolve_dependencies(vec![add.clone(), drop.clone()]),
            vec![drop.clone(), add.clone()]
        );
        assert_eq!(
            resolve_dependencies(vec![drop.clone(), add.clone()]),
            vec![drop, add]
        );
    }
}
#[test]
fn replacing_key_relaxes_old_column_and_tightens_new_without_cycles() {
    let relax = MigrationOperation::SetNullable {
        table: "t".into(),
        column: "old".into(),
        nullable: true,
    };
    let tighten = MigrationOperation::SetNullable {
        table: "t".into(),
        column: "new".into(),
        nullable: false,
    };
    let unrelated = MigrationOperation::SetNullable {
        table: "t".into(),
        column: "other".into(),
        nullable: true,
    };
    let all = vec![
        pk(true),
        relax.clone(),
        tighten.clone(),
        unrelated.clone(),
        pk(false),
    ];
    let sorted = resolve_dependencies(all.clone());
    assert_eq!(sorted.len(), all.len());
    let position = |op: &MigrationOperation| sorted.iter().position(|item| item == op).unwrap();
    assert!(position(&pk(false)) < position(&relax));
    assert!(position(&pk(false)) < position(&pk(true)));
    assert!(position(&tighten) < position(&pk(true)));
    let mut selected = vec![relax, tighten.clone(), unrelated.clone(), pk(true)];
    retain_dependency_closed(&all, &mut selected);
    assert_eq!(selected, vec![tighten, unrelated]);
}

#[test]
fn filtering_either_half_removes_the_entire_replacement() {
    for all in [
        vec![pk(true), pk(false)],
        vec![idx(true), idx(false)],
        vec![fk(true), fk(false)],
    ] {
        for half in &all {
            let mut selected = vec![half.clone()];
            retain_dependency_closed(&all, &mut selected);
            assert!(selected.is_empty());
        }
    }
}

#[test]
fn changing_column_type_orders_default_drop_and_replacement_as_one_transition() {
    let drop_default = MigrationOperation::SetDefault {
        table: "users".into(),
        column: "active".into(),
        from: Some("1".into()),
        to: None,
    };
    let alter_type = MigrationOperation::AlterColumnType {
        table: "users".into(),
        column: "active".into(),
        from: "integer".into(),
        to: "boolean".into(),
    };
    let set_default = MigrationOperation::SetDefault {
        table: "users".into(),
        column: "active".into(),
        from: None,
        to: Some("TRUE".into()),
    };
    let all = vec![
        set_default.clone(),
        alter_type.clone(),
        drop_default.clone(),
    ];

    let mut selected_without_type_change = vec![drop_default.clone()];
    retain_dependency_closed(&all, &mut selected_without_type_change);
    assert!(selected_without_type_change.is_empty());

    let ordered = resolve_dependencies(all);
    assert_eq!(ordered, vec![drop_default, alter_type, set_default]);
}

#[test]
fn column_drop_requires_removing_its_index_and_key() {
    let column = super::tests::snap("old");
    let drop = MigrationOperation::DropColumn {
        table: "t".into(),
        column,
    };
    let all = vec![drop.clone(), pk(false), idx(false)];
    let sorted = resolve_dependencies(all.clone());
    assert_eq!(sorted.last(), Some(&drop));
    let mut selected = vec![drop];
    retain_dependency_closed(&all, &mut selected);
    assert!(selected.is_empty());
}

#[test]
fn foreign_key_waits_for_referenced_table_and_columns() {
    let add_ref_table = MigrationOperation::CreateTable {
        table: "users".into(),
        columns: vec![super::tests::snap("id")],
        primary_keys: vec!["id".into()],
        table_options: Default::default(),
    };
    let add_local_table = MigrationOperation::CreateTable {
        table: "orders".into(),
        columns: vec![super::tests::snap("user_id")],
        primary_keys: vec![],
        table_options: Default::default(),
    };
    let sorted = resolve_dependencies(vec![fk(true), add_local_table, add_ref_table]);
    let fk_position = sorted
        .iter()
        .position(|op| matches!(op, MigrationOperation::AddForeignKey { .. }))
        .unwrap();
    assert!(sorted[..fk_position]
        .iter()
        .any(|op| matches!(op, MigrationOperation::CreateTable { table, .. } if table == "users")));
    assert!(sorted[..fk_position].iter().any(
        |op| matches!(op, MigrationOperation::CreateTable { table, .. } if table == "orders")
    ));
}

#[test]
fn typed_type_table_fk_trigger_chain_is_topologically_ordered() {
    let status_type = MigrationOperation::CreateType {
        type_definition: datazen_driver_api::MigrationType {
            schema: Some("public".into()),
            name: "order_status".into(),
            definition: "CREATE TYPE public.order_status AS ENUM ('new')".into(),
        },
    };
    let orders = MigrationOperation::CreateTable {
        table: "public.orders".into(),
        columns: vec![
            super::tests::snap("user_id"),
            crate::types::ColumnSnapshot {
                name: "status".into(),
                data_type: "public.order_status".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".into()],
        table_options: Default::default(),
    };
    let users = MigrationOperation::CreateTable {
        table: "public.users".into(),
        columns: vec![super::tests::snap("id")],
        primary_keys: vec!["id".into()],
        table_options: Default::default(),
    };
    let foreign_key = MigrationOperation::AddForeignKey {
        table: "public.orders".into(),
        foreign_key: ForeignKeyInfo {
            name: "orders_user_id_fk".into(),
            columns: vec!["user_id".into()],
            referenced_table: "public.users".into(),
            referenced_columns: vec!["id".into()],
            on_update: "CASCADE".into(),
            on_delete: "RESTRICT".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        },
    };
    let trigger = MigrationOperation::CreateTrigger {
        trigger: datazen_driver_api::MigrationTrigger {
            schema: Some("public".into()),
            name: "orders_audit".into(),
            target_schema: Some("public".into()),
            target_name: "orders".into(),
            definition: "CREATE TRIGGER orders_audit AFTER INSERT ON public.orders".into(),
        },
    };

    let ordered = try_resolve_dependencies(&[
        trigger.clone(),
        foreign_key.clone(),
        orders.clone(),
        status_type.clone(),
        users.clone(),
    ])
    .unwrap();
    let position = |expected: &MigrationOperation| {
        ordered
            .iter()
            .position(|operation| operation == expected)
            .unwrap()
    };
    assert!(position(&status_type) < position(&orders));
    assert!(position(&orders) < position(&foreign_key));
    assert!(position(&users) < position(&foreign_key));
    assert!(position(&orders) < position(&trigger));
}

#[test]
fn operation_identity_keeps_overloads_and_trigger_targets_distinct() {
    let routine = |signature: &str| MigrationOperation::CreateRoutine {
        routine: datazen_driver_api::MigrationRoutine {
            kind: datazen_driver_api::ObjectKind::Function,
            schema: Some("public".into()),
            name: "lookup".into(),
            signature: Some(signature.into()),
            definition: String::new(),
        },
    };
    assert_ne!(
        operation_identity(&routine("integer")),
        operation_identity(&routine("text"))
    );

    let trigger = |table: &str| MigrationOperation::CreateTrigger {
        trigger: datazen_driver_api::MigrationTrigger {
            schema: Some("public".into()),
            name: "audit".into(),
            target_schema: Some("public".into()),
            target_name: table.into(),
            definition: String::new(),
        },
    };
    assert_ne!(
        operation_identity(&trigger("orders")),
        operation_identity(&trigger("users"))
    );
}

#[test]
fn unqualified_foreign_key_reference_is_rejected_when_selected_tables_are_ambiguous() {
    let make_table = |table: &str| MigrationOperation::CreateTable {
        table: table.into(),
        columns: vec![super::tests::snap("id")],
        primary_keys: vec!["id".into()],
        table_options: Default::default(),
    };
    let error = try_resolve_dependencies(&[
        fk(true),
        make_table("public.users"),
        make_table("audit.users"),
    ])
    .expect_err("unqualified FK must not guess between schemas");
    assert!(error.contains("ambiguous migration dependency reference"));

    let qualified_fk = || MigrationOperation::AddForeignKey {
        table: "orders".into(),
        foreign_key: ForeignKeyInfo {
            name: "orders_user_id_fk".into(),
            columns: vec!["user_id".into()],
            referenced_table: "public.users".into(),
            referenced_columns: vec!["id".into()],
            on_update: "CASCADE".into(),
            on_delete: "RESTRICT".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        },
    };
    let operations = vec![
        qualified_fk(),
        make_table("audit.users"),
        make_table("public.users"),
    ];
    let qualified = &operations[0];
    let audit = &operations[1];
    let public = &operations[2];
    assert!(!precedes(audit, qualified));
    assert!(precedes(public, qualified));
    let ordered = try_resolve_dependencies(&operations).unwrap();
    let fk_position = ordered
        .iter()
        .position(|operation| matches!(operation, MigrationOperation::AddForeignKey { .. }))
        .unwrap();
    assert!(ordered[..fk_position].iter().any(
            |operation| matches!(operation, MigrationOperation::CreateTable { table, .. } if table == "public.users")
        ));
}

#[test]
fn basename_only_fk_trigger_and_type_references_fail_closed() {
    let qualified_table = MigrationOperation::CreateTable {
        table: "public.users".into(),
        columns: vec![super::tests::snap("id")],
        primary_keys: vec!["id".into()],
        table_options: Default::default(),
    };
    let fk_error = try_resolve_dependencies(&[fk(true), qualified_table.clone()])
        .expect_err("an unqualified FK reference cannot prove which schema is intended");
    assert!(fk_error.contains("basename match"));

    let trigger = MigrationOperation::CreateTrigger {
        trigger: datazen_driver_api::MigrationTrigger {
            schema: None,
            name: "users_audit".into(),
            target_schema: None,
            target_name: "users".into(),
            definition: "CREATE TRIGGER users_audit AFTER INSERT ON users".into(),
        },
    };
    let trigger_error = try_resolve_dependencies(&[trigger, qualified_table])
        .expect_err("an unqualified trigger target cannot prove a schema");
    assert!(trigger_error.contains("basename match"));

    let type_operation = MigrationOperation::CreateType {
        type_definition: datazen_driver_api::MigrationType {
            schema: Some("public".into()),
            name: "order_status".into(),
            definition: "CREATE TYPE public.order_status AS ENUM ('new')".into(),
        },
    };
    let table_operation = MigrationOperation::CreateTable {
        table: "public.orders".into(),
        columns: vec![crate::types::ColumnSnapshot {
            name: "status".into(),
            data_type: "order_status".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }],
        primary_keys: vec![],
        table_options: Default::default(),
    };
    let type_error = try_resolve_dependencies(&[type_operation, table_operation])
        .expect_err("an unqualified custom type cannot prove its schema");
    assert!(type_error.contains("basename only"));
}

#[test]
fn dropping_foreign_key_precedes_local_column_drop() {
    let drop_column = MigrationOperation::DropColumn {
        table: "orders".into(),
        column: super::tests::snap("user_id"),
    };
    let sorted = resolve_dependencies(vec![drop_column.clone(), fk(false)]);
    assert!(matches!(
        sorted.as_slice(),
        [
            MigrationOperation::DropForeignKey { .. },
            MigrationOperation::DropColumn { .. }
        ]
    ));
}

#[test]
fn dropping_referenced_table_waits_for_foreign_key_removal() {
    let drop_table = MigrationOperation::DropTable {
        table: "users".into(),
    };
    let drop_fk = fk(false);
    let sorted = resolve_dependencies(vec![drop_table.clone(), drop_fk.clone()]);
    assert_eq!(sorted, vec![drop_fk, drop_table]);
}
