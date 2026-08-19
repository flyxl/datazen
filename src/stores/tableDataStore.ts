import { create } from 'zustand';
import { databaseCommands, type RowUpdateBatch, type RowDeleteBatch } from '../commands/database';
import { t } from '../locales/t';
import type { ColumnSchema, DatabaseType, FilterCondition, SortCondition, Value } from '../types';
import { DB_REGISTRY } from '../lib/databaseTypes';
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
  tableStates: Map<string, TableState>;
  detailRowIndex: number | null;
}

function emptyConnectionTableState(): ConnectionTableState {
  return {
    activeTable: null,
    databaseType: null,
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
    selectedRows: ts.selectedRows,
    lastSelectedIndex: ts.lastSelectedIndex,
    editingCell: ts.editingCell,
    loading: ts.loading,
    error: ts.error,
  };
}

function flattenActive(
  perConnection: Map<string, ConnectionTableState>,
  activeConnectionId: string | null,
): ConnectionTableState & ReturnType<typeof syncFlat> {
  const cs = activeConnectionId
    ? (perConnection.get(activeConnectionId) ?? emptyConnectionTableState())
    : emptyConnectionTableState();
  return {
    ...cs,
    ...syncFlat(cs.activeTable, cs.tableStates),
  };
}

function patchConnection(
  perConnection: Map<string, ConnectionTableState>,
  connectionId: string,
  patch: Partial<ConnectionTableState>,
): Map<string, ConnectionTableState> {
  const next = new Map(perConnection);
  const current = perConnection.get(connectionId) ?? emptyConnectionTableState();
  next.set(connectionId, { ...current, ...patch });
  return next;
}

// ── Store ─────────────────────────────────────────────────────────

interface TableDataStore extends ConnectionTableState {
  perConnection: Map<string, ConnectionTableState>;
  activeConnectionId: string | null;

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
  selectedRows: Set<number>;
  lastSelectedIndex: number | null;
  editingCell: { row: number; col: string } | null;
  loading: boolean;
  error: string | null;
  tableName: string | null;

