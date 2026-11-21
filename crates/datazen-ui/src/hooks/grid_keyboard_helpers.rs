//! Seam module for the grid keyboard family, carried for the hooks that
//! need it until the owning ledger tasks land (the T041/T384 mirror
//! convention):
//!
//! * `lib/gridKeyboard.ts` (segment-2 **T527**): [`GridKeyEventLike`],
//!   [`GridKeyIntent`], [`GridKeyboardContext`], [`GridKeyCommand`],
//!   [`to_grid_key_event_like`], [`is_ime_composing_key`],
//!   [`read_grid_key_intent`], [`resolve_keyboard_cursor`],
//!   [`resolve_entry_cursor`], [`resolve_page_delta`],
//!   [`grid_cursor_equals`], [`clamp_grid_cursor`],
//!   [`resolve_grid_key_command`] — branch for branch.
//! * `lib/gridEventGuards.ts` (segment-2 **T525**):
//!   [`is_composing_event`]. [`is_editable_target`] is DOM-bound
//!   (`closest('input, textarea, …')`); the port models it as the
//!   event projection's [`GridKeyDownEvent::target_editable`] flag —
//!   the host knows whether the key came from an editor surface.
//! * `stores/tableData/gridSelection.ts` (segment-3 **T210**) — the five
//!   pure functions the keyboard layer delegates every coordinate to:
//!   [`column_index_from_coord`], [`cell_coord_equals`], [`coord_at`],
//!   [`clamp_cell_coord`], [`move_cell_focus`],
//!   [`selection_from_anchor_focus`]. The types themselves
//!   (`CellCoord` / `CellRange` / `GridSelection`) are the T041 mirrors
//!   in `table_data_connection_state.rs` and are reused, not re-mirrored.
//!
//! When T527 / T525 / T210 land, this module's items move to their own
//! files and the `pub(crate) use` re-export here keeps the consumers'
//! paths valid during the dedupe.

use crate::table_data_connection_state::{
    CellCoord, CellRange, GridSelection, GridSelectionMode,
};

/// `GridKeyEventLike`: the structural projection of a key event — only
/// the fields the decision needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridKeyEventLike {
    /// Normalised to lower case, like `useKeyboardShortcuts` does.
    pub(crate) key: String,
    pub(crate) meta_key: bool,
    pub(crate) ctrl_key: bool,
    pub(crate) shift_key: bool,
    pub(crate) alt_key: bool,
    /// True while an IME composition is active.
    pub(crate) is_composing: bool,
}

/// The host-side key-down projection: what `useGridKeyboardNav`'s
/// handler receives (React's `KeyboardEvent` adapted, the T341
/// `ShortcutEvent` convention — no DOM handle crosses the seam).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridKeyDownEvent {
    /// `e.key` verbatim (lower-cased by [`to_grid_key_event_like`]).
    pub(crate) key: String,
    pub(crate) meta_key: bool,
    pub(crate) ctrl_key: bool,
    pub(crate) shift_key: bool,
    pub(crate) alt_key: bool,
    /// `nativeEvent.isComposing`.
    pub(crate) native_is_composing: bool,
    /// `nativeEvent.keyCode` (the legacy IME signal is 229).
    pub(crate) native_key_code: i32,
    /// `isEditableTarget(e.target)`: the event came from an input, a
    /// textarea, a select, a contenteditable, or an open cell editor
    /// (the DOM `closest()` check is the host's — it owns the tree).
    pub(crate) target_editable: bool,
}

/// `toGridKeyEventLike`: flatten the event into the projection.
pub(crate) fn to_grid_key_event_like(e: &GridKeyDownEvent) -> GridKeyEventLike {
    GridKeyEventLike {
        key: e.key.to_lowercase(),
        meta_key: e.meta_key,
        ctrl_key: e.ctrl_key,
        shift_key: e.shift_key,
        alt_key: e.alt_key,
        // `keyCode === 229` is the legacy IME signal some engines still
        // send (01 §3.10).
        is_composing: e.native_is_composing || e.native_key_code == 229,
    }
}

/// `isComposingEvent` (`gridEventGuards.ts`): both signals are checked —
/// browsers disagree (Chrome/Safari set `isComposing`, older engines
/// only send `keyCode === 229`).
pub(crate) fn is_composing_event(e: &GridKeyDownEvent) -> bool {
    e.native_is_composing || e.native_key_code == 229
}

/// `isImeComposingKey`: a composition owns every key; Chromium reports
/// the combination as `process`.
pub(crate) fn is_ime_composing_key(e: &GridKeyEventLike) -> bool {
    e.is_composing || e.key == "process"
}

/// `Cmd` on macOS, `Ctrl` elsewhere. The pure layer cannot ask the
/// platform.
fn is_mod_key(e: &GridKeyEventLike) -> bool {
    e.meta_key || e.ctrl_key
}

/// The four arrow keys and the cell they move to.
fn arrow_move(key: &str) -> Option<(i64, i64)> {
    match key {
        "arrowup" => Some((-1, 0)),
        "arrowdown" => Some((1, 0)),
        "arrowleft" => Some((0, -1)),
        "arrowright" => Some((0, 1)),
        _ => None,
    }
}

