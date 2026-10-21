//! Target dependency catalog and metadata helper tests.

use super::*;

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
    let handle = datazen_driver_api::ConnectionHandle {
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
    let handle = datazen_driver_api::ConnectionHandle {
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
    let handle = datazen_driver_api::ConnectionHandle {
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
    let table = |name: &str| crate::db::TableSchema {
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
    changed_child.foreign_keys.push(crate::db::ForeignKeyInfo {
        name: "fk_child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "public.parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: crate::db::ForeignKeyDeferrability::NotDeferrable,
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
