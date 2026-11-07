//! DuckDB dialect SQL for schema object browser queries (list / DDL).

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::ObjectKind;
use datazen_driver_duckdb::DuckDbSchemaObjectSql;

/// The dialect SQL lives in this driver crate, so the test reads it from the
/// driver's own provider.
fn list_objects_sql(kind: ObjectKind) -> Option<String> {
    DuckDbSchemaObjectSql.list_objects_sql(kind)
}

fn object_ddl_sql(kind: ObjectKind, name: &str) -> Option<String> {
    DuckDbSchemaObjectSql.object_ddl_sql(&ObjectDdlRequest {
        kind,
        name,
        schema: None,
        signature: None,
        target_schema: None,
        target_name: None,
    })
}

#[test]
fn duckdb_has_its_own_sequence_and_trigger_family() {
    assert!(list_objects_sql(ObjectKind::Trigger).is_some());
    assert!(list_objects_sql(ObjectKind::Sequence).is_some());
    // DuckDB exposes neither routines nor views in the object browser.
    assert!(list_objects_sql(ObjectKind::Function).is_none());
    assert!(list_objects_sql(ObjectKind::View).is_none());
}

#[test]
fn sequence_ddl_reads_the_duckdb_catalog() {
    let ddl = object_ddl_sql(ObjectKind::Sequence, "seq").unwrap();
    assert!(ddl.contains("duckdb_sequences()"));
    assert!(ddl.contains("'seq'"));
}

#[test]
fn trigger_ddl_reads_information_schema() {
    let ddl = object_ddl_sql(ObjectKind::Trigger, "trg").unwrap();
    assert!(ddl.contains("information_schema.triggers"));
    assert!(ddl.contains("'trg'"));
}
