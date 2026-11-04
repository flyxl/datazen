//! SQLite dialect SQL for schema object browser queries.

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::ObjectKind;
use datazen_driver_sqlite::SqliteSchemaObjectSql;

/// The dialect SQL now lives in the driver crate, so the test reads it from
/// the driver's own provider instead of the removed shared tables.
fn list_objects_sql(kind: ObjectKind) -> Option<String> {
    SqliteSchemaObjectSql.list_objects_sql(kind)
}

fn object_ddl_sql(kind: ObjectKind, name: &str, schema: Option<&str>) -> Option<String> {
    SqliteSchemaObjectSql.object_ddl_sql(&ObjectDdlRequest {
        kind,
        name,
        schema,
        signature: None,
        target_schema: None,
        target_name: None,
    })
}

fn list_privileges_sql() -> Option<String> {
    SqliteSchemaObjectSql.list_privileges_sql()
}

#[test]
fn has_triggers_only() {
    assert!(list_objects_sql(ObjectKind::Trigger).is_some());
    assert!(list_objects_sql(ObjectKind::Function).is_none());
    assert!(list_privileges_sql().is_none());
}

#[test]
fn trigger_ddl_reads_sqlite_master() {
    let sqlite = object_ddl_sql(ObjectKind::Trigger, "trg'x", None).unwrap();
    assert!(sqlite.contains("sqlite_master"));
    assert!(sqlite.contains("trg''x"));
    assert!(object_ddl_sql(ObjectKind::Function, "f", None).is_none());
}

#[test]
fn view_queries_return_query_body_from_sqlite_master() {
    let list = list_objects_sql(ObjectKind::View).unwrap();
    assert!(list.contains("type = 'view'"));
    let ddl = object_ddl_sql(ObjectKind::View, "active_users", None).unwrap();
    assert!(ddl.contains("substr(sql"));
    assert!(ddl.contains("name = 'active_users'"));
}

#[test]
fn function_list_returns_none_so_host_skips_query() {
    // Host IPC returns empty when list_objects_sql is None; assert dialect contract here.
    assert!(list_objects_sql(ObjectKind::Function).is_none());
    assert!(list_privileges_sql().is_none());
}

#[test]
fn trigger_catalog_exposes_target_table_and_sequences_are_unsupported() {
    let trigger_list = list_objects_sql(ObjectKind::Trigger).unwrap();
    assert!(trigger_list.contains("tbl_name AS target_name"));
    assert!(list_objects_sql(ObjectKind::Sequence).is_none());
    assert!(object_ddl_sql(ObjectKind::Sequence, "seq", None).is_none());
}

#[test]
fn unsupported_sequence_families_fail_closed() {
    assert!(list_objects_sql(ObjectKind::Sequence).is_none());
    assert!(object_ddl_sql(ObjectKind::Sequence, "seq", None).is_none());
}

#[test]
fn table_ddl_reads_sqlite_master() {
    assert!(object_ddl_sql(ObjectKind::Table, "users", None).is_some());
}
