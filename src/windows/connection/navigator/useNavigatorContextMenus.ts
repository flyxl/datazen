import { useCallback, useMemo } from 'react';
import { formatGroupLabel } from '../../../lib/connectionGroups';
import { DB_REGISTRY } from '../../../lib/databaseTypes';
import { getSqlDialect } from '../../../lib/sqlDialects';
import { invalidateSchemaCache, getCachedDDL } from '../../../lib/schemaCache';
import {
  buildMainConnectionContextMenuItems,
  buildMainGroupContextMenuItems,
} from '../../../lib/mainWindowContextMenu';
import { buildSchemaTreeContextMenuItems } from '../../../lib/schemaTreeContextMenu';
import { isProductFeatureEnabled } from '../../../lib/productFeatures';
import { buildConnectionUrl } from '../../../lib/buildConnectionUrl';
import { hasCommand } from '../../../lib/commandSchema';
import {
  SERVER_STATUS_SNAPSHOT_COMMAND,
  LIST_PROCESSES_COMMAND,
} from '../../../lib/driverCommandIds';
import {
  openDataSyncWindow,
  openDataTransferWindow,
  openSchemaDiffWindow,
  openBackupWindow,
} from '../../../lib/windowManager';
import { showWebContextMenu } from '../../../stores/contextMenuStore';
import { usePanelStore } from '../../../stores/panelStore';
import { useSchemaStore } from '../../../stores/schemaStore';
import { buildQueryOpenContext } from '../../../lib/tableSqlActions';
import type { ConnectionOpenTarget } from '../../../lib/connectionViews/types';
import { databaseCommands } from '../../../commands/database';
import { driverCommands } from '../../../commands/driver';
import { queryCommands } from '../../../commands/query';
import { shouldUseMultiDatabaseTree } from '../schema-tree/SchemaTree';
import type { I18nKey } from '../../../locales';
import type { ConnectionConfig, TableInfo } from '../../../types';
import type { ConnectionEntry } from '../../../stores/activeConnectionStore';
import { extractErrorMessage, quoteRelationName, resolveDropDatabaseFallback } from './utils';
import type { ConnectionNavigatorTreeProps } from './types';

type ConfirmFn = (opts: {
  title: string;
  message: string;
  confirmLabel: string;
  kind: 'warning';
}) => Promise<boolean>;

export interface NavigatorContextMenuDeps {
  t: (key: I18nKey, params?: Record<string, string | number>) => string;
  safeMode: boolean;
  groups: string[];
  connections: ConnectionConfig[];
  activeConnections: Record<string, ConnectionEntry | undefined>;
  dbTablesMap: Record<string, TableInfo[]>;
  onSelectConnection: ConnectionNavigatorTreeProps['onSelectConnection'];
  onSelectTable: ConnectionNavigatorTreeProps['onSelectTable'];
  onDisconnect: ConnectionNavigatorTreeProps['onDisconnect'];
  onEditConnection: ConnectionNavigatorTreeProps['onEditConnection'];
  onDeleteConnection: ConnectionNavigatorTreeProps['onDeleteConnection'];
  onShowMessage?: ConnectionNavigatorTreeProps['onShowMessage'];
  viewActions?: ConnectionNavigatorTreeProps['viewActions'];
  connect: (conn: ConnectionConfig) => Promise<void>;
  duplicateConnection: (id: string) => Promise<void>;
  deleteGroup: (name: string) => Promise<void>;
  moveConnectionToGroup: (id: string, group?: string) => Promise<void>;
  toggleConnectionPinned: (id: string) => Promise<void>;
  toggleConnection: (connectionId: string, sectionGroup: string) => void;
  refreshConnection: (connectionId: string) => Promise<void>;
  refreshDatabase: (connectionId: string, dbName: string) => Promise<void>;
  refreshSchema: (connectionId: string, dbName: string, schemaName: string) => Promise<void>;
  reloadDbTables: (dbSessionId: string, dbName: string) => Promise<void>;
  reloadDbObjectCategory: (dbSessionId: string, catKey: string, catId: string) => Promise<void>;
  loadForConnection: (
    dbSessionId: string,
    opts: {
      preferredDatabase?: string;
      skipLoadTables?: boolean;
      databaseType: ConnectionConfig['databaseType'];
    },
  ) => Promise<void>;
  activateDatabase: (dbSessionId: string, dbName: string) => Promise<void>;
  clearDbLocalCache: (connectionId: string, dbSessionId: string, dbName: string) => void;
  removeRelation: (name: string, dbSessionId: string) => void;
  setDbTablesMap: React.Dispatch<React.SetStateAction<Record<string, TableInfo[]>>>;
  buildOpenTarget: (conn: {
    id: string;
    name: string;
    databaseType: string;
  }) => ConnectionOpenTarget | null;
  confirmDeleteGroup: ConfirmFn;
  confirmDropDatabase: ConfirmFn;
  confirmDropSchema: ConfirmFn;
  confirmDropRelation: ConfirmFn;
  confirmTruncateTable: ConfirmFn;
  setNewGroupDialogOpen: (open: boolean) => void;
  setNewGroupName: (name: string) => void;
  setRenamingGroup: (name: string | null) => void;
  setRenameValue: (value: string) => void;
  setObjectFilterConn: (conn: ConnectionConfig | null) => void;
  onNewConnection: (defaultGroup?: string) => void;
}

