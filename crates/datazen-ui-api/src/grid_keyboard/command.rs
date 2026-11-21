use crate::grid_keyboard::selection::{
    cell_coord_equals, clamp_cell_coord, coord_at, move_cell_focus, selection_from_anchor_focus,
};
use crate::grid_keyboard::types::{
    CellCoord, GridEdge, GridKeyCommand, GridKeyIntent, GridKeyboardContext, GridPageDirection,
    GridSelection, GridSelectionMode,
};

pub fn resolve_keyboard_cursor(
    selection: &GridSelection,
    last_selected_index: Option<usize>,
    column_order: &[String],
) -> Option<CellCoord> {
    match selection.mode {
        GridSelectionMode::Cell => selection.focus.clone(),
        GridSelectionMode::Row => {
            let index = last_selected_index?;
            column_order.first().map(|name| CellCoord::new(index, name))
        }
        GridSelectionMode::None => None,
    }
}

pub fn resolve_entry_cursor(
    remembered: Option<&CellCoord>,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    if row_count == 0 || column_order.is_empty() {
        return None;
    }
    if let Some(coord) = remembered {
        if let Some(clamped) = clamp_cell_coord(coord, row_count, column_order) {
            return Some(clamped);
        }
    }
    coord_at(0, 0, column_order)
}

pub fn resolve_page_delta(viewport_row_count: usize) -> i64 {
    (viewport_row_count as i64 - 1).max(1)
}

pub fn grid_cursor_equals(a: Option<&CellCoord>, b: Option<&CellCoord>) -> bool {
    cell_coord_equals(a, b)
}

pub fn clamp_grid_cursor(
    cursor: Option<&CellCoord>,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    cursor.and_then(|coord| clamp_cell_coord(coord, row_count, column_order))
}

fn current_cursor(context: &GridKeyboardContext, selection: &GridSelection) -> Option<CellCoord> {
    context
        .cursor
        .clone()
        .or_else(|| match context.mode {
            GridSelectionMode::Cell => selection.focus.clone(),
            _ => None,
        })
}

fn enter_cell_mode(context: &GridKeyboardContext) -> GridKeyCommand {
    match resolve_entry_cursor(context.cursor.as_ref(), context.row_count, &context.column_order) {
        None => GridKeyCommand::Consume,
        Some(entry) => GridKeyCommand::SetSelection {
            selection: selection_from_anchor_focus(
                entry.clone(),
                entry,
                GridSelectionMode::Cell,
                Vec::new(),
            ),
        },
    }
}

fn build_cell_selection(
    next: CellCoord,
    context: &GridKeyboardContext,
    selection: &GridSelection,
) -> GridKeyCommand {
    if context.extend {
        let anchor = selection.anchor.clone().unwrap_or_else(|| next.clone());
        GridKeyCommand::SetSelection {
            selection: selection_from_anchor_focus(
                anchor,
                next,
                GridSelectionMode::Cell,
                selection.extra_ranges.clone(),
            ),
        }
    } else {
        GridKeyCommand::SetSelection {
            selection: selection_from_anchor_focus(
                next.clone(),
                next,
                GridSelectionMode::Cell,
                Vec::new(),
            ),
        }
    }
}

fn resolve_begin_edit(
    context: &GridKeyboardContext,
    selection: &GridSelection,
) -> GridKeyCommand {
    if context.mode == GridSelectionMode::None {
        return GridKeyCommand::Ignored;
    }
    match current_cursor(context, selection) {
        None => GridKeyCommand::Ignored,
        Some(coord) => GridKeyCommand::BeginEdit { coord },
    }
}

fn edge_coord(
    edge: GridEdge,
    cursor: &CellCoord,
    context: &GridKeyboardContext,
) -> Option<CellCoord> {
    match edge {
        GridEdge::RowStart => coord_at(cursor.row_index, 0, &context.column_order),
        GridEdge::RowEnd => {
            coord_at(cursor.row_index, context.column_order.len().saturating_sub(1), &context.column_order)
        }
        GridEdge::GridStart => coord_at(0, 0, &context.column_order),
        GridEdge::GridEnd => coord_at(
            context.row_count.saturating_sub(1),
            context.column_order.len().saturating_sub(1),
            &context.column_order,
        ),
    }
}

