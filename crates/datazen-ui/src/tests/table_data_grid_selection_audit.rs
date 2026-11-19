//! T210 independent audit (tester): fresh vectors for the grid-selection
//! geometry — a different render order (age before id, so lexicographic
//! and render order disagree everywhere), boundary walls, combined
//! deltas, and reconcile corners the coder's set does not reach.

use std::collections::HashSet;

use crate::table_data_grid_selection::{
    cell_coords_in_range, cell_range_contains, clamp_cell_coord, columns_covered_by_selection,
    grid_selection_cell_count, grid_selection_contains, grid_selection_equals,
    move_cell_focus, normalize_cell_range, reconcile_grid_selection, rows_covered_by_selection,
    selection_from_anchor_focus, toggle_extra_range, CellCoord, CellRange, GridSelection,
    GridSelectionMode, GridSelectionState,
};

/// A render order where the lexicographic order disagrees on every
/// adjacent pair: age, city, id, name.
const AUDIT_ORDER: &[&str] = &["age", "city", "id", "name"];

fn coord(row: usize, column: &str) -> CellCoord {
    CellCoord {
        row_index: row,
        column_name: column.to_owned(),
    }
}

fn range(start_row: usize, start_col: &str, end_row: usize, end_col: &str) -> CellRange {
    CellRange {
        start: coord(start_row, start_col),
        end: coord(end_row, end_col),
    }
}

#[test]
fn audit_normalization_follows_the_render_order_not_lexicographic() {
    // name renders last although it sorts after id; age renders first.
    let normalized = normalize_cell_range(&range(0, "name", 0, "age"), AUDIT_ORDER).unwrap();
    assert_eq!(normalized.start.column_name, "age");
    assert_eq!(normalized.end.column_name, "name", "age renders before name");
    // The reverse corners normalize identically.
    let same = normalize_cell_range(&range(0, "age", 0, "name"), AUDIT_ORDER).unwrap();
    assert_eq!(normalized, same);
}

#[test]
fn audit_containment_at_the_rectangle_walls() {
    let r = range(0, "city", 2, "id");
    for wall in [
        coord(0, "city"),
        coord(2, "id"),
        coord(2, "city"),
        coord(0, "id"),
        coord(1, "id"),
    ] {
        assert!(
            cell_range_contains(&r, &wall, AUDIT_ORDER),
            "the wall {wall:?} is inside"
        );
    }
    for outside in [coord(3, "city"), coord(0, "name"), coord(1, "age"), coord(3, "name")] {
        assert!(
            !cell_range_contains(&r, &outside, AUDIT_ORDER),
            "the point {outside:?} is outside"
        );
    }
}

#[test]
fn audit_clamp_only_moves_rows_not_columns() {
    // A clamped coordinate keeps its column name — the column is either
    // legal or the whole clamp fails.
    let clamped = clamp_cell_coord(&coord(50, "name"), 3, AUDIT_ORDER).unwrap();
    assert_eq!(clamped, coord(2, "name"));
    assert!(clamp_cell_coord(&coord(50, "ghost"), 3, AUDIT_ORDER).is_none());
}

#[test]
fn audit_combined_deltas_clamp_both_axes() {
    let focus = coord(1, "id");
    // Down 5 and left 2 with only 3 rows and 4 columns: both clamp.
    assert_eq!(
        move_cell_focus(&focus, 5, -2, 3, AUDIT_ORDER).unwrap(),
        coord(2, "age")
    );
    // The zero-delta move is nowhere to go.
    assert!(move_cell_focus(&focus, 0, 0, 3, AUDIT_ORDER).is_none());
    // A move that lands on the same slot through clamping is too.
    let corner = coord(0, "age");
    assert!(move_cell_focus(&corner, -9, -9, 3, AUDIT_ORDER).is_none());
}

#[test]
fn audit_overlapping_regions_count_twice_and_columns_deduplicate() {
    let selection = GridSelection {
        anchor: Some(coord(0, "city")),
        focus: Some(coord(2, "id")),
        extra_ranges: vec![range(0, "id", 2, "city")],
        mode: GridSelectionMode::Cell,
    };
    // Two identical 3x2 rectangles: 12 cells.
    assert_eq!(grid_selection_cell_count(&selection, AUDIT_ORDER), 12);
    // Columns: city+id (main) and id+city (extra) = 2 distinct.
    assert_eq!(columns_covered_by_selection(&selection, AUDIT_ORDER), 2);
}

