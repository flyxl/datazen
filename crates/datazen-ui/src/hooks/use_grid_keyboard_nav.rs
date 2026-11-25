//! Port of `packages/frontend/src/hooks/useGridKeyboardNav.ts` (T313) —
//! the React wiring of the grid keyboard (DB-02).
//!
//! The hook owns no state: the store keeps the selection, the component
//! keeps the scroll position, and this hook only translates a key event
//! into one of the callbacks that already exist. It adds **no** store
//! action (02 §3.3) and registers **no** listener of its own — the grid
//! has exactly one keyboard entry point, `useGridSelection`'s
//! `surfaceProps.onKeyDown`.
//!
//! ## What maps where
//!
//! - **The hook is its remembered cursor** (the T294 env convention):
//!   React's single `useRef` becomes [`GridKeyboardNavHook`], and every
//!   `useCallback`/`useMemo` becomes a method taking the per-frame
//!   [`GridKeyboardNavEnv`] — in immediate mode there is no closure to
//!   capture, the environment is re-supplied on every call.
//! - The render half is [`GridKeyboardNavHook::frame`]: the derived
//!   cursor, the remembered-cursor follow (written during the render,
//!   so the very next key press already sees it), the clamp, and the
//!   two active-cell projections (`activeCoord` /
//!   `activeRowIndex`).
//! - `handleKeyDown` is [`GridKeyboardNavHook::handle_key_down`]: the
//!   enabled gate, the editor-target gate, the event projection plus
//!   the composing flag, the intent, the command, the select-all
//!   dispatchability, and the dispatch — answered as one
//!   [`GridKeyboardNavEffect`] the host executes. `event.preventDefault()`
//!   is the effect's presence: every dispatched effect except the two
//!   idle ones carries it (02 §3.2 — `pass` and `ignored` never do).
//! - `isEditableTarget(e.target)` is the DOM-bound guard
//!   (`gridEventGuards.ts`, T525): the event projection carries
//!   [`GridKeyDownEvent::target_editable`] — the host knows whether the
//!   key came from an editor surface.
//! - `Cmd+A` without a select-all action is claimed and **nothing is
//!   written** — the grid never writes a row-mode selection of its own
//!   (02 §4.6: that write would clear the rows the action has just
//!   selected).
//! - The keyboard semantics themselves live in the T527 seam
//!   ([`crate::hooks::grid_keyboard_helpers`]); the selection types are
//!   the T041 mirrors.
//!
//! ## Keys
//!
//! No elements are painted here (the hook is the machine; the grid
//! surface paints). No `data-testid` of its own.

use crate::hooks::grid_keyboard_helpers::{
    GridKeyDownEvent, GridKeyCommand, GridKeyboardContext, clamp_grid_cursor,
    read_grid_key_intent, resolve_grid_key_command, resolve_keyboard_cursor,
    to_grid_key_event_like,
};
use crate::table_data_connection_state::{CellCoord, GridSelection, GridSelectionMode};

/// The hook's per-frame environment (React's `UseGridKeyboardNavOptions`
/// plus the two optional-action presences — in immediate mode the host
/// reports "the action exists" instead of passing a `vi.fn`).
pub struct GridKeyboardNavEnv<'a> {
    /// `columnOrder`: displayed column names in render order.
    pub column_order: &'a [String],
    /// `rowCount`.
    pub row_count: usize,
    /// The controlled selection — the store is the only truth.
    pub selection: &'a GridSelection,
    /// `lastSelectedIndex`, read-only: the anchor of a row selection.
    pub last_selected_index: Option<usize>,
    /// An editor is open (`editingCell !== null`).
    pub editing: bool,
    /// Visible rows of the viewport; the caller injects it, the hook
    /// never measures.
    pub viewport_row_count: usize,
    /// Off by default: a grid without it intercepts no key at all.
    pub enabled: bool,
    /// `typeof onToggleSelectAll === 'function'`.
    pub has_select_all: bool,
    /// `onRequestScrollIntoView` is supplied.
    pub has_scroll_into_view: bool,
}

impl<'a> GridKeyboardNavEnv<'a> {
    /// React's default: `enabled = true`, both optional actions absent.
    pub fn new(
        column_order: &'a [String],
        row_count: usize,
        selection: &'a GridSelection,
        last_selected_index: Option<usize>,
        editing: bool,
        viewport_row_count: usize,
    ) -> Self {
        Self {
            column_order,
            row_count,
            selection,
            last_selected_index,
            editing,
            viewport_row_count,
            enabled: true,
            has_select_all: false,
            has_scroll_into_view: false,
        }
    }
}

