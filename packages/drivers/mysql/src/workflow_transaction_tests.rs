//! No database credentials or network required. These are driver admission
//! contract tests, not a simulation claiming to reproduce MySQL privilege caches.
use crate::MysqlDriver;
use datazen_driver_api::{namespace::NamespaceTarget, DatabaseDriver};

#[test]
fn static_sql_and_namespace_never_substitute_for_session_effective_visibility() {
    let driver = MysqlDriver::new(false);
    let target = NamespaceTarget::empty().with_database("approved");
    for sql in [
        "SELECT 1",
        "SELECT id FROM items WHERE id IS NULL",
        "INSERT INTO items(id) VALUES (1)",
        "UPDATE items SET id = 2",
        "DELETE FROM items",
        // A view-rejecting access shape is not a complete transaction proof:
        // session-effective trigger and incoming-FK visibility is still missing.
        "SELECT 1 FROM items FORCE INDEX(PRIMARY) LIMIT 0",
        // SQL claiming a safe catalog result is never interpreted as evidence.
        "SELECT TRUE AS all_triggers_and_foreign_keys_visible",
    ] {
        assert!(driver
            .workflow_transaction_plan(&[sql.into()], &target)
            .unwrap()
            .is_none());
    }
}

#[test]
fn unsafe_or_cross_namespace_sql_cannot_obtain_a_recipe_either() {
    let driver = MysqlDriver::new(false);
    let target = NamespaceTarget::empty().with_database("approved");
    for sql in [
        "SELECT * FROM otherdb.secret",
        "SELECT * FROM `otherdb`.`secret`",
        "INSERT INTO items VALUES (unsafe_function())",
        "CALL unsafe_procedure()",
        "CREATE TABLE items(id INT)",
        "LOCK TABLES items WRITE",
        "SET autocommit = 1",
        "COMMIT",
        "SELECT 1; DELETE FROM items",
    ] {
        assert!(driver
            .workflow_transaction_plan(&[sql.into()], &target)
            .unwrap()
            .is_none());
    }
}

#[test]
fn mariadb_is_not_opted_in_by_mysql_server_evidence() {
    assert!(MysqlDriver::new(true)
        .workflow_transaction_plan(
            &["INSERT INTO items(id) VALUES (1)".into()],
            &NamespaceTarget::empty().with_database("approved"),
        )
        .unwrap()
        .is_none());
}
