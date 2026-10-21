//! Independent re-verification: the Data Transfer source filter must refuse
//! an operator it cannot model (Wave 5 / track B).
//!
//! The shared `FilterOperator` enum grew twelve variants. Two crates built
//! their `WHERE` fragment with an **exhaustive** `match` over the operator, so
//! they had to grow a catch-all arm. The catch-all is the dangerous kind of
//! arm: it could have been written as "treat it like an equality", which
//! would return the wrong rows without complaining. This file pins the
//! behaviour that the arm **rejects** the condition, loudly, by name.

use datazen_data_transfer::{FilterLogic, SourceFilter};
use datazen_driver_api::filters::{FilterCondition, FilterOperator};
use datazen_driver_api::Value;

fn condition(operator: FilterOperator) -> FilterCondition {
    FilterCondition {
        column: "name".into(),
        operator,
        value: Value::String("ada".into()),
        connector: None,
    }
}

#[test]
fn a_shared_extension_operator_is_refused_by_name() {
    let filter = SourceFilter::new(vec![condition(FilterOperator::Contains)], FilterLogic::And)
        .expect("the filter itself is well formed");
    let error = filter
        .preview_where('"')
        .expect_err("the operator cannot be modelled, so the filter must be refused");
    let message = error.to_string();
    assert!(
        message.contains("does not support the 'contains' operator"),
        "the refusal must name the operator: {message}"
    );
    assert!(
        !message.contains("name = "),
        "the refusal must not fall back to an equality comparison: {message}"
    );
}

#[test]
fn a_dialect_operator_is_refused_by_name() {
    let filter = SourceFilter::new(vec![condition(FilterOperator::Regex)], FilterLogic::And)
        .expect("the filter itself is well formed");
    let error = filter
        .preview_where('"')
        .expect_err("the operator cannot be modelled, so the filter must be refused");
    let message = error.to_string();
    assert!(
        message.contains("does not support the 'regex' operator"),
        "the refusal must name the operator: {message}"
    );
}

#[test]
fn the_operators_the_builder_does_model_still_work() {
    for operator in [
        FilterOperator::Eq,
        FilterOperator::Ne,
        FilterOperator::Gt,
        FilterOperator::Lt,
        FilterOperator::Gte,
        FilterOperator::Lte,
        FilterOperator::Like,
    ] {
        let filter =
            SourceFilter::new(vec![condition(operator)], FilterLogic::And).expect("well formed");
        let sql = filter
            .preview_where('"')
            .expect("the operator is modelled and must render");
        assert!(
            sql.is_some(),
            "operator {operator:?} must render a predicate"
        );
    }
}
