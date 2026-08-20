//! PostgreSQL dialect SQL for schema object browser queries (list / DDL / privileges).

use datazen_driver_api::schema_objects::{
    list_objects_sql, list_privileges_sql, object_ddl_sql, ObjectKind,
};

#[test]
fn function_list_sql_excludes_catalog() {
    let sql = list_objects_sql("postgresql", ObjectKind::Function).unwrap();
    assert!(sql.contains("pg_proc"));
    assert!(sql.contains("pg_catalog"));
}

#[test]
fn trigger_ddl_uses_pg_get_triggerdef() {
    let sql = object_ddl_sql("postgresql", ObjectKind::Trigger, "trg", Some("public")).unwrap();
    assert!(sql.contains("pg_get_triggerdef"));
}

#[test]
fn function_ddl_defaults_public_schema() {
    let sql = object_ddl_sql("postgres", ObjectKind::Function, "fn", None).unwrap();
    assert!(sql.contains("'public'"));
    assert_eq!(ObjectKind::Function.as_str(), "function");
    assert_eq!(ObjectKind::Procedure.as_str(), "procedure");
    assert_eq!(ObjectKind::Trigger.as_str(), "trigger");
}

#[test]
fn function_ddl_escapes_quotes_in_name() {
    let sql = object_ddl_sql("postgresql", ObjectKind::Function, "f\"n", Some("s")).unwrap();
    assert!(sql.contains("pg_get_functiondef"));
    assert!(sql.contains("'f\"n'") || sql.contains("f\"n"));
}

#[test]
fn privilege_sql_includes_roles_and_table_grants() {
    let pg = list_privileges_sql("postgres").unwrap();
    assert!(pg.contains("role_table_grants"));
    assert!(
        pg.contains("pg_roles"),
        "PG should include role-level privileges"
    );
}
