import { create } from 'zustand';
import { databaseCommands } from '../commands/database';
import { t } from '../locales/t';
import type { ColumnSchema, DatabaseType, FilterCondition, SortCondition, Value } from '../types';
import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  buildRowIdentity,
  clonePendingRowChange,
  rowIdentityKey,
  valuesEqual,
  type CommitPendingChangesResult,
  type PendingRowChange,
  type PendingStatus,
  type RowChangePlan,
} from '../lib/tableChanges';
import { useSettingsStore } from './settingsStore';

function rowsToRecords(
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

export interface CellEdit {
  rowIndex: number;
  columnName: string;
  originalValue: unknown;
  newValue: unknown;
  pkSnapshot: Record<string, unknown>;
}

function editKey(rowIndex: number, columnName: string) {
  return `${rowIndex}:${columnName}`;
}

function toCellValue(val: unknown): Value | null {
  if (val === null || val === undefined) return null;
  return val as Value;
}

function extractErrorMessage(e: unknown, fallback: string): string {
  if (typeof e === 'string' && e.trim()) return e;
  if (e instanceof Error && e.message.trim()) return e.message;
  if (e && typeof e === 'object') {
    const msg = (e as { message?: unknown }).message;
    if (typeof msg === 'string' && msg.trim()) return msg;
  }
  return fallback;
}

/** Incomplete filters (just added / cleared value) must not hit the backend. */
export function isCompleteFilter(filter: FilterCondition): boolean {
  if (!filter.column) return false;
  if (filter.operator === 'isNull' || filter.operator === 'isNotNull') return true;
  if (filter.value === null || filter.value === undefined) return false;
  if (typeof filter.value === 'string' && filter.value === '') return false;
  if (Array.isArray(filter.value) && filter.value.length === 0) return false;
  return true;
}

export function cloneFilters(filters: FilterCondition[]): FilterCondition[] {
  return filters.map((f) => ({ ...f }));
}

export function filterDraftEqualsApplied(
  draftFilters: FilterCondition[],
  draftLogic: 'and' | 'or',
  appliedFilters: FilterCondition[],
  appliedLogic: 'and' | 'or',
): boolean {
  return (
    draftLogic === appliedLogic && JSON.stringify(draftFilters) === JSON.stringify(appliedFilters)
  );
}

/** Per-table state slice */
export interface TableState {
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  totalRows: number;
  page: number;
  pageSize: number;
  /** Applied filters used by queries. */
  filters: FilterCondition[];
  filterLogic: 'and' | 'or';
  draftFilters: FilterCondition[];
  draftFilterLogic: 'and' | 'or';
  filterPanelOpen: boolean;
  sorts: SortCondition[];
  editBuffer: Map<string, CellEdit>;
  /** Changes staged against this table; the map key is the stable PK identity. */
  pendingChanges: Map<string, PendingRowChange>;
  previewPlan: RowChangePlan | null;
  pendingStatus: PendingStatus;
  lastCommitResult: CommitPendingChangesResult | null;
  selectedRows: Set<number>;
  lastSelectedIndex: number | null;
  editingCell: { row: number; col: string } | null;
  loading: boolean;
  error: string | null;
}

function emptyTableState(): TableState {
  return {
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
    previewPlan: null,
    pendingStatus: 'idle',
    lastCommitResult: null,
    selectedRows: new Set(),
    lastSelectedIndex: null,
    editingCell: null,
    loading: false,
    error: null,
  };
}

// ── Per-connection state ──────────────────────────────────────────

interface ConnectionTableState {
  activeTable: string | null;
  databaseType: string | null;
  /** F1: last target database used for table data loads on this session. */
  activeDatabase: string | null;
  tableStates: Map<string, TableState>;
  detailRowIndex: number | null;
}

function emptyConnectionTableState(): ConnectionTableState {
  return {
    activeTable: null,
    databaseType: null,
    activeDatabase: null,
    tableStates: new Map(),
    detailRowIndex: null,
  };
}

function getState(states: Map<string, TableState>, table: string | null): TableState {
  if (!table) return emptyTableState();
  return states.get(table) ?? emptyTableState();
}

function syncFlat(active: string | null, states: Map<string, TableState>) {
  const ts = getState(states, active);
  return {
    tableName: active,
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

function flattenActive(
  perConnection: Map<string, ConnectionTableState>,
  activeDbSessionId: string | null,
): ConnectionTableState & ReturnType<typeof syncFlat> {
  const cs = activeDbSessionId
    ? (perConnection.get(activeDbSessionId) ?? emptyConnectionTableState())
    : emptyConnectionTableState();
  return {
    ...cs,
    ...syncFlat(cs.activeTable, cs.tableStates),
  };
}

function patchConnection(
  perConnection: Map<string, ConnectionTableState>,
  dbSessionId: string,
  patch: Partial<ConnectionTableState>,
): Map<string, ConnectionTableState> {
  const next = new Map(perConnection);
  const current = perConnection.get(dbSessionId) ?? emptyConnectionTableState();
  next.set(dbSessionId, { ...current, ...patch });
  return next;
}

// ── Store ─────────────────────────────────────────────────────────

interface TableDataStore extends ConnectionTableState {
  perConnection: Map<string, ConnectionTableState>;
  /** Runtime DB session id of the active session. */
  activeDbSessionId: string | null;

  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  totalRows: number;
  page: number;
  pageSize: number;
  filters: FilterCondition[];
  filterLogic: 'and' | 'or';
  draftFilters: FilterCondition[];
  draftFilterLogic: 'and' | 'or';
  filterPanelOpen: boolean;
  sorts: SortCondition[];
  editBuffer: Map<string, CellEdit>;
  pendingChanges: Map<string, PendingRowChange>;
  previewPlan: RowChangePlan | null;
  pendingStatus: PendingStatus;
  lastCommitResult: CommitPendingChangesResult | null;
  selectedRows: Set<number>;
  lastSelectedIndex: number | null;
  editingCell: { row: number; col: string } | null;
  loading: boolean;
  error: string | null;
  tableName: string | null;

  setActiveConnection: (dbSessionId: string | null) => void;
  removeConnection: (dbSessionId: string) => void;
  setDatabaseType: (dbType: string) => void;
  switchToTable: (table: string) => void;
  loadTableData: (params: {
    dbSessionId: string;
    table: string;
    skipCount?: boolean;
    /** F1: explicit target database; remembered per connection so store-driven
     * refreshes (paging, filters, row edits) keep hitting the right database. */
    database?: string | null;
  }) => Promise<void>;
  setPage: (page: number) => void;
  setPageSize: (size: number) => void;
  addFilter: (filter: FilterCondition) => void;
  setFilters: (filters: FilterCondition[]) => void;
  updateFilter: (index: number, filter: FilterCondition) => void;
  setFilterLogic: (logic: 'and' | 'or') => void;
  removeFilter: (index: number) => void;
  clearFilters: () => void;
  applyFilters: () => void;
  setFilterPanelOpen: (open: boolean) => void;
  setSort: (sort: SortCondition) => void;
  startEdit: (row: number, col: string) => void;
  stageCellChange: (row: number, col: string, value: unknown) => void;
  stageRowDelete: (rowIndices: number | number[]) => void;
  discardPendingChanges: () => void;
  rollbackPendingChanges: () => void;
  previewPendingChanges: () => Promise<RowChangePlan | null>;
  commitPendingChanges: () => Promise<CommitPendingChangesResult>;
  /** Compatibility aliases retained until the shared UI wiring is migrated. */
  updateCell: (row: number, col: string, value: unknown) => void;
  applyColumnToRows: (col: string, value: unknown, rows: number[]) => void;
  cancelEdit: () => void;
  commitChanges: () => Promise<CommitPendingChangesResult>;
  discardChanges: () => void;
  selectRow: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  toggleSelectAll: () => void;
  deleteSelectedRows: () => Promise<void>;
  deleteRows: (rowIndices: number[]) => Promise<void>;
  closeTable: (table: string) => void;

  setDetailRow: (index: number | null) => void;
  reset: () => void;
}

/** Get the active DB session's state. */
function getActiveConn(get: () => TableDataStore): ConnectionTableState {
  const { activeDbSessionId, perConnection } = get();
  if (!activeDbSessionId) return emptyConnectionTableState();
  return perConnection.get(activeDbSessionId) ?? emptyConnectionTableState();
}

/** Commit a per-connection patch and re-flatten. */
function commitPatch(
  get: () => TableDataStore,
  set: (partial: Partial<TableDataStore>) => void,
  dbSessionId: string | null,
  patch: Partial<ConnectionTableState>,
): void {
  const state = get();
  const targetDbSessionId = dbSessionId ?? state.activeDbSessionId;
  if (!targetDbSessionId) return;
  const perConnection = patchConnection(state.perConnection, targetDbSessionId, patch);
  set({ perConnection, ...flattenActive(perConnection, state.activeDbSessionId) });
}

/** Update active table's state within the active connection. */
function updateActive(
  get: () => TableDataStore,
  set: (partial: Partial<TableDataStore>) => void,
  updater: (ts: TableState) => Partial<TableState>,
): void {
  const conn = getActiveConn(get);
  if (!conn.activeTable) return;
  const current = getState(conn.tableStates, conn.activeTable);
  const patched = { ...current, ...updater(current) };
  const next = new Map(conn.tableStates);
  next.set(conn.activeTable, patched);
  commitPatch(get, set, null, { tableStates: next });
}

function findPendingForRow(
  ts: TableState,
  rowIndex: number,
  row: Record<string, unknown>,
  pkColumns: ColumnSchema[],
  allowRowIndexFallback = true,
): { key: string; change: PendingRowChange } | null {
  const identity = buildRowIdentity(row, pkColumns);
  if (!identity) return null;
  const directKey = rowIdentityKey(identity);
  const direct = ts.pendingChanges.get(directKey);
  if (direct) return { key: directKey, change: direct };

  for (const [key, change] of ts.pendingChanges) {
    const currentIdentity = { ...change.rowIdentity };
    for (const column of pkColumns) {
      if (Object.prototype.hasOwnProperty.call(change.currentValues, column.name)) {
        currentIdentity[column.name] = change.currentValues[column.name];
      }
    }
    if (rowIdentityKey(currentIdentity) === directKey) return { key, change };
  }

  // A primary-key edit changes the identity visible in the local row. Keep
  // associating current-page interactions with the original pending row by
  // its page position until the shared UI starts carrying row handles.
  if (!allowRowIndexFallback) return null;
  for (const [key, change] of ts.pendingChanges) {
    if (change.rowIndex === rowIndex) return { key, change };
  }
  return null;
}

function rebuildEditBuffer(ts: TableState): Map<string, CellEdit> {
  const next = new Map<string, CellEdit>();
  const pkColumns = ts.columns.filter((column) => column.isPrimaryKey);
  if (pkColumns.length === 0) return next;

  ts.rows.forEach((row, rowIndex) => {
    const match = findPendingForRow(ts, rowIndex, row, pkColumns);
    if (!match || match.change.deleteMarked) return;
    for (const column of match.change.changedColumns) {
      next.set(editKey(rowIndex, column), {
        rowIndex,
        columnName: column,
        originalValue: match.change.originalValues[column],
        newValue: match.change.currentValues[column],
        pkSnapshot: { ...match.change.rowIdentity },
      });
    }
  });
  return next;
}

function overlayPendingRows(ts: TableState, rows: Record<string, unknown>[]): Record<string, unknown>[] {
  const pkColumns = ts.columns.filter((column) => column.isPrimaryKey);
  if (pkColumns.length === 0 || ts.pendingChanges.size === 0) return rows;
  return rows.map((row, rowIndex) => {
    // Never use a page-local row number while applying a fetch result. A
    // stable identity match is required so a page switch cannot overlay a
    // pending value onto an unrelated row.
    const match = findPendingForRow(ts, rowIndex, row, pkColumns, false);
    return match ? { ...row, ...match.change.currentValues } : row;
  });
}

function pendingChangesSignature(pendingChanges: Map<string, PendingRowChange>): string {
  return JSON.stringify(
    Array.from(pendingChanges.entries())
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, change]) => [key, clonePendingRowChange(change)]),
  );
}

