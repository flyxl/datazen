use serde_json::Value;

use super::matrix::{dialect_operators_by_cell_type, operators_by_cell_type};
use super::types::{
    meta, FilterDriverMeta, FilterOperatorGroup as Group, FilterOperatorMeta, FilterValue,
    FilterValueInputKind as Input,
};
use crate::{resolve_cell_type, CellTypeHints, FilterOperator, SHARED_FILTER_OPERATORS};

pub const ALL_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Eq,
    FilterOperator::Ne,
    FilterOperator::Gt,
    FilterOperator::Lt,
    FilterOperator::Gte,
    FilterOperator::Lte,
    FilterOperator::Between,
    FilterOperator::NotBetween,
    FilterOperator::Like,
    FilterOperator::Contains,
    FilterOperator::NotContains,
    FilterOperator::StartsWith,
    FilterOperator::EndsWith,
    FilterOperator::In,
    FilterOperator::NotIn,
    FilterOperator::IsNull,
    FilterOperator::IsNotNull,
    FilterOperator::IsEmpty,
    FilterOperator::IsNotEmpty,
    FilterOperator::Regex,
    FilterOperator::NotRegex,
    FilterOperator::JsonContains,
];

pub fn operator_meta(id: FilterOperator) -> FilterOperatorMeta {
    match id {
        FilterOperator::Eq => meta(id, Input::Single, Group::Compare, false),
        FilterOperator::Ne => meta(id, Input::Single, Group::Compare, false),
        FilterOperator::Gt => meta(id, Input::Single, Group::Compare, false),
        FilterOperator::Lt => meta(id, Input::Single, Group::Compare, false),
        FilterOperator::Gte => meta(id, Input::Single, Group::Compare, false),
        FilterOperator::Lte => meta(id, Input::Single, Group::Compare, false),
        FilterOperator::Between => meta(id, Input::Pair, Group::Compare, false),
        FilterOperator::NotBetween => meta(id, Input::Pair, Group::Compare, false),
        FilterOperator::Like => meta(id, Input::Single, Group::Pattern, false),
        FilterOperator::Contains => meta(id, Input::Single, Group::Pattern, true),
        FilterOperator::NotContains => meta(id, Input::Single, Group::Pattern, true),
        FilterOperator::StartsWith => meta(id, Input::Single, Group::Pattern, true),
        FilterOperator::EndsWith => meta(id, Input::Single, Group::Pattern, true),
        FilterOperator::In => meta(id, Input::Tokens, Group::Set, false),
        FilterOperator::NotIn => meta(id, Input::Tokens, Group::Set, false),
        FilterOperator::IsNull => meta(id, Input::None, Group::Null, false),
        FilterOperator::IsNotNull => meta(id, Input::None, Group::Null, false),
        FilterOperator::IsEmpty => meta(id, Input::None, Group::Empty, false),
        FilterOperator::IsNotEmpty => meta(id, Input::None, Group::Empty, false),
        FilterOperator::Regex => meta(id, Input::Single, Group::Dialect, false),
        FilterOperator::NotRegex => meta(id, Input::Single, Group::Dialect, false),
        FilterOperator::JsonContains => meta(id, Input::Single, Group::Dialect, false),
    }
}

pub fn filter_operator_metas() -> Vec<FilterOperatorMeta> {
    ALL_OPERATORS.iter().copied().map(operator_meta).collect()
}

pub fn operators_for_driver(declared: Option<&[FilterOperator]>) -> &[FilterOperator] {
    match declared {
        Some(list) if !list.is_empty() => list,
        _ => SHARED_FILTER_OPERATORS,
    }
}

pub fn supported_operators_for_meta(meta: Option<&FilterDriverMeta>) -> &[FilterOperator] {
    operators_for_driver(meta.and_then(|entry| entry.filter_operators.as_deref()))
}

pub fn like_case_insensitive_for_meta(meta: Option<&FilterDriverMeta>) -> bool {
    meta.is_some_and(|entry| entry.like_case_insensitive)
}

