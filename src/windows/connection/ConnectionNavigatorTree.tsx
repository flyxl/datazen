import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  Braces,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  Database,
  Download,
  Eye,
  FolderClosed,
  FolderOpen,
  FolderPlus,
  Hash,
  Loader2,
  PanelLeftClose,
  Plus,
  RefreshCw,
  Search,
  Shapes,
  Table2,
  Upload,
  Zap,
} from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { formatGroupLabel } from '../../lib/connectionGroups';
import { DB_REGISTRY, escapeIdent } from '../../lib/databaseTypes';
import { getSqlDialect } from '../../lib/sqlDialects';
import { invalidateSchemaCache } from '../../lib/schemaCache';
import { isLeaf, pathKey, type SqlNamespace } from '../../lib/sqlNamespace';
import {
  buildMainConnectionContextMenuItems,
  buildMainGroupContextMenuItems,
} from '../../lib/mainWindowContextMenu';
import { buildSchemaTreeContextMenuItems } from '../../lib/schemaTreeContextMenu';
import { buildConnectionUrl } from '../../lib/buildConnectionUrl';
import {
  groupConnectionsWithPinnedSection,
  PINNED_GROUP_KEY,
  useConnectionStore,
} from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { useSchemaStore } from '../../stores/schemaStore';
import type { ConnectionOpenTarget } from '../../lib/connectionViews/types';
import { showWebContextMenu } from '../../stores/contextMenuStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { shouldUseMultiDatabaseTree } from './schema-tree/SchemaTree';
import { useExpandedDbCacheRefresh } from './schema-tree/useExpandedDbCacheRefresh';
import { connectionCommands } from '../../commands/connection';
import { databaseCommands } from '../../commands/database';
import { driverCommands } from '../../commands/driver';
import { queryCommands } from '../../commands/query';
import { hasCommand } from '../../lib/commandSchema';
import { SERVER_STATUS_SNAPSHOT_COMMAND, LIST_PROCESSES_COMMAND } from '../../lib/driverCommandIds';
import {
  openDataSyncWindow,
  openDataTransferWindow,
  openSchemaDiffWindow,
  openBackupWindow,
} from '../../lib/windowManager';
import { ObjectFilterDialog } from '../../components/connection/ObjectFilterDialog';
import {
  filterTableItems,
  getObjectFilter,
  matchesTableNameFilter,
  shouldShowDatabase,
  shouldShowSchema,
} from '../../lib/objectFilter';
import type { ConnectionConfig, DatabaseObject, TableInfo } from '../../types';
import type { ObjectFilterPrefs } from '../../lib/objectFilter';

// ── Category definitions ────────────────────────────────────────

interface CategoryDef {
  id: string;
  labelKey: string;
  icon: React.ComponentType<{ className?: string }>;
  color: string;
}

const BASE_CATEGORIES: CategoryDef[] = [
  { id: 'tables', labelKey: 'schemaTree.tables', icon: Table2, color: 'text-blue-400' },
  { id: 'views', labelKey: 'schemaTree.views', icon: Eye, color: 'text-purple-400' },
];

const OBJECT_KIND_CATEGORIES: Record<string, CategoryDef> = {
  function: {
    id: 'function',
    labelKey: 'schemaTree.functions',
    icon: Braces,
    color: 'text-orange-400',
  },
  procedure: {
    id: 'procedure',
    labelKey: 'schemaTree.procedures',
    icon: Braces,
    color: 'text-emerald-400',
  },
  trigger: { id: 'trigger', labelKey: 'schemaTree.triggers', icon: Zap, color: 'text-amber-400' },
  sequence: {
    id: 'sequence',
    labelKey: 'schemaTree.sequences',
    icon: Hash,
    color: 'text-cyan-400',
  },
  type: { id: 'type', labelKey: 'schemaTree.types', icon: Shapes, color: 'text-pink-400' },
};

function getCategoriesForDriver(databaseType: string): CategoryDef[] {
  const meta = DB_REGISTRY[databaseType as keyof typeof DB_REGISTRY];
  const objectKinds = meta?.supportedObjectKinds;
  if (!objectKinds || objectKinds.length === 0) return BASE_CATEGORIES;
  const objectCats = objectKinds.map((kind) => OBJECT_KIND_CATEGORIES[kind]).filter(Boolean);
  return [...BASE_CATEGORIES, ...objectCats];
}

const LEAF_KIND_ICON: Record<
  string,
  { icon: React.ComponentType<{ className?: string }>; color: string }
> = {
  table: { icon: Table2, color: 'text-blue-400' },
  view: { icon: Eye, color: 'text-purple-400' },
  materializedView: { icon: Eye, color: 'text-purple-400' },
  systemTable: { icon: Table2, color: 'text-gray-400' },
  function: { icon: Braces, color: 'text-orange-400' },
  procedure: { icon: Braces, color: 'text-emerald-400' },
  trigger: { icon: Zap, color: 'text-amber-400' },
  sequence: { icon: Hash, color: 'text-cyan-400' },
  type: { icon: Shapes, color: 'text-pink-400' },
};

// ── Flat row types ──────────────────────────────────────────────

type UnifiedRow =
  | { type: 'group'; groupName: string; displayName: string; count: number; expanded: boolean }
  | {
      type: 'connection';
      conn: ConnectionConfig;
      isSelected: boolean;
      status: string;
      expanded: boolean;
      depth: number;
    }
  | {
      type: 'db';
      connectionId: string;
      dbSessionId: string;
      dbName: string;
      expanded: boolean;
      loading: boolean;
      depth: number;
    }
  | {
      type: 'schema';
      connectionId: string;
      dbName: string;
      schemaName: string;
      expanded: boolean;
      depth: number;
    }
  | {
      type: 'category';
      key: string;
      cat: CategoryDef;
      count: number;
      expanded: boolean;
      depth: number;
    }
  | {
      type: 'table';
      item: TableInfo;
      depth: number;
      catId: string;
      isSelected: boolean;
      connectionId: string;
      dbSessionId: string;
      dbName: string;
    }
  | { type: 'object'; obj: DatabaseObject; depth: number; catId: string }
  | {
      type: 'kv-db';
      connectionId: string;
      dbSessionId: string;
      dbName: string;
      depth: number;
      isSelected: boolean;
    }
  | { type: 'db-loading'; depth: number }
  | {
      type: 'namespace-node';
      name: string;
      depth: number;
      expanded: boolean;
      isLeaf: boolean;
      /** Object kind for leaf nodes (table, view, function, etc.) */
      leafKind?:
        | 'table'
        | 'view'
        | 'materializedView'
        | 'systemTable'
        | 'function'
        | 'procedure'
        | 'trigger';
      /** Path segments from root to this node (for ensureNamespacePath) */
      segments: string[];
      key: string;
      connectionId: string;
      dbSessionId: string;
    }
  | { type: 'empty-group' }
  | { type: 'no-connections' };

/** Check if any key in the namespace tree contains the query string. */
function namespaceTreeContains(tree: SqlNamespace, query: string): boolean {
  if (isLeaf(tree)) return false;
  for (const [key, child] of Object.entries(tree)) {
    if (key.toLowerCase().includes(query)) return true;
    if (!isLeaf(child) && namespaceTreeContains(child, query)) return true;
  }
  return false;
}

/**
 * Flatten a SqlNamespace tree into UnifiedRow entries for path-hierarchy drivers.
 * Branches become expandable namespace-node rows; leaves get their kind from tables metadata.
 */
function flattenNamespaceTree(
  tree: SqlNamespace,
  connectionId: string,
  dbSessionId: string,
  baseDepth: number,
  rows: UnifiedRow[],
  expandedDbs: Set<string>,
  query: string,
  tableTypeMap: Map<string, TableInfo['tableType']>,
  loadedPaths: Set<string>,
  parentSegments: string[] = [],
): void {
  if (isLeaf(tree)) return;

  const entries = Object.entries(tree).sort(([a], [b]) => a.localeCompare(b));
  for (const [name, child] of entries) {
    if (query && !name.toLowerCase().includes(query)) {
      if (isLeaf(child)) continue;
      if (!namespaceTreeContains(child, query)) continue;
    }

    const segments = [...parentSegments, name];
    const nodeKey = `${connectionId}::ns::${segments.join('/')}`;
    const nodeIsLeaf = isLeaf(child);

    if (nodeIsLeaf) {
      rows.push({
        type: 'namespace-node',
        name,
        depth: baseDepth,
        expanded: false,
        isLeaf: true,
        leafKind: tableTypeMap.get(name) ?? 'table',
        segments,
        key: nodeKey,
        connectionId,
        dbSessionId,
      });
    } else {
      const expanded = expandedDbs.has(nodeKey) || !!query;
      rows.push({
        type: 'namespace-node',
        name,
        depth: baseDepth,
        expanded,
        isLeaf: false,
        segments,
        key: nodeKey,
        connectionId,
        dbSessionId,
      });
      if (expanded) {
        const childEntries = Object.entries(child);
        const pathLoaded = loadedPaths.has(pathKey(segments));
        if (childEntries.length === 0 && !pathLoaded && !query) {
          rows.push({ type: 'db-loading', depth: baseDepth + 1 });
        } else {
          flattenNamespaceTree(
            child,
            connectionId,
            dbSessionId,
            baseDepth + 1,
            rows,
            expandedDbs,
            query,
            tableTypeMap,
            loadedPaths,
            segments,
          );
        }
      }
    }
  }
}

const ROW_HEIGHT = 28;

function depthPadding(depth: number): string {
  return `${0.375 + depth * 1}rem`;
}

function groupBySchema(
  items: TableInfo[],
  extraSchemaNames?: string[],
): Map<string, TableInfo[]> | null {
  const hasAnySchema = items.some((i) => !!i.schema);
  if (!hasAnySchema && (!extraSchemaNames || extraSchemaNames.length === 0)) return null;

  const map = new Map<string, TableInfo[]>();
  if (extraSchemaNames) {
    for (const s of extraSchemaNames) map.set(s, []);
  }
  for (const item of items) {
    if (!item.name) continue;
    const key = item.schema ?? '';
    const arr = map.get(key);
    if (arr) arr.push(item);
    else map.set(key, [item]);
  }
  return map;
}

/** Pick a safe database to switch to before dropping `dropping`. */
function resolveDropDatabaseFallback(
  databases: string[],
  dropping: string,
  configuredDb?: string,
): string | null {
  if (databases.includes('postgres') && dropping !== 'postgres') return 'postgres';
  const configured = configuredDb?.trim();
  if (configured && configured !== dropping && databases.includes(configured)) {
    return configured;
  }
  return databases.find((d) => d !== dropping) ?? null;
}

function extractErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function quoteRelationName(
  name: string,
  schema: string | undefined,
  databaseType: string,
): string {
  const quote = (part: string) => escapeIdent(part, databaseType as ConnectionConfig['databaseType']);
  return schema ? `${quote(schema)}.${quote(name)}` : quote(name);
}

// ── Props ───────────────────────────────────────────────────────

export interface ConnectionNavigatorTreeHandle {
  refreshAllConnections: () => Promise<void>;
  refreshConnection: (connectionId: string) => Promise<void>;
}

export interface ConnectionNavigatorTreeProps {
  onSelectConnection: (connectionId: string) => void;
  onSelectTable: (tableName: string, schema?: string, database?: string) => void;
  onSelectKvDb?: (connectionId: string, dbName: string) => void;
  activeConnectionId: string | null;
  onNewConnection: () => void;
  onRefresh?: () => void;
  onEditConnection: (connectionId: string) => void;
  onDeleteConnection: (connectionId: string) => void;
  onDisconnect: (connectionId: string) => void;
  onExportConnections?: () => void;
  onImportConnections?: () => void;
  onCollapseSidebar?: () => void;
  onShowMessage?: (text: string, kind: 'error' | 'success') => void;
  onNodeContextMenu?: (payload: {
    kind: string;
    name: string;
    x: number;
    y: number;
    schema?: string;
  }) => void;
  viewActions?: {
    newQuery?: (initialSql?: string) => void;
    openSqlFile?: () => void;
    createTable?: () => void;
    openCreateDatabase?: () => void;
    openCreateSchema?: () => void;
    openCreateUser?: () => void;
    openErDiagram?: (focusTable?: string) => void;
    refresh?: () => void;
    openObject?: (
      kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
      name: string,
      schema?: string,
    ) => void;
    openQueryHistory?: () => void;
    openServerStatus?: (ctx?: ConnectionOpenTarget) => void;
    openProcessList?: (ctx?: ConnectionOpenTarget) => void;
  };
}

// ── Component ───────────────────────────────────────────────────

export const ConnectionNavigatorTree = forwardRef<
  ConnectionNavigatorTreeHandle,
  ConnectionNavigatorTreeProps