/// `GridKeyIntent`: key → intent. The order of the checks in
/// [`read_grid_key_intent`] is the priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GridKeyIntent {
    /// `{ kind: 'move'; deltaRow; deltaColumn }`.
    Move { delta_row: i64, delta_column: i64 },
    /// `{ kind: 'move-page'; direction }`.
    MovePage { down: bool },
    /// `{ kind: 'move-edge'; edge }` — `row-start` / `row-end` /
    /// `grid-start` / `grid-end`.
    MoveEdge { edge: GridEdge },
    SelectAllRows,
    BeginEdit,
    /// `Tab` leaves the grid.
    LeaveGrid,
    /// The grid claims the key name but has nothing to do here.
    Ignored,
    /// Not the grid's key.
    Pass,
}

/// `move-edge`'s four edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GridEdge {
    RowStart,
    RowEnd,
    GridStart,
    GridEnd,
}

/// `GridKeyboardContext`.
#[derive(Clone, Debug)]
pub(crate) struct GridKeyboardContext<'a> {
    /// Displayed column names in render order; the arrows move between
    /// them only.
    pub(crate) column_order: &'a [String],
    pub(crate) row_count: usize,
    pub(crate) mode: GridSelectionMode,
    /// The current cursor; `None` in `none` mode when there is nothing
    /// to resume.
    pub(crate) cursor: Option<CellCoord>,
    /// Visible rows of the viewport (≥ 1); the step size of
    /// PageUp/PageDown.
    pub(crate) viewport_row_count: usize,
    /// An editor is open (`editingCell !== null`).
    pub(crate) editing: bool,
    /// Shift was held: the command layer extends the region instead of
    /// moving it.
    pub(crate) extend: bool,
}

/// `GridKeyCommand`: what the key means for the grid. The host only
/// dispatches it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GridKeyCommand {
    /// `{ kind: 'set-selection'; selection }`.
    SetSelection(GridSelection),
    /// `{ kind: 'row-select'; rowIndex; opts }`.
    RowSelect { row_index: usize, multi: bool, range: bool },
    SelectAllRows,
    /// `{ kind: 'begin-edit'; coord }`.
    BeginEdit { coord: CellCoord },
    /// Claimed and prevented, but no state changes (border no-op,
    /// `Tab`).
    Consume,
    /// Claimed, but nothing to do here; the browser default stays
    /// untouched.
    Ignored,
    /// Not the grid's key: never prevent, never stop propagation.
    Pass,
}

/// `readGridKeyIntent`: key → intent. Pure mapping, no state access
/// beyond the context. The order of the checks **is** the priority
/// table (02 §4.0): ① IME composition ② open editor ③ `Escape` (01's)
/// ④ clipboard/undo (03/06) ⑤ `Alt` ⑥ the key table ⑦ everything else.
pub(crate) fn read_grid_key_intent(e: &GridKeyEventLike, ctx: &GridKeyboardContext) -> GridKeyIntent {
    if is_ime_composing_key(e) {
        return GridKeyIntent::Pass;
    }
    if ctx.editing {
        return GridKeyIntent::Pass;
    }
    if e.key == "escape" {
        return GridKeyIntent::Pass;
    }
    if is_mod_key(e) && matches!(e.key.as_str(), "c" | "x" | "v" | "z") {
        return GridKeyIntent::Pass;
    }
    if e.alt_key {
        return GridKeyIntent::Pass;
    }

    let mod_key = is_mod_key(e);
    // `Cmd/Ctrl+Left/Right` is the row edge: a laptop keyboard has no
    // `Home`.
    if mod_key && (e.key == "arrowleft" || e.key == "arrowright") {
        return GridKeyIntent::MoveEdge {
            edge: if e.key == "arrowleft" { GridEdge::RowStart } else { GridEdge::RowEnd },
        };
    }
    if let Some((delta_row, delta_column)) = arrow_move(&e.key) {
        return GridKeyIntent::Move { delta_row, delta_column };
    }
    match e.key.as_str() {
        "home" => {
            return GridKeyIntent::MoveEdge {
                edge: if mod_key { GridEdge::GridStart } else { GridEdge::RowStart },
            }
        }
        "end" => {
            return GridKeyIntent::MoveEdge {
                edge: if mod_key { GridEdge::GridEnd } else { GridEdge::RowEnd },
            }
        }
        "pageup" => return GridKeyIntent::MovePage { down: false },
        "pagedown" => return GridKeyIntent::MovePage { down: true },
        // `Tab` leaves the grid; the row-number slot must not become the
        // target.
        "tab" => return GridKeyIntent::LeaveGrid,
        "enter" | "f2" => {
            // `Cmd+Enter` executes the query (a global shortcut) — not
            // the grid's.
            if mod_key {
                return GridKeyIntent::Pass;
            }
            // Without a selection there is no cell to edit: the grid
            // claims the key name but does nothing (02 §4.1).
            return if ctx.mode == GridSelectionMode::None {
                GridKeyIntent::Ignored
            } else {
                GridKeyIntent::BeginEdit
            };
        }
        "a" => {
            return if mod_key { GridKeyIntent::SelectAllRows } else { GridKeyIntent::Pass }
        }
        // Printable characters, `Space`, `Delete`/`Backspace`: not the
        // grid's.
        _ => GridKeyIntent::Pass,
    }
}

