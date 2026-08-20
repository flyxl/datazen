//! SQLite dialect SQL for schema object browser queries.

use datazen_driver_api::schema_objects::{
    list_objects_sql, list_privileges_sql, object_ddl_sql, ObjectKind,
};

#[test]
fn has_triggers_only() {
    assert!(list_objects_sql("sqlite", ObjectKind::Trigger).is_some());
    assert!(list_objects_sql("sqlite", ObjectKind::Function).is_none());
    assert!(list_privileges_sql("sqlite").is_none());
}

#[test]
fn trigger_ddl_reads_sqlite_master() {
    let sqlite = object_ddl_sql("sqlite", ObjectKind::Trigger, "trg'x", None).unwrap();
    assert!(sqlite.contains("sqlite_master"));
    assert!(sqlite.contains("trg''x"));
    assert!(object_ddl_sql("sqlite", ObjectKind::Function, "f", None).is_none());
}

#[test]
fn function_list_returns_none_so_host_skips_query() {
    // Host IPC returns empty when list_objects_sql is None; assert dialect contract here.
    assert!(list_objects_sql("sqlite", ObjectKind::Function).is_none());
    assert!(list_privileges_sql("sqlite").is_none());
}