#[test]
fn audit_row_coverage_reads_extras_without_the_column_order() {
    let empty = HashSet::new();
    let selection = GridSelection {
        anchor: Some(coord(4, "city")),
        focus: Some(coord(1, "age")),
        extra_ranges: vec![range(0, "name", 6, "name")],
        mode: GridSelectionMode::Cell,
    };
    let state = GridSelectionState {
        selection: &selection,
        selected_rows: &empty,
    };
    assert_eq!(
        rows_covered_by_selection(&state),
        [0, 1, 2, 3, 4, 5, 6].into_iter().collect::<HashSet<_>>(),
        "rows 1..=4 plus the extra 0..=6, deduplicated"
    );
}

#[test]
fn audit_the_expansion_walks_row_major_in_the_render_order() {
    let coords = cell_coords_in_range(&range(0, "name", 1, "age"), AUDIT_ORDER);
    let expected: Vec<CellCoord> = vec![
        coord(0, "age"),
        coord(0, "city"),
        coord(0, "id"),
        coord(0, "name"),
        coord(1, "age"),
        coord(1, "city"),
        coord(1, "id"),
        coord(1, "name"),
    ];
    assert_eq!(coords, expected);
}

#[test]
fn audit_euality_is_structural_not_rectangular() {
    // React compares the corners point-wise: two selections that cover
    // the same rectangle with swapped anchor/focus corners are NOT equal
    // (the anchor is part of the structure — it is what pointerdown
    // fixed), and neither is the middle-point variant.
    let a = selection_from_anchor_focus(
        coord(0, "name"),
        coord(2, "age"),
        GridSelectionMode::Cell,
        Vec::new(),
    );
    let swapped = selection_from_anchor_focus(
        coord(2, "age"),
        coord(0, "name"),
        GridSelectionMode::Cell,
        Vec::new(),
    );
    assert!(
        !grid_selection_equals(&a, &swapped, AUDIT_ORDER),
        "swapped corners describe the same rectangle but are not the same structure"
    );
    let same_corners = selection_from_anchor_focus(
        coord(0, "name"),
        coord(2, "age"),
        GridSelectionMode::Cell,
        Vec::new(),
    );
    assert!(grid_selection_equals(&a, &same_corners, AUDIT_ORDER));
    let narrower = selection_from_anchor_focus(
        coord(0, "city"),
        coord(2, "age"),
        GridSelectionMode::Cell,
        Vec::new(),
    );
    assert!(!grid_selection_equals(&a, &narrower, AUDIT_ORDER));
}

#[test]
fn audit_the_toggle_drops_only_the_matching_extra() {
    let selection = GridSelection {
        anchor: Some(coord(0, "age")),
        focus: Some(coord(0, "city")),
        extra_ranges: vec![range(3, "id", 3, "id"), range(4, "name", 4, "name")],
        mode: GridSelectionMode::Cell,
    };
    let next = toggle_extra_range(&selection, &coord(3, "id"), AUDIT_ORDER);
    assert_eq!(next.extra_ranges, vec![range(4, "name", 4, "name")]);
    // The main region is untouched by the drop.
    assert_eq!(next.anchor, Some(coord(0, "age")));
}

#[test]
fn audit_grid_contains_reads_the_first_covering_region() {
    let selection = GridSelection {
        anchor: Some(coord(0, "age")),
        focus: Some(coord(0, "age")),
        extra_ranges: vec![range(1, "id", 1, "name")],
        mode: GridSelectionMode::Cell,
    };
    assert!(grid_selection_contains(&selection, &coord(0, "age"), AUDIT_ORDER));
    assert!(grid_selection_contains(&selection, &coord(1, "name"), AUDIT_ORDER));
    assert!(grid_selection_contains(&selection, &coord(1, "id"), AUDIT_ORDER));
    assert!(!grid_selection_contains(&selection, &coord(2, "id"), AUDIT_ORDER));
}

#[test]
fn audit_reconcile_drops_only_the_out_of_range_extras() {
    let selection = GridSelection {
        anchor: Some(coord(0, "age")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![
            range(2, "id", 2, "id"),
            range(0, "name", 7, "name"),
            range(8, "city", 8, "city"),
        ],
        mode: GridSelectionMode::Cell,
    };
    let reconciled = reconcile_grid_selection(&selection, 3, AUDIT_ORDER);
    assert_eq!(reconciled.extra_ranges, vec![range(2, "id", 2, "id")]);
    assert_eq!(reconciled.anchor, Some(coord(0, "age")));
    assert_eq!(reconciled.focus, Some(coord(1, "name")));
}

#[test]
fn audit_reconcile_returns_the_same_selection_when_nothing_moves() {
    let clean = GridSelection {
        anchor: Some(coord(0, "age")),
        focus: Some(coord(2, "name")),
        extra_ranges: vec![range(1, "id", 1, "id")],
        mode: GridSelectionMode::Cell,
    };
    let reconciled = reconcile_grid_selection(&clean, 5, AUDIT_ORDER);
    assert!(grid_selection_equals(&reconciled, &clean, AUDIT_ORDER));
}