fn cell_movement_target(
    movement: &GridKeyIntent,
    context: &GridKeyboardContext,
    cursor: &CellCoord,
) -> Option<CellCoord> {
    match movement {
        GridKeyIntent::Move { delta_row, delta_column } => {
            move_cell_focus(cursor, *delta_row, *delta_column, context.row_count, &context.column_order)
        }
        GridKeyIntent::MovePage { direction } => {
            let step = resolve_page_delta(context.viewport_row_count)
                * if *direction == GridPageDirection::Down { 1 } else { -1 };
            move_cell_focus(cursor, step, 0, context.row_count, &context.column_order)
        }
        GridKeyIntent::MoveEdge { edge } => edge_coord(*edge, cursor, context),
        _ => None,
    }
}

fn row_movement_target(
    movement: &GridKeyIntent,
    context: &GridKeyboardContext,
    cursor: &CellCoord,
) -> Option<usize> {
    if matches!(movement, GridKeyIntent::MoveEdge { .. }) {
        return None;
    }
    if let GridKeyIntent::Move { delta_column, .. } = movement {
        if *delta_column != 0 {
            return None;
        }
    }
    let delta = match movement {
        GridKeyIntent::MovePage { direction } => {
            resolve_page_delta(context.viewport_row_count)
                * if *direction == GridPageDirection::Down { 1 } else { -1 }
        }
        GridKeyIntent::Move { delta_row, .. } => *delta_row,
        _ => return None,
    };
    move_cell_focus(cursor, delta, 0, context.row_count, &context.column_order)
        .map(|next| next.row_index)
}

fn resolve_movement(
    movement: &GridKeyIntent,
    context: &GridKeyboardContext,
    selection: &GridSelection,
) -> GridKeyCommand {
    if context.row_count == 0 || context.column_order.is_empty() {
        return GridKeyCommand::Consume;
    }
    if context.mode == GridSelectionMode::None {
        return enter_cell_mode(context);
    }
    let Some(cursor) = current_cursor(context, selection) else {
        return GridKeyCommand::Consume;
    };
    if context.mode == GridSelectionMode::Row {
        return match row_movement_target(movement, context, &cursor) {
            None => GridKeyCommand::Consume,
            Some(row) => GridKeyCommand::RowSelect {
                row_index: row,
                multi: false,
                range: context.extend,
            },
        };
    }
    let Some(target) = cell_movement_target(movement, context, &cursor) else {
        return GridKeyCommand::Consume;
    };
    if grid_cursor_equals(Some(&target), Some(&cursor)) {
        return GridKeyCommand::Consume;
    }
    build_cell_selection(target, context, selection)
}

