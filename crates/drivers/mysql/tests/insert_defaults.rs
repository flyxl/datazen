//! MySQL-specific spelling of an all-default INSERT.
//!
//! `INSERT INTO t DEFAULT VALUES` is PostgreSQL/SQLite/SQL Server syntax; MySQL
//! rejects it (`ER_PARSE_ERROR`). The shared trait default cannot know that, so
//! the driver overrides the empty-column case. These tests pin the rendered
//! statement without opening a connection; the non-empty case is covered by the
//! unit test inside `src/tests.rs`.

use datazen_driver_api::*;
use datazen_driver_mysql::MysqlDriver;

#[test]
fn empty_column_list_renders_the_mysql_empty_row_form() {
    let driver = MysqlDriver::new(false);

    let statement = driver
        .build_insert_statement("users", &[])
        .expect("all-default insert must build");

    assert_eq!(statement.sql, "INSERT INTO `users` () VALUES ()");
    assert!(
        statement.parameters.is_empty(),
        "an all-default insert binds nothing"
    );
}

#[test]
fn empty_column_list_keeps_backtick_escaping() {
    let driver = MysqlDriver::new(true);

    let statement = driver
        .build_insert_statement("we`ird", &[])
        .expect("all-default insert must build");

    assert_eq!(statement.sql, "INSERT INTO `we``ird` () VALUES ()");
}