>(function ConnectionNavigatorTree(
  {
    onSelectConnection,
    onSelectTable,
    onSelectKvDb,
    activeConnectionId,
    onNewConnection,
    onRefresh,
    onEditConnection,
    onDeleteConnection,
    onDisconnect,
    onExportConnections,
    onImportConnections,
    onCollapseSidebar,
    onShowMessage,
    viewActions,
  },
  ref,
) {
  const { t } = useI18n();
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const [searchQuery, setSearchQuery] = useState('');
  const connections = useConnectionStore((s) => s.connections);
  const groups = useConnectionStore((s) => s.groups);
  const duplicateConnection = useConnectionStore((s) => s.duplicateConnection);
  const addGroup = useConnectionStore((s) => s.addGroup);
  const deleteGroup = useConnectionStore((s) => s.deleteGroup);
  const renameGroup = useConnectionStore((s) => s.renameGroup);
  const moveConnectionToGroup = useConnectionStore((s) => s.moveConnectionToGroup);
  const toggleConnectionPinned = useConnectionStore((s) => s.toggleConnectionPinned);
  const saveConnection = useConnectionStore((s) => s.saveConnection);
  const activeConnections = useActiveConnectionStore((s) => s.connections);
  const connect = useActiveConnectionStore((s) => s.connect);

  /**
   * 由右键点击的连接显式构造「打开进程列表 / 服务器仪表盘」的目标连接，
   * 直接把 connectionId + 当前实时 dbSessionId 传给面板，避免依赖全局活动连接串数据。
   */
  const buildOpenTarget = useCallback(
    (conn: { id: string; name: string; databaseType: string }): ConnectionOpenTarget | null => {
      const live = activeConnections[conn.id]?.dbSessionId;
      if (!live) return null;
      return {
        connectionId: conn.id,
        dbSessionId: live,
        connectionName: conn.name,
        databaseType: conn.databaseType as ConnectionOpenTarget['databaseType'],
      };
    },
    [activeConnections],
  );

  // ── Group dialog state ──
  const [newGroupDialogOpen, setNewGroupDialogOpen] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');
  const [renamingGroup, setRenamingGroup] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [confirmDeleteGroup, confirmDeleteGroupDialog] = useConfirmDialog();
  const [confirmDropDatabase, confirmDropDatabaseDialog] = useConfirmDialog();
  const [confirmDropSchema, confirmDropSchemaDialog] = useConfirmDialog();
  const [confirmDropRelation, confirmDropRelationDialog] = useConfirmDialog();
  const [confirmTruncateTable, confirmTruncateTableDialog] = useConfirmDialog();
  const removeRelation = useSchemaStore((s) => s.removeRelation);
  const [objectFilterConn, setObjectFilterConn] = useState<ConnectionConfig | null>(null);
  const schemas = useSchemaStore((s) => s.schemas);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const ensureNamespacePath = useSchemaStore((s) => s.ensureNamespacePath);

  const scrollRef = useRef<HTMLDivElement>(null);

  // Always get all connections grouped; deep search filtering happens in flatRows
  const grouped = useMemo(
    () => groupConnectionsWithPinnedSection(connections, groups, ''),
    [connections, groups],
  );

  // ── Expansion states (composite keys) ──

  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [expandedConnections, setExpandedConnections] = useState<Set<string>>(new Set());
  const [expandedDbs, setExpandedDbs] = useState<Set<string>>(new Set());
  const [expandedSchemas, setExpandedSchemas] = useState<Set<string>>(new Set());
  const [expandedCats, setExpandedCats] = useState<Set<string>>(new Set());

  // ── Per-connection multi-db data ──

  const [dbTablesMap, setDbTablesMap] = useState<Record<string, TableInfo[]>>({});
  const [dbObjectsMap, setDbObjectsMap] = useState<Record<string, DatabaseObject[]>>({});
  const [loadingDbs, setLoadingDbs] = useState<Set<string>>(new Set());

  const reloadDbTables = useCallback(async (dbSessionId: string, dbName: string) => {
    const tableKey = `${dbSessionId}::${dbName}`;
    try {
      // No useDatabase here: get_tables is session-neutral in every driver,
      // so refreshing a cache must never flip the shared SQL session.
      const all = await databaseCommands.getTables(dbSessionId, dbName);
      setDbTablesMap((prev) => ({ ...prev, [tableKey]: all }));
      useSchemaStore.getState().setLoadedTables(dbName, all, dbSessionId);
    } catch {
      // ignore
    }
  }, []);

  const activateDatabase = useCallback(
    async (dbSessionId: string, dbName: string) => {
      const tableKey = `${dbSessionId}::${dbName}`;
      const cached = dbTablesMap[tableKey];
      if (cached) {
        useSchemaStore.getState().setLoadedTables(dbName, cached, dbSessionId);
        return;
      }
      // F1: no use_database IPC — track the active database as local UI state;
      // query commands pin it explicitly and the backend switches lazily.
      useSchemaStore.setState((state) => {
        const entry = state.schemas.get(dbSessionId);
        if (!entry || entry.currentDatabase === dbName) return state;
        const next = new Map(state.schemas);
        next.set(dbSessionId, { ...entry, currentDatabase: dbName });
        return { ...state, schemas: next };
      });
    },
    [dbTablesMap],
  );

  const clearDbLocalCache = useCallback(
    (connectionId: string, dbSessionId: string, dbName: string) => {
      const tableKey = `${dbSessionId}::${dbName}`;
      const dbKey = `${connectionId}::${dbName}`;
      setDbTablesMap((prev) => {
        if (!(tableKey in prev)) return prev;
        const next = { ...prev };
        delete next[tableKey];
        return next;
      });
      setDbObjectsMap((prev) => {
        const prefix = `${connectionId}::${dbName}::`;
        const hasMatch = Object.keys(prev).some((k) => k.startsWith(prefix));
        if (!hasMatch) return prev;
        const next = { ...prev };
        for (const key of Object.keys(next)) {
          if (key.startsWith(prefix)) delete next[key];
        }
        return next;
      });
      setExpandedDbs((prev) => {
        const next = new Set(prev);
        next.delete(dbKey);
        for (const key of prev) {
          if (key.startsWith(`${connectionId}::${dbName}::`)) next.delete(key);
        }
        return next;
      });
      setExpandedSchemas((prev) => {
        const next = new Set(prev);
        for (const key of prev) {
          if (key.startsWith(`${connectionId}::${dbName}::`)) next.delete(key);
        }
        return next;
      });
      setExpandedCats((prev) => {
        const next = new Set(prev);
        for (const key of prev) {
          if (key.startsWith(`${connectionId}::${dbName}::`)) next.delete(key);
        }
        return next;
      });
    },
    [],
  );

  const reloadDbObjectCategory = useCallback(
    async (dbSessionId: string, catKey: string, catId: string) => {
      if (catId === 'tables' || catId === 'views') return;
      try {
        const objs = await databaseCommands.getDatabaseObjects(dbSessionId, catId);
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: objs }));
      } catch {
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: [] }));
      }
    },
    [],
  );

  // Invalidate + reload per-db caches when the connection's schema surface
  // genuinely changes (db list or epoch). Shared hook — no useDatabase inside.
  // F1-BUG-005: the epoch-triggered invalidation used to drop object-category
  // caches without reloading them (and filtered them by session id although
  // their keys use the persistent connection id). The hook now wipes with both
  // ids and schedules its own category recovery wave, so expanded categories
  // always come back after a refresh instead of staying empty.
  useExpandedDbCacheRefresh({
    activeConnections,
    expandedDbs,
    expandedCats,
    loadTablesForDb: reloadDbTables,
    loadObjectsForCat: reloadDbObjectCategory,
    clearCaches: (sessionId: string, connectionId?: string) => {
      setDbTablesMap((m) => {
        const next: Record<string, TableInfo[]> = {};
        for (const key of Object.keys(m)) {
          if (!key.startsWith(sessionId + '::')) next[key] = m[key];
        }
        return next;
      });
      if (!connectionId) return;
      // Object-category keys are "<connectionId>::<dbName>::[<schema>::]<cat>".
      setDbObjectsMap((m) => {
        const prefix = connectionId + '::';
        const next: Record<string, DatabaseObject[]> = {};
        for (const key of Object.keys(m)) {
          if (!key.startsWith(prefix)) next[key] = m[key];
        }
        return next;
      });
    },
  });

  // ── Auto-expand groups on first load ──

  const prevGroupsRef = useRef<string[] | null>(null);
  useEffect(() => {
    const prev = prevGroupsRef.current;
    prevGroupsRef.current = groups;
    if (!prev) {
      setExpandedGroups(new Set([...groups, '']));
      return;
    }
    const newGroups = groups.filter((g) => !prev.includes(g));
    if (newGroups.length === 0) return;
    setExpandedGroups((s) => {
      const next = new Set(s);
      for (const g of newGroups) next.add(g);
      next.add('');
      return next;
    });
  }, [groups]);

  // Auto-expand connected connections in the tree
  useEffect(() => {
    setExpandedConnections((prev) => {
      let changed = false;
      const next = new Set(prev);
      for (const [connectionId, entry] of Object.entries(activeConnections)) {
        if (entry?.status === 'connected' && !next.has(connectionId)) {
          next.add(connectionId);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [activeConnections]);

  // ── Load schema when connection expanded ──

  const loadedConnectionsRef = useRef<Set<string>>(new Set());
  const prevConnectionIdsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const currentIds = new Set(
      Object.keys(activeConnections).filter((id) => activeConnections[id]?.status === 'connected'),
    );
    for (const id of prevConnectionIdsRef.current) {
      if (!currentIds.has(id)) {
        loadedConnectionsRef.current.delete(id);
      }
    }
    prevConnectionIdsRef.current = currentIds;
  }, [activeConnections]);

  useEffect(() => {
    for (const connectionId of expandedConnections) {
      const entry = activeConnections[connectionId];
      if (entry?.status !== 'connected' || !entry.dbSessionId) continue;
      if (loadedConnectionsRef.current.has(connectionId)) continue;
      loadedConnectionsRef.current.add(connectionId);

      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) continue;

      const meta = DB_REGISTRY[conn.databaseType];
      const isCustomTree = meta?.schemaTreeMode === 'custom';
      const isPathHierarchyOnly = meta?.namespaceEnsure === 'path-hierarchy' && !isCustomTree;
      const isPluginManaged = isCustomTree || isPathHierarchyOnly;
      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      void loadForConnection(entry.dbSessionId, {
        preferredDatabase: conn.database,
        skipLoadTables: isMultiDb || isPluginManaged,
        databaseType: conn.databaseType,
      }).then(() => {
        if (isPathHierarchyOnly) {
          void ensureNamespacePath([], entry.dbSessionId);
        }

        if (isMultiDb) {
          const sd = useSchemaStore.getState().schemas.get(entry.dbSessionId);
          const configuredDb = conn.database?.trim();
          const dbName =
            configuredDb && sd?.databases.includes(configuredDb)
              ? configuredDb
              : (sd?.currentDatabase ?? sd?.databases[0]);
          if (dbName) {
            const dbKey = `${connectionId}::${dbName}`;
            setExpandedDbs((prev) => new Set(prev).add(dbKey));
            setExpandedCats((prev) => new Set(prev).add(`${dbKey}::tables`));
            void reloadDbTables(entry.dbSessionId, dbName);
          }
        }
      });

      // Auto-expand default database for standard single-db schema trees
      if (!isMultiDb && !isPluginManaged && conn.database) {
        const dbKey = `${connectionId}::${conn.database}`;
        setExpandedDbs((prev) => new Set(prev).add(dbKey));
        setExpandedCats((prev) => new Set(prev).add(`${dbKey}::tables`));
      }
    }
  }, [
    expandedConnections,
    activeConnections,
    connections,
    loadForConnection,
    ensureNamespacePath,
    reloadDbTables,
  ]);

  // ── Context menu labels ──

  const contextLabels = useMemo(
    () => ({
      newGroup: t('common.newGroup'),
      newConnection: t('common.newConnection'),
      renameGroup: t('main.ctx.renameGroup'),
      deleteGroup: t('main.ctx.deleteGroup'),
      openConnection: t('main.ctx.openConnection'),
      disconnect: t('main.ctx.disconnect'),
      editConnection: t('common.editConnection'),
      duplicateConnection: t('main.ctx.duplicateConnection'),
      moveToGroup: t('main.ctx.moveToGroup'),
      removeFromGroup: t('main.ctx.removeFromGroup'),
      deleteConnection: t('common.deleteConnection'),
      copyName: t('common.copyName'),
      copyConnectionUrl: t('main.ctx.copyConnectionUrl'),
      newQuery: t('common.newQuery'),
      queryHistory: t('main.ctx.queryHistory'),
      executeSqlFile: t('common.executeSqlFile'),
      createDatabase: t('common.createDatabase'),
      createSchema: t('common.createSchema'),
      createUser: t('common.createUser'),
      refresh: t('main.ctx.refresh'),
      pinConnection: t('main.ctx.pinConnection'),
      unpinConnection: t('main.ctx.unpinConnection'),
      objectFilter: t('common.objectFilter'),
      processList: t('common.processList'),
      serverStatus: t('common.serverStatus'),
      backup: t('common.backupDatabase'),
      restore: t('common.restoreDatabase'),
    }),
    [t],
  );

  const schemaLabels = useMemo(
    () => ({
      open: t('schemaTree.openTable'),
      openStructure: t('schemaTree.openStructure'),
      copyName: t('common.copyName'),
      copyDdl: t('common.copyDdl'),
      focusEr: '',
      exportData: t('common.exportData'),
      importData: t('common.importData'),
      refresh: t('connWin.refresh'),
      newQuery: t('common.newQuery'),
      queryHistory: t('main.ctx.queryHistory'),
      copyDatabaseName: t('schemaTree.copyDatabaseName'),
      newTable: t('common.newTable'),
      batchExport: `${t('batchExport.title')}…`,
      truncate: t('schemaTree.truncate'),
      drop: t('schemaTree.drop'),
      dropView: t('schemaTree.dropView'),
      dropDatabase: t('schemaTree.dropDatabase'),
      viewErDiagram: t('schemaTree.viewErDiagram'),
      newSchema: t('schemaTree.newSchema'),
      createSchema: t('common.createSchema'),
      dropSchema: t('schemaTree.dropSchema'),
      executeSqlFile: t('common.executeSqlFile'),
      dataTransfer: t('common.dataTransfer'),
      compareSchema: t('schemaTree.compareSchema'),
      compareData: t('schemaTree.compareData'),
      backup: t('common.backupDatabase'),
      restore: t('common.restoreDatabase'),
    }),
    [t],
  );

  // ── Toggle helpers ──

  const collapseAll = useCallback(() => {
    setExpandedGroups(new Set());
    setExpandedConnections(new Set());
    setExpandedDbs(new Set());
    setExpandedSchemas(new Set());
    setExpandedCats(new Set());
  }, []);

  const toggleGroup = useCallback((group: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  }, []);

  const toggleConnection = useCallback(
    (connectionId: string) => {
      setExpandedConnections((prev) => {
        const next = new Set(prev);
        if (next.has(connectionId)) {
          next.delete(connectionId);
        } else {
          next.add(connectionId);
          onSelectConnection(connectionId);
        }
        return next;
      });
    },
    [onSelectConnection],
  );

  const toggleDb = useCallback(
    async (connectionId: string, dbSessionId: string, dbName: string) => {
      const dbKey = `${connectionId}::${dbName}`;
      const wasExpanded = expandedDbs.has(dbKey);

      setExpandedDbs((prev) => {
        const next = new Set(prev);
        if (next.has(dbKey)) next.delete(dbKey);
        else next.add(dbKey);
        return next;
      });

      if (wasExpanded) return;

      // Auto-expand tables category
      setExpandedCats((prev) => new Set(prev).add(`${dbKey}::tables`));

      const tableKey = `${dbSessionId}::${dbName}`;
      if (dbTablesMap[tableKey]) return;
      if (loadingDbs.has(tableKey)) return;

      setLoadingDbs((prev) => new Set(prev).add(tableKey));
      try {
        await reloadDbTables(dbSessionId, dbName);
      } catch {
        setDbTablesMap((prev) => ({ ...prev, [tableKey]: [] }));
      } finally {
        setLoadingDbs((prev) => {
          const next = new Set(prev);
          next.delete(tableKey);
          return next;
        });
      }
    },
    [expandedDbs, dbTablesMap, loadingDbs, reloadDbTables],
  );

  const toggleSchema = useCallback((schemaKey: string) => {
    setExpandedSchemas((prev) => {
      const next = new Set(prev);
      if (next.has(schemaKey)) next.delete(schemaKey);
      else next.add(schemaKey);
      return next;
    });
  }, []);

  const toggleCategory = useCallback(
    async (catKey: string, catId: string, dbSessionId: string) => {
      const wasExpanded = expandedCats.has(catKey);

      setExpandedCats((prev) => {
        const next = new Set(prev);
        if (next.has(catKey)) next.delete(catKey);
        else next.add(catKey);
        return next;
      });

      if (wasExpanded) return;
      if (catId === 'tables' || catId === 'views') return;
      if (dbObjectsMap[catKey]) return;

      try {
        const objs = await databaseCommands.getDatabaseObjects(dbSessionId, catId);
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: objs }));
      } catch {
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: [] }));
      }
    },
    [expandedCats, dbObjectsMap],
  );

  const reloadExpandedObjectCategories = useCallback(
    async (connectionId: string, dbSessionId: string) => {
      for (const catKey of expandedCats) {
        if (!catKey.startsWith(`${connectionId}::`)) continue;
        const catId = catKey.split('::').pop();
        if (!catId || catId === 'tables' || catId === 'views') continue;
        await reloadDbObjectCategory(dbSessionId, catKey, catId);
      }
    },
    [expandedCats, reloadDbObjectCategory],
  );

  const refreshConnection = useCallback(
    async (connectionId: string) => {
      const entry = activeConnections[connectionId];
      if (entry?.status !== 'connected' || !entry.dbSessionId) return;

      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) return;

      const meta = DB_REGISTRY[conn.databaseType];
      const isCustomTree = meta?.schemaTreeMode === 'custom';
      const isPathHierarchyOnly = meta?.namespaceEnsure === 'path-hierarchy' && !isCustomTree;
      const isPluginManaged = isCustomTree || isPathHierarchyOnly;
      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      await loadForConnection(entry.dbSessionId, {
        preferredDatabase: conn.database,
        skipLoadTables: isMultiDb || isPluginManaged,
        databaseType: conn.databaseType,
      });

      if (isPathHierarchyOnly) {
        await ensureNamespacePath([], entry.dbSessionId);
      }

      if (isMultiDb) {
        await Promise.all(
          [...expandedDbs]
            .filter((dbKey) => dbKey.startsWith(`${connectionId}::`))
            .map((dbKey) =>
              reloadDbTables(entry.dbSessionId, dbKey.slice(connectionId.length + 2)),
            ),
        );
      }

      await reloadExpandedObjectCategories(connectionId, entry.dbSessionId);
    },
    [
      activeConnections,
      connections,
      ensureNamespacePath,
      expandedDbs,
      loadForConnection,
      reloadDbTables,
      reloadExpandedObjectCategories,
    ],
  );

  const refreshAllConnections = useCallback(async () => {
    await Promise.all(
      Object.keys(activeConnections)
        .filter((connectionId) => activeConnections[connectionId]?.status === 'connected')
        .map((connectionId) => refreshConnection(connectionId)),
    );
  }, [activeConnections, refreshConnection]);

  const refreshDatabase = useCallback(
    async (connectionId: string, dbName: string) => {
      const entry = activeConnections[connectionId];
      if (!entry?.dbSessionId) return;

      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) return;

      const meta = DB_REGISTRY[conn.databaseType];
      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      if (isMultiDb) {
        await reloadDbTables(entry.dbSessionId, dbName);
      } else {
        await loadForConnection(entry.dbSessionId, {
          preferredDatabase: dbName,
          skipLoadTables: false,
          databaseType: conn.databaseType,
        });
      }

      const prefix = `${connectionId}::${dbName}::`;
      for (const catKey of expandedCats) {
        if (!catKey.startsWith(prefix)) continue;
        const catId = catKey.split('::').pop();
        if (!catId || catId === 'tables' || catId === 'views') continue;
        await reloadDbObjectCategory(entry.dbSessionId, catKey, catId);
      }
    },
    [
      activeConnections,
      connections,
      expandedCats,
      loadForConnection,
      reloadDbObjectCategory,
      reloadDbTables,
    ],
  );

  const refreshSchema = useCallback(
    async (connectionId: string, dbName: string, _schemaName: string) => {
      const entry = activeConnections[connectionId];
      if (!entry?.dbSessionId) return;

      await reloadDbTables(entry.dbSessionId, dbName);

      const prefix = `${connectionId}::${dbName}::${_schemaName}::`;
      for (const catKey of expandedCats) {
        if (!catKey.startsWith(prefix)) continue;
        const catId = catKey.split('::').pop();
        if (!catId || catId === 'tables' || catId === 'views') continue;
        await reloadDbObjectCategory(entry.dbSessionId, catKey, catId);
      }
    },
    [activeConnections, expandedCats, reloadDbObjectCategory, reloadDbTables],
  );

  useImperativeHandle(
    ref,
    () => ({
      refreshAllConnections,
      refreshConnection,
    }),
    [refreshAllConnections, refreshConnection],
  );

  // ── Context menus ──

  const handleGroupContextMenu = useCallback(
    (e: React.MouseEvent, groupName: string) => {
      e.preventDefault();
      e.stopPropagation();
      showWebContextMenu(
        buildMainGroupContextMenuItems({
          labels: contextLabels,
          isUngrouped: groupName === '',
          onNewGroup: () => {
            setNewGroupName('');
            setNewGroupDialogOpen(true);
          },
          onRenameGroup: () => {
            setRenamingGroup(groupName);
            setRenameValue(formatGroupLabel(groupName, t));
          },
          onDeleteGroup: () => {
            void (async () => {
              const ok = await confirmDeleteGroup({
                title: t('main.ctx.deleteGroup'),
                message: t('main.confirmDeleteGroup', { name: formatGroupLabel(groupName, t) }),
                confirmLabel: t('common.delete'),
                kind: 'warning',
              });
              if (ok) void deleteGroup(groupName);
            })();
          },
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [contextLabels, confirmDeleteGroup, deleteGroup, t],
  );

  const handleConnectionContextMenu = useCallback(
    (e: React.MouseEvent, conn: ConnectionConfig) => {
      e.preventDefault();
      e.stopPropagation();

      void (async () => {
        const isConnected = activeConnections[conn.id]?.status === 'connected';
        const dbMeta = DB_REGISTRY[conn.databaseType];
        const isMultiDb = shouldUseMultiDatabaseTree(dbMeta, conn.database);
        const moveTargets = groups
          .filter((g) => g !== conn.group)
          .map((g) => ({ id: g, label: formatGroupLabel(g, t) }));

        // Command discovery is best-effort: a failed probe must not block
        // the context menu — the menu only loses its driver-specific entries.
        const commands = await driverCommands
          .getDriverCommands(conn.databaseType)
          .catch(() => undefined);
        const supportsServerStatus =
          isConnected && hasCommand(commands, SERVER_STATUS_SNAPSHOT_COMMAND);
        const supportsProcessList = isConnected && hasCommand(commands, LIST_PROCESSES_COMMAND);
        const supportsBackup = dbMeta?.supportsBackup === true;

        showWebContextMenu(
          buildMainConnectionContextMenuItems({
            labels: contextLabels,
            isConnected,
            grouped: Boolean(conn.group),
            pinned: conn.pinned === true,
            moveTargets,
            onOpenOrDisconnect: () => {
              if (isConnected) {
                onDisconnect(conn.id);
              } else {
                onSelectConnection(conn.id);
                toggleConnection(conn.id);
                void connect(conn);
              }
            },
            onCopyName: () => {
              void navigator.clipboard.writeText(conn.name);
            },
            onCopyUrl: () => {
              const url = buildConnectionUrl(conn);
              if (url) void navigator.clipboard.writeText(url);
            },
            onNewQuery: () => {
              onSelectConnection(conn.id);
              viewActions?.newQuery?.();
            },
            onQueryHistory: () => {
              onSelectConnection(conn.id);
              viewActions?.openQueryHistory?.();
            },
            onExecuteSqlFile: undefined,
            onCreateDatabase:
              dbMeta?.supportsCreateDatabase && isMultiDb
                ? () => {
                    onSelectConnection(conn.id);
                    viewActions?.openCreateDatabase?.();
                  }
                : undefined,
            onCreateUser: dbMeta?.supportsCreateUser
              ? () => {
                  onSelectConnection(conn.id);
                  viewActions?.openCreateUser?.();
                }
              : undefined,
            onServerStatus: supportsServerStatus
              ? () => {
                  onSelectConnection(conn.id);
                  viewActions?.openServerStatus?.(buildOpenTarget(conn) ?? undefined);
                }
              : undefined,
            onPin: () => {
              void toggleConnectionPinned(conn.id);
            },
            onObjectFilter: () => {
              setObjectFilterConn(conn);
            },
            onProcessList: supportsProcessList
              ? () => {
                  onSelectConnection(conn.id);
                  viewActions?.openProcessList?.(buildOpenTarget(conn) ?? undefined);
                }
              : undefined,
            onBackup: supportsBackup
              ? () => {
                  openBackupWindow('backup', { connectionId: conn.id, database: conn.database });
                }
              : undefined,
            onRestore: supportsBackup
              ? () => {
                  openBackupWindow('restore', { connectionId: conn.id, database: conn.database });
                }
              : undefined,
            onRefresh: () => {
              if (activeConnections[conn.id]?.dbSessionId) {
                void refreshConnection(conn.id);
              }
            },
            onEdit: () => onEditConnection(conn.id),
            onDuplicate: () => {
              void duplicateConnection(conn.id);
            },
            onMoveToGroup: (groupId) => {
              void moveConnectionToGroup(conn.id, groupId);
            },
            onRemoveFromGroup: () => {
              void moveConnectionToGroup(conn.id, undefined);
            },
            onDelete: () => onDeleteConnection(conn.id),
          }),
          { x: e.clientX, y: e.clientY },
        );
      })();
    },
    [
      activeConnections,
      connect,
      contextLabels,
      duplicateConnection,
      groups,
      moveConnectionToGroup,
      onDeleteConnection,
      onDisconnect,
      onEditConnection,
      onSelectConnection,
      refreshConnection,
      t,
      toggleConnection,
      toggleConnectionPinned,
      viewActions,
    ],
  );

  // ── Database / Schema / Category / Object context menus ──

  const handleDatabaseContextMenu = useCallback(
    (e: React.MouseEvent, dbName: string, connectionId: string) => {
      e.preventDefault();
      e.stopPropagation();
      const entry = activeConnections[connectionId];
      const dbSessionId = entry?.dbSessionId;
      const conn = connections.find((c) => c.id === connectionId);
      const dbMeta = conn ? DB_REGISTRY[conn.databaseType] : undefined;
      const readOnly = conn?.readOnly === true || dbMeta?.readOnly === true;
      showWebContextMenu(
        buildSchemaTreeContextMenuItems({
          kind: 'database',
          labels: schemaLabels,
          handlers: {
            onRefresh: dbSessionId
              ? () => {
                  void refreshDatabase(connectionId, dbName);
                }
              : undefined,
            onNewQuery: () => {
              onSelectConnection(connectionId);
              useSchemaStore.setState({ currentDatabase: dbName });
              viewActions?.newQuery?.();
            },
            onQueryHistory: () => {
              onSelectConnection(connectionId);
              viewActions?.openQueryHistory?.();
            },
            onCopyDatabaseName: () => {
              void navigator.clipboard.writeText(dbName);
            },
            onViewErDiagram: () => {
              onSelectConnection(connectionId);
              viewActions?.openErDiagram?.();
            },
            onExecuteSqlFile:
              viewActions?.openSqlFile && !readOnly && !safeMode
                ? () => {
                    onSelectConnection(connectionId);
                    viewActions.openSqlFile!();
                  }
                : undefined,
            onNewTable: viewActions?.createTable
              ? () => {
                  onSelectConnection(connectionId);
                  viewActions.createTable!();
                }
              : undefined,
            onCreateSchema: dbMeta?.supportsCreateSchema
              ? () => {
                  onSelectConnection(connectionId);
                  viewActions?.openCreateSchema?.();
                }
              : undefined,
            onDropDatabase: dbSessionId
              ? () => {
                  void (async () => {
                    const ok = await confirmDropDatabase({
                      title: t('schemaTree.dropDatabase'),
                      message: t('schemaTree.confirmDropDatabase', { name: dbName }),
                      confirmLabel: t('schemaTree.dropDatabase'),
                      kind: 'warning',
                    });
                    if (!ok || !conn) return;
                    try {
                      const schemaData = useSchemaStore.getState().schemas.get(dbSessionId);
                      const activeDb = schemaData?.currentDatabase;
                      if (activeDb === dbName && schemaData) {
                        const fallback = resolveDropDatabaseFallback(
                          schemaData.databases,
                          dbName,
                          conn.database,
                        );
                        if (fallback) {
                          // F1: no use_database IPC — move the local active
                          // database to the fallback and refresh tables.
                          const cached = dbTablesMap[`${dbSessionId}::${fallback}`];
                          if (cached) {
                            useSchemaStore
                              .getState()
                              .setLoadedTables(fallback, cached, dbSessionId);
                          } else {
                            useSchemaStore.setState((state) => {
                              const entry = state.schemas.get(dbSessionId);
                              let schemas = state.schemas;
                              if (entry) {
                                schemas = new Map(state.schemas);
                                schemas.set(dbSessionId, { ...entry, currentDatabase: fallback });
                              }
                              return {
                                ...state,
                                schemas,
                                currentDatabase:
                                  state.currentDatabase === dbName
                                    ? fallback
                                    : state.currentDatabase,
                              };
                            });
                          }
                        }
                      }
                      await driverCommands.execute({
                        dbSessionId: dbSessionId,
                        command: 'drop_database',
                        input: { name: dbName },
                      });
                      clearDbLocalCache(connectionId, dbSessionId, dbName);
                      await loadForConnection(dbSessionId, {
                        databaseType: conn.databaseType,
                        skipLoadTables: true,
                      });
                    } catch (e) {
                      onShowMessage?.(
                        extractErrorMessage(e, t('schemaTree.dropDatabaseFailed')),
                        'error',
                      );
                    }
                  })();
                }
              : undefined,
            onDataTransfer: () => openDataTransferWindow(),
            onCompareSchema: () => openSchemaDiffWindow(),
            onCompareData: () => openDataSyncWindow(),
            onBackup: dbMeta?.supportsBackup
              ? () => {
                  openBackupWindow('backup', { connectionId, database: dbName });
                }
              : undefined,
            onRestore: dbMeta?.supportsBackup
              ? () => {
                  openBackupWindow('restore', { connectionId, database: dbName });
                }
              : undefined,
          },
          readOnly,
          safeMode,
          showNewTable: true,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      activeConnections,
      confirmDropDatabase,
      connections,
      dbTablesMap,
      clearDbLocalCache,
      loadForConnection,
      onSelectConnection,
      onShowMessage,
      refreshDatabase,
      schemaLabels,
      t,
      viewActions,
    ],
  );

  const handleSchemaContextMenu = useCallback(
    (e: React.MouseEvent, schemaName: string, dbName: string, connectionId: string) => {
      e.preventDefault();
      e.stopPropagation();
      const entry = activeConnections[connectionId];
      const dbSessionId = entry?.dbSessionId;
      const conn = connections.find((c) => c.id === connectionId);
      const dbMeta = conn ? DB_REGISTRY[conn.databaseType] : undefined;
      const readOnly = conn?.readOnly === true || dbMeta?.readOnly === true;
      showWebContextMenu(
        buildSchemaTreeContextMenuItems({
          kind: 'schema',
          labels: schemaLabels,
          handlers: {
            onRefresh: () => {
              void refreshSchema(connectionId, dbName, schemaName);
            },
            onNewQuery: () => {
              onSelectConnection(connectionId);
              useSchemaStore.setState({ currentDatabase: dbName });
              viewActions?.newQuery?.();
            },
            onQueryHistory: () => {
              onSelectConnection(connectionId);
              viewActions?.openQueryHistory?.();
            },
            onExecuteSqlFile:
              viewActions?.openSqlFile && !readOnly && !safeMode
                ? () => {
                    onSelectConnection(connectionId);
                    viewActions.openSqlFile!();
                  }
                : undefined,
            onNewTable: viewActions?.createTable
              ? () => {
                  onSelectConnection(connectionId);
                  viewActions.createTable!();
                }
              : undefined,
            onCopyName: () => {
              void navigator.clipboard.writeText(schemaName);
            },
            onViewErDiagram: () => {
              onSelectConnection(connectionId);
              viewActions?.openErDiagram?.();
            },
            onDropSchema:
              dbSessionId && !schemaName.startsWith('pg_') && schemaName !== 'information_schema'
                ? () => {
                    void (async () => {
                      const ok = await confirmDropSchema({
                        title: t('schemaTree.dropSchema'),
                        message: t('schemaTree.confirmDropSchema', { name: schemaName }),
                        confirmLabel: t('schemaTree.dropSchema'),
                        kind: 'warning',
                      });
                      if (!ok || !conn) return;
                      try {
                        await driverCommands.execute({
                          dbSessionId: dbSessionId,
                          command: 'drop_schema',
                          input: { name: schemaName, cascade: true },
                        });
                        await loadForConnection(dbSessionId, {
                          preferredDatabase: conn.database,
                          databaseType: conn.databaseType,
                          skipLoadTables: false,
                        });
                      } catch (e) {
                        onShowMessage?.(
                          extractErrorMessage(e, t('schemaTree.dropSchemaFailed')),
                          'error',
                        );
                      }
                    })();
                  }
                : undefined,
            onDataTransfer: () => openDataTransferWindow(),
            onCompareSchema: () => openSchemaDiffWindow(),
            onCompareData: () => openDataSyncWindow(),
          },
          readOnly,
          safeMode,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      activeConnections,
      confirmDropSchema,
      connections,
      loadForConnection,
      onSelectConnection,
      onShowMessage,
      refreshSchema,
      schemaLabels,
      t,
      viewActions,
    ],
  );

  const handleTableContextMenu = useCallback(
    (
      e: React.MouseEvent,
      args: {
        kind: 'table' | 'view';
        name: string;
        schema?: string;
        dbName: string;
        connectionId: string;
        dbSessionId: string;
      },
    ) => {
      e.preventDefault();
      e.stopPropagation();
      const { kind, name, schema, dbName, connectionId, dbSessionId } = args;
      const conn = connections.find((c) => c.id === connectionId);
      const dbMeta = conn ? DB_REGISTRY[conn.databaseType] : undefined;
      const readOnly = conn?.readOnly === true || dbMeta?.readOnly === true;
      const quoted = quoteRelationName(name, schema, conn?.databaseType ?? 'postgresql');
      const isView = kind === 'view';

      const refreshAfterMutation = () => {
        invalidateSchemaCache(dbSessionId, name);
        removeRelation(name, dbSessionId);
        const tableKey = `${dbSessionId}::${dbName}`;
        setDbTablesMap((prev) => {
          const current = prev[tableKey];
          if (!current) return prev;
          return {
            ...prev,
            [tableKey]: current.filter((item) => item.name !== name),
          };
        });
        void reloadDbTables(dbSessionId, dbName);
      };

      showWebContextMenu(
        buildSchemaTreeContextMenuItems({
          kind,
          labels: schemaLabels,
          handlers: {
            onOpen: () => {
              onSelectConnection(connectionId);
              void activateDatabase(dbSessionId, dbName);
              onSelectTable(name, schema, dbName);
            },
            onCopyName: () => {
              void navigator.clipboard.writeText(name);
            },
            onNewQuery: () => {
              onSelectConnection(connectionId);
              useSchemaStore.setState({ currentDatabase: dbName });
              if (kind === 'table') {
                viewActions?.newQuery?.(`SELECT * FROM ${quoted} LIMIT 100`);
              } else {
                viewActions?.newQuery?.();
              }
            },
            onTruncate:
              kind === 'table' && !readOnly && !safeMode
                ? () => {
                    void (async () => {
                      const ok = await confirmTruncateTable({
                        title: t('schemaTree.truncate'),
                        message: t('schemaTree.confirmTruncate', { name }),
                        confirmLabel: t('schemaTree.truncate'),
                        kind: 'warning',
                      });
                      if (!ok) return;
                      const dialect = getSqlDialect(conn?.databaseType ?? 'postgresql');
                      const sql = dialect?.getTruncateTableSql
                        ? dialect.getTruncateTableSql(quoted)
                        : `TRUNCATE TABLE ${quoted}`;
                      try {
                        await queryCommands.executeQuery(dbSessionId, sql);
                      } catch (err) {
                        onShowMessage?.(extractErrorMessage(err, t('schemaTree.truncateFailed')), 'error');
                      }
                    })();
                  }
                : undefined,
            onDrop:
              !readOnly && !safeMode
                ? () => {
                    void (async () => {
                      const ok = await confirmDropRelation({
                        title: t(isView ? 'schemaTree.dropView' : 'schemaTree.drop'),
                        message: t(
                          isView ? 'schemaTree.confirmDropView' : 'schemaTree.confirmDrop',
                          { name },
                        ),
                        confirmLabel: t(isView ? 'schemaTree.dropView' : 'schemaTree.drop'),
                        kind: 'warning',
                      });
                      if (!ok) return;
                      const sql = isView ? `DROP VIEW ${quoted}` : `DROP TABLE ${quoted}`;
                      try {
                        await queryCommands.executeQuery(dbSessionId, sql);
                        refreshAfterMutation();
                      } catch (err) {
                        onShowMessage?.(
                          extractErrorMessage(err, t('schemaTree.dropRelationFailed')),
                          'error',
                        );
                      }
                    })();
                  }
                : undefined,
          },
          readOnly,
          safeMode,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      activateDatabase,
      confirmDropRelation,
      confirmTruncateTable,
      connections,
      onSelectConnection,
      onSelectTable,
      onShowMessage,
      reloadDbTables,
      removeRelation,
      safeMode,
      schemaLabels,
      t,
      viewActions,
    ],
  );

  const handleCategoryContextMenu = useCallback(
    (e: React.MouseEvent, catKey: string, catId: string, connectionId: string) => {
      e.preventDefault();
      e.stopPropagation();
      showWebContextMenu(
        buildSchemaTreeContextMenuItems({
          kind: 'category',
          labels: schemaLabels,
          handlers: {
            onRefresh: () => {
              const entry = activeConnections[connectionId];
              if (!entry?.dbSessionId) return;
              const conn = connections.find((c) => c.id === connectionId);
              if (!conn) return;

              const parts = catKey.split('::');
              const dbName = parts[1];
              if (!dbName) return;

              if (catId === 'tables' || catId === 'views') {
                const meta = DB_REGISTRY[conn.databaseType];
                const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);
                if (isMultiDb) {
                  void reloadDbTables(entry.dbSessionId, dbName);
                } else {
                  void loadForConnection(entry.dbSessionId, {
                    preferredDatabase: dbName,
                    skipLoadTables: false,
                    databaseType: conn.databaseType,
                  });
                }
              } else {
                void reloadDbObjectCategory(entry.dbSessionId, catKey, catId);
              }
            },
          },
          categoryId: catId,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      activeConnections,
      connections,
      loadForConnection,
      reloadDbObjectCategory,
      reloadDbTables,
      schemaLabels,
    ],
  );

  const handleObjectContextMenu = useCallback(
    (e: React.MouseEvent, name: string) => {
      e.preventDefault();
      e.stopPropagation();
      showWebContextMenu(
        [
          {
            kind: 'item',
            id: 'copy-name',
            label: schemaLabels.copyName,
            action: () => void navigator.clipboard.writeText(name),
          },
        ],
        { x: e.clientX, y: e.clientY },
      );
    },
    [schemaLabels],
  );

  // ── Drag & Drop ──

  const dragConnId = useRef<string | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    id: string;
    position: 'before' | 'after';
  } | null>(null);
  const dropTargetRef = useRef(dropTarget);
  dropTargetRef.current = dropTarget;

  const handleDragStart = useCallback((e: React.DragEvent, connId: string) => {
    dragConnId.current = connId;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', connId);
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent, targetId: string) => {
    if (!dragConnId.current || dragConnId.current === targetId) {
      setDropTarget(null);
      return;
    }
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    const rect = e.currentTarget.getBoundingClientRect();
    const midY = rect.top + rect.height / 2;
    const position = e.clientY < midY ? 'before' : 'after';
    setDropTarget((prev) =>
      prev?.id === targetId && prev.position === position ? prev : { id: targetId, position },
    );
  }, []);

  const handleDragLeave = useCallback(() => {
    setDropTarget(null);
  }, []);

  const handleDragEnd = useCallback(() => {
    dragConnId.current = null;
    setDropTarget(null);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const sourceId = dragConnId.current;
      const target = dropTargetRef.current;
      if (!sourceId || !target) {
        handleDragEnd();
        return;
      }

      const ids = connections.map((c) => c.id);
      const fromIndex = ids.indexOf(sourceId);
      const toIndex = ids.indexOf(target.id);
      if (fromIndex === -1 || toIndex === -1 || fromIndex === toIndex) {
        handleDragEnd();
        return;
      }

      const reordered = [...ids];
      reordered.splice(fromIndex, 1);
      const insertAt =
        target.position === 'before'
          ? reordered.indexOf(target.id)
          : reordered.indexOf(target.id) + 1;
      reordered.splice(insertAt, 0, sourceId);

      void connectionCommands.reorderConnections(reordered).then(() => {
        useConnectionStore.getState().fetchConnections();
      });
      handleDragEnd();
    },
    [connections, handleDragEnd],
  );

  // ── Click handlers ──

  const handleConnectionClick = useCallback(
    (conn: ConnectionConfig) => {
      onSelectConnection(conn.id);
      const entry = activeConnections[conn.id];
      if (entry?.status === 'connected') {
        setExpandedConnections((prev) => {
          if (prev.has(conn.id)) return prev;
          return new Set(prev).add(conn.id);
        });
      }
    },
    [onSelectConnection, activeConnections],
  );

  const handleConnectionDoubleClick = useCallback(
    (conn: ConnectionConfig) => {
      onSelectConnection(conn.id);
      const status = activeConnections[conn.id]?.status ?? 'idle';
      if (status === 'connected') return;
      if (status !== 'connecting') {
        void connect(conn);
      }
    },
    [activeConnections, connect, onSelectConnection],
  );

  // ── Debounced search (100ms) ──

  const [debouncedSearch, setDebouncedSearch] = useState('');
  useEffect(() => {
    const raw = searchQuery.trim();
    if (!raw) {
      setDebouncedSearch('');
      return;
    }
    const timer = setTimeout(() => setDebouncedSearch(raw), 100);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  // ── Build unified flat rows ──

  const query = debouncedSearch.toLowerCase();

  const flatRows = useMemo<UnifiedRow[]>(() => {
    const rows: UnifiedRow[] = [];

    const addCategories = (
      allItems: TableInfo[],
      connectionId: string,
      _connectionId: string,
      dbName: string,
      schemaName: string | undefined,
      baseDepth: number,
      dbType: string,
      objectFilter: ObjectFilterPrefs,
    ) => {
      const filteredItems = filterTableItems(allItems, objectFilter);
      const realItems = filteredItems.filter((i) => i.name !== '');
      const tblItems = realItems.filter(
        (i) => i.tableType === 'table' || i.tableType === 'systemTable',
      );
      const viewItems = realItems.filter(
        (i) => i.tableType === 'view' || i.tableType === 'materializedView',
      );

      for (const cat of getCategoriesForDriver(dbType)) {
        const catKey = schemaName
          ? `${connectionId}::${dbName}::${schemaName}::${cat.id}`
          : `${connectionId}::${dbName}::${cat.id}`;
        const isExpanded = expandedCats.has(catKey);

        let count = 0;
        if (cat.id === 'tables') count = tblItems.length;
        else if (cat.id === 'views') count = viewItems.length;
        else count = (dbObjectsMap[catKey] ?? []).length;

        rows.push({
          type: 'category',
          key: catKey,
          cat,
          count,
          expanded: isExpanded,
          depth: baseDepth,
        });

        if (isExpanded) {
          let items: TableInfo[] = [];
          let objs: DatabaseObject[] = [];
          if (cat.id === 'tables') items = tblItems;
          else if (cat.id === 'views') items = viewItems;
          else objs = dbObjectsMap[catKey] ?? [];
          if (
            objectFilter.hideSystemSchemas ||
            objectFilter.tableNameInclude ||
            objectFilter.tableNameExclude
          ) {
            objs = objs.filter((o) => matchesTableNameFilter(o.name, objectFilter));
          }

          if (query) {
            items = items.filter((i) => i.name.toLowerCase().includes(query));
            objs = objs.filter((o) => o.name.toLowerCase().includes(query));
          }

          if (items.length > 0) {
            for (const item of items) {
              rows.push({
                type: 'table',
                item,
                depth: baseDepth + 1,
                catId: cat.id,
                isSelected: false,
                connectionId,
                dbSessionId: _connectionId,
                dbName,
              });
            }
          } else if (objs.length > 0) {
            for (const obj of objs) {
              rows.push({ type: 'object', obj, depth: baseDepth + 1, catId: cat.id });
            }
          }
        }
      }
    };

    if (grouped.length === 0) {
      rows.push({ type: 'no-connections' });
      return rows;
    }

    // Check if a connection has schema data matching the query
    const connectionMatchesQuery = (conn: ConnectionConfig): boolean => {
      if (!query) return true;
      const hay =
        `${conn.name} ${conn.host ?? ''} ${conn.database ?? ''} ${conn.databaseType}`.toLowerCase();
      if (hay.includes(query)) return true;

      const cEntry = activeConnections[conn.id];
      if (cEntry?.status !== 'connected' || !cEntry.dbSessionId) return false;
      const sd = schemas.get(cEntry.dbSessionId);
      if (!sd) return false;

      // Check database names
      if (sd.databases.some((d) => d.toLowerCase().includes(query))) return true;
      // Check table/view names
      if (sd.tables.some((tbl) => tbl.name.toLowerCase().includes(query))) return true;
      if (sd.views.some((v) => v.name.toLowerCase().includes(query))) return true;
      // Check schema names
      if ([...sd.tables, ...sd.views].some((i) => i.schema?.toLowerCase().includes(query)))
        return true;
      // Check multi-db cached tables
      for (const [key, items] of Object.entries(dbTablesMap)) {
        if (!key.startsWith(cEntry.dbSessionId + '::')) continue;
        if (items.some((i) => i.name.toLowerCase().includes(query))) return true;
      }
      // Check namespace tree keys (path-hierarchy / custom drivers)
      if (!isLeaf(sd.namespaceTree) && namespaceTreeContains(sd.namespaceTree, query)) return true;
      // Check cached path items
      for (const items of Object.values(sd.pathItems)) {
        if (items.some((i) => i.name.toLowerCase().includes(query))) return true;
      }
      return false;
    };

    for (const { group: groupName, connections: groupConns } of grouped) {
      const filteredConns = query ? groupConns.filter(connectionMatchesQuery) : groupConns;
      if (query && filteredConns.length === 0) continue;

      const isPinnedSection = groupName === PINNED_GROUP_KEY;
      const expanded = isPinnedSection || expandedGroups.has(groupName) || !!query;
      const displayName = groupName ? formatGroupLabel(groupName, t) : t('main.ungrouped');

      if (!isPinnedSection) {
        rows.push({
          type: 'group',
          groupName,
          displayName,
          count: filteredConns.length,
          expanded,
        });
      }

      if (!expanded) continue;

      if (filteredConns.length === 0) {
        rows.push({ type: 'empty-group' });
        continue;
      }

      for (const conn of filteredConns) {
        const objectFilter = getObjectFilter(conn);
        const entry = activeConnections[conn.id];
        const status = entry?.status ?? 'idle';
        const isConnected = status === 'connected';
        const isExpanded = expandedConnections.has(conn.id) || !!query;

        rows.push({
          type: 'connection',
          conn,
          isSelected: activeConnectionId === conn.id,
          status,
          expanded: (isExpanded && isConnected) || !!query,
          depth: 1,
        });

        if (!isConnected || (!isExpanded && !query)) continue;

        const dbSessionId = entry!.dbSessionId!;
        const schemaData = schemas.get(dbSessionId);
        if (!schemaData) {
          rows.push({ type: 'db-loading', depth: 2 });
          continue;
        }

        const meta = DB_REGISTRY[conn.databaseType];

        if (meta?.schemaTreeMode === 'custom' || meta?.namespaceEnsure === 'path-hierarchy') {
          const tree = schemaData.namespaceTree;
          const treeEmpty = isLeaf(tree) || Object.keys(tree).length === 0;
          if (treeEmpty) {
            if (!query && (schemaData.loading || schemaData.ensuringCount > 0)) {
              rows.push({ type: 'db-loading', depth: 2 });
            }
            continue;
          }
          const typeMap = new Map<string, TableInfo['tableType']>();
          for (const t of schemaData.tables) typeMap.set(t.name, t.tableType);
          flattenNamespaceTree(
            tree,
            conn.id,
            dbSessionId,
            2,
            rows,
            expandedDbs,
            query,
            typeMap,
            schemaData.loadedPaths,
          );
          continue;
        }

        // KV stores (Redis): databases as non-expandable leaf nodes
        if (meta?.isKeyValue) {
          const dbs = schemaData.databases;
          if (schemaData.loading && dbs.length === 0) {
            rows.push({ type: 'db-loading', depth: 2 });
          } else {
            const filteredDbs = query ? dbs.filter((d) => d.toLowerCase().includes(query)) : dbs;
            for (const dbName of filteredDbs) {
              rows.push({
                type: 'kv-db',
                connectionId: conn.id,
                dbSessionId,
                dbName,
                depth: 2,
                isSelected: false,
              });
            }
          }
          continue;
        }

        const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

        if (isMultiDb) {
          let dbs = query
            ? schemaData.databases.filter((d) => {
                if (d.toLowerCase().includes(query)) return true;
                const tblKey = `${dbSessionId}::${d}`;
                const tbls = dbTablesMap[tblKey];
                return tbls?.some((tbl) => tbl.name.toLowerCase().includes(query)) ?? false;
              })
            : schemaData.databases;
          dbs = dbs.filter((d) => shouldShowDatabase(d, objectFilter));

          for (const dbName of dbs) {
            const dbKey = `${conn.id}::${dbName}`;
            const tableKey = `${dbSessionId}::${dbName}`;
            const isDbExpanded = expandedDbs.has(dbKey) || !!query;
            const isLoading = loadingDbs.has(tableKey);

            rows.push({
              type: 'db',
              connectionId: conn.id,
              dbSessionId,
              dbName,
              expanded: isDbExpanded,
              loading: isLoading,
              depth: 2,
            });

            if (!isDbExpanded) continue;
            if (isLoading) {
              rows.push({ type: 'db-loading', depth: 3 });
              continue;
            }

            const allItems = dbTablesMap[tableKey] ?? [];
            const dbSchemaNames = [
              ...new Set(allItems.map((i) => i.schema).filter((s): s is string => !!s)),
            ];
            let schemaGroups = groupBySchema(allItems, dbSchemaNames);

            if (schemaGroups) {
              const schemaKeys = [...schemaGroups.keys()];
              if (schemaKeys.length === 1 && schemaKeys[0] === dbName) {
                schemaGroups = null;
              }
            }

            if (schemaGroups) {
              const preferred = DB_REGISTRY[conn.databaseType]?.defaultSchema;
              const sortedSchemas = [...schemaGroups.keys()].sort((a, b) => {
                if (preferred) {
                  if (a === preferred) return -1;
                  if (b === preferred) return 1;
                }
                return a.localeCompare(b);
              });

              for (const schemaName of sortedSchemas) {
                if (!shouldShowSchema(schemaName, objectFilter)) continue;
                const schemaKey = `${conn.id}::${dbName}::${schemaName}`;
                const schemaItems = schemaGroups.get(schemaName) ?? [];
                const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

                rows.push({
                  type: 'schema',
                  connectionId: conn.id,
                  dbName,
                  schemaName,
                  expanded: schemaExpanded,
                  depth: 3,
                });

                if (schemaExpanded) {
                  addCategories(
                    schemaItems,
                    conn.id,
                    dbSessionId,
                    dbName,
                    schemaName,
                    4,
                    conn.databaseType,
                    objectFilter,
                  );
                }
              }
            } else {
              addCategories(
                allItems,
                conn.id,
                dbSessionId,
                dbName,
                undefined,
                3,
                conn.databaseType,
                objectFilter,
              );
            }
          }
        } else {
          // Standard (single-db) schema tree
          const dbName = schemaData.currentDatabase ?? conn.database ?? '';
          if (!dbName) continue;

          const dbKey = `${conn.id}::${dbName}`;
          const isDbExpanded = expandedDbs.has(dbKey);

          rows.push({
            type: 'db',
            connectionId: conn.id,
            dbSessionId,
            dbName,
            expanded: isDbExpanded,
            loading: schemaData.loading && schemaData.tables.length === 0,
            depth: 2,
          });

          if (!isDbExpanded) continue;

          if (schemaData.loading && schemaData.tables.length === 0) {
            rows.push({ type: 'db-loading', depth: 3 });
            continue;
          }

          const allItems = [...schemaData.tables, ...schemaData.views];
          const schemaGroups = groupBySchema(allItems, schemaData.schemaNames);

          if (schemaGroups) {
            const preferred = DB_REGISTRY[conn.databaseType]?.defaultSchema;
            const sortedSchemas = [...schemaGroups.keys()].sort((a, b) => {
              if (preferred) {
                if (a === preferred) return -1;
                if (b === preferred) return 1;
              }
              return a.localeCompare(b);
            });

            for (const schemaName of sortedSchemas) {
              if (!shouldShowSchema(schemaName, objectFilter)) continue;
              const schemaKey = `${conn.id}::${dbName}::${schemaName}`;
              const schemaItems = schemaGroups.get(schemaName) ?? [];
              const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

              rows.push({
                type: 'schema',
                connectionId: conn.id,
                dbName,
                schemaName,
                expanded: schemaExpanded,
                depth: 3,
              });

              if (schemaExpanded) {
                addCategories(
                  schemaItems,
                  conn.id,
                  dbSessionId,
                  dbName,
                  schemaName,
                  4,
                  conn.databaseType,
                  objectFilter,
                );
              }
            }
          } else {
            addCategories(
              allItems,
              conn.id,
              dbSessionId,
              dbName,
              undefined,
              3,
              conn.databaseType,
              objectFilter,
            );
          }
        }
      }
    }

    return rows;
  }, [
    grouped,
    expandedGroups,
    expandedConnections,
    expandedDbs,
    expandedSchemas,
    expandedCats,
    activeConnections,
    activeConnectionId,
    schemas,
    dbTablesMap,
    dbObjectsMap,
    loadingDbs,
    query,
    t,
  ]);

  // ── Virtualizer ──

  const virtualizer = useVirtualizer({
    count: flatRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 25,
  });

  // ── Render helpers ──

  const renderStatusDot = (connectionId: string) => {
    const status = activeConnections[connectionId]?.status ?? 'idle';
    if (status === 'connecting') {
      return (
        <span
          className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-yellow-400"
          title={t('conn.connecting')}
        />
      );
    }
    if (status === 'connected') {
      return (
        <span className="h-2 w-2 shrink-0 rounded-full bg-green-500" title={t('conn.connected')} />
      );
    }
    if (status === 'error') {
      return <span className="h-2 w-2 shrink-0 rounded-full bg-red-500" title={t('conn.failed')} />;
    }
    return null;
  };

  const renderRow = (row: UnifiedRow) => {
    switch (row.type) {
      case 'group':
        return (
          <div
            data-group-header
            className="flex cursor-pointer items-center gap-1.5 px-2 py-1 hover:bg-surface-raised/50"
            onClick={() => toggleGroup(row.groupName)}
            onContextMenu={(e) => handleGroupContextMenu(e, row.groupName)}
          >
            {row.expanded ? (
              <ChevronDown className="h-3 w-3 shrink-0 text-fg-muted" />
            ) : (
              <ChevronRight className="h-3 w-3 shrink-0 text-fg-muted" />
            )}
            {row.expanded ? (
              <FolderOpen className="h-3.5 w-3.5 shrink-0 text-amber-400" />
            ) : (
              <FolderClosed className="h-3.5 w-3.5 shrink-0 text-amber-400" />
            )}
            <span className="text-[13px] font-semibold text-fg">{row.displayName}</span>
            <span className="text-[11px] text-fg-muted">({row.count})</span>
          </div>
        );

      case 'connection': {
        const showDropBefore = dropTarget?.id === row.conn.id && dropTarget.position === 'before';
        const showDropAfter = dropTarget?.id === row.conn.id && dropTarget.position === 'after';

        return (
          <div>
            {showDropBefore && <div className="mx-2 h-[2px] rounded-full bg-accent" />}
            <div
              data-conn-item
              data-conn-name={row.conn.name}
              draggable
              onDragStart={(e) => handleDragStart(e, row.conn.id)}
              onDragOver={(e) => handleDragOver(e, row.conn.id)}
              onDragLeave={handleDragLeave}
              onDragEnd={handleDragEnd}
              onDrop={handleDrop}
              className={cn(
                'group relative flex cursor-default items-center gap-1.5 py-1 pr-2 text-[13px] transition-colors',
                row.isSelected
                  ? 'bg-accent/10 text-fg'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
              style={{ paddingLeft: depthPadding(row.depth) }}
              onClick={() => handleConnectionClick(row.conn)}
              onDoubleClick={() => handleConnectionDoubleClick(row.conn)}
              onContextMenu={(e) => handleConnectionContextMenu(e, row.conn)}
            >
              {row.isSelected && <span className="absolute inset-y-0 left-0 w-0.5 bg-accent" />}
              <button
                type="button"
                className="flex h-4 w-4 shrink-0 items-center justify-center rounded text-fg-muted hover:text-fg"
                onClick={(e) => {
                  e.stopPropagation();
                  if (row.status === 'connected') {
                    toggleConnection(row.conn.id);
                  } else {
                    handleConnectionDoubleClick(row.conn);
                  }
                }}
                onDoubleClick={(e) => e.stopPropagation()}
                aria-expanded={row.status === 'connected' ? row.expanded : undefined}
              >
                {row.status === 'connected' && row.expanded ? (
                  <ChevronDown className="h-3 w-3" />
                ) : (
                  <ChevronRight className="h-3 w-3" />
                )}
              </button>
              <DbTypeBadge databaseType={row.conn.databaseType} size={18} className="shrink-0" />
              <span className="min-w-0 flex-1 truncate font-medium">{row.conn.name}</span>
              {renderStatusDot(row.conn.id)}
            </div>
            {showDropAfter && <div className="mx-2 h-[2px] rounded-full bg-accent" />}
          </div>
        );
      }

      case 'db':
        return (
          <button
            type="button"
            data-tree-node="db"
            data-db-name={row.dbName}
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => void toggleDb(row.connectionId, row.dbSessionId, row.dbName)}
            onContextMenu={(e) => handleDatabaseContextMenu(e, row.dbName, row.connectionId)}
          >
            {row.expanded ? (
              <ChevronDown className="h-3 w-3 shrink-0" />
            ) : (
              <ChevronRight className="h-3 w-3 shrink-0" />
            )}
            <Database className="h-3.5 w-3.5 shrink-0 text-teal-400" />
            <span className="selectable min-w-0 truncate">{row.dbName}</span>
            {row.loading && <Loader2 className="h-3 w-3 shrink-0 animate-spin text-fg-muted" />}
          </button>
        );

      case 'schema':
        return (
          <button
            type="button"
            data-tree-node="schema"
            data-schema-name={row.schemaName}
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => toggleSchema(`${row.connectionId}::${row.dbName}::${row.schemaName}`)}
            onContextMenu={(e) =>
              handleSchemaContextMenu(e, row.schemaName, row.dbName, row.connectionId)
            }
          >
            {row.expanded ? (
              <ChevronDown className="h-3 w-3 shrink-0" />
            ) : (
              <ChevronRight className="h-3 w-3 shrink-0" />
            )}
            {row.expanded ? (
              <FolderOpen className="h-3.5 w-3.5 shrink-0 text-teal-400" />
            ) : (
              <FolderClosed className="h-3.5 w-3.5 shrink-0 text-teal-400" />
            )}
            <span className="min-w-0 truncate">{row.schemaName || t('common.default')}</span>
          </button>
        );

      case 'category': {
        const catConnectionId = row.key.split('::')[0];
        return (
          <button
            type="button"
            data-tree-node="category"
            data-cat-id={row.cat.id}
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => {
              const conn = connections.find((c) => c.id === catConnectionId);
              const dbSessionId =
                conn && activeConnections[conn.id]?.dbSessionId
                  ? activeConnections[conn.id].dbSessionId!
                  : '';
              void toggleCategory(row.key, row.cat.id, dbSessionId);
            }}
            onContextMenu={(e) =>
              handleCategoryContextMenu(e, row.key, row.cat.id, catConnectionId)
            }
          >
            {row.expanded ? (
              <ChevronDown className="h-3 w-3 shrink-0" />
            ) : (
              <ChevronRight className="h-3 w-3 shrink-0" />
            )}
            <row.cat.icon className={`h-3.5 w-3.5 shrink-0 ${row.cat.color}`} />
            <span className="min-w-0 truncate">
              {t(row.cat.labelKey as Parameters<typeof t>[0])}
            </span>
            <span className="ml-auto shrink-0 text-[10px] text-fg-muted">{row.count}</span>
          </button>
        );
      }

      case 'table': {
        const iconColor = row.catId === 'views' ? 'text-purple-400' : 'text-blue-400';
        const Icon = row.catId === 'views' ? Eye : Table2;
        return (
          <button
            type="button"
            data-tree-node={row.catId === 'views' ? 'view' : 'table'}
            data-item-name={row.item.name}
            className={cn(
              'flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised',
              row.isSelected ? 'bg-surface-raised text-fg' : 'text-fg-secondary',
            )}
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => {
              void (async () => {
                onSelectConnection(row.connectionId);
                await activateDatabase(row.dbSessionId, row.dbName);
                onSelectTable(row.item.name, row.item.schema ?? undefined, row.dbName);
              })();
            }}
            onContextMenu={(e) => {
              handleTableContextMenu(e, {
                kind: row.catId === 'views' ? 'view' : 'table',
                name: row.item.name,
                schema: row.item.schema ?? undefined,
                dbName: row.dbName,
                connectionId: row.connectionId,
                dbSessionId: row.dbSessionId,
              });
            }}
          >
            <Icon className={`h-3.5 w-3.5 shrink-0 ${iconColor}`} />
            <span className="selectable min-w-0 truncate">{row.item.name}</span>
          </button>
        );
      }

      case 'object': {
        const objColor =
          row.catId === 'procedure'
            ? 'text-emerald-400'
            : row.catId === 'trigger'
              ? 'text-amber-400'
              : 'text-orange-400';
        const ObjIcon = row.catId === 'trigger' ? Zap : Braces;
        return (
          <button
            type="button"
            data-tree-node={row.catId}
            data-item-name={row.obj.name}
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => {
              const kind = row.obj.kind ?? row.catId;
              if (kind === 'function' || kind === 'procedure' || kind === 'trigger') {
                viewActions?.openObject?.(kind, row.obj.name, row.obj.schema ?? undefined);
              }
            }}
            onContextMenu={(e) => handleObjectContextMenu(e, row.obj.name)}
          >
            <ObjIcon className={`h-3.5 w-3.5 shrink-0 ${objColor}`} />
            <span className="min-w-0 truncate">{row.obj.name}</span>
          </button>
        );
      }

      case 'kv-db':
        return (
          <button
            type="button"
            data-tree-node="kv-db"
            data-db-name={row.dbName}
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] hover:bg-surface-raised text-fg-secondary"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => {
              if (onSelectKvDb) {
                onSelectKvDb(row.connectionId, row.dbName);
              } else {
                onSelectConnection(row.connectionId);
                onSelectTable(row.dbName);
              }
            }}
          >
            <Database className="h-3.5 w-3.5 shrink-0 text-teal-400" />
            <span className="selectable min-w-0 truncate">{row.dbName}</span>
          </button>
        );

      case 'db-loading':
        return (
          <div
            className="flex items-center gap-2 py-1 text-xs text-fg-muted"
            style={{ paddingLeft: depthPadding(row.depth) }}
          >
            <Loader2 className="h-3 w-3 animate-spin" />
            {t('common.loading')}
          </div>
        );

      case 'namespace-node': {
        if (row.isLeaf) {
          const leafIcon = LEAF_KIND_ICON[row.leafKind ?? 'table'];
          const LeafIcon = leafIcon.icon;
          const menuKind =
            row.leafKind === 'view' || row.leafKind === 'materializedView'
              ? 'view'
              : row.leafKind === 'function' ||
                  row.leafKind === 'procedure' ||
                  row.leafKind === 'trigger'
                ? row.leafKind
                : 'table';
          return (
            <button
              type="button"
              data-tree-node={menuKind}
              data-item-name={row.name}
              className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
              style={{ paddingLeft: depthPadding(row.depth) }}
              onClick={() => onSelectTable(row.name)}
              onContextMenu={(e) => {
                if (
                  row.leafKind === 'function' ||
                  row.leafKind === 'procedure' ||
                  row.leafKind === 'trigger'
                ) {
                  handleObjectContextMenu(e, row.name);
                  return;
                }
                const conn = connections.find((c) => c.id === row.connectionId);
                const dbName =
                  conn?.database ??
                  useSchemaStore.getState().schemas.get(row.dbSessionId)?.currentDatabase ??
                  '';
                const relationKind =
                  row.leafKind === 'view' || row.leafKind === 'materializedView' ? 'view' : 'table';
                handleTableContextMenu(e, {
                  kind: relationKind,
                  name: row.name,
                  dbName,
                  connectionId: row.connectionId,
                  dbSessionId: row.dbSessionId,
                });
              }}
            >
              <LeafIcon className={`h-3.5 w-3.5 shrink-0 ${leafIcon.color}`} />
              <span className="selectable min-w-0 truncate">{row.name}</span>
            </button>
          );
        }
        return (
          <button
            type="button"
            data-tree-node="namespace"
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => {
              const willExpand = !expandedDbs.has(row.key);
              setExpandedDbs((prev) => {
                const next = new Set(prev);
                if (next.has(row.key)) next.delete(row.key);
                else next.add(row.key);
                return next;
              });
              if (willExpand) {
                void ensureNamespacePath(row.segments, row.dbSessionId);
              }
            }}
          >
            {row.expanded ? (
              <ChevronDown className="h-3 w-3 shrink-0" />
            ) : (
              <ChevronRight className="h-3 w-3 shrink-0" />
            )}
            <FolderOpen className="h-3.5 w-3.5 shrink-0 text-amber-400" />
            <span className="min-w-0 truncate">{row.name}</span>
          </button>
        );
      }

      case 'empty-group':
        return (
          <div className="px-4 py-1.5 text-[11px] text-fg-muted">{t('main.noConnections')}</div>
        );

      case 'no-connections':
        return (
          <div className="flex flex-col items-center justify-center px-4 py-12 text-center">
            <p className="text-[13px] text-fg-muted">{t('main.noConnections')}</p>
            <button
              type="button"
              className="mt-3 inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 text-[13px] text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onNewConnection}
            >
              <Plus className="h-4 w-4" />
              {t('main.createFirst')}
            </button>
          </div>
        );
    }
  };

  // ── Main render ──

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col bg-surface">
      {/* Toolbar */}
      <div className="flex h-12 min-h-[48px] shrink-0 items-center justify-between border-b border-edge px-2">
        <span className="text-[13px] font-semibold text-fg">{t('nav.connections')}</span>
        <div className="flex items-center gap-0.5">
          {onExportConnections && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onExportConnections}
              title={t('common.exportConnections')}
            >
              <Upload className="h-3.5 w-3.5" />
            </button>
          )}
          {onImportConnections && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onImportConnections}
              title={t('common.importConnections')}
            >
              <Download className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={onNewConnection}
            title={t('common.newConnection')}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={() => {
              setNewGroupName('');
              setNewGroupDialogOpen(true);
            }}
            title={t('common.newGroup')}
          >
            <FolderPlus className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={collapseAll}
            title={t('connWin.collapseAll')}
          >
            <ChevronsDownUp className="h-3.5 w-3.5" />
          </button>
          {onRefresh && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onRefresh}
              title={t('connWin.refresh')}
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          )}
          {onCollapseSidebar && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onCollapseSidebar}
              title={t('connWin.collapseSidebar')}
            >
              <PanelLeftClose className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* Search */}
      <div className="flex shrink-0 items-center border-b border-edge px-2 py-1.5">
        <div className="relative flex-1">
          <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
          <input
            type="text"
            className="h-7 w-full rounded-md bg-surface pl-7 pr-2 text-xs text-fg placeholder:text-fg-muted focus:outline-none focus:ring-1 focus:ring-accent"
            placeholder={t('main.searchPlaceholder')}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
      </div>

      {/* Unified virtual list */}
      <div ref={scrollRef} className="flex-1 min-h-0 overflow-y-auto py-1">
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((virtualRow) => (
            <div
              key={virtualRow.index}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: virtualRow.size,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              {renderRow(flatRows[virtualRow.index])}
            </div>
          ))}
        </div>
      </div>

      {/* New group dialog */}
      <Dialog
        open={newGroupDialogOpen}
        title={t('common.newGroup')}
        onClose={() => setNewGroupDialogOpen(false)}
        className="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setNewGroupDialogOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                if (newGroupName.trim()) {
                  void addGroup(newGroupName.trim());
                }
                setNewGroupDialogOpen(false);
              }}
            >
              {t('common.ok')}
            </Button>
          </>
        }
      >
        <Input
          autoFocus
          value={newGroupName}
          onChange={(e) => setNewGroupName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              if (newGroupName.trim()) void addGroup(newGroupName.trim());
              setNewGroupDialogOpen(false);
            }
          }}
          placeholder={t('main.groupNamePlaceholder')}
          className="text-sm"
          autoCapitalize="off"
        />
      </Dialog>

      {/* Rename group dialog */}
      <Dialog
        open={renamingGroup !== null}
        title={t('main.ctx.renameGroup')}
        onClose={() => setRenamingGroup(null)}
        className="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setRenamingGroup(null)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                if (renamingGroup !== null && renameValue.trim()) {
                  void renameGroup(renamingGroup, renameValue.trim());
                }
                setRenamingGroup(null);
              }}
            >
              {t('common.ok')}
            </Button>
          </>
        }
      >
        <Input
          autoFocus
          value={renameValue}
          onChange={(e) => setRenameValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              if (renamingGroup !== null && renameValue.trim()) {
                void renameGroup(renamingGroup, renameValue.trim());
              }
              setRenamingGroup(null);
            }
          }}
          placeholder={t('main.groupNamePlaceholder')}
          className="text-sm"
          autoCapitalize="off"
        />
      </Dialog>

      <ObjectFilterDialog
        open={objectFilterConn != null}
        connection={objectFilterConn}
        onClose={() => setObjectFilterConn(null)}
        onSave={async (config) => {
          await saveConnection(config);
          await refreshConnection(config.id);
        }}
      />

      {confirmDeleteGroupDialog}
      {confirmDropDatabaseDialog}
      {confirmDropSchemaDialog}
      {confirmDropRelationDialog}
      {confirmTruncateTableDialog}
    </div>
  );
});
