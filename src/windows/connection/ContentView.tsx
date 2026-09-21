import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MutableRefObject,
} from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { usePanelStore, type ViewPanel } from '../../stores/panelStore';
import { useQueryBuilderStore } from '../../stores/queryBuilderStore';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { ContentToolbar } from './ContentToolbar';
import { PanelTabBar } from './PanelTabBar';
import { ContentStatusBar } from './ContentStatusBar';
import { PanelContentRenderer } from './PanelContentRenderer';
import { usePanelHandlers } from './usePanelHandlers';
import { useConnectionContextMenu } from './useConnectionContextMenu';
import { useConnectionWorkspaceMeta } from './useConnectionWorkspaceMeta';
import { ContentViewDialogs } from './ContentViewDialogs';
import { ContentViewDrawers } from './ContentViewDrawers';
import { ConnectionWorkspaceHome } from './ConnectionWorkspaceHome';
import { openHistoryQuery } from './openHistoryQuery';
import { openNewConnectionDialog } from '../../lib/windowManager';
import { openConnectionShareDialog } from '../../lib/connectionShare';
import { getActionShortcut, toShortcutHookFormat } from '../../lib/keymap';
import type {
  ConnectionViewActions,
  NodeContextMenuPayload,
} from '../../lib/connectionViews/types';
import type { DatabaseType } from '../../types';
import type { SchemaTreeNodeContextMenuPayload } from '../../lib/schemaTreeContextMenu';
import type { AiChatDraftRequest, ContentViewCallbacks } from './query/aiDraftBridge';

export interface ContentViewProps {
  selectTableRef?: MutableRefObject<
    ((table: string, schema: string | null, database: string) => void) | undefined
  >;
  nodeContextMenuRef?: MutableRefObject<((payload: NodeContextMenuPayload) => void) | undefined>;
  actionsRef?: MutableRefObject<ConnectionViewActions | undefined>;
  onSelectConnection?: (connectionId: string) => void;
}

