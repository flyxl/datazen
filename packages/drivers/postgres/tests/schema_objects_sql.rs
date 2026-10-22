//! PostgreSQL dialect SQL for schema object browser queries (list / DDL / privileges).

use datazen_driver_api::schema_dependencies::view_dependencies_sql;
use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::ObjectKind;
use datazen_driver_postgres::PostgresSchemaObjectSql;

/// The dialect SQL now lives in the driver crate, so the test reads it from
/// the driver's own provider instead of the legacy shared tables.
fn list_objects_sql(kind: ObjectKind) -> Option<String> {
    PostgresSchemaObjectSql.list_objects_sql(kind)
}

fn object_ddl_sql(kind: ObjectKind, name: &str, schema: Option<&str>) -> Option<String> {
    PostgresSchemaObjectSql.object_ddl_sql(&ObjectDdlRequest {
        kind,
        name,
        schema,
        signature: None,
        target_schema: None,
        target_name: None,
    })
}

fn object_ddl_sql_with_metadata(
    kind: ObjectKind,
    name: &str,
    schema: Option<&str>,
    signature: Option<&str>,
    target_schema: Option<&str>,
    target_name: Option<&str>,
) -> Option<String> {
    PostgresSchemaObjectSql.object_ddl_sql(&ObjectDdlRequest {
        kind,
        name,
        schema,
        signature,
        target_schema,
        target_name,
    })
}

fn list_privileges_sql() -> Option<String> {
    PostgresSchemaObjectSql.list_privileges_sql()
}

#[test]
fn function_list_sql_excludes_catalog() {
    let sql = list_objects_sql(ObjectKind::Function).unwrap();
    assert!(sql.contains("pg_proc"));
    assert!(sql.contains("pg_catalog"));
}

#[test]
fn view_queries_return_schema_and_query_body_metadata() {
    let list = list_objects_sql(ObjectKind::View).unwrap();
    assert!(list.contains("pg_views"));
    assert!(list.contains("viewname AS name"));
    let ddl = object_ddl_sql(ObjectKind::View, "active_users", Some("public")).unwrap();
    assert!(ddl.contains("pg_get_viewdef"));
    assert!(ddl.contains("'public'"));
}

#[test]
fn view_dependency_catalog_uses_pg_depend_and_resolves_exact_routine_overloads() {
    let sql = view_dependencies_sql("postgresql", "active_users", Some("public")).unwrap();
    assert!(sql.contains("pg_catalog.pg_depend"));
    assert!(sql.contains("information_schema.view_table_usage"));
    assert!(sql.contains("information_schema.view_routine_usage"));
    assert!(sql.contains("pg_catalog.nameconcatoid(procedure.proname, procedure.oid)"));
    assert!(sql.contains("pg_catalog.pg_get_function_identity_arguments(procedure.oid)"));
}

#[test]
fn trigger_ddl_uses_pg_get_triggerdef() {
    let sql = object_ddl_sql(ObjectKind::Trigger, "trg", Some("public")).unwrap();
    assert!(sql.contains("pg_get_triggerdef"));
}

#[test]
fn routine_catalog_and_ddl_use_overload_signature() {
    let list = list_objects_sql(ObjectKind::Procedure).unwrap();
    assert!(list.contains("pg_get_function_identity_arguments"));
    assert!(list.contains("p.prokind = 'p'"));
    let ddl = object_ddl_sql_with_metadata(
        ObjectKind::Procedure,
        "rebuild",
        Some("ops"),
        Some("uuid, text"),
        None,
        None,
    )
    .unwrap();
    assert!(ddl.contains("'uuid, text'"));
    assert!(ddl.contains("p.prokind = 'p'"));
}

#[test]
fn trigger_ddl_can_filter_attached_relation() {
    let sql = object_ddl_sql_with_metadata(
        ObjectKind::Trigger,
        "audit_trigger",
        Some("app"),
        None,
        Some("app"),
        Some("orders"),
    )
    .unwrap();
    assert!(sql.contains("c.relname = 'orders'"));
}

#[test]
fn function_ddl_defaults_public_schema() {
    let sql = object_ddl_sql(ObjectKind::Function, "fn", None).unwrap();
    assert!(sql.contains("'public'"));
    assert_eq!(ObjectKind::Function.as_str(), "function");
    assert_eq!(ObjectKind::Procedure.as_str(), "procedure");
    assert_eq!(ObjectKind::Trigger.as_str(), "trigger");
}

#[test]
fn function_ddl_escapes_quotes_in_name() {
    let sql = object_ddl_sql(ObjectKind::Function, "f\"n", Some("s")).unwrap();
    assert!(sql.contains("pg_get_functiondef"));
    assert!(sql.contains("'f\"n'") || sql.contains("f\"n"));
}

