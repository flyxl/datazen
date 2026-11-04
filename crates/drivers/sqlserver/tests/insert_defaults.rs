//! Round-2 regression: the MySQL empty-column override must not affect SQL Server.
//!
//! SQL Server accepts `INSERT INTO t DEFAULT VALUES`; only MySQL needs the
//! `() VALUES ()` spelling. This pins the untouched side of that split.

use datazen_driver_api::DatabaseDriver;
use datazen_driver_sqlserver::SqlServerDriver;

#[test]
fn an_all_default_insert_keeps_the_shared_default_values_spelling() {
    let driver = SqlServerDriver::new();

    let statement = driver
        .build_insert_statement("users", &[])
        .expect("an all-default insert must build");

    assert_eq!(statement.sql, "INSERT INTO \"users\" DEFAULT VALUES");
    assert!(statement.parameters.is_empty());
}