/// `resolveKeyboardCursor`: the cursor of the current selection — the
/// focus corner in cell mode, the row anchor of the row selection in
/// row mode, nothing in `none` mode.
pub(crate) fn resolve_keyboard_cursor(
    selection: &GridSelection,
    last_selected_index: Option<usize>,
    column_order: &[String],
) -> Option<CellCoord> {
    match selection.mode {
        GridSelectionMode::Cell => selection.focus.clone(),
        GridSelectionMode::Row => {
            let last = last_selected_index?;
            let first = column_order.first()?;
            Some(CellCoord { row_index: last, column_name: first.clone() })
        }
        GridSelectionMode::None => None,
    }
}

/// `resolveEntryCursor`: where the grid is entered from `none` mode —
/// the remembered cursor when it is still displayable, the first cell
/// otherwise.
pub(crate) fn resolve_entry_cursor(
    remembered: Option<&CellCoord>,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    if row_count == 0 || column_order.is_empty() {
        return None;
    }
    if let Some(remembered) = remembered {
        if let Some(clamped) = clamp_cell_coord(remembered, row_count, column_order) {
            return Some(clamped);
        }
    }
    coord_at(0, 0, column_order)
}

/// `resolvePageDelta`: the step of PageUp/PageDown — one screen minus
/// one row, so the origin stays visible.
pub(crate) fn resolve_page_delta(viewport_row_count: usize) -> i64 {
    (viewport_row_count as i64 - 1).max(1)
}

/// `gridCursorEquals`: coordinate comparison with 01's semantics.
pub(crate) fn grid_cursor_equals(a: Option<&CellCoord>, b: Option<&CellCoord>) -> bool {
    cell_coord_equals(a, b)
}

/// `clampGridCursor`: converge a cursor after the rows or the displayed
/// columns changed — a hidden column or an empty grid has no legal
/// coordinate at all, hence `None`.
pub(crate) fn clamp_grid_cursor(
    cursor: Option<&CellCoord>,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    clamp_cell_coord(cursor?, row_count, column_order)
}

/// `currentCursor`: the cursor the command layer works with, in
/// whichever mode the grid is in.
fn current_cursor(ctx: &GridKeyboardContext, selection: &GridSelection) -> Option<CellCoord> {
    ctx.cursor
        .clone()
        .or_else(|| (ctx.mode == GridSelectionMode::Cell).then(|| selection.focus.clone()).flatten())
}

/// `enterCellMode`: entering the grid from `none` mode — the remembered
/// cursor, or the first cell. The region is a single cell — an arrow
/// key never invents a selection of its own in `none` mode.
fn enter_cell_mode(ctx: &GridKeyboardContext) -> GridKeyCommand {
    let entry = resolve_entry_cursor(ctx.cursor.as_ref(), ctx.row_count, ctx.column_order);
    match entry {
        None => GridKeyCommand::Consume,
        Some(entry) => GridKeyCommand::SetSelection(
            selection_from_anchor_focus(&entry, &entry, GridSelectionMode::Cell, &[]),
        ),
    }
}

/// `buildCellSelection`: the selection a cell mode move produces (01's
/// `selectionFromAnchorFocus`, always).
fn build_cell_selection(
    next: &CellCoord,
    ctx: &GridKeyboardContext,
    selection: &GridSelection,
) -> GridKeyCommand {
    if ctx.extend {
        // The anchor stays where it is and the extra ranges survive
        // (02 §4.7). A missing anchor is an incomplete state: the new
        // cursor becomes a single-cell region rather than a lost event
        // (02 §3.1).
        let anchor = selection.anchor.clone().unwrap_or_else(|| next.clone());
        return GridKeyCommand::SetSelection(selection_from_anchor_focus(
            &anchor,
            next,
            GridSelectionMode::Cell,
            &selection.extra_ranges,
        ));
    }
    // An unmodified move collapses the main region and drops the extra
    // ranges — the same thing a plain click does (02 §4.7).
    GridKeyCommand::SetSelection(selection_from_anchor_focus(
        next, next, GridSelectionMode::Cell, &[],
    ))
}

/// `resolveBeginEdit`: `Enter`/`F2` — the cell to edit, or nothing when
/// there is none.
fn resolve_begin_edit(ctx: &GridKeyboardContext, selection: &GridSelection) -> GridKeyCommand {
    if ctx.mode == GridSelectionMode::None {
        return GridKeyCommand::Ignored;
    }
    match current_cursor(ctx, selection) {
        None => GridKeyCommand::Ignored,
        Some(coord) => GridKeyCommand::BeginEdit { coord },
    }
}

/// `edgeCoord`: the coordinate of an edge, in the render order of the
/// columns (01's `coordAt`).
fn edge_coord(edge: GridEdge, cursor: &CellCoord, ctx: &GridKeyboardContext) -> Option<CellCoord> {
    match edge {
        GridEdge::RowStart => coord_at(cursor.row_index, 0, ctx.column_order),
        GridEdge::RowEnd => {
            coord_at(cursor.row_index, ctx.column_order.len().saturating_sub(1), ctx.column_order)
        }
        GridEdge::GridStart => coord_at(0, 0, ctx.column_order),
        GridEdge::GridEnd => {
            coord_at(ctx.row_count.saturating_sub(1), ctx.column_order.len().saturating_sub(1), ctx.column_order)
        }
    }
}

