mod cell_revert;
mod stage_cell;
mod stage_edit;
mod types;

#[cfg(test)]
mod tests;

pub use cell_revert::{has_pending_column_change, restored_cell_value};
pub use stage_cell::{compute_stage_cell_value_result, compute_stage_expression_value_result};
pub use stage_edit::compute_stage_result;
pub use types::{
    PendingRowChange, RowIdentity, StageCellValueInput, StageCellValueResult, StageEditInput,
    StageEditResult, StageExpressionValueInput, ValueMap, WriteExpressionId,
};

use crate::Value;

pub const STAGE_CELL_VALUE_SURFACE: [&str; 9] = [
    "computeStageCellValueResult",
    "computeStageExpressionValueResult",
    "restoredCellValue",
    "hasPendingColumnChange",
    "computeStageResult",
    "PendingRowChange",
    "valuesEqual",
    "WriteExpressionId",
    "STAGE_CELL_VALUE_GAP",
];

pub const STAGE_CELL_VALUE_GAP: [&str; 3] = [
    "the caller writes change into pendingChanges and deletes the key when change is null",
    "JSON.stringify canonical comparison mirrored by serde_json::Value::to_string",
    "the store action that feeds StageCellValueInput and applies StageCellValueResult",
];

pub fn values_equal(left: Option<&Value>, right: Option<&Value>) -> bool {
    let null = Value::Null;
    left.unwrap_or(&null).to_string() == right.unwrap_or(&null).to_string()
}