function pendingChangesForWire(pendingChanges: Map<string, PendingRowChange>): PendingRowChange[] {
  return Array.from(pendingChanges.values()).map((change) => {
    const { rowIndex: _rowIndex, ...wireChange } = clonePendingRowChange(change);
    return wireChange;
  });
}

function reloadActive(get: () => TableDataStore): void {
  const conn = getActiveConn(get);
  const { activeDbSessionId } = get();
  if (activeDbSessionId && conn.activeTable) {
    void get().loadTableData({ dbSessionId: activeDbSessionId, table: conn.activeTable });
  }
}

export const useTableDataStore = create<TableDataStore>((set, get) => ({
  perConnection: new Map(),
  activeDbSessionId: null,
  ...emptyConnectionTableState(),
  ...syncFlat(null, new Map()),

  setActiveConnection: (dbSessionId) => {
    const state = get();
    let perConnection = state.perConnection;
    if (dbSessionId && !perConnection.has(dbSessionId)) {
      perConnection = new Map(perConnection);
      perConnection.set(dbSessionId, emptyConnectionTableState());
    }
    set({
      perConnection,
      activeDbSessionId: dbSessionId,
      ...flattenActive(perConnection, dbSessionId),
    });
  },

  removeConnection: (dbSessionId) => {
    const state = get();
    const perConnection = new Map(state.perConnection);
    perConnection.delete(dbSessionId);
    const activeDbSessionId =
      state.activeDbSessionId === dbSessionId ? null : state.activeDbSessionId;
    set({
      perConnection,
      activeDbSessionId,
      ...flattenActive(perConnection, activeDbSessionId),
    });
  },

  setDetailRow: (index) => {
    commitPatch(get, set, null, { detailRowIndex: index });
  },

  setDatabaseType: (dbType: string) => {
    commitPatch(get, set, null, { databaseType: dbType });
  },

  switchToTable: (table: string) => {
    commitPatch(get, set, null, { activeTable: table });
  },

  loadTableData: async ({ dbSessionId, table, skipCount, database }) => {
    const state = get();
    const connState = state.perConnection.get(dbSessionId) ?? emptyConnectionTableState();
    const existing = connState.tableStates.get(table) ?? emptyTableState();

    if (existing.loading) return;

    // F1: explicit pin wins; otherwise reuse the remembered target database so
    // store-driven refreshes stay on the panel's database.
    const targetDatabase = database ?? connState.activeDatabase ?? null;

    const { page, filters, sorts, filterLogic } = existing;
    const driverPageSize = DB_REGISTRY[connState.databaseType as DatabaseType]?.defaultPageSize;
    const settingsPageSize = useSettingsStore.getState().settings.defaultPageSize;
    const pageSize =
      existing.columns.length > 0
        ? existing.pageSize
        : settingsPageSize || driverPageSize || existing.pageSize;

    const nextStates = new Map(connState.tableStates);
    nextStates.set(table, { ...existing, loading: true, error: null });
    commitPatch(get, set, dbSessionId, {
      activeTable: table,
      tableStates: nextStates,
      ...(database != null ? { activeDatabase: database } : {}),
    });

    try {
      const res = await databaseCommands.getTableData({
        dbSessionId,
        table,
        page,
        pageSize,
        filters: filters.filter(isCompleteFilter),
        sorts,
        skipCount,
        filterLogic,
        database: targetDatabase,
      });
      const latestConn = get().perConnection.get(dbSessionId) ?? emptyConnectionTableState();
      const updated = new Map(latestConn.tableStates);
      const ts = updated.get(table) ?? emptyTableState();
      const fetchedRows = rowsToRecords(res.columns, res.rows);
      const rowsWithPending = overlayPendingRows(
        { ...ts, columns: res.columns, rows: fetchedRows },
        fetchedRows,
      );
      const patched: TableState = {
        ...ts,
        columns: res.columns,
        rows: rowsWithPending,
        totalRows: res.totalRows ?? ts.totalRows,
        page: res.page,
        pageSize: res.pageSize,
        loading: false,
        selectedRows: new Set(),
        editBuffer: rebuildEditBuffer({ ...ts, columns: res.columns, rows: rowsWithPending }),
        editingCell: null,
        error: null,
      };
      updated.set(table, patched);
      commitPatch(get, set, dbSessionId, { tableStates: updated });
    } catch (e) {
      const latestConn = get().perConnection.get(dbSessionId) ?? emptyConnectionTableState();
      const updated = new Map(latestConn.tableStates);
      const ts = updated.get(table) ?? emptyTableState();
      updated.set(table, {
        ...ts,
        loading: false,
        error: extractErrorMessage(e, t('tableData.loadFailed')),
      });
      commitPatch(get, set, dbSessionId, { tableStates: updated });
    }
  },

  setPage: (page) => {
    updateActive(get, set, () => ({ page }));
    const conn = getActiveConn(get);
    const { activeDbSessionId } = get();
    if (activeDbSessionId && conn.activeTable)
      void get().loadTableData({
        dbSessionId: activeDbSessionId,
        table: conn.activeTable,
        skipCount: true,
      });
  },

  setPageSize: (size) => {
    updateActive(get, set, () => ({ pageSize: size, page: 0 }));
    const conn = getActiveConn(get);
    const { activeDbSessionId } = get();
    if (activeDbSessionId && conn.activeTable)
      void get().loadTableData({
        dbSessionId: activeDbSessionId,
        table: conn.activeTable,
        skipCount: true,
      });
  },

  addFilter: (filter) => {
    updateActive(get, set, (ts) => ({
      draftFilters: [...ts.draftFilters, filter],
      filterPanelOpen: true,
    }));
  },

  setFilters: (filters) => {
    const next = cloneFilters(filters);
    updateActive(get, set, () => ({
      filters: next,
      draftFilters: cloneFilters(next),
      page: 0,
      filterPanelOpen: true,
    }));
    reloadActive(get);
  },

  updateFilter: (index, filter) => {
    updateActive(get, set, (ts) => ({
      draftFilters: ts.draftFilters.map((f, i) => (i === index ? filter : f)),
    }));
  },

  setFilterLogic: (logic) => {
    updateActive(get, set, () => ({ draftFilterLogic: logic }));
  },

  removeFilter: (index) => {
    updateActive(get, set, (ts) => ({
      draftFilters: ts.draftFilters.filter((_, i) => i !== index),
    }));
  },

  clearFilters: () => {
    updateActive(get, set, () => ({
      filters: [],
      draftFilters: [],
      filterLogic: 'and',
      draftFilterLogic: 'and',
      page: 0,
    }));
    reloadActive(get);
  },

  applyFilters: () => {
    const conn = getActiveConn(get);
    if (!conn.activeTable) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    if (
      filterDraftEqualsApplied(ts.draftFilters, ts.draftFilterLogic, ts.filters, ts.filterLogic)
    ) {
      return;
    }
    updateActive(get, set, (cur) => ({
      filters: cloneFilters(cur.draftFilters),
      filterLogic: cur.draftFilterLogic,
      page: 0,
    }));
    reloadActive(get);
  },

  setFilterPanelOpen: (open) => {
    updateActive(get, set, () => ({ filterPanelOpen: open }));
  },

  setSort: (sort) => {
    updateActive(get, set, () => ({ sorts: [sort], page: 0 }));
    const conn = getActiveConn(get);
    const { activeDbSessionId } = get();
    if (activeDbSessionId && conn.activeTable)
      void get().loadTableData({
        dbSessionId: activeDbSessionId,
        table: conn.activeTable,
        skipCount: true,
      });
  },

  startEdit: (row, col) => updateActive(get, set, () => ({ editingCell: { row, col } })),

  stageCellChange: (row, col, value) => {
    const conn = getActiveConn(get);
    if (!conn.activeTable) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    const rowObj = ts.rows[row];
    if (!rowObj) return;
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      updateActive(get, set, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }

    const match = findPendingForRow(ts, row, rowObj, pkCols);
    const identity = match?.change.rowIdentity ?? buildRowIdentity(rowObj, pkCols);
    if (!identity) {
      updateActive(get, set, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }
    const existing = match?.change;
    const hasOriginalValue =
      existing !== undefined && Object.prototype.hasOwnProperty.call(existing.originalValues, col);
    const originalValue = hasOriginalValue
      ? existing.originalValues[col]
      : toCellValue(rowObj[col]);
    const currentValue = toCellValue(value);
    const originalValues = { ...(existing?.originalValues ?? {}) };
    const currentValues = { ...(existing?.currentValues ?? {}) };
    let changedColumns = [...(existing?.changedColumns ?? [])];

    if (valuesEqual(originalValue, currentValue)) {
      delete originalValues[col];
      delete currentValues[col];
      changedColumns = changedColumns.filter((column) => column !== col);
    } else {
      originalValues[col] = originalValue;
      currentValues[col] = currentValue;
      if (!changedColumns.includes(col)) changedColumns.push(col);
    }

    const pendingChanges = new Map(ts.pendingChanges);
    const key = match?.key ?? rowIdentityKey(identity);
    if (changedColumns.length === 0 && !existing?.deleteMarked) {
      pendingChanges.delete(key);
    } else {
      pendingChanges.set(key, {
        rowIndex: row,
        rowIdentity: { ...identity },
        originalValues,
        currentValues,
        changedColumns,
        deleteMarked: existing?.deleteMarked ?? false,
      });
    }

    const nextRows = [...ts.rows];
    nextRows[row] = { ...rowObj, [col]: currentValue };

    const next = new Map(conn.tableStates);
    next.set(conn.activeTable, {
      ...ts,
      rows: nextRows,
      pendingChanges,
      previewPlan: null,
      pendingStatus: 'idle',
      lastCommitResult: null,
      editBuffer: rebuildEditBuffer({ ...ts, rows: nextRows, pendingChanges }),
      editingCell: null,
      error: null,
    });
    commitPatch(get, set, null, { tableStates: next });
  },

  applyColumnToRows: (col, value, rows) => {
    for (const row of rows) get().stageCellChange(row, col, value);
  },

  cancelEdit: () => updateActive(get, set, () => ({ editingCell: null })),

  stageRowDelete: (rowIndices) => {
    const conn = getActiveConn(get);
    if (!conn.activeTable) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      updateActive(get, set, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }

    const indices = [...new Set((Array.isArray(rowIndices) ? rowIndices : [rowIndices]).filter(
      (index) => Number.isInteger(index) && index >= 0,
    ))].sort((left, right) => left - right);
    if (indices.length === 0) return;

    const pendingChanges = new Map(ts.pendingChanges);
    for (const rowIndex of indices) {
      const row = ts.rows[rowIndex];
      if (!row) continue;
      const match = findPendingForRow(ts, rowIndex, row, pkCols);
      const identity = match?.change.rowIdentity ?? buildRowIdentity(row, pkCols);
      if (!identity) continue;
      const existing = match?.change;
      pendingChanges.set(match?.key ?? rowIdentityKey(identity), {
        rowIndex,
        rowIdentity: { ...identity },
        originalValues: { ...(existing?.originalValues ?? {}) },
        currentValues: { ...(existing?.currentValues ?? {}) },
        changedColumns: [...(existing?.changedColumns ?? [])],
        deleteMarked: true,
      });
    }

    const next = new Map(conn.tableStates);
    next.set(conn.activeTable, {
      ...ts,
      pendingChanges,
      previewPlan: null,
      pendingStatus: 'idle',
      lastCommitResult: null,
      editBuffer: rebuildEditBuffer({ ...ts, pendingChanges }),
      editingCell: null,
      error: null,
    });
    commitPatch(get, set, null, { tableStates: next });
  },

  discardPendingChanges: () => {
    const conn = getActiveConn(get);
    const { activeDbSessionId } = get();
    if (!conn.activeTable) return;
    updateActive(get, set, () => ({
      pendingChanges: new Map(),
      previewPlan: null,
      pendingStatus: 'idle',
      lastCommitResult: null,
      editBuffer: new Map(),
      editingCell: null,
      error: null,
    }));
    if (activeDbSessionId)
      void get().loadTableData({ dbSessionId: activeDbSessionId, table: conn.activeTable });
  },

  rollbackPendingChanges: () => get().discardPendingChanges(),

  previewPendingChanges: async () => {
    const conn = getActiveConn(get);
    const { activeDbSessionId } = get();
    if (!conn.activeTable || !activeDbSessionId) return null;
    const table = conn.activeTable;
    const ts = getState(conn.tableStates, table);
    if (ts.pendingChanges.size === 0) return null;

    const signature = pendingChangesSignature(ts.pendingChanges);
    const changes = pendingChangesForWire(ts.pendingChanges);
    updateActive(get, set, () => ({ pendingStatus: 'previewing', error: null }));
    try {
      const plan = await databaseCommands.previewPendingChanges({
        dbSessionId: activeDbSessionId,
        table,
        database: conn.activeDatabase,
        changes,
      });
      const latestConn = get().perConnection.get(activeDbSessionId) ?? emptyConnectionTableState();
      const latest = getState(latestConn.tableStates, table);
      if (pendingChangesSignature(latest.pendingChanges) === signature) {
        const next = new Map(latestConn.tableStates);
        next.set(table, {
          ...latest,
          previewPlan: plan,
          pendingStatus: 'idle',
          error: null,
        });
        commitPatch(get, set, activeDbSessionId, { tableStates: next });
      }
      return plan;
    } catch (e) {
      updateActive(get, set, () => ({
        pendingStatus: 'idle',
        error: extractErrorMessage(e, t('tableData.commitFailed')),
      }));
      return null;
    }
  },

  commitPendingChanges: async () => {
    const emptyResult: CommitPendingChangesResult = {
      status: 'noop',
      planId: '',
      fingerprint: '',
      statements: [],
      affectedRows: 0,
      refreshed: false,
      refreshRequired: false,
    };
    const initialConn = getActiveConn(get);
    const { activeDbSessionId } = get();
    if (!initialConn.activeTable || !activeDbSessionId) return emptyResult;
    const table = initialConn.activeTable;
    const initial = getState(initialConn.tableStates, table);
    if (initial.pendingChanges.size === 0) return emptyResult;

    const signature = pendingChangesSignature(initial.pendingChanges);
    let plan = initial.previewPlan;
    if (!plan || plan.table.dbSessionId !== activeDbSessionId || plan.table.table !== table) {
      plan = await get().previewPendingChanges();
    }
    if (!plan) return emptyResult;

    updateActive(get, set, () => ({ pendingStatus: 'committing', error: null }));
    try {
      const response = await databaseCommands.commitPendingChanges({
        dbSessionId: activeDbSessionId,
        plan,
        fingerprint: plan.fingerprint,
      });
      const latestConn = get().perConnection.get(activeDbSessionId) ?? emptyConnectionTableState();
      const latest = getState(latestConn.tableStates, table);
      const unchanged = pendingChangesSignature(latest.pendingChanges) === signature;
      if (unchanged) {
        const next = new Map(latestConn.tableStates);
        next.set(table, {
          ...latest,
          pendingChanges: new Map(),
          previewPlan: null,
          pendingStatus: 'idle',
          editBuffer: new Map(),
          editingCell: null,
          error: null,
        });
        commitPatch(get, set, activeDbSessionId, { tableStates: next });
      }

      await get().loadTableData({ dbSessionId: activeDbSessionId, table });
      const refreshedConn = get().perConnection.get(activeDbSessionId) ?? emptyConnectionTableState();
      const refreshedState = getState(refreshedConn.tableStates, table);
      const result: CommitPendingChangesResult = {
        ...response,
        status: 'committed',
        refreshed: !refreshedState.error,
        refreshRequired: true,
        refreshStatus: refreshedState.error ? 'failed' : 'completed',
      };
      const afterRefreshConn = get().perConnection.get(activeDbSessionId) ?? emptyConnectionTableState();
      const afterRefresh = new Map(afterRefreshConn.tableStates);
      const afterRefreshState = getState(afterRefreshConn.tableStates, table);
      afterRefresh.set(table, { ...afterRefreshState, pendingStatus: 'idle', lastCommitResult: result });
      commitPatch(get, set, activeDbSessionId, { tableStates: afterRefresh });
      return result;
    } catch (e) {
      const error = extractErrorMessage(e, t('tableData.commitFailed'));
      updateActive(get, set, () => ({ pendingStatus: 'idle', error }));
      return {
        ...emptyResult,
        status: 'failed',
        planId: plan?.planId ?? '',
        fingerprint: plan?.fingerprint ?? '',
        error,
      };
    }
  },

  updateCell: (row, col, value) => get().stageCellChange(row, col, value),

  commitChanges: () => get().commitPendingChanges(),

  discardChanges: () => get().discardPendingChanges(),

  selectRow: (index, opts) => {
    updateActive(get, set, (ts) => {
      if (opts?.range && ts.lastSelectedIndex !== null) {
        const lo = Math.min(ts.lastSelectedIndex, index);
        const hi = Math.max(ts.lastSelectedIndex, index);
        const next = new Set(ts.selectedRows);
        for (let i = lo; i <= hi; i += 1) next.add(i);
        return { selectedRows: next, lastSelectedIndex: index };
      } else if (opts?.multi) {
        const next = new Set(ts.selectedRows);
        if (next.has(index)) next.delete(index);
        else next.add(index);
        return { selectedRows: next, lastSelectedIndex: index };
      }
      return { selectedRows: new Set([index]), lastSelectedIndex: index };
    });
  },

  toggleSelectAll: () => {
    updateActive(get, set, (ts) => {
      const allSelected = ts.selectedRows.size === ts.rows.length && ts.rows.length > 0;
      if (allSelected) return { selectedRows: new Set(), lastSelectedIndex: null };
      const next = new Set<number>();
      for (let i = 0; i < ts.rows.length; i += 1) next.add(i);
      return { selectedRows: next, lastSelectedIndex: null };
    });
  },

  deleteSelectedRows: async () => {
    const conn = getActiveConn(get);
    if (!conn.activeTable) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    const indices = Array.from(ts.selectedRows).sort((a, b) => a - b);
    if (indices.length === 0) return;
    get().stageRowDelete(indices);
  },

  deleteRows: async (rowIndices: number[]) => {
    const conn = getActiveConn(get);
    if (!conn.activeTable) return;
    const unique = [...new Set(rowIndices.filter((i) => Number.isInteger(i) && i >= 0))];
    if (unique.length === 0) return;
    updateActive(get, set, () => ({
      selectedRows: new Set(unique),
      lastSelectedIndex: unique[unique.length - 1] ?? null,
    }));
    await get().deleteSelectedRows();
  },

  closeTable: (table: string) => {
    const conn = getActiveConn(get);
    const next = new Map(conn.tableStates);
    next.delete(table);
    const newActive = conn.activeTable === table ? null : conn.activeTable;
    commitPatch(get, set, null, { activeTable: newActive, tableStates: next });
  },

  reset: () =>
    set({
      perConnection: new Map(),
      activeDbSessionId: null,
      ...emptyConnectionTableState(),
      ...syncFlat(null, new Map()),
    }),
}));

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__tableDataStore = useTableDataStore;
}
