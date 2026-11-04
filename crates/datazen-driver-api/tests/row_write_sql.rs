//! Tester's independent re-verification: the **byte identity** of the five
//! row-write statement constructors that the result-grid track moved out of
//! `traits.rs` into `traits/sql_text.rs`.
//!
//! The move is claimed to be a pure move: "the statements they render are
//! byte for byte the ones the trait rendered before". This test renders all
//! five through the **public trait API** and pins the exact bytes, so the
//! claim can be checked against the pre-move tree by running the same file
//! there (the base commit `66fa0b72e`).
//!
//! The constructors are the ones a driver inherits:
//! `build_update_sql`, `build_delete_sql`, `build_update_statement`,
//! `build_delete_statement`, `build_insert_statement`.

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{DatabaseDriver, Value};

fn driver() -> std::sync::Arc<MockDriver> {
    MockDriver::new(
        "postgres",
        MockDriverOptions {
            parameterized_writes: true,
            ..MockDriverOptions::default()
        },
    )
}

#[test]
fn the_row_write_statements_render_the_same_bytes_as_before_the_move() {
    let driver = driver();

    // ── The legacy inline-value UPDATE.
    let update = driver.build_update_sql(
        "orders",
        &[("total", Some(Value::Integer(11)))],
        &[("id", Some(Value::Integer(7)))],
    );
    assert_eq!(update, r#"UPDATE "orders" SET "total" = 11 WHERE "id" = 7"#);

    // ── The legacy inline-value DELETE.
    let delete = driver.build_delete_sql("orders", &[("id", Some(Value::Integer(7)))]);
    assert_eq!(delete, r#"DELETE FROM "orders" WHERE "id" = 7"#);

    // ── The parameterized UPDATE.
    let update_statement = driver
        .build_update_statement(
            "orders",
            &[("total", Some(Value::Integer(11)))],
            &[("id", Some(Value::Integer(7)))],
        )
        .expect("postgres supports bound writes");
    assert_eq!(
        update_statement.sql,
        r#"UPDATE "orders" SET "total" = ?1 WHERE "id" = ?2"#
    );
    assert_eq!(
        format!("{:?}", update_statement.parameters),
        "[Integer(11), Integer(7)]"
    );

    // ── The parameterized DELETE.
    let delete_statement = driver
        .build_delete_statement("orders", &[("id", Some(Value::Integer(7)))])
        .expect("postgres supports bound writes");
    assert_eq!(
        delete_statement.sql,
        r#"DELETE FROM "orders" WHERE "id" = ?1"#
    );
    assert_eq!(format!("{:?}", delete_statement.parameters), "[Integer(7)]");

    // ── The parameterized INSERT.
    let insert_statement = driver
        .build_insert_statement(
            "orders",
            &[
                ("id", Some(Value::Integer(7))),
                ("total", Some(Value::Integer(11))),
            ],
        )
        .expect("postgres supports bound writes");
    assert_eq!(
        insert_statement.sql,
        r#"INSERT INTO "orders" ("id", "total") VALUES (?1, ?2)"#
    );
    assert_eq!(
        format!("{:?}", insert_statement.parameters),
        "[Integer(7), Integer(11)]"
    );
}
