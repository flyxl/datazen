//! SQL Server's half of the keyset-paging contract.
//!
//! T-SQL has **no row-value comparison**, so the condition is always the
//! lexicographic disjunction. That is not a stylistic choice: `(a, b) > (1,
//! 2)` is a syntax error in this dialect, and a condition that cannot be
//! parsed is a page the user cannot read. The shape is asserted here, not
//! just the values.
//!
//! The boundary literals also go through this driver's own `SqlServer`
//! grammar instead of the ANSI fallback: T-SQL spells bytes `0x…` and has no
//! `TRUE`, so an ANSI literal would fail to parse or, for `X'..'`, quietly
//! mean something else.

use datazen_driver_api::*;

fn driver() -> std::sync::Arc<dyn DatabaseDriver> {
    crate::SqlServerFactory.create()
}

#[test]
fn the_driver_claims_keyset_pagination() {
    assert!(driver().supports_keyset_pagination());
}

#[test]
fn the_condition_never_uses_a_row_value_comparison() {
    let sql = driver()
        .keyset_predicate_sql(
            &[
                ("id".into(), false),
                ("name".into(), false),
                ("age".into(), false),
            ],
            &[
                ("id".into(), Value::Integer(7)),
                ("name".into(), Value::String("bob".into())),
                ("age".into(), Value::Integer(30)),
            ],
        )
        .expect("a three-column order is seekable");

    assert_eq!(
        sql,
        r#"(("id" > 7) OR ("id" = 7 AND "name" > 'bob') OR ("id" = 7 AND "name" = 'bob' AND "age" > 30))"#,
        "the shape changed: {sql}"
    );

    for shape in [") > (", ") < (", ") >= (", ") <= ("] {
        assert!(
            !sql.contains(shape),
            "T-SQL has no row-value comparison, found {shape:?} in: {sql}"
        );
    }
    // The strongest form of the same statement: a row value is the only
    // reason a condition would hold a comma at all.
    assert!(
        !sql.contains(','),
        "a row value is the only comma-bearing shape here: {sql}"
    );
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
fn a_descending_order_reads_downwards() {
    let sql = driver()
        .keyset_predicate_sql(&[("id".into(), true)], &[("id".into(), Value::Integer(7))])
        .expect("a descending order is seekable");

    assert_eq!(sql, "(\"id\" < 7)");
}

#[test]
fn the_boundary_literals_use_the_tsql_grammar() {
    // The ANSI fallback would render `X'00ff'`, which T-SQL does not parse.
    let sql = driver()
        .keyset_predicate_sql(
            &[("blob".into(), false)],
            &[("blob".into(), Value::Bytes(vec![0x00, 0xff]))],
        )
        .expect("a binary boundary is seekable");

    assert_eq!(sql, "(\"blob\" > 0x00ff)");
    assert!(
        !sql.contains("X'"),
        "the ANSI byte literal is not T-SQL: {sql}"
    );
}

#[test]
fn a_boolean_boundary_degrades_instead_of_breaking_the_statement() {
    // T-SQL has no `TRUE` literal and no boolean comparison, so there is no
    // condition to build; the host then reads the page by offset.
    assert_eq!(
        driver().keyset_predicate_sql(
            &[("active".into(), false)],
            &[("active".into(), Value::Bool(true))],
        ),
        None
    );
}

#[test]
fn a_string_boundary_is_an_inline_literal_not_an_export_cast() {
    // The artifact grammar would say `CONVERT(nvarchar(max), 0x62006f00…)`,
    // which is not a comparable type for an index: every seek would scan.
    let sql = driver()
        .keyset_predicate_sql(
            &[("name".into(), false)],
            &[("name".into(), Value::String("bob".into()))],
        )
        .expect("a string boundary is seekable");

    assert_eq!(sql, r#"("name" > 'bob')"#);
    assert!(
        !sql.to_ascii_lowercase().contains("nvarchar(max)"),
        "an inline predicate may not cast to an unindexable type: {sql}"
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
