use serde_json::Value;

use crate::{FilterCondition, FilterLogic, FilterOperator, TableState};

pub fn is_complete_filter(filter: &FilterCondition) -> bool {
    if filter.column.is_empty() {
        return false;
    }
    match filter.operator {
        FilterOperator::IsNull
        | FilterOperator::IsNotNull
        | FilterOperator::IsEmpty
        | FilterOperator::IsNotEmpty => return true,
        _ => {}
    }
    let Some(value) = filter.value.as_ref() else {
        return false;
    };
    match filter.operator {
        FilterOperator::Between | FilterOperator::NotBetween => match value {
            Value::Array(bounds) => {
                bounds.len() == 2 && bounds.iter().all(|bound| filled(bound))
            }
            _ => false,
        },
        FilterOperator::In | FilterOperator::NotIn => match value {
            Value::Array(tokens) => !tokens.is_empty() && tokens.iter().all(filled),
            _ => false,
        },
        _ => !matches!(value, Value::String(text) if text.is_empty()),
    }
}

fn filled(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

pub fn filter_draft_equals_applied(state: &TableState) -> bool {
    state.draft_filters == state.filters && state.draft_filter_logic == state.filter_logic
}

pub fn apply_filters_decision(state: &TableState) -> bool {
    if filter_draft_equals_applied(state) {
        return false;
    }
    !state.draft_filters.iter().any(|filter| !is_complete_filter(filter))
}
