import type { TableState } from './types';
import {
  EMPTY_GRID_SELECTION,
  reconcileGridSelection,
  selectionFromAnchorFocus,
  type CellCoord,
  type GridSelection,
} from './gridSelection';

/**
 * Row/column context a selection transition needs. Kept as one object so a caller cannot
 * silently swap the two arguments (both are "counts of things").
 */
export interface SelectionTransitionDeps {
  /** Displayed column names, in render order. */
  columnOrder: readonly string[];
  rowCount: number;
}

/** Row mode carries no geometry: the row highlight comes from `selectedRows` alone. */
export const ROW_MODE_GRID_SELECTION: GridSelection = {
  anchor: null,
  focus: null,
  extraRanges: [],
  mode: 'row',
};

/**
 * Resolve the displayed column order of a panel, which is what every range comparison uses.
 * `visibleColumns === null` means "all columns"; otherwise the original order is preserved.
 */
export function displayedColumnNames(
  columns: readonly { name: string }[],
  visibleColumns: readonly string[] | null,
): string[] {
  if (visibleColumns === null) return columns.map((column) => column.name);
  const visible = new Set(visibleColumns);
  return columns.filter((column) => visible.has(column.name)).map((column) => column.name);
}

/** Displayed column order of a whole panel slice (the order the grid actually renders). */
export function panelColumnOrder(ts: TableState): string[] {
  return displayedColumnNames(ts.columns, ts.visibleColumns);
}

/**
 * Whole-row selection, moved verbatim out of `tableDataStore.selectRow`, including the
 * "click the only selected row again clears it" rule that existing tests pin down.
 */
export function applyRowSelect(
  ts: TableState,
  index: number,
  opts: { multi?: boolean; range?: boolean } | undefined,
): Partial<TableState> {
  if (opts?.range && ts.lastSelectedIndex !== null) {
    const lo = Math.min(ts.lastSelectedIndex, index);
    const hi = Math.max(ts.lastSelectedIndex, index);
    const next = new Set(ts.selectedRows);
    for (let i = lo; i <= hi; i += 1) next.add(i);
    return { selectedRows: next, lastSelectedIndex: index };
  }
  if (opts?.multi) {
    const next = new Set(ts.selectedRows);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    return { selectedRows: next, lastSelectedIndex: index };
  }
  if (!opts?.range && ts.selectedRows.size === 1 && ts.selectedRows.has(index)) {
    return { selectedRows: new Set(), lastSelectedIndex: null };
  }
  return { selectedRows: new Set([index]), lastSelectedIndex: index };
}

/** Select-all / deselect-all over the current page, moved verbatim from the store. */
export function applyToggleSelectAll(ts: TableState): Partial<TableState> {
  const allSelected = ts.selectedRows.size === ts.rows.length && ts.rows.length > 0;
  if (allSelected) return { selectedRows: new Set(), lastSelectedIndex: null };
  const next = new Set<number>();
  for (let i = 0; i < ts.rows.length; i += 1) next.add(i);
  return { selectedRows: next, lastSelectedIndex: null };
}

/**
 * Enter cell mode. `selectedRows` must be empty in cell mode (00-contracts §4.2 rule 2),
 * otherwise the same rows would look selected twice with two different meanings.
 */
export function applyBeginCellSelection(_ts: TableState, coord: CellCoord): Partial<TableState> {
  return {
    gridSelection: selectionFromAnchorFocus(coord, coord, 'cell'),
    selectedRows: new Set(),
    lastSelectedIndex: null,
  };
}

/**
 * Row gutter click: switch to row mode (geometry cleared) while the row set itself is still
 * decided by {@link applyRowSelect}, so Shift/Cmd range selection keeps its existing meaning.
 */
export function applyRowModeTransition(
  ts: TableState,
  index: number,
  opts?: { multi?: boolean; range?: boolean },
): Partial<TableState> {
  return { ...applyRowSelect(ts, index, opts), gridSelection: ROW_MODE_GRID_SELECTION };
}

/** Store a selection computed elsewhere, enforcing the cell-mode row-set invariant. */
export function applyGridSelection(_ts: TableState, selection: GridSelection): Partial<TableState> {
  if (selection.mode !== 'cell') return { gridSelection: selection };
  return { gridSelection: selection, selectedRows: new Set(), lastSelectedIndex: null };
}

/** Escape / blank click / column-hidden convergence: no mode, no rows, no anchor. */
export function applyClearGridSelection(_ts: TableState): Partial<TableState> {
  return {
    gridSelection: EMPTY_GRID_SELECTION,
    selectedRows: new Set(),
    lastSelectedIndex: null,
  };
}

/** Converge the selection after the visible column set changed; rows are untouched. */
export function applyReconcileSelection(
  ts: TableState,
  columnOrder: readonly string[],
): Partial<TableState> {
  const deps: SelectionTransitionDeps = { columnOrder, rowCount: ts.rows.length };
  return {
    gridSelection: reconcileGridSelection(ts.gridSelection, deps.rowCount, deps.columnOrder),
  };
}

/**
 * The single reset object for "new data, old selection is meaningless". Every place that
 * re-fetches (page/filter/sort transition, committed response, cache invalidation) spreads
 * this exact object, so a fourth fetch path cannot forget one of the three fields.
 */
export function rowIndexSelectionReset(): Partial<TableState> {
  return {
    gridSelection: EMPTY_GRID_SELECTION,
    selectedRows: new Set<number>(),
    lastSelectedIndex: null,
  };
}

/** The six store actions that own selection, pre-bound to a panel-scoped patch function. */
export interface SelectionStoreSlice {
  selectRow: (panelId: string, index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  toggleSelectAll: (panelId: string) => void;
  setGridSelection: (panelId: string, selection: GridSelection) => void;
  clearGridSelection: (panelId: string) => void;
  beginCellSelection: (panelId: string, coord: CellCoord) => void;
  reconcileGridSelectionForColumns: (panelId: string, columnOrder: readonly string[]) => void;
}

/**
 * Build the selection actions for the store. The store passes its own `patchPanel`, so the
 * action bodies live here instead of adding ~50 lines to the already oversized store file.
 */
export function createSelectionSlice(
  patch: (panelId: string, updater: (ts: TableState) => Partial<TableState>) => void,
): SelectionStoreSlice {
  return {
    selectRow: (panelId, index, opts) => patch(panelId, (ts) => applyRowSelect(ts, index, opts)),
    toggleSelectAll: (panelId) => patch(panelId, (ts) => applyToggleSelectAll(ts)),
    setGridSelection: (panelId, selection) =>
      patch(panelId, (ts) => applyGridSelection(ts, selection)),
    clearGridSelection: (panelId) => patch(panelId, (ts) => applyClearGridSelection(ts)),
    beginCellSelection: (panelId, coord) =>
      patch(panelId, (ts) => applyBeginCellSelection(ts, coord)),
    reconcileGridSelectionForColumns: (panelId, columnOrder) =>
      patch(panelId, (ts) => applyReconcileSelection(ts, columnOrder)),
  };
}
