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
  /** In-progress filter editor state; committed via applyFilters(). */
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

interface TableDataStore {
  connectionId: string | null;
  databaseType: string | null;
  activeTable: string | null;
  tableStates: Map<string, TableState>;

  /** Convenience selectors that read from the active table's state */
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

  setDatabaseType: (dbType: string) => void;
  switchToTable: (table: string) => void;
  loadTableData: (params: {
    connectionId: string;
    table: string;
    skipCount?: boolean;
  }) => Promise<void>;
  setPage: (page: number) => void;
  setPageSize: (size: number) => void;
  /** Draft-only; opens the filter panel. Does not query until applyFilters(). */
  addFilter: (filter: FilterCondition) => void;
  /** Apply immediately (e.g. AI NL filter). Updates draft + applied and reloads. */
  setFilters: (filters: FilterCondition[]) => void;
  updateFilter: (index: number, filter: FilterCondition) => void;
  setFilterLogic: (logic: 'and' | 'or') => void;
  removeFilter: (index: number) => void;
  /** Clears draft + applied and reloads. */
  clearFilters: () => void;
  /** Commits draft → applied and reloads. */
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
  /** Delete by explicit page-row indices (selects them first). */
  deleteRows: (rowIndices: number[]) => Promise<void>;
  closeTable: (table: string) => void;

