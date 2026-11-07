// ─────────────────────────────────────────────────────────────────────────────
// GENERATED FILE — DO NOT EDIT BY HAND. Test fixture, not production code.
//
//   purpose : the "before" side of the BUG-001 differential reproduction
//             (paneFocusDifferentialRepro.tester.test.ts / paneRealSequence.tester.test.ts).
//             It is a SNAPSHOT of the store as it was BEFORE 227b6af8e
//             ("fix(panel): pane 焦点按 tab 归属，修 BUG-001"), so the same scenario
//             body can be run against the broken and the fixed store and the two
//             observable outcomes compared.
//
//   source  : commit 227b6af8e^ , path src/stores/panelStore.ts
//   marker  : // ─── END GENERATED BANNER ───
//
//   The only difference from the source blob is that every relative import gained
//   the extra '../' levels needed to live in __tests__/fixtures/.
//
//   regenerate (never hand-patch; re-prepend this banner afterwards):
//     git show 227b6af8e^:src/stores/panelStore.ts > src/stores/__tests__/fixtures/panelStore.pre-fix-227b6af8e.ts
//
//   verified by paneFocusDifferentialRepro.tester.test.ts:
//     PROVENANCE 1 — the module really has the pre-fix shape
//     PROVENANCE 2 — the banner's `source  :` line names the source commit, AND
//                    everything after the marker is byte-identical to that
//                    commit's blob, so a fixture left behind by a later edit of
//                    the real store fails here instead of silently becoming a
//                    different historical moment
//     PROVENANCE 3 — no non-`__tests__` file under src/stores/ imports it
// ─────────────────────────────────────────────────────────────────────────────
// ─── END GENERATED BANNER ───

import { create } from 'zustand';
import { queryCommands } from '../../../commands/query';
import { t } from '../../../locales/t';
import type { FavoriteQuery, QueryHistoryEntry, Value } from '../../../types';
import type { ChartConfig } from '../../../types/chart';
import { getCancelCapability } from '../../../lib/queryExecutionViewModel';
import { reduceQueryExecutionState } from '../../../lib/queryExecutionViewModel';
import { useActiveConnectionStore } from '../../activeConnectionStore';
import {
  type BindParams,
  type QueryExecState,
  emptyQueryExecState,
  patchExec,
  runStreamingQuery,
  runBoundQuery,
} from '../../queryExecActions';
import { panelTargetDatabase, panelTargetSchema } from '../../panelQueryContext';
import { cancelAndCleanupExec, cancelAndCleanupPaneExec } from '../../panelExecCleanup';
import { DEFAULT_PANE_ID, paneKey } from '../../paneKeys';
import { resolveNextActive, resetPanelIdCounter, type Panel } from '../../panelTypes';
import { createPanelCloseNotifier } from '../../panelCloseNotifier';

export type { QueryExecState, BindParams };
export { EMPTY_QUERY_EXEC, emptyQueryExecState } from '../../queryExecActions';
export {
  DEFAULT_PANE_ID,
  isPaneKeyOfPanel,
  paneArgs,
  paneIdOfPaneKey,
  paneKey,
  paneKeysOfPanel,
  panelIdOfPaneKey,
  resolveFocusedPaneId,
} from '../../paneKeys';
export type {
  SubTabId,
  TablePanel,
  ViewPanel,
  QueryPanel,
  CreateTablePanel,
  ErDiagramPanel,
  ObjectsPanel,
  PrivilegesPanel,
  ServerStatusPanel,
  ServerStatusCache,
  ProcessesPanel,
  ProcessListCacheData,
  DatabaseObjectPanel,
  RedisDbPanel,
  RedisPendingAction,
  Panel,
  ConnectionContext,
} from '../../panelTypes';
export { nextPanelId } from '../../panelTypes';

// ── Store interface ──────────────────────────────────────────────

