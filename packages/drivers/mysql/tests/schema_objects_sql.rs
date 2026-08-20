//! MySQL dialect SQL for schema object browser queries (list / DDL / privileges).

use datazen_driver_api::schema_objects::{
    list_objects_sql, list_privileges_sql, object_ddl_sql, ObjectKind,
};

#[test]
fn procedure_list_uses_show_status() {
    let sql = list_objects_sql("mysql", ObjectKind::Procedure).unwrap();
    assert!(sql.to_ascii_uppercase().contains("SHOW PROCEDURE"));
}

#[test]
fn list_and_ddl_cover_all_kinds() {
    assert!(list_objects_sql("mysql", ObjectKind::Function)
        .unwrap()
        .contains("FUNCTION"));
    assert!(list_objects_sql("mysql", ObjectKind::Trigger)
        .unwrap()
        .contains("TRIGGERS"));
    assert!(object_ddl_sql("mysql", ObjectKind::Procedure, "p", None)
        .unwrap()
        .contains("SHOW CREATE PROCEDURE"));
    assert!(object_ddl_sql("mysql", ObjectKind::Trigger, "t", None)
        .unwrap()
        .contains("SHOW CREATE TRIGGER"));
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
