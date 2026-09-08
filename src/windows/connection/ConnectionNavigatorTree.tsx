import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { useI18n } from '../../hooks/useI18n';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { ConnectionOpenTarget } from '../../lib/connectionViews/types';
import {
  EVENT_CONNECTIONS_CHANGED,
  groupConnectionsWithPinnedSection,
  useConnectionStore,
} from '../../stores/connectionStore';
import { emitCrossWindow } from '../../lib/crossWindowBus';
import {
  connectionExpandKey,
  parseConnectionExpandKey,
  PINNED_GROUP_KEY,
  RECENT_GROUP_KEY,
} from '../../lib/connectionLocator';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { shouldUseMultiDatabaseTree } from './schema-tree/SchemaTree';
import { connectionCommands } from '../../commands/connection';
import type { ConnectionConfig } from '../../types';
import { buildNavigatorFlatRows } from './navigator/buildFlatRows';
import { NavigatorDialogs } from './navigator/NavigatorDialogs';
import { NavigatorToolbar } from './navigator/NavigatorToolbar';
import { NavigatorTreeRow, type GroupDropTarget } from './navigator/NavigatorTreeRow';
import { createDragGhost, getUnifiedRowKey, removeDragGhost } from './navigator/utils';
import {
  ConnectionNavigatorTreeHandle,
  ConnectionNavigatorTreeProps,
  NAVIGATOR_ROW_HEIGHT,
} from './navigator/types';
import { useNavigatorContextMenus } from './navigator/useNavigatorContextMenus';
import { useNavigatorDbState } from './navigator/useNavigatorDbState';

