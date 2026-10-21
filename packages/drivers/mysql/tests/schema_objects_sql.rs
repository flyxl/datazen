//! MySQL dialect SQL for schema object browser queries (list / DDL / privileges).

use datazen_driver_api::schema_dependencies::{
    mysql_table_dependencies_sql, view_dependencies_sql,
};
use datazen_driver_api::schema_object_commands::parse_object_list;
use datazen_driver_api::schema_objects::{
    list_objects_sql, list_privileges_sql, mysql_show_create_view_sql, object_ddl_sql, ObjectKind,
};
use datazen_driver_api::{ColumnInfo, QueryResult, Value};

#[test]
fn procedure_list_uses_information_schema() {
    let sql = list_objects_sql("mysql", ObjectKind::Procedure).unwrap();
    assert!(sql.contains("information_schema.ROUTINES"));
    assert!(sql.contains("ROUTINE_TYPE = 'PROCEDURE'"));
}

#[test]
fn list_and_ddl_cover_all_kinds() {
    assert!(list_objects_sql("mysql", ObjectKind::Function)
        .unwrap()
        .contains("FUNCTION"));
    assert!(list_objects_sql("mysql", ObjectKind::Trigger)
        .unwrap()
        .contains("information_schema.TRIGGERS"));
    assert!(object_ddl_sql("mysql", ObjectKind::Procedure, "p", None)
        .unwrap()
        .contains("SHOW CREATE PROCEDURE"));
    assert!(object_ddl_sql("mysql", ObjectKind::Trigger, "t", None)
        .unwrap()
        .contains("SHOW CREATE TRIGGER"));
}

#[test]
fn routines_and_triggers_preserve_identity_and_mysql_has_no_sequences() {
    let functions = list_objects_sql("mysql", ObjectKind::Function).unwrap();
    assert!(functions.contains("ROUTINE_TYPE = 'FUNCTION'"));
    let triggers = list_objects_sql("mysql", ObjectKind::Trigger).unwrap();
    assert!(triggers.contains("EVENT_OBJECT_TABLE AS target_name"));
    let ddl = object_ddl_sql("mysql", ObjectKind::Function, "f`n", Some("db`name")).unwrap();
    assert!(ddl.contains("SHOW CREATE FUNCTION `db``name`.`f``n`"));
    assert!(list_objects_sql("mysql", ObjectKind::Sequence).is_none());
    assert!(object_ddl_sql("mysql", ObjectKind::Sequence, "seq", None).is_none());
}

#[test]
fn view_queries_return_view_body_metadata_contract() {
    let list = list_objects_sql("mysql", ObjectKind::View).unwrap();
    assert!(list.contains("information_schema.VIEWS"));
    // MySQL's `*_SCHEMA` columns identify databases, not a separate namespace
    // schema, so the list projects a null schema and the caller keeps database
    // ownership in the independent target. The `schema` *field name* is still
    // what the shared object-list parser consumes (see the alias contract test).
    assert!(list.contains("NULL AS `schema`"));
    let ddl = object_ddl_sql("mysql", ObjectKind::View, "active_users", None).unwrap();
    assert!(ddl.contains("VIEW_DEFINITION AS ddl"));
    assert!(ddl.contains("TABLE_SCHEMA AS view_schema"));
    assert!(ddl.contains("DEFINER AS view_definer"));
    assert!(ddl.contains("SECURITY_TYPE AS view_security_type"));
    assert!(ddl.contains("CHECK_OPTION AS view_check_option"));
    assert!(ddl.contains("CHARACTER_SET_CLIENT AS view_character_set_client"));
    assert!(ddl.contains("COLLATION_CONNECTION AS view_collation_connection"));
    assert!(ddl.contains("TABLE_NAME = 'active_users'"));
    assert_eq!(
        mysql_show_create_view_sql("active`users", Some("source`db")),
        "SHOW CREATE VIEW `source``db`.`active``users`"
    );
}