/// One dispatch the host executes. The variant's presence **is**
/// `event.preventDefault()` — the two idle answers (`Pass`/`Ignored`)
/// are `Idle` and never prevent (02 §3.2: the clipboard, the undo and
/// the browser keep their keys).
#[derive(Clone, Debug, PartialEq)]
pub enum GridKeyboardNavEffect {
    /// `pass` or `ignored`: neither preventDefault nor stopPropagation
    /// — or the hook declined the event entirely (disabled, or the
    /// editor target owns it).
    Idle,
    /// `consume`: claimed without a state change (`Tab` must not reach
    /// the row-number slot, `Cmd+Home` must not scroll the page).
    PreventDefaultOnly,
    /// `set-selection` + the focus-cell scroll request.
    SetSelection { selection: GridSelection, scroll_into_view: Option<CellCoord> },
    /// `row-select` + the first-column scroll request (the row
    /// highlight has to stay visible: the row mode carries no cell
    /// cursor, so the scroll request is the only thing that follows
    /// the row).
    RowSelect {
        row_index: usize,
        multi: bool,
        range: bool,
        scroll_into_view: Option<CellCoord>,
    },
    /// `select-all-rows`: the action owns both the row set and the mode
    /// (01's `toggleSelectAll` writes `mode: 'row'` together with the
    /// rows) — the grid writes no selection of its own here.
    ToggleSelectAll,
    /// `begin-edit`: the existing double-click entry.
    BeginEdit { coord: CellCoord },
}

/// The render half's answers (`activeCoord` / `activeRowIndex` plus the
/// cursor the caller may need).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridKeyboardNavFrame {
    /// The clamped cursor the command layer works with.
    pub cursor: Option<CellCoord>,
    /// The cell that carries `data-dt-active="true"` (cell mode only).
    pub active_coord: Option<CellCoord>,
    /// The row that carries the keyboard cursor (row mode uses the row
    /// anchor).
    pub active_row_index: Option<usize>,
}

/// The hook: the remembered cursor of the last cell the user was on.
///
/// It is **not** in the store: it does not influence the rendering, so
/// a store field would only add one more place that has to be cleared
/// when the data is fetched again (02 §K-9). It is the landing spot of
/// the next arrow key after `Escape` cleared the selection (02 §4.7).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridKeyboardNavHook {
    /// `rememberedRef.current`.
    remembered: Option<CellCoord>,
}

impl GridKeyboardNavHook {
    /// `useGridKeyboardNav()`: nothing remembered yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The render half: the derived cursor, the remembered-cursor
    /// follow (the remembered cursor follows the live one — written
    /// during the render, not in an effect, so that the very next key
    /// press already sees it), the clamp, and the two active
    /// projections.
    pub fn frame(&mut self, env: &GridKeyboardNavEnv) -> GridKeyboardNavFrame {
        let derived =
            resolve_keyboard_cursor(env.selection, env.last_selected_index, env.column_order);
        // In `none` mode the live cursor is gone, and only the
        // remembered one is left.
        if derived.is_some() {
            self.remembered = derived.clone();
        }
        let cursor =
            clamp_grid_cursor(derived.or_else(|| self.remembered.clone()).as_ref(), env.row_count, env.column_order);
        let active_coord = (env.enabled && env.selection.mode == GridSelectionMode::Cell)
            .then(|| cursor.clone())
            .flatten();
        let active_row_index = (env.enabled && env.selection.mode == GridSelectionMode::Row)
            .then(|| cursor.as_ref().map(|c| c.row_index))
            .flatten();
        GridKeyboardNavFrame { cursor, active_coord, active_row_index }
    }

    /// The remembered cursor (the landing spot after `Escape`).
    pub fn remembered(&self) -> Option<&CellCoord> {
        self.remembered.as_ref()
    }