export function useNavigatorContextMenus(deps: NavigatorContextMenuDeps) {
  const {
    t,
    safeMode,
    groups,
    connections,
    activeConnections,
    dbTablesMap,
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
    refreshConnection,
    refreshDatabase,
    refreshSchema,
    reloadDbTables,
    reloadDbObjectCategory,
    loadForConnection,
    activateDatabase,
    clearDbLocalCache,
    removeRelation,
    setDbTablesMap,
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
    onNewConnection,
  } = deps;

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
      connection: t('main.ctx.connection'),
      server: t('main.ctx.server'),
      organize: t('main.ctx.organize'),
      createNew: t('main.ctx.createNew'),
      database: t('common.database'),
      user: t('common.user'),
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
          onNewConnection: () => onNewConnection(groupName || undefined),
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
    [
      contextLabels,
      confirmDeleteGroup,
      deleteGroup,
      setNewGroupDialogOpen,
      setNewGroupName,
      setRenamingGroup,
      setRenameValue,
      t,
      onNewConnection,
    ],
  );

  const handleConnectionContextMenu = useCallback(
    (e: React.MouseEvent, conn: ConnectionConfig, sectionGroup: string) => {
      e.preventDefault();
      e.stopPropagation();

      void (async () => {
        const isConnected = activeConnections[conn.id]?.status === 'connected';
        const dbMeta = DB_REGISTRY[conn.databaseType];
        const isMultiDb = shouldUseMultiDatabaseTree(dbMeta, conn.database);
        const moveTargets = groups
          .filter((g) => g !== conn.group)
          .map((g) => ({ id: g, label: formatGroupLabel(g, t) }));

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
                toggleConnection(conn.id, sectionGroup);
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
              if (isConnected) {
                viewActions?.openQueryHistory?.();
              } else {
                // Connection not open yet — ContentView hasn't mounted, so
                // actionsRef.current is null. Store a pending intent; usePanelHandlers
                // will consume it once the connection context becomes available.
                usePanelStore.getState().setPendingQueryHistory(conn.id);
                toggleConnection(conn.id, sectionGroup);
                void connect(conn);
              }
            },
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
      buildOpenTarget,
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
      setObjectFilterConn,
      t,
      toggleConnection,
      toggleConnectionPinned,
      viewActions,
    ],
  );

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
                  useSchemaStore.setState({ currentDatabase: dbName });
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
                      const fallback = resolveDropDatabaseFallback(
                        schemaData?.databases ?? [],
                        dbName,
                        conn.database,
                      );
                      if (fallback) {
                        await databaseCommands.getTables(dbSessionId, fallback);
                        if (activeDb === dbName && schemaData) {
                          const cached = dbTablesMap[`${dbSessionId}::${fallback}`];
                          if (cached) {
                            useSchemaStore
                              .getState()
                              .setLoadedTables(fallback, cached, dbSessionId);
                          } else {
                            useSchemaStore.setState((state) => {
                              const schemaEntry = state.schemas.get(dbSessionId);
                              let schemas = state.schemas;
                              if (schemaEntry) {
                                schemas = new Map(state.schemas);
                                schemas.set(dbSessionId, {
                                  ...schemaEntry,
                                  currentDatabase: fallback,
                                });
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
                        dbSessionId,
                        command: 'drop_database',
                        input: { name: dbName },
                      });
                      clearDbLocalCache(connectionId, dbSessionId, dbName);
                      await loadForConnection(dbSessionId, {
                        databaseType: conn.databaseType,
                        skipLoadTables: true,
                      });
                    } catch (err) {
                      onShowMessage?.(
                        extractErrorMessage(err, t('schemaTree.dropDatabaseFailed')),
                        'error',
                      );
                    }
                  })();
                }
              : undefined,
            onDataTransfer: isProductFeatureEnabled('dataTransfer')
              ? () => openDataTransferWindow()
              : undefined,
            onCompareSchema: isProductFeatureEnabled('schemaDiff')
              ? () => openSchemaDiffWindow()
              : undefined,
            onCompareData: isProductFeatureEnabled('dataSync')
              ? () => openDataSyncWindow()
              : undefined,
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
      clearDbLocalCache,
      confirmDropDatabase,
      connections,
      dbTablesMap,
      loadForConnection,
      onSelectConnection,
      onShowMessage,
      refreshDatabase,
      safeMode,
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
                          dbSessionId,
                          command: 'drop_schema',
                          input: { name: schemaName, cascade: true },
                          database: dbName,
                        });
                        await refreshDatabase(connectionId, dbName);
                      } catch (err) {
                        onShowMessage?.(
                          extractErrorMessage(err, t('schemaTree.dropSchemaFailed')),
                          'error',
                        );
                      }
                    })();
                  }
                : undefined,
            onDataTransfer: isProductFeatureEnabled('dataTransfer')
              ? () => openDataTransferWindow()
              : undefined,
            onCompareSchema: isProductFeatureEnabled('schemaDiff')
              ? () => openSchemaDiffWindow()
              : undefined,
            onCompareData: isProductFeatureEnabled('dataSync')
              ? () => openDataSyncWindow()
              : undefined,
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
      onSelectConnection,
      onShowMessage,
      refreshDatabase,
      refreshSchema,
      safeMode,
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
      const supportsErDiagram = dbMeta?.supportsErDiagram !== false;

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
            onCopyDdl: () => {
              const dialect = getSqlDialect(conn?.databaseType ?? 'postgresql');
              if (!dialect) return;
              const { sql, extractColumnIndex } = dialect.ddl.getTableDdlQuery(name);
              void getCachedDDL(dbSessionId, name, sql, (rows) => {
                const row = rows[0];
                const val = row?.[extractColumnIndex];
                const ddl = typeof val === 'string' ? val : val != null ? String(val) : '';
                if (ddl) void navigator.clipboard.writeText(ddl);
                return ddl;
              }).catch((err) => console.warn(err));
            },
            onFocusEr:
              kind === 'table' && supportsErDiagram
                ? () => {
                    onSelectConnection(connectionId);
                    viewActions?.openErDiagram?.(name);
                  }
                : undefined,
            onNewQuery: () => {
              onSelectConnection(connectionId);
              useSchemaStore.setState({ currentDatabase: dbName });
              if (kind === 'table') {
                const query = buildQueryOpenContext(
                  {
                    connectionId,
                    dbSessionId,
                    databaseType: conn?.databaseType ?? 'postgresql',
                    database: dbName,
                    schema,
                    tableName: name,
                  },
                  { kind: 'select', source: 'table-action' },
                );
                viewActions?.newQuery?.(query.initialSql, query);
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
                        await queryCommands.executeQuery(
                          dbSessionId,
                          sql,
                          undefined,
                          dbName,
                          schema ?? null,
                        );
                      } catch (err) {
                        onShowMessage?.(
                          extractErrorMessage(err, t('schemaTree.truncateFailed')),
                          'error',
                        );
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
                        await queryCommands.executeQuery(
                          dbSessionId,
                          sql,
                          undefined,
                          dbName,
                          schema ?? null,
                        );
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
          showErFocus: supportsErDiagram,
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
      setDbTablesMap,
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

  return {
    handleGroupContextMenu,
    handleConnectionContextMenu,
    handleDatabaseContextMenu,
    handleSchemaContextMenu,
    handleTableContextMenu,
    handleCategoryContextMenu,
    handleObjectContextMenu,
  };
}