#[test]
fn object_lists_quote_reserved_schema_alias_and_preserve_parser_contract() {
    for kind in [
        ObjectKind::Function,
        ObjectKind::Procedure,
        ObjectKind::Trigger,
        ObjectKind::View,
    ] {
        let sql = list_objects_sql("mysql", kind).unwrap();
        assert!(
            sql.contains("AS `schema`"),
            "{kind:?} list must retain the `schema` result name with MySQL quoting: {sql}"
        );
        assert!(
            !sql.contains("AS schema"),
            "{kind:?} list must not emit the unquoted reserved alias: {sql}"
        );
    }

    // MySQL returns the quoted identifier's alias as the same `schema` field
    // name consumed by the shared object-list parser.
    let result = QueryResult {
        columns: vec![
            ColumnInfo {
                name: "schema".into(),
                data_type: "varchar".into(),
                nullable: false,
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "varchar".into(),
                nullable: false,
            },
        ],
        rows: vec![vec![
            Some(Value::String("fixture_db".into())),
            Some(Value::String("fixture_view".into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let objects = parse_object_list(&result, ObjectKind::View.as_str()).unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].schema.as_deref(), Some("fixture_db"));
    assert_eq!(objects[0].name, "fixture_view");
}

#[test]
fn view_dependency_catalog_reads_structured_table_and_routine_usage() {
    let sql = view_dependencies_sql("mysql", "active_users", Some("app")).unwrap();
    assert!(sql.contains("information_schema.VIEW_TABLE_USAGE"));
    assert!(sql.contains("information_schema.VIEW_ROUTINE_USAGE"));
    assert!(sql.contains("information_schema.ROUTINES"));
    assert!(sql.contains("routine.SPECIFIC_NAME = view_usage.SPECIFIC_NAME"));
}

#[test]
fn table_dependency_catalog_proves_selected_fk_metadata_before_emitting_edges() {
    let sql = mysql_table_dependencies_sql("child_table", Some("source_db"));
    assert!(sql.contains("information_schema.TABLES"));
    assert!(sql.contains("TABLE_TYPE = 'BASE TABLE'"));
    assert!(sql.contains("information_schema.TABLE_CONSTRAINTS"));
    assert!(sql.contains("information_schema.KEY_COLUMN_USAGE"));
    assert!(sql.contains("information_schema.REFERENTIAL_CONSTRAINTS"));
    assert!(sql.contains("constraint_row.CONSTRAINT_TYPE = 'FOREIGN KEY'"));
    assert!(sql.contains("BINARY TABLE_SCHEMA = BINARY 'source_db'"));
    assert!(sql.contains("selected_count"));
    assert!(sql.contains("unsupported_count"));
    assert!(sql.contains("orphan_key_rows"));
    assert!(sql.contains("COUNT(DISTINCT key_column.POSITION_IN_UNIQUE_CONSTRAINT)"));
    assert!(sql.contains(
        "BINARY referenced_table.TABLE_SCHEMA = BINARY key_column.REFERENCED_TABLE_SCHEMA"
    ));
    assert!(sql.contains("SELECT DISTINCT dependency_schema, dependency_name"));
    assert!(sql.contains(
        "BINARY dependency.dependency_name = BINARY (SELECT selected_name FROM selected_tables)"
    ));
}

#[test]
fn ddl_quotes_backtick_in_ident() {
    let sql = object_ddl_sql("mysql", ObjectKind::Function, "foo`bar", None).unwrap();
    assert!(sql.contains("`foo``bar`"));
}

#[test]
fn privilege_sql_avoids_reserved_schema_alias() {
    let mysql = list_privileges_sql("mariadb").unwrap();
    assert!(mysql.contains("TABLE_PRIVILEGES"));
    assert!(
        mysql.contains("USER_PRIVILEGES"),
        "MySQL should include user-level privileges"
    );
    assert!(
        mysql.contains("AS table_schema"),
        "MySQL must not alias as bare `schema` (reserved word)"
    );
    assert!(
        !mysql.contains("AS schema,"),
        "bare AS schema breaks MySQL 1064"
    );
}
