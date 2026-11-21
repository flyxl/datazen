use serde_json::{Map, Value};

use super::types::{ClipboardCellCoord, GridClipboardRange};

pub fn to_clipboard_range(
    normalized: &(ClipboardCellCoord, ClipboardCellCoord),
    column_order: &[String],
) -> Option<GridClipboardRange> {
    let left = column_order
        .iter()
        .position(|name| name == &normalized.0.column_name)?;
    let right = column_order
        .iter()
        .position(|name| name == &normalized.1.column_name)?;
    let top_row = normalized.0.row_index.min(normalized.1.row_index);
    let bottom_row = normalized.0.row_index.max(normalized.1.row_index);
    if top_row < 0 {
        return None;
    }
    Some(GridClipboardRange {
        top_row,
        bottom_row,
        left_column: column_order[left.min(right)].clone(),
        right_column: column_order[left.max(right)].clone(),
    })
}

pub fn build_selection_matrix(
    rows: &[Map<String, Value>],
    coords: &[ClipboardCellCoord],
) -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = Vec::new();
    let mut current_row_index = -1i64;
    for coord in coords {
        if coord.row_index != current_row_index {
            out.push(Vec::new());
            current_row_index = coord.row_index;
        }
        let value = usize::try_from(coord.row_index)
            .ok()
            .and_then(|index| rows.get(index))
            .and_then(|row| row.get(&coord.column_name))
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(row) = out.last_mut() {
            row.push(value);
        }
    }
    out
}

pub fn build_rows_matrix(
    rows: &[Map<String, Value>],
    column_order: &[String],
    row_indices: &[i64],
) -> Vec<Vec<Value>> {
    let mut ordered: Vec<i64> = row_indices
        .iter()
        .copied()
        .filter(|index| *index >= 0)
        .collect();
    ordered.sort_unstable();
    ordered
        .into_iter()
        .filter_map(|index| usize::try_from(index).ok())
        .map(|index| {
            column_order
                .iter()
                .map(|name| {
                    rows.get(index)
                        .and_then(|row| row.get(name))
                        .cloned()
                        .unwrap_or(Value::Null)
                })
                .collect()
        })
        .collect()
}
