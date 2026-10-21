import type { ColumnSchema, Value } from '../../types';
import type { TableChangeContext } from '../../lib/tableChanges';
import { EMPTY_GRID_SELECTION } from './gridSelection';
import type { TableState } from './types';

export function rowsToRecords(
  columns: ColumnSchema[],
  rows: (Value | null)[][],
): Record<string, unknown>[] {
  return rows.map((row) => {
    const record: Record<string, unknown> = {};
    columns.forEach((col, i) => {
      record[col.name] = row[i] ?? null;
    });
    return record;
  });
}

export function editKey(rowIndex: number, columnName: string) {
  return `${rowIndex}:${columnName}`;
}

export function toCellValue(val: unknown): Value | null {
  if (val === null || val === undefined) return null;
  return val as Value;
}

export function extractErrorMessage(e: unknown, fallback: string): string {
  if (typeof e === 'string' && e.trim()) return e;
  if (e instanceof Error && e.message.trim()) return e.message;
  if (e && typeof e === 'object') {
    const msg = (e as { message?: unknown }).message;
    if (typeof msg === 'string' && msg.trim()) return msg;
  }
  return fallback;
}

export function emptyTableState(context: TableChangeContext | null = null): TableState {
  return {
    context,
    columns: [],
    rows: [],
    totalRows: 0,
    page: 0,
    pageSize: 50,
    filters: [],
    filterLogic: 'and',
    draftFilters: [],
    draftFilterLogic: 'and',
    filterPanelOpen: false,
    sorts: [],
    editBuffer: new Map(),
    pendingChanges: new Map(),
    rowIdentityAnchors: new Map(),
    previewPlan: null,
    pendingStatus: 'idle',
    selectedRows: new Set(),
    lastSelectedIndex: null,
    gridSelection: EMPTY_GRID_SELECTION,
    editingCell: null,
    detailRowIndex: null,
    loading: false,
    requestRevision: 0,
    loadingRevision: null,
    error: null,
    visibleColumns: null,
  };
}

/** The panel's immutable load target, as carried in every request. */
export interface TableTarget {
  dbSessionId: string;
  table: string;
  connectionId?: string | null;
  driverType?: string | null;
  database?: string | null;
  schema?: string | null;
}

export function buildTableContext(target: TableTarget): TableChangeContext {
  return {
    connectionId: target.connectionId ?? null,
    dbSessionId: target.dbSessionId,
    driverType: target.driverType ?? null,
    database: target.database ?? null,
    schema: target.schema ?? null,
    table: target.table,
  };
}