#[test]
fn test_tester_routine_identity_filters_prokind_and_escapes_query_literals() {
    let function_list = list_objects_sql(ObjectKind::Function).unwrap();
    assert!(function_list.contains("p.prokind = 'f'"));
    assert!(function_list.contains("pg_get_function_identity_arguments(p.oid) AS signature"));
    assert!(!function_list.contains("p.prokind = 'p'"));

    let procedure_list = list_objects_sql(ObjectKind::Procedure).unwrap();
    assert!(procedure_list.contains("p.prokind = 'p'"));
    assert!(!procedure_list.contains("p.prokind = 'f'"));

    let function_ddl = object_ddl_sql_with_metadata(
        ObjectKind::Function,
        "lookup'name",
        Some("ops'schema"),
        Some("text, uuid'suffix"),
        None,
        None,
    )
    .unwrap();
    assert!(function_ddl.contains("p.proname = 'lookup''name'"));
    assert!(function_ddl.contains("n.nspname = 'ops''schema'"));
    assert!(function_ddl.contains("p.prokind = 'f'"));
    assert!(
        function_ddl.contains("pg_get_function_identity_arguments(p.oid) = 'text, uuid''suffix'")
    );

    let procedure_ddl = object_ddl_sql_with_metadata(
        ObjectKind::Procedure,
        "rebuild",
        Some("ops"),
        Some(""),
        None,
        None,
    )
    .unwrap();
    assert!(procedure_ddl.contains("p.prokind = 'p'"));
    assert!(procedure_ddl.contains("pg_get_function_identity_arguments(p.oid) = ''"));
}

#[test]
fn test_tester_trigger_identity_escapes_relation_literals() {
    let sql = object_ddl_sql_with_metadata(
        ObjectKind::Trigger,
        "audit'trigger",
        Some("app'schema"),
        None,
        Some("target'schema"),
        Some("orders'table"),
    )
    .unwrap();
    assert!(sql.contains("t.tgname = 'audit''trigger'"));
    assert!(sql.contains("n.nspname = 'app''schema'"));
    assert!(sql.contains("n.nspname = 'target''schema'"));
    assert!(sql.contains("c.relname = 'orders''table'"));
}

#[test]
fn privilege_sql_includes_roles_and_table_grants() {
    let pg = list_privileges_sql().unwrap();
    assert!(pg.contains("role_table_grants"));
    assert!(
        pg.contains("pg_roles"),
        "PG should include role-level privileges"
    );
}

#[test]
fn routine_catalog_and_ddl_use_overload_identity() {
    let list = list_objects_sql(ObjectKind::Function).unwrap();
    assert!(list.contains("pg_get_function_identity_arguments"));
    assert!(list.contains("p.prokind = 'f'"));

    let ddl = object_ddl_sql_with_metadata(
        ObjectKind::Function,
        "lookup",
        Some("app"),
        Some("integer, text"),
        None,
        None,
    )
    .unwrap();
    assert!(ddl.contains("pg_get_function_identity_arguments"));
    assert!(ddl.contains("'integer, text'"));
    assert!(ddl.contains("p.prokind = 'f'"));
}

#[test]
fn type_catalog_excludes_table_row_types_and_preserves_type_options() {
    let list = list_objects_sql(ObjectKind::Type).unwrap();
    assert!(list.contains("LEFT JOIN pg_class type_rel"));
    assert!(list.contains("type_rel.relkind = 'c'"));
    assert!(list.contains("t.typtype IN ('e','d','r')"));

    let ddl = object_ddl_sql_with_metadata(
        ObjectKind::Type,
        "delivery_window",
        Some("app"),
        None,
        None,
        None,
    )
    .unwrap();
    assert!(ddl.contains("pg_enum"));
    assert!(ddl.contains("typcollation"));
    assert!(ddl.contains("attcollation"));
    assert!(ddl.contains("rngsubopc"));
    assert!(ddl.contains("rngcanonical"));
    assert!(ddl.contains("rngsubdiff"));
    assert!(ddl.contains("rngmultitypid"));
    assert!(ddl.contains("con.contypid = t.oid"));
    assert!(ddl.contains("n.nspname = 'app'"));
    assert!(ddl.contains("t.typname = 'delivery_window'"));
}

#[test]
fn ddl_identity_literals_escape_sql_metacharacters() {
    let sql = object_ddl_sql_with_metadata(
        ObjectKind::Function,
        "lookup' OR 1=1 --",
        Some("app'\"schema"),
        Some("integer' OR 1=1 --"),
        None,
        None,
    )
    .unwrap();
    assert!(sql.contains("p.proname = 'lookup'' OR 1=1 --'"));
    assert!(sql.contains("n.nspname = 'app''\"schema'"));
    assert!(sql.contains("= 'integer'' OR 1=1 --'"));
}

#[test]
fn trigger_ddl_includes_attached_relation() {
    let ddl = object_ddl_sql_with_metadata(
        ObjectKind::Trigger,
        "audit_trigger",
        Some("app"),
        None,
        Some("app"),
        Some("orders"),
    )
    .unwrap();
    assert!(ddl.contains("c.relname = 'orders'"));
}
