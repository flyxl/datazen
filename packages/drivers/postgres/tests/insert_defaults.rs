//! Round-2 regression: the MySQL fix must not leak into dialects that really do
//! accept `DEFAULT VALUES`.
//!
//! The all-default spelling is dialect-specific: PostgreSQL/SQLite/SQL Server
//! accept `INSERT INTO t DEFAULT VALUES`, MySQL does not and overrides the empty
//! column list (see `datazen-driver-mysql/tests/insert_defaults.rs`). This test
//! pins the *other* side of that split for PostgreSQL.

use datazen_driver_api::DatabaseDriver;
use datazen_driver_postgres::PostgresDriver;

#[test]
fn an_all_default_insert_keeps_the_shared_default_values_spelling() {
    let driver = PostgresDriver::new();

    let statement = driver
        .build_insert_statement("users", &[])
        .expect("an all-default insert must build");

    assert_eq!(statement.sql, "INSERT INTO \"users\" DEFAULT VALUES");
    assert!(statement.parameters.is_empty());
}

#[test]
fn a_written_column_still_uses_the_shared_parameterized_rendering() {
    use datazen_driver_api::Value;

    let driver = PostgresDriver::new();
    let statement = driver
        .build_insert_statement(
            "users",
            &[("name", Some(Value::String("Ada".into()))), ("note", None)],
        )
        .expect("a bound insert must build");

    assert_eq!(
        statement.sql,
        "INSERT INTO \"users\" (\"name\", \"note\") VALUES ($1, $2)"
    );
    assert_eq!(statement.parameters.len(), 2);
}
