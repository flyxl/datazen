use crate::{FilterCondition, FilterConnector, FilterOperator};

use super::types::{ValueSearchLocateIntent, ValueSearchMode};

pub fn filter_operator_for_mode(mode: ValueSearchMode) -> FilterOperator {
    match mode {
        ValueSearchMode::Contains => FilterOperator::Contains,
        ValueSearchMode::NotContains => FilterOperator::NotContains,
        ValueSearchMode::Prefix => FilterOperator::StartsWith,
        ValueSearchMode::Suffix => FilterOperator::EndsWith,
        ValueSearchMode::Exact => FilterOperator::Eq,
    }
}

pub fn locate_conditions(intent: &ValueSearchLocateIntent) -> Vec<FilterCondition> {
    let operator = filter_operator_for_mode(intent.mode);
    intent
        .columns
        .iter()
        .map(|column| FilterCondition {
            column: column.clone(),
            operator,
            value: Some(crate::Value::String(intent.value.clone())),
            connector: None,
        })
        .collect()
}

pub fn locate_filter_logic(mode: ValueSearchMode) -> FilterConnector {
    if mode == ValueSearchMode::NotContains {
        FilterConnector::And
    } else {
        FilterConnector::Or
    }
}