/// `cellMovementTarget`: the cell a movement lands on in cell mode
/// (01's `moveCellFocus` and `coordAt`).
fn cell_movement_target(
    movement: &GridMovement,
    ctx: &GridKeyboardContext,
    cursor: &CellCoord,
) -> Option<CellCoord> {
    match *movement {
        GridMovement::Move { delta_row, delta_column } => {
            move_cell_focus(cursor, delta_row, delta_column, ctx.row_count, ctx.column_order)
        }
        GridMovement::MovePage { down } => {
            let delta = resolve_page_delta(ctx.viewport_row_count) * if down { 1 } else { -1 };
            move_cell_focus(cursor, delta, 0, ctx.row_count, ctx.column_order)
        }
        GridMovement::MoveEdge { edge } => edge_coord(edge, cursor, ctx),
    }
}

/// A movement of the cursor: an arrow key, a page or an edge.
#[derive(Clone, Copy, Debug)]
enum GridMovement {
    Move { delta_row: i64, delta_column: i64 },
    MovePage { down: bool },
    MoveEdge { edge: GridEdge },
}

impl GridMovement {
    fn of(intent: &GridKeyIntent) -> Option<Self> {
        match *intent {
            GridKeyIntent::Move { delta_row, delta_column } => {
                Some(Self::Move { delta_row, delta_column })
            }
            GridKeyIntent::MovePage { down } => Some(Self::MovePage { down }),
            GridKeyIntent::MoveEdge { edge } => Some(Self::MoveEdge { edge }),
            _ => None,
        }
    }
}

/// `rowMovementTarget`: the row a movement lands on in row mode. Row
/// mode carries no cell geometry (00-contracts §4, I2), so an edge has
/// no target there.
///
/// The border of an arrow key is a no-op: `selectRow` on the only
/// selected row *clears* the selection, so one more `↑` at the top row
/// would wipe it (02 §7 E6). A page key clamps into the page instead
/// (02 §7 E11).
fn row_movement_target(
    movement: &GridMovement,
    ctx: &GridKeyboardContext,
    cursor: &CellCoord,
) -> Option<usize> {
    match *movement {
        GridMovement::MoveEdge { .. } => return None,
        // A horizontal arrow has no row.
        GridMovement::Move { delta_column, .. } if delta_column != 0 => return None,
        _ => {}
    }
    let delta = match *movement {
        GridMovement::MovePage { down } => {
            resolve_page_delta(ctx.viewport_row_count) * if down { 1 } else { -1 }
        }
        GridMovement::Move { delta_row, .. } => delta_row,
        GridMovement::MoveEdge { .. } => return None,
    };
    // The row movement goes through 01's `moveCellFocus` like every
    // other movement: it clamps the row into the page and answers
    // `None` when there is no room left. A hand-rolled border check
    // here would silently diverge from the cell mode the day 01 changes
    // its border semantics.
    move_cell_focus(cursor, delta, 0, ctx.row_count, ctx.column_order).map(|next| next.row_index)
}

/// `resolveMovement`: a movement is a cell in cell mode, a row in row
/// mode, an entry in `none`.
fn resolve_movement(
    movement: &GridMovement,
    ctx: &GridKeyboardContext,
    selection: &GridSelection,
) -> GridKeyCommand {
    if ctx.row_count == 0 || ctx.column_order.is_empty() {
        return GridKeyCommand::Consume;
    }
    if ctx.mode == GridSelectionMode::None {
        return enter_cell_mode(ctx);
    }
    let Some(cursor) = current_cursor(ctx, selection) else {
        return GridKeyCommand::Consume;
    };
    if ctx.mode == GridSelectionMode::Row {
        return match row_movement_target(movement, ctx, &cursor) {
            None => GridKeyCommand::Consume,
            Some(row) => GridKeyCommand::RowSelect {
                row_index: row,
                multi: false,
                range: ctx.extend,
            },
        };
    }
    let target = cell_movement_target(movement, ctx, &cursor);
    // No room left: the selection stays exactly as it is (02 §7 E4/E5)
    // — it must not collapse into the border cell.
    match target {
        Some(target) if !grid_cursor_equals(Some(&target), Some(&cursor)) => {
            build_cell_selection(&target, ctx, selection)
        }
        _ => GridKeyCommand::Consume,
    }
}

/// `resolveGridKeyCommand`: intent → command. This is the only semantic
/// centre of the volume: every keyboard behaviour of the grid can be
/// decided without a browser.
pub(crate) fn resolve_grid_key_command(
    intent: &GridKeyIntent,
    ctx: &GridKeyboardContext,
    selection: &GridSelection,
) -> GridKeyCommand {
    match *intent {
        GridKeyIntent::Pass => GridKeyCommand::Pass,
        GridKeyIntent::Ignored => GridKeyCommand::Ignored,
        // `Tab` leaves the grid: the key is claimed (the browser must
        // not move the focus into the row-number slot) but no state
        // changes.
        GridKeyIntent::LeaveGrid => GridKeyCommand::Consume,
        GridKeyIntent::SelectAllRows => GridKeyCommand::SelectAllRows,
        GridKeyIntent::BeginEdit => resolve_begin_edit(ctx, selection),
        GridKeyIntent::Move { .. } | GridKeyIntent::MovePage { .. } | GridKeyIntent::MoveEdge { .. } => {
            resolve_movement(&GridMovement::of(intent).unwrap_or(GridMovement::Move {
                delta_row: 0,
                delta_column: 0,
            }), ctx, selection)
        }
    }
}

// ---- the gridSelection pure functions (T210's, mirrored) --------------------

/// `columnIndexFromCoord`.
pub(crate) fn column_index_from_coord(coord: &CellCoord, column_order: &[String]) -> i64 {
    column_order.iter().position(|name| *name == coord.column_name).map_or(-1, |i| i as i64)
}

