use crate::{DIALECT_FILTER_OPERATORS, SHARED_FILTER_OPERATORS};

use super::types::{NamedView, PortFilterCondition, TableStateForView, ViewFilterReadout};

pub struct ViewFromTableStateOptions<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub is_default: bool,
    pub now: &'a str,
}

pub fn view_from_table_state(
    state: &TableStateForView,
    options: &ViewFromTableStateOptions<'_>,
) -> NamedView {
    NamedView {
        id: options.id.to_owned(),
        name: options.name.to_owned(),
        scope: state.scope.clone(),
        filters: state.filters.clone(),
        filter_logic: state.filter_logic.clone(),
        raw_where: state.raw_where.clone(),
        sorts: state.sorts.clone(),
        visible_columns: state.visible_columns.clone(),
        column_order: state.column_order.clone(),
        is_default: options.is_default,
        created_at: options.now.to_owned(),
        updated_at: None,
    }
}

pub fn is_host_filter_operator(operator: &str) -> bool {
    SHARED_FILTER_OPERATORS
        .iter()
        .chain(DIALECT_FILTER_OPERATORS.iter())
        .any(|known| known.as_str() == operator)
}

pub fn read_view_filters(conditions: &[PortFilterCondition]) -> ViewFilterReadout {
    let mut readout = ViewFilterReadout::default();
    for condition in conditions {
        if !is_host_filter_operator(&condition.operator) {
            if !readout.unknown_operators.contains(&condition.operator) {
                readout.unknown_operators.push(condition.operator.clone());
            }
            continue;
        }
        readout.filters.push(crate::FilterCondition {
            column: condition.column.clone(),
            operator: match condition.operator.as_str() {
                "eq" => crate::FilterOperator::Eq,
                "ne" => crate::FilterOperator::Ne,
                "gt" => crate::FilterOperator::Gt,
                "lt" => crate::FilterOperator::Lt,
                "gte" => crate::FilterOperator::Gte,
                "lte" => crate::FilterOperator::Lte,
                "like" => crate::FilterOperator::Like,
                "in" => crate::FilterOperator::In,
                "isNull" => crate::FilterOperator::IsNull,
                "isNotNull" => crate::FilterOperator::IsNotNull,
                "notIn" => crate::FilterOperator::NotIn,
                "between" => crate::FilterOperator::Between,
                "notBetween" => crate::FilterOperator::NotBetween,
                "contains" => crate::FilterOperator::Contains,
                "notContains" => crate::FilterOperator::NotContains,
                "startsWith" => crate::FilterOperator::StartsWith,
                "endsWith" => crate::FilterOperator::EndsWith,
                "isEmpty" => crate::FilterOperator::IsEmpty,
                "isNotEmpty" => crate::FilterOperator::IsNotEmpty,
                "regex" => crate::FilterOperator::Regex,
                "notRegex" => crate::FilterOperator::NotRegex,
                "jsonContains" => crate::FilterOperator::JsonContains,
                _ => continue,
            },
            value: condition.value.clone(),
            connector: condition.connector,
        });
    }
    readout
}
