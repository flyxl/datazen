//! Grid-selection geometry and state transitions, ported from React
//! `packages/frontend/src/stores/tableData/gridSelection.ts` (431 lines,
//! T210, DB-01).
//!
//! Pure logic only: no UI, no store — the whole selection model is
//! unit-tested without a component tree. Coordinates are
//! `(row_index, column_name)`: the column **name** stays stable across
//! `visibleColumns` changes and column moves, while a column index
//! does not (00-contracts §4.1).
//!
//! ## JS → Rust boundary notes
//!
//! * React guards several inputs with `Number.isInteger` / `>= 0`
//!   (negative or fractional row indices arriving from outside). The
//!   port's `row_index` is `usize`: those guards are unreachable and are
//!   not re-implemented; the sites carry a note instead.
//! * React freezes `EMPTY_GRID_SELECTION` so memo dependencies stay
//!   referentially stable. Rust has no memo identity: [`GridSelection::empty`]
//!   is the same shape, freshly built.
//! * React's `GridSelectionState` makes `selected_rows` a required field
//!   so a bare `GridSelection` can never silently report an empty row
//!   set in row mode (00-contracts §4.2.1 落地校正). The port keeps the
//!   freeze with [`GridSelectionState`], borrowing the caller's set.
//! * `row` mode carries no geometry at all (00-contracts §4 invariant
//!   I2): the row highlight comes from `selected_rows`, so
//!   anchor/focus/extras are dropped instead of kept as a hidden second
//!   source of truth.

use std::collections::HashSet;

/// One cell coordinate: the 0-based row index inside the current page
/// (render-only, never used to build SQL) and the stable column name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CellCoord {
    pub(crate) row_index: usize,
    pub(crate) column_name: String,
}

/// A rectangle by its two corners; the order is meaningless and
/// normalized by [`normalize_cell_range`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CellRange {
    pub(crate) start: CellCoord,
    pub(crate) end: CellCoord,
}

/// The selection mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GridSelectionMode {
    #[default]
    None,
    Cell,
    Row,
}

/// The selection: the anchor corner (fixed on pointerdown /
/// Shift-extension start), the moving focus corner, and the extra
/// non-contiguous single-cell ranges (Cmd/Ctrl+click).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridSelection {
    pub(crate) anchor: Option<CellCoord>,
    pub(crate) focus: Option<CellCoord>,
    pub(crate) extra_ranges: Vec<CellRange>,
    pub(crate) mode: GridSelectionMode,
}

impl GridSelection {
    /// `EMPTY_GRID_SELECTION`: every "no selection" site's shape.
    pub(crate) fn empty() -> Self {
        Self::default()
    }
}

/// The selection plus the row set that `row` mode keeps in
/// `TableState.selectedRows` — [`rows_covered_by_selection`] reads it
/// verbatim in row mode.
#[derive(Clone, Debug)]
pub(crate) struct GridSelectionState<'a> {
    pub(crate) selection: &'a GridSelection,
    pub(crate) selected_rows: &'a HashSet<usize>,
}

fn coord_equals(a: &CellCoord, b: &CellCoord) -> bool {
    a.row_index == b.row_index && a.column_name == b.column_name
}

/// Column name → index in render order; `None` when the column is not
/// displayed (React's `-1`).
pub(crate) fn column_index_from_coord(
    coord: &CellCoord,
    column_order: &[&str],
) -> Option<usize> {
    column_order.iter().position(|name| *name == coord.column_name.as_str())
}

/// `cellCoordEquals`: both-null equality, then coordinate equality.
pub(crate) fn cell_coord_equals(a: Option<&CellCoord>, b: Option<&CellCoord>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => coord_equals(a, b),
        _ => false,
    }
}

/// Row/cell coordinate → coordinate, or `None` when either index falls
/// outside the grid.
pub(crate) fn coord_at(
    row_index: usize,
    column_index: usize,
    column_order: &[&str],
) -> Option<CellCoord> {
    let column_name = *column_order.get(column_index)?;
    Some(CellCoord {
        row_index,
        column_name: column_name.to_owned(),
    })
}