/// `cellCoordEquals`.
pub(crate) fn cell_coord_equals(a: Option<&CellCoord>, b: Option<&CellCoord>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.row_index == b.row_index && a.column_name == b.column_name,
        _ => false,
    }
}

/// `coordAt`: row/cell coordinate → coordinate, or `None` when either
/// index falls outside the grid.
pub(crate) fn coord_at(
    row_index: usize,
    column_index: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    let name = column_order.get(column_index)?;
    Some(CellCoord { row_index, column_name: name.clone() })
}

/// `clampCellCoord`: pull a coordinate back into the grid. An unknown
/// column (hidden while referenced) or an empty grid has no legal
/// coordinate at all, hence `None`; a row index is clamped.
pub(crate) fn clamp_cell_coord(
    coord: &CellCoord,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    if row_count == 0 {
        return None;
    }
    if column_index_from_coord(coord, column_order) < 0 {
        return None;
    }
    let row_index = coord.row_index.min(row_count - 1);
    Some(CellCoord { row_index, column_name: coord.column_name.clone() })
}

/// `moveCellFocus`.
pub(crate) fn move_cell_focus(
    focus: &CellCoord,
    delta_row: i64,
    delta_column: i64,
    row_count: usize,
    column_order: &[String],
) -> Option<CellCoord> {
    if row_count == 0 {
        return None;
    }
    let from_column = column_index_from_coord(focus, column_order);
    if from_column < 0 {
        return None;
    }
    let from_column = from_column as usize;
    let clamp = |value: i64, max: usize| -> usize {
        value.clamp(0, max as i64) as usize
    };
    let next_row = clamp(focus.row_index as i64 + delta_row, row_count - 1);
    let next_column = clamp(from_column as i64 + delta_column, column_order.len() - 1);
    if next_row == focus.row_index && next_column == from_column {
        return None;
    }
    let name = column_order.get(next_column)?;
    Some(CellCoord { row_index: next_row, column_name: name.clone() })
}

