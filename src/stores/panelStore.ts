import { useTableDataStore } from './tableDataStore';
import { useBackendClient, toTableValue } from '@datazen/backend-client';
import { tableSessionKey } from '../lib/session/tableSession';
import { getQueryPanelSessionRegistry } from '../lib/session/queryPanelSessionRoot';
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
  cancelAndCleanupExec,
  cancelAndCleanupPaneExec,
  canCancelPane,
  requestPaneCancel,
  sessionKeysOfPanel,
} from './panelExecCleanup';
import { createRunningCloseGuard } from './runningExecutionClose';
import { DEFAULT_PANE_ID, paneKey } from './paneKeys';
import { resolveNextActive, resetPanelIdCounter, type Panel } from './panelTypes';
import { createPanelCloseNotifier } from './panelCloseNotifier';
import { syncPaneFocus } from './paneFocus';

const resultWriteInflight = new Set<string>();

export type { QueryExecState, BindParams };
export { EMPTY_QUERY_EXEC, emptyQueryExecState } from './queryExecActions';
export {
  DEFAULT_PANE_ID,
  isPaneKeyOfPanel,
  paneArgs,
  paneIdOfPaneKey,
  paneKey,
  paneKeysOfPanel,
  panelIdOfPaneKey,
  resolveFocusedPaneId,
} from './paneKeys';
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
} from './panelTypes';
export { nextPanelId } from './panelTypes';

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
   * Source of truth: which pane editor actions (execute / format / save …)
   * route to, **per tab**. A tab sitting on its default pane has no entry; the
   * presence of a key means "this tab is split and its focus is on that pane".
   *
   * Focus has to be per tab because a pane belongs to a *panel*: with one global
   * value, splitting tab A handed tab B a pane id B never opened, so B's editor
   * resolved to an `addPanel`-unseeded key and its SQL and results became
   * unreachable. See `paneFocusScope` regression tests.
   */
  focusedPaneIdByPanel: Record<string, string>;
  /**
   * Mirror of `focusedPaneIdByPanel[activePanelId]` — the focused pane of the
   * tab on screen, or `null` when that tab is not split. This is the value the
   * view layer hands down; the map above answers per-tab questions.
   *
   * It is written **only** by `syncPaneFocus`, which every focus/activation
   * action goes through, so it cannot drift from the map. That single-writer
   * rule is what `panelStore.panes.test.ts` pins.
   */
  focusedPaneId: string | null;
  queryHistory: QueryHistoryEntry[];
  queryFavorites: FavoriteQuery[];
  /**
   * Resolved favorites directory. Null until the first load, and null again if
   * the lookup failed — the panel then shows no path rather than a wrong one.
   */
  favoritesRoot: string | null;
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

  /**
   * Route editor actions of `panelId` (the active tab by default) to `paneId`;
   * `null` restores that tab's default pane.
   */
  setFocusedPane: (paneId: string | null, panelId?: string) => void;
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
  ) => Promise<void>;
  setChartConfig: (panelId: string, config: ChartConfig, paneId?: string) => void;
  setResultViewMode: (panelId: string, mode: 'table' | 'chart', paneId?: string) => void;

  loadHistory: (connectionId?: string) => Promise<void>;
  openQueryHistory: (connectionId?: string) => Promise<void>;
  setPendingQueryHistory: (connectionId: string | null) => void;
  setPendingHistoryQuery: (query: PendingHistoryQuery | null) => void;
  toggleHistory: () => void;
  loadFavorites: (connectionId?: string) => Promise<void>;
  /**
   * Re-scan the favorites directory. Use this — not `loadFavorites` — whenever
   * the user is about to look at the panel or has just returned to the window:
   * the backend caches the listing, so a `.sql` file a sync client wrote while
   * the app was running is invisible until the cache is dropped.
   */
  refreshFavorites: (connectionId?: string) => Promise<void>;
  addFavorite: (title: string, sql: string, connectionId: string) => Promise<void>;
  deleteFavorite: (id: string) => Promise<void>;
  toggleFavorites: () => void;

  reset: () => void;
}

