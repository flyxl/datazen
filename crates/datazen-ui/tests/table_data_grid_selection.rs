//! T210: grid-selection geometry and state transitions (DB-01) —
//! normalization in render order (never lexicographic), containment,
//! coverage counts, structural equality, focus moves, toggle, and
//! reconcile.

use std::collections::HashSet;

use datazen_ui::table_data_grid_selection::{
    cell_coord_equals, cell_coords_in_range, cell_range_contains, clamp_cell_coord,
    column_index_from_coord, coord_at, columns_covered_by_selection,
    grid_selection_cell_count, grid_selection_contains, grid_selection_equals,
    move_cell_focus, normalize_cell_range, normalize_selection_range,
    reconcile_grid_selection, rows_covered_by_selection, selection_from_anchor_focus,
    selection_ranges, toggle_extra_range, CellCoord, CellRange, GridSelection,
    GridSelectionMode, GridSelectionState,
};

const ORDER: &[&str] = &["id", "name", "age", "city"];

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
fn normalization_orders_rows_downwards_and_columns_in_render_order() {
    // name renders before age although "age" sorts first lexicographically.
    let normalized = normalize_cell_range(&range(4, "age", 1, "name"), ORDER).unwrap();
    assert_eq!(normalized.start.row_index, 1);
    assert_eq!(normalized.start.column_name, "name");
    assert_eq!(normalized.end.row_index, 4);
    assert_eq!(normalized.end.column_name, "age");
}

#[test]
fn normalization_rejects_an_unknown_column() {
    assert!(normalize_cell_range(&range(0, "ghost", 1, "name"), ORDER).is_none());
    assert!(normalize_cell_range(&range(0, "name", 1, "ghost"), ORDER).is_none());
}

#[test]
fn coord_at_maps_only_displayed_slots() {
    assert_eq!(
        coord_at(3, 2, ORDER).unwrap(),
        coord(3, "age")
    );
    assert!(coord_at(0, 4, ORDER).is_none(), "past the last column");
}

#[test]
fn clamp_pulls_the_row_back_and_requires_a_known_column() {
    assert_eq!(
        clamp_cell_coord(&coord(99, "name"), 10, ORDER).unwrap(),
        coord(9, "name")
    );
    assert!(clamp_cell_coord(&coord(0, "ghost"), 10, ORDER).is_none());
    assert!(clamp_cell_coord(&coord(0, "name"), 0, ORDER).is_none(), "empty grid");
}

#[test]
fn column_index_reads_render_positions() {
    assert_eq!(column_index_from_coord(&coord(0, "id"), ORDER), Some(0));
    assert_eq!(column_index_from_coord(&coord(0, "city"), ORDER), Some(3));
    assert_eq!(column_index_from_coord(&coord(0, "ghost"), ORDER), None);
}

#[test]
fn the_selection_ranges_cover_main_then_extras_all_normalized() {
    let selection = GridSelection {
        anchor: Some(coord(4, "age")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "id", 0, "id")],
        mode: GridSelectionMode::Cell,
    };
    let ranges = selection_ranges(&selection, ORDER);
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0], range(1, "name", 4, "age"));
    assert_eq!(ranges[1], range(0, "id", 0, "id"));

    let quiet = GridSelection {
        mode: GridSelectionMode::Row,
        ..GridSelection::empty()
    };
    assert!(selection_ranges(&quiet, ORDER).is_empty(), "row mode has no geometry");
}

#[test]
fn containment_reads_the_normalized_rectangle() {
    let r = range(1, "name", 3, "age");
    assert!(cell_range_contains(&r, &coord(2, "age"), ORDER));
    assert!(cell_range_contains(&r, &coord(1, "name"), ORDER));
    assert!(cell_range_contains(&r, &coord(3, "age"), ORDER));
    assert!(!cell_range_contains(&r, &coord(0, "name"), ORDER));
    assert!(!cell_range_contains(&r, &coord(2, "city"), ORDER));
    assert!(!cell_range_contains(&r, &coord(2, "ghost"), ORDER));
}

#[test]
fn the_grid_covers_across_the_main_and_extra_regions() {
    let selection = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(5, "city", 5, "city")],
        mode: GridSelectionMode::Cell,
    };
    assert!(grid_selection_contains(&selection, &coord(0, "id"), ORDER));
    assert!(grid_selection_contains(&selection, &coord(1, "name"), ORDER));
    assert!(grid_selection_contains(&selection, &coord(5, "city"), ORDER));
    assert!(!grid_selection_contains(&selection, &coord(5, "id"), ORDER));
}

#[test]
fn the_cell_count_counts_overlaps_twice() {
    let selection = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "id", 1, "name")],
        mode: GridSelectionMode::Cell,
    };
    // Two identical 2x2 rectangles: 8, not 4 (the render/summary metric).
    assert_eq!(grid_selection_cell_count(&selection, ORDER), 8);
}

