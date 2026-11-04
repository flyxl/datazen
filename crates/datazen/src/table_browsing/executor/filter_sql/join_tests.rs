//! Tests for the way the per-condition connectors are joined.
//!
//! The connectors are read the way SQL reads them — `AND` binds stronger than
//! `OR` — so a mixed list has to be written out explicitly. These tests pin
//! that reading, because the other one is wrong in a way nobody notices: it
//! answers a different question and does not say so.

use super::test_support::*;
use super::*;

#[test]
fn join_filters_all_connectors_equal_is_byte_identical_to_today() {
    let parts = vec![
        part("A", Some(FilterConnector::And)),
        part("B", Some(FilterConnector::And)),
        part("C", Some(FilterConnector::And)),
    ];
    assert_eq!(
        join_filters(&parts, Some("and")),
        parts
            .iter()
            .map(|(sql, _)| sql.as_str())
            .collect::<Vec<_>>()
            .join(" AND ")
    );

    let parts = vec![part("A", None), part("B", None), part("C", None)];
    assert_eq!(
        join_filters(&parts, Some("or")),
        parts
            .iter()
            .map(|(sql, _)| sql.as_str())
            .collect::<Vec<_>>()
            .join(" OR ")
    );
}

#[test]
fn join_filters_mixed_parenthesizes_and_runs() {
    // The connectors are read the way SQL reads them: AND binds stronger
    // than OR. A mixed list is therefore cut into its maximal AND groups,
    // and every group that is not a single condition is written out — so the
    // database cannot group the list differently than the caller meant.
    assert_eq!(
        join_filters(
            &[
                part("A", None),
                part("B", Some(FilterConnector::Or)),
                part("C", Some(FilterConnector::And)),
            ],
            Some("and")
        ),
        "A OR (B AND C)"
    );
    assert_eq!(
        join_filters(
            &[
                part("A", None),
                part("B", Some(FilterConnector::Or)),
                part("C", Some(FilterConnector::And)),
                part("D", Some(FilterConnector::Or)),
            ],
            Some("and")
        ),
        "A OR (B AND C) OR D"
    );
    // A single fragment never gets parentheses.
    assert_eq!(join_filters(&[part("A", None)], Some("and")), "A");
    // An AND group that is not sandwiched between ORs keeps the plain
    // shape, because there is nothing to disambiguate.
    assert_eq!(
        join_filters(
            &[
                part("A", None),
                part("B", Some(FilterConnector::And)),
                part("C", Some(FilterConnector::And)),
            ],
            Some("or")
        ),
        "A AND B AND C"
    );
}

#[test]
fn join_filters_first_entry_connector_is_ignored() {
    let with_connector = join_filters(
        &[
            part("A", Some(FilterConnector::Or)),
            part("B", Some(FilterConnector::And)),
        ],
        Some("and"),
    );
    let without_connector = join_filters(
        &[part("A", None), part("B", Some(FilterConnector::And))],
        Some("and"),
    );
    assert_eq!(with_connector, without_connector);
    assert_eq!(with_connector, "A AND B");
}

#[test]
fn join_filters_empty_is_empty_string() {
    assert_eq!(join_filters(&[], Some("and")), "");
}

#[test]
fn join_filters_survives_a_dropped_fragment() {
    // A driver may answer with an empty fragment; the connector has to
    // travel with the fragment it belongs to, or every connector after the
    // dropped one shifts by a position.
    let parts = vec![
        part("A", None),
        part("", Some(FilterConnector::Or)),
        part("C", Some(FilterConnector::And)),
    ];
    let kept: Vec<(String, Option<FilterConnector>)> = parts
        .into_iter()
        .filter(|(sql, _)| !sql.is_empty())
        .collect();
    assert_eq!(join_filters(&kept, Some("and")), "A AND C");
}

#[test]
fn render_filter_parts_keeps_fragments_and_connectors_paired() {
    let filters = vec![
        FilterCondition {
            column: "a".into(),
            operator: FilterOperator::Eq,
            value: Value::String("1".into()),
            connector: None,
        },
        FilterCondition {
            column: "b".into(),
            operator: FilterOperator::Eq,
            value: Value::String("2".into()),
            connector: Some(FilterConnector::Or),
        },
    ];
    let parts = render_filter_parts(&filters, &PLAIN).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].1, None);
    assert_eq!(parts[1].1, Some(FilterConnector::Or));
    assert_eq!(parts[0].0, "\"a\" = '1'");
    assert_eq!(parts[1].0, "\"b\" = '2'");
}

#[test]
fn join_filters_keeps_sql_precedence_for_or_then_and() {
    // `a OR b AND c` means `a OR (b AND c)`: SQL binds AND stronger than OR.
    // The shape that must **not** appear is `(a OR b) AND c` — it asks a
    // different question and answers it without complaining.
    let parts = vec![
        part("a = 1", None),
        part("b = 2", Some(FilterConnector::Or)),
        part("c = 3", Some(FilterConnector::And)),
    ];
    let sql = join_filters(&parts, Some("and"));
    assert_eq!(sql, "a = 1 OR (b = 2 AND c = 3)");
    assert_ne!(sql, "(a = 1 OR b = 2) AND c = 3");
}

#[test]
fn join_filters_keeps_sql_precedence_for_and_then_or() {
    // The mirror image: `a AND b OR c` means `(a AND b) OR c`.
    let parts = vec![
        part("a = 1", None),
        part("b = 2", Some(FilterConnector::And)),
        part("c = 3", Some(FilterConnector::Or)),
    ];
    let sql = join_filters(&parts, Some("and"));
    assert_eq!(sql, "(a = 1 AND b = 2) OR c = 3");
    assert_ne!(sql, "a = 1 AND (b = 2 OR c = 3)");
}

#[test]
fn rendered_filter_list_carries_the_sql_precedence() {
    // The end-to-end path: a real filter list goes through the renderer and
    // comes out as the SQL the user meant.
    let filters = vec![
        linked("a", FilterOperator::Eq, Value::Integer(1), None),
        linked(
            "b",
            FilterOperator::Eq,
            Value::Integer(2),
            Some(FilterConnector::Or),
        ),
        linked(
            "c",
            FilterOperator::Eq,
            Value::Integer(3),
            Some(FilterConnector::And),
        ),
    ];
    let parts = render_filter_parts(&filters, &PLAIN).expect("the list is renderable");
    assert_eq!(
        join_filters(&parts, Some("and")),
        "\"a\" = 1 OR (\"b\" = 2 AND \"c\" = 3)"
    );
}