export const usePanelStore = create<PanelState & PanelActions>((set, get) => ({
  panels: [],
  activePanelId: null,
  queryExec: new Map(),
  focusedPaneIdByPanel: {},
  focusedPaneId: null,
  queryHistory: [],
  queryFavorites: [],
  favoritesRoot: null,
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
      queryExec: nextExec,
      // A freshly opened tab starts on its own default pane, so activating it
      // must not inherit the pane id the previously active tab had focused.
      ...syncPaneFocus(s, [...s.panels, panel], activate ? panel.id : s.activePanelId),
    }));
  },

  removePanel: async (panelId) => {
    const initial = get();
    const panel = initial.panels.find((p) => p.id === panelId);
    if (!panel) return;
    // 关闭顺序见 `runningExecutionClose`：先问用户 → 停掉语句并等终态 → 才释放会话与标签页。
    const guard = askBeforeClosingRunningPanes({ panel, panelId });
    if (guard !== true && !(await guard)) return;
    if (panel.type === 'table' || panel.type === 'view') {
      const registry = getQueryPanelSessionRegistry();
      const key = tableSessionKey(panelId);
      if (registry.peek(key)) {
        try {
          await registry.remove(key);
        } catch {
          useTableDataStore.setState((state) => {
            const previous = state.byPanel.get(panelId);
            if (!previous) return state;
            const byPanel = new Map(state.byPanel);
            byPanel.set(panelId, { ...previous, error: t('query.session.closeBlocked') });
            return { byPanel };
          });
          return; // keep the tab and its transaction controls reachable
        }
      }
    }
    if (panel.type === 'query') {
      const registry = getQueryPanelSessionRegistry();
      // 会话注册表才是权威口径：pane 的会话在编辑器挂载时就开好了，按 exec 表枚举
      // 会漏掉「开过会话但从未执行」的 pane，那正是额度被占满却只剩一个标签页的来源。
      for (const key of sessionKeysOfPanel(initial.queryExec, panelId)) {
        if (!registry.peek(key)) continue;
        try {
          await registry.remove(key);
        } catch {
          set((state) => ({
            queryExec: patchExec(state.queryExec, key, { error: t('query.session.closeBlocked') }),
          }));
          return;
        }
      }
    }
    const { panels, activePanelId, queryExec } = get();
    const current = panels.find((item) => item.id === panelId);
    const nextExec = current ? cancelAndCleanupExec([current], queryExec) : queryExec;
    const nextPanels = panels.filter((item) => item.id !== panelId);
    set((state) => ({
      panels: nextPanels,
      queryExec: nextExec,
      ...syncPaneFocus(state, nextPanels, resolveNextActive(panels, panelId, activePanelId)),
    }));
  },

  removeAllForConnection: (connectionId) => {
    for (const panel of get().panels.filter((item) => item.connectionId === connectionId))
      void get().removePanel(panel.id);
  },

  removePanelsForRelation: (connectionId, tableName, database) => {
    const { panels } = get();
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
    for (const panel of toRemove) void get().removePanel(panel.id);
  },

  removePanelsForDatabase: (connectionId, database, sessionDatabase) => {
    const { panels } = get();
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
    for (const panel of toRemove) void get().removePanel(panel.id);
  },

  setActivePanel: (panelId) => {
    // Switching tabs hands the screen that tab's own focus — never the one the
    // tab being left had.
    set((s) => syncPaneFocus(s, s.panels, panelId ?? null));
  },

  updatePanel: (panelId, patch) => {
    set((s) => ({
      panels: s.panels.map((p) => (p.id === panelId ? ({ ...p, ...patch } as Panel) : p)),
    }));
  },

  closeOtherPanels: (panelId) => {
    for (const panel of get().panels.filter((item) => item.id !== panelId))
      void get().removePanel(panel.id);
  },
  closeAllPanels: () => {
    for (const panel of get().panels) void get().removePanel(panel.id);
  },
  closePanelsToTheRight: (panelId) => {
    const panels = get().panels;
    const index = panels.findIndex((panel) => panel.id === panelId);
    if (index >= 0) for (const panel of panels.slice(index + 1)) void get().removePanel(panel.id);
  },
  closePanelsToTheLeft: (panelId) => {
    const panels = get().panels;
    const index = panels.findIndex((panel) => panel.id === panelId);
    if (index >= 0) for (const panel of panels.slice(0, index)) void get().removePanel(panel.id);
  },

  // ── Panes ─────────────────────────────────────────────────────

  setFocusedPane: (paneId, panelId) => {
    const s = get();
    // Without a tab there is nothing to focus; the default is the tab on screen.
    const target = panelId ?? s.activePanelId;
    if (!target) return;
    set((cur) => syncPaneFocus(cur, cur.panels, cur.activePanelId, { panelId: target, paneId }));
  },

  openPane: (panelId, paneId) => {
    const panel = get().panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query' || !paneId) return;
    const key = paneKey(panelId, paneId);
    set((s) => {
      // Splitting a tab focuses the pane it just opened, and a tab you split is
      // the tab you are working in, so it becomes the active one.
      const focus = syncPaneFocus(s, s.panels, panelId, { panelId, paneId });
      if (s.queryExec.has(key)) return focus;
      return { queryExec: new Map(s.queryExec).set(key, emptyQueryExecState()), ...focus };
    });
  },

  closePane: async (panelId, paneId) => {
    const panel = get().panels.find((p) => p.id === panelId);
    if (!panel || panel.type !== 'query') return;
    const key = paneKey(panelId, paneId);
    // 与整屏关闭同一条顺序：先确认 → 停掉语句并等终态 → 才释放会话与摘掉面板。
    const guard = askBeforeClosingRunningPanes({ panel, panelId, paneId });
    if (guard !== true && !(await guard)) return;
    const registry = getQueryPanelSessionRegistry();
    if (registry.peek(key)) {
      try {
        await registry.remove(key);
      } catch {
        set((state) => ({
          queryExec: patchExec(state.queryExec, key, { error: t('query.session.closeBlocked') }),
        }));
        return;
      }
    }
    const nextExec = cancelAndCleanupPaneExec(panel, paneId, get().queryExec);
    set((s) => {
      // A closed pane cannot stay focused, but only *its own* tab loses focus:
      // the mirror is re-derived from the active tab, so closing a pane in a
      // background tab leaves the focus on screen untouched.
      const clearsFocus = s.focusedPaneIdByPanel[panelId] === paneId;
      return {
        queryExec: nextExec,
        ...syncPaneFocus(
          s,
          s.panels,
          s.activePanelId,
          clearsFocus ? { panelId, paneId: null } : undefined,
        ),
      };
    });
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
        panel.connectionId,
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
        undefined,
        panel.connectionId,
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
        panel.connectionId,
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
        undefined,
        panel.connectionId,
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
      const controller = getQueryPanelSessionRegistry().peek(key);
      if (!controller) throw new Error('Editor session unavailable');
      await controller.cancel();
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
    set((s) => ({
      queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), { activeResultIdx: idx }),
    }));
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

  /** The **narrow** channel's write path — see `panelResultEdit` for the channels. */
  updateResultCell: async (panelId, resultIdx, row, col, value, paneId) => {
    const key = paneKey(panelId, paneId);
    const exec = get().queryExec.get(key);
    if (!exec) return;
    const original = exec.results[resultIdx];
    if (resultWriteInflight.has(key)) throw new Error('A result edit is already in progress');
    resultWriteInflight.add(key);
    try {
      // Refusals go through the error handler below, so they reach the user.
      if (!original?.runtimeBinding || original.source?.writableMapping !== 'verified')
        throw new Error('Result is read-only; execute the query again');
      const columnIndex = original.columns.findIndex((column) => column.name === col);
      if (columnIndex < 0 || !original.rows[row]) throw new Error('Result cell unavailable');
      if (JSON.stringify(original.rows[row][columnIndex]) === JSON.stringify(value)) return;
      // The write goes through the bound `BackendClient` so the desktop and a
      // browser build take the same command path. The binding travels with the
      // request: the backend refuses a result whose runtime resource is gone
      // instead of writing through some other handle.
      const cellValue = toTableValue(value);
      if (cellValue === undefined) throw new Error('Result cell value is not sendable');
      await useBackendClient().commitQueryResultCell({
        binding: original.runtimeBinding,
        statementIndex: original.source.statementIndex,
        rowIndex: row,
        column: col,
        value: cellValue,
      });
    } catch (error) {
      set((state) => ({
        queryExec: patchExec(state.queryExec, key, {
          error: error instanceof Error ? error.message : String(error),
        }),
      }));
      throw error;
    } finally {
      resultWriteInflight.delete(key);
    }
    // Retained artifacts are immutable. This edited snapshot requires a fresh
    // explicit query before another edit; never replay arbitrary original SQL.
    const current = get().queryExec.get(key);
    if (!current || current.results[resultIdx]?.runtimeBinding !== original.runtimeBinding) return;
    const results = current.results.map((r, ri) => {
      if (ri !== resultIdx) return r;
      const colIdx = r.columns.findIndex((c) => c.name === col);
      if (colIdx === -1) return r;
      const rows = r.rows.map((rowArr, rowI) => {
        if (rowI !== row) return rowArr;
        const next = [...rowArr];
        next[colIdx] = value as Value;
        return next;
      });
      return { ...r, rows, runtimeBinding: null };
    });
    set((s) => ({ queryExec: patchExec(s.queryExec, key, { results }) }));
  },

  setChartConfig: (panelId, config, paneId) => {
    set((s) => ({
      queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), { chartConfig: config }),
    }));
  },

  setResultViewMode: (panelId, mode, paneId) => {
    set((s) => ({
      queryExec: patchExec(s.queryExec, paneKey(panelId, paneId), { resultViewMode: mode }),
    }));
  },

  // ── History / Favorites ────────────────────────────────────────

  loadHistory: async (connectionId) => {
    const { entries } = await queryCommands.getQueryHistoryPage({ limit: 1000, connectionId });
    set({ queryHistory: entries });
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

  refreshFavorites: async (connectionId) => {
    // The root is a display concern, not a blocker: if it cannot be read the
    // panel simply omits the path rather than showing a stale one.
    const [queryFavorites, favoritesRoot] = await Promise.all([
      queryCommands.refreshFavorites(connectionId),
      queryCommands.getFavoritesRoot(),
    ]);
    set({ queryFavorites, favoritesRoot });
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
      focusedPaneIdByPanel: {},
      focusedPaneId: null,
      queryHistory: [],
      queryFavorites: [],
      favoritesRoot: null,
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

/**
 * Ask before closing something that is still running.
 *
 * The conversation itself lives in `runningExecutionClose`; this only wires it to
 * this store's state, so the rules are testable without a store and the store
 * keeps no copy of them.
 */
const askBeforeClosingRunningPanes = createRunningCloseGuard({
  queryExec: () => usePanelStore.getState().queryExec,
  hasSession: (panelId, paneId) =>
    getQueryPanelSessionRegistry().peek(paneKey(panelId, paneId)) != null,
  canCancel: canCancelPane,
  cancelPane: (panelId, paneId) =>
    requestPaneCancel(() => usePanelStore.getState(), panelId, paneId),
  markBlocked: (panelId, paneId) => {
    usePanelStore.setState((state) => ({
      queryExec: patchExec(state.queryExec, paneKey(panelId, paneId), {
        error: t('query.session.closeBlocked'),
      }),
    }));
  },
});
