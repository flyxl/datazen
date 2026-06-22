import { create } from 'zustand';
import { queryCommands } from '../commands/query';
import { t } from '../locales/t';
import type { FavoriteQuery, QueryHistoryEntry, StatementResult } from '../types';

export interface QueryTab {
  id: string;
  title: string;
  sql: string;
  results: StatementResult[];
  activeResultIdx: number;
  error: string | null;
  running: boolean;
  executionTimeMs: number | null;
}

function extractError(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return t('query.executeFailed');
}

function newTab(): QueryTab {
  const id = `tab-${Math.random().toString(36).slice(2, 9)}`;
  return {
    id,
    title: t('query.tab', { n: '1' }),
    sql: '',
    results: [],
    activeResultIdx: 0,
    error: null,
    running: false,
    executionTimeMs: null,
  };
}

interface QueryStore {
  connectionId: string | null;
  tabs: QueryTab[];
  activeTabId: string;
  historyVisible: boolean;
  history: QueryHistoryEntry[];
  resultDetailRowIndex: number | null;
  favorites: FavoriteQuery[];
  favoritesVisible: boolean;

  setConnectionId: (id: string | null) => void;
  createTab: () => void;
  closeTab: (id: string) => void;
  setActiveTab: (id: string) => void;
  updateSql: (tabId: string, sql: string) => void;
  setActiveResult: (tabId: string, idx: number) => void;
  executeQuery: (tabId: string) => Promise<void>;
  executeSelection: (tabId: string, sql: string) => Promise<void>;
  cancelQuery: (tabId: string) => Promise<void>;
  loadHistory: () => Promise<void>;
  toggleHistory: () => void;
  loadFavorites: () => Promise<void>;
  addFavorite: (title: string, sql: string) => Promise<void>;
  deleteFavorite: (id: string) => Promise<void>;
  toggleFavorites: () => void;
  updateResultCell: (tabId: string, resultIdx: number, row: number, col: string, value: unknown) => void;
  reset: () => void;
  setResultDetailRow: (index: number | null) => void;
}

let tabCounter = 0;

export const useQueryStore = create<QueryStore>((set, get) => ({
  connectionId: null,
  tabs: [],
  activeTabId: '',
  historyVisible: false,
  history: [],
  resultDetailRowIndex: null,
  favorites: [],
  favoritesVisible: false,

  setConnectionId: (id) => set({ connectionId: id }),

  createTab: () => {
    tabCounter += 1;
    const tab = newTab();
    tab.title = t('query.tab', { n: String(tabCounter) });
    set((s) => ({
      tabs: [...s.tabs, tab],
      activeTabId: tab.id,
    }));
  },

  closeTab: (id) =>
    set((s) => {
      if (s.tabs.length <= 1) return s;
      const tabs = s.tabs.filter((t) => t.id !== id);
      const activeTabId = s.activeTabId === id ? tabs[0]?.id ?? s.activeTabId : s.activeTabId;
      return { tabs, activeTabId };
    }),

  setActiveTab: (id) => set({ activeTabId: id }),

  updateSql: (tabId, sql) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, sql } : t)),
    })),

  setActiveResult: (tabId, idx) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, activeResultIdx: idx } : t)),
    })),

  executeQuery: async (tabId) => {
    const { connectionId, tabs } = get();
    if (!connectionId) {
      set({
        tabs: tabs.map((tab) =>
          tab.id === tabId
            ? { ...tab, error: t('query.notConnected'), running: false, results: [], activeResultIdx: 0 }
            : tab,
        ),
      });
      return;
    }
    const tab = tabs.find((t) => t.id === tabId);
    if (!tab) return;

    set({
      tabs: tabs.map((t) => (t.id === tabId ? { ...t, running: true, error: null } : t)),
    });

    try {
      const multi = await queryCommands.executeQuery(connectionId, tab.sql);
      set((s) => ({
        tabs: s.tabs.map((t) =>
          t.id === tabId
            ? {
                ...t,
                running: false,
                results: multi.results,
                activeResultIdx: 0,
                error: null,
                executionTimeMs: multi.totalTimeMs ?? null,
              }
            : t,
        ),
      }));
      await get().loadHistory();
    } catch (e) {
      set((s) => ({
        tabs: s.tabs.map((t) =>
          t.id === tabId
            ? {
                ...t,
                running: false,
                error: extractError(e),
                results: [],
                activeResultIdx: 0,
              }
            : t,
        ),
      }));
    }
  },

  executeSelection: async (tabId, sql) => {
    const { connectionId, tabs } = get();
    if (!connectionId) return;
    set({
      tabs: tabs.map((t) => (t.id === tabId ? { ...t, running: true, error: null } : t)),
    });
    try {
      const multi = await queryCommands.executeQuery(connectionId, sql);
      set((s) => ({
        tabs: s.tabs.map((t) =>
          t.id === tabId
            ? {
                ...t,
                running: false,
                results: multi.results,
                activeResultIdx: 0,
                error: null,
                executionTimeMs: multi.totalTimeMs ?? null,
              }
            : t,
        ),
      }));
      await get().loadHistory();
    } catch (e) {
      set((s) => ({
        tabs: s.tabs.map((t) =>
          t.id === tabId
            ? {
                ...t,
                running: false,
                error: extractError(e),
                results: [],
                activeResultIdx: 0,
              }
            : t,
        ),
      }));
    }
  },

  cancelQuery: async (tabId) => {
    const { connectionId } = get();
    if (!connectionId) return;
    await queryCommands.cancelQuery(connectionId);
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === tabId ? { ...t, running: false } : t)),
    }));
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

  setResultDetailRow: (index) => set({ resultDetailRowIndex: index }),

  updateResultCell: (tabId, resultIdx, row, col, value) =>
    set((s) => ({
      tabs: s.tabs.map((tab) => {
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
    })),

  reset: () => {
    tabCounter = 0;
    set({
      connectionId: null,
      tabs: [],
      activeTabId: '',
      historyVisible: false,
      history: [],
      favorites: [],
      favoritesVisible: false,
    });
  },
}));
