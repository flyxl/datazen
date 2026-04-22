import { create } from 'zustand';
import { databaseCommands } from '../commands/database';
import { queryCommands } from '../commands/query';
import { t } from '../locales/t';
import type { ColumnSchema, DatabaseType, FilterCondition, SortCondition, Value } from '../types';
import { escapeIdent } from '../lib/databaseTypes';

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

function escapeSqlValue(val: unknown): string {
  if (val === null || val === undefined) return 'NULL';
  if (typeof val === 'number') return String(val);
  if (typeof val === 'boolean') return val ? 'TRUE' : 'FALSE';
  if (typeof val === 'object') return `'${JSON.stringify(val).replaceAll("'", "''")}'`;
  return `'${String(val).replaceAll("'", "''")}'`;
}

function escapeSqlIdent(name: string, dbType?: string): string {
  return escapeIdent(name, dbType as DatabaseType | undefined);
}

/** Per-table state slice */
export interface TableState {
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];
  totalRows: number;
  page: number;
  pageSize: number;
  filters: FilterCondition[];
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
  loadTableData: (params: { connectionId: string; table: string }) => Promise<void>;
  setPage: (page: number) => void;
  setPageSize: (size: number) => void;
  addFilter: (filter: FilterCondition) => void;
  removeFilter: (index: number) => void;
  clearFilters: () => void;
  setSort: (sort: SortCondition) => void;
  startEdit: (row: number, col: string) => void;
  updateCell: (row: number, col: string, value: unknown) => void;
  cancelEdit: () => void;
  commitChanges: () => Promise<void>;
  discardChanges: () => void;
  selectRow: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  toggleSelectAll: () => void;
  deleteSelectedRows: () => Promise<void>;
  closeTable: (table: string) => void;
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
    sorts: ts.sorts,
    editBuffer: ts.editBuffer,
    selectedRows: ts.selectedRows,
    lastSelectedIndex: ts.lastSelectedIndex,
    editingCell: ts.editingCell,
    loading: ts.loading,
    error: ts.error,
  };
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

  setDatabaseType: (dbType: string) => set({ databaseType: dbType }),

  switchToTable: (table: string) => {
    const { tableStates } = get();
    set({ activeTable: table, ...syncFlat(table, tableStates) });
  },

  loadTableData: async ({ connectionId, table }) => {
    const { tableStates } = get();
    const existing = tableStates.get(table) ?? emptyTableState();
    const { page, pageSize, filters, sorts } = existing;

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
        filters,
        sorts,
      });
      const updated = new Map(get().tableStates);
      const ts = updated.get(table) ?? emptyTableState();
      const patched: TableState = {
        ...ts,
        columns: res.columns,
        rows: rowsToRecords(res.columns, res.rows),
        totalRows: res.totalRows ?? 0,
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
        error: e instanceof Error ? e.message : t('tableData.loadFailed'),
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
    if (connectionId && activeTable) void get().loadTableData({ connectionId, table: activeTable });
  },

  setPageSize: (size) => {
    updateActive(get, set, () => ({ pageSize: size, page: 0 }));
    const { connectionId, activeTable } = get();
    if (connectionId && activeTable) void get().loadTableData({ connectionId, table: activeTable });
  },

  addFilter: (filter) => updateActive(get, set, (ts) => ({
    filters: [...ts.filters, filter],
    page: 0,
  })),

  removeFilter: (index) => updateActive(get, set, (ts) => ({
    filters: ts.filters.filter((_, i) => i !== index),
    page: 0,
  })),

  clearFilters: () => updateActive(get, set, () => ({ filters: [], page: 0 })),

  setSort: (sort) => {
    updateActive(get, set, () => ({ sorts: [sort], page: 0 }));
    const { connectionId, activeTable } = get();
    if (connectionId && activeTable) void get().loadTableData({ connectionId, table: activeTable });
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
    nextBuffer.set(key, { rowIndex: row, columnName: col, originalValue, newValue: value, pkSnapshot });
    const nextRows = [...ts.rows];
    nextRows[row] = { ...rowObj, [col]: value as Value };

    const next = new Map(tableStates);
    next.set(activeTable, { ...ts, rows: nextRows, editBuffer: nextBuffer, editingCell: null });
    set({ tableStates: next, ...syncFlat(activeTable, next) });
    void get().commitChanges();
  },

  cancelEdit: () => updateActive(get, set, () => ({ editingCell: null })),

  commitChanges: async () => {
    const { activeTable, tableStates, connectionId, databaseType } = get();
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

    const dbType = databaseType ?? undefined;
    const statements: string[] = [];
    for (const [, edits] of editsByRow) {
      const setClauses = edits.map(
        (e) => `${escapeSqlIdent(e.columnName, dbType)} = ${escapeSqlValue(e.newValue)}`,
      );
      const { pkSnapshot } = edits[0];
      const whereClauses = pkCols.map((pk) => {
        const pkVal = pkSnapshot[pk.name];
        if (pkVal === null || pkVal === undefined) return `${escapeSqlIdent(pk.name, dbType)} IS NULL`;
        return `${escapeSqlIdent(pk.name, dbType)} = ${escapeSqlValue(pkVal)}`;
      });
      statements.push(
        `UPDATE ${escapeSqlIdent(activeTable, dbType)} SET ${setClauses.join(', ')} WHERE ${whereClauses.join(' AND ')}`,
      );
    }

    try {
      for (const sql of statements) {
        await queryCommands.executeQuery(connectionId, sql);
      }
      void get().loadTableData({ connectionId, table: activeTable });
    } catch (e) {
      const current = getState(get().tableStates, activeTable);
      const merged = new Map(current.editBuffer);
      for (const [key, edit] of snapshot) merged.set(key, edit);
      updateActive(get, set, () => ({
        editBuffer: merged,
        error: e instanceof Error ? e.message : t('tableData.commitFailed'),
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
    updateActive(get, set, (ts) => ({
      rows: ts.rows.filter((_, i) => !ts.selectedRows.has(i)),
      selectedRows: new Set(),
    }));
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
