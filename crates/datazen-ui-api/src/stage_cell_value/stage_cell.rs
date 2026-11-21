use std::collections::BTreeMap;

use super::stage_edit::compute_stage_result;
use super::types::{
    PendingRowChange, StageCellValueInput, StageCellValueResult, StageEditInput,
    StageExpressionValueInput, WriteExpressionId,
};
use crate::Value;

pub fn compute_stage_expression_value_result(
    input: &StageExpressionValueInput,
) -> StageCellValueResult {
    let mut expression_values = input
        .existing
        .as_ref()
        .and_then(|change| change.expression_values.clone())
        .unwrap_or_default();
    expression_values.insert(input.column.clone(), WriteExpressionId::CurrentTimestamp);

    let mut original_values = input
        .existing
        .as_ref()
        .map(|change| change.original_values.clone())
        .unwrap_or_default();
    let mut current_values = input
        .existing
        .as_ref()
        .map(|change| change.current_values.clone())
        .unwrap_or_default();
    let mut changed_columns = input
        .existing
        .as_ref()
        .map(|change| change.changed_columns.clone())
        .unwrap_or_default();
    if !changed_columns.contains(&input.column) {
        changed_columns.push(input.column.clone());
    }
    if !original_values.contains_key(&input.column) {
        original_values.insert(
            input.column.clone(),
            input.original_value.clone().unwrap_or(Value::Null),
        );
    }
    let current = match input
        .existing
        .as_ref()
        .and_then(|change| change.current_values.get(&input.column))
    {
        None => input.original_value.clone().unwrap_or(Value::Null),
        Some(value) => value.clone(),
    };
    current_values.insert(input.column.clone(), current);

    let mut change = match &input.existing {
        Some(existing) => existing.clone(),
        None => PendingRowChange {
            row_index: Some(input.row_index),
            row_identity: input.row_identity.clone(),
            original_values: BTreeMap::new(),
            current_values: BTreeMap::new(),
            changed_columns: Vec::new(),
            delete_marked: false,
            insert_marked: None,
            expression_values: None,
        },
    };
    change.original_values = original_values;
    change.current_values = current_values;
    change.changed_columns = changed_columns.clone();
    change.expression_values = Some(expression_values);

    StageCellValueResult {
        change: Some(change),
        changed_columns,
    }
}

pub fn compute_stage_cell_value_result(input: &StageCellValueInput) -> StageCellValueResult {
    if let Some(value) = input.value.as_ref() {
        let mut staged = compute_stage_result(&StageEditInput {
            existing: input.existing.clone(),
            column: input.column.clone(),
            original_value: input.original_value.clone(),
            current_value: Some(value.clone()),
            row_identity: input.row_identity.clone(),
            row_index: input.row_index,
        });
        if let Some(change) = staged.change.as_mut() {
            if let Some(expressions) = change.expression_values.as_mut() {
                expressions.remove(&input.column);
                if expressions.is_empty() {
                    change.expression_values = None;
                }
            }
        }
        return StageCellValueResult {
            change: staged.change,
            changed_columns: staged.changed_columns,
        };
    }

    let existing = match input.existing.as_ref() {
        Some(existing) => existing,
        None => {
            return StageCellValueResult {
                change: None,
                changed_columns: Vec::new(),
            }
        }
    };

    let mut original_values = existing.original_values.clone();
    let mut current_values = existing.current_values.clone();
    original_values.remove(&input.column);
    current_values.remove(&input.column);
    let changed_columns = existing
        .changed_columns
        .iter()
        .filter(|column| *column != &input.column)
        .cloned()
        .collect::<Vec<_>>();

    let mut expression_values = existing.expression_values.clone();
    if let Some(expressions) = expression_values.as_mut() {
        if expressions.contains_key(&input.column) {
            expressions.remove(&input.column);
            if expressions.is_empty() {
                expression_values = None;
            }
        }
    }

    if changed_columns.is_empty() && !existing.delete_marked {
        return StageCellValueResult {
            change: None,
            changed_columns,
        };
    }

    let mut change = existing.clone();
    change.original_values = original_values;
    change.current_values = current_values;
    change.changed_columns = changed_columns.clone();
    if let Some(expressions) = expression_values {
        change.expression_values = Some(expressions);
    }

    StageCellValueResult {
        change: Some(change),
        changed_columns,
    }
}
