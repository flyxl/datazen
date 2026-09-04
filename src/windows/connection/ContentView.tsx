import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  type MutableRefObject,
} from 'react';
import { useResizable } from '../../hooks/useResizable';
import { useI18n } from '../../hooks/useI18n';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { usePanelStore, type ViewPanel, type ConnectionContext } from '../../stores/panelStore';
import { DB_REGISTRY, escapeIdent } from '../../lib/databaseTypes';
import { canOpenStructureEditor } from '../../lib/structureEditor/canOpenStructureEditor';
import {
  resolveExportScope,
  supportsAnyExport,
  supportsFullTableExport,
} from '../../lib/exportCapability';
import { getCachedDDL, invalidateSchemaCache } from '../../lib/schemaCache';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { buildSchemaTreeContextMenuItems } from '../../lib/schemaTreeContextMenu';
import { getSqlDialect } from '../../lib/sqlDialects';
import { openBackupWindow } from '../../lib/windowManager';
import { queryCommands } from '../../commands/query';
import type {
  ConnectionViewActions,
  NodeContextMenuPayload,
} from '../../lib/connectionViews/types';
import type { DatabaseType } from '../../types';
import type { SchemaTreeNodeContextMenuPayload } from './schema-tree/SchemaTree';
import { ExportDialog } from './ExportDialog';
import { BatchExportDialog } from './BatchExportDialog';
import { ImportDialog } from './ImportDialog';
import { loadBatchExportTableData } from '../../lib/loadBatchExportTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { DetailPanel } from '../../components/DataTable/DetailPanel';
import { AiChatPanel } from '../../components/ai/AiChatPanel';
import { rowToRecord } from '../../lib/rowToRecord';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { resolveConnectionContext } from './contentViewHelpers';
import { ContentToolbar } from './ContentToolbar';
import { PanelTabBar } from './PanelTabBar';
import { ContentStatusBar } from './ContentStatusBar';
import { PanelContentRenderer } from './PanelContentRenderer';
import { usePanelHandlers } from './usePanelHandlers';
import { CreateDatabaseDialog } from './CreateDatabaseDialog';
import { CreateSchemaDialog } from './CreateSchemaDialog';
import { CreateUserDialog } from './CreateUserDialog';
import { ExecuteSqlFileDialog } from './ExecuteSqlFileDialog';
import { ConnectionWorkspaceHome } from './ConnectionWorkspaceHome';
import { openNewConnectionDialog } from '../../lib/windowManager';
import { openConnectionShareDialog } from '../../lib/connectionShare';

export interface ContentViewProps {
  selectTableRef?: MutableRefObject<
    ((table: string, schema?: string, database?: string) => void) | undefined
  >;
  nodeContextMenuRef?: MutableRefObject<((payload: NodeContextMenuPayload) => void) | undefined>;
  actionsRef?: MutableRefObject<ConnectionViewActions | undefined>;
}

