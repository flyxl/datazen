//! SQLite's half of the keyset-paging contract.
//!
//! SQLite is read by seek through the **portable** expansion the trait
//! provides, so these cases assert the shape the host will actually embed:
//! the lexicographic disjunction, in SQLite's own quoting and literals.

use datazen_driver_api::*;

fn driver() -> std::sync::Arc<dyn DatabaseDriver> {
    crate::SqliteFactory.create()
}

#[test]
fn the_driver_claims_keyset_pagination() {
    assert!(driver().supports_keyset_pagination());
}

#[test]
fn a_one_column_order_is_a_plain_comparison() {
    let sql = driver()
        .keyset_predicate_sql(&[("id".into(), false)], &[("id".into(), Value::Integer(7))])
        .expect("a single key column is seekable");

    assert_eq!(sql, "(\"id\" > 7)");
}

#[test]
fn a_compound_order_keeps_the_leftmost_columns_equal() {
    let sql = driver()
        .keyset_predicate_sql(
            &[("id".into(), false), ("name".into(), false)],
            &[
                ("id".into(), Value::Integer(7)),
                ("name".into(), Value::String("bob".into())),
            ],
        )
        .expect("a two-column order is seekable");

    assert_eq!(sql, "((\"id\" > 7) OR (\"id\" = 7 AND \"name\" > 'bob'))");
}

#[test]
fn descending_columns_compare_the_other_way() {
    let sql = driver()
        .keyset_predicate_sql(
            &[("id".into(), true), ("name".into(), false)],
            &[
                ("id".into(), Value::Integer(7)),
                ("name".into(), Value::String("bob".into())),
            ],
        )
        .expect("a mixed order is seekable");

    // `id DESC` reads downwards, `name ASC` upwards — a mixed order is
    // exactly where a row-value comparison would ask the wrong question.
    assert_eq!(sql, "((\"id\" < 7) OR (\"id\" = 7 AND \"name\" > 'bob'))");
}

#[test]
fn a_null_boundary_has_no_condition() {
    assert_eq!(
        driver().keyset_predicate_sql(&[("id".into(), false)], &[("id".into(), Value::Null)]),
        None
    );
}

#[test]
fn the_boundary_literals_are_escaped_by_the_driver() {
    let sql = driver()
        .keyset_predicate_sql(
            &[("name".into(), false)],
            &[("name".into(), Value::String("O'Brien".into()))],
        )
        .expect("the order is seekable");

    assert_eq!(sql, "(\"name\" > 'O''Brien')");
}
