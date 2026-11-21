use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::clipboard::{pending_cell_key, table_change_key};
use super::types::{
    CellPasteWrite, ColumnWritability, ColumnWritabilityReason, PasteAlignment, PasteOverflow,
    PastePlan, PastePlanOptions,
};
use crate::types::ColumnSchema;
use crate::{PasteProblem, PasteProblemCode, GRID_PASTE_MAX_BYTES, GRID_PASTE_MAX_CELLS};

fn problem(
    code: PasteProblemCode,
    row: Option<i64>,
    column: Option<&str>,
    detail: Option<String>,
) -> PasteProblem {
    PasteProblem {
        code,
        row,
        column: column.map(str::to_owned),
        detail,
    }
}

fn empty_plan(width: usize) -> PastePlan {
    PastePlan {
        writes: Vec::new(),
        bounds: None,
        problems: Vec::new(),
        skipped: 0,
        overwritten_pending: 0,
        skipped_columns: Vec::new(),
        cross_table: None,
        alignment: PasteAlignment::Unknown,
        dropped_columns: Vec::new(),
        requires_confirmation: true,
        matched_columns: 0,
        total_columns: width,
    }
}

fn is_null_text(text: &str, null_literal: &str) -> bool {
    text == "\\N" || text.to_lowercase() == null_literal.to_lowercase()
}

pub fn column_writability(column: Option<&ColumnSchema>) -> ColumnWritability {
    match column {
        None => ColumnWritability {
            writable: false,
            reason: Some(ColumnWritabilityReason::UnknownColumn),
        },
        Some(schema) if schema.is_auto_increment == Some(true) => ColumnWritability {
            writable: false,
            reason: Some(ColumnWritabilityReason::AutoIncrement),
        },
        _ => ColumnWritability {
            writable: true,
            reason: None,
        },
    }
}