    /// `handleKeyDown`: the grid's single keyboard entry point. The
    /// event is the host projection (the T341 `ShortcutEvent`
    /// convention — no DOM handle crosses the seam).
    pub fn handle_key_down(
        &mut self,
        env: &GridKeyboardNavEnv,
        event: &GridKeyDownEvent,
    ) -> GridKeyboardNavEffect {
        if !env.enabled {
            return GridKeyboardNavEffect::Idle;
        }
        // The editor owns the keyboard while it is open — including the
        // keys that bubble up from its input (02 §4.3). Without this
        // the grid would steal the arrow keys while the user types.
        if event.target_editable {
            return GridKeyboardNavEffect::Idle;
        }
        // The render half ran first: the remembered cursor is current.
        let frame = self.frame(env);
        let projection = to_grid_key_event_like(event);
        // `isComposingEvent` forces the projection's flag when the full
        // (synthetic-level) event reports a composition — the projection
        // already carries the native signal, so the two agree.
        let context = GridKeyboardContext {
            column_order: env.column_order,
            row_count: env.row_count,
            mode: env.selection.mode,
            cursor: frame.cursor,
            viewport_row_count: env.viewport_row_count,
            editing: env.editing,
            extend: projection.shift_key,
        };
        let intent = read_grid_key_intent(&projection, &context);
        let mut command =
            resolve_grid_key_command(&intent, &context, env.selection);
        // `Cmd+A` is the only command that needs an action the grid may
        // not have: a grid without a select-all path has nothing to
        // select. The key is then claimed and **nothing is written** —
        // the grid never writes a row-mode selection of its own
        // (02 §4.6).
        if matches!(command, GridKeyCommand::SelectAllRows) && !env.has_select_all {
            command = GridKeyCommand::Ignored;
        }
        self.apply(env, command)
    }

