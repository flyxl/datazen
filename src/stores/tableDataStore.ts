import { create } from 'zustand';
import { tableManualHandle } from '../lib/session/tableSession';
import { databaseCommands } from '../commands/database';
import { t } from '../locales/t';
import type { CountStrategy } from '@datazen/backend-client';
import type { ColumnSchema, DatabaseType, FilterCondition, SortCondition } from '../types';
import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  buildRowIdentity,
  duplicateRowIdentityKeys,
  isCompleteTableChangeContext,
  rowIdentityKey,
  tableChangeContextKey,
  type CommitPendingChangesResult,
  type RowChangePlan,
  type TableChangeContext,
} from '../lib/tableChanges';
import { useSettingsStore } from './settingsStore';
import { resolveDefaultTableSort } from '../lib/defaultSort';
import { resolvePanelCountStrategy } from './tableData/countStrategy';
import { DEFAULT_TABLE_SORT, type DefaultTableSort } from '../types/settings';
import type { TableState } from './tableData/types';
import { cloneFilters, filterDraftEqualsApplied, isCompleteFilter } from './tableData/filterUtils';
import {
  buildTableContext,
  emptyTableState,
  extractErrorMessage,
  rowsToRecords,
  type TableTarget,
} from './tableData/connectionState';
import {
  createSelectionSlice,
  panelColumnOrder,
  rowIndexSelectionReset,
  type SelectionStoreSlice,
} from './tableData/selectionActions';
import {
  createPendingEditSlice,
  type PendingEditStoreSlice,
} from './tableData/pendingEditActions';
import {
  AMBIGUOUS_ROW_IDENTITY_ERROR,
  draftsForWire,
  hasPendingWork,
  overlayPendingRows,
  pendingChangesForWire,
  pendingWorkSignature,
} from './tableData/pendingChanges';
import {
  createDraftSlice,
  type DraftStoreSlice,
} from './tableData/draftInserts';
import {
  createPanelSeedSlice,
  type PanelSeedSlice,
} from './tableData/panelSeed';
import { locateInsertedRow } from './tableData/insertLocation';

// ── Store ─────────────────────────────────────────────────────────

export interface LoadTableDataParams extends TableTarget {
  /** Table/view panel this load belongs to; its state slice is keyed by this id. */
  panelId: string;
  /** Reuse the cached total instead of running COUNT(*) (pagination/sort re-reads). */
  skipCount?: boolean;
  /**
   * The row-count strategy to ask the host for. Omitted means "the host
   * decides" — which is what the driver declares.
   */
  countStrategy?: CountStrategy;
}

/** Every action is scoped to one table/view panel, like `panelStore`'s queryExec map. */
interface TableDataStore
  extends SelectionStoreSlice,
    DraftStoreSlice,
    PendingEditStoreSlice,
    PanelSeedSlice {
  byPanel: Map<string, TableState>;

  loadTableData: (params: LoadTableDataParams) => Promise<void>;
  /** Re-run the load a panel last performed, keeping its page/filters/sorts. */
  reloadPanel: (panelId: string, opts?: { skipCount?: boolean; countStrategy?: CountStrategy }) => void;
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
  /** Change the panel's temporary default-order override. */
  setDefaultOrderOverride: (
    panelId: string,
    override: TableState['defaultOrderOverride'],
  ) => void;
  setDetailRow: (panelId: string, index: number | null) => void;

  startEdit: (panelId: string, row: number, col: string) => void;
  cancelEdit: (panelId: string) => void;
  /**
   * Clear the "new row" highlight. It expires on its own (the row is
   * highlighted for a moment after the commit) and it is cleared when the
   * page, the filters or the sort change (04 §7 #17).
   */
  clearHighlightedNewRow: (panelId: string) => void;
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
    ...rowIndexSelectionReset(),
    // Paging, filtering or sorting moves the new row out of sight: the
    // highlight would point at a row that is no longer on the page (04 §7 #17).
    highlightedNewRowKey: null,
    requestRevision: ts.requestRevision + 1,
  }));
}

/**
 * The order a request asks for.
 *
 * The request's `defaultOrderColumns` carries column names but no direction,
 * so "the primary key, descending" is an **explicit sort** rather than a
 * default order. An explicit sort the user made always wins; a composite
 * primary key cannot be expressed by the one-column sort channel at all, so
 * the descending override is left to the interface, which explains why it is
 * greyed out there.
 *
 * The panel's temporary override beats the global setting; the global setting
 * beats the driver's own default, which is what every read asked for before
 * the setting existed.
 */