#[test]
fn the_column_coverage_deduplicates() {
    let selection = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "name", 5, "city")],
        mode: GridSelectionMode::Cell,
    };
    // Main covers id+name, extra covers name+age+city: 4 distinct.
    assert_eq!(columns_covered_by_selection(&selection, ORDER), 4);
}

#[test]
fn row_coverage_never_reads_the_column_order() {
    let rows: HashSet<usize> = [7].into_iter().collect();
    let row_mode = GridSelection {
        mode: GridSelectionMode::Row,
        anchor: Some(coord(0, "id")), // geometry present is tolerated, not read
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "id", 0, "id")],
    };
    let state = GridSelectionState {
        selection: &row_mode,
        selected_rows: &rows,
    };
    assert_eq!(rows_covered_by_selection(&state), rows, "row mode reports the set verbatim");

    let cell_mode = selection_from_anchor_focus(
        coord(2, "age"),
        coord(4, "id"),
        GridSelectionMode::Cell,
        vec![range(0, "id", 0, "id")],
    );
    let empty = HashSet::new();
    let state = GridSelectionState {
        selection: &cell_mode,
        selected_rows: &empty,
    };
    assert_eq!(
        rows_covered_by_selection(&state),
        [0, 2, 3, 4].into_iter().collect::<HashSet<_>>(),
        "rows 2..=4 plus the extra 0, unordered, de-duplicated"
    );

    let quiet = GridSelection::empty();
    let state = GridSelectionState {
        selection: &quiet,
        selected_rows: &empty,
    };
    assert!(rows_covered_by_selection(&state).is_empty());
}

#[test]
fn the_range_expansion_walks_row_major() {
    let coords = cell_coords_in_range(&range(1, "age", 2, "name"), ORDER);
    let expected: Vec<CellCoord> = vec![
        coord(1, "name"),
        coord(1, "age"),
        coord(2, "name"),
        coord(2, "age"),
    ];
    assert_eq!(coords, expected);
    assert!(cell_coords_in_range(&range(0, "ghost", 1, "id"), ORDER).is_empty());
}

#[test]
fn structural_equality_treats_extra_ranges_as_an_unordered_set() {
    let a = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "city", 0, "city"), range(2, "id", 2, "id")],
        mode: GridSelectionMode::Cell,
    };
    let b = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(2, "id", 2, "id"), range(0, "city", 0, "city")],
        mode: GridSelectionMode::Cell,
    };
    assert!(grid_selection_equals(&a, &b, ORDER), "order of extras is irrelevant");

    let c = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "city", 0, "city")],
        mode: GridSelectionMode::Cell,
    };
    assert!(!grid_selection_equals(&a, &c, ORDER), "different extra counts");
    let d = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "city", 0, "city"), range(2, "age", 2, "age")],
        mode: GridSelectionMode::Cell,
    };
    assert!(!grid_selection_equals(&a, &d, ORDER), "a different extra region");
}

#[test]
fn the_focus_move_clamps_and_reports_nowhere_to_go() {
    let top_left = coord(0, "id");
    assert!(move_cell_focus(&top_left, -1, -1, 10, ORDER).is_none(), "nowhere to go");
    assert!(move_cell_focus(&top_left, 0, 0, 10, ORDER).is_none());
    assert_eq!(
        move_cell_focus(&top_left, 3, 2, 10, ORDER).unwrap(),
        coord(3, "age")
    );
    let bottom_right = coord(9, "city");
    assert!(move_cell_focus(&bottom_right, 1, 1, 10, ORDER).is_none());
    assert_eq!(
        move_cell_focus(&bottom_right, -1, -1, 10, ORDER).unwrap(),
        coord(8, "age")
    );
    assert!(move_cell_focus(&bottom_right, 1, 1, 0, ORDER).is_none(), "empty grid");
    assert!(move_cell_focus(&coord(0, "ghost"), 0, 1, 10, ORDER).is_none(), "unknown column");
}

#[test]
fn building_from_corners_drops_geometry_in_row_mode() {
    let built = selection_from_anchor_focus(
        coord(0, "id"),
        coord(1, "name"),
        GridSelectionMode::Row,
        vec![range(0, "id", 0, "id")],
    );
    assert_eq!(
        built,
        GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        }
    );
    let built = selection_from_anchor_focus(
        coord(0, "id"),
        coord(1, "name"),
        GridSelectionMode::Cell,
        vec![range(0, "id", 0, "id")],
    );
    assert_eq!(built.anchor, Some(coord(0, "id")));
    assert_eq!(built.extra_ranges.len(), 1);
}

