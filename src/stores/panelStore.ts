import { create } from 'zustand';
import { queryCommands } from '../commands/query';
import type { DatabaseType, FavoriteQuery, QueryHistoryEntry, Value } from '../types';
import type { ChartConfig } from '../types/chart';
import {
  type BindParams,
  type QueryExecState,
  emptyQueryExecState,
  patchExec,
  runStreamingQuery,
  runBoundQuery,
} from './queryExecActions';

export type { QueryExecState, BindParams };
export { EMPTY_QUERY_EXEC, emptyQueryExecState } from './queryExecActions';

// ── Sub-tab types ────────────────────────────────────────────────

export type SubTabId = 'data' | 'structure' | 'indexes' | 'foreignKeys' | 'ddl';

// ── Panel types ──────────────────────────────────────────────────

interface PanelBase {
  id: string;
  /** Persistent connection config ID. */
  configId: string;
  /** Live connection ID (from activeConnectionStore). */
  connectionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

export interface TablePanel extends PanelBase {
  type: 'table';
  tableName: string;
  subTab: SubTabId;
  structureEditing?: boolean;
}

export interface ViewPanel extends PanelBase {
  type: 'view';
  viewName: string;
  subTab: SubTabId;
}

export interface QueryPanel extends PanelBase {
  type: 'query';
  title: string;
}

export interface CreateTablePanel extends PanelBase {
  type: 'create-table';
}

export interface ErDiagramPanel extends PanelBase {
  type: 'er-diagram';
  focusTable?: string;
}

export interface ObjectsPanel extends PanelBase {
  type: 'objects';
}

export interface PrivilegesPanel extends PanelBase {
  type: 'privileges';
}

export interface DatabaseObjectPanel extends PanelBase {
  type: 'db-object';
  objectKind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type';
  objectName: string;
  objectSchema?: string;
}

export interface RedisDbPanel extends PanelBase {
  type: 'redis-db';
  dbName: string;
}

export type Panel =
  | TablePanel
  | ViewPanel
  | QueryPanel
  | CreateTablePanel
  | ErDiagramPanel
  | ObjectsPanel
  | PrivilegesPanel
  | DatabaseObjectPanel
  | RedisDbPanel;

// ── ID generation ────────────────────────────────────────────────

let counter = 0;
export function nextPanelId(prefix: string): string {
  counter += 1;
  return `panel-${prefix}-${counter}`;
}

// ── Connection context (for panel creation helpers) ──────────────

export interface ConnectionContext {
  configId: string;
  connectionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

// ── Helper: cancel running queries and clean up queryExec entries ──

function cancelAndCleanupExec(
  panelsToRemove: Panel[],
  currentExec: Map<string, QueryExecState>,
): Map<string, QueryExecState> {
  const nextExec = new Map(currentExec);
  for (const panel of panelsToRemove) {
    if (panel.type === 'query') {
      const exec = nextExec.get(panel.id);
      if (exec?.running) {
        queryCommands.cancelQuery(panel.connectionId).catch(() => {});
      }
      nextExec.delete(panel.id);
    }
  }
  return nextExec;
}

// ── Store interface ──────────────────────────────────────────────

function resolveNextActive(
  panels: Panel[],
  removedId: string,
  currentActiveId: string | null,
): string | null {
  if (currentActiveId !== removedId) return currentActiveId;
  const idx = panels.findIndex((p) => p.id === removedId);
  if (idx < 0) return null;
  const remaining = panels.filter((p) => p.id !== removedId);
  if (remaining.length === 0) return null;
  return remaining[Math.min(idx, remaining.length - 1)].id;
}

interface PanelState {
  panels: Panel[];
  activePanelId: string | null;
  queryExec: Map<string, QueryExecState>;
  queryHistory: QueryHistoryEntry[];
  queryFavorites: FavoriteQuery[];
  historyVisible: boolean;
  favoritesVisible: boolean;
}

interface PanelActions {
  addPanel: (panel: Panel, activate?: boolean) => void;
  removePanel: (panelId: string) => void;
  removeAllForConnection: (configId: string) => void;
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

  loadHistory: (configId?: string) => Promise<void>;
  toggleHistory: () => void;
  loadFavorites: (configId?: string) => Promise<void>;
  addFavorite: (title: string, sql: string, configId: string) => Promise<void>;
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

  // ── Panel CRUD ──────────────────────────────────────────────

  addPanel: (panel, activate = true) => {
    const nextExec =
      panel.type === 'query'
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

  removeAllForConnection: (configId) => {
    const { panels, activePanelId, queryExec } = get();
    const toRemove = panels.filter((p) => p.configId === configId);
    const remaining = panels.filter((p) => p.configId !== configId);
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
      await runBoundQuery(panelId, panel.connectionId, sql, params, getExec, setExec);
    } else {
      await runStreamingQuery(panelId, panel.connectionId, sql, getExec, setExec);
    }
    await get().loadHistory(panel.configId);
  },

  executeSelection: async (panelId, sql, params) => {
    const { panels } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query') return;

    const getExec = () => get().queryExec;
    const setExec = (next: Map<string, QueryExecState>) => set({ queryExec: next });

    if (params && Object.keys(params).length > 0) {
      await runBoundQuery(panelId, panel.connectionId, sql, params, getExec, setExec);
    } else {
      await runStreamingQuery(panelId, panel.connectionId, sql, getExec, setExec);
    }
    await get().loadHistory(panel.configId);
  },

  cancelQuery: async (panelId) => {
    const { panels, queryExec } = get();
    const panel = panels.find((p) => p.id === panelId);
    if (!panel) return;
    try {
      await queryCommands.cancelQuery(panel.connectionId);
    } catch {
      // best-effort
    }
    const exec = queryExec.get(panelId);
    if (exec) {
      set((s) => ({
        queryExec: patchExec(s.queryExec, panelId, { running: false, error: 'Cancelled' }),
      }));
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

  loadHistory: async (configId) => {
    const queryHistory = await queryCommands.getQueryHistory(100, configId);
    set({ queryHistory });
  },

  toggleHistory: () => set((s) => ({ historyVisible: !s.historyVisible })),

  loadFavorites: async (configId) => {
    const queryFavorites = await queryCommands.getFavoriteQueries(configId);
    set({ queryFavorites });
  },

  addFavorite: async (title, sql, configId) => {
    await queryCommands.addFavoriteQuery(configId, title, sql);
    await get().loadFavorites(configId);
  },

  deleteFavorite: async (id) => {
    await queryCommands.deleteFavoriteQuery(id);
    const activePanel = get().panels.find((p) => p.id === get().activePanelId);
    await get().loadFavorites(activePanel?.configId);
  },

  toggleFavorites: () => set((s) => ({ favoritesVisible: !s.favoritesVisible })),

  // ── Reset ──────────────────────────────────────────────────────

  reset: () => {
    counter = 0;
    set({
      panels: [],
      activePanelId: null,
      queryExec: new Map(),
      queryHistory: [],
      queryFavorites: [],
      historyVisible: false,
      favoritesVisible: false,
    });
  },
}));