  setActiveConnection: (connectionId: string | null) => void;
  removeConnection: (connectionId: string) => void;
  setDatabaseType: (dbType: string) => void;
  switchToTable: (table: string) => void;
  loadTableData: (params: {
    connectionId: string;
    table: string;
    skipCount?: boolean;
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
  updateCell: (row: number, col: string, value: unknown) => void;
  applyColumnToRows: (col: string, value: unknown, rows: number[]) => void;
  cancelEdit: () => void;
  commitChanges: () => Promise<void>;
  discardChanges: () => void;
  selectRow: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  toggleSelectAll: () => void;
  deleteSelectedRows: () => Promise<void>;
  deleteRows: (rowIndices: number[]) => Promise<void>;
  closeTable: (table: string) => void;

  setDetailRow: (index: number | null) => void;
  reset: () => void;
}

/** Get the active connection's state. */
function getActiveConn(get: () => TableDataStore): ConnectionTableState {
  const { activeConnectionId, perConnection } = get();
  if (!activeConnectionId) return emptyConnectionTableState();
  return perConnection.get(activeConnectionId) ?? emptyConnectionTableState();
}

/** Commit a per-connection patch and re-flatten. */
function commitPatch(
  get: () => TableDataStore,
  set: (partial: Partial<TableDataStore>) => void,
  connectionId: string | null,
  patch: Partial<ConnectionTableState>,
): void {
  const state = get();
  const cid = connectionId ?? state.activeConnectionId;
  if (!cid) return;
  const perConnection = patchConnection(state.perConnection, cid, patch);
  set({ perConnection, ...flattenActive(perConnection, state.activeConnectionId) });
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

function reloadActive(get: () => TableDataStore): void {
  const conn = getActiveConn(get);
  const { activeConnectionId } = get();
  if (activeConnectionId && conn.activeTable) {
    void get().loadTableData({ connectionId: activeConnectionId, table: conn.activeTable });
  }
}

export const useTableDataStore = create<TableDataStore>((set, get) => ({
  perConnection: new Map(),
  activeConnectionId: null,
  ...emptyConnectionTableState(),
  ...syncFlat(null, new Map()),

  setActiveConnection: (connectionId) => {
    const state = get();
    let perConnection = state.perConnection;
    if (connectionId && !perConnection.has(connectionId)) {
      perConnection = new Map(perConnection);
      perConnection.set(connectionId, emptyConnectionTableState());
    }
    set({
      perConnection,
      activeConnectionId: connectionId,
      ...flattenActive(perConnection, connectionId),
    });
  },

  removeConnection: (connectionId) => {
    const state = get();
    const perConnection = new Map(state.perConnection);
    perConnection.delete(connectionId);
    const activeConnectionId =
      state.activeConnectionId === connectionId ? null : state.activeConnectionId;
    set({
      perConnection,
      activeConnectionId,
      ...flattenActive(perConnection, activeConnectionId),
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

  loadTableData: async ({ connectionId, table, skipCount }) => {
    const state = get();
    const connState = state.perConnection.get(connectionId) ?? emptyConnectionTableState();
    const existing = connState.tableStates.get(table) ?? emptyTableState();

    if (existing.loading) return;

    const { page, filters, sorts, filterLogic } = existing;
    const driverPageSize = DB_REGISTRY[connState.databaseType as DatabaseType]?.defaultPageSize;
    const settingsPageSize = useSettingsStore.getState().settings.defaultPageSize;
    const pageSize =
      existing.columns.length > 0
        ? existing.pageSize
        : settingsPageSize || driverPageSize || existing.pageSize;

    const nextStates = new Map(connState.tableStates);
    nextStates.set(table, { ...existing, loading: true, error: null });
    commitPatch(get, set, connectionId, { activeTable: table, tableStates: nextStates });

    try {
      const res = await databaseCommands.getTableData({
        connectionId,
        table,
        page,
        pageSize,
        filters: filters.filter(isCompleteFilter),
        sorts,
        skipCount,
        filterLogic,
      });
      const latestConn = get().perConnection.get(connectionId) ?? emptyConnectionTableState();
      const updated = new Map(latestConn.tableStates);
      const ts = updated.get(table) ?? emptyTableState();
      const patched: TableState = {
        ...ts,
        columns: res.columns,
        rows: rowsToRecords(res.columns, res.rows),
        totalRows: res.totalRows ?? ts.totalRows,
        page: res.page,
        pageSize: res.pageSize,
        loading: false,
        selectedRows: new Set(),
        editBuffer: new Map(),
        editingCell: null,
        error: null,
      };
      updated.set(table, patched);
      commitPatch(get, set, connectionId, { tableStates: updated });
    } catch (e) {
      const latestConn = get().perConnection.get(connectionId) ?? emptyConnectionTableState();
      const updated = new Map(latestConn.tableStates);
      const ts = updated.get(table) ?? emptyTableState();
      updated.set(table, {
        ...ts,
        loading: false,
        error: extractErrorMessage(e, t('tableData.loadFailed')),
      });
      commitPatch(get, set, connectionId, { tableStates: updated });
    }
  },

  setPage: (page) => {
    updateActive(get, set, () => ({ page }));
    const conn = getActiveConn(get);
    const { activeConnectionId } = get();
    if (activeConnectionId && conn.activeTable)
      void get().loadTableData({
        connectionId: activeConnectionId,
        table: conn.activeTable,
        skipCount: true,
      });
  },

  setPageSize: (size) => {
    updateActive(get, set, () => ({ pageSize: size, page: 0 }));
    const conn = getActiveConn(get);
    const { activeConnectionId } = get();
    if (activeConnectionId && conn.activeTable)
      void get().loadTableData({
        connectionId: activeConnectionId,
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
    const { activeConnectionId } = get();
    if (activeConnectionId && conn.activeTable)
      void get().loadTableData({
        connectionId: activeConnectionId,
        table: conn.activeTable,
        skipCount: true,
      });
  },

  startEdit: (row, col) => updateActive(get, set, () => ({ editingCell: { row, col } })),

  updateCell: (row, col, value) => {
    const conn = getActiveConn(get);
    if (!conn.activeTable) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    const rowObj = ts.rows[row];
    if (!rowObj) return;
    const originalValue = rowObj[col];
    const key = editKey(row, col);

    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    const pkSnapshot: Record<string, unknown> = {};
    for (const pk of pkCols) {
      pkSnapshot[pk.name] = rowObj[pk.name];
    }

    const nextBuffer = new Map(ts.editBuffer);
    nextBuffer.set(key, {
      rowIndex: row,
      columnName: col,
      originalValue,
      newValue: value,
      pkSnapshot,
    });
    const nextRows = [...ts.rows];
    nextRows[row] = { ...rowObj, [col]: value as Value };

    const next = new Map(conn.tableStates);
    next.set(conn.activeTable, {
      ...ts,
      rows: nextRows,
      editBuffer: nextBuffer,
      editingCell: null,
    });
    commitPatch(get, set, null, { tableStates: next });
    void get().commitChanges();
  },

  applyColumnToRows: (col, value, rows) => {
    const conn = getActiveConn(get);
    if (!conn.activeTable) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    const nextBuffer = new Map(ts.editBuffer);
    const nextRows = [...ts.rows];

    for (const row of rows) {
      const rowObj = nextRows[row];
      if (!rowObj) continue;
      const pkSnapshot: Record<string, unknown> = {};
      for (const pk of pkCols) {
        pkSnapshot[pk.name] = rowObj[pk.name];
      }
      const key = editKey(row, col);
      nextBuffer.set(key, {
        rowIndex: row,
        columnName: col,
        originalValue: rowObj[col],
        newValue: value,
        pkSnapshot,
      });
      nextRows[row] = { ...rowObj, [col]: value as Value };
    }

    if (nextBuffer.size === 0) return;
    const next = new Map(conn.tableStates);
    next.set(conn.activeTable, {
      ...ts,
      rows: nextRows,
      editBuffer: nextBuffer,
      editingCell: null,
    });
    commitPatch(get, set, null, { tableStates: next });
    void get().commitChanges();
  },

  applyColumnToRows: (col, value, rows) => {
    const { activeTable, tableStates } = get();
    if (!activeTable) return;
    const ts = getState(tableStates, activeTable);
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    const nextBuffer = new Map(ts.editBuffer);
    const nextRows = [...ts.rows];

    for (const row of rows) {
      const rowObj = nextRows[row];
      if (!rowObj) continue;
      const pkSnapshot: Record<string, unknown> = {};
      for (const pk of pkCols) {
        pkSnapshot[pk.name] = rowObj[pk.name];
      }
      const key = editKey(row, col);
      nextBuffer.set(key, {
        rowIndex: row,
        columnName: col,
        originalValue: rowObj[col],
        newValue: value,
        pkSnapshot,
      });
      nextRows[row] = { ...rowObj, [col]: value as Value };
    }

    if (nextBuffer.size === 0) return;
    const next = new Map(tableStates);
    next.set(activeTable, { ...ts, rows: nextRows, editBuffer: nextBuffer, editingCell: null });
    set({ tableStates: next, ...syncFlat(activeTable, next) });
    void get().commitChanges();
  },

  cancelEdit: () => updateActive(get, set, () => ({ editingCell: null })),

  commitChanges: async () => {
    const conn = getActiveConn(get);
    const { activeConnectionId } = get();
    if (!conn.activeTable || !activeConnectionId) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    if (ts.editBuffer.size === 0) return;

    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      updateActive(get, set, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }

    const snapshot = new Map(ts.editBuffer);
    updateActive(get, set, () => ({ editBuffer: new Map() }));

    const editsByRow = new Map<number, CellEdit[]>();
    for (const edit of snapshot.values()) {
      const existing = editsByRow.get(edit.rowIndex) ?? [];
      existing.push(edit);
      editsByRow.set(edit.rowIndex, existing);
    }

    const batches: RowUpdateBatch[] = [];
    for (const [, edits] of editsByRow) {
      const { pkSnapshot } = edits[0];
      batches.push({
        setColumns: edits.map((e) => ({
          column: e.columnName,
          value: toCellValue(e.newValue),
        })),
        pkColumns: pkCols.map((pk) => ({
          column: pk.name,
          value: toCellValue(pkSnapshot[pk.name]),
        })),
      });
    }

    try {
      await databaseCommands.commitRowUpdates(activeConnectionId, conn.activeTable, batches);
      void get().loadTableData({ connectionId: activeConnectionId, table: conn.activeTable });
    } catch (e) {
      const latestConn = getActiveConn(get);
      const current = getState(latestConn.tableStates, conn.activeTable);
      const merged = new Map(current.editBuffer);
      for (const [key, edit] of snapshot) merged.set(key, edit);
      updateActive(get, set, () => ({
        editBuffer: merged,
        error: extractErrorMessage(e, t('tableData.commitFailed')),
      }));
    }
  },

  discardChanges: () => {
    const conn = getActiveConn(get);
    const { activeConnectionId } = get();
    if (!conn.activeTable) return;
    updateActive(get, set, () => ({ editBuffer: new Map(), editingCell: null }));
    if (activeConnectionId)
      void get().loadTableData({ connectionId: activeConnectionId, table: conn.activeTable });
  },

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
    const { activeConnectionId } = get();
    if (!conn.activeTable || !activeConnectionId) return;
    const ts = getState(conn.tableStates, conn.activeTable);
    const indices = Array.from(ts.selectedRows).sort((a, b) => a - b);
    if (indices.length === 0) return;

    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      updateActive(get, set, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }

    const deletes: RowDeleteBatch[] = [];
    for (const index of indices) {
      const row = ts.rows[index];
      if (!row) continue;
      deletes.push({
        pkColumns: pkCols.map((pk) => ({
          column: pk.name,
          value: toCellValue(row[pk.name]),
        })),
      });
    }
    if (deletes.length === 0) return;

    try {
      await databaseCommands.commitRowDeletes(activeConnectionId, conn.activeTable, deletes);
      void get().loadTableData({ connectionId: activeConnectionId, table: conn.activeTable });
    } catch (e) {
      updateActive(get, set, () => ({
        error: extractErrorMessage(e, t('tableData.deleteFailed')),
      }));
    }
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
      activeConnectionId: null,
      ...emptyConnectionTableState(),
      ...syncFlat(null, new Map()),
    }),
}));

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__tableDataStore = useTableDataStore;
}
