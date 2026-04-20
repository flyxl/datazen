import { create } from 'zustand';
import { databaseCommands } from '../commands/database';
import { queryCommands } from '../commands/query';
import type { ColumnSchema, FilterCondition, SortCondition, Value } from '../types';

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
  /** Snapshot of PK values from the row BEFORE mutation, used for WHERE clause. */
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

function escapeSqlIdent(name: string): string {
  return `"${name.replaceAll('"', '""')}"`;
}

interface TableDataStore {
  connectionId: string | null;
  tableName: string | null;
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
  reset: () => void;
}

export const useTableDataStore = create<TableDataStore>((set, get) => ({
  connectionId: null,
  tableName: null,
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

  loadTableData: async ({ connectionId, table }) => {
    const { page, pageSize, filters, sorts } = get();
    set({ loading: true, error: null, connectionId, tableName: table });
    try {
      const res = await databaseCommands.getTableData({
        connectionId,
        table,
        page,
        pageSize,
        filters,
        sorts,
      });
      set({
        columns: res.columns,
        rows: rowsToRecords(res.columns, res.rows),
        totalRows: res.totalRows ?? 0,
        page: res.page,
        pageSize: res.pageSize,
        loading: false,
        selectedRows: new Set(),
        editBuffer: new Map(),
        editingCell: null,
      });
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : '加载表数据失败',
      });
    }
  },

  setPage: (page) => {
    set({ page });
    const { connectionId, tableName } = get();
    if (connectionId && tableName) void get().loadTableData({ connectionId, table: tableName });
  },

  setPageSize: (size) => {
    set({ pageSize: size, page: 0 });
    const { connectionId, tableName } = get();
    if (connectionId && tableName) void get().loadTableData({ connectionId, table: tableName });
  },

  addFilter: (filter) =>
    set((s) => ({
      filters: [...s.filters, filter],
      page: 0,
    })),

  removeFilter: (index) =>
    set((s) => ({
      filters: s.filters.filter((_, i) => i !== index),
      page: 0,
    })),

  clearFilters: () => set({ filters: [], page: 0 }),

  setSort: (sort) => {
    set({ sorts: [sort], page: 0 });
    const { connectionId, tableName } = get();
    if (connectionId && tableName) void get().loadTableData({ connectionId, table: tableName });
  },

  startEdit: (row, col) => set({ editingCell: { row, col } }),

  updateCell: (row, col, value) => {
    const { rows, editBuffer, columns } = get();
    const rowObj = rows[row];
    if (!rowObj) return;
    const originalValue = rowObj[col];
    const key = editKey(row, col);

    const pkCols = columns.filter((c) => c.isPrimaryKey);
    const pkSnapshot: Record<string, unknown> = {};
    for (const pk of pkCols) {
      pkSnapshot[pk.name] = rowObj[pk.name];
    }

    const next = new Map(editBuffer);
    next.set(key, { rowIndex: row, columnName: col, originalValue, newValue: value, pkSnapshot });
    const nextRows = [...rows];
    nextRows[row] = { ...rowObj, [col]: value as Value };
    set({ rows: nextRows, editBuffer: next, editingCell: null });
    void get().commitChanges();
  },

  cancelEdit: () => set({ editingCell: null }),

  commitChanges: async () => {
    const { editBuffer, columns, connectionId, tableName } = get();
    if (editBuffer.size === 0 || !connectionId || !tableName) return;

    const pkCols = columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      set({ error: '无法提交更改：表没有主键' });
      return;
    }

    const snapshot = new Map(editBuffer);
    set({ editBuffer: new Map() });

    const editsByRow = new Map<number, CellEdit[]>();
    for (const edit of snapshot.values()) {
      const existing = editsByRow.get(edit.rowIndex) ?? [];
      existing.push(edit);
      editsByRow.set(edit.rowIndex, existing);
    }

    const statements: string[] = [];
    for (const [, edits] of editsByRow) {
      const setClauses = edits.map(
        (e) => `${escapeSqlIdent(e.columnName)} = ${escapeSqlValue(e.newValue)}`,
      );

      const { pkSnapshot } = edits[0];
      const whereClauses = pkCols.map((pk) => {
        const pkVal = pkSnapshot[pk.name];
        if (pkVal === null || pkVal === undefined) return `${escapeSqlIdent(pk.name)} IS NULL`;
        return `${escapeSqlIdent(pk.name)} = ${escapeSqlValue(pkVal)}`;
      });

      statements.push(
        `UPDATE ${escapeSqlIdent(tableName)} SET ${setClauses.join(', ')} WHERE ${whereClauses.join(' AND ')}`,
      );
    }

    try {
      for (const sql of statements) {
        await queryCommands.executeQuery(connectionId, sql);
      }
      void get().loadTableData({ connectionId, table: tableName });
    } catch (e) {
      for (const [key, edit] of snapshot) {
        get().editBuffer.set(key, edit);
      }
      set({ editBuffer: new Map(get().editBuffer), error: e instanceof Error ? e.message : '提交更改失败' });
    }
  },

  discardChanges: () => {
    const { editBuffer, connectionId, tableName } = get();
    if (editBuffer.size === 0) return;
    set({ editBuffer: new Map(), editingCell: null });
    if (connectionId && tableName) void get().loadTableData({ connectionId, table: tableName });
  },

  selectRow: (index, opts) => {
    const { lastSelectedIndex } = get();
    if (opts?.range && lastSelectedIndex !== null) {
      const lo = Math.min(lastSelectedIndex, index);
      const hi = Math.max(lastSelectedIndex, index);
      const next = new Set(get().selectedRows);
      for (let i = lo; i <= hi; i += 1) next.add(i);
      set({ selectedRows: next, lastSelectedIndex: index });
    } else if (opts?.multi) {
      const next = new Set(get().selectedRows);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      set({ selectedRows: next, lastSelectedIndex: index });
    } else {
      set({ selectedRows: new Set([index]), lastSelectedIndex: index });
    }
  },

  toggleSelectAll: () =>
    set((s) => {
      const allSelected = s.selectedRows.size === s.rows.length && s.rows.length > 0;
      if (allSelected) return { selectedRows: new Set(), lastSelectedIndex: null };
      const next = new Set<number>();
      for (let i = 0; i < s.rows.length; i += 1) next.add(i);
      return { selectedRows: next, lastSelectedIndex: null };
    }),

  deleteSelectedRows: async () => {
    set((s) => ({
      rows: s.rows.filter((_, i) => !s.selectedRows.has(i)),
      selectedRows: new Set(),
    }));
  },

  reset: () =>
    set({
      connectionId: null,
      tableName: null,
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
    }),
}));

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__tableDataStore = useTableDataStore;
}