pub fn operators_for_column(
    data_type: Option<&str>,
    supported: &[FilterOperator],
    hints: Option<&CellTypeHints>,
) -> Vec<FilterOperator> {
    let cell_type = resolve_cell_type(data_type, hints);
    operators_by_cell_type(cell_type)
        .iter()
        .chain(dialect_operators_by_cell_type(cell_type).iter())
        .copied()
        .filter(|operator| supported.contains(operator))
        .collect()
}

pub fn value_input_for(operator: FilterOperator) -> Input {
    operator_meta(operator).value_input
}

pub fn value_input_for_literal(operator: &str) -> Input {
    for candidate in ALL_OPERATORS {
        if candidate.as_str() == operator {
            return value_input_for(*candidate);
        }
    }
    Input::Single
}

pub fn normalize_operator_for_column(
    current: FilterOperator,
    data_type: Option<&str>,
    supported: &[FilterOperator],
    hints: Option<&CellTypeHints>,
) -> FilterOperator {
    let allowed = operators_for_column(data_type, supported, hints);
    if allowed.contains(&current) {
        return current;
    }
    if allowed.contains(&FilterOperator::Eq) {
        return FilterOperator::Eq;
    }
    allowed.first().copied().unwrap_or(current)
}

pub fn default_operator_for_column(
    data_type: Option<&str>,
    hints: Option<&CellTypeHints>,
) -> FilterOperator {
    let cell_type = resolve_cell_type(data_type, hints);
    let operators = operators_by_cell_type(cell_type);
    if operators.contains(&FilterOperator::Eq) {
        FilterOperator::Eq
    } else {
        operators.first().copied().unwrap_or(FilterOperator::Eq)
    }
}

fn as_text_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => String::new(),
    }
}

fn as_text(value: &FilterValue) -> String {
    value.as_ref().map(as_text_value).unwrap_or_default()
}

fn as_tokens(value: &FilterValue) -> Vec<String> {
    if let Some(Value::Array(items)) = value {
        return items.iter().map(as_text_value).collect();
    }
    let text = as_text(value);
    if text.is_empty() {
        return Vec::new();
    }
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn as_pair(value: &FilterValue) -> (String, String) {
    if let Some(Value::Array(items)) = value {
        return (
            items.first().map(as_text_value).unwrap_or_default(),
            items.get(1).map(as_text_value).unwrap_or_default(),
        );
    }
    let text = as_text(value);
    let Some(separator) = text.find(',') else {
        return (text, String::new());
    };
    (
        text[..separator].trim().to_owned(),
        text[separator + 1..].trim().to_owned(),
    )
}

pub fn convert_filter_value(from: Input, to: Input, value: &FilterValue) -> FilterValue {
    if from == to || to == Input::None || from == Input::None {
        return value.clone();
    }
    if to == Input::Tokens {
        let tokens = if from == Input::Pair {
            let (lower, upper) = as_pair(value);
            [lower, upper]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect()
        } else {
            as_tokens(value)
        };
        return Some(Value::Array(
            tokens.into_iter().map(Value::String).collect(),
        ));
    }
    if to == Input::Pair {
        if from == Input::Tokens {
            let tokens = as_tokens(value);
            let lower = tokens.first().cloned().unwrap_or_default();
            let upper = tokens.get(1).cloned().unwrap_or_default();
            return Some(Value::Array(vec![
                Value::String(lower),
                Value::String(upper),
            ]));
        }
        let (lower, _) = as_pair(value);
        return Some(Value::Array(vec![
            Value::String(lower),
            Value::String(String::new()),
        ]));
    }
    if from == Input::Tokens {
        return Some(Value::String(as_tokens(value).join(", ")));
    }
    if from == Input::Pair {
        let (lower, _) = as_pair(value);
        return Some(Value::String(lower));
    }
    value.clone()
}