    /// `applyCommand`: the command → the effect the host executes.
    fn apply(&self, env: &GridKeyboardNavEnv, command: GridKeyCommand) -> GridKeyboardNavEffect {
        match command {
            // Not the grid's key: neither preventDefault nor
            // stopPropagation (02 §3.2) — the clipboard, the undo and
            // the browser keep them.
            GridKeyCommand::Pass => GridKeyboardNavEffect::Idle,
            // The grid claims the key name but has nothing to do here.
            // The browser default is left alone on purpose (02 §3.2).
            GridKeyCommand::Ignored => GridKeyboardNavEffect::Idle,
            // Claimed without a state change: `Tab` must not reach the
            // row-number slot, `Cmd+Home` must not scroll the page.
            GridKeyCommand::Consume => GridKeyboardNavEffect::PreventDefaultOnly,
            GridKeyCommand::SetSelection(selection) => {
                let scroll_into_view = env
                    .has_scroll_into_view
                    .then(|| selection.focus.clone())
                    .flatten();
                GridKeyboardNavEffect::SetSelection { selection, scroll_into_view }
            }
            GridKeyCommand::RowSelect { row_index, multi, range } => {
                // The row highlight has to stay visible: the row mode
                // carries no cell cursor, so the scroll request is the
                // only thing that follows the row (volume 10's hook
                // point).
                let scroll_into_view = env
                    .has_scroll_into_view
                    .then(|| {
                        env.column_order.first().map(|first| CellCoord {
                            row_index,
                            column_name: first.clone(),
                        })
                    })
                    .flatten();
                GridKeyboardNavEffect::RowSelect { row_index, multi, range, scroll_into_view }
            }
            GridKeyCommand::SelectAllRows => GridKeyboardNavEffect::ToggleSelectAll,
            GridKeyCommand::BeginEdit { coord } => GridKeyboardNavEffect::BeginEdit { coord },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table_data_connection_state::CellRange;

    fn columns() -> Vec<String> {
        vec!["id".to_owned(), "name".to_owned()]
    }

    fn empty_selection() -> GridSelection {
        GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::None,
        }
    }

    fn row_selection() -> GridSelection {
        GridSelection {
            anchor: None,
            focus: None,
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Row,
        }
    }

    fn cell_selection(row: usize, column: &str) -> GridSelection {
        let coord = CellCoord { row_index: row, column_name: column.to_owned() };
        GridSelection {
            anchor: Some(coord.clone()),
            focus: Some(coord),
            extra_ranges: Vec::new(),
            mode: GridSelectionMode::Cell,
        }
    }

    fn key_down(key: &str) -> GridKeyDownEvent {
        GridKeyDownEvent { key: key.to_owned(), ..Default::default() }
    }

    /// The options of a grid that has every action (the test file's
    /// `baseOptions`).
    fn full_env<'a>(
        columns: &'a [String],
        selection: &'a GridSelection,
        last_selected_index: Option<usize>,
    ) -> GridKeyboardNavEnv<'a> {
        let mut env = GridKeyboardNavEnv::new(columns, 10, selection, last_selected_index, false, 20);
        env.has_select_all = true;
        env.has_scroll_into_view = true;
        env
    }

    #[test]
    fn claims_cmd_a_without_writing_when_there_is_no_select_all_action() {
        let columns = columns();
        let selection = empty_selection();
        let mut env = full_env(&columns, &selection, None);
        // The action is missing — the grid must not fall back to a write
        // of its own (02 §4.6): a row-mode write would clear the
        // selected rows.
        env.has_select_all = false;
        let mut hook = GridKeyboardNavHook::new();
        let mut cmd_a = key_down("a");
        cmd_a.meta_key = true;
        // `fireEvent` reports `true` when the default was prevented: the
        // missing-action branch is `ignored`, never prevented.
        assert_eq!(hook.handle_key_down(&env, &cmd_a), GridKeyboardNavEffect::Idle);
    }

    #[test]
    fn hands_cmd_a_to_the_select_all_action_and_writes_no_selection() {
        let columns = columns();
        let selection = empty_selection();
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        let mut cmd_a = key_down("a");
        cmd_a.meta_key = true;
        // `fireEvent` reports `false`: the default WAS prevented.
        assert_eq!(hook.handle_key_down(&env, &cmd_a), GridKeyboardNavEffect::ToggleSelectAll);
    }

    #[test]
    fn asks_for_the_row_to_scroll_into_view_when_the_row_selection_moves() {
        let columns = columns();
        let selection = row_selection();
        let env = full_env(&columns, &selection, Some(3));
        let mut hook = GridKeyboardNavHook::new();
        let effect = hook.handle_key_down(&env, &key_down("ArrowDown"));
        assert_eq!(
            effect,
            GridKeyboardNavEffect::RowSelect {
                row_index: 4,
                multi: false,
                range: false,
                scroll_into_view: Some(CellCoord {
                    row_index: 4,
                    column_name: "id".to_owned(),
                }),
            },
            "the row highlight has to stay visible: the row mode has no cell cursor, so the scroll request is the only thing that follows the row"
        );
    }

    #[test]
    fn a_disabled_grid_intercepts_no_key_at_all() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let mut env = full_env(&columns, &selection, None);
        env.enabled = false;
        let mut hook = GridKeyboardNavHook::new();
        assert_eq!(hook.handle_key_down(&env, &key_down("ArrowDown")), GridKeyboardNavEffect::Idle);
        // The active projections are gone too.
        let frame = hook.frame(&env);
        assert!(frame.active_coord.is_none());
        assert!(frame.active_row_index.is_none());
    }

    #[test]
    fn the_editor_target_owns_the_keyboard() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        let mut from_editor = key_down("ArrowDown");
        from_editor.target_editable = true;
        assert_eq!(hook.handle_key_down(&env, &from_editor), GridKeyboardNavEffect::Idle);
    }

    #[test]
    fn a_set_selection_carries_the_focus_scroll_request() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        let effect = hook.handle_key_down(&env, &key_down("ArrowDown"));
        let GridKeyboardNavEffect::SetSelection { selection, scroll_into_view } = effect else {
            panic!("expected a selection");
        };
        assert_eq!(selection.focus.as_ref().map(|c| c.row_index), Some(4));
        assert_eq!(selection.mode, GridSelectionMode::Cell);
        assert!(selection.extra_ranges.is_empty(), "a plain move collapses the region");
        assert_eq!(
            scroll_into_view,
            selection.focus,
            "the scroll follows the focus cell"
        );
    }

    #[test]
    fn without_a_scroll_action_no_request_is_carried() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let mut env = full_env(&columns, &selection, None);
        env.has_scroll_into_view = false;
        let mut hook = GridKeyboardNavHook::new();
        let effect = hook.handle_key_down(&env, &key_down("ArrowDown"));
        let GridKeyboardNavEffect::SetSelection { scroll_into_view, .. } = effect else {
            panic!("expected a selection");
        };
        assert!(scroll_into_view.is_none());
    }

    #[test]
    fn tab_leaves_the_grid_claimed_but_idle() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        assert_eq!(hook.handle_key_down(&env, &key_down("Tab")), GridKeyboardNavEffect::PreventDefaultOnly);
    }

    #[test]
    fn enter_and_f2_reuse_the_double_click_entry() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        for key in ["Enter", "F2"] {
            assert_eq!(
                hook.handle_key_down(&env, &key_down(key)),
                GridKeyboardNavEffect::BeginEdit {
                    coord: CellCoord { row_index: 3, column_name: "name".to_owned() }
                }
            );
        }
        // Without a selection the key is claimed but idle.
        let none = empty_selection();
        let env = full_env(&columns, &none, None);
        assert_eq!(hook.handle_key_down(&env, &key_down("Enter")), GridKeyboardNavEffect::Idle);
    }

    #[test]
    fn the_active_projections_follow_the_mode() {
        let columns = columns();
        let mut hook = GridKeyboardNavHook::new();
        // Cell mode: the cursor cell is active.
        let cell = cell_selection(3, "name");
        let env = full_env(&columns, &cell, None);
        let frame = hook.frame(&env);
        assert_eq!(frame.active_coord.as_ref().map(|c| c.row_index), Some(3));
        assert!(frame.active_row_index.is_none());
        // Row mode: the row anchor is active, no cell.
        let row = row_selection();
        let env = full_env(&columns, &row, Some(5));
        let frame = hook.frame(&env);
        assert!(frame.active_coord.is_none());
        assert_eq!(frame.active_row_index, Some(5));
        // None mode: neither (the remembered cursor is only the landing
        // spot of the next key).
        let none = empty_selection();
        let env = full_env(&columns, &none, None);
        let frame = hook.frame(&env);
        assert!(frame.active_coord.is_none());
        assert!(frame.active_row_index.is_none());
    }

    #[test]
    fn the_remembered_cursor_is_the_landing_spot_after_escape() {
        let columns = columns();
        let mut hook = GridKeyboardNavHook::new();
        // A live cell selection remembers its cursor.
        let cell = cell_selection(3, "name");
        let env = full_env(&columns, &cell, None);
        hook.frame(&env);
        assert_eq!(hook.remembered().map(|c| c.row_index), Some(3));
        // Escape clears the selection (01's job): the live cursor is
        // gone, only the remembered one is left.
        let none = empty_selection();
        let env = full_env(&columns, &none, None);
        let frame = hook.frame(&env);
        assert!(frame.active_coord.is_none());
        assert_eq!(frame.cursor.as_ref().map(|c| c.row_index), Some(3), "still clamped-visible");
        // The next arrow re-enters AT the remembered cell — an arrow
        // never invents a move in `none` mode.
        let effect = hook.handle_key_down(&env, &key_down("ArrowDown"));
        let GridKeyboardNavEffect::SetSelection { selection, .. } = effect else {
            panic!("expected a selection");
        };
        assert_eq!(selection.focus.as_ref().map(|c| c.row_index), Some(3), "the remembered row");
        assert_eq!(
            selection.focus.as_ref().map(|c| c.column_name.clone()),
            Some("name".to_owned()),
            "the remembered column"
        );
    }

    #[test]
    fn an_ime_composition_is_never_the_grids_key() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        let mut composing = key_down("ArrowDown");
        composing.native_is_composing = true;
        assert_eq!(hook.handle_key_down(&env, &composing), GridKeyboardNavEffect::Idle);
        let mut legacy = key_down("process");
        legacy.native_key_code = 229;
        assert_eq!(hook.handle_key_down(&env, &legacy), GridKeyboardNavEffect::Idle);
    }

    #[test]
    fn an_open_editor_passes_every_key() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let mut env = full_env(&columns, &selection, None);
        env.editing = true;
        let mut hook = GridKeyboardNavHook::new();
        assert_eq!(hook.handle_key_down(&env, &key_down("ArrowDown")), GridKeyboardNavEffect::Idle);
    }

    #[test]
    fn the_clipboard_and_alt_keys_stay_the_browsers() {
        let columns = columns();
        let selection = cell_selection(3, "name");
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        for key in ["c", "x", "v", "z"] {
            let mut clipboard = key_down(key);
            clipboard.meta_key = true;
            assert_eq!(hook.handle_key_down(&env, &clipboard), GridKeyboardNavEffect::Idle);
        }
        let mut alt = key_down("ArrowDown");
        alt.alt_key = true;
        assert_eq!(hook.handle_key_down(&env, &alt), GridKeyboardNavEffect::Idle);
        assert_eq!(hook.handle_key_down(&env, &key_down("Escape")), GridKeyboardNavEffect::Idle);
    }

    #[test]
    fn an_extended_move_keeps_the_selections_extras() {
        let columns = columns();
        let extra = CellRange {
            start: CellCoord { row_index: 8, column_name: "id".to_owned() },
            end: CellCoord { row_index: 8, column_name: "id".to_owned() },
        };
        let mut selection = cell_selection(3, "name");
        selection.anchor = Some(CellCoord { row_index: 3, column_name: "id".to_owned() });
        selection.extra_ranges = vec![extra.clone()];
        let env = full_env(&columns, &selection, None);
        let mut hook = GridKeyboardNavHook::new();
        let mut shift_left = key_down("ArrowLeft");
        shift_left.shift_key = true;
        let effect = hook.handle_key_down(&env, &shift_left);
        let GridKeyboardNavEffect::SetSelection { selection, .. } = effect else {
            panic!("expected a selection");
        };
        assert_eq!(selection.anchor.as_ref().map(|c| c.column_name.clone()), Some("id".to_owned()));
        assert_eq!(selection.focus.as_ref().map(|c| c.column_name.clone()), Some("id".to_owned()), "name -> id under Shift");
        assert_eq!(selection.extra_ranges, vec![extra], "Shift keeps the extras");
    }
}