export function ContentView({ selectTableRef, nodeContextMenuRef, actionsRef }: ContentViewProps) {
  const { t } = useI18n();
  const [confirmAction, confirmActionDialog] = useConfirmDialog();
  const safeMode = useSettingsStore((s) => s.settings.safeMode);

  const allPanels = usePanelStore((s) => s.panels);
  const activePanelId = usePanelStore((s) => s.activePanelId);
  const removePanel = usePanelStore((s) => s.removePanel);
  const setActivePanel = usePanelStore((s) => s.setActivePanel);
  const storeUpdatePanel = usePanelStore((s) => s.updatePanel);

  const activePanel = allPanels.find((p) => p.id === activePanelId) ?? null;

  const dbSessionId = activePanel?.dbSessionId ?? '';
  const connectionId = activePanel?.connectionId ?? '';
  const connectionName = activePanel?.connectionName ?? '';
  const databaseType = activePanel?.databaseType as DatabaseType | undefined;

  const connCtx: ConnectionContext | null = useMemo(() => {
    if (!activePanel) return null;
    return {
      connectionId: activePanel.connectionId,
      dbSessionId: activePanel.dbSessionId,
      connectionName: activePanel.connectionName,
      databaseType: activePanel.databaseType,
    };
  }, [
    activePanel?.connectionId,
    activePanel?.dbSessionId,
    activePanel?.connectionName,
    activePanel?.databaseType,
  ]);

  const savedConnections = useConnectionStore((s) => s.connections);
  const hasSavedConnections = savedConnections.length > 0;
  const activeConnections = useActiveConnectionStore((s) => s.connections);
  const storeActiveDbSessionId = useSchemaStore((s) => s.activeDbSessionId);

  const sidebarConnCtx = useMemo(() => {
    if (!storeActiveDbSessionId) return connCtx;
    return (
      resolveConnectionContext(storeActiveDbSessionId, activeConnections, savedConnections) ??
      connCtx
    );
  }, [storeActiveDbSessionId, activeConnections, savedConnections, connCtx]);

  const initialDatabase = useMemo(() => {
    const ctxConnectionId = sidebarConnCtx?.connectionId ?? connectionId;
    if (!ctxConnectionId) return undefined;
    return savedConnections.find((c) => c.id === ctxConnectionId)?.database;
  }, [savedConnections, sidebarConnCtx?.connectionId, connectionId]);

  const toolbarDbType = databaseType ?? (sidebarConnCtx?.databaseType as DatabaseType | undefined);
  const dbMeta = databaseType ? DB_REGISTRY[databaseType] : undefined;
  const toolbarDbMeta = toolbarDbType ? DB_REGISTRY[toolbarDbType] : undefined;
  const showStructureEditor = canOpenStructureEditor(dbMeta) && dbMeta?.readOnly !== true;
  const exportScope = resolveExportScope(dbMeta);
  const toolbarExportScope = resolveExportScope(toolbarDbMeta);
  const batchExportSupported = supportsFullTableExport(toolbarExportScope);

  // Guard: key-value / document panels (Redis, future MongoDB KV) hide SQL-oriented toolbar items.
  // Uses DB_REGISTRY metadata instead of panel type literal so new KV drivers get the same behaviour.
  const isKvPanel =
    (activePanel?.type === 'redis-db') ||
    (toolbarDbMeta?.isKeyValue === true);
  const showNewQuery =
    !isKvPanel && toolbarDbMeta?.supportsSQL !== false && !!toolbarDbType;
  const showNewTable =
    !isKvPanel &&
    canOpenStructureEditor(toolbarDbMeta) &&
    toolbarDbMeta?.readOnly !== true &&
    !!toolbarDbType;
  const showErDiagramToolbar =
    !isKvPanel && toolbarDbMeta?.supportsErDiagram !== false && !!toolbarDbType;
  const showObjectsToolbar =
    !isKvPanel && toolbarDbMeta?.readOnly !== true && !!toolbarDbType;

  // Detect a connection that is still being established (no dbSessionId yet).
  // When no panel is active, ConnectionWorkspaceHome needs to show a spinner
  // instead of the "select a connection" prompt.
  const connectingEntry = useMemo(() => {
    if (activePanel) return null;
    const entries = Object.values(activeConnections);
    return entries.find((e) => e.status === 'connecting') ?? null;
  }, [activePanel, activeConnections]);

  const connectingName = useMemo(() => {
    if (!connectingEntry) return undefined;
    return savedConnections.find((c) => c.id === connectingEntry.connectionId)?.name;
  }, [connectingEntry, savedConnections]);

  const connectingDbType = useMemo(() => {
    if (!connectingEntry) return undefined;
    return savedConnections.find((c) => c.id === connectingEntry.connectionId)?.databaseType as
      | DatabaseType
      | undefined;
  }, [connectingEntry, savedConnections]);

  const recentPanels = useMemo(() => {
    if (!sidebarConnCtx) return [];
    return allPanels
      .filter((panel) => panel.connectionId === sidebarConnCtx.connectionId)
      .slice(-6)
      .reverse();
  }, [allPanels, sidebarConnCtx]);

  const [aiChatOpen, setAiChatOpen] = useState(false);
  const [createDbOpen, setCreateDbOpen] = useState(false);
  const [createSchemaOpen, setCreateSchemaOpen] = useState(false);
  const [createUserOpen, setCreateUserOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [exportTableName, setExportTableName] = useState<string | null>(null);
  const [importTableName, setImportTableName] = useState<string | null>(null);
  const [batchExportOpen, setBatchExportOpen] = useState(false);
  const [batchExportInitialSelected, setBatchExportInitialSelected] = useState<string[]>([]);
  const [lastTableSchema, setLastTableSchema] = useState<string | null>(null);
  const [sqlFileDialogOpen, setSqlFileDialogOpen] = useState(false);

  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const schemaTables = useSchemaStore((s) => s.tables);
  const schemaViews = useSchemaStore((s) => s.views);
  const removeRelation = useSchemaStore((s) => s.removeRelation);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const tableColumns = useTableDataStore((s) => s.columns);
  const tableRows = useTableDataStore((s) => s.rows);
  const totalRows = useTableDataStore((s) => s.totalRows);
  const selectedRows = useTableDataStore((s) => s.selectedRows);
  const tableName = useTableDataStore((s) => s.tableName);
  const setDbType = useTableDataStore((s) => s.setDatabaseType);
  const detailRowIndex = useTableDataStore((s) => s.detailRowIndex);
  const updateCell = useTableDataStore((s) => s.updateCell);
  const applyColumnToRows = useTableDataStore((s) => s.applyColumnToRows);
  const [detailOpen, setDetailOpen] = useState(false);

  const updateQuerySql = usePanelStore((s) => s.updateSql);
  const activeQueryExec = usePanelStore((s) =>
    activePanel?.type === 'query' ? s.queryExec.get(activePanel.id) : undefined,
  );
  const updateResultCell = usePanelStore((s) => s.updateResultCell);

  const detailPanelApplicable =
    activePanel != null &&
    (activePanel.type !== 'table' || activePanel.subTab === 'data') &&
    (activePanel.type !== 'view' || (activePanel as ViewPanel).subTab === 'data');

  const resolveTableSchema = useCallback(
    (table: string): string | null => {
      const hit = [...schemaTables, ...schemaViews].find((tbl) => tbl.name === table);
      return hit?.schema ?? currentDatabase ?? null;
    },
    [schemaTables, schemaViews, currentDatabase],
  );

  const { size: aiSidebarWidth, handleRef: aiHandleRef } = useResizable({
    direction: 'horizontal',
    initialSize: 320,
    minSize: 240,
    maxSize: 600,
    reverse: true,
    storageKey: 'connection.aiSidebar',
  });

  useEffect(() => {
    if (databaseType) setDbType(databaseType);
  }, [databaseType, setDbType]);

  const schemaTreeDbSessionId = sidebarConnCtx?.dbSessionId ?? dbSessionId;
  const schemaTreeDatabaseType = sidebarConnCtx?.databaseType ?? databaseType;

  useEffect(() => {
    if (!schemaTreeDbSessionId || !schemaTreeDatabaseType) return;
    const meta = DB_REGISTRY[schemaTreeDatabaseType];
    void loadForConnection(schemaTreeDbSessionId, {
      preferredDatabase: initialDatabase,
      skipLoadTables: Boolean(meta?.hasMultiDatabase) && !initialDatabase?.trim(),
      databaseType: schemaTreeDatabaseType,
    });
  }, [schemaTreeDbSessionId, schemaTreeDatabaseType, initialDatabase, loadForConnection]);

  const handlers = usePanelHandlers({
    connCtx: sidebarConnCtx,
    showStructureEditor,
    currentDatabase,
    initialDatabase,
    lastTableSchema,
    schemaViews,
  });

  const handleOpenSqlFile = useCallback(() => {
    if (!sidebarConnCtx) return;
    const saved = savedConnections.find((c) => c.id === sidebarConnCtx.connectionId);
    const driverReadOnly = sidebarConnCtx.databaseType
      ? DB_REGISTRY[sidebarConnCtx.databaseType as DatabaseType]?.readOnly === true
      : false;
    if (saved?.readOnly || driverReadOnly || safeMode) {
      return;
    }
    setSqlFileDialogOpen(true);
  }, [safeMode, savedConnections, sidebarConnCtx]);

  const handleSelectTableWithSchema = useCallback(
    (table: string, schema?: string, database?: string) => {
      if (schema) setLastTableSchema(schema);
      handlers.handleSelectTable(table, schema, database);
    },
    [handlers.handleSelectTable],
  );

  useLayoutEffect(() => {
    if (selectTableRef) selectTableRef.current = handleSelectTableWithSchema;
    return () => {
      if (selectTableRef) selectTableRef.current = undefined;
    };
  }, [selectTableRef, handleSelectTableWithSchema]);

  const exportableTableNames = useMemo(() => schemaTables.map((tbl) => tbl.name), [schemaTables]);

  const openBatchExport = useCallback((initialSelected: string[] = []) => {
    setBatchExportInitialSelected(initialSelected);
    setBatchExportOpen(true);
  }, []);

  const handleOpenBatchExportFromToolbar = useCallback(() => {
    const preselected = activePanel?.type === 'table' ? [activePanel.tableName] : [];
    openBatchExport(preselected);
  }, [activePanel, openBatchExport]);

  const loadTableExportData = useCallback(
    (name: string) => {
      const ctx = sidebarConnCtx;
      if (!ctx) return Promise.reject(new Error('No active connection'));
      return loadBatchExportTableData({
        dbSessionId: ctx.dbSessionId,
        tableName: name,
        databaseType: ctx.databaseType,
        includeRows: false,
      });
    },
    [sidebarConnCtx],
  );

  const handleNodeContextMenu = useCallback(
    (payload: SchemaTreeNodeContextMenuPayload) => {
      const ctx = sidebarConnCtx;
      if (!ctx) return;
      const { kind, name, schema } = payload;
      const ctxDbType = ctx.databaseType as DatabaseType;
      const ctxDbMeta = DB_REGISTRY[ctxDbType];
      const saved = savedConnections.find((c) => c.id === ctx.connectionId);
      const ctxIsReadOnly = ctxDbMeta?.readOnly === true || saved?.readOnly === true;
      const ctxShowStructureEditor = canOpenStructureEditor(ctxDbMeta) && !ctxIsReadOnly;
      const ctxSupportsErDiagram = ctxDbMeta?.supportsErDiagram !== false;
      const ctxExportScope = resolveExportScope(ctxDbMeta);
      const ctxExportDataSupported = supportsAnyExport(ctxExportScope);
      const ctxBatchExportSupported = supportsFullTableExport(ctxExportScope);
      const scopedPanels = usePanelStore
        .getState()
        .panels.filter((p) => p.connectionId === ctx.connectionId);

      const copyText = (text: string) => {
        void navigator.clipboard.writeText(text);
      };
      const quoted = escapeIdent(name, ctxDbType);

      const copyDdl = () => {
        const dialect = getSqlDialect(ctxDbType);
        if (!dialect) return;
        const { sql, extractColumnIndex } = dialect.ddl.getTableDdlQuery(name);
        void getCachedDDL(ctx.dbSessionId, name, sql, (rows) => {
          const row = rows[0];
          const val = row?.[extractColumnIndex];
          return typeof val === 'string' ? val : val != null ? String(val) : '';
        })
          .then((ddl) => {
            if (ddl) copyText(ddl);
          })
          .catch((e) => console.warn(e));
      };

      const confirmAndRun = async (
        message: string,
        title: string,
        sql: string,
        afterSuccess?: () => void,
      ) => {
        const confirmed = await confirmAction({ title, message, kind: 'warning' });
        if (!confirmed) return;
        const database = currentDatabase ?? initialDatabase ?? null;
        try {
          await queryCommands.executeQuery(
            ctx.dbSessionId,
            sql,
            undefined,
            database,
            schema ?? null,
          );
          afterSuccess?.();
        } catch (e) {
          console.warn(e);
        }
      };

      const closePanelsForTable = (table: string) => {
        const toClose = scopedPanels.filter((p) => p.type === 'table' && p.tableName === table);
        for (const p of toClose) removePanel(p.id);
      };

      void showNativeContextMenu(
        buildSchemaTreeContextMenuItems({
          kind,
          labels: {
            open: kind === 'view' ? t('schemaTree.open') : t('schemaTree.openTable'),
            openStructure: t('schemaTree.openStructure'),
            copyName: t('common.copyName'),
            copyDdl: t('common.copyDdl'),
            focusEr: t('erDiagram.focusTable'),
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
            dropSchema: t('schemaTree.dropSchema'),
            viewErDiagram: t('schemaTree.viewErDiagram'),
            newSchema: t('schemaTree.newSchema'),
            createSchema: t('common.createSchema'),
            executeSqlFile: t('common.executeSqlFile'),
            dataTransfer: t('common.dataTransfer'),
            compareSchema: t('schemaTree.compareSchema'),
            compareData: t('schemaTree.compareData'),
            backup: t('common.backupDatabase'),
            restore: t('common.restoreDatabase'),
          },
          handlers: {
            onOpen:
              kind === 'table' || kind === 'view'
                ? () => handleSelectTableWithSchema(name, schema)
                : undefined,
            onOpenStructure:
              kind === 'table' ? () => handlers.handleOpenStructure(name) : undefined,
            onCopyName: kind === 'table' || kind === 'view' ? () => copyText(name) : undefined,
            onCopyDdl: kind === 'table' || kind === 'view' ? () => copyDdl() : undefined,
            onFocusEr: kind === 'table' ? () => handlers.handleOpenErDiagram(name) : undefined,
            onExport:
              kind === 'table' || kind === 'view'
                ? () => {
                    setExportTableName(name);
                    handleSelectTableWithSchema(name, schema);
                    setExportOpen(true);
                  }
                : undefined,
            onBatchExport: () => {
              if (kind === 'table' || kind === 'view') {
                openBatchExport([name]);
              } else {
                openBatchExport([]);
              }
            },
            onImport:
              !ctxIsReadOnly && (kind === 'table' || kind === 'database' || kind === 'blank')
                ? () => {
                    setImportTableName(kind === 'table' ? name : null);
                    setImportOpen(true);
                  }
                : undefined,
            onRefresh: handlers.handleRefresh,
            onNewQuery: () => {
              if (kind === 'table') {
                handlers.handleOpenTableAction(
                  {
                    connectionId: ctx.connectionId,
                    dbSessionId: ctx.dbSessionId,
                    databaseType: ctx.databaseType,
                    database: currentDatabase ?? initialDatabase,
                    schema,
                    tableName: name,
                  },
                  'select',
                );
              } else {
                handlers.handleNewQuery();
              }
            },
            onQueryHistory:
              kind === 'database' || kind === 'schema'
                ? () => handlers.handleOpenQueryHistory()
                : undefined,
            onCopyDatabaseName: kind === 'database' ? () => copyText(name) : undefined,
            onBackup:
              kind === 'database' && ctxDbMeta?.supportsBackup
                ? () =>
                    openBackupWindow('backup', { connectionId: ctx.connectionId, database: name })
                : undefined,
            onRestore:
              kind === 'database' && ctxDbMeta?.supportsBackup
                ? () =>
                    openBackupWindow('restore', { connectionId: ctx.connectionId, database: name })
                : undefined,
            onNewTable: handlers.handleCreateTable,
            onTruncate:
              kind === 'table' && !ctxIsReadOnly && !safeMode
                ? () => {
                    const dialect = getSqlDialect(ctxDbType);
                    const sql = dialect?.getTruncateTableSql
                      ? dialect.getTruncateTableSql(quoted)
                      : `TRUNCATE TABLE ${quoted}`;
                    void confirmAndRun(
                      t('schemaTree.confirmTruncate', { name }),
                      t('schemaTree.truncate'),
                      sql,
                      () => {
                        const store = useTableDataStore.getState();
                        if (store.activeTable === name) {
                          void store.loadTableData({
                            dbSessionId: ctx.dbSessionId,
                            table: name,
                            connectionId: ctx.connectionId,
                            driverType: ctx.databaseType,
                            database: currentDatabase,
                            schema,
                          });
                        }
                      },
                    );
                  }
                : undefined,
            onDrop:
              (kind === 'table' || kind === 'view') && !ctxIsReadOnly && !safeMode
                ? () => {
                    const isView = kind === 'view';
                    const sql = isView ? `DROP VIEW ${quoted}` : `DROP TABLE ${quoted}`;
                    void confirmAndRun(
                      t(isView ? 'schemaTree.confirmDropView' : 'schemaTree.confirmDrop', {
                        name,
                      }),
                      t(isView ? 'schemaTree.dropView' : 'schemaTree.drop'),
                      sql,
                      () => {
                        invalidateSchemaCache(ctx.dbSessionId, name);
                        removeRelation(name);
                        handlers.handleRefresh();
                        closePanelsForTable(name);
                      },
                    );
                  }
                : undefined,
          },
          readOnly: ctxIsReadOnly,
          safeMode,
          showOpenStructure: true,
          showErFocus: ctxSupportsErDiagram,
          showExport: kind === 'table' ? false : ctxExportDataSupported,
          showBatchExport: kind === 'table' ? false : ctxBatchExportSupported,
          showNewTable: ctxShowStructureEditor,
        }),
        { x: payload.x, y: payload.y },
      );
    },
    [
      sidebarConnCtx,
      currentDatabase,
      initialDatabase,
      t,
      handleSelectTableWithSchema,
      handlers,
      removeRelation,
      openBatchExport,
      safeMode,
      removePanel,
      confirmAction,
    ],
  );

  useLayoutEffect(() => {
    if (nodeContextMenuRef) {
      nodeContextMenuRef.current = (payload) =>
        handleNodeContextMenu(payload as SchemaTreeNodeContextMenuPayload);
    }
    return () => {
      if (nodeContextMenuRef) nodeContextMenuRef.current = undefined;
    };
  }, [nodeContextMenuRef, handleNodeContextMenu]);

  useLayoutEffect(() => {
    if (actionsRef) {
      actionsRef.current = {
        newQuery: handlers.handleNewQuery,
        openSqlFile: handleOpenSqlFile,
        createTable: handlers.handleCreateTable,
        openCreateDatabase: () => setCreateDbOpen(true),
        openCreateSchema: () => setCreateSchemaOpen(true),
        openCreateUser: () => setCreateUserOpen(true),
        openErDiagram: handlers.handleOpenErDiagram,
        refresh: handlers.handleRefresh,
        openObject: handlers.handleOpenDbObject,
        openTableAction: handlers.handleOpenTableAction,
        openQueryHistory: handlers.handleOpenQueryHistory,
        openServerStatus: handlers.handleOpenServerStatus,
        openProcessList: handlers.handleOpenProcessList,
      };
    }
    return () => {
      if (actionsRef) actionsRef.current = undefined;
    };
  }, [actionsRef, handlers, handleOpenSqlFile]);

  useKeyboardShortcuts([
    {
      key: 'mod+n',
      scope: 'global',
      description: t('common.newQuery'),
      action: () => handlers.handleNewQuery(),
    },
    {
      key: 'mod+w',
      scope: 'global',
      description: t('common.close'),
      action: () => {
        if (activePanelId) handlers.handleClosePanel(activePanelId);
      },
    },
  ]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== ' ' || e.repeat) return;
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      if (
        tag === 'input' ||
        tag === 'textarea' ||
        tag === 'select' ||
        tag === 'button' ||
        target?.isContentEditable === true
      ) {
        return;
      }
      if (!detailPanelApplicable) return;
      e.preventDefault();
      setDetailOpen((v) => !v);
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [detailPanelApplicable]);

  const activeQueryResult =
    activeQueryExec && activeQueryExec.results.length > 0
      ? (activeQueryExec.results[activeQueryExec.activeResultIdx] ?? null)
      : null;

  // KV / document panels own their logical database context. The schema store tracks
  // the outer SQL navigator and may still point at db0 after a KV panel was
  // opened on db5, so the status bar must use the panel's immutable target.
  const statusDatabase = dbMeta?.isKeyValue === true && activePanel
    ? (activePanel as { dbName?: string }).dbName ?? currentDatabase
    : currentDatabase;

  const detailColumnDefs: ColumnDef[] = useMemo(() => {
    if (activePanel?.type === 'table') {
      return tableColumns.map((c) => ({ id: c.name, name: c.name, type: c.dataType }));
    }
    if (activeQueryResult) {
      return activeQueryResult.columns.map((c) => ({ id: c.name, name: c.name, type: c.dataType }));
    }
    return [];
  }, [activePanel?.type, tableColumns, activeQueryResult]);

  const resultDetailRowIndex = activeQueryExec?.resultDetailRowIndex ?? null;
  const detailRowIdx = activePanel?.type === 'table' ? detailRowIndex : resultDetailRowIndex;

  const detailRow: Record<string, unknown> | null = useMemo(() => {
    if (activePanel?.type === 'table') {
      return detailRowIndex !== null && detailRowIndex < tableRows.length
        ? tableRows[detailRowIndex]
        : null;
    }
    if (
      activeQueryResult &&
      resultDetailRowIndex !== null &&
      resultDetailRowIndex < activeQueryResult.rows.length
    ) {
      return rowToRecord(activeQueryResult.rows[resultDetailRowIndex], activeQueryResult.columns);
    }
    return null;
  }, [activePanel?.type, detailRowIndex, tableRows, activeQueryResult, resultDetailRowIndex]);

  const handleDetailFieldEdit = useCallback(
    (row: number, col: string, value: unknown) => {
      if (activePanel?.type === 'table') {
        if (selectedRows.size > 1) {
          applyColumnToRows(col, value, [...selectedRows]);
        } else {
          updateCell(row, col, value);
        }
      } else if (activePanel?.type === 'query' && activeQueryExec) {
        updateResultCell(activePanel.id, activeQueryExec.activeResultIdx, row, col, value);
      }
    },
    [activePanel, activeQueryExec, updateCell, updateResultCell, applyColumnToRows, selectedRows],
  );

  return (
    <>
      {activePanel && (
        <ContentToolbar
          showNewQuery={showNewQuery}
          showNewTable={showNewTable}
          showErDiagram={showErDiagramToolbar}
          showObjects={showObjectsToolbar}
          showBatchExport={batchExportSupported}
          aiChatOpen={aiChatOpen}
          detailPanelApplicable={detailPanelApplicable}
          detailOpen={detailOpen}
          onNewQuery={() => handlers.handleNewQuery()}
          onCreateTable={handlers.handleCreateTable}
          onOpenErDiagram={() => handlers.handleOpenErDiagram()}
          onOpenObjects={handlers.handleOpenObjects}
          onOpenPrivileges={handlers.handleOpenPrivileges}
          onBatchExport={handleOpenBatchExportFromToolbar}
          onToggleAiChat={() => setAiChatOpen((v) => !v)}
          onToggleDetail={() => setDetailOpen((p) => !p)}
          onRefresh={handlers.handleRefresh}
        />
      )}

      <PanelTabBar
        panels={allPanels}
        activePanelId={activePanelId}
        onSelectPanel={setActivePanel}
        onClosePanel={handlers.handleClosePanel}
        onContextMenu={handlers.handlePanelTabContextMenu}
      />

      <div className="flex min-h-0 flex-1">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {!activePanel ? (
            <ConnectionWorkspaceHome
              hasConnections={hasSavedConnections}
              connectionContext={sidebarConnCtx}
              recentPanels={recentPanels}
              showNewQuery={showNewQuery}
              showNewTable={showNewTable}
              showErDiagram={showErDiagramToolbar}
              showObjects={showObjectsToolbar}
              isConnecting={!!connectingEntry}
              connectingName={connectingName}
              connectingDbType={connectingDbType}
              onNewConnection={() => openNewConnectionDialog()}
              onImportConnections={() => openConnectionShareDialog('import')}
              onNewQuery={() => handlers.handleNewQuery()}
              onCreateTable={handlers.handleCreateTable}
              onOpenErDiagram={() => handlers.handleOpenErDiagram()}
              onOpenObjects={handlers.handleOpenObjects}
              onOpenPanel={setActivePanel}
            />
          ) : (
            <PanelContentRenderer
              activePanel={activePanel}
              currentDatabase={currentDatabase}
              lastTableSchema={lastTableSchema}
              onSetSubTab={handlers.handleSetSubTab}
              onExitStructureEditing={handlers.handleExitStructureEditing}
              onEditTableStructure={handlers.handleEditTableStructure}
              onSelectTable={handleSelectTableWithSchema}
              onOpenErDiagram={handlers.handleOpenErDiagram}
              onClosePanel={handlers.handleClosePanel}
              onRefresh={handlers.handleRefresh}
              resolveTableSchema={resolveTableSchema}
              onUpdatePanelData={(id, data) =>
                storeUpdatePanel(id, data as Parameters<typeof storeUpdatePanel>[1])
              }
            />
          )}
        </div>

        {detailPanelApplicable && (
          <DetailPanel
            open={detailOpen}
            columns={detailColumnDefs}
            row={detailRow}
            rowIndex={detailRowIdx}
            selectedRows={
              activePanel?.type === 'table' || activePanel?.type === 'view'
                ? selectedRows
                : undefined
            }
            editable
            onFieldEdit={handleDetailFieldEdit}
          />
        )}

        {aiChatOpen && dbSessionId && (
          <>
            <div
              ref={aiHandleRef}
              className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/30"
            />
            <aside
              style={{ width: aiSidebarWidth }}
              className="shrink-0 border-l border-edge bg-surface"
            >
              <AiChatPanel
                dbSessionId={dbSessionId}
                database={currentDatabase ?? undefined}
                sqlDialect={databaseType ? DB_REGISTRY[databaseType]?.sqlDialect : undefined}
                onInsertSql={(sql) => {
                  if (activePanel?.type === 'query') {
                    updateQuerySql(activePanel.id, sql);
                  }
                }}
              />
            </aside>
          </>
        )}
      </div>

      {activePanel && (
        <ContentStatusBar
          databaseType={databaseType}
          connectionName={connectionName}
          currentDatabase={statusDatabase}
          tableName={tableName ?? ''}
          columnCount={tableColumns.length}
          totalRows={totalRows}
        />
      )}

      {exportOpen && exportTableName && sidebarConnCtx && (
        <ExportDialog
          open={exportOpen}
          onClose={() => {
            setExportOpen(false);
            setExportTableName(null);
          }}
          tableName={exportTableName}
          columns={tableColumns}
          rows={tableRows}
          selectedRows={selectedRows}
          databaseType={sidebarConnCtx.databaseType}
          dbSessionId={sidebarConnCtx.dbSessionId}
          totalRows={totalRows}
          defaultScope="entire_table"
          dataExportCapability={exportScope}
        />
      )}

      {sidebarConnCtx && (
        <BatchExportDialog
          open={batchExportOpen}
          onClose={() => {
            setBatchExportOpen(false);
            setBatchExportInitialSelected([]);
          }}
          dbSessionId={sidebarConnCtx.dbSessionId}
          databaseType={sidebarConnCtx.databaseType}
          database={currentDatabase ?? undefined}
          tables={exportableTableNames}
          initialSelected={batchExportInitialSelected}
          loadTableExportData={loadTableExportData}
          dataExportCapability={exportScope}
        />
      )}

      {sidebarConnCtx && (
        <ImportDialog
          open={importOpen}
          onClose={() => {
            setImportOpen(false);
            setImportTableName(null);
          }}
          dbSessionId={sidebarConnCtx.dbSessionId}
          tableName={importTableName}
          onImported={handlers.handleRefresh}
          databaseType={sidebarConnCtx.databaseType}
        />
      )}

      {confirmActionDialog}

      {sidebarConnCtx && (
        <ExecuteSqlFileDialog
          open={sqlFileDialogOpen}
          onClose={() => setSqlFileDialogOpen(false)}
          dbSessionId={sidebarConnCtx.dbSessionId}
          database={currentDatabase ?? initialDatabase ?? null}
          connectionName={sidebarConnCtx.connectionName}
          onExecuted={handlers.handleRefresh}
        />
      )}

      {sidebarConnCtx && (
        <CreateDatabaseDialog
          open={createDbOpen}
          onClose={() => setCreateDbOpen(false)}
          dbSessionId={sidebarConnCtx.dbSessionId}
          onCreated={async () => {
            const sessionId = sidebarConnCtx.dbSessionId;
            const dbType = sidebarConnCtx.databaseType;
            await loadForConnection(sessionId, {
              preferredDatabase: initialDatabase,
              databaseType: dbType,
              skipLoadTables: true,
            });
          }}
        />
      )}

      {sidebarConnCtx && (
        <CreateSchemaDialog
          open={createSchemaOpen}
          onClose={() => setCreateSchemaOpen(false)}
          dbSessionId={sidebarConnCtx.dbSessionId}
          database={currentDatabase ?? initialDatabase ?? null}
          onCreated={async () => {
            const sessionId = sidebarConnCtx.dbSessionId;
            const db = currentDatabase ?? initialDatabase;
            if (db) {
              await useSchemaStore.getState().loadTables(db, sessionId);
            }
            await loadForConnection(sessionId, {
              preferredDatabase: initialDatabase,
              databaseType: sidebarConnCtx.databaseType,
              skipLoadTables: false,
            });
          }}
        />
      )}

      {sidebarConnCtx && (
        <CreateUserDialog
          open={createUserOpen}
          onClose={() => setCreateUserOpen(false)}
          dbSessionId={sidebarConnCtx.dbSessionId}
          onCreated={() => {
            const ctx = sidebarConnCtx;
            const store = usePanelStore.getState();
            const existingPriv = store.panels.find(
              (p) => p.type === 'privileges' && p.connectionId === ctx.connectionId,
            );
            if (existingPriv) {
              store.removePanel(existingPriv.id);
            }
            const panel = {
              ...ctx,
              type: 'privileges' as const,
              id: `priv-${Date.now()}`,
            };
            store.addPanel(panel);
          }}
        />
      )}
    </>
  );
}
