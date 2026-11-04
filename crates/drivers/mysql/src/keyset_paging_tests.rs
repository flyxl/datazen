//! MySQL's half of the keyset-paging contract.
//!
//! MySQL compares row values, so a one-direction order becomes a single
//! comparison; a mixed order — and a boundary that holds a `NULL` — has no
//! row-value form and is spelled out lexicographically.

use datazen_driver_api::*;

fn driver() -> std::sync::Arc<dyn DatabaseDriver> {
    crate::MysqlFactory.create()
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

    // MySQL quotes with backticks, and the boundary literals are the
    // driver's own — this assertion is what keeps the two in step.
    assert_eq!(sql, "(`id`, `name`) > (7, 'bob')");
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

    assert_eq!(sql, "(`id`, `name`) < (7, 'bob')");
}

#[test]
fn a_mixed_order_is_spelled_out_lexicographically() {
    let sql = driver()
        .keyset_predicate_sql(
            &[("id".into(), false), ("name".into(), true)],
            &[
                ("id".into(), Value::Integer(7)),
                ("name".into(), Value::String("bob".into())),
            ],
        )
        .expect("a mixed order is still seekable");

    assert_eq!(sql, "((`id` > 7) OR (`id` = 7 AND `name` < 'bob'))");
    assert!(
        !sql.contains("(`id`, `name`)"),
        "no row value where the directions differ: {sql}"
    );
}

#[test]
fn a_null_boundary_has_no_condition() {
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
