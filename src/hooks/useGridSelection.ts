import { useCallback, useId, useRef, useState } from 'react';
import { resolveDataTableCellFromEvent } from '../lib/dataTableContextMenu';
import { isComposingEvent, isEditableTarget } from '../lib/gridEventGuards';
import {
  EMPTY_GRID_SELECTION,
  columnIndexFromCoord,
  gridSelectionContains,
  selectionFromAnchorFocus,
  toggleExtraRange,
  type CellCoord,
  type GridSelection,
} from '../stores/tableData/gridSelection';

export interface UseGridSelectionOptions {
  /** Displayed column names, in render order (normally `DataTable`'s `columns`). */
  columnOrder: readonly string[];
  rowCount: number;
  /** Controlled value: the store owns the selection, this hook keeps no copy. */
  selection: GridSelection;
  /** Called once per finished interaction — never on every pointermove. */
  onChange: (next: GridSelection) => void;
  /** Grid scroll container; the pointer handlers are spread onto it. */
  scrollElement: HTMLElement | null;
  /** Row gutter click handed to the existing whole-row selection path. */
  onRowGutterSelect?: (rowIndex: number, opts: { multi: boolean; range: boolean }) => void;
  /** Cell selection switch; defaults to true. Read-only grids stay selectable. */
  enabled?: boolean;
  /** While a cell is being edited, Escape belongs to the editor. */
  editingCell: { row: number; col: string } | null;
  /**
   * Prefix for the `aria-activedescendant` id. The component that renders the cells has to
   * build the very same ids, so it owns the prefix and passes it in; without it the hook
   * falls back to its own `useId()` value.
   */
  idPrefix?: string;
}

export interface UseGridSelectionResult {
  /** Spread onto the grid scroll container: `<div ref={setScrollEl} {...surfaceProps} />`. */
  surfaceProps: {
    onPointerDown: (event: React.PointerEvent<HTMLDivElement>) => void;
    onPointerMove: (event: React.PointerEvent<HTMLDivElement>) => void;
    onPointerUp: (event: React.PointerEvent<HTMLDivElement>) => void;
    onPointerCancel: (event: React.PointerEvent<HTMLDivElement>) => void;
    onKeyDown: (event: React.KeyboardEvent<HTMLDivElement>) => void;
    onFocus: () => void;
    onBlur: () => void;
  };
  /** True while a drag is in progress (drives `select-none` and journey assertions). */
  dragging: boolean;
  /** Render-time membership test; goes through the pure helpers, never through the DOM. */
  isCellSelected: (rowIndex: number, columnName: string) => boolean;
  /** `aria-activedescendant` target for the focus cell. */
  activeDescendantId: string | null;
  /** Stable key of the focus cell, so virtual scrolling can keep it mounted. */
  focusKey: string | null;
}

/**
 * One pointer interaction. `anchor === null` means it started on the blank surface; `focus`
 * keeps the last cell the pointer was over so a drag leaving the grid keeps its last hit.
 */
interface DragSession {
  pointerId: number;
  anchor: CellCoord | null;
  focus: CellCoord | null;
  moved: boolean;
  additive: boolean;
  extended: boolean;
  /** Element that took pointer capture, needed to hand it back when the session ends. */
  captureTarget: Element | null;
}

/** Hand pointer capture back; jsdom has no pointer capture API, so both members are probed. */
function releasePointerCapture(target: Element | null, pointerId: number): void {
  if (target === null || typeof target.releasePointerCapture !== 'function') return;
  if (typeof target.hasPointerCapture === 'function' && !target.hasPointerCapture(pointerId)) {
    return;
  }
  try {
    target.releasePointerCapture(pointerId);
  } catch {
    // The capture may already have been released by the browser (pointercancel does it).
  }
}

/**
 * Take pointer capture, tolerating a pointer id the browser does not know.
 *
 * A synthetic `PointerEvent` (the E2E suites dispatch those) has no active pointer, and
 * `setPointerCapture` throws `NotFoundError` for it. An exception here would abort the whole
 * pointerdown handler, so the drag session would never start and the click would be lost.
 */