pub fn plan_paste(options: &PastePlanOptions) -> PastePlan {
    let height = options.matrix.cells.len();
    let width = options.matrix.cells.first().map_or(0, Vec::len);
    let mut plan = empty_plan(width);

    if height == 0 || width == 0 {
        plan.problems = vec![problem(PasteProblemCode::Empty, None, None, None)];
        return plan;
    }
    let cell_count = height.saturating_mul(width);
    if cell_count > GRID_PASTE_MAX_CELLS || options.matrix.byte_length > GRID_PASTE_MAX_BYTES {
        plan.problems = vec![problem(
            PasteProblemCode::TooLarge,
            None,
            None,
            Some(cell_count.to_string()),
        )];
        return plan;
    }

    let mut alignment = PasteAlignment::ByPosition;
    let mut mapping: Vec<Option<String>> = Vec::new();
    let mut dropped_columns: Vec<String> = Vec::new();
    let mut matched_columns = 0usize;
    if let Some(source) = &options.source {
        if source.column_names.len() == width {
            let target_names: HashSet<&str> =
                options.column_order.iter().map(String::as_str).collect();
            let by_name: Vec<Option<String>> = source
                .column_names
                .iter()
                .map(|name| {
                    if target_names.contains(name.as_str()) {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .collect();
            let matched = by_name.iter().filter(|name| name.is_some()).count();
            if matched > 0 {
                alignment = PasteAlignment::ByName;
                mapping = by_name;
                matched_columns = matched;
                let dropped: HashSet<&String> = source
                    .column_names
                    .iter()
                    .filter(|name| !target_names.contains(name.as_str()))
                    .collect();
                dropped_columns = dropped.into_iter().cloned().collect();
            }
        }
    }
    if alignment == PasteAlignment::ByPosition {
        let landing = options
            .region
            .as_ref()
            .map_or(options.anchor.column_name.as_str(), |region| {
                region.left_column.as_str()
            });
        let Some(left_index) = options.column_order.iter().position(|name| name == landing) else {
            plan.problems = vec![problem(PasteProblemCode::NoWritableCell, None, None, None)];
            return plan;
        };
        mapping = (0..width)
            .map(|column| options.column_order.get(left_index + column).cloned())
            .collect();
        matched_columns = 0;
        dropped_columns.clear();
    }

    let source_key = options
        .source
        .as_ref()
        .map(|source| source.table_key.as_str());
    let target_key = options
        .context
        .as_ref()
        .map(|context| table_change_key(Some(context)));
    let cross_table = match (source_key, target_key.as_ref()) {
        (Some(source), Some(target)) => Some(source != target),
        _ => None,
    };
    let requires_confirmation = !matches!((source_key, target_key.as_ref()), (Some(source), Some(target)) if source == target.as_str());

    plan.alignment = alignment;
    plan.cross_table = cross_table;
    plan.requires_confirmation = requires_confirmation;
    plan.matched_columns = matched_columns;
    plan.total_columns = width;
    plan.dropped_columns = dropped_columns.clone();

    let row_start = options.anchor.row_index;
    if row_start < 0 || options.page_row_count <= 0 {
        plan.problems = vec![problem(PasteProblemCode::OutOfBounds, None, None, None)];
        return plan;
    }

    let column_by_name: HashMap<&str, &ColumnSchema> = options
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect();
    let right_edge: i64 = if options.overflow == PasteOverflow::Clip {
        if let Some(region) = &options.region {
            options
                .column_order
                .iter()
                .position(|name| name == &region.right_column)
                .map_or(-1, |index| index as i64)
        } else {
            options.column_order.len() as i64 - 1
        }
    } else {
        options.column_order.len() as i64 - 1
    };
    let bottom_edge: i64 = if options.overflow == PasteOverflow::Clip {
        if let Some(region) = &options.region {
            region.bottom_row.min(options.page_row_count - 1)
        } else {
            options.page_row_count - 1
        }
    } else {
        options.page_row_count - 1
    };

    let mut writes = Vec::new();
    let mut problems = Vec::new();
    let mut unwritable_columns = HashSet::new();
    let mut skipped_column_order: Vec<String> = Vec::new();
    let mut reported_columns = HashSet::new();
    let mut skipped = 0usize;
    let mut clipped = 0usize;
    let mut overwritten_pending = 0usize;
    let mut min_row: Option<i64> = None;
    let mut max_row: Option<i64> = None;
    let mut min_column: Option<usize> = None;
    let mut max_column: Option<usize> = None;

    for matrix_row in 0..height {
        let row_index = row_start + matrix_row as i64;
        if row_index > bottom_edge {
            clipped += width;
            continue;
        }
        for matrix_column in 0..width {
            let Some(column_name) = mapping[matrix_column].clone() else {
                skipped += 1;
                continue;
            };
            let Some(column_position) = options
                .column_order
                .iter()
                .position(|name| name == &column_name)
            else {
                skipped += 1;
                continue;
            };
            if options.overflow == PasteOverflow::Clip && (column_position as i64) > right_edge {
                skipped += 1;
                continue;
            }
            let column = column_by_name.get(column_name.as_str()).copied();
            let writability = column_writability(column);
            if !writability.writable {
                skipped += 1;
                unwritable_columns.insert(column_name.clone());
                if writability.reason == Some(ColumnWritabilityReason::AutoIncrement)
                    && !skipped_column_order.contains(&column_name)
                {
                    skipped_column_order.push(column_name.clone());
                }
                if reported_columns.insert(column_name.clone()) {
                    problems.push(problem(
                        PasteProblemCode::ReadOnly,
                        None,
                        Some(&column_name),
                        writability.reason.map(|reason| reason.as_str().to_owned()),
                    ));
                }
                continue;
            }
            let text = options
                .matrix
                .cells
                .get(matrix_row)
                .and_then(|row| row.get(matrix_column))
                .map_or("", String::as_str);
            let quoted = options
                .matrix
                .quoted
                .get(matrix_row)
                .and_then(|row| row.get(matrix_column))
                .copied()
                .unwrap_or(false);
            let value = if quoted {
                Value::String(text.to_owned())
            } else if text.is_empty() || is_null_text(text, &options.null_literal) {
                if column.is_some_and(|schema| !schema.nullable) {
                    skipped += 1;
                    problems.push(problem(
                        PasteProblemCode::NotNullEmpty,
                        Some(matrix_row as i64),
                        Some(&column_name),
                        None,
                    ));
                    continue;
                }
                Value::Null
            } else {
                Value::String(text.to_owned())
            };
            if options
                .pending_cell_keys
                .contains(&pending_cell_key(row_index, &column_name))
            {
                overwritten_pending += 1;
            }
            writes.push(CellPasteWrite {
                row_index,
                column_name: column_name.clone(),
                value,
            });
            min_row = Some(min_row.map_or(row_index, |value| value.min(row_index)));
            max_row = Some(max_row.map_or(row_index, |value| value.max(row_index)));
            min_column =
                Some(min_column.map_or(column_position, |value| value.min(column_position)));
            max_column =
                Some(max_column.map_or(column_position, |value| value.max(column_position)));
        }
    }

    if alignment == PasteAlignment::ByName && !plan.dropped_columns.is_empty() {
        problems.push(problem(
            PasteProblemCode::ColumnMismatch,
            None,
            None,
            Some(plan.dropped_columns.join(", ")),
        ));
    }
    if clipped > 0 {
        problems.push(problem(
            PasteProblemCode::OutOfBounds,
            None,
            None,
            Some(clipped.to_string()),
        ));
    }
    if writes.is_empty() && !unwritable_columns.is_empty() {
        let target_columns: HashSet<String> = mapping.iter().flatten().cloned().collect();
        if !target_columns.is_empty() && unwritable_columns.len() >= target_columns.len() {
            problems.push(problem(PasteProblemCode::NoWritableCell, None, None, None));
        }
    }

    plan.writes = writes;
    plan.problems = problems;
    plan.skipped = skipped + clipped;
    plan.overwritten_pending = overwritten_pending;
    plan.skipped_columns = skipped_column_order;
    plan.bounds = match (min_row, max_row, min_column, max_column) {
        (Some(top), Some(bottom), Some(left), Some(right)) => Some(crate::GridClipboardRange {
            top_row: top,
            bottom_row: bottom,
            left_column: options
                .column_order
                .get(left)
                .cloned()
                .unwrap_or_else(|| options.anchor.column_name.clone()),
            right_column: options
                .column_order
                .get(right)
                .cloned()
                .unwrap_or_else(|| options.anchor.column_name.clone()),
        }),
        _ => None,
    };
    plan
}