pub fn resolve_grid_key_command(
    intent: &GridKeyIntent,
    context: &GridKeyboardContext,
    selection: &GridSelection,
) -> GridKeyCommand {
    match intent {
        GridKeyIntent::Pass => GridKeyCommand::Pass,
        GridKeyIntent::Ignored => GridKeyCommand::Ignored,
        GridKeyIntent::LeaveGrid => GridKeyCommand::Consume,
        GridKeyIntent::SelectAllRows => GridKeyCommand::SelectAllRows,
        GridKeyIntent::BeginEdit => resolve_begin_edit(context, selection),
        GridKeyIntent::Move { .. }
        | GridKeyIntent::MovePage { .. }
        | GridKeyIntent::MoveEdge { .. } => resolve_movement(intent, context, selection),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_keyboard::types::CellRange;

    fn order() -> Vec<String> {
        vec!["id".to_owned(), "name".to_owned()]
    }

    fn context() -> GridKeyboardContext {
        GridKeyboardContext {
            column_order: order(),
            row_count: 3,
            mode: GridSelectionMode::Cell,
            cursor: Some(CellCoord::new(1, "name")),
            viewport_row_count: 4,
            editing: false,
            extend: false,
        }
    }

    fn cell_selection() -> GridSelection {
        selection_from_anchor_focus(
            CellCoord::new(1, "name"),
            CellCoord::new(1, "name"),
            GridSelectionMode::Cell,
            Vec::new(),
        )
    }

    #[test]
    fn keyboard_cursor_follows_the_mode() {
        let mut selection = cell_selection();
        assert_eq!(resolve_keyboard_cursor(&selection, Some(2), &order()), Some(CellCoord::new(1, "name")));
        selection.mode = GridSelectionMode::Row;
        assert_eq!(resolve_keyboard_cursor(&selection, Some(2), &order()), Some(CellCoord::new(2, "id")));
        assert_eq!(resolve_keyboard_cursor(&selection, None, &order()), None);
        selection.mode = GridSelectionMode::None;
        assert_eq!(resolve_keyboard_cursor(&selection, Some(2), &order()), None);
    }

    #[test]
    fn entry_cursor_uses_remembered_cell_or_the_first_cell() {
        assert_eq!(
            resolve_entry_cursor(Some(&CellCoord::new(9, "name")), 3, &order()),
            Some(CellCoord::new(2, "name"))
        );
        assert_eq!(resolve_entry_cursor(Some(&CellCoord::new(0, "ghost")), 3, &order()), Some(CellCoord::new(0, "id")));
        assert_eq!(resolve_entry_cursor(None, 3, &order()), Some(CellCoord::new(0, "id")));
        assert_eq!(resolve_entry_cursor(None, 0, &order()), None);
        assert_eq!(resolve_entry_cursor(None, 3, &[]), None);
    }

    #[test]
    fn page_delta_is_a_screen_minus_one_row() {
        assert_eq!(resolve_page_delta(8), 7);
        assert_eq!(resolve_page_delta(1), 1);
        assert_eq!(resolve_page_delta(0), 1);
    }

    #[test]
    fn cursor_equality_and_clamping_delegate_to_the_projection() {
        assert!(grid_cursor_equals(None, None));
        assert!(grid_cursor_equals(Some(&CellCoord::new(1, "id")), Some(&CellCoord::new(1, "id"))));
        assert!(!grid_cursor_equals(Some(&CellCoord::new(1, "id")), Some(&CellCoord::new(2, "id"))));
        assert_eq!(clamp_grid_cursor(Some(&CellCoord::new(9, "name")), 3, &order()), Some(CellCoord::new(2, "name")));
        assert_eq!(clamp_grid_cursor(None, 3, &order()), None);
    }

    #[test]
    fn pass_ignored_leave_and_select_all_are_direct() {
        let ctx = context();
        let selection = cell_selection();
        assert_eq!(resolve_grid_key_command(&GridKeyIntent::Pass, &ctx, &selection), GridKeyCommand::Pass);
        assert_eq!(resolve_grid_key_command(&GridKeyIntent::Ignored, &ctx, &selection), GridKeyCommand::Ignored);
        assert_eq!(resolve_grid_key_command(&GridKeyIntent::LeaveGrid, &ctx, &selection), GridKeyCommand::Consume);
        assert_eq!(resolve_grid_key_command(&GridKeyIntent::SelectAllRows, &ctx, &selection), GridKeyCommand::SelectAllRows);
    }

    #[test]
    fn begin_edit_uses_the_current_cursor() {
        let ctx = context();
        assert_eq!(
            resolve_grid_key_command(&GridKeyIntent::BeginEdit, &ctx, &cell_selection()),
            GridKeyCommand::BeginEdit { coord: CellCoord::new(1, "name") }
        );
        let ctx = GridKeyboardContext { mode: GridSelectionMode::None, ..ctx };
        assert_eq!(resolve_grid_key_command(&GridKeyIntent::BeginEdit, &ctx, &cell_selection()), GridKeyCommand::Ignored);
    }

    #[test]
    fn an_arrow_move_collapses_the_region_without_shift() {
        let mut selection = cell_selection();
        selection.extra_ranges = vec![CellRange { start: CellCoord::new(0, "id"), end: CellCoord::new(0, "id") }];
        let intent = GridKeyIntent::Move { delta_row: 1, delta_column: 0 };
        let command = resolve_grid_key_command(&intent, &context(), &selection);
        let GridKeyCommand::SetSelection { selection: next } = command else { panic!("expected a selection") };
        assert_eq!(next.focus, Some(CellCoord::new(2, "name")));
        assert!(next.extra_ranges.is_empty());
    }

    #[test]
    fn shift_move_extends_from_the_anchor_and_keeps_extra_ranges() {
        let mut ctx = context();
        ctx.extend = true;
        let mut selection = cell_selection();
        selection.anchor = Some(CellCoord::new(0, "id"));
        let extra = vec![CellRange { start: CellCoord::new(0, "id"), end: CellCoord::new(0, "id") }];
        selection.extra_ranges = extra.clone();
        let intent = GridKeyIntent::Move { delta_row: 1, delta_column: 0 };
        let command = resolve_grid_key_command(&intent, &ctx, &selection);
        let GridKeyCommand::SetSelection { selection: next } = command else { panic!("expected a selection") };
        assert_eq!(next.anchor, Some(CellCoord::new(0, "id")));
        assert_eq!(next.focus, Some(CellCoord::new(2, "name")));
        assert_eq!(next.extra_ranges, extra);
    }

    #[test]
    fn a_border_move_consumes_without_changing_the_selection() {
        let ctx = GridKeyboardContext { cursor: Some(CellCoord::new(0, "id")), ..context() };
        let intent = GridKeyIntent::Move { delta_row: -1, delta_column: -1 };
        assert_eq!(resolve_grid_key_command(&intent, &ctx, &cell_selection()), GridKeyCommand::Consume);
    }

    #[test]
    fn page_and_edge_moves_land_where_the_projection_says() {
        let ctx = context();
        let down = GridKeyIntent::MovePage { direction: GridPageDirection::Down };
        let GridKeyCommand::SetSelection { selection: next } = resolve_grid_key_command(&down, &ctx, &cell_selection()) else { panic!("expected a selection") };
        assert_eq!(next.focus, Some(CellCoord::new(2, "name")));
        let edge = GridKeyIntent::MoveEdge { edge: GridEdge::RowStart };
        let GridKeyCommand::SetSelection { selection: next } = resolve_grid_key_command(&edge, &ctx, &cell_selection()) else { panic!("expected a selection") };
        assert_eq!(next.focus, Some(CellCoord::new(1, "id")));
    }

    #[test]
    fn row_mode_moves_rows_and_rejects_edges_and_horizontal_arrows() {
        let mut ctx = context();
        ctx.mode = GridSelectionMode::Row;
        ctx.cursor = Some(CellCoord::new(1, "id"));
        let down = GridKeyIntent::Move { delta_row: 1, delta_column: 0 };
        assert_eq!(
            resolve_grid_key_command(&down, &ctx, &GridSelection::empty()),
            GridKeyCommand::RowSelect { row_index: 2, multi: false, range: false }
        );
        let horizontal = GridKeyIntent::Move { delta_row: 0, delta_column: 1 };
        assert_eq!(resolve_grid_key_command(&horizontal, &ctx, &GridSelection::empty()), GridKeyCommand::Consume);
        let edge = GridKeyIntent::MoveEdge { edge: GridEdge::RowStart };
        assert_eq!(resolve_grid_key_command(&edge, &ctx, &GridSelection::empty()), GridKeyCommand::Consume);
    }

    #[test]
    fn none_mode_enter_enters_cell_mode_with_a_single_cell() {
        let ctx = GridKeyboardContext {
            mode: GridSelectionMode::None,
            cursor: Some(CellCoord::new(2, "name")),
            ..context()
        };
        let intent = GridKeyIntent::Move { delta_row: 1, delta_column: 0 };
        let GridKeyCommand::SetSelection { selection } = resolve_grid_key_command(&intent, &ctx, &GridSelection::empty()) else { panic!("expected a selection") };
        assert_eq!(selection.anchor, Some(CellCoord::new(2, "name")));
        assert_eq!(selection.focus, Some(CellCoord::new(2, "name")));
        assert_eq!(selection.mode, GridSelectionMode::Cell);
    }
}