/// `selectionFromAnchorFocus`: build a selection from its two corners.
///
/// `row` mode carries no geometry at all (00-contracts §4 invariant
/// I2): the row highlight comes from `selectedRows`, so anchor/focus/
/// extras are dropped here instead of being kept as a hidden second
/// source of truth.
pub(crate) fn selection_from_anchor_focus(
    anchor: &CellCoord,
    focus: &CellCoord,
    mode: GridSelectionMode,
    extra_ranges: &[CellRange],
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
        anchor: Some(anchor.clone()),
        focus: Some(focus.clone()),
        extra_ranges: extra_ranges.to_vec(),
        mode: GridSelectionMode::Cell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns() -> Vec<String> {
        vec!["id".to_owned(), "name".to_owned(), "notes".to_owned()]
    }

    fn coord(row: usize, column: &str) -> CellCoord {
        CellCoord { row_index: row, column_name: column.to_owned() }
    }

    fn event(key: &str) -> GridKeyEventLike {
        GridKeyEventLike { key: key.to_owned(), ..Default::default() }
    }

    fn ctx<'a>(column_order: &'a [String]) -> GridKeyboardContext<'a> {
        GridKeyboardContext {
            column_order,
            row_count: 10,
            mode: GridSelectionMode::Cell,
            cursor: Some(coord(3, "name")),
            viewport_row_count: 20,
            editing: false,
            extend: false,
        }
    }

    #[test]
    fn the_intent_priority_table_is_the_written_order() {
        let columns = columns();
        let mut context = ctx(&columns);
        // ① IME composition owns every key.
        let mut composing = event("arrowdown");
        composing.is_composing = true;
        assert_eq!(read_grid_key_intent(&composing, &context), GridKeyIntent::Pass);
        let mut process = event("process");
        process.is_composing = false;
        assert_eq!(read_grid_key_intent(&process, &context), GridKeyIntent::Pass);
        // ② an open editor owns the keyboard.
        context.editing = true;
        assert_eq!(read_grid_key_intent(&event("arrowdown"), &context), GridKeyIntent::Pass);
        context.editing = false;
        // ③ Escape is 01's, not the grid's.
        assert_eq!(read_grid_key_intent(&event("escape"), &context), GridKeyIntent::Pass);
        // ④ clipboard/undo.
        for key in ["c", "x", "v", "z"] {
            let mut clipboard = event(key);
            clipboard.meta_key = true;
            assert_eq!(read_grid_key_intent(&clipboard, &context), GridKeyIntent::Pass);
        }
        // ⑤ Alt.
        let mut alt = event("arrowdown");
        alt.alt_key = true;
        assert_eq!(read_grid_key_intent(&alt, &context), GridKeyIntent::Pass);
    }

    #[test]
    fn the_key_table_maps_verbatim() {
        let columns = columns();
        let context = ctx(&columns);
        // The arrows.
        assert_eq!(
            read_grid_key_intent(&event("arrowup"), &context),
            GridKeyIntent::Move { delta_row: -1, delta_column: 0 }
        );
        assert_eq!(
            read_grid_key_intent(&event("arrowright"), &context),
            GridKeyIntent::Move { delta_row: 0, delta_column: 1 }
        );
        // Mod+arrow is the row edge.
        let mut left = event("arrowleft");
        left.meta_key = true;
        assert_eq!(
            read_grid_key_intent(&left, &context),
            GridKeyIntent::MoveEdge { edge: GridEdge::RowStart }
        );
        let mut right = event("arrowright");
        right.ctrl_key = true;
        assert_eq!(
            read_grid_key_intent(&right, &context),
            GridKeyIntent::MoveEdge { edge: GridEdge::RowEnd }
        );
        // Home/End, bare and with mod.
        assert_eq!(
            read_grid_key_intent(&event("home"), &context),
            GridKeyIntent::MoveEdge { edge: GridEdge::RowStart }
        );
        let mut home = event("home");
        home.meta_key = true;
        assert_eq!(
            read_grid_key_intent(&home, &context),
            GridKeyIntent::MoveEdge { edge: GridEdge::GridStart }
        );
        let mut end = event("end");
        end.ctrl_key = true;
        assert_eq!(
            read_grid_key_intent(&end, &context),
            GridKeyIntent::MoveEdge { edge: GridEdge::GridEnd }
        );
        // Pages.
        assert_eq!(
            read_grid_key_intent(&event("pageup"), &context),
            GridKeyIntent::MovePage { down: false }
        );
        assert_eq!(
            read_grid_key_intent(&event("pagedown"), &context),
            GridKeyIntent::MovePage { down: true }
        );
        // Tab leaves.
        assert_eq!(read_grid_key_intent(&event("tab"), &context), GridKeyIntent::LeaveGrid);
        // Enter/F2 begin an edit (with a selection); Cmd+Enter is the
        // global run shortcut.
        assert_eq!(read_grid_key_intent(&event("enter"), &context), GridKeyIntent::BeginEdit);
        assert_eq!(read_grid_key_intent(&event("f2"), &context), GridKeyIntent::BeginEdit);
        let mut run = event("enter");
        run.meta_key = true;
        assert_eq!(read_grid_key_intent(&run, &context), GridKeyIntent::Pass);
        // Without a selection Enter is claimed-but-idle.
        let mut none_mode = ctx(&columns);
        none_mode.mode = GridSelectionMode::None;
        assert_eq!(read_grid_key_intent(&event("enter"), &none_mode), GridKeyIntent::Ignored);
        // Cmd+A selects every row; a bare 'a' is not the grid's.
        let mut select_all = event("a");
        select_all.meta_key = true;
        assert_eq!(read_grid_key_intent(&select_all, &context), GridKeyIntent::SelectAllRows);
        assert_eq!(read_grid_key_intent(&event("a"), &context), GridKeyIntent::Pass);
        // Everything else passes through.
        for key in [" ", "delete", "backspace", "q"] {
            assert_eq!(read_grid_key_intent(&event(key), &context), GridKeyIntent::Pass);
        }
    }

    #[test]
    fn the_cursor_resolution_per_mode() {
        let columns = columns();
        // Cell mode: the focus corner.
        let cell = GridSelection {
            anchor: Some(coord(1, "id")),
            focus: Some(coord(5, "notes")),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        };
        assert_eq!(
            resolve_keyboard_cursor(&cell, None, &columns).as_ref(),
            Some(&coord(5, "notes"))
        );
        // Row mode: the anchor row, first column; nothing without one.
        let row = GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        };
        assert_eq!(
            resolve_keyboard_cursor(&row, Some(3), &columns).as_ref(),
            Some(&coord(3, "id"))
        );
        assert_eq!(resolve_keyboard_cursor(&row, None, &columns), None);
        let empty_columns: Vec<String> = Vec::new();
        assert_eq!(resolve_keyboard_cursor(&row, Some(3), &empty_columns), None);
        // None mode: nothing.
        let none = GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::None,
        };
        assert_eq!(resolve_keyboard_cursor(&none, Some(3), &columns), None);
    }

    #[test]
    fn the_page_delta_is_one_screen_minus_one_row() {
        assert_eq!(resolve_page_delta(20), 19);
        assert_eq!(resolve_page_delta(1), 1, "a one-row viewport still steps one");
        assert_eq!(resolve_page_delta(0), 1, "never zero");
    }

    #[test]
    fn a_cell_move_clamps_and_consumes_at_the_border() {
        let columns = columns();
        let context = ctx(&columns);
        let selection = GridSelection {
            anchor: Some(coord(3, "name")),
            focus: Some(coord(3, "name")),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        };
        // A plain move collapses to the new single cell.
        let intent = GridKeyIntent::Move { delta_row: 0, delta_column: 1 };
        let command = resolve_grid_key_command(&intent, &context, &selection);
        assert_eq!(
            command,
            GridKeyCommand::SetSelection(GridSelection {
                anchor: Some(coord(3, "notes")),
                focus: Some(coord(3, "notes")),
                extra_ranges: Vec::new(),
                mode: GridSelectionMode::Cell,
            })
        );
        // From the first column a left move is consumed, the selection
        // unchanged (02 §7 E4/E5 — it must not collapse into the border).
        let mut at_first = ctx(&columns);
        at_first.cursor = Some(coord(3, "id"));
        let border = GridKeyIntent::Move { delta_row: 0, delta_column: -1 };
        assert_eq!(resolve_grid_key_command(&border, &at_first, &selection), GridKeyCommand::Consume);
        // From an inner column the same move is a real move (clamped).
        let inner = GridKeyIntent::Move { delta_row: 0, delta_column: -1 };
        let command = resolve_grid_key_command(&inner, &context, &selection);
        let GridKeyCommand::SetSelection(moved) = command else {
            panic!("expected a selection");
        };
        assert_eq!(moved.focus.as_ref(), Some(&coord(3, "id")));
        // Row edges.
        let edge = GridKeyIntent::MoveEdge { edge: GridEdge::RowStart };
        let command = resolve_grid_key_command(&edge, &context, &selection);
        let GridKeyCommand::SetSelection(moved) = command else {
            panic!("expected a selection");
        };
        assert_eq!(moved.focus.as_ref(), Some(&coord(3, "id")));
        // Grid edges.
        let grid_end = GridKeyIntent::MoveEdge { edge: GridEdge::GridEnd };
        let command = resolve_grid_key_command(&grid_end, &context, &selection);
        let GridKeyCommand::SetSelection(moved) = command else {
            panic!("expected a selection");
        };
        assert_eq!(moved.focus.as_ref(), Some(&coord(9, "notes")));
        // A page move steps the viewport minus one.
        let page = GridKeyIntent::MovePage { down: true };
        let command = resolve_grid_key_command(&page, &context, &selection);
        let GridKeyCommand::SetSelection(moved) = command else {
            panic!("expected a selection");
        };
        assert_eq!(moved.focus.as_ref(), Some(&coord(9, "name")), "3 + 19 clamps to the last row");
    }

    #[test]
    fn an_extended_move_keeps_the_anchor_and_the_extras() {
        let columns = columns();
        let mut context = ctx(&columns);
        context.extend = true;
        let extra = CellRange { start: coord(8, "id"), end: coord(8, "id") };
        let selection = GridSelection {
            anchor: Some(coord(3, "id")),
            focus: Some(coord(3, "name")),
            extra_ranges: vec![extra.clone()],
            mode: GridSelectionMode::Cell,
        };
        let intent = GridKeyIntent::Move { delta_row: 0, delta_column: 1 };
        let command = resolve_grid_key_command(&intent, &context, &selection);
        let GridKeyCommand::SetSelection(extended) = command else {
            panic!("expected a selection");
        };
        assert_eq!(extended.anchor.as_ref(), Some(&coord(3, "id")), "the anchor stays");
        assert_eq!(extended.focus.as_ref(), Some(&coord(3, "notes")));
        assert_eq!(extended.extra_ranges, vec![extra], "the extra ranges survive");
        // A missing anchor adopts the cursor as a single-cell region.
        let mut anchorless = selection.clone();
        anchorless.anchor = None;
        let command = resolve_grid_key_command(&intent, &context, &anchorless);
        let GridKeyCommand::SetSelection(adopted) = command else {
            panic!("expected a selection");
        };
        assert_eq!(adopted.anchor.as_ref(), Some(&coord(3, "notes")), "the cursor is the anchor");
    }

    #[test]
    fn none_mode_enters_at_the_remembered_or_first_cell() {
        let columns = columns();
        let mut context = ctx(&columns);
        context.mode = GridSelectionMode::None;
        let selection = GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::None,
        };
        // The remembered cursor is the landing spot.
        let intent = GridKeyIntent::Move { delta_row: 1, delta_column: 0 };
        let command = resolve_grid_key_command(&intent, &context, &selection);
        assert_eq!(
            command,
            GridKeyCommand::SetSelection(GridSelection {
                anchor: Some(coord(3, "name")),
                focus: Some(coord(3, "name")),
                extra_ranges: Vec::new(),
                mode: GridSelectionMode::Cell,
            }),
            "an arrow never invents a move in none mode — it enters at the remembered cell"
        );
        // Nothing remembered: the first cell.
        context.cursor = None;
        let command = resolve_grid_key_command(&intent, &context, &selection);
        let GridKeyCommand::SetSelection(entered) = command else {
            panic!("expected a selection");
        };
        assert_eq!(entered.focus.as_ref(), Some(&coord(0, "id")));
        // An empty grid consumes.
        context.row_count = 0;
        assert_eq!(resolve_grid_key_command(&intent, &context, &selection), GridKeyCommand::Consume);
    }

    #[test]
    fn row_mode_moves_rows_and_consumes_at_the_border() {
        let columns = columns();
        let mut context = ctx(&columns);
        context.mode = GridSelectionMode::Row;
        let selection = GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        };
        // A vertical move selects the next row.
        let intent = GridKeyIntent::Move { delta_row: 1, delta_column: 0 };
        assert_eq!(
            resolve_grid_key_command(&intent, &context, &selection),
            GridKeyCommand::RowSelect { row_index: 4, multi: false, range: false }
        );
        // Shift extends the row range.
        context.extend = true;
        assert_eq!(
            resolve_grid_key_command(&intent, &context, &selection),
            GridKeyCommand::RowSelect { row_index: 4, multi: false, range: true }
        );
        context.extend = false;
        // A horizontal arrow has no row; an edge has no row.
        let horizontal = GridKeyIntent::Move { delta_row: 0, delta_column: 1 };
        assert_eq!(
            resolve_grid_key_command(&horizontal, &context, &selection),
            GridKeyCommand::Consume
        );
        let edge = GridKeyIntent::MoveEdge { edge: GridEdge::RowStart };
        assert_eq!(resolve_grid_key_command(&edge, &context, &selection), GridKeyCommand::Consume);
        // The top border is a no-op (one more ↑ must not wipe the
        // selection, 02 §7 E6).
        context.cursor = Some(coord(0, "id"));
        let up = GridKeyIntent::Move { delta_row: -1, delta_column: 0 };
        assert_eq!(resolve_grid_key_command(&up, &context, &selection), GridKeyCommand::Consume);
        // A page key clamps into the page (E11).
        let page = GridKeyIntent::MovePage { down: true };
        assert_eq!(
            resolve_grid_key_command(&page, &context, &selection),
            GridKeyCommand::RowSelect { row_index: 9, multi: false, range: false }
        );
    }

    #[test]
    fn begin_edit_resolves_the_cursor() {
        let columns = columns();
        let context = ctx(&columns);
        let selection = GridSelection {
            anchor: Some(coord(3, "name")),
            focus: Some(coord(3, "name")),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        };
        assert_eq!(
            resolve_grid_key_command(&GridKeyIntent::BeginEdit, &context, &selection),
            GridKeyCommand::BeginEdit { coord: coord(3, "name") }
        );
        // None mode is claimed-but-idle.
        let mut none_mode = ctx(&columns);
        none_mode.mode = GridSelectionMode::None;
        assert_eq!(
            resolve_grid_key_command(&GridKeyIntent::BeginEdit, &none_mode, &selection),
            GridKeyCommand::Ignored
        );
    }

    #[test]
    fn the_composing_guard_checks_both_signals() {
        let mut e = GridKeyDownEvent { key: "a".to_owned(), ..Default::default() };
        assert!(!is_composing_event(&e));
        e.native_is_composing = true;
        assert!(is_composing_event(&e));
        e.native_is_composing = false;
        e.native_key_code = 229;
        assert!(is_composing_event(&e), "the legacy keyCode signal");
        e.native_key_code = 65;
        assert!(!is_composing_event(&e));
    }

    #[test]
    fn the_projection_lowercases_and_carries_the_flags() {
        let e = GridKeyDownEvent {
            key: "ArrowDown".to_owned(),
            meta_key: true,
            ctrl_key: false,
            shift_key: true,
            alt_key: false,
            native_is_composing: false,
            native_key_code: 229,
            target_editable: false,
        };
        let projected = to_grid_key_event_like(&e);
        assert_eq!(projected.key, "arrowdown");
        assert!(projected.meta_key);
        assert!(projected.shift_key);
        assert!(projected.is_composing, "keyCode 229 projects to composing");
    }

    // ---- the gridSelection mirrors ------------------------------------------

    #[test]
    fn the_coord_helpers_are_ones_semantics() {
        let columns = columns();
        assert_eq!(column_index_from_coord(&coord(0, "name"), &columns), 1);
        assert_eq!(column_index_from_coord(&coord(0, "missing"), &columns), -1);
        assert_eq!(coord_at(2, 1, &columns).as_ref(), Some(&coord(2, "name")));
        assert_eq!(coord_at(2, 3, &columns), None, "past the last column");
        assert_eq!(clamp_cell_coord(&coord(99, "id"), 10, &columns).as_ref(), Some(&coord(9, "id")));
        assert_eq!(clamp_cell_coord(&coord(0, "missing"), 10, &columns), None);
        assert_eq!(clamp_cell_coord(&coord(0, "id"), 0, &columns), None, "empty grid");
        assert!(cell_coord_equals(Some(&coord(1, "id")), Some(&coord(1, "id"))));
        assert!(!cell_coord_equals(Some(&coord(1, "id")), Some(&coord(1, "name"))));
        assert!(!cell_coord_equals(Some(&coord(1, "id")), None));
        assert!(cell_coord_equals(None, None));
    }

    #[test]
    fn move_cell_focus_clamps_and_answers_none_at_rest() {
        let columns = columns();
        assert_eq!(
            move_cell_focus(&coord(3, "name"), -10, 0, 10, &columns).as_ref(),
            Some(&coord(0, "name")),
            "clamped to the top"
        );
        assert_eq!(
            move_cell_focus(&coord(3, "name"), 0, 5, 10, &columns).as_ref(),
            Some(&coord(3, "notes")),
            "clamped to the last column"
        );
        assert_eq!(
            move_cell_focus(&coord(3, "name"), 0, 0, 10, &columns),
            None,
            "no movement is no answer (the border consume)"
        );
        assert_eq!(
            move_cell_focus(&coord(3, "missing"), 1, 0, 10, &columns),
            None,
            "an unknown column has no move"
        );
        assert_eq!(move_cell_focus(&coord(3, "name"), 1, 0, 0, &columns), None, "empty grid");
    }

    #[test]
    fn row_mode_selections_carry_no_geometry() {
        let selection = selection_from_anchor_focus(
            &coord(1, "id"),
            &coord(2, "name"),
            GridSelectionMode::Row,
            &[CellRange { start: coord(8, "id"), end: coord(8, "id") }],
        );
        assert_eq!(selection.mode, GridSelectionMode::Row);
        assert!(selection.anchor.is_none());
        assert!(selection.focus.is_none());
        assert!(selection.extra_ranges.is_empty());
        let cell = selection_from_anchor_focus(
            &coord(1, "id"),
            &coord(2, "name"),
            GridSelectionMode::Cell,
            &[CellRange { start: coord(8, "id"), end: coord(8, "id") }],
        );
        assert_eq!(cell.anchor.as_ref(), Some(&coord(1, "id")));
        assert_eq!(cell.focus.as_ref(), Some(&coord(2, "name")));
        assert_eq!(cell.extra_ranges.len(), 1);
    }
}
