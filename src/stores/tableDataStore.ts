import { create } from 'zustand';
import { tableManualHandle } from '../lib/session/tableSession';
import { databaseCommands } from '../commands/database';
import { t } from '../locales/t';
import type { DatabaseType, FilterCondition, SortCondition } from '../types';
import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  buildRowIdentity,
  duplicateRowIdentityKeys,
  isCompleteTableChangeContext,
  rowIdentityKey,
  tableChangeContextKey,
  valuesEqual,
  type CommitPendingChangesResult,
  type RowChangePlan,
  type TableChangeContext,
} from '../lib/tableChanges';
import { useSettingsStore } from './settingsStore';
import type { TableState } from './tableData/types';
import { cloneFilters, filterDraftEqualsApplied, isCompleteFilter } from './tableData/filterUtils';
import {
  buildTableContext,
  emptyTableState,
  extractErrorMessage,
  rowsToRecords,
  toCellValue,
  type TableTarget,
} from './tableData/connectionState';
import {
  AMBIGUOUS_ROW_IDENTITY_ERROR,
  findPendingForRow,
  hasPendingIdentityCollision,
  overlayPendingRows,
  pendingChangesForWire,
  pendingChangesSignature,
  rebuildEditBuffer,
  rowIdentityIsUnique,
} from './tableData/pendingChanges';

// ── Store ─────────────────────────────────────────────────────────

export interface LoadTableDataParams extends TableTarget {
  /** Table/view panel this load belongs to; its state slice is keyed by this id. */
  panelId: string;
  /** Reuse the cached total instead of running COUNT(*) (pagination/sort re-reads). */
  skipCount?: boolean;
}

/** Every action is scoped to one table/view panel, like `panelStore`'s queryExec map. */
interface TableDataStore {
  byPanel: Map<string, TableState>;

  loadTableData: (params: LoadTableDataParams) => Promise<void>;
  /** Re-run the load a panel last performed, keeping its page/filters/sorts. */
  reloadPanel: (panelId: string, opts?: { skipCount?: boolean }) => void;
  /** Drop a panel's slice entirely — called when the tab closes. */
  removePanel: (panelId: string) => void;
  /**
   * Drop cached rows of a session's panels so the next render re-fetches. Pass
   * `tableName` to hit only that table. Panels with staged (uncommitted) edits
   * are never touched.
   */
  invalidateCachedData: (dbSessionId: string, tableName?: string) => void;
  reset: () => void;

  setPage: (panelId: string, page: number) => void;
  setPageSize: (panelId: string, size: number) => void;
  addFilter: (panelId: string, filter: FilterCondition) => void;
  setFilters: (panelId: string, filters: FilterCondition[], logic?: 'and' | 'or') => void;
  updateFilter: (panelId: string, index: number, filter: FilterCondition) => void;
  setFilterLogic: (panelId: string, logic: 'and' | 'or') => void;
  removeFilter: (panelId: string, index: number) => void;
  clearFilters: (panelId: string) => void;
  applyFilters: (panelId: string) => void;
  setFilterPanelOpen: (panelId: string, open: boolean) => void;
  setVisibleColumns: (panelId: string, columns: string[] | null) => void;
  setSort: (panelId: string, sort: SortCondition) => void;
  setDetailRow: (panelId: string, index: number | null) => void;