#[test]
fn the_toggle_appends_drops_and_leaves_the_main_region_alone() {
    let selection = selection_from_anchor_focus(
        coord(0, "id"),
        coord(1, "name"),
        GridSelectionMode::Cell,
        Vec::new(),
    );
    // Inside the main rectangle: a no-op.
    assert_eq!(toggle_extra_range(&selection, &coord(1, "id"), ORDER), selection);
    // Outside: appended as a single-cell region.
    let appended = toggle_extra_range(&selection, &coord(5, "city"), ORDER);
    assert_eq!(appended.extra_ranges.len(), 1);
    assert_eq!(appended.extra_ranges[0], range(5, "city", 5, "city"));
    // Toggling the same cell again drops it.
    let dropped = toggle_extra_range(&appended, &coord(5, "city"), ORDER);
    assert_eq!(dropped, selection);
    // An unknown column leaves the selection untouched.
    assert_eq!(toggle_extra_range(&selection, &coord(5, "ghost"), ORDER), selection);
    // A bare selection (no anchor) becomes a single-cell selection.
    let bare = toggle_extra_range(&GridSelection::empty(), &coord(2, "age"), ORDER);
    assert_eq!(bare.anchor, Some(coord(2, "age")));
    assert_eq!(bare.focus, Some(coord(2, "age")));
    assert_eq!(bare.extra_ranges.len(), 0);
}

#[test]
fn reconcile_converges_after_rows_or_columns_change() {
    let selection = GridSelection {
        anchor: Some(coord(8, "id")),
        focus: Some(coord(9, "name")),
        extra_ranges: vec![range(0, "id", 0, "id"), range(0, "city", 1, "city")],
        mode: GridSelectionMode::Cell,
    };
    // The page shrank to 4 rows: the main corners are invalid, everything clears.
    assert_eq!(
        reconcile_grid_selection(&selection, 4, ORDER),
        GridSelection::empty()
    );

    // The main corners are valid; the extra out-of-range rows drop, the
    // in-range extra stays.
    let valid = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "id", 0, "id"), range(0, "city", 9, "city")],
        mode: GridSelectionMode::Cell,
    };
    let reconciled = reconcile_grid_selection(&valid, 4, ORDER);
    assert_eq!(reconciled.anchor, Some(coord(0, "id")));
    assert_eq!(reconciled.extra_ranges, vec![range(0, "id", 0, "id")]);

    // Everything valid: the selection is returned unchanged.
    let clean = GridSelection {
        anchor: Some(coord(0, "id")),
        focus: Some(coord(1, "name")),
        extra_ranges: vec![range(0, "id", 0, "id")],
        mode: GridSelectionMode::Cell,
    };
    assert_eq!(reconcile_grid_selection(&clean, 4, ORDER), clean);

    // A hidden column clears the main selection through the corner check.
    let hidden = GridSelection {
        anchor: Some(coord(0, "city")),
        focus: Some(coord(1, "name")),
        extra_ranges: Vec::new(),
        mode: GridSelectionMode::Cell,
    };
    let trimmed: &[&str] = &["id", "name", "age"];
    assert_eq!(
        reconcile_grid_selection(&hidden, 4, trimmed),
        GridSelection::empty()
    );

    // Row mode strips any stray geometry.
    let stray = GridSelection {
        mode: GridSelectionMode::Row,
        anchor: Some(coord(0, "id")),
        focus: Some(coord(0, "id")),
        extra_ranges: vec![range(0, "id", 0, "id")],
    };
    assert_eq!(
        reconcile_grid_selection(&stray, 4, ORDER),
        GridSelection {
            mode: GridSelectionMode::Row,
            ..GridSelection::empty()
        }
    );
    let quiet_row = GridSelection {
        mode: GridSelectionMode::Row,
        ..GridSelection::empty()
    };
    assert_eq!(reconcile_grid_selection(&quiet_row, 4, ORDER), quiet_row);
}

#[test]
fn coord_equality_covers_the_null_shapes() {
    assert!(cell_coord_equals(None, None));
    assert!(!cell_coord_equals(None, Some(&coord(0, "id"))));
    assert!(!cell_coord_equals(Some(&coord(0, "id")), None));
    assert!(cell_coord_equals(Some(&coord(0, "id")), Some(&coord(0, "id"))));
    assert!(!cell_coord_equals(Some(&coord(0, "id")), Some(&coord(1, "id"))));
}

#[test]
fn the_anchor_focus_range_needs_both_corners() {
    let selection = selection_from_anchor_focus(
        coord(0, "id"),
        coord(2, "city"),
        GridSelectionMode::Cell,
        Vec::new(),
    );
    assert_eq!(
        normalize_selection_range(&selection, ORDER).unwrap(),
        range(0, "id", 2, "city")
    );
    let quiet = GridSelection {
        focus: Some(coord(0, "id")),
        ..GridSelection::empty()
    };
    assert!(normalize_selection_range(&quiet, ORDER).is_none());
}