export type { ConnectionNavigatorTreeHandle, ConnectionNavigatorTreeProps };

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

  const grouped = useMemo(
    () => groupConnectionsWithPinnedSection(connections, groups, ''),
    [connections, groups],
  );

  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [expandedConnections, setExpandedConnections] = useState<Set<string>>(new Set());
  const [expandedDbs, setExpandedDbs] = useState<Set<string>>(new Set());
  const [expandedSchemas, setExpandedSchemas] = useState<Set<string>>(new Set());
  const [expandedCats, setExpandedCats] = useState<Set<string>>(new Set());
  const pendingConnectionExpansionRef = useRef<{
    connectionId: string;
    sectionGroup: string;
  } | null>(null);

  const dbState = useNavigatorDbState(
    activeConnections,
    connections,
    expandedDbs,
    expandedCats,
    loadForConnection,
    ensureNamespacePath,
  );

  const clearDbLocalCache = useCallback(
    (connectionId: string, dbSessionId: string, dbName: string) => {
      dbState.clearDbLocalCache(connectionId, dbSessionId, dbName);
      const dbKey = `${connectionId}::${dbName}`;
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
    [dbState],
  );

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

  useEffect(() => {
    setExpandedConnections((prev) => {
      // A connection can be rendered in more than one section (for example
      // in both "Recent" and its persisted group). Keep only one scoped key,
      // otherwise expanding one occurrence also expands every shortcut row.
      let pending = pendingConnectionExpansionRef.current;

      const validKeys = [...prev].filter((key) => {
        const { sectionGroup, connectionId } = parseConnectionExpandKey(key);
        const status = activeConnections[connectionId]?.status ?? 'idle';
        const visibleInTree = grouped.some(
          (section) =>
            section.group === sectionGroup &&
            section.connections.some((connection) => connection.id === connectionId),
        );
        if (!visibleInTree) return false;
        if (status === 'connected') return true;
        if (status === 'connecting' && pending?.connectionId === connectionId) return true;
        return false;
      });

      const keepValidExpansion = () => {
        if (validKeys.length > 0) {
          const keep = validKeys[0];
          return validKeys.length === 1 && prev.size === 1 && prev.has(keep)
            ? prev
            : new Set([keep]);
        }
        return prev.size > 0 ? new Set<string>() : prev;
      };

      const activeEntry = activeConnectionId ? activeConnections[activeConnectionId] : undefined;

      // Connections can also be opened outside the navigator. Track an active
      // connecting tab so a slow connection never causes an unrelated already
      // connected row (usually the first item in Recent) to be auto-expanded.
      if (!pending && activeConnectionId && activeEntry?.status === 'connecting') {
        const section = grouped.find((candidate) =>
          candidate.connections.some((connection) => connection.id === activeConnectionId),
        );
        if (section) {
          pending = { connectionId: activeConnectionId, sectionGroup: section.group };
          pendingConnectionExpansionRef.current = pending;
        } else {
          return keepValidExpansion();
        }
      }

      if (pending) {
        const entry = activeConnections[pending.connectionId];
        const connectionStillExists = connections.some(
          (connection) => connection.id === pending.connectionId,
        );

        if (!connectionStillExists || entry?.status === 'error') {
          pendingConnectionExpansionRef.current = null;
          return keepValidExpansion();
        }

        if (entry?.status === 'connected') {
          const section =
            grouped.find(
              (candidate) =>
                candidate.group === pending.sectionGroup &&
                candidate.connections.some((connection) => connection.id === pending.connectionId),
            ) ??
            grouped.find((candidate) =>
              candidate.connections.some((connection) => connection.id === pending.connectionId),
            );
          pendingConnectionExpansionRef.current = null;
          if (section) {
            return new Set([connectionExpandKey(section.group, pending.connectionId)]);
          }
        } else if (entry?.status === 'connecting') {
          const section =
            grouped.find(
              (candidate) =>
                candidate.group === pending.sectionGroup &&
                candidate.connections.some((connection) => connection.id === pending.connectionId),
            ) ??
            grouped.find((candidate) =>
              candidate.connections.some((connection) => connection.id === pending.connectionId),
            );
          if (section) {
            return new Set([connectionExpandKey(section.group, pending.connectionId)]);
          }
        }

        return keepValidExpansion();
      }

      if (validKeys.length > 0) {
        const keep = validKeys[0];
        return validKeys.length === 1 && prev.size === 1 && prev.has(keep) ? prev : new Set([keep]);
      }

      // Preserve an intentional collapsed state. The first connected session
      // is expanded only when there is no prior expansion to retain.
      if (prev.size > 0) return new Set();

      const initialConnectionId =
        activeConnectionId && activeConnections[activeConnectionId]?.status === 'connected'
          ? activeConnectionId
          : Object.entries(activeConnections).find(
              ([, entry]) => entry?.status === 'connected',
            )?.[0];
      if (!initialConnectionId) return prev;

      const initialSection = grouped.find((section) =>
        section.connections.some((connection) => connection.id === initialConnectionId),
      );
      if (!initialSection) return prev;
      return new Set([connectionExpandKey(initialSection.group, initialConnectionId)]);
    });
  }, [activeConnectionId, activeConnections, connections, grouped]);

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
    for (const key of expandedConnections) {
      const { connectionId } = parseConnectionExpandKey(key);
      const entry = activeConnections[connectionId];
      if (entry?.status !== 'connected' || !entry.dbSessionId) continue;
      if (loadedConnectionsRef.current.has(connectionId)) continue;
      loadedConnectionsRef.current.add(connectionId);

      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) continue;

      const meta = DB_REGISTRY[conn.databaseType];
      const isCustomTree = meta?.schemaTreeMode === 'custom';
      const isPathHierarchy = meta?.namespaceEnsure === 'path-hierarchy';
      const isPluginManaged = isCustomTree || isPathHierarchy;
      const isMultiDb = shouldUseMultiDatabaseTree(meta, conn.database);

      void loadForConnection(entry.dbSessionId, {
        preferredDatabase: conn.database,
        skipLoadTables: isMultiDb || isPluginManaged,
        databaseType: conn.databaseType,
      }).then(() => {
        if (isPathHierarchy) {
          void ensureNamespacePath([], entry.dbSessionId);
        }
        // For multi-db connections, do not auto-expand the default database;
        // keep expansion at the databases container level so the user sees the database list.
      });

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
    dbState.reloadDbTables,
  ]);

  const collapseAll = useCallback(() => {
    pendingConnectionExpansionRef.current = null;
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
    (connectionId: string, sectionGroup: string) => {
      const key = connectionExpandKey(sectionGroup, connectionId);
      pendingConnectionExpansionRef.current = null;
      setExpandedConnections((prev) => {
        if (prev.has(key)) return new Set();
        onSelectConnection(connectionId);
        return new Set([key]);
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

      setExpandedCats((prev) => new Set(prev).add(`${dbKey}::tables`));
      await dbState.toggleDb(connectionId, dbSessionId, dbName);
    },
    [expandedDbs, dbState],
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
      await dbState.toggleCategoryLoad(catKey, catId, dbSessionId);
    },
    [expandedCats, dbState],
  );

  useImperativeHandle(
    ref,
    () => ({
      refreshAllConnections: dbState.refreshAllConnections,
      refreshConnection: dbState.refreshConnection,
    }),
    [dbState.refreshAllConnections, dbState.refreshConnection],
  );

  const contextMenus = useNavigatorContextMenus({
    t,
    safeMode,
    groups,
    connections,
    activeConnections,
    dbTablesMap: dbState.dbTablesMap,
    onSelectConnection,
    onSelectTable,
    onDisconnect,
    onEditConnection,
    onDeleteConnection,
    onShowMessage,
    viewActions,
    connect,
    duplicateConnection,
    deleteGroup,
    moveConnectionToGroup,
    toggleConnectionPinned,
    toggleConnection,
    refreshConnection: dbState.refreshConnection,
    refreshDatabase: dbState.refreshDatabase,
    refreshSchema: dbState.refreshSchema,
    reloadDbTables: dbState.reloadDbTables,
    reloadDbObjectCategory: dbState.reloadDbObjectCategory,
    loadForConnection,
    activateDatabase: dbState.activateDatabase,
    clearDbLocalCache,
    removeRelation,
    setDbTablesMap: dbState.setDbTablesMap,
    buildOpenTarget,
    confirmDeleteGroup,
    confirmDropDatabase,
    confirmDropSchema,
    confirmDropRelation,
    confirmTruncateTable,
    setNewGroupDialogOpen,
    setNewGroupName,
    setRenamingGroup,
    setRenameValue,
    setObjectFilterConn,
    onNewConnection: (defaultGroup) => onNewConnection(defaultGroup),
  });

  const dragConnId = useRef<string | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    id: string;
    position: 'before' | 'after';
    targetGroup?: string;
  } | null>(null);
  const dropTargetRef = useRef(dropTarget);
  dropTargetRef.current = dropTarget;
  const [groupDropTarget, setGroupDropTarget] = useState<GroupDropTarget | null>(null);

  const handleDragStart = useCallback(
    (e: React.DragEvent, connId: string) => {
      const sel = window.getSelection();
      if (sel && !sel.isCollapsed) {
        sel.removeAllRanges();
      }

      dragConnId.current = connId;
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', connId);
      e.dataTransfer.setData('application/datazen-connection', connId);

      if (e.dataTransfer.setDragImage) {
        const conn = connections.find((c) => c.id === connId);
        const ghost = createDragGhost(conn?.name ?? connId);
        e.dataTransfer.setDragImage(ghost, 16, 14);
      }
    },
    [connections],
  );

  const handleDragOver = useCallback(
    (e: React.DragEvent, targetId: string, targetSectionGroup?: string) => {
      if (!dragConnId.current || dragConnId.current === targetId) {
        setDropTarget(null);
        return;
      }
      setGroupDropTarget(null);
      if (targetSectionGroup === RECENT_GROUP_KEY || targetSectionGroup === PINNED_GROUP_KEY) {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'none';
        setDropTarget(null);
        return;
      }
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      const rect = e.currentTarget.getBoundingClientRect();
      const midY = rect.top + rect.height / 2;
      const position = e.clientY < midY ? 'before' : 'after';
      setDropTarget((prev) =>
        prev?.id === targetId &&
        prev.position === position &&
        prev.targetGroup === targetSectionGroup
          ? prev
          : { id: targetId, position, targetGroup: targetSectionGroup },
      );
    },
    [],
  );

  const handleDragLeave = useCallback((_e: React.DragEvent) => {
    // In WebKit / Safari, e.relatedTarget is ALWAYS null during drag events.
    // Relying on relatedTarget to check if drag left the element will ALWAYS evaluate to false
    // and wipe dropTarget on every mouse move across children!
    // Therefore, do NOT clear dropTarget on row dragleave.
    // dropTarget is updated on every dragover, and cleared on dragend or container dragleave.
  }, []);
  const handleDragEnd = useCallback(() => {
    dragConnId.current = null;
    setDropTarget(null);
    setGroupDropTarget(null);
    removeDragGhost();
  }, []);

  const handleGroupDragOver = useCallback(
    (e: React.DragEvent, groupName: string, target: 'header' | 'empty' = 'header') => {
      if (!dragConnId.current) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      setDropTarget(null);
      setGroupDropTarget((prev) =>
        prev?.groupName === groupName && prev.target === target ? prev : { groupName, target },
      );
    },
    [],
  );

  const handleGroupDragLeave = useCallback((_e: React.DragEvent) => {
    // Do not clear groupDropTarget on child leave; it will be updated by dragover
    // or cleared on dragend.
  }, []);

  const handleGroupDrop = useCallback(
    (e: React.DragEvent, groupName: string) => {
      // Only consume drops from connection reordering drags — table/view drops
      // must pass through to the SQL editor drop handler.
      const dt = e.dataTransfer;
      const dtConnId =
        typeof dt?.getData === 'function'
          ? dt.getData('application/datazen-connection') || dt.getData('text/plain')
          : null;
      const connId = dragConnId.current || dtConnId;
      if (!connId || !connections.some((c) => c.id === connId)) return;
      e.preventDefault();
      void (async () => {
        await useConnectionStore.getState().moveConnectionToGroup(connId, groupName || undefined);
        await useConnectionStore.getState().fetchConnections?.();
        void emitCrossWindow(EVENT_CONNECTIONS_CHANGED);
      })();
      dragConnId.current = null;
      setDropTarget(null);
      setGroupDropTarget(null);
    },
    [connections],
  );

  const handleSectionDragOver = useCallback((e: React.DragEvent, section: string) => {
    // Only respond to connection reordering drags — table/view drops from the
    // schema tree must pass through to the SQL editor drop handler.
    if (!dragConnId.current) return;
    if (section === 'recent' || section === 'pinned') {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'none';
    }
  }, []);

  const handleSectionDrop = useCallback((e: React.DragEvent, _section: string) => {
    // Only consume drops from connection reordering drags — table/view drops
    // must pass through to the SQL editor drop handler.
    if (!dragConnId.current) return;
    e.preventDefault();
    setDropTarget(null);
    setGroupDropTarget(null);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const dt = e.dataTransfer;
      const dtSourceId =
        typeof dt?.getData === 'function'
          ? dt.getData('application/datazen-connection') || dt.getData('text/plain')
          : null;
      const sourceId = dragConnId.current || dtSourceId;
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

      const sourceConn = connections.find((c) => c.id === sourceId);
      const targetGroup = target.targetGroup;

      const reordered = [...ids];
      reordered.splice(fromIndex, 1);
      const insertAt =
        target.position === 'before'
          ? reordered.indexOf(target.id)
          : reordered.indexOf(target.id) + 1;
      reordered.splice(insertAt, 0, sourceId);

      void (async () => {
        if (sourceConn && targetGroup !== undefined && (sourceConn.group ?? '') !== targetGroup) {
          await connectionCommands.saveConnection({
            ...sourceConn,
            group: targetGroup || undefined,
          });
        }
        await connectionCommands.reorderConnections(reordered);
        await useConnectionStore.getState().fetchConnections();
        void emitCrossWindow(EVENT_CONNECTIONS_CHANGED);
      })();

      handleDragEnd();
    },
    [connections, handleDragEnd],
  );

  const handleConnectionClick = useCallback(
    (conn: ConnectionConfig, sectionGroup: string) => {
      const entry = activeConnections[conn.id];
      const key = connectionExpandKey(sectionGroup, conn.id);
      if (entry?.status === 'connected') {
        pendingConnectionExpansionRef.current = null;
        setExpandedConnections((prev) =>
          prev.size === 1 && prev.has(key) ? prev : new Set([key]),
        );
      } else {
        pendingConnectionExpansionRef.current = {
          connectionId: conn.id,
          sectionGroup,
        };
        setExpandedConnections(new Set([key]));
      }
      onSelectConnection(conn.id);
    },
    [onSelectConnection, activeConnections],
  );

  const handleConnectionDoubleClick = useCallback(
    (conn: ConnectionConfig, sectionGroup: string) => {
      const status = activeConnections[conn.id]?.status ?? 'idle';
      if (status === 'connected') {
        pendingConnectionExpansionRef.current = null;
        const key = connectionExpandKey(sectionGroup, conn.id);
        setExpandedConnections((prev) =>
          prev.size === 1 && prev.has(key) ? prev : new Set([key]),
        );
        onSelectConnection(conn.id);
        return;
      }
      pendingConnectionExpansionRef.current = {
        connectionId: conn.id,
        sectionGroup,
      };
      setExpandedConnections(new Set([connectionExpandKey(sectionGroup, conn.id)]));
      onSelectConnection(conn.id);
      if (status !== 'connecting') {
        void connect(conn);
      }
    },
    [activeConnections, connect, onSelectConnection],
  );

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

  const query = debouncedSearch.toLowerCase();

  const flatRows = useMemo(
    () =>
      buildNavigatorFlatRows({
        grouped,
        expandedGroups,
        expandedConnections,
        expandedDbs,
        expandedSchemas,
        expandedCats,
        activeConnections,
        activeConnectionId,
        schemas,
        dbTablesMap: dbState.dbTablesMap,
        dbObjectsMap: dbState.dbObjectsMap,
        loadingDbs: dbState.loadingDbs,
        query,
        t,
      }),
    [
      grouped,
      expandedGroups,
      expandedConnections,
      expandedDbs,
      expandedSchemas,
      expandedCats,
      activeConnections,
      activeConnectionId,
      schemas,
      dbState.dbTablesMap,
      dbState.dbObjectsMap,
      dbState.loadingDbs,
      query,
      t,
    ],
  );

  const getItemKey = useCallback(
    (index: number) => {
      const row = flatRows[index];
      return row ? getUnifiedRowKey(row, index) : index;
    },
    [flatRows],
  );

  const virtualizer = useVirtualizer({
    count: flatRows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => NAVIGATOR_ROW_HEIGHT,
    getItemKey,
    overscan: 25,
  });

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

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col bg-surface select-none">
      <NavigatorToolbar
        t={t}
        searchQuery={searchQuery}
        setSearchQuery={setSearchQuery}
        onNewConnection={onNewConnection}
        onExportConnections={onExportConnections}
        onImportConnections={onImportConnections}
        onRefresh={onRefresh}
        onCollapseSidebar={onCollapseSidebar}
        onNewGroup={() => {
          setNewGroupName('');
          setNewGroupDialogOpen(true);
        }}
        onCollapseAll={collapseAll}
      />

      <div
        ref={scrollRef}
        className="flex-1 min-h-0 overflow-y-auto py-1 select-none"
        onDragLeave={(e) => {
          const current = scrollRef.current;
          if (!current) return;
          // Ignore events bubbling from child elements
          if (e.target !== current) return;
          const related = (e.relatedTarget ||
            (e.nativeEvent as MouseEvent)?.relatedTarget) as Node | null;
          if (related && current.contains(related)) return;
          const rect = current.getBoundingClientRect();
          if (rect.width > 0 && rect.height > 0) {
            if (
              e.clientX >= rect.left &&
              e.clientX < rect.right &&
              e.clientY >= rect.top &&
              e.clientY < rect.bottom
            ) {
              return;
            }
          }
          setDropTarget(null);
          setGroupDropTarget(null);
        }}
        onDragOver={(e) => {
          if (dropTargetRef.current || groupDropTarget !== null) {
            e.preventDefault();
            e.dataTransfer.dropEffect = 'move';
          }
        }}
        onDrop={(e) => {
          if (dropTargetRef.current) {
            handleDrop(e);
          } else if (groupDropTarget !== null) {
            handleGroupDrop(e, groupDropTarget.groupName);
          }
        }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((virtualRow) => (
            <div
              key={virtualRow.key}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: virtualRow.size,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              <NavigatorTreeRow
                row={flatRows[virtualRow.index]}
                t={t}
                connections={connections}
                activeConnections={activeConnections}
                dropTarget={dropTarget}
                expandedDbs={expandedDbs}
                onNewConnection={onNewConnection}
                onSelectConnection={onSelectConnection}
                onSelectTable={onSelectTable}
                onSelectKvDb={onSelectKvDb}
                toggleGroup={toggleGroup}
                toggleConnection={toggleConnection}
                toggleDb={toggleDb}
                toggleSchema={toggleSchema}
                toggleCategory={toggleCategory}
                activateDatabase={dbState.activateDatabase}
                ensureNamespacePath={ensureNamespacePath}
                setExpandedDbs={setExpandedDbs}
                handleGroupContextMenu={contextMenus.handleGroupContextMenu}
                handleConnectionContextMenu={contextMenus.handleConnectionContextMenu}
                handleDatabaseContextMenu={contextMenus.handleDatabaseContextMenu}
                handleSchemaContextMenu={contextMenus.handleSchemaContextMenu}
                handleTableContextMenu={contextMenus.handleTableContextMenu}
                handleCategoryContextMenu={contextMenus.handleCategoryContextMenu}
                handleObjectContextMenu={contextMenus.handleObjectContextMenu}
                handleConnectionClick={handleConnectionClick}
                handleConnectionDoubleClick={handleConnectionDoubleClick}
                handleDragStart={handleDragStart}
                handleDragOver={handleDragOver}
                handleDragLeave={handleDragLeave}
                handleDragEnd={handleDragEnd}
                handleDrop={handleDrop}
                groupDropTarget={groupDropTarget}
                handleGroupDragOver={handleGroupDragOver}
                handleGroupDragLeave={handleGroupDragLeave}
                handleGroupDrop={handleGroupDrop}
                handleSectionDragOver={handleSectionDragOver}
                handleSectionDrop={handleSectionDrop}
                renderStatusDot={renderStatusDot}
                viewActions={viewActions}
              />
            </div>
          ))}
        </div>
      </div>

      <NavigatorDialogs
        t={t}
        newGroupDialogOpen={newGroupDialogOpen}
        setNewGroupDialogOpen={setNewGroupDialogOpen}
        newGroupName={newGroupName}
        setNewGroupName={setNewGroupName}
        addGroup={addGroup}
        renamingGroup={renamingGroup}
        setRenamingGroup={setRenamingGroup}
        renameValue={renameValue}
        setRenameValue={setRenameValue}
        renameGroup={renameGroup}
        objectFilterConn={objectFilterConn}
        setObjectFilterConn={setObjectFilterConn}
        saveConnection={saveConnection}
        refreshConnection={dbState.refreshConnection}
        confirmDeleteGroupDialog={confirmDeleteGroupDialog}
        confirmDropDatabaseDialog={confirmDropDatabaseDialog}
        confirmDropSchemaDialog={confirmDropSchemaDialog}
        confirmDropRelationDialog={confirmDropRelationDialog}
        confirmTruncateTableDialog={confirmTruncateTableDialog}
      />
    </div>
  );
});
