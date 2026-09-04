import { create } from 'zustand';
import { queryCommands } from '../commands/query';
import { t } from '../locales/t';
import type { FavoriteQuery, QueryHistoryEntry, Value } from '../types';
import type { ChartConfig } from '../types/chart';
import { getCancelCapability } from '../lib/queryExecutionViewModel';
import { reduceQueryExecutionState } from '../lib/queryExecutionViewModel';
import { useActiveConnectionStore } from './activeConnectionStore';
import {
  type BindParams,
  type QueryExecState,
  emptyQueryExecState,
  patchExec,
  runStreamingQuery,
  runBoundQuery,
} from './queryExecActions';
import { panelTargetDatabase, panelTargetSchema } from './panelQueryContext';
import {
  resolveNextActive,
  resetPanelIdCounter,
  type Panel,
} from './panelTypes';

export type { QueryExecState, BindParams };
export { EMPTY_QUERY_EXEC, emptyQueryExecState } from './queryExecActions';
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
  Panel,
  ConnectionContext,
} from './panelTypes';
export { nextPanelId } from './panelTypes';

// ── Helper: cancel running queries and clean up queryExec entries ──

function cancelAndCleanupExec(
  panelsToRemove: Panel[],
  currentExec: Map<string, QueryExecState>,
): Map<string, QueryExecState> {
  const nextExec = new Map(currentExec);
  for (const panel of panelsToRemove) {
    if (panel.type === 'query') {
      const exec = nextExec.get(panel.id);
      const capabilities =
        useActiveConnectionStore.getState().connections[panel.connectionId]?.capabilities;
      if (exec?.running && exec.executionId && getCancelCapability(capabilities) === 'supported') {
        queryCommands.cancelQuery(panel.dbSessionId, exec.executionId).catch(() => {});
      }
      nextExec.delete(panel.id);
    }
  }
  return nextExec;
}

// ── Store interface ──────────────────────────────────────────────

interface PanelState {
  panels: Panel[];
  activePanelId: string | null;
  queryExec: Map<string, QueryExecState>;
  queryHistory: QueryHistoryEntry[];
  queryFavorites: FavoriteQuery[];
  historyVisible: boolean;
  favoritesVisible: boolean;
  /** Connection id waiting for query-history to open once ContentView mounts. */
  pendingQueryHistoryConnectionId: string | null;
}

interface PanelActions {
  addPanel: (panel: Panel, activate?: boolean) => void;
  removePanel: (panelId: string) => void;
  removeAllForConnection: (connectionId: string) => void;
  setActivePanel: (panelId: string) => void;
  updatePanel: (panelId: string, patch: Partial<Panel>) => void;
  closeOtherPanels: (panelId: string) => void;
  closeAllPanels: () => void;
  closePanelsToTheRight: (panelId: string) => void;
  closePanelsToTheLeft: (panelId: string) => void;

  updateSql: (panelId: string, sql: string) => void;
  executeQuery: (panelId: string, params?: BindParams) => Promise<void>;
  executeSelection: (panelId: string, sql: string, params?: BindParams) => Promise<void>;
  cancelQuery: (panelId: string) => Promise<void>;
  setActiveResult: (panelId: string, idx: number) => void;
  setResultDetailRow: (panelId: string, index: number | null) => void;
  updateResultCell: (
    panelId: string,
    resultIdx: number,
    row: number,
    col: string,
    value: unknown,
  ) => void;
  setChartConfig: (panelId: string, config: ChartConfig) => void;
  setResultViewMode: (panelId: string, mode: 'table' | 'chart') => void;

  loadHistory: (connectionId?: string) => Promise<void>;
  openQueryHistory: (connectionId?: string) => Promise<void>;
  setPendingQueryHistory: (connectionId: string | null) => void;
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
  queryHistory: [],
  queryFavorites: [],
  historyVisible: false,
  favoritesVisible: false,
  pendingQueryHistoryConnectionId: null,

  // ── Panel CRUD ──────────────────────────────────────────────

  addPanel: (panel, activate = true) => {
    const needsExec = panel.type === 'query';
    const nextExec = needsExec
      ? new Map(get().queryExec).set(panel.id, emptyQueryExecState())
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

  setActivePanel: (panelId) => {
    set({ activePanelId: panelId });
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

  // ── Query execution ────────────────────────────────────────────

  updateSql: (panelId, sql) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, panelId, { sql }) }));
  },

  executeQuery: async (panelId, params) => {
    const { panels, queryExec } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query') return;
    const exec = queryExec.get(panelId);
    if (!exec) return;
    const sql = exec.sql.trim();
    if (!sql) return;

    const getExec = () => get().queryExec;
    const setExec = (next: Map<string, QueryExecState>) => set({ queryExec: next });

    if (params && Object.keys(params).length > 0) {
      await runBoundQuery(
        panelId,
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
        panelId,
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

  executeSelection: async (panelId, sql, params) => {
    const { panels } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query') return;

    const getExec = () => get().queryExec;
    const setExec = (next: Map<string, QueryExecState>) => set({ queryExec: next });

    if (params && Object.keys(params).length > 0) {
      await runBoundQuery(
        panelId,
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
        panelId,
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

  cancelQuery: async (panelId) => {
    const { panels, queryExec } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel) return;

    const exec = queryExec.get(panelId);
    if (!exec?.running || exec.cancelState === 'requested' || !exec.executionId) return;

    const capabilities =
      useActiveConnectionStore.getState().connections[panel.connectionId]?.capabilities;
    if (getCancelCapability(capabilities) !== 'supported') return;

    set((s) => {
      const current = s.queryExec.get(panelId);
      if (!current) return s;
      return {
        queryExec: patchExec(
          s.queryExec,
          panelId,
          reduceQueryExecutionState(current, { type: 'cancel_requested' }),
        ),
      };
    });

    try {
      await queryCommands.cancelQuery(panel.dbSessionId, exec.executionId);
    } catch {
      set((s) => {
        const current = s.queryExec.get(panelId);
        if (!current) return s;
        return {
          queryExec: patchExec(
            s.queryExec,
            panelId,
            reduceQueryExecutionState(current, {
              type: 'cancel_failed',
              error: t('query.cancelFailed'),
            }),
          ),
        };
      });
    }
  },

  setActiveResult: (panelId, idx) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, panelId, { activeResultIdx: idx }) }));
  },

  setResultDetailRow: (panelId, index) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, panelId, { resultDetailRowIndex: index }) }));
  },

  updateResultCell: (panelId, resultIdx, row, col, value) => {
    const exec = get().queryExec.get(panelId);
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
    set((s) => ({ queryExec: patchExec(s.queryExec, panelId, { results }) }));
  },

  setChartConfig: (panelId, config) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, panelId, { chartConfig: config }) }));
  },

  setResultViewMode: (panelId, mode) => {
    set((s) => ({ queryExec: patchExec(s.queryExec, panelId, { resultViewMode: mode }) }));
  },

  // ── History / Favorites ────────────────────────────────────────

  loadHistory: async (connectionId) => {
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
      queryHistory: [],
      queryFavorites: [],
      historyVisible: false,
      favoritesVisible: false,
      pendingQueryHistoryConnectionId: null,
    });
  },
}));
