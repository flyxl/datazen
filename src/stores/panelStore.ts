import { create } from 'zustand';
import { queryCommands } from '../commands/query';
import { t } from '../locales/t';
import { DB_REGISTRY } from '../lib/databaseTypes';
import {
  buildPathHierarchyDatabasePin,
  inferSqlRelationPath,
  namespaceRootsFrom,
  pathHierarchyConnectionRoot,
  pathHierarchyRelativeNamespacePath,
  resolveQueryContextPath,
} from '../lib/queryContextPath';
import type { DatabaseType, FavoriteQuery, QueryHistoryEntry, Value } from '../types';
import type { ChartConfig } from '../types/chart';
import type { TrendSeries } from '../lib/serverStatusTrends';
import { getCancelCapability } from '../lib/queryExecutionViewModel';
import { reduceQueryExecutionState } from '../lib/queryExecutionViewModel';
import { useSchemaStore } from './schemaStore';
import { useActiveConnectionStore } from './activeConnectionStore';
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
  /** Persistent connection id (saved connection this panel belongs to). */
  connectionId: string;
  /** Live database session id (from activeConnectionStore). */
  dbSessionId: string;
  connectionName: string;
  databaseType: DatabaseType;
}

export interface TablePanel extends PanelBase {
  type: 'table';
  tableName: string;
  /** Database the table was opened from (multi-db drivers). */
  database?: string;
  tableSchema?: string;
  subTab: SubTabId;
  structureEditing?: boolean;
}

export interface ViewPanel extends PanelBase {
  type: 'view';
  viewName: string;
  /** Database the view was opened from (multi-db drivers). */
  database?: string;
  viewSchema?: string;
  subTab: SubTabId;
}

export interface QueryPanel extends PanelBase {
  type: 'query';
  title: string;
  /** Superset / path-hierarchy connection database (not catalog name). */
  database?: string;
  schema?: string;
  /** Catalog/schema segments for path-hierarchy query context selectors. */
  namespacePath?: string[];
}

export interface CreateTablePanel extends PanelBase {
  type: 'create-table';
  /** Database the create-table panel was opened from (multi-db drivers). */
  database?: string;
  /** PG schema namespace when known from sidebar selection. */
  tableSchema?: string;
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

/** 服务器仪表盘面板（含 仪表盘 / 状态变量 / 服务器详情 三个内部子标签）。 */
export interface ServerStatusPanel extends PanelBase {
  type: 'server-status';
  /** 该面板自身缓存的数据（与 connectionId 绑定，切换时保留已加载内容）。 */
  data?: ServerStatusCache;
}

export interface ServerStatusCache {
  status: Record<string, string | number | boolean | null>;
  variables?: { name: string; value: string | null }[];
  history?: Record<string, TrendSeries>;
  /** 上次成功刷新时刻（wall-clock ms），用于「上次更新时间」显示。 */
  updatedAt?: number;
}

/** 进程列表面板（独立于服务器仪表盘）。 */
export interface ProcessesPanel extends PanelBase {
  type: 'processes';
  /** 该面板自身缓存的数据（与 connectionId 绑定）。 */
  data?: ProcessListCacheData;
}

export interface ProcessListCacheData {
  rows: (string | number | boolean | null)[][];
  columns?: { name: string; dataType: string; nullable?: boolean }[];
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
  | ServerStatusPanel
  | ProcessesPanel
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
  connectionId: string;
  dbSessionId: string;
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

/**
 * F1: the SQL editor's database dropdown is pure local state
 * (`schemaStore.currentDatabase`), so every execution must carry the panel's
 * selected database explicitly — the backend pins the session to it before
 * running unqualified SQL (BUG-001 fix).
 */
function panelTargetDatabase(panel: QueryPanel, sql?: string): string | null {
  const schemaState = useSchemaStore.getState().schemas.get(panel.dbSessionId);
  const meta = DB_REGISTRY[panel.databaseType];
  if (meta?.namespaceEnsure === 'path-hierarchy') {
    const databases = schemaState?.databases ?? [];
    const pathAliases = schemaState?.pathAliases ?? {};
    const namespaceTree = schemaState?.namespaceTree ?? {};
    const roots = namespaceRootsFrom(namespaceTree, pathAliases, databases);
    const namespaceRootNames = Object.keys(namespaceTree);
    const rootSet = new Set(roots);
    let fromSql = sql?.trim()
      ? resolveQueryContextPath(sql, { databases, namespaceRoots: roots })
      : null;
    if (sql?.trim()) {
      const relation = inferSqlRelationPath(sql);
      if (relation.length >= 3) {
        const inferred = relation.slice(0, -1);
        if (!fromSql || inferred.length > fromSql.length) fromSql = inferred;
      } else if (relation.length >= 2) {
        const inferred = relation.slice(0, -1);
        if (inferred[0] && rootSet.has(inferred[0])) {
          if (!fromSql || inferred.length > fromSql.length) fromSql = inferred;
        }
      }
    }
    const namespacePath = fromSql ?? panel.namespacePath ?? [];
    const relativeNamespacePath = pathHierarchyRelativeNamespacePath(
      databases,
      namespaceTree,
      namespacePath,
    );
    const root = pathHierarchyConnectionRoot(
      databases,
      panel.database,
      schemaState?.currentDatabase ?? null,
      pathAliases,
      namespaceRootNames,
    );
    if (!root && relativeNamespacePath.length === 0) return null;
    if (!root) return relativeNamespacePath.join('/');
    return buildPathHierarchyDatabasePin(root, relativeNamespacePath);
  }
  if (panel.database?.trim()) return panel.database;
  return schemaState?.currentDatabase ?? null;
}

/**
 * F7: the PG-family current schema is pure local state
 * (`schemaStore.currentSchema`, set via `setCurrentSchema`), so executions
 * carry it explicitly as the envelope `schema` field — rewrite-capable
 * drivers inline it (`"schema"."t"`); others ignore it.
 */
function panelTargetSchema(panel: QueryPanel): string | null {
  if (panel.schema?.trim()) return panel.schema;
  return useSchemaStore.getState().schemas.get(panel.dbSessionId)?.currentSchema ?? null;
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
    counter = 0;
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
