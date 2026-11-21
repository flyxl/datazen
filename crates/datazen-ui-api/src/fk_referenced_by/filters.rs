use std::collections::BTreeMap;

use crate::{FilterCondition, FilterOperator, Value};

use super::types::InboundForeignKeyRef;

pub fn referenced_values_of_row(
    reference: &InboundForeignKeyRef,
    row: &BTreeMap<String, Value>,
) -> Option<Vec<Value>> {
    let mut values = Vec::with_capacity(reference.referenced_columns.len());
    for name in &reference.referenced_columns {
        let value = row.get(name)?;
        if matches!(value, Value::Null) {
            return None;
        }
        values.push(value.clone());
    }
    Some(values)
}

pub fn build_inbound_filters(
    reference: &InboundForeignKeyRef,
    values: &[Value],
) -> Vec<FilterCondition> {
    let mut filters = Vec::with_capacity(reference.columns.len());
    for (index, column) in reference.columns.iter().enumerate() {
        let Some(value) = values.get(index) else {
            return Vec::new();
        };
        if matches!(value, Value::Null) {
            return Vec::new();
        }
        filters.push(FilterCondition {
            column: column.clone(),
            operator: FilterOperator::Eq,
            value: Some(value.clone()),
            connector: None,
        });
    }
    filters
}
