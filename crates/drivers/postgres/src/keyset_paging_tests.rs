//! PostgreSQL's half of the keyset-paging contract.
//!
//! Two things are asserted here, and they are the two the host relies on:
//! the driver **claims** the capability, and the condition it renders really
//! reads the rows after the boundary — in one direction a row-value
//! comparison, in a mixed direction the lexicographic expansion.

use super::PostgresDriver;
use datazen_driver_api::*;

fn driver() -> PostgresDriver {
    PostgresDriver::new()
}

#[test]
fn the_driver_claims_keyset_pagination() {
    assert!(driver().supports_keyset_pagination());
}

#[test]
fn a_one_direction_order_is_one_row_value_comparison() {
    let sql = driver()
        .keyset_predicate_sql(
            &[("id".into(), false), ("name".into(), false)],
            &[
                ("id".into(), Value::Integer(7)),
                ("name".into(), Value::String("bob".into())),
            ],
        )
        .expect("a two-column order is seekable");

    assert_eq!(sql, r#"("id", "name") > (7, 'bob')"#);
}

#[test]
fn a_descending_order_compares_the_other_way() {
    let sql = driver()
        .keyset_predicate_sql(
            &[("id".into(), true), ("name".into(), true)],
            &[
                ("id".into(), Value::Integer(7)),
                ("name".into(), Value::String("bob".into())),
            ],
        )
        .expect("a descending order is seekable");

    assert_eq!(sql, r#"("id", "name") < (7, 'bob')"#);
}

#[test]
fn a_mixed_order_is_spelled_out_lexicographically() {
    // `id ASC, name DESC`: a row value cannot express this, because
    // `(id, name) > (7, 'bob')` asks the wrong question about `name`.
    let sql = driver()
        .keyset_predicate_sql(
            &[("id".into(), false), ("name".into(), true)],
            &[
                ("id".into(), Value::Integer(7)),
                ("name".into(), Value::String("bob".into())),
            ],
        )
        .expect("a mixed order is still seekable");

    assert_eq!(sql, r#"(("id" > 7) OR ("id" = 7 AND "name" < 'bob'))"#);
    assert!(
        !sql.contains(r#"("id", "name")"#),
        "no row value where the directions differ: {sql}"
    );
}

#[test]
fn a_single_column_is_a_plain_comparison() {
    let sql = driver()
        .keyset_predicate_sql(&[("id".into(), false)], &[("id".into(), Value::Integer(7))])
        .expect("a single key column is seekable");

    assert_eq!(sql, r#"("id" > 7)"#);
}

#[test]
fn a_null_boundary_has_no_condition() {
    // NULL takes no part in comparisons: a condition built from it would
    // skip rows without saying so.
    assert_eq!(
        driver().keyset_predicate_sql(&[("id".into(), false)], &[("id".into(), Value::Null)]),
        None
    );
}

#[test]
fn a_boundary_of_the_wrong_length_has_no_condition() {
    assert_eq!(
        driver().keyset_predicate_sql(
            &[("id".into(), false), ("name".into(), false)],
            &[("id".into(), Value::Integer(7))],
        ),
        None
    );
}

#[test]
fn an_empty_order_has_no_condition() {
    assert_eq!(driver().keyset_predicate_sql(&[], &[]), None);
}

#[test]
fn the_literal_of_the_boundary_is_the_drivers_own() {
    // A string with a quote in it must be escaped by the driver, not by the
    // condition builder: this is the one place a boundary value enters SQL.
    let sql = driver()
        .keyset_predicate_sql(
            &[("name".into(), false)],
            &[("name".into(), Value::String("O'Brien".into()))],
        )
        .expect("the order is seekable");

    assert_eq!(sql, r#"("name" > 'O''Brien')"#);
}
