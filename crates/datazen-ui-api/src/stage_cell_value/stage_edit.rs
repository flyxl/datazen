use super::{values_equal, PendingRowChange, StageEditInput, StageEditResult, Value};

pub fn compute_stage_result(input: &StageEditInput) -> StageEditResult {
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
    let delete_marked = input
        .existing
        .as_ref()
        .map(|change| change.delete_marked)
        .unwrap_or(false);

    if values_equal(input.original_value.as_ref(), input.current_value.as_ref()) {
        original_values.remove(&input.column);
        current_values.remove(&input.column);
        changed_columns.retain(|column| column != &input.column);
    } else {
        original_values.insert(
            input.column.clone(),
            input.original_value.clone().unwrap_or(Value::Null),
        );
        current_values.insert(
            input.column.clone(),
            input.current_value.clone().unwrap_or(Value::Null),
        );
        if !changed_columns.contains(&input.column) {
            changed_columns.push(input.column.clone());
        }
    }

    if changed_columns.is_empty() && !delete_marked {
        return StageEditResult {
            change: None,
            changed_columns,
        };
    }

    StageEditResult {
        change: Some(PendingRowChange {
            row_index: Some(input.row_index),
            row_identity: input.row_identity.clone(),
            original_values,
            current_values,
            changed_columns: changed_columns.clone(),
            delete_marked,
            insert_marked: None,
            expression_values: None,
        }),
        changed_columns,
    }
}
