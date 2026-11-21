use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellCoord {
    pub row_index: usize,
    pub column_name: String,
}

impl CellCoord {
    pub fn new(row_index: usize, column_name: &str) -> Self {
        Self {
            row_index,
            column_name: column_name.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellRange {
    pub start: CellCoord,
    pub end: CellCoord,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridSelectionMode {
    #[default]
    None,
    Cell,
    Row,
}

impl GridSelectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Cell => "cell",
            Self::Row => "row",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridSelection {
    pub anchor: Option<CellCoord>,
    pub focus: Option<CellCoord>,
    pub extra_ranges: Vec<CellRange>,
    pub mode: GridSelectionMode,
}

impl GridSelection {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn row_mode() -> Self {
        Self {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableStateForSelection {
    pub columns: Vec<String>,
    pub visible_columns: Option<Vec<String>>,
    pub column_order: Option<Vec<String>>,
    pub rows_count: usize,
    pub selected_rows: BTreeSet<usize>,
    pub last_selected_index: Option<usize>,
    pub grid_selection: GridSelection,
}

impl Default for TableStateForSelection {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            visible_columns: None,
            column_order: None,
            rows_count: 0,
            selected_rows: BTreeSet::new(),
            last_selected_index: None,
            grid_selection: GridSelection::empty(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionPatch {
    pub grid_selection: Option<GridSelection>,
    pub selected_rows: Option<BTreeSet<usize>>,
    pub last_selected_index: Option<Option<usize>>,
}

fn coord_is_displayable(
    coord: Option<&CellCoord>,
    row_count: usize,
    column_order: &[String],
) -> bool {
    match coord {
        Some(coord) => {
            coord.row_index < row_count
                && column_order.iter().any(|name| name == &coord.column_name)
        }
        None => false,
    }
}

pub fn normalize_cell_range(range: &CellRange, column_order: &[String]) -> Option<CellRange> {
    let start_column = column_order
        .iter()
        .position(|name| name == &range.start.column_name)?;
    let end_column = column_order
        .iter()
        .position(|name| name == &range.end.column_name)?;
    let top_row = range.start.row_index.min(range.end.row_index);
    let bottom_row = range.start.row_index.max(range.end.row_index);
    let (left, right) = if start_column <= end_column {
        (&range.start, &range.end)
    } else {
        (&range.end, &range.start)
    };
    Some(CellRange {
        start: CellCoord {
            row_index: top_row,
            column_name: left.column_name.clone(),
        },
        end: CellCoord {
            row_index: bottom_row,
            column_name: right.column_name.clone(),
        },
    })
}

fn same_range(a: &CellRange, b: &CellRange) -> bool {
    a.start == b.start && a.end == b.end
}

pub fn selection_from_anchor_focus(
    anchor: &CellCoord,
    focus: &CellCoord,
    mode: GridSelectionMode,
) -> GridSelection {
    match mode {
        GridSelectionMode::Row => GridSelection::row_mode(),
        _ => GridSelection {
            anchor: Some(anchor.clone()),
            focus: Some(focus.clone()),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        },
    }
}

pub fn reconcile_grid_selection(
    selection: &GridSelection,
    row_count: usize,
    column_order: &[String],
) -> GridSelection {
    match selection.mode {
        GridSelectionMode::Row => {
            if selection.anchor.is_none()
                && selection.focus.is_none()
                && selection.extra_ranges.is_empty()
            {
                selection.clone()
            } else {
                GridSelection::row_mode()
            }
        }
        GridSelectionMode::None => GridSelection::empty(),
        GridSelectionMode::Cell => {
            if !coord_is_displayable(selection.anchor.as_ref(), row_count, column_order)
                || !coord_is_displayable(selection.focus.as_ref(), row_count, column_order)
            {
                return GridSelection::empty();
            }
            let mut extra_ranges = Vec::new();
            let mut dropped_any = false;
            for range in selection.extra_ranges.iter() {
                let normalized = normalize_cell_range(range, column_order);
                match normalized {
                    Some(normalized)
                        if normalized.start.row_index < row_count
                            && normalized.end.row_index < row_count =>
                    {
                        extra_ranges.push(normalized)
                    }
                    _ => dropped_any = true,
                }
            }
            if !dropped_any
                && extra_ranges.len() == selection.extra_ranges.len()
                && extra_ranges
                    .iter()
                    .zip(selection.extra_ranges.iter())
                    .all(|(a, b)| same_range(a, b))
            {
                return selection.clone();
            }
            GridSelection {
                extra_ranges,
                ..selection.clone()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> Vec<String> {
        ["a", "b", "c"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect()
    }

    #[test]
    fn empty_and_row_mode_shapes_are_pinned() {
        assert_eq!(GridSelection::empty().mode.as_str(), "none");
        let row = GridSelection::row_mode();
        assert_eq!(row.mode, GridSelectionMode::Row);
        assert_eq!(row.anchor, None);
        assert!(row.extra_ranges.is_empty());
    }

    #[test]
    fn anchor_focus_builder_drops_geometry_in_row_mode() {
        let anchor = CellCoord::new(1, "a");
        let focus = CellCoord::new(2, "b");
        let cell = selection_from_anchor_focus(&anchor, &focus, GridSelectionMode::Cell);
        assert_eq!(cell.anchor, Some(anchor.clone()));
        assert_eq!(cell.focus, Some(focus.clone()));
        let row = selection_from_anchor_focus(&anchor, &focus, GridSelectionMode::Row);
        assert_eq!(row, GridSelection::row_mode());
    }

    #[test]
    fn cell_ranges_normalize_corner_order_and_drop_unknown_columns() {
        let range = CellRange {
            start: CellCoord::new(3, "c"),
            end: CellCoord::new(1, "a"),
        };
        let normalized = normalize_cell_range(&range, &order()).unwrap();
        assert_eq!(normalized.start, CellCoord::new(1, "a"));
        assert_eq!(normalized.end, CellCoord::new(3, "c"));
        assert_eq!(
            normalize_cell_range(
                &CellRange {
                    start: CellCoord::new(0, "?"),
                    end: CellCoord::new(1, "a")
                },
                &order()
            ),
            None
        );
    }

    #[test]
    fn reconcile_drops_invalid_corners_and_extra_ranges() {
        let selection = GridSelection {
            anchor: Some(CellCoord::new(0, "a")),
            focus: Some(CellCoord::new(1, "b")),
            extra_ranges: vec![
                CellRange {
                    start: CellCoord::new(0, "c"),
                    end: CellCoord::new(0, "c"),
                },
                CellRange {
                    start: CellCoord::new(9, "a"),
                    end: CellCoord::new(9, "a"),
                },
            ],
            mode: GridSelectionMode::Cell,
        };
        let reconciled = reconcile_grid_selection(&selection, 2, &order());
        assert_eq!(reconciled.extra_ranges.len(), 1);
        assert_eq!(reconciled.extra_ranges[0].start, CellCoord::new(0, "c"));

        let out_of_bounds = GridSelection {
            anchor: Some(CellCoord::new(9, "a")),
            focus: Some(CellCoord::new(0, "a")),
            ..GridSelection::default()
        };
        assert_eq!(
            reconcile_grid_selection(&out_of_bounds, 2, &order()),
            GridSelection::empty()
        );

        assert_eq!(
            reconcile_grid_selection(&GridSelection::row_mode(), 0, &order()).mode,
            GridSelectionMode::Row
        );
    }
}
