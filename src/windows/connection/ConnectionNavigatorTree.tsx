import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { isLeaf, pathKey, type SqlNamespace } from '../../lib/sqlNamespace';
import {
  buildMainConnectionContextMenuItems,
  buildMainGroupContextMenuItems,
} from '../../lib/mainWindowContextMenu';
import { buildSchemaTreeContextMenuItems } from '../../lib/schemaTreeContextMenu';
import { buildConnectionUrl } from '../../lib/buildConnectionUrl';
import { groupConnections, useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { showWebContextMenu } from '../../stores/contextMenuStore';
import { shouldUseMultiDatabaseTree } from './schema-tree/SchemaTree';
import { connectionCommands } from '../../commands/connection';
import { driverCommands } from '../../commands/driver';
import { openDataSyncWindow, openSchemaDiffWindow } from '../../lib/windowManager';
import type { ConnectionConfig, DatabaseObject, TableInfo } from '../../types';

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
      configId: string;
      connectionId: string;
      dbName: string;
      expanded: boolean;
      loading: boolean;
      depth: number;
    }
  | {
      type: 'schema';
      configId: string;
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
      configId: string;
    }
  | { type: 'object'; obj: DatabaseObject; depth: number; catId: string }
  | {
      type: 'kv-db';
      configId: string;
      connectionId: string;
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
      configId: string;
      connectionId: string;
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
  configId: string,
  connectionId: string,
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
    const nodeKey = `${configId}::ns::${segments.join('/')}`;
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
        configId,
        connectionId,
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
        configId,
        connectionId,
      });
      if (expanded) {
        const childEntries = Object.entries(child);
        const pathLoaded = loadedPaths.has(pathKey(segments));
        if (childEntries.length === 0 && !pathLoaded && !query) {
          rows.push({ type: 'db-loading', depth: baseDepth + 1 });
        } else {
          flattenNamespaceTree(
            child,
            configId,
            connectionId,
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

// ── Props ───────────────────────────────────────────────────────

export interface ConnectionNavigatorTreeProps {
  onSelectConnection: (configId: string) => void;
  onSelectTable: (tableName: string, schema?: string) => void;
  onSelectKvDb?: (configId: string, dbName: string) => void;
  activeConfigId: string | null;
  onNewConnection: () => void;
  onRefresh?: () => void;
  onEditConnection: (configId: string) => void;
  onDeleteConnection: (configId: string) => void;
  onDisconnect: (configId: string) => void;
  onExportConnections?: () => void;
  onImportConnections?: () => void;
  onCollapseSidebar?: () => void;
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
  };
}

// ── Component ───────────────────────────────────────────────────

export function ConnectionNavigatorTree({
  onSelectConnection,
  onSelectTable,
  onSelectKvDb,
  activeConfigId,
  onNewConnection,
  onRefresh,
  onEditConnection,
  onDeleteConnection,
  onDisconnect,
  onExportConnections,
  onImportConnections,
  onCollapseSidebar,
  onNodeContextMenu,
  viewActions,
}: ConnectionNavigatorTreeProps) {
  const { t } = useI18n();
  const [searchQuery, setSearchQuery] = useState('');
  const connections = useConnectionStore((s) => s.connections);
  const groups = useConnectionStore((s) => s.groups);
  const duplicateConnection = useConnectionStore((s) => s.duplicateConnection);
  const addGroup = useConnectionStore((s) => s.addGroup);
  const deleteGroup = useConnectionStore((s) => s.deleteGroup);
  const renameGroup = useConnectionStore((s) => s.renameGroup);
  const moveConnectionToGroup = useConnectionStore((s) => s.moveConnectionToGroup);
  const activeConnections = useActiveConnectionStore((s) => s.connections);
  const connect = useActiveConnectionStore((s) => s.connect);

  // ── Group dialog state ──
  const [newGroupDialogOpen, setNewGroupDialogOpen] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');
  const [renamingGroup, setRenamingGroup] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [confirmDeleteGroup, confirmDeleteGroupDialog] = useConfirmDialog();
  const [confirmDropDatabase, confirmDropDatabaseDialog] = useConfirmDialog();
  const [confirmDropSchema, confirmDropSchemaDialog] = useConfirmDialog();
  const schemas = useSchemaStore((s) => s.schemas);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const ensureNamespacePath = useSchemaStore((s) => s.ensureNamespacePath);

  const scrollRef = useRef<HTMLDivElement>(null);

  // Always get all connections grouped; deep search filtering happens in flatRows
  const grouped = useMemo(() => groupConnections(connections, groups, ''), [connections, groups]);

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

  const reloadDbTables = useCallback(async (connectionId: string, dbName: string) => {
    const tableKey = `${connectionId}::${dbName}`;
    try {
      const { databaseCommands } = await import('../../commands/database');
      await databaseCommands.useDatabase(connectionId, dbName);
      const all = await databaseCommands.getTables(connectionId, dbName);
      setDbTablesMap((prev) => ({ ...prev, [tableKey]: all }));
      useSchemaStore.getState().setLoadedTables(dbName, all, connectionId);
    } catch {
      // ignore
    }
  }, []);

  // Invalidate + reload dbTablesMap when databases list or schemaEpoch changes
  const prevFpRef = useRef<Map<string, string>>(new Map());
  useEffect(() => {
    const nextFp = new Map<string, string>();
    for (const [, cEntry] of Object.entries(activeConnections)) {
      if (!cEntry.connectionId) continue;
      const sd = schemas.get(cEntry.connectionId);
      if (sd) {
        nextFp.set(cEntry.connectionId, `${sd.databases.join('\0')}|${sd.schemaEpoch}`);
      }
    }
    const prev = prevFpRef.current;
    prevFpRef.current = nextFp;
    if (prev.size === 0) return;
    for (const [connId, fp] of nextFp) {
      if (prev.get(connId) !== fp) {
        // Find configId for this connectionId
        const configId = Object.entries(activeConnections).find(
          ([, e]) => e.connectionId === connId,
        )?.[0];
        // Invalidate and auto-reload expanded databases
        setDbTablesMap((m) => {
          const next: Record<string, TableInfo[]> = {};
          for (const key of Object.keys(m)) {
            if (!key.startsWith(connId + '::')) next[key] = m[key];
          }
          return next;
        });
        setDbObjectsMap((m) => {
          const next: Record<string, DatabaseObject[]> = {};
          for (const key of Object.keys(m)) {
            if (!key.startsWith(connId + '::')) next[key] = m[key];
          }
          return next;
        });
        if (configId) {
          for (const dbKey of expandedDbs) {
            if (dbKey.startsWith(configId + '::')) {
              const dbName = dbKey.slice(configId.length + 2);
              void reloadDbTables(connId, dbName);
            }
          }
        }
      }
    }
  }, [activeConnections, schemas, expandedDbs, reloadDbTables]);

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

  // Auto-expand newly connected connections
  useEffect(() => {
    if (!activeConfigId) return;
    if (activeConnections[activeConfigId]?.status === 'connected') {
      setExpandedConnections((prev) => new Set(prev).add(activeConfigId));
    }
  }, [activeConfigId, activeConnections]);

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
    for (const configId of expandedConnections) {
      const entry = activeConnections[configId];
      if (entry?.status !== 'connected' || !entry.connectionId) continue;
      if (loadedConnectionsRef.current.has(configId)) continue;
      loadedConnectionsRef.current.add(configId);

      const conn = connections.find((c) => c.id === configId);
      if (!conn) continue;

      const meta = DB_REGISTRY[conn.databaseType];
      const isCustomTree = meta?.schemaTreeMode === 'custom';
      const isPathHierarchyOnly = meta?.namespaceEnsure === 'path-hierarchy' && !isCustomTree;
      const isPluginManaged = isCustomTree || isPathHierarchyOnly;
      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      void loadForConnection(entry.connectionId, {
        preferredDatabase: conn.database,
        skipLoadTables: isMultiDb || isPluginManaged,
        databaseType: conn.databaseType,
      }).then(() => {
        if (isPathHierarchyOnly) {
          void ensureNamespacePath([], entry.connectionId);
        }
      });

      // Auto-expand default database for standard schema trees
      if (!isMultiDb && !isPluginManaged && conn.database) {
        const dbKey = `${configId}::${conn.database}`;
        setExpandedDbs((prev) => new Set(prev).add(dbKey));
        setExpandedCats((prev) => new Set(prev).add(`${dbKey}::tables`));
      }
    }
  }, [expandedConnections, activeConnections, connections, loadForConnection, ensureNamespacePath]);

  // ── Context menu labels ──

  const contextLabels = useMemo(
    () => ({
      newGroup: t('main.ctx.newGroup'),
      newConnection: t('main.newConnection'),
      renameGroup: t('main.ctx.renameGroup'),
      deleteGroup: t('main.ctx.deleteGroup'),
      openConnection: t('main.ctx.openConnection'),
      disconnect: t('main.ctx.disconnect'),
      editConnection: t('main.ctx.editConnection'),
      duplicateConnection: t('main.ctx.duplicateConnection'),
      moveToGroup: t('main.ctx.moveToGroup'),
      removeFromGroup: t('main.ctx.removeFromGroup'),
      deleteConnection: t('main.ctx.deleteConnection'),
      copyName: t('main.ctx.copyName'),
      copyConnectionUrl: t('main.ctx.copyConnectionUrl'),
      newQuery: t('main.ctx.newQuery'),
      executeSqlFile: t('main.ctx.executeSqlFile'),
      createDatabase: t('createDb.create'),
      createSchema: t('createSchema.create'),
      createUser: t('createUser.create'),
      refresh: t('main.ctx.refresh'),
    }),
    [t],
  );

  const schemaLabels = useMemo(
    () => ({
      open: t('schemaTree.openTable'),
      openStructure: t('schemaTree.openStructure'),
      copyName: t('schemaTree.copyName'),
      copyDdl: t('connWin.copyDDL'),
      focusEr: '',
      exportData: t('connWin.exportData'),
      importData: t('connWin.importData'),
      refresh: t('connWin.refresh'),
      newQuery: t('connWin.newQuery'),
      copyDatabaseName: t('schemaTree.copyDatabaseName'),
      newTable: t('connWin.newTable'),
      batchExport: `${t('batchExport.title')}…`,
      truncate: t('schemaTree.truncate'),
      drop: t('schemaTree.drop'),
      dropView: t('schemaTree.dropView'),
      dropDatabase: t('schemaTree.dropDatabase'),
      viewErDiagram: t('schemaTree.viewErDiagram'),
      newSchema: t('schemaTree.newSchema'),
      createSchema: t('createSchema.create'),
      dropSchema: t('schemaTree.dropSchema'),
      executeSqlFile: t('main.ctx.executeSqlFile'),
      dataTransfer: t('schemaTree.dataTransfer'),
      compareSchema: t('schemaTree.compareSchema'),
      compareData: t('schemaTree.compareData'),
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
    (configId: string) => {
      setExpandedConnections((prev) => {
        const next = new Set(prev);
        if (next.has(configId)) {
          next.delete(configId);
        } else {
          next.add(configId);
          onSelectConnection(configId);
        }
        return next;
      });
    },
    [onSelectConnection],
  );

  const toggleDb = useCallback(
    async (configId: string, connectionId: string, dbName: string) => {
      const dbKey = `${configId}::${dbName}`;
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

      const tableKey = `${connectionId}::${dbName}`;
      if (dbTablesMap[tableKey]) return;
      if (loadingDbs.has(tableKey)) return;

      setLoadingDbs((prev) => new Set(prev).add(tableKey));
      try {
        await reloadDbTables(connectionId, dbName);
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
    async (catKey: string, catId: string, connectionId: string) => {
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
        const { databaseCommands } = await import('../../commands/database');
        const objs = await databaseCommands.getDatabaseObjects(connectionId, catId);
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: objs }));
      } catch {
        setDbObjectsMap((prev) => ({ ...prev, [catKey]: [] }));
      }
    },
    [expandedCats, dbObjectsMap],
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

      const isConnected = activeConnections[conn.id]?.status === 'connected';
      const dbMeta = DB_REGISTRY[conn.databaseType];
      const isMultiDb = shouldUseMultiDatabaseTree(dbMeta, conn.database);
      const moveTargets = groups
        .filter((g) => g !== conn.group)
        .map((g) => ({ id: g, label: formatGroupLabel(g, t) }));

      showWebContextMenu(
        buildMainConnectionContextMenuItems({
          labels: contextLabels,
          isConnected,
          grouped: Boolean(conn.group),
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
          onRefresh: () => {
            const entry = activeConnections[conn.id];
            if (entry?.connectionId) {
              viewActions?.refresh?.();
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
    },
    [
      activeConnections,
      connect,
      contextLabels,
      duplicateConnection,
      groups,
      loadForConnection,
      moveConnectionToGroup,
      onDeleteConnection,
      onDisconnect,
      onEditConnection,
      onNodeContextMenu,
      onSelectConnection,
      t,
    ],
  );

  // ── Database / Schema / Category / Object context menus ──

  const handleDatabaseContextMenu = useCallback(
    (e: React.MouseEvent, dbName: string, configId: string) => {
      e.preventDefault();
      e.stopPropagation();
      const entry = activeConnections[configId];
      const connectionId = entry?.connectionId;
      const conn = connections.find((c) => c.id === configId);
      const dbMeta = conn ? DB_REGISTRY[conn.databaseType] : undefined;
      showWebContextMenu(
        buildSchemaTreeContextMenuItems({
          kind: 'database',
          labels: schemaLabels,
          handlers: {
            onRefresh: connectionId
              ? () => {
                  if (!conn) return;
                  void loadForConnection(connectionId, {
                    preferredDatabase: dbName,
                    skipLoadTables: shouldUseMultiDatabaseTree(
                      DB_REGISTRY[conn.databaseType],
                      conn.database,
                    ),
                    databaseType: conn.databaseType,
                  });
                }
              : undefined,
            onNewQuery: () => {
              onSelectConnection(configId);
              viewActions?.newQuery?.();
            },
            onCopyDatabaseName: () => {
              void navigator.clipboard.writeText(dbName);
            },
            onViewErDiagram: () => {
              onSelectConnection(configId);
              viewActions?.openErDiagram?.();
            },
            onExecuteSqlFile: viewActions?.openSqlFile
              ? () => {
                  onSelectConnection(configId);
                  viewActions.openSqlFile!();
                }
              : undefined,
            onNewTable: viewActions?.createTable
              ? () => {
                  onSelectConnection(configId);
                  viewActions.createTable!();
                }
              : undefined,
            onCreateSchema: dbMeta?.supportsCreateSchema
              ? () => {
                  onSelectConnection(configId);
                  viewActions?.openCreateSchema?.();
                }
              : undefined,
            onDropDatabase: connectionId
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
                      await driverCommands.execute({
                        connectionId,
                        command: 'drop_database',
                        input: { name: dbName },
                      });
                      await loadForConnection(connectionId, {
                        databaseType: conn.databaseType,
                        skipLoadTables: true,
                      });
                    } catch (e) {
                      console.warn('drop_database failed', e);
                    }
                  })();
                }
              : undefined,
            onDataTransfer: () => openDataSyncWindow(),
            onCompareSchema: () => openSchemaDiffWindow(),
            onCompareData: () => openDataSyncWindow(),
          },
          showNewTable: true,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      activeConnections,
      confirmDropDatabase,
      connections,
      loadForConnection,
      onSelectConnection,
      schemaLabels,
      t,
      viewActions,
    ],
  );

  const handleSchemaContextMenu = useCallback(
    (e: React.MouseEvent, schemaName: string, configId: string) => {
      e.preventDefault();
      e.stopPropagation();
      const entry = activeConnections[configId];
      const connectionId = entry?.connectionId;
      const conn = connections.find((c) => c.id === configId);
      showWebContextMenu(
        buildSchemaTreeContextMenuItems({
          kind: 'schema',
          labels: schemaLabels,
          handlers: {
            onRefresh: () => {
              if (!connectionId || !conn) return;
              void loadForConnection(connectionId, {
                preferredDatabase: conn.database,
                skipLoadTables: shouldUseMultiDatabaseTree(
                  DB_REGISTRY[conn.databaseType],
                  conn.database,
                ),
                databaseType: conn.databaseType,
              });
            },
            onNewQuery: () => {
              onSelectConnection(configId);
              viewActions?.newQuery?.();
            },
            onExecuteSqlFile: viewActions?.openSqlFile
              ? () => {
                  onSelectConnection(configId);
                  viewActions.openSqlFile!();
                }
              : undefined,
            onNewTable: viewActions?.createTable
              ? () => {
                  onSelectConnection(configId);
                  viewActions.createTable!();
                }
              : undefined,
            onCopyName: () => {
              void navigator.clipboard.writeText(schemaName);
            },
            onViewErDiagram: () => {
              onSelectConnection(configId);
              viewActions?.openErDiagram?.();
            },
            onDropSchema:
              connectionId && !schemaName.startsWith('pg_') && schemaName !== 'information_schema'
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
                          connectionId,
                          command: 'drop_schema',
                          input: { name: schemaName, cascade: true },
                        });
                        await loadForConnection(connectionId, {
                          preferredDatabase: conn.database,
                          databaseType: conn.databaseType,
                          skipLoadTables: false,
                        });
                      } catch (e) {
                        console.warn('drop_schema failed', e);
                      }
                    })();
                  }
                : undefined,
            onDataTransfer: () => openDataSyncWindow(),
            onCompareSchema: () => openSchemaDiffWindow(),
            onCompareData: () => openDataSyncWindow(),
          },
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
      schemaLabels,
      t,
      viewActions,
    ],
  );

  const handleCategoryContextMenu = useCallback(
    (e: React.MouseEvent, catKey: string, catId: string, configId: string) => {
      e.preventDefault();
      e.stopPropagation();
      showWebContextMenu(
        buildSchemaTreeContextMenuItems({
          kind: 'category',
          labels: schemaLabels,
          handlers: {
            onRefresh: () => {
              const entry = activeConnections[configId];
              if (!entry?.connectionId) return;
              if (catId === 'tables' || catId === 'views') {
                const conn = connections.find((c) => c.id === configId);
                if (!conn) return;
                void loadForConnection(entry.connectionId, {
                  preferredDatabase: conn.database,
                  skipLoadTables: false,
                  databaseType: conn.databaseType,
                });
              } else {
                setExpandedCats((prev) => {
                  const next = new Set(prev);
                  next.delete(catKey);
                  return next;
                });
                setDbObjectsMap((prev) => {
                  const next = { ...prev };
                  delete next[catKey];
                  return next;
                });
              }
            },
          },
          categoryId: catId,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [activeConnections, connections, loadForConnection, schemaLabels],
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
        toggleConnection(conn.id);
      }
    },
    [onSelectConnection, activeConnections, toggleConnection],
  );

  const handleConnectionDoubleClick = useCallback(
    (conn: ConnectionConfig) => {
      const status = activeConnections[conn.id]?.status ?? 'idle';
      if (status === 'connected') {
        onSelectConnection(conn.id);
      } else if (status !== 'connecting') {
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
      configId: string,
      _connectionId: string,
      dbName: string,
      schemaName: string | undefined,
      baseDepth: number,
      dbType: string,
    ) => {
      const realItems = allItems.filter((i) => i.name !== '');
      const tblItems = realItems.filter(
        (i) => i.tableType === 'table' || i.tableType === 'systemTable',
      );
      const viewItems = realItems.filter(
        (i) => i.tableType === 'view' || i.tableType === 'materializedView',
      );

      for (const cat of getCategoriesForDriver(dbType)) {
        const catKey = schemaName
          ? `${configId}::${dbName}::${schemaName}::${cat.id}`
          : `${configId}::${dbName}::${cat.id}`;
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
                configId,
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
      if (cEntry?.status !== 'connected' || !cEntry.connectionId) return false;
      const sd = schemas.get(cEntry.connectionId);
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
        if (!key.startsWith(cEntry.connectionId + '::')) continue;
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

      const expanded = expandedGroups.has(groupName) || !!query;
      const displayName = groupName ? formatGroupLabel(groupName, t) : t('main.ungrouped');

      rows.push({
        type: 'group',
        groupName,
        displayName,
        count: filteredConns.length,
        expanded,
      });

      if (!expanded) continue;

      if (filteredConns.length === 0) {
        rows.push({ type: 'empty-group' });
        continue;
      }

      for (const conn of filteredConns) {
        const entry = activeConnections[conn.id];
        const status = entry?.status ?? 'idle';
        const isConnected = status === 'connected';
        const isExpanded = expandedConnections.has(conn.id) || !!query;

        rows.push({
          type: 'connection',
          conn,
          isSelected: activeConfigId === conn.id,
          status,
          expanded: (isExpanded && isConnected) || !!query,
          depth: 1,
        });

        if (!isConnected || (!isExpanded && !query)) continue;

        const connectionId = entry!.connectionId!;
        const schemaData = schemas.get(connectionId);
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
            connectionId,
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
                configId: conn.id,
                connectionId,
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
          const dbs = query
            ? schemaData.databases.filter((d) => {
                if (d.toLowerCase().includes(query)) return true;
                const tblKey = `${connectionId}::${d}`;
                const tbls = dbTablesMap[tblKey];
                return tbls?.some((tbl) => tbl.name.toLowerCase().includes(query)) ?? false;
              })
            : schemaData.databases;

          for (const dbName of dbs) {
            const dbKey = `${conn.id}::${dbName}`;
            const tableKey = `${connectionId}::${dbName}`;
            const isDbExpanded = expandedDbs.has(dbKey) || !!query;
            const isLoading = loadingDbs.has(tableKey);

            rows.push({
              type: 'db',
              configId: conn.id,
              connectionId,
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
                const schemaKey = `${conn.id}::${dbName}::${schemaName}`;
                const schemaItems = schemaGroups.get(schemaName) ?? [];
                const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

                rows.push({
                  type: 'schema',
                  configId: conn.id,
                  dbName,
                  schemaName,
                  expanded: schemaExpanded,
                  depth: 3,
                });

                if (schemaExpanded) {
                  addCategories(
                    schemaItems,
                    conn.id,
                    connectionId,
                    dbName,
                    schemaName,
                    4,
                    conn.databaseType,
                  );
                }
              }
            } else {
              addCategories(
                allItems,
                conn.id,
                connectionId,
                dbName,
                undefined,
                3,
                conn.databaseType,
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
            configId: conn.id,
            connectionId,
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
              const schemaKey = `${conn.id}::${dbName}::${schemaName}`;
              const schemaItems = schemaGroups.get(schemaName) ?? [];
              const schemaExpanded = expandedSchemas.has(schemaKey) || !!query;

              rows.push({
                type: 'schema',
                configId: conn.id,
                dbName,
                schemaName,
                expanded: schemaExpanded,
                depth: 3,
              });

              if (schemaExpanded) {
                addCategories(
                  schemaItems,
                  conn.id,
                  connectionId,
                  dbName,
                  schemaName,
                  4,
                  conn.databaseType,
                );
              }
            }
          } else {
            addCategories(allItems, conn.id, connectionId, dbName, undefined, 3, conn.databaseType);
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
    activeConfigId,
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

  const renderStatusDot = (configId: string) => {
    const status = activeConnections[configId]?.status ?? 'idle';
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
            onClick={() => void toggleDb(row.configId, row.connectionId, row.dbName)}
            onContextMenu={(e) => handleDatabaseContextMenu(e, row.dbName, row.configId)}
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
            onClick={() => toggleSchema(`${row.configId}::${row.dbName}::${row.schemaName}`)}
            onContextMenu={(e) => handleSchemaContextMenu(e, row.schemaName, row.configId)}
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
        const catConfigId = row.key.split('::')[0];
        return (
          <button
            type="button"
            data-tree-node="category"
            data-cat-id={row.cat.id}
            className="flex w-full items-center gap-1.5 py-1 pr-2 text-left text-[13px] text-fg-secondary hover:bg-surface-raised"
            style={{ paddingLeft: depthPadding(row.depth) }}
            onClick={() => {
              const conn = connections.find((c) => c.id === catConfigId);
              const connectionId =
                conn && activeConnections[conn.id]?.connectionId
                  ? activeConnections[conn.id].connectionId!
                  : '';
              void toggleCategory(row.key, row.cat.id, connectionId);
            }}
            onContextMenu={(e) => handleCategoryContextMenu(e, row.key, row.cat.id, catConfigId)}
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
              onSelectConnection(row.configId);
              onSelectTable(row.item.name, row.item.schema);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onNodeContextMenu?.({
                kind: row.catId === 'views' ? 'view' : 'table',
                name: row.item.name,
                x: e.clientX,
                y: e.clientY,
                schema: row.item.schema ?? undefined,
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
                onSelectKvDb(row.configId, row.dbName);
              } else {
                onSelectConnection(row.configId);
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
                e.preventDefault();
                e.stopPropagation();
                onNodeContextMenu?.({ kind: menuKind, name: row.name, x: e.clientX, y: e.clientY });
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
                void ensureNamespacePath(row.segments, row.connectionId);
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
    <div className="flex min-h-0 flex-1 flex-col bg-surface">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center justify-between border-b border-edge px-2 py-1.5">
        <span className="text-[13px] font-semibold text-fg">{t('nav.connections')}</span>
        <div className="flex items-center gap-0.5">
          {onExportConnections && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onExportConnections}
              title={t('menu.exportConnections')}
            >
              <Upload className="h-3.5 w-3.5" />
            </button>
          )}
          {onImportConnections && (
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={onImportConnections}
              title={t('menu.importConnections')}
            >
              <Download className="h-3.5 w-3.5" />
            </button>
          )}
          <button
            type="button"
            className="flex h-6 w-6 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
            onClick={onNewConnection}
            title={t('main.newConnection')}
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
            title={t('main.newGroupTitle')}
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
            placeholder={t('common.search')}
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
        title={t('main.newGroupTitle')}
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

      {confirmDeleteGroupDialog}
      {confirmDropDatabaseDialog}
      {confirmDropSchemaDialog}
    </div>
  );
}
