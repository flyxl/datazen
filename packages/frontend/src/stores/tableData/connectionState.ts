import type { ColumnSchema, Value } from '../../types';
import type { TableChangeContext } from '../../lib/tableChanges';
import { EMPTY_GRID_SELECTION } from './gridSelection';
import { emptyKeysetPagingState } from '../../lib/keysetPaging';
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
    // A panel that has never read a row has no count to doubt yet: the
    // first response says how much of it may be trusted.
    totalRowsKind: 'exact',
    defaultOrderOverride: 'inherit',
    page: 0,
    pageSize: 50,
    // A panel that has never paged by seek reads by offset — which is what
    // every read did before the keyset contract existed.
    keyset: emptyKeysetPagingState,
    driftRecountPage: null,
    filters: [],
    filterLogic: 'and',
    draftFilters: [],
    draftFilterLogic: 'and',
    filterPanelOpen: false,
    sorts: [],
    columnOrder: null,
    frozenColumnCount: 0,
    pendingChanges: new Map(),
    draftInserts: new Map(),
    draftInsertOrder: [],
    highlightedNewRowKey: null,
    rowIdentityAnchors: new Map(),
    previewPlan: null,
    pendingStatus: 'idle',
    selectedRows: new Set(),
    lastSelectedIndex: null,
    gridSelection: EMPTY_GRID_SELECTION,
    editingCell: null,
    enumCandidates: new Map(),
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