function orderRequest(
  override: TableState['defaultOrderOverride'],
  columns: ColumnSchema[],
  sorts: SortCondition[],
  setting: DefaultTableSort,
): { defaultOrderColumns?: string[]; sorts: SortCondition[] } {
  const order = resolveDefaultTableSort(override, setting);
  if (order === 'none') {
    return { defaultOrderColumns: [], sorts };
  }
  if (order === 'primaryKeyDesc' && sorts.length === 0) {
    const keyColumns = columns.filter((column) => column.isPrimaryKey);
    if (keyColumns.length === 1) {
      return { sorts: [{ column: keyColumns[0].name, descending: true }] };
    }
  }
  return { sorts };
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
    // A response that carries no kind says nothing about the count — the
    // panel keeps the value it had. Only a kind that is actually reported
    // replaces it.
    totalRowsKind: res.totalRowsKind ?? current.totalRowsKind,
    page: res.page,
    pageSize: res.pageSize,
    context,
    rowIdentityAnchors: anchors,
    loading: false,
    loadingRevision: null,
    ...rowIndexSelectionReset(),
    editingCell: null,
    error: duplicateIdentityKeys.length > 0 ? AMBIGUOUS_ROW_IDENTITY_ERROR : null,
  });
  set({ byPanel: next });
}