  startEdit: (panelId: string, row: number, col: string) => void;
  cancelEdit: (panelId: string) => void;
  selectRow: (panelId: string, index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  toggleSelectAll: (panelId: string) => void;
  stageCellChange: (panelId: string, row: number, col: string, value: unknown) => void;
  applyColumnToRows: (panelId: string, col: string, value: unknown, rows: number[]) => void;
  stageRowDelete: (panelId: string, rowIndices: number | number[]) => void;
  deleteRows: (panelId: string, rowIndices: number[]) => void;
  rollbackPendingChanges: (panelId: string) => void;
  previewPendingChanges: (panelId: string) => Promise<RowChangePlan | null>;
  commitPendingChanges: (panelId: string) => Promise<CommitPendingChangesResult>;
}

type Getter = () => TableDataStore;
type Setter = (partial: Partial<TableDataStore>) => void;

/** Write one panel's slice. Panels that never loaded have no slice, so writes are no-ops. */
function patchPanel(
  get: Getter,
  set: Setter,
  panelId: string,
  updater: (ts: TableState) => Partial<TableState>,
): void {
  const current = get().byPanel.get(panelId);
  if (!current) return;
  const next = new Map(get().byPanel);
  next.set(panelId, { ...current, ...updater(current) });
  set({ byPanel: next });
}

/** Same as {@link patchPanel} but marks a page/filter/sort transition, so an older response can never commit. */
function patchPanelForReload(
  get: Getter,
  set: Setter,
  panelId: string,
  updater: (ts: TableState) => Partial<TableState>,
): void {
  patchPanel(get, set, panelId, (ts) => ({
    ...updater(ts),
    requestRevision: ts.requestRevision + 1,
  }));
}

function targetOf(context: TableChangeContext): TableTarget {
  return {
    dbSessionId: context.dbSessionId,
    table: context.table,
    connectionId: context.connectionId,
    driverType: context.driverType,
    database: context.database,
    schema: context.schema,
  };
}

/** Apply a successful page fetch to the panel slice, unless it was superseded meanwhile. */
function commitFetchedPage(
  get: Getter,
  set: Setter,
  panelId: string,
  context: TableChangeContext,
  requestRevision: number,
  res: Awaited<ReturnType<typeof databaseCommands.getTableData>>,
): void {
  const current = get().byPanel.get(panelId);
  if (!current) return; // panel closed while the request was in flight
  if (current.requestRevision !== requestRevision || current.loadingRevision !== requestRevision)
    return;

  const fetchedRows = rowsToRecords(res.columns, res.rows);
  const pkColumns = res.columns.filter((column) => column.isPrimaryKey);
  const duplicateIdentityKeys = duplicateRowIdentityKeys(fetchedRows, pkColumns);
  const anchors = new Map(current.rowIdentityAnchors);
  fetchedRows.forEach((row, rowIndex) => {
    const identity = buildRowIdentity(row, pkColumns);
    if (identity) anchors.set(rowIndex, rowIdentityKey(identity));
    else anchors.delete(rowIndex);
  });
  const rowsWithPending = overlayPendingRows(
    { ...current, columns: res.columns, rows: fetchedRows, rowIdentityAnchors: anchors },
    fetchedRows,
  );
  const validColumnNames = new Set(res.columns.map((c) => c.name));
  const sanitizedVisible = current.visibleColumns
    ? current.visibleColumns.filter((c) => validColumnNames.has(c))
    : null;

  const next = new Map(get().byPanel);
  next.set(panelId, {
    ...current,
    columns: res.columns,
    visibleColumns:
      sanitizedVisible && sanitizedVisible.length === res.columns.length ? null : sanitizedVisible,
    rows: rowsWithPending,
    totalRows: res.totalRows ?? current.totalRows,
    page: res.page,
    pageSize: res.pageSize,
    context,
    rowIdentityAnchors: anchors,
    loading: false,
    loadingRevision: null,
    selectedRows: new Set(),
    editBuffer: rebuildEditBuffer({
      ...current,
      columns: res.columns,
      rows: rowsWithPending,
      rowIdentityAnchors: anchors,
    }),
    editingCell: null,
    error: duplicateIdentityKeys.length > 0 ? AMBIGUOUS_ROW_IDENTITY_ERROR : null,
  });
  set({ byPanel: next });
}

export const useTableDataStore = create<TableDataStore>((set, get) => ({
  byPanel: new Map(),

  loadTableData: async ({ panelId, skipCount, ...target }) => {
    const context = buildTableContext(target);
    const existing = get().byPanel.get(panelId) ?? emptyTableState(context);
    const requestRevision = existing.requestRevision;
    if (existing.loading && existing.loadingRevision === requestRevision) return;

    const { page, filters, sorts, filterLogic } = existing;
    const driverPageSize = DB_REGISTRY[context.driverType as DatabaseType]?.defaultPageSize;
    const settingsPageSize = useSettingsStore.getState().settings.defaultPageSize;
    const pageSize =
      existing.columns.length > 0
        ? existing.pageSize
        : settingsPageSize || driverPageSize || existing.pageSize;

    const states = new Map(get().byPanel);
    states.set(panelId, {
      ...existing,
      context,
      pageSize,
      loading: true,
      loadingRevision: requestRevision,
      error: null,
    });
    set({ byPanel: states });

    try {
      const res = await databaseCommands.getTableData({
        context,
        manualHandle: tableManualHandle(panelId),
        dbSessionId: context.dbSessionId,
        table: context.table,
        page,
        pageSize,
        filters: filters.filter(isCompleteFilter),
        sorts,
        skipCount,
        filterLogic,
        database: context.database,
        // The table's own schema, not a guess: without it the host falls back
        // to the connection default and a table outside it reads as missing.
        schema: context.schema ?? null,
      });
      commitFetchedPage(get, set, panelId, context, requestRevision, res);
    } catch (e) {
      patchPanel(get, set, panelId, (ts) =>
        ts.requestRevision === requestRevision && ts.loadingRevision === requestRevision
          ? {
              loading: false,
              loadingRevision: null,
              error: extractErrorMessage(e, t('tableData.loadFailed')),
            }
          : {},
      );
    }
  },

  reloadPanel: (panelId, opts) => {
    const context = get().byPanel.get(panelId)?.context;
    if (!context) return;
    void get().loadTableData({ panelId, ...targetOf(context), skipCount: opts?.skipCount });
  },

  removePanel: (panelId) => {
    if (!get().byPanel.has(panelId)) return;
    const next = new Map(get().byPanel);
    next.delete(panelId);
    set({ byPanel: next });
  },

  invalidateCachedData: (dbSessionId, tableName) => {
    let changed = false;
    const next = new Map(get().byPanel);
    for (const [panelId, ts] of next) {
      if (ts.context?.dbSessionId !== dbSessionId) continue;
      if (tableName && ts.context?.table !== tableName) continue;
      if (ts.pendingChanges.size > 0) continue;
      if (ts.columns.length === 0 && ts.rows.length === 0) continue;
      next.set(panelId, {
        ...ts,
        columns: [],
        rows: [],
        totalRows: 0,
        selectedRows: new Set(),
        editBuffer: new Map(),
        editingCell: null,
        // Bump the revision so an in-flight (or late) response can never commit.
        requestRevision: ts.requestRevision + 1,
        loadingRevision: null,
      });
      changed = true;
    }
    if (changed) set({ byPanel: next });
  },

  reset: () => set({ byPanel: new Map() }),

  setPage: (panelId, page) => {
    patchPanelForReload(get, set, panelId, () => ({ page }));
    get().reloadPanel(panelId, { skipCount: true });
  },

  setPageSize: (panelId, size) => {
    patchPanelForReload(get, set, panelId, () => ({ pageSize: size, page: 0 }));
    get().reloadPanel(panelId, { skipCount: true });
  },

  addFilter: (panelId, filter) => {
    patchPanel(get, set, panelId, (ts) => ({
      draftFilters: [...ts.draftFilters, filter],
      filterPanelOpen: true,
    }));
  },

  setFilters: (panelId, filters, logic = 'and') => {
    const next = cloneFilters(filters);
    patchPanelForReload(get, set, panelId, () => ({
      filters: next,
      draftFilters: cloneFilters(next),
      filterLogic: logic,
      draftFilterLogic: logic,
      page: 0,
      filterPanelOpen: true,
    }));
    get().reloadPanel(panelId);
  },

  updateFilter: (panelId, index, filter) => {
    patchPanel(get, set, panelId, (ts) => ({
      draftFilters: ts.draftFilters.map((f, i) => (i === index ? filter : f)),
    }));
  },

  setFilterLogic: (panelId, logic) => {
    patchPanel(get, set, panelId, () => ({ draftFilterLogic: logic }));
  },

  removeFilter: (panelId, index) => {
    patchPanel(get, set, panelId, (ts) => ({
      draftFilters: ts.draftFilters.filter((_, i) => i !== index),
    }));
  },

  clearFilters: (panelId) => {
    patchPanelForReload(get, set, panelId, () => ({
      filters: [],
      draftFilters: [],
      filterLogic: 'and',
      draftFilterLogic: 'and',
      page: 0,
    }));
    get().reloadPanel(panelId);
  },

  applyFilters: (panelId) => {
    const ts = get().byPanel.get(panelId);
    if (!ts) return;
    if (
      filterDraftEqualsApplied(ts.draftFilters, ts.draftFilterLogic, ts.filters, ts.filterLogic)
    ) {
      return;
    }
    patchPanelForReload(get, set, panelId, (cur) => ({
      filters: cloneFilters(cur.draftFilters),
      filterLogic: cur.draftFilterLogic,
      page: 0,
    }));
    get().reloadPanel(panelId);
  },

  setFilterPanelOpen: (panelId, open) => {
    patchPanel(get, set, panelId, () => ({ filterPanelOpen: open }));
  },

  setVisibleColumns: (panelId, columns) => {
    patchPanel(get, set, panelId, () => ({ visibleColumns: columns }));
  },

  setSort: (panelId, sort) => {
    patchPanelForReload(get, set, panelId, () => ({ sorts: [sort], page: 0 }));
    get().reloadPanel(panelId, { skipCount: true });
  },

  setDetailRow: (panelId, index) => {
    patchPanel(get, set, panelId, () => ({ detailRowIndex: index }));
  },

  startEdit: (panelId, row, col) => {
    patchPanel(get, set, panelId, () => ({ editingCell: { row, col } }));
  },

  cancelEdit: (panelId) => {
    patchPanel(get, set, panelId, () => ({ editingCell: null }));
  },

  selectRow: (panelId, index, opts) => {
    patchPanel(get, set, panelId, (ts) => {
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
      // Plain click: select the row, or — when it already is the only selected
      // row — clear the selection, so a second click undoes the first.
      if (!opts?.range && ts.selectedRows.size === 1 && ts.selectedRows.has(index)) {
        return { selectedRows: new Set(), lastSelectedIndex: null };
      }
      return { selectedRows: new Set([index]), lastSelectedIndex: index };
    });
  },

  toggleSelectAll: (panelId) => {
    patchPanel(get, set, panelId, (ts) => {
      const allSelected = ts.selectedRows.size === ts.rows.length && ts.rows.length > 0;
      if (allSelected) return { selectedRows: new Set(), lastSelectedIndex: null };
      const next = new Set<number>();
      for (let i = 0; i < ts.rows.length; i += 1) next.add(i);
      return { selectedRows: next, lastSelectedIndex: null };
    });
  },

  stageCellChange: (panelId, row, col, value) => {
    const ts = get().byPanel.get(panelId);
    if (!ts) return;
    const rowObj = ts.rows[row];
    if (!rowObj) return;
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      patchPanel(get, set, panelId, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }

    const rowIdentity = buildRowIdentity(rowObj, pkCols);
    if (!rowIdentity || !rowIdentityIsUnique(ts, row, rowIdentity, pkCols)) {
      patchPanel(get, set, panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
      return;
    }
    const match = findPendingForRow(ts, row, rowObj, pkCols);
    const identity = match?.change.rowIdentity ?? rowIdentity;
    if (!identity) {
      patchPanel(get, set, panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
      return;
    }
    const existing = match?.change;
    const hasOriginalValue =
      existing !== undefined && Object.prototype.hasOwnProperty.call(existing.originalValues, col);
    const originalValue = hasOriginalValue
      ? existing.originalValues[col]
      : toCellValue(rowObj[col]);
    const currentValue = toCellValue(value);
    const prospectiveRow = { ...rowObj, [col]: currentValue };
    const prospectiveIdentity = buildRowIdentity(prospectiveRow, pkCols);
    const key = match?.key ?? rowIdentityKey(identity);
    if (
      !prospectiveIdentity ||
      !rowIdentityIsUnique(
        {
          ...ts,
          rows: ts.rows.map((candidate, index) => (index === row ? prospectiveRow : candidate)),
        },
        row,
        prospectiveIdentity,
        pkCols,
      ) ||
      hasPendingIdentityCollision(ts, key, prospectiveIdentity, pkCols)
    ) {
      patchPanel(get, set, panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
      return;
    }
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
    const rowIdentityAnchors = new Map(ts.rowIdentityAnchors);
    rowIdentityAnchors.set(row, key);

    patchPanel(get, set, panelId, () => ({
      rows: nextRows,
      pendingChanges,
      rowIdentityAnchors,
      previewPlan: null,
      pendingStatus: 'idle',
      editBuffer: rebuildEditBuffer({ ...ts, rows: nextRows, pendingChanges }),
      editingCell: null,
      error: null,
    }));
  },

  applyColumnToRows: (panelId, col, value, rows) => {
    for (const row of rows) get().stageCellChange(panelId, row, col, value);
  },

  stageRowDelete: (panelId, rowIndices) => {
    const ts = get().byPanel.get(panelId);
    if (!ts) return;
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      patchPanel(get, set, panelId, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }

    const indices = [
      ...new Set(
        (Array.isArray(rowIndices) ? rowIndices : [rowIndices]).filter(
          (index) => Number.isInteger(index) && index >= 0,
        ),
      ),
    ].sort((left, right) => left - right);
    if (indices.length === 0) return;

    const pendingChanges = new Map(ts.pendingChanges);
    const rowIdentityAnchors = new Map(ts.rowIdentityAnchors);
    for (const rowIndex of indices) {
      const row = ts.rows[rowIndex];
      if (!row) continue;
      const rowIdentity = buildRowIdentity(row, pkCols);
      if (!rowIdentity || !rowIdentityIsUnique(ts, rowIndex, rowIdentity, pkCols)) {
        patchPanel(get, set, panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
        return;
      }
      const match = findPendingForRow(ts, rowIndex, row, pkCols);
      const identity = match?.change.rowIdentity ?? rowIdentity;
      const key = match?.key ?? rowIdentityKey(identity);
      if (hasPendingIdentityCollision(ts, key, rowIdentity, pkCols)) {
        patchPanel(get, set, panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
        return;
      }
      const existing = match?.change;
      pendingChanges.set(key, {
        rowIndex,
        rowIdentity: { ...identity },
        originalValues: { ...(existing?.originalValues ?? {}) },
        currentValues: { ...(existing?.currentValues ?? {}) },
        changedColumns: [...(existing?.changedColumns ?? [])],
        deleteMarked: true,
      });
      rowIdentityAnchors.set(rowIndex, key);
    }

    patchPanel(get, set, panelId, () => ({
      pendingChanges,
      rowIdentityAnchors,
      previewPlan: null,
      pendingStatus: 'idle',
      editBuffer: rebuildEditBuffer({ ...ts, pendingChanges }),
      editingCell: null,
      error: null,
    }));
  },

  deleteRows: (panelId, rowIndices) => {
    const ts = get().byPanel.get(panelId);
    if (!ts) return;
    const unique = [...new Set(rowIndices.filter((i) => Number.isInteger(i) && i >= 0))];
    if (unique.length === 0) return;
    patchPanel(get, set, panelId, () => ({
      selectedRows: new Set(unique),
      lastSelectedIndex: unique[unique.length - 1] ?? null,
    }));
    get().stageRowDelete(panelId, unique);
  },

  rollbackPendingChanges: (panelId) => {
    const ts = get().byPanel.get(panelId);
    if (!ts) return;
    patchPanel(get, set, panelId, () => ({
      pendingChanges: new Map(),
      previewPlan: null,
      pendingStatus: 'idle',
      editBuffer: new Map(),
      editingCell: null,
      error: null,
    }));
    get().reloadPanel(panelId);
  },

  previewPendingChanges: async (panelId) => {
    const ts = get().byPanel.get(panelId);
    if (!ts) return null;
    if (ts.pendingChanges.size === 0) return null;
    if (!ts.context || !isCompleteTableChangeContext(ts.context)) {
      patchPanel(get, set, panelId, () => ({
        pendingStatus: 'idle',
        error: 'Table context is incomplete; pending changes cannot be committed.',
      }));
      return null;
    }

    const signature = pendingChangesSignature(ts.pendingChanges);
    const context = ts.context;
    patchPanel(get, set, panelId, () => ({ pendingStatus: 'previewing', error: null }));
    try {
      const plan = await databaseCommands.previewPendingChanges({
        context,
        changes: pendingChangesForWire(ts.pendingChanges),
      });
      const latest = get().byPanel.get(panelId);
      if (latest && pendingChangesSignature(latest.pendingChanges) === signature) {
        patchPanel(get, set, panelId, () => ({
          previewPlan: plan,
          pendingStatus: 'idle',
          error: null,
        }));
      }
      return plan;
    } catch (e) {
      patchPanel(get, set, panelId, () => ({
        pendingStatus: 'idle',
        error: extractErrorMessage(e, t('tableData.commitFailed')),
      }));
      return null;
    }
  },

  commitPendingChanges: async (panelId) => {
    const emptyResult: CommitPendingChangesResult = {
      status: 'noop',
      planId: '',
      fingerprint: '',
      statements: [],
      affectedRows: 0,
      refreshed: false,
      refreshRequired: false,
    };
    const initial = get().byPanel.get(panelId);
    if (!initial) return emptyResult;
    if (initial.pendingChanges.size === 0) return emptyResult;
    if (!initial.context || !isCompleteTableChangeContext(initial.context)) {
      return {
        ...emptyResult,
        status: 'failed',
        error: 'Table context is incomplete; pending changes cannot be committed.',
      };
    }
    const context = initial.context;

    const signature = pendingChangesSignature(initial.pendingChanges);
    let plan = initial.previewPlan;
    if (!plan || tableChangeContextKey(plan.table) !== tableChangeContextKey(context)) {
      plan = await get().previewPendingChanges(panelId);
    }
    if (!plan) return emptyResult;
    if (tableChangeContextKey(plan.table) !== tableChangeContextKey(context)) {
      return {
        ...emptyResult,
        status: 'failed',
        error: 'Table context changed; preview must be rebuilt before commit.',
      };
    }

    patchPanel(get, set, panelId, () => ({ pendingStatus: 'committing', error: null }));
    try {
      const response = await databaseCommands.commitPendingChanges({
        dbSessionId: context.dbSessionId,
        plan,
        manualHandle: tableManualHandle(panelId),
        fingerprint: plan.fingerprint,
      });
      const latest = get().byPanel.get(panelId);
      if (latest && pendingChangesSignature(latest.pendingChanges) === signature) {
        patchPanel(get, set, panelId, () => ({
          pendingChanges: new Map(),
          previewPlan: null,
          pendingStatus: 'idle',
          editBuffer: new Map(),
          editingCell: null,
          error: null,
        }));
      }

      await get().loadTableData({ panelId, ...targetOf(context) });
      const refreshed = get().byPanel.get(panelId);
      const result: CommitPendingChangesResult = {
        ...response,
        status: 'committed',
        refreshed: !refreshed?.error,
        refreshRequired: true,
        refreshStatus: refreshed?.error ? 'failed' : 'completed',
      };
      patchPanel(get, set, panelId, () => ({ pendingStatus: 'idle' }));
      return result;
    } catch (e) {
      let error = extractErrorMessage(e, t('tableData.commitFailed'));
      try {
        if (tableManualHandle(panelId)) error += ' Some changes may remain in the current transaction. Inspect or roll back before retrying.';
      } catch { /* Preserve the original failure when the owner was lost. */ }
      patchPanel(get, set, panelId, () => ({ pendingStatus: 'idle', error }));
      return {
        ...emptyResult,
        status: 'failed',
        planId: plan?.planId ?? '',
        fingerprint: plan?.fingerprint ?? '',
        error,
      };
    }
  },
}));

if (import.meta.env.DEV) {
  (window as unknown as Record<string, unknown>).__tableDataStore = useTableDataStore;
}
