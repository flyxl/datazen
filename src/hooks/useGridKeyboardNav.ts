/**
 * React wiring of the grid keyboard (DB-02).
 *
 * The hook owns no state: the store keeps the selection, the component keeps
 * the scroll position, and this hook only translates a key event into one of
 * the callbacks that already exist. It adds **no** store action (02 §3.3) and
 * registers **no** listener of its own — the grid has exactly one keyboard
 * entry point, `useGridSelection`'s `surfaceProps.onKeyDown`.
 */
import { useCallback, useMemo, useRef, type KeyboardEvent } from 'react';
import { isComposingEvent, isEditableTarget } from '../lib/gridEventGuards';
import {
  EMPTY_ROW_MODE_SELECTION,
  clampGridCursor,
  readGridKeyIntent,
  resolveGridKeyCommand,
  resolveKeyboardCursor,
  toGridKeyEventLike,
  type GridKeyCommand,
  type GridKeyEventLike,
  type GridKeyboardContext,
} from '../lib/gridKeyboard';
import type { CellCoord, GridSelection } from '../stores/tableData/gridSelection';

/** The actions 02 needs. All of them exist already; 02 adds none. */
export interface GridKeyboardDispatch {
  /** 01's controlled write-back (`DataTableProps.onGridSelectionChange`). */
  onGridSelectionChange: (next: GridSelection) => void;
  /** The existing double-click entry; `Enter`/`F2` reuse it. */
  onCellDoubleClick: (rowIndex: number, columnName: string) => void;
  /** 01's row-number slot path (the existing `selectRow`); row mode moves with it. */
  onRowGutterSelect: (rowIndex: number, opts: { multi: boolean; range: boolean }) => void;
  /**
   * The existing "select every row" action (01's `toggleSelectAll`).
   *
   * Optional: a grid without a select-all path has nothing to select, so the
   * key is claimed without a state change instead of calling a fake action.
   */
  onToggleSelectAll?: () => void;
  /** Volume 10's hook point: scroll the cursor cell into view. */
  onRequestScrollIntoView?: (coord: CellCoord) => void;
}

export interface UseGridKeyboardNavOptions extends GridKeyboardDispatch {
  columnOrder: readonly string[];
  rowCount: number;
  /** The controlled selection — the store is the only truth. */
  selection: GridSelection;
  /** `TableState.lastSelectedIndex`, read-only: the anchor of a row selection. */
  lastSelectedIndex: number | null;
  /** An editor is open (`editingCell !== null`). */
  editing: boolean;
  /** Visible rows of the viewport; the caller injects it, the hook never measures. */
  viewportRowCount: number;
  /** Off by default: a grid without it intercepts no key at all. */
  enabled?: boolean;
}

export interface UseGridKeyboardNavResult {
  /** Called **first** on the grid's single keydown; it decides what happens. */
  handleKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  /** The cell that carries `data-dt-active="true"`. */
  activeCoord: CellCoord | null;
  /** The row that carries the keyboard cursor (row mode uses the row anchor). */
  activeRowIndex: number | null;
}