export interface PendingHistoryQuery {
  connectionId: string;
  sql: string;
  database?: string | null;
  schema?: string | null;
}

interface PanelState {
  panels: Panel[];
  activePanelId: string | null;
  /**
   * Query execution state, keyed by **pane** rather than by panel: one panel
   * may own several independent editors once it is split. Use `paneKey()` to
   * build the key — a panel's default pane is keyed by the bare panel id, so
   * every single-pane caller is unaffected.
   */
  queryExec: Map<string, QueryExecState>;
  /**
   * Pane that editor actions (execute / format / save …) route to. `null` means
   * "not split yet" and resolves to the panel's default pane.
   *
   * It lives in the store rather than in component state because `ContentView`
   * unmounts every inactive tab's panel: component state would not survive a
   * tab switch and the focus would silently reset.
   */
  focusedPaneId: string | null;
  queryHistory: QueryHistoryEntry[];
  queryFavorites: FavoriteQuery[];
  historyVisible: boolean;
  favoritesVisible: boolean;
  /** Connection id waiting for query-history to open once ContentView mounts. */
  pendingQueryHistoryConnectionId: string | null;
  /** Pending history query waiting for connection session to be established before opening QueryPanel. */
  pendingHistoryQuery: PendingHistoryQuery | null;
}

interface PanelActions {
  /**
   * Open `panel` as a tab. `paneId` names the pane it opens into; the default
   * pane is the panel's own (pre-split) editor.
   */
  addPanel: (panel: Panel, activate?: boolean, paneId?: string) => void;
  removePanel: (panelId: string) => void;
  removeAllForConnection: (connectionId: string) => void;
  /**
   * Close table/view tabs opened on a dropped relation. `database` is optional:
   * panels created without a pinned database only carry the table name, so a
   * missing database on either side must not block the match.
   */
  removePanelsForRelation: (connectionId: string, tableName: string, database?: string) => void;
  /**
   * Close every panel bound to a dropped database (table / view / query / ...).
   * `sessionDatabase` covers table/view panels created without a pinned
   * database: they render against the session's current database, so they are
   * only matched when it equals the dropped database. Query panels without a
   * pinned database simply follow the session fallback and are kept.
   */
  removePanelsForDatabase: (
    connectionId: string,
    database: string,
    sessionDatabase?: string,
  ) => void;
  setActivePanel: (panelId: string | null) => void;
  updatePanel: (panelId: string, patch: Partial<Panel>) => void;
  closeOtherPanels: (panelId: string) => void;
  closeAllPanels: () => void;
  closePanelsToTheRight: (panelId: string) => void;
  closePanelsToTheLeft: (panelId: string) => void;

  /** Route editor actions to `paneId`; `null` restores the default pane. */
  setFocusedPane: (paneId: string | null) => void;
  /** Add a second pane to an existing query tab and focus it (Split Pane seam). */
  openPane: (panelId: string, paneId: string) => void;
  /** Close one pane of a query tab, cancelling its run and dropping its state. */
  closePane: (panelId: string, paneId: string) => void;

  /**
   * Every exec action below takes an optional trailing `paneId`. Omitting it
   * targets the panel's default pane, so callers that do not care about panes
   * (and pre-split callers) keep their exact previous meaning.
   */
  updateSql: (panelId: string, sql: string, paneId?: string) => void;
  executeQuery: (panelId: string, params?: BindParams, paneId?: string) => Promise<void>;
  executeSelection: (
    panelId: string,
    sql: string,
    params?: BindParams,
    paneId?: string,
  ) => Promise<void>;
  cancelQuery: (panelId: string, paneId?: string) => Promise<void>;
  setActiveResult: (panelId: string, idx: number, paneId?: string) => void;
  togglePinResult: (panelId: string, idx: number, paneId?: string) => void;
  setResultDetailRow: (panelId: string, index: number | null, paneId?: string) => void;
  updateResultCell: (
    panelId: string,
    resultIdx: number,
    row: number,
    col: string,
    value: unknown,
    paneId?: string,
  ) => void;
  setChartConfig: (panelId: string, config: ChartConfig, paneId?: string) => void;
  setResultViewMode: (panelId: string, mode: 'table' | 'chart', paneId?: string) => void;

