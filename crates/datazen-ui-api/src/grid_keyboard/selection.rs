use crate::grid_keyboard::types::{CellCoord, CellRange, GridSelection, GridSelectionMode};

fn column_index(coord: &CellCoord, column_order: &[String]) -> Option<usize> {
    column_order.iter().position(|name| name == &coord.column_name)
}

pub fn cell_coord_equals(a: Option<&CellCoord>, b: Option<&CellCoord>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.row_index == right.row_index && left.column_name == right.column_name
        }
        _ => false,
    }
}

pub fn coord_at(row_index: usize, column_index: usize, column_order: &[String]) -> Option<CellCoord> {
    let column_name = column_order.get(column_index)?;
    Some(CellCoord::new(row_index, column_name))
}

pub fn clamp_cell_coord(
    coord: &CellCoord,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    if row_count == 0 {
        return None;
    }
    if column_index(coord, column_order).is_none() {
        return None;
    }
    Some(CellCoord::new(coord.row_index.min(row_count - 1), &coord.column_name))
}

pub fn move_cell_focus(
    focus: &CellCoord,
    delta_row: i64,
    delta_column: i64,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    if row_count == 0 {
        return None;
    }
    let from_column = column_index(focus, column_order)? as i64;
    let next_row = (focus.row_index as i64 + delta_row).clamp(0, row_count as i64 - 1) as usize;
    let next_column = (from_column + delta_column).clamp(0, column_order.len() as i64 - 1) as usize;
    if next_row == focus.row_index && next_column == from_column as usize {
        return None;
    }
    coord_at(next_row, next_column, column_order)
}

pub fn selection_from_anchor_focus(
    anchor: CellCoord,
    focus: CellCoord,
    mode: GridSelectionMode,
    extra_ranges: Vec<CellRange>,
) -> GridSelection {
    if mode == GridSelectionMode::Row {
        return GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        };
    }
    GridSelection {
        anchor: Some(anchor),
        focus: Some(focus),
        extra_ranges,
        mode: GridSelectionMode::Cell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_keyboard::types::EMPTY_ROW_MODE_SELECTION;

    fn order() -> Vec<String> {
        vec!["id".to_owned(), "name".to_owned(), "amount".to_owned()]
    }

    #[test]
    fn coord_at_uses_render_order_and_rejects_out_of_range() {
        assert_eq!(coord_at(2, 1, &order()), Some(CellCoord::new(2, "name")));
        assert_eq!(coord_at(0, 3, &order()), None);
    }

    #[test]
    fn clamp_keeps_known_columns_and_clamps_rows() {
        assert_eq!(clamp_cell_coord(&CellCoord::new(9, "name"), 3, &order()), Some(CellCoord::new(2, "name")));
        assert_eq!(clamp_cell_coord(&CellCoord::new(0, "ghost"), 3, &order()), None);
        assert_eq!(clamp_cell_coord(&CellCoord::new(0, "id"), 0, &order()), None);
    }

    #[test]
    fn moving_clamps_and_returns_none_at_a_border() {
        let focus = CellCoord::new(1, "name");
        assert_eq!(move_cell_focus(&focus, 1, 1, 3, &order()), Some(CellCoord::new(2, "amount")));
        assert_eq!(move_cell_focus(&focus, -9, -9, 3, &order()), Some(CellCoord::new(0, "id")));
        let corner = CellCoord::new(0, "id");
        assert_eq!(move_cell_focus(&corner, -1, -1, 3, &order()), None);
        assert_eq!(move_cell_focus(&corner, 0, 0, 3, &order()), None);
    }

    #[test]
    fn row_mode_selection_drops_all_geometry() {
        let selection = selection_from_anchor_focus(
            CellCoord::new(0, "id"),
            CellCoord::new(1, "name"),
            GridSelectionMode::Row,
            Vec::new(),
        );
        assert_eq!(selection, EMPTY_ROW_MODE_SELECTION.clone());
    }

    #[test]
    fn cell_mode_selection_keeps_corners_and_extra_ranges() {
        let extra = vec![CellRange { start: CellCoord::new(2, "id"), end: CellCoord::new(2, "id") }];
        let selection = selection_from_anchor_focus(
            CellCoord::new(0, "id"),
            CellCoord::new(1, "name"),
            GridSelectionMode::Cell,
            extra.clone(),
        );
        assert_eq!(selection.anchor, Some(CellCoord::new(0, "id")));
        assert_eq!(selection.focus, Some(CellCoord::new(1, "name")));
        assert_eq!(selection.extra_ranges, extra);
    }

    #[test]
    fn equality_handles_both_null_and_mismatched_sides() {
        assert!(cell_coord_equals(None, None));
        assert!(!cell_coord_equals(None, Some(&CellCoord::new(0, "id"))));
        assert!(cell_coord_equals(Some(&CellCoord::new(1, "id")), Some(&CellCoord::new(1, "id"))));
        assert!(!cell_coord_equals(Some(&CellCoord::new(1, "id")), Some(&CellCoord::new(1, "name"))));
    }
}
