import { create } from 'zustand';
import { queryCommands } from '../commands/query';
import { applyQueryStreamEvent } from '../lib/queryStream';
import { resolvePostQueryViewMode } from '../lib/chart/postQueryView';
import { t } from '../locales/t';
import type { FavoriteQuery, QueryHistoryEntry, QueryStreamEvent, StatementResult } from '../types';
import type { ChartConfig } from '../types/chart';

export interface QueryTab {
  id: string;
  connectionId: string;
  title: string;
  sql: string;
  results: StatementResult[];
  activeResultIdx: number;
  error: string | null;
  running: boolean;
  executionTimeMs: number | null;
  chartConfig?: ChartConfig;
  resultViewMode?: 'table' | 'chart';
  streamRunId?: number;
}

function extractError(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return t('query.executeFailed');
}

function newTab(connectionId: string): QueryTab {
  const id = `tab-${Math.random().toString(36).slice(2, 9)}`;
  return {
    id,
    connectionId,
    title: t('query.tab', { n: '1' }),
    sql: '',
    results: [],
    activeResultIdx: 0,
    error: null,
    running: false,
    executionTimeMs: null,
  };
}

// ── Per-connection state ──────────────────────────────────────────

interface ConnectionQueryState {
  tabs: QueryTab[];
  activeTabId: string;
  resultDetailRowIndex: number | null;
}

function emptyConnectionQueryState(): ConnectionQueryState {
  return { tabs: [], activeTabId: '', resultDetailRowIndex: null };
}

function flattenActive(
  states: Map<string, ConnectionQueryState>,
  activeConnectionId: string | null,
): ConnectionQueryState {
  if (!activeConnectionId) return emptyConnectionQueryState();
  return states.get(activeConnectionId) ?? emptyConnectionQueryState();
}

function patchConnection(
  states: Map<string, ConnectionQueryState>,
  connectionId: string,
  patch: Partial<ConnectionQueryState>,
): Map<string, ConnectionQueryState> {
  const next = new Map(states);
  const current = states.get(connectionId) ?? emptyConnectionQueryState();
  next.set(connectionId, { ...current, ...patch });
  return next;
}

// ── Store ─────────────────────────────────────────────────────────

interface QueryStore extends ConnectionQueryState {
  states: Map<string, ConnectionQueryState>;
  activeConnectionId: string | null;

  historyVisible: boolean;
  history: QueryHistoryEntry[];
  favorites: FavoriteQuery[];
  favoritesVisible: boolean;

  setActiveConnection: (connectionId: string | null) => void;
  removeConnection: (connectionId: string) => void;
  createTab: () => void;
  closeTab: (id: string) => void;
  setActiveTab: (id: string) => void;
  updateSql: (tabId: string, sql: string) => void;
  setActiveResult: (tabId: string, idx: number) => void;
  executeQuery: (
    tabId: string,
    params?: Record<string, string | number | boolean | null>,
  ) => Promise<void>;
  executeSelection: (
    tabId: string,
    sql: string,
    params?: Record<string, string | number | boolean | null>,
  ) => Promise<void>;
  cancelQuery: (tabId: string) => Promise<void>;
  loadHistory: () => Promise<void>;
  toggleHistory: () => void;
  loadFavorites: () => Promise<void>;
  addFavorite: (title: string, sql: string) => Promise<void>;
  deleteFavorite: (id: string) => Promise<void>;
  toggleFavorites: () => void;
  updateResultCell: (
    tabId: string,
    resultIdx: number,
    row: number,
    col: string,
    value: unknown,
  ) => void;
  reset: () => void;
  setResultDetailRow: (index: number | null) => void;
  setChartConfig: (tabId: string, config: ChartConfig) => void;
  setResultViewMode: (tabId: string, mode: 'table' | 'chart') => void;
}

let tabCounter = 0;
let streamRunCounter = 0;

/** Resolve the connectionId for a query tab, falling back to the global active. */
function resolveTabConnectionId(state: QueryStore, tabId: string): string | null {
  const tab = state.tabs.find((t) => t.id === tabId);
  return tab?.connectionId ?? state.activeConnectionId;
}

/**
 * Commit a per-connection patch and re-flatten top-level fields.
 * If `connectionId` is null, falls back to the active connection.
 */
function commitPatch(
  get: () => QueryStore,
  set: (partial: Partial<QueryStore>) => void,
  connectionId: string | null,
  patch: Partial<ConnectionQueryState>,
): void {
  const state = get();
  const cid = connectionId ?? state.activeConnectionId;
  if (!cid) return;
  const states = patchConnection(state.states, cid, patch);
  set({ states, ...flattenActive(states, state.activeConnectionId) });
}

/** Update tabs within a specific connection's state. */
function updateTabs(
  get: () => QueryStore,
  set: (partial: Partial<QueryStore>) => void,
  connectionId: string,
  updater: (tabs: QueryTab[]) => QueryTab[],
): void {
  const state = get();
  const connState = state.states.get(connectionId) ?? emptyConnectionQueryState();
  commitPatch(get, set, connectionId, { tabs: updater(connState.tabs) });
}