export function ContentView({
  selectTableRef,
  nodeContextMenuRef,
  actionsRef,
  onSelectConnection,
}: ContentViewProps) {
  const { t } = useI18n();
  const safeMode = useSettingsStore((s) => s.settings.safeMode);

  const allPanels = usePanelStore((s) => s.panels);
  const activePanelId = usePanelStore((s) => s.activePanelId);
  const setActivePanel = usePanelStore((s) => s.setActivePanel);
  const storeUpdatePanel = usePanelStore((s) => s.updatePanel);

  const activePanel = allPanels.find((p) => p.id === activePanelId) ?? null;

  const savedConnections = useConnectionStore((s) => s.connections);
  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const schemaTables = useSchemaStore((s) => s.tables);
  const schemaViews = useSchemaStore((s) => s.views);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const tableColumns = useTableDataStore((s) => s.columns);
  const tableRows = useTableDataStore((s) => s.rows);
  const totalRows = useTableDataStore((s) => s.totalRows);
  const selectedRows = useTableDataStore((s) => s.selectedRows);
  const tableName = useTableDataStore((s) => s.tableName);
  const setDbType = useTableDataStore((s) => s.setDatabaseType);

  const {
    sidebarConnCtx,
    initialDatabase,
    databaseType,
    dbSessionId,
    connectionName,
    hasSavedConnections,
    showStructureEditor,
    exportScope,
    batchExportSupported,
    showNewQuery,
    showNewTable,
    showErDiagramToolbar,
    showObjectsToolbar,
    connectingEntry,
    connectingName,
    connectingDbType,
    recentPanels,
    statusDatabase,
  } = useConnectionWorkspaceMeta(activePanel);

  const [aiChatOpen, setAiChatOpen] = useState(false);
  // ── S3-B2: AI draft bridge ──────────────────────────────────────────────
  const pendingDraftRef = useRef<AiChatDraftRequest | null>(null);
  const [pendingDraftRequest, setPendingDraftRequest] = useState<AiChatDraftRequest | null>(null);

  const openAiChatDraft = useCallback((request: AiChatDraftRequest) => {
    pendingDraftRef.current = request;
    setPendingDraftRequest(request);
    setAiChatOpen(true);
  }, []);

  const handleDraftConsumed = useCallback((requestId: string) => {
    if (pendingDraftRef.current?.requestId === requestId) {
      pendingDraftRef.current = null;
      setPendingDraftRequest(null);
    }
  }, []);

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
  const [detailOpen, setDetailOpen] = useState(false);

  const detailPanelApplicable =
    activePanel != null &&
    (activePanel.type !== 'table' || activePanel.subTab === 'data') &&
    (activePanel.type !== 'view' || (activePanel as ViewPanel).subTab === 'data');

  // The schema of a relation, for per-table metadata reads. Never falls back to
  // `currentDatabase`: that is a *database*, and a schema-aware driver would
  // resolve the table in the wrong namespace while a schema-less driver would
  // reject the argument outright. `null` means "let the host/driver decide".
  const resolveTableSchema = useCallback(
    (table: string): string | null => {
      const hit = [...schemaTables, ...schemaViews].find((tbl) => tbl.name === table);
      return hit?.schema ?? null;
    },
    [schemaTables, schemaViews],
  );

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

  // The visual builder belongs to the query panel that opened it, so it is torn
  // down when that panel is closed — not when the component unmounts, because
  // switching tabs unmounts the inactive panel and its canvas must survive that.
  const destroyQbForPanel = useQueryBuilderStore((s) => s.destroyFor);
  const knownPanelIdsRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    const liveIds = new Set(allPanels.map((p) => p.id));
    for (const id of knownPanelIdsRef.current) {
      if (!liveIds.has(id)) destroyQbForPanel(id);
    }
    knownPanelIdsRef.current = liveIds;
  }, [allPanels, destroyQbForPanel]);

  // Keep the session-level `currentDatabase` aligned with the ACTIVE panel's
  // bound database. Without this, loadForConnection/schema-tree defaults can
  // re-pin it to the first database on a reload (e.g. Settings round-trip)
  // while the active query tab is still targeting another database — the store
  // drifts even though the panel itself is correct. Driving it from the panel
  // also makes switching tabs instantly reflect the selected tab's database.
  const activePanelBoundDatabase = (activePanel as { database?: string } | null)?.database;
  useEffect(() => {
    if (!activePanel?.dbSessionId || !activePanelBoundDatabase?.trim()) return;
    useSchemaStore.getState().setCurrentDatabase(activePanelBoundDatabase, activePanel.dbSessionId);
  }, [activePanel?.id, activePanel?.dbSessionId, activePanelBoundDatabase]);

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
    (
      table: string,
      schema: string | null,
      database: string,
      subTab?: 'data' | 'structure' | 'ddl',
      targetColumn?: string,
    ) => {
      if (schema) setLastTableSchema(schema);
      handlers.handleSelectTable(table, schema, database, subTab, targetColumn);
    },
    [handlers.handleSelectTable],
  );

  useLayoutEffect(() => {
    if (selectTableRef) selectTableRef.current = handleSelectTableWithSchema;
    return () => {
      if (selectTableRef) selectTableRef.current = undefined;
    };
  }, [selectTableRef, handleSelectTableWithSchema]);

  // ── S3-B2: compile callbacks after handleSelectTableWithSchema is defined ──
  const callbacks: ContentViewCallbacks = useMemo(
    () => ({
      openRelation: handleSelectTableWithSchema,
      openAiChatDraft,
    }),
    [handleSelectTableWithSchema, openAiChatDraft],
  );

  const exportableTableNames = useMemo(() => schemaTables.map((tbl) => tbl.name), [schemaTables]);

  const openBatchExport = useCallback((initialSelected: string[] = []) => {
    setBatchExportInitialSelected(initialSelected);
    setBatchExportOpen(true);
  }, []);

  const handleOpenBatchExportFromToolbar = useCallback(() => {
    const preselected = activePanel?.type === 'table' ? [activePanel.tableName] : [];
    openBatchExport(preselected);
  }, [activePanel, openBatchExport]);

  // Dialog-trigger callbacks reused by the node context menu (export/import).
  const requestExport = useCallback(
    (name: string, schema: string | null, database: string) => {
      setExportTableName(name);
      handleSelectTableWithSchema(name, schema, database);
      setExportOpen(true);
    },
    [setExportTableName, handleSelectTableWithSchema, setExportOpen],
  );

  const requestImport = useCallback(
    (isTable: boolean, name: string) => {
      setImportTableName(isTable ? name : null);
      setImportOpen(true);
    },
    [setImportTableName, setImportOpen],
  );

  const { handleNodeContextMenu, confirmActionDialog } = useConnectionContextMenu({
    sidebarConnCtx,
    currentDatabase,
    initialDatabase,
    handleSelectTableWithSchema,
    handlers,
    openBatchExport,
    safeMode,
    requestExport,
    requestImport,
  });

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

  const keymapPreset = useSettingsStore((s) => s.settings.keymapPreset);
  const customKeymap = useSettingsStore((s) => s.settings.customKeymap);
  const newQueryKey = toShortcutHookFormat(
    getActionShortcut('newQuery', keymapPreset, customKeymap),
  );
  const closeTabKey = toShortcutHookFormat(
    getActionShortcut('closeTab', keymapPreset, customKeymap),
  );

  useKeyboardShortcuts([
    {
      key: newQueryKey,
      scope: 'global',
      description: t('common.newQuery'),
      action: () => handlers.handleNewQuery(),
    },
    {
      key: closeTabKey,
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

  const closeExport = useCallback(() => {
    setExportOpen(false);
    setExportTableName(null);
  }, []);
  const closeBatchExport = useCallback(() => {
    setBatchExportOpen(false);
    setBatchExportInitialSelected([]);
  }, []);
  const closeImport = useCallback(() => {
    setImportOpen(false);
    setImportTableName(null);
  }, []);
  const closeSqlFile = useCallback(() => setSqlFileDialogOpen(false), []);
  const closeCreateDb = useCallback(() => setCreateDbOpen(false), []);
  const closeCreateSchema = useCallback(() => setCreateSchemaOpen(false), []);
  const closeCreateUser = useCallback(() => setCreateUserOpen(false), []);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
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
              onSelectConnection={onSelectConnection}
              onOpenQueryHistory={handlers.handleOpenQueryHistory}
              onSelectHistoryQuery={(entry) => {
                openHistoryQuery(entry, {
                  onSelectConnection,
                  currentConnectionId: sidebarConnCtx?.connectionId,
                });
              }}
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
              callbacks={callbacks}
            />
          )}
        </div>

        <ContentViewDrawers
          activePanel={activePanel}
          detailOpen={detailOpen}
          aiChatOpen={aiChatOpen}
          detailPanelApplicable={detailPanelApplicable}
          dbSessionId={dbSessionId}
          currentDatabase={currentDatabase}
          databaseType={databaseType}
          pendingDraftRequest={pendingDraftRequest}
          onDraftConsumed={handleDraftConsumed}
        />
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

      <ContentViewDialogs
        connCtx={sidebarConnCtx}
        currentDatabase={currentDatabase}
        initialDatabase={initialDatabase}
        exportCapability={exportScope}
        exportOpen={exportOpen}
        exportTableName={exportTableName}
        onCloseExport={closeExport}
        tableColumns={tableColumns}
        tableRows={tableRows}
        selectedRows={selectedRows}
        totalRows={totalRows}
        batchExportOpen={batchExportOpen}
        batchExportInitialSelected={batchExportInitialSelected}
        onCloseBatchExport={closeBatchExport}
        exportableTableNames={exportableTableNames}
        importOpen={importOpen}
        importTableName={importTableName}
        onCloseImport={closeImport}
        onImported={handlers.handleRefresh}
        sqlFileDialogOpen={sqlFileDialogOpen}
        onCloseSqlFile={closeSqlFile}
        onExecuted={handlers.handleRefresh}
        createDbOpen={createDbOpen}
        onCloseCreateDb={closeCreateDb}
        createSchemaOpen={createSchemaOpen}
        onCloseCreateSchema={closeCreateSchema}
        createUserOpen={createUserOpen}
        onCloseCreateUser={closeCreateUser}
      />

      {confirmActionDialog}
    </div>
  );
}