function capturePointer(target: Element | null, pointerId: number): void {
  if (target === null || typeof target.setPointerCapture !== 'function') return;
  try {
    target.setPointerCapture(pointerId);
  } catch (error) {
    if (error instanceof DOMException && error.name === 'NotFoundError') {
      // No active pointer for this id (a synthetic PointerEvent has none): the drag still
      // works, only the capture is missing.
      return;
    }
    // Anything else is unexpected. Report it instead of swallowing it — but do NOT
    // rethrow: that would abort the pointerdown handler and turn a degraded drag into a
    // completely dead click.
    console.warn('[grid] pointer capture failed', error);
  }
}

/**
 * Row index of the row gutter the pointer landed on, if any. The gutter is a
 * `<div role="rowheader">` around a real button (00-contracts §4.4), so the row is resolved
 * from the header itself or from the first data cell of the same row.
 */
function rowIndexFromRowHeader(target: EventTarget | null): number | null {
  if (!(target instanceof Element)) return null;
  const header = target.closest('[role="rowheader"]');
  if (!header) return null;
  const read = (element: Element | null): number | null => {
    const raw = element?.getAttribute('data-dt-row') ?? null;
    if (raw === null) return null;
    const rowIndex = Number(raw);
    return Number.isInteger(rowIndex) && rowIndex >= 0 ? rowIndex : null;
  };
  const own = read(header);
  if (own !== null) return own;
  return read(header.parentElement?.querySelector('[data-dt-row]') ?? null);
}

/** Resolve the cell under a viewport point; jsdom has no layout engine, hence the guard. */
function cellFromPoint(clientX: number, clientY: number): CellCoord | null {
  if (typeof document.elementFromPoint !== 'function') return null;
  return resolveDataTableCellFromEvent(document.elementFromPoint(clientX, clientY));
}

function sameCoord(left: CellCoord | null, right: CellCoord | null): boolean {
  if (left === null || right === null) return left === right;
  return left.rowIndex === right.rowIndex && left.columnName === right.columnName;
}

/**
 * Translate pointer/keyboard events into `GridSelection` transitions (DB-01).
 *
 * The store stays the only source of truth: this hook keeps just the in-flight drag session
 * (a render optimisation, not a second copy) and calls `onChange` once per interaction.
 */