export function useGridKeyboardNav(
  options: UseGridKeyboardNavOptions,
): UseGridKeyboardNavResult {
  const {
    columnOrder,
    rowCount,
    selection,
    lastSelectedIndex,
    editing,
    viewportRowCount,
    enabled = true,
    onGridSelectionChange,
    onCellDoubleClick,
    onRowGutterSelect,
    onToggleSelectAll,
    onRequestScrollIntoView,
  } = options;

  /**
   * The cursor of the last cell the user was on.
   *
   * It is **not** in the store: it does not influence the rendering, so a store
   * field would only add one more place that has to be cleared when the data
   * is fetched again (02 §K-9). It is the landing spot of the next arrow key
   * after `Escape` cleared the selection (02 §4.7).
   */
  const rememberedRef = useRef<CellCoord | null>(null);

  const derivedCursor = useMemo(
    () => resolveKeyboardCursor(selection, lastSelectedIndex, columnOrder),
    [columnOrder, lastSelectedIndex, selection],
  );
  // The remembered cursor follows the live one. It is written during the render
  // (not in an effect) so that the very next key press already sees it: in
  // `none` mode the live cursor is gone, and only the remembered one is left.
  if (derivedCursor !== null) rememberedRef.current = derivedCursor;

  const cursor = useMemo(
    () => clampGridCursor(derivedCursor ?? rememberedRef.current, rowCount, columnOrder),
    [columnOrder, derivedCursor, rowCount],
  );

  const context = useMemo<GridKeyboardContext>(
    () => ({
      columnOrder,
      rowCount,
      mode: selection.mode,
      cursor,
      viewportRowCount,
      editing,
    }),
    [columnOrder, cursor, editing, rowCount, selection.mode, viewportRowCount],
  );

  const scrollIntoView = useCallback(
    (coord: CellCoord | null) => {
      if (coord === null) return;
      onRequestScrollIntoView?.(coord);
    },
    [onRequestScrollIntoView],
  );

  const applyCommand = useCallback(
    (command: GridKeyCommand, event: KeyboardEvent<HTMLDivElement>) => {
      switch (command.kind) {
        case 'pass':
          // Not the grid's key: neither preventDefault nor stopPropagation
          // (02 §3.2) — the clipboard, the undo and the browser keep them.
          return;
        case 'ignored':
          // The grid claims the key name but has nothing to do here. The
          // browser default is left alone on purpose (02 §3.2).
          return;
        case 'consume':
          // Claimed without a state change: `Tab` must not reach the
          // row-number slot, `Cmd+Home` must not scroll the page.
          event.preventDefault();
          return;
        case 'set-selection':
          event.preventDefault();
          onGridSelectionChange(command.selection);
          scrollIntoView(command.selection.focus);
          return;
        case 'row-select':
          event.preventDefault();
          onRowGutterSelect(command.rowIndex, command.opts);
          return;
        case 'select-all-rows':
          event.preventDefault();
          // The existing action owns both the row set and the mode (01's
          // `toggleSelectAll` writes `mode: 'row'` together with the rows).
          // Writing a row-mode selection afterwards would clear the rows the
          // action just selected, so the action comes alone.
          if (onToggleSelectAll) {
            onToggleSelectAll();
            return;
          }
          onGridSelectionChange(EMPTY_ROW_MODE_SELECTION);
          return;
        case 'begin-edit':
          event.preventDefault();
          onCellDoubleClick(command.coord.rowIndex, command.coord.columnName);
          return;
      }
    },
    [onCellDoubleClick, onGridSelectionChange, onRowGutterSelect, onToggleSelectAll, scrollIntoView],
  );

  const handleKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      if (!enabled) return;
      // The editor owns the keyboard while it is open — including the keys that
      // bubble up from its input (02 §4.3). Without this the grid would steal
      // the arrow keys while the user types.
      if (isEditableTarget(event.target)) return;
      const projection = toGridKeyEventLike(event);
      const keyEvent: GridKeyEventLike = isComposingEvent(event)
        ? { ...projection, isComposing: true }
        : projection;
      const intent = readGridKeyIntent(keyEvent, context);
      const command = resolveGridKeyCommand(intent, { ...context, extend: keyEvent.shiftKey }, selection);
      applyCommand(command, event);
    },
    [applyCommand, context, enabled, selection],
  );

  /**
   * The cell that carries `data-dt-active`. Only the cell mode has one: the
   * row mode highlights whole rows (the highlight *is* the cursor) and the
   * `none` mode has no cursor at all — the remembered one is only the landing
   * spot of the next key.
   */
  const activeCoord = enabled && selection.mode === 'cell' ? cursor : null;
  const activeRowIndex =
    enabled && selection.mode === 'row' ? (cursor?.rowIndex ?? null) : null;

  return { handleKeyDown, activeCoord, activeRowIndex };
}
