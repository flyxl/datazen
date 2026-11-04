//! `schema_diff::api` 算法测试（host-boundary F4c-3 自桌面宿主搬移）。
//!
//! 原宿主 `commands/schema_diff/tests/{catalog,selectors}.rs` 逐字迁移：
//! 这些测试断言的是目录读取/选择器解析的行为，属于算法测试，按边界方案
//! §3「算法测试留在 modules」的纪律落位。MockDriver 直接取自 driver-api
//! 的公开测试替身（桌面 `crate::testing::mock_driver` 即其再导出）。

use super::*;
use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{
    ColumnSchema, ForeignKeyDeferrability, ForeignKeyInfo, TableInfo, TableType,
};

fn pg_fk(name: &str) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.into(),
        columns: vec!["parent_id".into()],
        referenced_table: "public.parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    }
}

fn pg_parent_schema(name: &str) -> TableSchema {
    TableSchema {
        table_name: name.into(),
        columns: vec![ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        }],
        primary_keys: vec!["id".into()],
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

fn pg_child_schema(name: &str, include_fk: bool) -> TableSchema {
    TableSchema {
        table_name: name.into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "parent_id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".into()],
        indexes: Vec::new(),
        foreign_keys: if include_fk {
            vec![pg_fk("fk_child_parent")]
        } else {
            Vec::new()
        },
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

fn target_table(name: &str, schema: Option<&str>) -> TableInfo {
    TableInfo {
        name: name.into(),
        schema: schema.map(str::to_owned),
        table_type: TableType::Table,
        row_count: None,
    }
}

// ---------------------------------------------------------------- 目录读取

#[tokio::test]
async fn target_dependency_catalog_reads_all_postgres_schemas_and_excludes_views() {
    let options = MockDriverOptions {
        has_schema_level: true,
        tables: vec![
            target_table("parent", Some("public")),
            target_table("child", Some("archive")),
            TableInfo {
                name: "active_view".into(),
                schema: Some("public".into()),
                table_type: TableType::View,
                row_count: None,
            },
        ],
        table_schemas_by_database: HashMap::from([(
            "target_db".into(),
            HashMap::from([
                ("parent".into(), pg_parent_schema("parent")),
                ("child".into(), pg_child_schema("child", true)),
            ]),
        )]),
        ..Default::default()
    };
    let driver = MockDriver::new("postgres", options);
    let handle = ConnectionHandle {
        id: "catalog-session".into(),
        pool_id: "catalog-pool".into(),
    };
    let selected = vec![("public.parent".into(), pg_parent_schema("public.parent"))];
    let catalog = fetch_target_table_dependency_catalog(
        driver.as_ref(),
        &handle,
        "target_db",
        "postgresql",
        Some("public"),
        &selected,
    )
    .await
    .expect("read PostgreSQL dependency catalog");

    assert_eq!(catalog.len(), 2);
    assert!(catalog
        .iter()
        .any(|(identity, _)| identity == "public.parent"));
    let child = catalog
        .iter()
        .find(|(identity, _)| identity == "archive.child")
        .expect("include table in a non-default schema");
    assert_eq!(child.1.foreign_keys[0].referenced_table, "public.parent");
}

#[tokio::test]
async fn mysql_target_dependency_catalog_fails_closed_without_global_visibility() {
    let driver = MockDriver::new("mysql", MockDriverOptions::default());
    let handle = ConnectionHandle {
        id: "mysql-catalog-session".into(),
        pool_id: "mysql-catalog-pool".into(),
    };
    let error = fetch_target_table_dependency_catalog(
        driver.as_ref(),
        &handle,
        "target_db",
        "mysql",
        None,
        &[],
    )
    .await
    .expect_err("unproven server-wide visibility must block target-only drops");
    assert!(error.to_string().contains("direct global SELECT"));
}

#[tokio::test]
async fn mysql_target_dependency_catalog_uses_database_qualified_identities() {
    let mut archive_child = pg_child_schema("child", true);
    archive_child.foreign_keys[0].referenced_table = "archive.parent".into();
    let options = MockDriverOptions {
        databases: vec!["target_db".into(), "archive".into()],
        complete_foreign_key_catalog_visibility: true,
        tables_by_database: HashMap::from([
            (
                "target_db".into(),
                vec![target_table("parent", None), target_table("child", None)],
            ),
            (
                "archive".into(),
                vec![target_table("parent", None), target_table("child", None)],
            ),
        ]),
        table_schemas_by_database: HashMap::from([
            (
                "target_db".into(),
                HashMap::from([
                    ("parent".into(), pg_parent_schema("parent")),
                    ("child".into(), archive_child),
                ]),
            ),
            (
                "archive".into(),
                HashMap::from([
                    ("parent".into(), pg_parent_schema("parent")),
                    ("child".into(), pg_child_schema("child", false)),
                ]),
            ),
        ]),
        ..Default::default()
    };
    let driver = MockDriver::new("mysql", options);
    let handle = ConnectionHandle {
        id: "mysql-catalog-session".into(),
        pool_id: "mysql-catalog-pool".into(),
    };
    let catalog = fetch_target_table_dependency_catalog(
        driver.as_ref(),
        &handle,
        "target_db",
        "mysql",
        None,
        &[],
    )
    .await
    .expect("read server-wide MySQL dependency catalog");

    assert!(catalog.iter().any(|(identity, _)| identity == "parent"));
    assert!(catalog
        .iter()
        .any(|(identity, _)| identity == "archive.parent"));
    let child = catalog
        .iter()
        .find(|(identity, _)| identity == "child")
        .expect("include selected database child");
    assert_eq!(child.1.foreign_keys[0].referenced_table, "archive.parent");
    assert_eq!(catalog.len(), 4);
}

#[test]
fn is_table_missing_error_detects_various_patterns() {
    assert!(is_table_missing_error(
        "Query failed: error returned from database: 1146 (42S02): Table 'datazen_demo.demo_customers' doesn't exist"
    ));
    assert!(is_table_missing_error(
        "relation \"public.demo_customers\" does not exist"
    ));
    assert!(is_table_missing_error("Table 'users' doesn't exist"));
    assert!(is_table_missing_error("Table not found: users"));
    assert!(!is_table_missing_error("Connection refused"));
    assert!(!is_table_missing_error("Syntax error in SQL statement"));
}

#[test]
fn reviewed_relation_snapshot_uses_its_own_schema_scope() {
    assert_eq!(
        resolve_reviewed_table_snapshot("postgresql", "archive.events", "app", Some("public")),
        ("events".into(), "app".into(), Some("archive".into()))
    );
    assert_eq!(
        resolve_reviewed_table_snapshot("postgresql", "events", "app", Some("public")),
        ("events".into(), "app".into(), Some("public".into()))
    );
    assert_eq!(
        resolve_reviewed_table_snapshot("sqlserver", "sales.orders", "app", Some("dbo")),
        ("orders".into(), "app".into(), Some("sales".into()))
    );
    assert_eq!(
        resolve_reviewed_table_snapshot("sqlserver", "orders", "app", Some("dbo")),
        ("orders".into(), "app".into(), Some("dbo".into()))
    );
    assert_eq!(
        resolve_reviewed_table_snapshot("mysql", "events", "app", None),
        ("events".into(), "app".into(), None)
    );
    assert_eq!(
        resolve_reviewed_table_snapshot("mysql", "archive.events", "app", None),
        ("events".into(), "archive".into(), None)
    );
}

#[test]
fn complete_target_dependency_catalog_rejects_new_relations_and_changed_foreign_keys() {
    let table = |name: &str| TableSchema {
        table_name: name.into(),
        columns: Vec::new(),
        primary_keys: Vec::new(),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        check_constraints: Vec::new(),
        table_options: Default::default(),
    };
    let reviewed = vec![("public.parent".into(), table("parent"))];
    let mut current = reviewed.clone();
    current.push(("public.child".into(), table("child")));
    let error = validate_target_dependency_catalog(&reviewed, &current).unwrap_err();
    assert!(error.contains("new relations: [public.child]"), "{error}");

    let mut reviewed = current;
    let mut changed_child = table("child");
    changed_child
        .foreign_keys
        .push(datazen_driver_api::ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: "public.parent".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
        });
    let changed = vec![
        ("public.parent".into(), table("parent")),
        ("public.child".into(), changed_child),
    ];
    let error = validate_target_dependency_catalog(&reviewed, &changed).unwrap_err();
    assert!(
        error.contains("Target schema changed for public.child"),
        "{error}"
    );
    reviewed.pop();
}

#[test]
fn sqlite_metadata_uses_catalog_name_instead_of_configured_file_path() {
    assert_eq!(
        schema_catalog_database("sqlite", Some("/tmp/source.sqlite")),
        "main"
    );
    assert_eq!(schema_catalog_database("postgresql", Some("app")), "app");
    assert_eq!(schema_catalog_database("sqlserver", Some("app")), "app");
    assert_eq!(schema_catalog_database("mysql", None), "");
    assert_eq!(
        schema_catalog_scope("sqlite", Some("/tmp/target.sqlite")),
        Some("main".into())
    );
    assert_eq!(
        schema_catalog_scope("postgresql", Some("app")),
        Some("app".into())
    );
}

// ---------------------------------------------------------------- 选择器与范围门

#[test]
fn sqlserver_object_only_plan_blocks_cross_schema_statements() {
    let source = vec![SchemaObjectSnapshot::view(
        Some("dbo"),
        "v",
        "SELECT 1 AS value",
    )];
    let target = vec![SchemaObjectSnapshot::view(
        Some("sales"),
        "v",
        "SELECT 2 AS value",
    )];
    let mut plan = SchemaDiffPlan {
        plan_id: None,
        table: "v".into(),
        tables: Vec::new(),
        source_dialect: "sqlserver".into(),
        target_dialect: "sqlserver".into(),
        same_dialect: true,
        statements: vec![super::super::types::PlanStatement {
            sql: "CREATE VIEW [dbo].[v] AS SELECT 1 AS value".into(),
            risk: super::super::types::StatementRisk::Additive,
            rollback_sql: None,
            summary: "create view".into(),
            requires_transaction: false,
        }],
        warnings: Vec::new(),
        requirements: Vec::new(),
        rollback_completeness: super::super::types::RollbackCompleteness {
            complete: false,
            missing: vec!["view rollback is not complete".into()],
        },
        type_suggestions: Vec::new(),
        expected_target_schemas: Vec::new(),
    };

    apply_sqlserver_object_scope_gate(
        &mut plan,
        "sqlserver",
        "sqlserver",
        Some("dbo"),
        Some("sales"),
        &source,
        &target,
    );

    assert!(plan.statements.is_empty());
    assert!(matches!(
        plan.requirements.as_slice(),
        [PlanRequirement::Unsupported { operation, reason }]
            if operation == "sqlserver-object-schema-scope"
                && reason.contains("cannot rewrite object definitions across schemas")
    ));

    assert!(sqlserver_object_scope_requirement(
        "sqlserver",
        "sqlserver",
        None,
        None,
        &[SchemaObjectSnapshot::view(
            Some("dbo"),
            "v",
            "SELECT 1 AS value",
        )],
        &[SchemaObjectSnapshot::view(
            Some("dbo"),
            "v",
            "SELECT 2 AS value",
        )],
    )
    .is_none());
}

#[test]
fn schema_object_selectors_require_unambiguous_identity_and_allow_target_only_drop() {
    let source = vec![DatabaseObject {
        kind: "view".into(),
        schema: Some("public".into()),
        name: "active_users".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    }];
    let target = vec![DatabaseObject {
        kind: "view".into(),
        schema: Some("public".into()),
        name: "legacy_users".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    }];
    let (source_selected, target_selected) =
        select_schema_object_pair(&source, &target, &["public.active_users".into()]).unwrap();
    assert_eq!(source_selected.len(), 1);
    assert!(target_selected.is_empty());
    let (source_selected, target_selected) =
        select_schema_object_pair(&source, &target, &["public.legacy_users".into()]).unwrap();
    assert!(source_selected.is_empty());
    assert_eq!(target_selected.len(), 1);

    let ambiguous = vec![
        DatabaseObject {
            kind: "view".into(),
            schema: Some("one".into()),
            name: "same".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        },
        DatabaseObject {
            kind: "view".into(),
            schema: Some("two".into()),
            name: "same".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        },
    ];
    assert!(select_schema_object_pair(&ambiguous, &[], &["same".into()]).is_err());
}

#[test]
fn schema_object_selectors_preserve_overloads_and_trigger_relations() {
    let functions = vec![
        DatabaseObject {
            kind: "function".into(),
            schema: Some("public".into()),
            name: "lookup".into(),
            signature: Some("integer".into()),
            target_schema: None,
            target_name: None,
        },
        DatabaseObject {
            kind: "function".into(),
            schema: Some("public".into()),
            name: "lookup".into(),
            signature: Some("text".into()),
            target_schema: None,
            target_name: None,
        },
    ];
    let (source, target) =
        select_schema_object_pair(&functions, &functions, &["public.lookup(integer)".into()])
            .unwrap();
    assert_eq!(source.len(), 1);
    assert_eq!(target.len(), 1);
    assert_eq!(source[0].signature.as_deref(), Some("integer"));
    assert!(select_schema_object_pair(&functions, &functions, &["lookup".into()]).is_err());

    let triggers = vec![
        DatabaseObject {
            kind: "trigger".into(),
            schema: Some("public".into()),
            name: "audit".into(),
            signature: None,
            target_schema: Some("public".into()),
            target_name: Some("orders".into()),
        },
        DatabaseObject {
            kind: "trigger".into(),
            schema: Some("public".into()),
            name: "audit".into(),
            signature: None,
            target_schema: Some("public".into()),
            target_name: Some("invoices".into()),
        },
    ];
    let (source, target) = select_schema_object_pair(
        &triggers,
        &triggers,
        &["public.audit ON public.orders".into()],
    )
    .unwrap();
    assert_eq!(source.len(), 1);
    assert_eq!(target.len(), 1);
    assert_eq!(source[0].target_name.as_deref(), Some("orders"));
    assert!(select_schema_object_pair(&triggers, &triggers, &["audit".into()]).is_err());
}