/// Pull a coordinate back into the grid: an unknown column (hidden while
/// referenced) or an empty grid has no legal coordinate (`None`); the row
/// index is clamped.
pub(crate) fn clamp_cell_coord(
    coord: &CellCoord,
    row_count: usize,
    column_order: &[&str],
) -> Option<CellCoord> {
    if row_count == 0 {
        return None;
    }
    if column_index_from_coord(coord, column_order).is_none() {
        return None;
    }
    Some(CellCoord {
        row_index: coord.row_index.min(row_count - 1),
        column_name: coord.column_name.clone(),
    })
}

/// Normalize a range so rows grow downwards and columns grow in render
/// order — never lexicographic, because `id` sorts before `name` while
/// the grid may show the opposite. An unknown column has no position to
/// compare, so the range is "not found".
pub(crate) fn normalize_cell_range(
    range: &CellRange,
    column_order: &[&str],
) -> Option<CellRange> {
    let start_column = column_index_from_coord(&range.start, column_order)?;
    let end_column = column_index_from_coord(&range.end, column_order)?;
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

/// Normalized anchor→focus rectangle; `None` unless both corners exist
/// and are displayable.
pub(crate) fn normalize_selection_range(
    selection: &GridSelection,
    column_order: &[&str],
) -> Option<CellRange> {
    let anchor = selection.anchor.as_ref()?;
    let focus = selection.focus.as_ref()?;
    normalize_cell_range(
        &CellRange {
            start: anchor.clone(),
            end: focus.clone(),
        },
        column_order,
    )
}

/// Every region the selection covers, the main region first, then the
/// extra ranges (all normalized).
pub(crate) fn selection_ranges(
    selection: &GridSelection,
    column_order: &[&str],
) -> Vec<CellRange> {
    if selection.mode != GridSelectionMode::Cell {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    if let Some(main) = normalize_selection_range(selection, column_order) {
        ranges.push(main);
    }
    for extra in &selection.extra_ranges {
        if let Some(normalized) = normalize_cell_range(extra, column_order) {
            ranges.push(normalized);
        }
    }
    ranges
}

/// Whether a normalized rectangle covers one coordinate.
pub(crate) fn cell_range_contains(
    range: &CellRange,
    coord: &CellCoord,
    column_order: &[&str],
) -> bool {
    let Some(normalized) = normalize_cell_range(range, column_order) else {
        return false;
    };
    let Some(column) = column_index_from_coord(coord, column_order) else {
        return false;
    };
    // Both corners resolve: normalize_cell_range succeeded on them.
    let start_column = column_index_from_coord(&normalized.start, column_order)
        .expect("normalized corner resolves");
    let end_column = column_index_from_coord(&normalized.end, column_order)
        .expect("normalized corner resolves");
    coord.row_index >= normalized.start.row_index
        && coord.row_index <= normalized.end.row_index
        && column >= start_column
        && column <= end_column
}

/// Whether any region of the selection covers one coordinate.
pub(crate) fn grid_selection_contains(
    selection: &GridSelection,
    coord: &CellCoord,
    column_order: &[&str],
) -> bool {
    selection_ranges(selection, column_order)
        .iter()
        .any(|range| cell_range_contains(range, coord, column_order))
}

/// Total cell area of every region, overlapping regions counted twice
/// (the render/summary metric).
pub(crate) fn grid_selection_cell_count(
    selection: &GridSelection,
    column_order: &[&str],
) -> usize {
    let mut total = 0;
    for range in selection_ranges(selection, column_order) {
        // Both corners are normalized, so both resolve.
        if let (Some(start), Some(end)) = (
            column_index_from_coord(&range.start, column_order),
            column_index_from_coord(&range.end, column_order),
        ) {
            let rows = range.end.row_index - range.start.row_index + 1;
            let columns = end - start + 1;
            total += rows * columns;
        }
    }
    total
}

/// Distinct displayed columns covered by the selection (the "M" of
/// "N rows × M columns").
pub(crate) fn columns_covered_by_selection(
    selection: &GridSelection,
    column_order: &[&str],
) -> usize {
    let mut covered: HashSet<usize> = HashSet::new();
    for range in selection_ranges(selection, column_order) {
        if let (Some(start), Some(end)) = (
            column_index_from_coord(&range.start, column_order),
            column_index_from_coord(&range.end, column_order),
        ) {
            for column in start..=end {
                covered.insert(column);
            }
        }
    }
    covered.len()
}

/// Row indices covered by the selection (unordered, de-duplicated) — the
/// only source for row-level batch actions in `cell` mode; never
/// written back into `selected_rows` (00-contracts §4.2 rule 3). Row
/// indices are UI indices: a write path still resolves a row identity
/// through `rowIdentityAnchors` before touching the database.
pub(crate) fn rows_covered_by_selection(state: &GridSelectionState<'_>) -> HashSet<usize> {
    if state.selection.mode == GridSelectionMode::Row {
        return state.selected_rows.clone();
    }
    if state.selection.mode != GridSelectionMode::Cell {
        return HashSet::new();
    }
    let (Some(anchor), Some(focus)) = (state.selection.anchor.as_ref(), state.selection.focus.as_ref())
    else {
        return HashSet::new();
    };
    // Row coverage never depends on column order: the rectangles are
    // rebuilt directly from the corners.
    let mut rows = HashSet::new();
    let mut add_range_rows = |start: &CellCoord, end: &CellCoord| {
        let (top, bottom) = (start.row_index.min(end.row_index), start.row_index.max(end.row_index));
        rows.extend(top..=bottom);
    };
    add_range_rows(anchor, focus);
    for extra in &state.selection.extra_ranges {
        add_range_rows(&extra.start, &extra.end);
    }
    rows
}

/// Every coordinate of a normalized rectangle, row-major — the **one**
/// place a region is expanded into coordinates (00-contracts §4.1).
pub(crate) fn cell_coords_in_range(
    range: &CellRange,
    column_order: &[&str],
) -> Vec<CellCoord> {
    let Some(normalized) = normalize_cell_range(range, column_order) else {
        return Vec::new();
    };
    let (Some(left), Some(right)) = (
        column_index_from_coord(&normalized.start, column_order),
        column_index_from_coord(&normalized.end, column_order),
    ) else {
        return Vec::new();
    };
    let mut coords = Vec::new();
    for row_index in normalized.start.row_index..=normalized.end.row_index {
        for column in left..=right {
            if let Some(name) = column_order.get(column) {
                coords.push(CellCoord {
                    row_index,
                    column_name: (*name).to_owned(),
                });
            }
        }
    }
    coords
}

fn same_range(a: &CellRange, b: &CellRange) -> bool {
    coord_equals(&a.start, &b.start) && coord_equals(&a.end, &b.end)
}

/// Structural equality; extra ranges are compared as an unordered set.
pub(crate) fn grid_selection_equals(
    a: &GridSelection,
    b: &GridSelection,
    column_order: &[&str],
) -> bool {
    if a.mode != b.mode {
        return false;
    }
    if !cell_coord_equals(a.anchor.as_ref(), b.anchor.as_ref()) {
        return false;
    }
    if !cell_coord_equals(a.focus.as_ref(), b.focus.as_ref()) {
        return false;
    }
    if a.extra_ranges.len() != b.extra_ranges.len() {
        return false;
    }
    a.extra_ranges.iter().all(|range_a| {
        let Some(normalized_a) = normalize_cell_range(range_a, column_order) else {
            return false;
        };
        b.extra_ranges.iter().any(|range_b| {
            let Some(normalized_b) = normalize_cell_range(range_b, column_order) else {
                return false;
            };
            same_range(&normalized_a, &normalized_b)
        })
    })
}

/// Move the focus corner by a row/column delta and clamp it into the
/// grid; `None` when the clamped result equals the input — nowhere left
/// to go, so the caller leaves the selection untouched.
pub(crate) fn move_cell_focus(
    focus: &CellCoord,
    delta_row: i64,
    delta_column: i64,
    row_count: usize,
    column_order: &[&str],
) -> Option<CellCoord> {
    if row_count == 0 {
        return None;
    }
    let from_column = column_index_from_coord(focus, column_order)? as i64;
    let next_row = (focus.row_index as i64 + delta_row)
        .clamp(0, row_count as i64 - 1) as usize;
    let next_column = (from_column + delta_column)
        .clamp(0, column_order.len() as i64 - 1) as usize;
    if next_row == focus.row_index && next_column == from_column as usize {
        return None;
    }
    let column_name = *column_order.get(next_column)?;
    Some(CellCoord {
        row_index: next_row,
        column_name: column_name.to_owned(),
    })
}

/// Build a selection from its two corners. `row` mode carries no
/// geometry at all (00-contracts §4 invariant I2).
pub(crate) fn selection_from_anchor_focus(
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

/// Cmd/Ctrl+click on one cell: append it as an extra single-cell region,
/// or drop it when it already is one. Clicking inside the main region is
/// a no-op (already selected); an unknown column leaves the selection
/// untouched.
pub(crate) fn toggle_extra_range(
    selection: &GridSelection,
    coord: &CellCoord,
    column_order: &[&str],
) -> GridSelection {
    let single = CellRange {
        start: coord.clone(),
        end: coord.clone(),
    };
    let Some(extra) = normalize_cell_range(&single, column_order) else {
        return selection.clone();
    };
    if selection.mode != GridSelectionMode::Cell
        || selection.anchor.is_none()
        || selection.focus.is_none()
    {
        return GridSelection {
            anchor: Some(coord.clone()),
            focus: Some(coord.clone()),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        };
    }
    if let Some(existing) = selection
        .extra_ranges
        .iter()
        .position(|range| {
            normalize_cell_range(range, column_order)
                .is_some_and(|normalized| same_range(&normalized, &extra))
        })
    {
        let mut next = selection.clone();
        next.extra_ranges.remove(existing);
        return next;
    }
    if let Some(main) = normalize_selection_range(selection, column_order) {
        if cell_range_contains(&main, coord, column_order) {
            return selection.clone();
        }
    }
    let mut next = selection.clone();
    next.extra_ranges.push(extra);
    next
}

fn coord_is_displayable(
    coord: Option<&CellCoord>,
    row_count: usize,
    column_order: &[&str],
) -> bool {
    let Some(coord) = coord else {
        return false;
    };
    coord.row_index < row_count && column_order.contains(&coord.column_name.as_str())
}

/// Converge a selection after rows or displayed columns changed (page
/// reload, column visibility): an invalid main corner clears the whole
/// selection; an invalid extra range is dropped; `row` mode keeps no
/// geometry by construction.
pub(crate) fn reconcile_grid_selection(
    selection: &GridSelection,
    row_count: usize,
    column_order: &[&str],
) -> GridSelection {
    if selection.mode == GridSelectionMode::Row {
        if selection.anchor.is_none()
            && selection.focus.is_none()
            && selection.extra_ranges.is_empty()
        {
            return selection.clone();
        }
        return GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        };
    }
    if selection.mode == GridSelectionMode::None {
        return GridSelection::empty();
    }
    if !coord_is_displayable(selection.anchor.as_ref(), row_count, column_order)
        || !coord_is_displayable(selection.focus.as_ref(), row_count, column_order)
    {
        return GridSelection::empty();
    }
    let mut extra_ranges = Vec::new();
    let mut dropped_any = false;
    for range in &selection.extra_ranges {
        match normalize_cell_range(range, column_order) {
            Some(normalized)
                if normalized.start.row_index < row_count
                    && normalized.end.row_index < row_count =>
            {
                extra_ranges.push(normalized);
            }
            _ => {
                dropped_any = true;
            }
        }
    }
    if !dropped_any {
        let unchanged = extra_ranges
            .iter()
            .zip(selection.extra_ranges.iter())
            .all(|(range, original)| same_range(range, original));
        if unchanged {
            return selection.clone();
        }
    }
    let mut next = selection.clone();
    next.extra_ranges = extra_ranges;
    next
}
