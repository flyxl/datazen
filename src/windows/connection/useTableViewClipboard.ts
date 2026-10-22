/**
 * The `TableView` → clipboard adapter (DB-03).
 *
 * `useGridClipboard` is deliberately panel-agnostic: it knows the selection model, the
 * column metadata and the staging action, but not the table-data store. This file is the
 * one place where the panel's slice is read and the panel's action table is bound, which
 * keeps `TableView.tsx` (already at its size ceiling) to a couple of lines of wiring.
 */
import { useMemo } from 'react';
import type { GridClipboardHandlers } from '../../components/DataTable/gridClipboardBridge';
import { useGridClipboard, type PasteSummary } from '../../hooks/useGridClipboard';
import type { PasteProblem } from '../../lib/gridErrors';
import { EMPTY_GRID_SELECTION } from '../../stores/tableData/gridSelection';
import type { TableState } from '../../stores/tableData/types';
import type { TableActions } from './useTableActions';

/** Stable empty fallbacks: a fresh collection per render would rebuild the memos. */
const EMPTY_ROWS: Record<string, unknown>[] = [];
const EMPTY_COLUMNS: TableState['columns'] = [];
const EMPTY_PENDING: TableState['pendingChanges'] = new Map();
const EMPTY_ANCHORS: TableState['rowIdentityAnchors'] = new Map();
const EMPTY_SELECTED: ReadonlySet<number> = new Set<number>();

export interface UseTableViewClipboardOptions {
  panelId: string;
  /** The panel slice; `undefined` before the first load. */
  ts: TableState | undefined;
  actions: Pick<TableActions, 'stagePasteWrites'>;
  isEditable: boolean;
  /** Displayed column names in render order — the same order the grid renders. */
  columnOrder: readonly string[];
  tableLabel: string;
  onReadOnlyPaste?: () => void;
}

export interface TableClipboardBridge {
  /** Spread onto `DataTable`: `clipboard` + `cellErrors`, both optional there. */
  dataTableProps: {
    clipboard: GridClipboardHandlers;
    cellErrors: ReadonlySet<string>;
  };
  handlers: GridClipboardHandlers;
  cellErrors: ReadonlySet<string>;
  problems: readonly PasteProblem[];
  lastPaste: PasteSummary | null;
  clear: () => void;
  copySelectedCellsAs: ReturnType<typeof useGridClipboard>['copySelectedCellsAs'];
  copyFeedback: { copied: boolean };
  confirmElement: React.ReactNode;
}

export function useTableViewClipboard({
  panelId,
  ts,
  actions,
  isEditable,
  columnOrder,
  tableLabel,
  onReadOnlyPaste,
}: UseTableViewClipboardOptions): TableClipboardBridge {
  const result = useGridClipboard({
    panelId,
    isEditable,
    columns: ts?.columns ?? EMPTY_COLUMNS,
    columnOrder,
    rows: ts?.rows ?? EMPTY_ROWS,
    pendingChanges: ts?.pendingChanges ?? EMPTY_PENDING,
    rowIdentityAnchors: ts?.rowIdentityAnchors ?? EMPTY_ANCHORS,
    selectedRows: ts?.selectedRows ?? EMPTY_SELECTED,
    gridSelection: ts?.gridSelection ?? EMPTY_GRID_SELECTION,
    context: ts?.context ?? null,
    tableLabel,
    stagePasteWrites: actions.stagePasteWrites,
    showReadOnlyTip: onReadOnlyPaste,
  });

  return useMemo(
    () => ({
      dataTableProps: { clipboard: result.handlers, cellErrors: result.cellErrors },
      handlers: result.handlers,
      cellErrors: result.cellErrors,
      problems: result.problems,
      lastPaste: result.lastPaste,
      clear: result.clearPasteResult,
      copySelectedCellsAs: result.copySelectedCellsAs,
      copyFeedback: result.copyFeedback,
      confirmElement: result.confirmElement,
    }),
    [
      result.handlers,
      result.cellErrors,
      result.problems,
      result.lastPaste,
      result.clearPasteResult,
      result.copySelectedCellsAs,
      result.copyFeedback,
      result.confirmElement,
    ],
  );
}

export { describePasteProblem } from '../../hooks/useGridClipboard';