async function runStreamingQuery(
  get: () => QueryStore,
  set: (partial: Partial<QueryStore>) => void,
  tabId: string,
  sql: string,
): Promise<void> {
  const connectionId = resolveTabConnectionId(get(), tabId);
  if (!connectionId) {
    updateTabs(get, set, get().activeConnectionId ?? '', (tabs) =>
      tabs.map((tab) =>
        tab.id === tabId
          ? {
              ...tab,
              error: t('query.notConnected'),
              running: false,
              results: [],
              activeResultIdx: 0,
            }
          : tab,
      ),
    );
    return;
  }

  const runId = ++streamRunCounter;
  updateTabs(get, set, connectionId, (tabs) =>
    tabs.map((tab) =>
      tab.id === tabId
        ? {
            ...tab,
            running: true,
            error: null,
            results: [],
            activeResultIdx: 0,
            streamRunId: runId,
            executionTimeMs: null,
          }
        : tab,
    ),
  );

  const onEvent = (event: QueryStreamEvent) => {
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) => {
        if (tab.id !== tabId || tab.streamRunId !== runId) return tab;
        return applyQueryStreamEvent(tab, event);
      }),
    );
  };

  try {
    await queryCommands.executeQueryStream(connectionId, sql, onEvent);
    const connState = get().states.get(connectionId);
    const tab = connState?.tabs.find((item) => item.id === tabId);
    if (tab && tab.streamRunId === runId) {
      const viewMode = resolvePostQueryViewMode(tab.results[0]);
      updateTabs(get, set, connectionId, (tabs) =>
        tabs.map((item) =>
          item.id === tabId && item.streamRunId === runId
            ? { ...item, resultViewMode: viewMode, running: false }
            : item,
        ),
      );
      await get().loadHistory();
    }
  } catch (e) {
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) =>
        tab.id === tabId && tab.streamRunId === runId
          ? { ...tab, running: false, error: extractError(e) }
          : tab,
      ),
    );
  }
}

async function runBoundQuery(
  get: () => QueryStore,
  set: (partial: Partial<QueryStore>) => void,
  tabId: string,
  sql: string,
  params: Record<string, string | number | boolean | null>,
): Promise<void> {
  const connectionId = resolveTabConnectionId(get(), tabId);
  if (!connectionId) {
    updateTabs(get, set, get().activeConnectionId ?? '', (tabs) =>
      tabs.map((tab) =>
        tab.id === tabId
          ? {
              ...tab,
              error: t('query.notConnected'),
              running: false,
              results: [],
              activeResultIdx: 0,
            }
          : tab,
      ),
    );
    return;
  }

  updateTabs(get, set, connectionId, (tabs) =>
    tabs.map((tab) => (tab.id === tabId ? { ...tab, running: true, error: null } : tab)),
  );

  try {
    const multi = await queryCommands.executeQuery(connectionId, sql, params);
    const viewMode = resolvePostQueryViewMode(multi.results[0]);
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) =>
        tab.id === tabId
          ? {
              ...tab,
              running: false,
              results: multi.results,
              activeResultIdx: 0,
              error: null,
              executionTimeMs: multi.totalTimeMs ?? null,
              resultViewMode: viewMode,
            }
          : tab,
      ),
    );
    await get().loadHistory();
  } catch (e) {
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) =>
        tab.id === tabId
          ? {
              ...tab,
              running: false,
              error: extractError(e),
              results: [],
              activeResultIdx: 0,
            }
          : tab,
      ),
    );
  }
}