  detailRowIndex: number | null;
  setDetailRow: (index: number | null) => void;
  reset: () => void;
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

function reloadActive(get: () => TableDataStore): void {
  const { connectionId, activeTable } = get();
  if (connectionId && activeTable) {
    void get().loadTableData({ connectionId, table: activeTable });
  }
}

function updateActive(
  get: () => TableDataStore,
  set: (partial: Partial<TableDataStore>) => void,
  updater: (ts: TableState) => Partial<TableState>,
): void {
  const { activeTable, tableStates } = get();
  if (!activeTable) return;
  const current = getState(tableStates, activeTable);
  const patched = { ...current, ...updater(current) };
  const next = new Map(tableStates);
  next.set(activeTable, patched);
  set({ tableStates: next, ...syncFlat(activeTable, next) });
}

export const useTableDataStore = create<TableDataStore>((set, get) => ({
  connectionId: null,
  databaseType: null,
  activeTable: null,
  tableStates: new Map(),
  ...syncFlat(null, new Map()),

  detailRowIndex: null,
  setDetailRow: (index) => set({ detailRowIndex: index }),

  setDatabaseType: (dbType: string) => set({ databaseType: dbType }),

  switchToTable: (table: string) => {
    const { tableStates } = get();
    set({ activeTable: table, ...syncFlat(table, tableStates) });
  },

  loadTableData: async ({ connectionId, table, skipCount }) => {
    const { tableStates, databaseType } = get();
    const existing = tableStates.get(table) ?? emptyTableState();

    // Prevent duplicate concurrent loads for the same table
    if (existing.loading) return;

    const { page, filters, sorts, filterLogic } = existing;
    const driverPageSize = DB_REGISTRY[databaseType as DatabaseType]?.defaultPageSize;
    const settingsPageSize = useSettingsStore.getState().settings.defaultPageSize;
    const pageSize =
      existing.columns.length > 0
        ? existing.pageSize
        : settingsPageSize || driverPageSize || existing.pageSize;

    const next = new Map(tableStates);
    next.set(table, { ...existing, loading: true, error: null });
    set({
      connectionId,
      activeTable: table,
      tableStates: next,
      ...syncFlat(table, next),
    });

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
      const updated = new Map(get().tableStates);
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
      set({
        tableStates: updated,
        ...(get().activeTable === table ? syncFlat(table, updated) : {}),
      });
    } catch (e) {
      const updated = new Map(get().tableStates);
      const ts = updated.get(table) ?? emptyTableState();
      updated.set(table, {
        ...ts,
        loading: false,
        error: extractErrorMessage(e, t('tableData.loadFailed')),
      });
      set({
        tableStates: updated,
        ...(get().activeTable === table ? syncFlat(table, updated) : {}),
      });
    }
  },

  setPage: (page) => {
    updateActive(get, set, () => ({ page }));
    const { connectionId, activeTable } = get();
    if (connectionId && activeTable)
      void get().loadTableData({ connectionId, table: activeTable, skipCount: true });
  },

  setPageSize: (size) => {
    updateActive(get, set, () => ({ pageSize: size, page: 0 }));
    const { connectionId, activeTable } = get();
    if (connectionId && activeTable)
      void get().loadTableData({ connectionId, table: activeTable, skipCount: true });
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
    const { activeTable, tableStates } = get();
    if (!activeTable) return;
    const ts = getState(tableStates, activeTable);
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
    const { connectionId, activeTable } = get();
    if (connectionId && activeTable)
      void get().loadTableData({ connectionId, table: activeTable, skipCount: true });
  },

  startEdit: (row, col) => updateActive(get, set, () => ({ editingCell: { row, col } })),

  updateCell: (row, col, value) => {
    const { activeTable, tableStates } = get();
    if (!activeTable) return;
    const ts = getState(tableStates, activeTable);
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

    const next = new Map(tableStates);
    next.set(activeTable, { ...ts, rows: nextRows, editBuffer: nextBuffer, editingCell: null });
    set({ tableStates: next, ...syncFlat(activeTable, next) });
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
    const { activeTable, tableStates, connectionId } = get();
    if (!activeTable || !connectionId) return;
    const ts = getState(tableStates, activeTable);
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
      await databaseCommands.commitRowUpdates(connectionId, activeTable, batches);
      void get().loadTableData({ connectionId, table: activeTable });
    } catch (e) {
      const current = getState(get().tableStates, activeTable);
      const merged = new Map(current.editBuffer);
      for (const [key, edit] of snapshot) merged.set(key, edit);
      updateActive(get, set, () => ({
        editBuffer: merged,
        error: extractErrorMessage(e, t('tableData.commitFailed')),
      }));
    }
  },

  discardChanges: () => {
    const { activeTable, connectionId } = get();
    if (!activeTable) return;
    updateActive(get, set, () => ({ editBuffer: new Map(), editingCell: null }));
    if (connectionId) void get().loadTableData({ connectionId, table: activeTable });
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
    const { activeTable, tableStates, connectionId } = get();
    if (!activeTable || !connectionId) return;
    const ts = getState(tableStates, activeTable);
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
      await databaseCommands.commitRowDeletes(connectionId, activeTable, deletes);
      void get().loadTableData({ connectionId, table: activeTable });
    } catch (e) {
      updateActive(get, set, () => ({
        error: extractErrorMessage(e, t('tableData.deleteFailed')),
      }));
    }
  },

  /** Delete specific row indices (e.g. right-clicked row when nothing is selected). */
  deleteRows: async (rowIndices: number[]) => {
    const { activeTable, tableStates } = get();
    if (!activeTable) return;
    const unique = [...new Set(rowIndices.filter((i) => Number.isInteger(i) && i >= 0))];
    if (unique.length === 0) return;
    updateActive(get, set, () => ({
      selectedRows: new Set(unique),
      lastSelectedIndex: unique[unique.length - 1] ?? null,
    }));
    await get().deleteSelectedRows();
  },

  closeTable: (table: string) => {
    const { tableStates, activeTable } = get();
    const next = new Map(tableStates);
    next.delete(table);
    const newActive = activeTable === table ? null : activeTable;
    set({ tableStates: next, activeTable: newActive, ...syncFlat(newActive, next) });
  },

  reset: () =>
    set({
      connectionId: null,
      databaseType: null,
      activeTable: null,
      tableStates: new Map(),
      ...syncFlat(null, new Map()),
    }),
}));

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__tableDataStore = useTableDataStore;
}