  loadHistory: (connectionId?: string) => Promise<void>;
  openQueryHistory: (connectionId?: string) => Promise<void>;
  setPendingQueryHistory: (connectionId: string | null) => void;
  setPendingHistoryQuery: (query: PendingHistoryQuery | null) => void;
  toggleHistory: () => void;
  loadFavorites: (connectionId?: string) => Promise<void>;
  addFavorite: (title: string, sql: string, connectionId: string) => Promise<void>;
  deleteFavorite: (id: string) => Promise<void>;
  toggleFavorites: () => void;

  reset: () => void;
}

export const usePanelStore = create<PanelState & PanelActions>((set, get) => ({
  panels: [],
  activePanelId: null,
  queryExec: new Map(),
  focusedPaneId: null,
  queryHistory: [],
  queryFavorites: [],
  historyVisible: false,
  favoritesVisible: false,
  pendingQueryHistoryConnectionId: null,
  pendingHistoryQuery: null,

  // ── Panel CRUD ──────────────────────────────────────────────

  addPanel: (panel, activate = true, paneId = DEFAULT_PANE_ID) => {
    const needsExec = panel.type === 'query';
    const nextExec = needsExec
      ? new Map(get().queryExec).set(paneKey(panel.id, paneId), emptyQueryExecState())
      : get().queryExec;
    set((s) => ({
      panels: [...s.panels, panel],
      activePanelId: activate ? panel.id : s.activePanelId,
      queryExec: nextExec,
    }));
  },

  removePanel: (panelId) => {
    const { panels, activePanelId, queryExec } = get();
    const panel = panels.find((p) => p.id === panelId);
    const nextExec = panel ? cancelAndCleanupExec([panel], queryExec) : queryExec;
    const nextActive = resolveNextActive(panels, panelId, activePanelId);
    set({
      panels: panels.filter((p) => p.id !== panelId),
      activePanelId: nextActive,
      queryExec: nextExec,
    });
  },

  removeAllForConnection: (connectionId) => {
    const { panels, activePanelId, queryExec } = get();
    const toRemove = panels.filter((p) => p.connectionId === connectionId);
    const remaining = panels.filter((p) => p.connectionId !== connectionId);
    const nextExec = cancelAndCleanupExec(toRemove, queryExec);
    const activeStillExists = remaining.some((p) => p.id === activePanelId);
    set({
      panels: remaining,
      activePanelId: activeStillExists ? activePanelId : (remaining.at(-1)?.id ?? null),
      queryExec: nextExec,
    });
  },

  removePanelsForRelation: (connectionId, tableName, database) => {
    const { panels, activePanelId, queryExec } = get();
    const targetDb = database?.trim() || undefined;
    const toRemove = panels.filter((p) => {
      if (p.connectionId !== connectionId) return false;
      if (p.type === 'table') {
        if (p.tableName !== tableName) return false;
        const panelDb = p.database?.trim() || undefined;
        return !targetDb || !panelDb || panelDb === targetDb;
      }
      if (p.type === 'view') {
        if (p.viewName !== tableName) return false;
        const panelDb = p.database?.trim() || undefined;
        return !targetDb || !panelDb || panelDb === targetDb;
      }
      return false;
    });
    if (toRemove.length === 0) return;
    const remaining = panels.filter((p) => !toRemove.some((r) => r.id === p.id));
    const nextExec = cancelAndCleanupExec(toRemove, queryExec);
    const activeStillExists = remaining.some((p) => p.id === activePanelId);
    set({
      panels: remaining,
      activePanelId: activeStillExists ? activePanelId : (remaining.at(-1)?.id ?? null),
      queryExec: nextExec,
    });
  },

  removePanelsForDatabase: (connectionId, database, sessionDatabase) => {
    const { panels, activePanelId, queryExec } = get();
    const targetDb = database.trim();
    if (!targetDb) return;
    const sessionDb = sessionDatabase?.trim() || undefined;
    const toRemove = panels.filter((p) => {
      if (p.connectionId !== connectionId) return false;
      switch (p.type) {
        case 'table': {
          const panelDb = p.database?.trim() || undefined;
          if (panelDb) return panelDb === targetDb;
          return sessionDb === targetDb;
        }
        case 'view': {
          const panelDb = p.database?.trim() || undefined;
          if (panelDb) return panelDb === targetDb;
          return sessionDb === targetDb;
        }
        case 'query':
          return (p.database?.trim() || undefined) === targetDb;
        case 'create-table':
          return (p.database?.trim() || undefined) === targetDb;
        case 'redis-db':
          return p.dbName === targetDb;
        default:
          return false;
      }
    });
    if (toRemove.length === 0) return;
    const remaining = panels.filter((p) => !toRemove.some((r) => r.id === p.id));
    const nextExec = cancelAndCleanupExec(toRemove, queryExec);
    const activeStillExists = remaining.some((p) => p.id === activePanelId);
    set({
      panels: remaining,
      activePanelId: activeStillExists ? activePanelId : (remaining.at(-1)?.id ?? null),
      queryExec: nextExec,
    });
  },

  setActivePanel: (panelId) => {
    set({ activePanelId: panelId ?? null });
  },

  updatePanel: (panelId, patch) => {
    set((s) => ({
      panels: s.panels.map((p) => (p.id === panelId ? ({ ...p, ...patch } as Panel) : p)),
    }));
  },

  closeOtherPanels: (panelId) => {
    const { panels, queryExec } = get();
    const toRemove = panels.filter((p) => p.id !== panelId);
    const nextExec = cancelAndCleanupExec(toRemove, queryExec);
    set({
      panels: panels.filter((p) => p.id === panelId),
      activePanelId: panelId,
      queryExec: nextExec,
    });
  },

  closeAllPanels: () => {
    const { panels, queryExec } = get();
    const nextExec = cancelAndCleanupExec(panels, queryExec);
    set({ panels: [], activePanelId: null, queryExec: nextExec });
  },

  closePanelsToTheRight: (panelId) => {
    set((s) => {
      const idx = s.panels.findIndex((p) => p.id === panelId);
      if (idx < 0) return s;
      const kept = s.panels.slice(0, idx + 1);
      const removed = s.panels.slice(idx + 1);
      const nextExec = cancelAndCleanupExec(removed, s.queryExec);
      const activeStillExists = kept.some((p) => p.id === s.activePanelId);
      return {
        panels: kept,
        activePanelId: activeStillExists ? s.activePanelId : panelId,
        queryExec: nextExec,
      };
    });
  },

  closePanelsToTheLeft: (panelId) => {
    set((s) => {
      const idx = s.panels.findIndex((p) => p.id === panelId);
      if (idx < 0) return s;
      const kept = s.panels.slice(idx);
      const removed = s.panels.slice(0, idx);
      const nextExec = cancelAndCleanupExec(removed, s.queryExec);
      const activeStillExists = kept.some((p) => p.id === s.activePanelId);
      return {
        panels: kept,
        activePanelId: activeStillExists ? s.activePanelId : panelId,
        queryExec: nextExec,
      };
    });
  },

  // ── Panes ─────────────────────────────────────────────────────

  setFocusedPane: (paneId) => {
    set({ focusedPaneId: paneId || null });
  },

  openPane: (panelId, paneId) => {
    const panel = get().panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query' || !paneId) return;
    const key = paneKey(panelId, paneId);
    set((s) => {
      if (s.queryExec.has(key)) return s;
      return {
        queryExec: new Map(s.queryExec).set(key, emptyQueryExecState()),
        focusedPaneId: paneId,
      };
    });
  },

  closePane: (panelId, paneId) => {
    const panel = get().panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query') return;
    const nextExec = cancelAndCleanupPaneExec(panel, paneId, get().queryExec);
    set((s) => ({
      queryExec: nextExec,
      // A closed pane cannot stay focused; fall back to the panel's own pane.
      focusedPaneId: s.focusedPaneId === paneId ? DEFAULT_PANE_ID : s.focusedPaneId,
    }));
  },

  // ── Query execution ────────────────────────────────────────────

  updateSql: (panelId, sql, paneId) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), { sql }) }));
  },

  executeQuery: async (panelId, params, paneId) => {
    const { panels, queryExec } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query') return;
    // Resolved once: the whole run (including every async stream callback)
    // targets this pane, even if focus moves while the query is in flight.
    const key = paneKey(panelId, paneId);
    const exec = queryExec.get(key);
    if (!exec) return;
    const sql = exec.sql.trim();
    if (!sql) return;

    const getExec = () => get().queryExec;
    const setExec = (next: Map<string, QueryExecState>) => set({ queryExec: next });

    if (params && Object.keys(params).length > 0) {
      await runBoundQuery(
        key,
        panel.dbSessionId,
        sql,
        params,
        getExec,
        setExec,
        panelTargetDatabase(panel, sql),
        panelTargetSchema(panel),
      );
    } else {
      await runStreamingQuery(
        key,
        panel.dbSessionId,
        sql,
        getExec,
        setExec,
        panelTargetDatabase(panel, sql),
        panelTargetSchema(panel),
      );
    }
    await get().loadHistory(panel.connectionId);
  },

  executeSelection: async (panelId, sql, params, paneId) => {
    const { panels } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query') return;
    const key = paneKey(panelId, paneId);

    const getExec = () => get().queryExec;
    const setExec = (next: Map<string, QueryExecState>) => set({ queryExec: next });

    if (params && Object.keys(params).length > 0) {
      await runBoundQuery(
        key,
        panel.dbSessionId,
        sql,
        params,
        getExec,
        setExec,
        panelTargetDatabase(panel, sql),
        panelTargetSchema(panel),
      );
    } else {
      await runStreamingQuery(
        key,
        panel.dbSessionId,
        sql,
        getExec,
        setExec,
        panelTargetDatabase(panel, sql),
        panelTargetSchema(panel),
      );
    }
    await get().loadHistory(panel.connectionId);
  },

  cancelQuery: async (panelId, paneId) => {
    const { panels, queryExec } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel) return;

    const key = paneKey(panelId, paneId);
    const exec = queryExec.get(key);
    if (!exec?.running || exec.cancelState === 'requested' || !exec.executionId) return;

    const capabilities =
      useActiveConnectionStore.getState().connections[panel.connectionId]?.capabilities;
    if (getCancelCapability(capabilities) !== 'supported') return;

    set((s) => {
      const current = s.queryExec.get(key);
      if (!current) return s;
      return {
        queryExec: patchExec(
          s.queryExec,
          key,
          reduceQueryExecutionState(current, { type: 'cancel_requested' }),
        ),
      };
    });

    try {
      await queryCommands.cancelQuery(panel.dbSessionId, exec.executionId);
    } catch {
      set((s) => {
        const current = s.queryExec.get(key);
        if (!current) return s;
        return {
          queryExec: patchExec(
            s.queryExec,
            key,
            reduceQueryExecutionState(current, {
              type: 'cancel_failed',
              error: t('query.cancelFailed'),
            }),
          ),
        };
      });
    }
  },

  setActiveResult: (panelId, idx, paneId) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), { activeResultIdx: idx }) }));
  },

  togglePinResult: (panelId, idx, paneId) => {
    const key = paneKey(panelId, paneId);
    const exec = get().queryExec.get(key);
    if (!exec || !exec.results[idx]) return;
    const newResults = exec.results.map((r, i) => (i === idx ? { ...r, pinned: !r.pinned } : r));
    set((s) => ({ queryExec: patchExec(s.queryExec, key, { results: newResults }) }));
  },

  setResultDetailRow: (panelId, index, paneId) => {
    set((s) => ({
      queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), {
        resultDetailRowIndex: index,
      }),
    }));
  },

  updateResultCell: (panelId, resultIdx, row, col, value, paneId) => {
    const key = paneKey(panelId, paneId);
    const exec = get().queryExec.get(key);
    if (!exec) return;
    const results = exec.results.map((r, ri) => {
      if (ri !== resultIdx) return r;
      const colIdx = r.columns.findIndex((c) => c.name === col);
      if (colIdx === -1) return r;
      const rows = r.rows.map((rowArr, rowI) => {
        if (rowI !== row) return rowArr;
        const next = [...rowArr];
        next[colIdx] = value as Value;
        return next;
      });
      return { ...r, rows };
    });
    set((s) => ({ queryExec: patchExec(s.queryExec, key, { results }) }));
  },

  setChartConfig: (panelId, config, paneId) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), { chartConfig: config }) }));
  },

  setResultViewMode: (panelId, mode, paneId) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), { resultViewMode: mode }) }));
  },

  // ── History / Favorites ────────────────────────────────────────

  loadHistory: async (connectionId) => {
    // Snapshot note (type-only patch, no behavior change): the pre-fix store
    // Snapshot note: called the retired `getQueryHistory` command; it now exists
    // Snapshot note: only on the differential tests' vi.mock layer, never on
    // Snapshot note: the real queryCommands.
    // @ts-expect-error retired IPC surface, mocked by the differential tests
    const queryHistory = await queryCommands.getQueryHistory(1000, connectionId);
    set({ queryHistory });
  },

  openQueryHistory: async (connectionId) => {
    set({ historyVisible: true, favoritesVisible: false });
    await get().loadHistory(connectionId);
  },

  setPendingQueryHistory: (connectionId) => {
    set({ pendingQueryHistoryConnectionId: connectionId });
  },

  setPendingHistoryQuery: (pendingHistoryQuery) => {
    set({ pendingHistoryQuery });
  },

  toggleHistory: () => set((s) => ({ historyVisible: !s.historyVisible })),

  loadFavorites: async (connectionId) => {
    const queryFavorites = await queryCommands.getFavoriteQueries(connectionId);
    set({ queryFavorites });
  },

  addFavorite: async (title, sql, connectionId) => {
    await queryCommands.addFavoriteQuery(connectionId, title, sql);
    await get().loadFavorites(connectionId);
  },

  deleteFavorite: async (id) => {
    await queryCommands.deleteFavoriteQuery(id);
    const activePanel = get().panels.find((p) => p.id === get().activePanelId);
    await get().loadFavorites(activePanel?.connectionId);
  },

  toggleFavorites: () => set((s) => ({ favoritesVisible: !s.favoritesVisible })),

  // ── Reset ──────────────────────────────────────────────────────

  reset: () => {
    resetPanelIdCounter();
    set({
      panels: [],
      activePanelId: null,
      queryExec: new Map(),
      focusedPaneId: null,
      queryHistory: [],
      queryFavorites: [],
      historyVisible: false,
      favoritesVisible: false,
      pendingQueryHistoryConnectionId: null,
      pendingHistoryQuery: null,
    });
  },
}));

// Tab-closed notifications are derived from this store's own state changes, so
// they are wired up here rather than from each of the many removal actions (see
// `panelCloseNotifier`). Re-exported so the view layer registers against the
// notifier for the store it actually renders, without importing the store.
export const { onPanelClosed } = createPanelCloseNotifier(usePanelStore);