export const useTableDataStore = create<TableDataStore>((set, get) => ({
  byPanel: new Map(),

  loadTableData: async ({ panelId, skipCount, countStrategy, ...target }) => {
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
      const order = orderRequest(
        existing.defaultOrderOverride,
        existing.columns,
        sorts,
        useSettingsStore.getState().settings.defaultTableSort ?? DEFAULT_TABLE_SORT,
      );
      const res = await databaseCommands.getTableData({
        context,
        manualHandle: tableManualHandle(panelId),
        dbSessionId: context.dbSessionId,
        table: context.table,
        page,
        pageSize,
        // The applied filters go on the wire as they are. Stripping the
        // incomplete ones here would make the query answer a question the
        // user did not ask — the result set would be *wider* than the filter
        // list on screen. The host refuses an incomplete condition with
        // `grid.filter.incomplete`, which is a visible, actionable error
        // instead of a silent widening.
        filters,
        // The default order override ("primary key, descending") is an
        // **explicit sort** — `defaultOrderColumns` carries no direction — so
        // the request must carry `order.sorts`, not the raw list.
        sorts: order.sorts,
        skipCount,
        filterLogic,
        // The caller's strategy wins; without one the shared resolution
        // decides — the same answer the panel's first read got, so the number
        // cannot change shape on a refresh.
        countStrategy: countStrategy ?? resolvePanelCountStrategy(context),
        defaultOrderColumns: order.defaultOrderColumns,
        database: context.database,
        // The table's own schema, not a guess: without it the host falls back
        // to the connection default and a table outside it reads as missing.
        schema: context.schema ?? null,
      });
      commitFetchedPage(get, set, panelId, context, requestRevision, res);
    } catch (e) {
      // Fourth convergence point (00-contracts §4 needs the sync): a *failed* fetch must
      // drop row-indexed selection exactly like a successful one. `patchPanelForReload`
      // already covers the six transition actions, but a bare `reloadPanel` / toolbar
      // refresh / post-commit reload has no such transition, so without this the old rows
      // stay highlighted under an error banner for a page the user can no longer see.
      patchPanel(get, set, panelId, (ts) =>
        ts.requestRevision === requestRevision && ts.loadingRevision === requestRevision
          ? {
              loading: false,
              loadingRevision: null,
              error: extractErrorMessage(e, t('tableData.loadFailed')),
              ...rowIndexSelectionReset(),
            }
          : {},
      );
    }
  },

  reloadPanel: (panelId, opts) => {
    const context = get().byPanel.get(panelId)?.context;
    if (!context) return;
    void get().loadTableData({
      panelId,
      ...targetOf(context),
      skipCount: opts?.skipCount,
      countStrategy: opts?.countStrategy,
    });
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
      // Drafts are staged work too: a panel with only drafts must not be
      // invalidated behind the user's back (04 §3.7 R-4).
      if (hasPendingWork(ts)) continue;
      if (ts.columns.length === 0 && ts.rows.length === 0) continue;
      next.set(panelId, {
        ...ts,
        columns: [],
        rows: [],
        totalRows: 0,
        ...rowIndexSelectionReset(),
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
    // An incomplete condition is refused, not skipped. Applying it would send
    // a condition the host rejects (`grid.filter.incomplete`), and dropping it
    // on the way would quietly widen the result set beyond what the user
    // asked for. The editor marks the condition and blocks the button; this
    // is the second gate for every other caller.
    if (ts.draftFilters.some((filter) => !isCompleteFilter(filter))) {
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
    const ts = get().byPanel.get(panelId);
    // Hiding a column must converge the selection now, or hidden cells stay "selected".
    if (ts) get().reconcileGridSelectionForColumns(panelId, panelColumnOrder(ts));
  },

  setSort: (panelId, sort) => {
    patchPanelForReload(get, set, panelId, () => ({ sorts: [sort], page: 0 }));
    get().reloadPanel(panelId, { skipCount: true });
  },

  setDefaultOrderOverride: (panelId, override) => {
    patchPanelForReload(get, set, panelId, () => ({
      defaultOrderOverride: override,
      // The override replaces the default order, so an explicit sort the
      // user made is no longer the only ordering in force.
      sorts: [],
      page: 0,
    }));
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

  // Whole-row + cell selection actions live in `tableData/selectionActions`.
  ...createSelectionSlice((panelId, updater) => patchPanel(get, set, panelId, updater)),

  // The staged-edit actions live in `tableData/pendingEditActions`; the store only wires them.
  ...createPendingEditSlice(
    (panelId, updater) => patchPanel(get, set, panelId, updater),
    (panelId) => get().byPanel.get(panelId),
    (panelId) => get().reloadPanel(panelId),
  ),

  // Draft actions live in `tableData/draftInserts`; the store only wires them.
  ...createDraftSlice(
    (panelId, updater) => patchPanel(get, set, panelId, updater),
    (panelId) => get().byPanel.get(panelId),
  ),

  // The first-frame seed of a panel (a foreign-key jump) lives in
  // `tableData/panelSeed`; the store only wires it. It writes the whole slice
  // because the panel may not have one yet — that is the whole point of it.
  ...createPanelSeedSlice((panelId, state) => {
    const next = new Map(get().byPanel);
    next.set(panelId, state);
    set({ byPanel: next });
  }),

  clearHighlightedNewRow: (panelId) => {
    patchPanel(get, set, panelId, (ts) =>
      ts.highlightedNewRowKey === null ? {} : { highlightedNewRowKey: null },
    );
  },

  previewPendingChanges: async (panelId) => {
    const ts = get().byPanel.get(panelId);
    if (!ts) return null;
    if (!hasPendingWork(ts)) return null;
    if (!ts.context || !isCompleteTableChangeContext(ts.context)) {
      patchPanel(get, set, panelId, () => ({
        pendingStatus: 'idle',
        error: 'Table context is incomplete; pending changes cannot be committed.',
      }));
      return null;
    }

    const signature = pendingWorkSignature(ts);
    const context = ts.context;
    patchPanel(get, set, panelId, () => ({ pendingStatus: 'previewing', error: null }));
    try {
      const plan = await databaseCommands.previewPendingChanges({
        context,
        // The drafts ride along in the same `changes` array as the staged
        // edits: the request shape does not change (04 §3.6).
        changes: [...pendingChangesForWire(ts.pendingChanges), ...draftsForWire(ts)],
      });
      const latest = get().byPanel.get(panelId);
      if (latest && pendingWorkSignature(latest) === signature) {
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
    if (!hasPendingWork(initial)) return emptyResult;
    if (!initial.context || !isCompleteTableChangeContext(initial.context)) {
      return {
        ...emptyResult,
        status: 'failed',
        error: 'Table context is incomplete; pending changes cannot be committed.',
      };
    }
    const context = initial.context;

    const signature = pendingWorkSignature(initial);
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
      // The rows as they are *before* the commit: the "where did the new row
      // end up" rule compares the page after the refresh with this one.
      const previousRows = get().byPanel.get(panelId)?.rows ?? [];
      const response = await databaseCommands.commitPendingChanges({
        dbSessionId: context.dbSessionId,
        plan,
        manualHandle: tableManualHandle(panelId),
        fingerprint: plan.fingerprint,
      });
      const latest = get().byPanel.get(panelId);
      if (latest && pendingWorkSignature(latest) === signature) {
        patchPanel(get, set, panelId, () => ({
          pendingChanges: new Map(),
          draftInserts: new Map(),
          draftInsertOrder: [],
          previewPlan: null,
          pendingStatus: 'idle',
          editingCell: null,
          highlightedNewRowKey: null,
          error: null,
        }));
      }

      await get().loadTableData({ panelId, ...targetOf(context) });
      const refreshed = get().byPanel.get(panelId);
      const location = locateInsertedRow(
        previousRows,
        refreshed?.rows ?? [],
        refreshed?.columns.filter((column) => column.isPrimaryKey) ?? [],
        response.statements,
      );
      patchPanel(get, set, panelId, () => ({ highlightedNewRowKey: location.key }));
      const result: CommitPendingChangesResult = {
        ...response,
        status: 'committed',
        refreshed: !refreshed?.error,
        refreshRequired: true,
        refreshStatus: refreshed?.error ? 'failed' : 'completed',
        insertLocation: location.status,
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