export function useGridSelection(options: UseGridSelectionOptions): UseGridSelectionResult {
  const {
    columnOrder,
    rowCount,
    selection,
    onChange,
    scrollElement,
    onRowGutterSelect,
    enabled = true,
    editingCell,
    idPrefix: idPrefixOption,
  } = options;
  const internalIdPrefix = useId();
  const idPrefix = idPrefixOption ?? internalIdPrefix;
  const sessionRef = useRef<DragSession | null>(null);
  const [dragging, setDragging] = useState(false);

  /** Drop the in-flight drag: forget the session, hand back capture, leave `dragging` false. */
  const clearSession = useCallback(() => {
    const session = sessionRef.current;
    if (session !== null) releasePointerCapture(session.captureTarget, session.pointerId);
    sessionRef.current = null;
    setDragging(false);
  }, []);

  const finishSession = useCallback(
    (pointerId: number, commit: boolean, clientX?: number, clientY?: number) => {
      const session = sessionRef.current;
      if (!session || session.pointerId !== pointerId) return;
      releasePointerCapture(session.captureTarget, session.pointerId);
      sessionRef.current = null;
      setDragging(false);
      if (!commit) return;
      const anchor = session.anchor;
      if (anchor === null) {
        // Pointer went down on the blank surface: a click clears, a drag is ignored.
        if (!session.moved) onChange(EMPTY_GRID_SELECTION);
        return;
      }
      const pointerCell =
        clientX === undefined || clientY === undefined
          ? null
          : cellFromPoint(clientX, clientY);
      const focus = pointerCell ?? session.focus ?? anchor;
      if (session.moved) {
        onChange(selectionFromAnchorFocus(anchor, focus, 'cell'));
        return;
      }
      if (session.additive) {
        onChange(toggleExtraRange(selection, anchor, columnOrder));
        return;
      }
      if (session.extended && selection.mode === 'cell' && selection.anchor !== null) {
        onChange(selectionFromAnchorFocus(selection.anchor, anchor, 'cell'));
        return;
      }
      onChange(selectionFromAnchorFocus(anchor, anchor, 'cell'));
    },
    [columnOrder, onChange, selection],
  );

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!enabled || event.button !== 0) return;
      if (isEditableTarget(event.target)) return;
      const gutterRow = rowIndexFromRowHeader(event.target);
      if (gutterRow !== null) {
        onRowGutterSelect?.(gutterRow, {
          multi: event.metaKey || event.ctrlKey,
          range: event.shiftKey,
        });
        return;
      }
      const resolved = resolveDataTableCellFromEvent(event.target);
      // A row index outside the current page is stale DOM from an uncommitted render.
      const cell = resolved !== null && resolved.rowIndex < rowCount ? resolved : null;
      const extended = cell !== null && event.shiftKey && selection.mode === 'cell';
      const current = event.currentTarget;
      const captureTarget = typeof current.setPointerCapture === 'function' ? current : null;
      capturePointer(captureTarget, event.pointerId);
      sessionRef.current = {
        pointerId: event.pointerId,
        anchor: cell,
        focus: cell,
        moved: false,
        additive: event.metaKey || event.ctrlKey,
        extended,
        captureTarget,
      };
      // Cell interactions move focus to the grid host; editor inputs are excluded above.
      scrollElement?.focus();
    },
    [enabled, onRowGutterSelect, rowCount, scrollElement, selection.mode],
  );

  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const session = sessionRef.current;
      if (!session || session.pointerId !== event.pointerId || session.anchor === null) return;
      const cell = cellFromPoint(event.clientX, event.clientY);
      if (!cell || sameCoord(cell, session.focus)) return;
      session.focus = cell;
      session.moved = true;
      setDragging(true);
    },
    [],
  );

  const onPointerUp = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      finishSession(event.pointerId, true, event.clientX, event.clientY);
    },
    [finishSession],
  );

  const onPointerCancel = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      finishSession(event.pointerId, true);
    },
    [finishSession],
  );

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      // 01-selection §4.5: an IME composition owns every key, Escape included.
      if (isComposingEvent(event)) return;
      if (event.key !== 'Escape') return;
      if (editingCell !== null) return;
      // Cancel the in-flight drag **first**: the store still reports the pre-drag mode, so
      // clearing the selection alone would leave the session alive and the next pointerup
      // would commit the region the user just cancelled.
      const hadSession = sessionRef.current !== null;
      clearSession();
      if (!hadSession && selection.mode === 'none') return;
      onChange(EMPTY_GRID_SELECTION);
    },
    [clearSession, editingCell, onChange, selection.mode],
  );

  // Focus leaving the grid must not clear the selection: the user may be on the way to a
  // toolbar action. Only the in-flight drag session ends.
  const onFocus = useCallback(() => {}, []);
  const onBlur = useCallback(() => {
    clearSession();
  }, [clearSession]);

  const isCellSelected = useCallback(
    (rowIndex: number, columnName: string) =>
      gridSelectionContains(selection, { rowIndex, columnName }, columnOrder),
    [columnOrder, selection],
  );

  const focus = selection.mode === 'cell' ? selection.focus : null;
  const focusColumn = focus === null ? -1 : columnIndexFromCoord(focus, columnOrder);
  const activeDescendantId =
    focus === null || focusColumn < 0 ? null : `${idPrefix}-r${focus.rowIndex}c${focusColumn}`;
  const focusKey = focus === null ? null : `${focus.rowIndex}:${focus.columnName}`;

  return {
    surfaceProps: {
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel,
      onKeyDown,
      onFocus,
      onBlur,
    },
    dragging,
    isCellSelected,
    activeDescendantId,
    focusKey,
  };
}
