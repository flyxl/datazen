//! SQL Server dialect SQL for schema object browser queries (list / DDL /
//! privileges).

use datazen_driver_api::schema_object_sql::{ObjectDdlRequest, SchemaObjectSql};
use datazen_driver_api::schema_objects::ObjectKind;
use datazen_driver_sqlserver::SqlServerSchemaObjectSql;

/// The dialect SQL lives in this driver crate, so the test reads it from the
/// driver's own provider.
fn list_objects_sql(kind: ObjectKind) -> Option<String> {
    SqlServerSchemaObjectSql.list_objects_sql(kind)
}

fn object_ddl_sql(kind: ObjectKind, name: &str, schema: Option<&str>) -> Option<String> {
    SqlServerSchemaObjectSql.object_ddl_sql(&ObjectDdlRequest {
        kind,
        name,
        schema,
        signature: None,
        target_schema: None,
        target_name: None,
    })
}

#[test]
fn objects_cover_all_kinds() {
    for kind in [
        ObjectKind::View,
        ObjectKind::Function,
        ObjectKind::Procedure,
        ObjectKind::Trigger,
        ObjectKind::Sequence,
        ObjectKind::Type,
    ] {
        let sql = list_objects_sql(kind)
            .unwrap_or_else(|| panic!("sqlserver should list object kind {kind:?}"));
        let catalog = match kind {
            ObjectKind::View => "sys.views",
            ObjectKind::Sequence => "sys.sequences",
            ObjectKind::Type => "sys.types",
            _ => "sys.objects",
        };
        assert!(
            sql.contains(catalog) || (kind == ObjectKind::Trigger && sql.contains("sys.triggers")),
            "sqlserver list should read catalog views for {kind:?}: {sql}"
        );
        // `schema` is a reserved keyword in T-SQL: an unquoted alias is a
        // syntax error (156), so these queries must bracket-quote it.
        assert!(
            !sql.contains("AS schema"),
            "sqlserver list must not alias a column `AS schema` (reserved word) for {kind:?}: {sql}"
        );
    }
}

#[test]
fn ddl_uses_object_definition_and_bracket_quoted_names() {
    let fn_ddl = object_ddl_sql(ObjectKind::Function, "fn", Some("dbo")).unwrap();
    assert!(fn_ddl.contains("OBJECT_DEFINITION"));
    assert!(fn_ddl.contains("[dbo].[fn]"));
    let view_ddl = object_ddl_sql(ObjectKind::View, "v", Some("dbo")).unwrap();
    assert!(view_ddl.contains("OBJECT_DEFINITION"));
    assert!(view_ddl.contains("[dbo].[v]"));
    // Without schema it falls back to dbo.
    let no_schema = object_ddl_sql(ObjectKind::Procedure, "p", None).unwrap();
    assert!(no_schema.contains("[dbo].[p]"));
}

#[test]
fn sequences_and_types_are_reconstructed_from_catalog_metadata() {
    let seq_ddl = object_ddl_sql(ObjectKind::Sequence, "seq", Some("dbo")).unwrap();
    assert!(seq_ddl.contains("sys.sequences"));
    assert!(seq_ddl.contains("CREATE SEQUENCE"));
    let type_ddl = object_ddl_sql(ObjectKind::Type, "phone", Some("dbo")).unwrap();
    assert!(type_ddl.contains("sys.types"));
    assert!(type_ddl.contains("CREATE TYPE"));
}

#[test]
fn privileges_are_listed() {
    assert!(SqlServerSchemaObjectSql.list_privileges_sql().is_some());
}

#[test]
fn object_ddl_escapes_single_quotes_and_brackets() {
    let ddl_sql =
        object_ddl_sql(ObjectKind::Function, "fn'special", Some("custom schema")).unwrap();
    assert!(ddl_sql.contains("[custom schema].[fn''special]"));
    assert!(ddl_sql.contains("fn''special"));

    let seq_sql = object_ddl_sql(ObjectKind::Sequence, "seq'one", Some("dbo")).unwrap();
    assert!(seq_sql.contains("WHERE q.name = 'seq''one'"));
    assert!(seq_sql.contains("[dbo].[seq'one]"));
}