export const useQueryStore = create<QueryStore>((set, get) => ({
  states: new Map(),
  activeConnectionId: null,
  ...emptyConnectionQueryState(),

  historyVisible: false,
  history: [],
  favorites: [],
  favoritesVisible: false,

  setActiveConnection: (connectionId) => {
    const state = get();
    const states = state.states;
    if (connectionId && !states.has(connectionId)) {
      const next = new Map(states);
      next.set(connectionId, emptyConnectionQueryState());
      set({ states: next, activeConnectionId: connectionId, ...flattenActive(next, connectionId) });
    } else {
      set({ activeConnectionId: connectionId, ...flattenActive(states, connectionId) });
    }
  },

  removeConnection: (connectionId) => {
    const state = get();
    const states = new Map(state.states);
    states.delete(connectionId);
    const activeConnectionId =
      state.activeConnectionId === connectionId ? null : state.activeConnectionId;
    set({ states, activeConnectionId, ...flattenActive(states, activeConnectionId) });
  },

  createTab: () => {
    const { activeConnectionId } = get();
    if (!activeConnectionId) return;
    tabCounter += 1;
    const tab = newTab(activeConnectionId);
    tab.title = t('query.tab', { n: String(tabCounter) });
    const state = get();
    const connState = state.states.get(activeConnectionId) ?? emptyConnectionQueryState();
    commitPatch(get, set, activeConnectionId, {
      tabs: [...connState.tabs, tab],
      activeTabId: tab.id,
    });
  },

  closeTab: (id) => {
    const { activeConnectionId } = get();
    if (!activeConnectionId) return;
    const state = get();
    const connState = state.states.get(activeConnectionId) ?? emptyConnectionQueryState();
    if (connState.tabs.length <= 1) return;
    const tabs = connState.tabs.filter((t) => t.id !== id);
    const activeTabId =
      connState.activeTabId === id ? (tabs[0]?.id ?? connState.activeTabId) : connState.activeTabId;
    commitPatch(get, set, activeConnectionId, { tabs, activeTabId });
  },

  setActiveTab: (id) => {
    commitPatch(get, set, null, { activeTabId: id });
  },

  updateSql: (tabId, sql) => {
    const connectionId = resolveTabConnectionId(get(), tabId) ?? get().activeConnectionId;
    if (!connectionId) return;
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((t) => (t.id === tabId ? { ...t, sql } : t)),
    );
  },

  setActiveResult: (tabId, idx) => {
    const connectionId = resolveTabConnectionId(get(), tabId) ?? get().activeConnectionId;
    if (!connectionId) return;
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((t) => (t.id === tabId ? { ...t, activeResultIdx: idx } : t)),
    );
  },

  executeQuery: async (tabId, params) => {
    const tab = get().tabs.find((t) => t.id === tabId);
    if (!tab) return;
    if (params && Object.keys(params).length > 0) {
      await runBoundQuery(get, set, tabId, tab.sql, params);
      return;
    }
    await runStreamingQuery(get, set, tabId, tab.sql);
  },

  executeSelection: async (tabId, sql, params) => {
    if (params && Object.keys(params).length > 0) {
      await runBoundQuery(get, set, tabId, sql, params);
      return;
    }
    await runStreamingQuery(get, set, tabId, sql);
  },

  cancelQuery: async (tabId) => {
    const connectionId = resolveTabConnectionId(get(), tabId);
    if (!connectionId) return;
    try {
      await queryCommands.cancelQuery(connectionId);
    } catch {
      // best-effort cancellation
    }
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) =>
        tab.id === tabId ? { ...tab, running: false, error: t('query.cancelled') } : tab,
      ),
    );
  },

  loadHistory: async () => {
    const history = await queryCommands.getQueryHistory(100);
    set({ history });
  },

  toggleHistory: () => set((s) => ({ historyVisible: !s.historyVisible })),

  loadFavorites: async () => {
    const favorites = await queryCommands.getFavoriteQueries();
    set({ favorites });
  },

  addFavorite: async (title, sql) => {
    await queryCommands.addFavoriteQuery(title, sql);
    await get().loadFavorites();
  },

  deleteFavorite: async (id) => {
    await queryCommands.deleteFavoriteQuery(id);
    await get().loadFavorites();
  },

  toggleFavorites: () => set((s) => ({ favoritesVisible: !s.favoritesVisible })),

  setResultDetailRow: (index) => {
    commitPatch(get, set, null, { resultDetailRowIndex: index });
  },

  updateResultCell: (tabId, resultIdx, row, col, value) => {
    const connectionId = resolveTabConnectionId(get(), tabId) ?? get().activeConnectionId;
    if (!connectionId) return;
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) => {
        if (tab.id !== tabId) return tab;
        const results = tab.results.map((r, ri) => {
          if (ri !== resultIdx) return r;
          const colIdx = r.columns.findIndex((c) => c.name === col);
          if (colIdx === -1) return r;
          const rows = r.rows.map((rowArr, rowI) => {
            if (rowI !== row) return rowArr;
            const next = [...rowArr];
            next[colIdx] = value as import('../types').Value;
            return next;
          });
          return { ...r, rows };
        });
        return { ...tab, results };
      }),
    );
  },

  reset: () => {
    tabCounter = 0;
    set({
      states: new Map(),
      activeConnectionId: null,
      ...emptyConnectionQueryState(),
      historyVisible: false,
      history: [],
      favorites: [],
      favoritesVisible: false,
    });
  },

  setChartConfig: (tabId, config) => {
    const connectionId = resolveTabConnectionId(get(), tabId) ?? get().activeConnectionId;
    if (!connectionId) return;
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) => (tab.id === tabId ? { ...tab, chartConfig: config } : tab)),
    );
  },

  setResultViewMode: (tabId, mode) => {
    const connectionId = resolveTabConnectionId(get(), tabId) ?? get().activeConnectionId;
    if (!connectionId) return;
    updateTabs(get, set, connectionId, (tabs) =>
      tabs.map((tab) => (tab.id === tabId ? { ...tab, resultViewMode: mode } : tab)),
    );
  },
}));
