//! Port of `packages/frontend/src/hooks/useGridSelection.ts` (T328,
//! split) — the grid's pointer wiring (DB-01).
//!
//! Translate pointer/keyboard events into `GridSelection` transitions.
//! The store stays the only source of truth: this hook keeps just the
//! in-flight drag session (a render optimisation, not a second copy)
//! and answers one selection change per finished interaction.
//!
//! ## What maps where
//!
//! - **The hook is its drag session** (the T294 env convention):
//!   [`GridSelectionHook`] holds the [`DragSession`] plus the
//!   `dragging` flag; every `useCallback` becomes a method over the
//!   per-frame [`GridSelectionEnv`].
//! - The DOM resolutions are the host's seams: the pointer event
//!   carries the already-resolved answers
//!   ([`GridPointerEvent::target_cell`] = `resolveDataTableCellFrom
//!   Event`, [`GridPointerEvent::target_row_header`] =
//!   `rowIndexFromRowHeader`, [`GridPointerEvent::cell_from_point`] =
//!   `document.elementFromPoint` → the cell, and
//!   [`GridPointerEvent::target_editable`] = `isEditableTarget`).
//! - Pointer capture (`capturePointer` / `releasePointerCapture`,
//!   with their synthetic-event and jsdom tolerances) is the host's
//!   pointer machinery: the session's lifecycle is the contract here.
//! - `finishSession` is [`GridSelectionHook::finish_session`]: the
//!   blank-surface rule (a click clears, a drag is ignored), the
//!   `moved` region, the `additive` toggle (01's `toggleExtraRange`),
//!   the `extended` anchor-extension, and the plain single cell — in
//!   that exact order.
//! - `onKeyDown` is [`GridSelectionHook::on_key_down`]: DB-02's nav
//!   hook runs FIRST (the host reports whether it claimed the key —
//!   `defaultPrevented`), a composition owns every key (01-selection
//!   §4.5), Escape belongs to an open editor, and the in-flight drag
//!   cancels FIRST (the store still reports the pre-drag mode, so
//!   clearing the selection alone would leave the session alive).
//! - Focus leaving the grid does NOT clear the selection (the user may
//!   be on the way to a toolbar action): [`GridSelectionHook::on_blur`]
//!   ends only the session.
//! - The pure helpers (`selectionRanges`' membership, the extra-range
//!   toggle, the id/key derivations) are the T210 seam's
//!   ([`crate::hooks::grid_keyboard_helpers`]).
//!
//! ## Keys
//!
//! No elements are painted here (the hook is the machine; the grid
//! surface paints). `activeDescendantId` is the `aria-activedescendant`
//! contract the cell renderer builds the same way.

use crate::hooks::grid_keyboard_helpers::{
    GridKeyDownEvent, cell_coord_equals, column_index_from_coord, grid_selection_contains,
    is_composing_event, selection_from_anchor_focus, toggle_extra_range,
};
use crate::table_data_connection_state::{
    CellCoord, GridSelection, GridSelectionMode, EMPTY_GRID_SELECTION,
};

/// One pointer interaction. `anchor == None` means it started on the
/// blank surface; `focus` keeps the last cell the pointer was over so
/// a drag leaving the grid keeps its last hit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DragSession {
    pub(crate) pointer_id: u64,
    pub(crate) anchor: Option<CellCoord>,
    pub(crate) focus: Option<CellCoord>,
    pub(crate) moved: bool,
    /// `metaKey || ctrlKey` at pointerdown (Cmd/Ctrl+click adds).
    pub(crate) additive: bool,
    /// `shiftKey && selection.mode === 'cell'` at pointerdown.
    pub(crate) extended: bool,
}

