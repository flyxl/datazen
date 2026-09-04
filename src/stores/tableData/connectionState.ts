import type { ColumnSchema, Value } from '../../types';
import type { TableChangeContext } from '../../lib/tableChanges';
import type { CellEdit, ConnectionTableState, TableState } from './types';

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
    lastCommitResult: null,
    selectedRows: new Set(),
    lastSelectedIndex: null,
    editingCell: null,
    loading: false,
    requestRevision: 0,
    loadingRevision: null,
    error: null,
  };
}

export function emptyConnectionTableState(): ConnectionTableState {
  return {
    activeTable: null,
    activeTableKey: null,
    connectionId: null,
    databaseType: null,
    activeDatabase: null,
    activeSchema: null,
    tableStates: new Map(),
    detailRowIndex: null,
  };
}

export function getState(states: Map<string, TableState>, tableKey: string | null): TableState {
  if (!tableKey) return emptyTableState();
  return states.get(tableKey) ?? emptyTableState();
}

export function syncFlat(
  activeTable: string | null,
  activeTableKey: string | null,
  states: Map<string, TableState>,
) {
  const ts = getState(states, activeTableKey);
  return {
    tableName: activeTable,
    columns: ts.columns,
    rows: ts.rows,
    totalRows: ts.totalRows,
    page: ts.page,
    pageSize: ts.pageSize,
    filters: ts.filters,
    filterLogic: ts.filterLogic,
    draftFilters: ts.draftFilters,
    draftFilterLogic: ts.draftFilterLogic,
    filterPanelOpen: ts.filterPanelOpen,
    sorts: ts.sorts,
    editBuffer: ts.editBuffer,
    pendingChanges: ts.pendingChanges,
    previewPlan: ts.previewPlan,
    pendingStatus: ts.pendingStatus,
    lastCommitResult: ts.lastCommitResult,
    selectedRows: ts.selectedRows,
    lastSelectedIndex: ts.lastSelectedIndex,
    editingCell: ts.editingCell,
    loading: ts.loading,
    error: ts.error,
  };
}

export function flattenActive(
  perConnection: Map<string, ConnectionTableState>,
  activeDbSessionId: string | null,
): ConnectionTableState & ReturnType<typeof syncFlat> {
  const cs = activeDbSessionId
    ? (perConnection.get(activeDbSessionId) ?? emptyConnectionTableState())
    : emptyConnectionTableState();
  return {
    ...cs,
    ...syncFlat(cs.activeTable, cs.activeTableKey, cs.tableStates),
  };
}

export function patchConnection(
  perConnection: Map<string, ConnectionTableState>,
  dbSessionId: string,
  patch: Partial<ConnectionTableState>,
): Map<string, ConnectionTableState> {
  const next = new Map(perConnection);
  const current = perConnection.get(dbSessionId) ?? emptyConnectionTableState();
  next.set(dbSessionId, { ...current, ...patch });
  return next;
}

export function buildTableChangeContext(
  connState: ConnectionTableState,
  params: {
    dbSessionId: string;
    table: string;
    connectionId?: string | null;
    driverType?: string | null;
    database?: string | null;
    schema?: string | null;
  },
): TableChangeContext {
  return {
    connectionId: params.connectionId !== undefined ? params.connectionId : connState.connectionId,
    dbSessionId: params.dbSessionId,
    driverType: params.driverType !== undefined ? params.driverType : connState.databaseType,
    database: params.database !== undefined ? params.database : connState.activeDatabase,
    schema: params.schema !== undefined ? params.schema : connState.activeSchema,
    table: params.table,
  };
}

export function activeTableContext(conn: ConnectionTableState): TableChangeContext | null {
  if (!conn.activeTableKey) return null;
  return conn.tableStates.get(conn.activeTableKey)?.context ?? null;
}

export type { CellEdit, ConnectionTableState, TableState };