/// The hook's per-frame environment (React's `UseGridSelectionOptions`;
/// the callbacks' presences are booleans — the host supplies the
/// actions themselves).
pub(crate) struct GridSelectionEnv<'a> {
    /// Displayed column names, in render order.
    pub(crate) column_order: &'a [String],
    pub(crate) row_count: usize,
    /// Controlled value: the store owns the selection, this hook keeps
    /// no copy.
    pub(crate) selection: &'a GridSelection,
    /// The cell selection switch (default true; read-only grids stay
    /// selectable).
    pub(crate) enabled: bool,
    /// While a cell is being edited, Escape belongs to the editor.
    pub(crate) editing_cell: bool,
    /// `onRowGutterSelect` is supplied.
    pub(crate) has_row_gutter: bool,
}

impl<'a> GridSelectionEnv<'a> {
    /// React's defaults: enabled, no editor, no gutter path.
    pub(crate) fn new(
        column_order: &'a [String],
        row_count: usize,
        selection: &'a GridSelection,
    ) -> Self {
        Self {
            column_order,
            row_count,
            selection,
            enabled: true,
            editing_cell: false,
            has_row_gutter: false,
        }
    }
}

/// The pointer event projection: the DOM resolutions the host already
/// performed (no DOM handle crosses the seam).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GridPointerEvent {
    pub(crate) pointer_id: u64,
    /// `event.button` (only `0` starts a session).
    pub(crate) button: i16,
    pub(crate) meta_key: bool,
    pub(crate) ctrl_key: bool,
    pub(crate) shift_key: bool,
    /// `isEditableTarget(event.target)`.
    pub(crate) target_editable: bool,
    /// `rowIndexFromRowHeader(event.target)`.
    pub(crate) target_row_header: Option<usize>,
    /// `resolveDataTableCellFromEvent(event.target)`.
    pub(crate) target_cell: Option<CellCoord>,
    /// `cellFromPoint(clientX, clientY)` — the move/up answer; `None`
    /// on a cancel (no coordinates) or a miss.
    pub(crate) cell_from_point: Option<CellCoord>,
}

/// What a pointerdown decided.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PointerDownEffect {
    /// Nothing (disabled, a secondary button, an editable target, or a
    /// stale cell outside the page).
    None,
    /// The row gutter: the existing whole-row selection path
    /// (`onRowGutterSelect(rowIndex, { multi, range })`).
    RowGutterSelect { row_index: usize, multi: bool, range: bool },
    /// A session started: the host moves focus to the grid scroll
    /// container (cell interactions focus the grid host; editor inputs
    /// were excluded above).
    SessionStarted,
}

/// The hook: the in-flight drag session plus the `dragging` flag.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GridSelectionHook {
    session: Option<DragSession>,
    dragging: bool,
}

impl GridSelectionHook {
    /// `useGridSelection()`: no session, not dragging.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// True while a drag is in progress (drives `select-none` and the
    /// journey assertions).
    pub(crate) fn dragging(&self) -> bool {
        self.dragging
    }

    /// The in-flight session (the host's capture handling).
    pub(crate) fn session(&self) -> Option<&DragSession> {
        self.session.as_ref()
    }

    /// `clearSession`: forget the session, hand back capture, leave
    /// `dragging` false. No selection change.
    pub(crate) fn clear_session(&mut self) {
        self.session = None;
        self.dragging = false;
    }

    /// `onPointerDown`.
    pub(crate) fn on_pointer_down(
        &mut self,
        env: &GridSelectionEnv,
        event: &GridPointerEvent,
    ) -> PointerDownEffect {
        if !env.enabled || event.button != 0 {
            return PointerDownEffect::None;
        }
        if event.target_editable {
            return PointerDownEffect::None;
        }
        if let Some(row_index) = event.target_row_header {
            if env.has_row_gutter {
                return PointerDownEffect::RowGutterSelect {
                    row_index,
                    multi: event.meta_key || event.ctrl_key,
                    range: event.shift_key,
                };
            }
        }
        // A row index outside the current page is stale DOM from an
        // uncommitted render.
        let cell = event
            .target_cell
            .as_ref()
            .filter(|cell| cell.row_index < env.row_count)
            .cloned();
        let extended =
            cell.is_some() && event.shift_key && env.selection.mode == GridSelectionMode::Cell;
        self.session = Some(DragSession {
            pointer_id: event.pointer_id,
            anchor: cell.clone(),
            focus: cell,
            moved: false,
            additive: event.meta_key || event.ctrl_key,
            extended,
        });
        PointerDownEffect::SessionStarted
    }

    /// `onPointerMove`: the session's focus follows the pointer; the
    /// first different cell marks the drag.
    pub(crate) fn on_pointer_move(&mut self, event: &GridPointerEvent) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.pointer_id != event.pointer_id || session.anchor.is_none() {
            return;
        }
        let Some(cell) = event.cell_from_point.clone() else {
            return;
        };
        if cell_coord_equals(Some(&cell), session.focus.as_ref()) {
            return;
        }
        session.focus = Some(cell);
        session.moved = true;
        self.dragging = true;
    }

    /// `onPointerUp`: finish with commit, the point's cell.
    pub(crate) fn on_pointer_up(
        &mut self,
        env: &GridSelectionEnv,
        event: &GridPointerEvent,
    ) -> Option<GridSelection> {
        self.finish_session(event.pointer_id, true, event.cell_from_point.clone(), env)
    }

    /// `onPointerCancel`: finish with commit but no coordinates (the
    /// focus falls back to the session's last hit).
    pub(crate) fn on_pointer_cancel(
        &mut self,
        env: &GridSelectionEnv,
        event: &GridPointerEvent,
    ) -> Option<GridSelection> {
        self.finish_session(event.pointer_id, true, None, env)
    }

    /// `finishSession(pointerId, commit, clientX?, clientY?)`: the
    /// once-per-interaction `onChange` answer.
    fn finish_session(
        &mut self,
        pointer_id: u64,
        commit: bool,
        pointer_cell: Option<CellCoord>,
        env: &GridSelectionEnv,
    ) -> Option<GridSelection> {
        let session = self.session.take()?;
        if session.pointer_id != pointer_id {
            // Not this session's pointer: put it back untouched.
            self.session = Some(session);
            return None;
        }
        self.dragging = false;
        if !commit {
            return None;
        }
        // Pointer went down on the blank surface: a click clears, a
        // drag is ignored.
        let Some(anchor) = session.anchor.clone() else {
            if !session.moved {
                return Some(EMPTY_GRID_SELECTION.clone());
            }
            return None;
        };
        let focus = pointer_cell
            .or(session.focus)
            .unwrap_or_else(|| anchor.clone());
        if session.moved {
            return Some(selection_from_anchor_focus(
                &anchor,
                &focus,
                GridSelectionMode::Cell,
                &[],
            ));
        }
        if session.additive {
            return Some(toggle_extra_range(env.selection, &anchor, env.column_order));
        }
        if session.extended
            && env.selection.mode == GridSelectionMode::Cell
        {
            if let Some(existing_anchor) = env.selection.anchor.clone() {
                return Some(selection_from_anchor_focus(
                    &existing_anchor,
                    &anchor,
                    GridSelectionMode::Cell,
                    &[],
                ));
            }
        }
        Some(selection_from_anchor_focus(&anchor, &anchor, GridSelectionMode::Cell, &[]))
    }

    /// `onKeyDown`: the nav hook first (the host reports whether it
    /// claimed the key), then Escape's clearing.
    pub(crate) fn on_key_down(
        &mut self,
        env: &GridSelectionEnv,
        event: &GridKeyDownEvent,
        nav_claimed: bool,
    ) -> Option<GridSelection> {
        // DB-02: the navigation claims the keys it owns; a claimed key
        // must not reach the selection handler.
        if nav_claimed {
            return None;
        }
        // 01-selection §4.5: an IME composition owns every key, Escape
        // included.
        if is_composing_event(event) {
            return None;
        }
        if event.key.to_lowercase() != "escape" {
            return None;
        }
        if env.editing_cell {
            return None;
        }
        // Cancel the in-flight drag FIRST: the store still reports the
        // pre-drag mode, so clearing the selection alone would leave
        // the session alive and the next pointerup would commit the
        // region the user just cancelled.
        let had_session = self.session.is_some();
        self.clear_session();
        if !had_session && env.selection.mode == GridSelectionMode::None {
            return None;
        }
        Some(EMPTY_GRID_SELECTION.clone())
    }

    /// `onBlur`: focus leaving the grid must not clear the selection —
    /// only the in-flight drag session ends.
    pub(crate) fn on_blur(&mut self) {
        self.clear_session();
    }

    /// `isCellSelected`: render-time membership test (the pure
    /// helpers, never the DOM).
    pub(crate) fn is_cell_selected(
        &self,
        env: &GridSelectionEnv,
        row_index: usize,
        column_name: &str,
    ) -> bool {
        let coord = CellCoord { row_index, column_name: column_name.to_owned() };
        grid_selection_contains(env.selection, &coord, env.column_order)
    }

    /// `activeDescendantId`: the `aria-activedescendant` target for
    /// the focus cell (`` {prefix}-r{row}c{column} ``, the column an
    /// index in render order).
    pub(crate) fn active_descendant_id(
        &self,
        env: &GridSelectionEnv,
        id_prefix: &str,
    ) -> Option<String> {
        let focus = (env.selection.mode == GridSelectionMode::Cell)
            .then(|| env.selection.focus.clone())
            .flatten()?;
        let column = column_index_from_coord(&focus, env.column_order);
        if column < 0 {
            return None;
        }
        Some(format!("{}-r{}c{}", id_prefix, focus.row_index, column))
    }

    /// `focusKey`: the stable key of the focus cell, so virtual
    /// scrolling can keep it mounted (`{row}:{column}`).
    pub(crate) fn focus_key(&self, env: &GridSelectionEnv) -> Option<String> {
        (env.selection.mode == GridSelectionMode::Cell)
            .then(|| env.selection.focus.clone())
            .flatten()
            .map(|focus| format!("{}:{}", focus.row_index, focus.column_name))
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

    fn empty() -> GridSelection {
        EMPTY_GRID_SELECTION.clone()
    }

    fn cell_selection(anchor: (usize, &str), focus: (usize, &str)) -> GridSelection {
        selection_from_anchor_focus(
            &coord(anchor.0, anchor.1),
            &coord(focus.0, focus.1),
            GridSelectionMode::Cell,
            &[],
        )
    }

    fn env_for<'a>(
        columns: &'a [String],
        selection: &'a GridSelection,
    ) -> GridSelectionEnv<'a> {
        GridSelectionEnv::new(columns, 10, selection)
    }

    fn pointer(button: i16, cell: Option<CellCoord>) -> GridPointerEvent {
        GridPointerEvent { pointer_id: 7, button, target_cell: cell, ..Default::default() }
    }

    #[test]
    fn a_plain_click_selects_the_single_cell() {
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        assert_eq!(
            hook.on_pointer_down(&env, &pointer(0, Some(coord(3, "name")))),
            PointerDownEffect::SessionStarted
        );
        let answer = hook.on_pointer_up(&env, &pointer(0, Some(coord(3, "name"))));
        assert_eq!(answer, Some(cell_selection((3, "name"), (3, "name"))));
        assert!(!hook.dragging());
    }

    #[test]
    fn a_drag_commits_the_anchor_to_focus_region() {
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let _ = hook.on_pointer_down(&env, &pointer(0, Some(coord(1, "id"))));
        // The move to a different cell marks the drag.
        let mut moving = pointer(0, None);
        moving.cell_from_point = Some(coord(4, "notes"));
        hook.on_pointer_move(&moving);
        assert!(hook.dragging());
        let answer = hook.on_pointer_up(&env, &moving);
        assert_eq!(answer, Some(cell_selection((1, "id"), (4, "notes"))));
        assert!(!hook.dragging());
        assert!(hook.session().is_none(), "the session ended");
    }

    #[test]
    fn a_move_to_the_same_cell_is_not_a_drag() {
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let _ = hook.on_pointer_down(&env, &pointer(0, Some(coord(2, "name"))));
        let mut moving = pointer(0, None);
        moving.cell_from_point = Some(coord(2, "name"));
        hook.on_pointer_move(&moving);
        assert!(!hook.dragging(), "sameCoord: no move, no drag");
        // A miss (elementFromPoint null) changes nothing.
        hook.on_pointer_move(&pointer(0, None));
        assert!(!hook.dragging());
        // The pointerup is then a click.
        let answer = hook.on_pointer_up(&env, &pointer(0, Some(coord(2, "name"))));
        assert_eq!(answer, Some(cell_selection((2, "name"), (2, "name"))));
    }

    #[test]
    fn the_blank_surface_click_clears_and_its_drag_is_ignored() {
        let columns = columns();
        let selection = cell_selection((1, "id"), (2, "name"));
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        // Down on the blank surface: the session starts anchorless.
        assert_eq!(hook.on_pointer_down(&env, &pointer(0, None)), PointerDownEffect::SessionStarted);
        // A click clears.
        assert_eq!(hook.on_pointer_up(&env, &pointer(0, None)), Some(empty()));
        // A drag from blank never accumulates: the move gate ignores
        // anchorless sessions (`session.anchor === null`), so `moved`
        // stays false and the up is a click — it clears too.
        let _ = hook.on_pointer_down(&env, &pointer(0, None));
        let mut moving = pointer(0, None);
        moving.cell_from_point = Some(coord(5, "id"));
        hook.on_pointer_move(&moving);
        assert!(!hook.dragging(), "an anchorless session never drags");
        assert_eq!(hook.on_pointer_up(&env, &moving), Some(empty()));
    }

    #[test]
    fn a_cancel_commits_the_last_hit_without_coordinates() {
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let _ = hook.on_pointer_down(&env, &pointer(0, Some(coord(1, "id"))));
        let mut moving = pointer(0, None);
        moving.cell_from_point = Some(coord(3, "name"));
        hook.on_pointer_move(&moving);
        // pointercancel: commit=true, no coordinates — the focus is the
        // session's last hit.
        let answer = hook.on_pointer_cancel(&env, &pointer(0, None));
        assert_eq!(answer, Some(cell_selection((1, "id"), (3, "name"))));
    }

    #[test]
    fn the_additive_click_toggles_the_extra_range() {
        let columns = columns();
        let selection = cell_selection((1, "id"), (2, "name"));
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let mut down = pointer(0, Some(coord(8, "notes")));
        down.meta_key = true;
        let _ = hook.on_pointer_down(&env, &down);
        let answer = hook.on_pointer_up(&env, &down);
        let next = answer.expect("a write");
        assert_eq!(next.extra_ranges.len(), 1, "the cell joined as an extra range");
        assert_eq!(next.anchor.as_ref(), selection.anchor.as_ref(), "the main region stays");
        // Toggling the same cell again drops it.
        let env_next = env_for(&columns, &next);
        let _ = hook.on_pointer_down(&env_next, &down);
        let answer = hook.on_pointer_up(&env_next, &down);
        let answer = answer.expect("a write");
        assert_eq!(answer.extra_ranges.len(), 0, "the extra range dropped");
        // Clicking inside the main region is a no-op write-back of the
        // same selection (nothing changes).
        let mut inside = pointer(0, Some(coord(1, "id")));
        inside.meta_key = true;
        let env = env_for(&columns, &selection);
        let _ = hook.on_pointer_down(&env, &inside);
        assert_eq!(hook.on_pointer_up(&env, &inside), Some(selection.clone()));
    }

    #[test]
    fn the_shift_click_extends_from_the_existing_anchor() {
        let columns = columns();
        let selection = cell_selection((1, "id"), (1, "id"));
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let mut down = pointer(0, Some(coord(5, "notes")));
        down.shift_key = true;
        let _ = hook.on_pointer_down(&env, &down);
        let answer = hook.on_pointer_up(&env, &down);
        // The EXISTING anchor stays; the clicked cell is the new focus.
        assert_eq!(answer, Some(cell_selection((1, "id"), (5, "notes"))));
        // Shift+click in row mode (or without an anchor) is a plain
        // single cell.
        let mut row_mode = selection.clone();
        row_mode.mode = GridSelectionMode::Row;
        row_mode.anchor = None;
        row_mode.focus = None;
        let env_row = env_for(&columns, &row_mode);
        let _ = hook.on_pointer_down(&env_row, &down);
        assert_eq!(hook.on_pointer_up(&env_row, &down), Some(cell_selection((5, "notes"), (5, "notes"))));
    }

    #[test]
    fn the_row_gutter_hands_off_to_the_whole_row_path() {
        let columns = columns();
        let selection = empty();
        let mut env = env_for(&columns, &selection);
        env.has_row_gutter = true;
        let mut hook = GridSelectionHook::new();
        let mut gutter = pointer(0, None);
        gutter.target_row_header = Some(4);
        gutter.meta_key = true;
        assert_eq!(
            hook.on_pointer_down(&env, &gutter),
            PointerDownEffect::RowGutterSelect { row_index: 4, multi: true, range: false }
        );
        assert!(hook.session().is_none(), "the gutter starts no session");
        let mut range = pointer(0, None);
        range.target_row_header = Some(2);
        range.shift_key = true;
        assert_eq!(
            hook.on_pointer_down(&env, &range),
            PointerDownEffect::RowGutterSelect { row_index: 2, multi: false, range: true }
        );
    }

    #[test]
    fn the_gates_refuse_secondary_buttons_editable_targets_and_stale_rows() {
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        // A secondary button.
        assert_eq!(hook.on_pointer_down(&env, &pointer(2, Some(coord(1, "id")))), PointerDownEffect::None);
        // An editable target (an open editor's input).
        let mut editable = pointer(0, Some(coord(1, "id")));
        editable.target_editable = true;
        assert_eq!(hook.on_pointer_down(&env, &editable), PointerDownEffect::None);
        // A stale row outside the page: the session starts BLANK (the
        // click clears, the drag is ignored).
        let mut stale = pointer(0, Some(coord(99, "id")));
        stale.pointer_id = 3;
        assert_eq!(hook.on_pointer_down(&env, &stale), PointerDownEffect::SessionStarted);
        assert!(hook.session().map(|s| s.anchor.is_none()).unwrap_or(false));
        // Disabled: nothing at all.
        let mut disabled = env_for(&columns, &selection);
        disabled.enabled = false;
        assert_eq!(hook.on_pointer_down(&disabled, &pointer(0, Some(coord(1, "id")))), PointerDownEffect::None);
    }

    #[test]
    fn a_foreign_pointer_up_does_not_end_the_session() {
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let _ = hook.on_pointer_down(&env, &pointer(0, Some(coord(1, "id"))));
        let mut other = pointer(0, Some(coord(5, "notes")));
        other.pointer_id = 42;
        assert_eq!(hook.on_pointer_up(&env, &other), None);
        assert!(hook.session().is_some(), "the session survives a foreign pointer");
    }

    #[test]
    fn escape_cancels_the_drag_first_then_clears() {
        let columns = columns();
        let selection = cell_selection((1, "id"), (2, "name"));
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        // A live drag session, then Escape: the session cancels FIRST.
        let _ = hook.on_pointer_down(&env, &pointer(0, Some(coord(1, "id"))));
        let mut moving = pointer(0, None);
        moving.cell_from_point = Some(coord(5, "notes"));
        hook.on_pointer_move(&moving);
        assert!(hook.dragging());
        let escape = GridKeyDownEvent { key: "Escape".to_owned(), ..Default::default() };
        assert_eq!(hook.on_key_down(&env, &escape, false), Some(empty()));
        assert!(hook.session().is_none());
        assert!(!hook.dragging());
        // The next pointerup (late) has no session to commit.
        assert_eq!(hook.on_pointer_up(&env, &moving), None);
    }

    #[test]
    fn escape_is_gated_by_the_editor_composition_and_the_nav_hook() {
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let escape = GridKeyDownEvent { key: "Escape".to_owned(), ..Default::default() };
        // The nav hook claimed it (defaultPrevented).
        assert_eq!(hook.on_key_down(&env, &escape, true), None);
        // An editor is open.
        let mut editing = env_for(&columns, &selection);
        editing.editing_cell = true;
        assert_eq!(hook.on_key_down(&editing, &escape, false), None);
        // A composition owns every key.
        let mut composing = GridKeyDownEvent {
            key: "Escape".to_owned(),
            native_is_composing: true,
            ..Default::default()
        };
        assert_eq!(hook.on_key_down(&env, &composing, false), None);
        composing.native_is_composing = false;
        composing.native_key_code = 229;
        assert_eq!(hook.on_key_down(&env, &composing, false), None);
        // Other keys are not the selection handler's.
        let other = GridKeyDownEvent { key: "a".to_owned(), ..Default::default() };
        assert_eq!(hook.on_key_down(&env, &other, false), None);
        // None-mode with no session: nothing to clear.
        assert_eq!(hook.on_key_down(&env, &escape, false), None, "mode none, no session");
    }

    #[test]
    fn escape_clears_even_in_none_mode_while_a_drag_is_live() {
        // The store reports the PRE-drag mode ('none'): the hadSession
        // rule keeps the clear alive — without it the early return
        // would leave the cancelled session's next pointerup able to
        // commit a region the user just cancelled.
        let columns = columns();
        let selection = empty();
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let _ = hook.on_pointer_down(&env, &pointer(0, Some(coord(1, "id"))));
        let mut moving = pointer(0, None);
        moving.cell_from_point = Some(coord(4, "notes"));
        hook.on_pointer_move(&moving);
        assert!(hook.dragging());
        let escape = GridKeyDownEvent { key: "Escape".to_owned(), ..Default::default() };
        assert_eq!(
            hook.on_key_down(&env, &escape, false),
            Some(empty()),
            "a live session clears even when the store still reports none mode"
        );
        assert!(hook.session().is_none());
        // The late pointerup has nothing to commit.
        assert_eq!(hook.on_pointer_up(&env, &moving), None);
    }

    #[test]
    fn blur_ends_the_session_but_keeps_the_selection() {
        let columns = columns();
        let selection = cell_selection((1, "id"), (2, "name"));
        let env = env_for(&columns, &selection);
        let mut hook = GridSelectionHook::new();
        let _ = hook.on_pointer_down(&env, &pointer(0, Some(coord(1, "id"))));
        let mut moving = pointer(0, None);
        moving.cell_from_point = Some(coord(4, "notes"));
        hook.on_pointer_move(&moving);
        assert!(hook.dragging());
        hook.on_blur();
        assert!(hook.session().is_none());
        assert!(!hook.dragging());
    }

    #[test]
    fn the_membership_and_id_projections() {
        let columns = columns();
        let selection = cell_selection((1, "id"), (2, "notes"));
        let env = env_for(&columns, &selection);
        let hook = GridSelectionHook::new();
        assert!(hook.is_cell_selected(&env, 2, "name"), "inside the region");
        assert!(!hook.is_cell_selected(&env, 3, "name"), "past the region's rows");
        assert!(!hook.is_cell_selected(&env, 0, "id"), "above the region's rows");
        assert!(hook.is_cell_selected(&env, 1, "notes"), "the region's corner is selected");
        assert_eq!(
            hook.active_descendant_id(&env, "grid"),
            Some("grid-r2c2".to_owned()),
            "the focus cell's row and its column index"
        );
        assert_eq!(hook.focus_key(&env), Some("2:notes".to_owned()));
        // Row/none modes have no focus cell.
        let mut row_mode = selection.clone();
        row_mode.mode = GridSelectionMode::Row;
        row_mode.anchor = None;
        row_mode.focus = None;
        let env = env_for(&columns, &row_mode);
        assert_eq!(hook.active_descendant_id(&env, "grid"), None);
        assert_eq!(hook.focus_key(&env), None);
        // An unknown focus column has no descendant id.
        let mut lost = cell_selection((1, "id"), (1, "gone"));
        lost.focus = Some(coord(1, "gone"));
        let env = env_for(&columns, &lost);
        assert_eq!(hook.active_descendant_id(&env, "grid"), None);
    }
}
