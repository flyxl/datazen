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
import { useSchemaStore, useConnectionSchemaField } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { usePanelStore, type ViewPanel } from '../../stores/panelStore';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { ContentToolbar } from './ContentToolbar';
import { PanelTabBar } from './PanelTabBar';
import { ContentStatusBar } from './ContentStatusBar';
import { PanelContentRenderer } from './PanelContentRenderer';
import { useQueryBuilderContribution } from './query/useQueryBuilderContribution';
import { usePanelHandlers } from './usePanelHandlers';
import { useConnectionContextMenu } from './useConnectionContextMenu';
import { useConnectionWorkspaceMeta } from './useConnectionWorkspaceMeta';
import { useKvWorkspaceSlots } from './useKvWorkspaceSlots';
import { useKvSlotActions } from './useKvSlotActions';
import { pruneKvSlotStates } from '../../lib/kvSlotState';
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
import type { ColumnSchema, DatabaseType } from '../../types';
import type { SchemaTreeNodeContextMenuPayload } from '../../lib/schemaTreeContextMenu';
import type { AiChatDraftRequest, ContentViewCallbacks } from './query/aiDraftBridge';

const NO_COLUMNS: ColumnSchema[] = [];
const NO_ROWS: Record<string, unknown>[] = [];
const NO_SELECTION: Set<number> = new Set();

export interface ContentViewProps {
  selectTableRef?: MutableRefObject<
    ((table: string, schema: string | null, database: string) => void) | undefined
  >;
  nodeContextMenuRef?: MutableRefObject<((payload: NodeContextMenuPayload) => void) | undefined>;
  actionsRef?: MutableRefObject<ConnectionViewActions | undefined>;
  onSelectConnection?: (connectionId: string) => void;
  /**
   * Bind the workspace to a KV driver's logical database.
   *
   * This is `ConnectionPage`'s `handleSelectKvDb` — the very callback the
   * navigation tree already calls — threaded down (not re-implemented) so the
   * driver context bar's db selector reaches the same single
   * activate-or-open implementation. It is handed to {@link useKvSlotActions} as
   * `onSelectDatabase`, with the connection supplied here because only this
   * layer knows which one the active panel belongs to.
   */
  onSelectKvDb?: (
    connectionId: string,
    dbName: string,
    pendingAction?: import('../../stores/panelStore').RedisPendingAction,
  ) => void;
}

export function ContentView({
  selectTableRef,
  nodeContextMenuRef,
  actionsRef,
  onSelectConnection,
  onSelectKvDb,
}: ContentViewProps) {
  const { t } = useI18n();
  const { contribution: queryBuilder } = useQueryBuilderContribution();
  const safeMode = useSettingsStore((s) => s.settings.safeMode);

  const allPanels = usePanelStore((s) => s.panels);
  const activePanelId = usePanelStore((s) => s.activePanelId);
  const setActivePanel = usePanelStore((s) => s.setActivePanel);
  const storeUpdatePanel = usePanelStore((s) => s.updatePanel);
  // Pane that editor actions of the tab on screen route to; `null` = that tab
  // has not been split. This mirror is derived per tab by the store
  // (`focusedPaneIdByPanel[activePanelId]`, written only by `syncPaneFocus`),
  // so the pane handed to a panel is always one of *its* panes and never a
  // sibling tab's — see `paneFocusScope.tester.test.ts`.
  const focusedPaneId = usePanelStore((s) => s.focusedPaneId);

  const activePanel = allPanels.find((p) => p.id === activePanelId) ?? null;

  const savedConnections = useConnectionStore((s) => s.connections);
  const currentDatabase = useConnectionSchemaField(
    activePanel?.dbSessionId ?? '',
    'currentDatabase',
  );
  const schemaTables = useConnectionSchemaField(activePanel?.dbSessionId ?? '', 'tables');
  const schemaViews = useConnectionSchemaField(activePanel?.dbSessionId ?? '', 'views');
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);

  const isTablePanel = activePanel?.type === 'table' || activePanel?.type === 'view';
  const tableSlice = useTableDataStore((s) =>
    isTablePanel && activePanelId ? s.byPanel.get(activePanelId) : undefined,
  );
  const tableColumns = tableSlice?.columns ?? NO_COLUMNS;
  const tableRows = tableSlice?.rows ?? NO_ROWS;
  const totalRows = tableSlice?.totalRows ?? 0;
  const selectedRows = tableSlice?.selectedRows ?? NO_SELECTION;
  const tableName =
    activePanel?.type === 'table'
      ? activePanel.tableName
      : activePanel?.type === 'view'
        ? (activePanel as ViewPanel).viewName
        : undefined;

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
    isKvPanel,
    connectionId,
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

  const closeDetail = useCallback(() => setDetailOpen(false), []);

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
  const knownPanelIdsRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    const liveIds = new Set(allPanels.map((p) => p.id));
    for (const id of knownPanelIdsRef.current) {
      if (!liveIds.has(id)) queryBuilder?.destroyFor(id);
    }
    knownPanelIdsRef.current = liveIds;
  }, [allPanels, queryBuilder]);

  // Table-data slices live and die with their panel; prune the ones left behind
  // when a tab (or a whole connection) closes.
  useEffect(() => {
    const store = useTableDataStore.getState();
    const liveIds = new Set(allPanels.map((p) => p.id));
    for (const panelId of store.byPanel.keys()) {
      if (!liveIds.has(panelId)) store.removePanel(panelId);
    }
    pruneKvSlotStates(liveIds);
  }, [allPanels]);

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
    resolveTableSchema,
  });

  // The single KV action dispatcher sits on this side of the boundary: a context
  // bar asks, the host decides (and ignores what it cannot do). See useKvSlotActions.
  //
  // `selectDatabase` resolves against the connection the ACTIVE panel belongs to
  // (`connectionId`, already resolved by useConnectionWorkspaceMeta). Bound here
  // rather than inside the dispatcher because the panel is the only thing that
  // knows the connection, and the callback itself stays ConnectionPage's.
  //
  // `undefined` when the host never handed `onSelectKvDb` down — that is what
  // keeps the dispatcher's documented no-op + warning path reachable (a host
  // without the callback must degrade, not silently call a hollow wrapper).
  const selectKvDatabase = useMemo(
    () =>
      onSelectKvDb
        ? (
            database: string,
            pendingAction?: import('../../stores/panelStore').RedisPendingAction,
          ) => {
            // sidebarConnCtx carries the active connection identity derived from the
            // running db session (even when no panel is open — the overview home).
            // `connectionId` falls back to '' when activePanel is null, which would
            // cause handleSelectKvDb to bail out silently.  Use sidebarConnCtx first.
            onSelectKvDb(sidebarConnCtx?.connectionId ?? connectionId, database, pendingAction);
          }
        : undefined,
    [onSelectKvDb, connectionId, sidebarConnCtx?.connectionId],
  );
  const kvActions = useKvSlotActions({
    onRefresh: handlers.handleRefresh,
    onSelectDatabase: selectKvDatabase,
  });

  // Driver-contributable KV surfaces (context bar / status bar / key-props sidebar /
  // connection home). Every binding stays `undefined` unless the driver both declares
  // the capability and contributed a component, so non-KV drivers render exactly
  // as before.
  const kvSlots = useKvWorkspaceSlots({
    activePanel,
    isKvPanel,
    databaseType,
    database: statusDatabase,
    dbSessionId,
    connectionId,
    connectionName,
    connectionContext: sidebarConnCtx,
    initialDatabase,
    onSlotAction: kvActions.request,
    onSelectKvDb: selectKvDatabase,
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
        openTableStructure: handlers.handleOpenStructure,
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
      {/*
        The 48px toolbar band is skipped entirely for KV panels.

        Every SQL affordance it carries is already false there (a key-value
        driver supports no query/table/ER/objects), and the one slot that used
        to fill it — the driver's `contextBar` — was removed rather than
        re-styled: the db selector and key/memory counters restated facts the
        key browser and status bar already show, and its action buttons belong
        to the panel, not to a host row shared with every other driver. The
        panel's own controls live in its tab bar, so a KV panel now starts at
        the panel tab bar with no empty band above it.

        Side effect, deliberate: the band's AI-chat button and detail-panel
        toggle (the latter opens a KV driver's key-props sidebar) lose their
        entry point along with the band. Both features are untouched in code.
      */}
      {activePanel && !isKvPanel && (
        <ContentToolbar
          showNewQuery={showNewQuery}
          showNewTable={showNewTable}
          showErDiagram={showErDiagramToolbar}
          showObjects={showObjectsToolbar}
          showBatchExport={batchExportSupported}
          aiChatOpen={aiChatOpen}
          detailPanelApplicable={detailPanelApplicable}
          detailOpen={detailOpen}
          contextBarSlot={kvSlots.contextBar}
          kvPanelState={kvSlots.panelState}
          onNewQuery={() => handlers.handleNewQuery()}
          onCreateTable={handlers.handleCreateTable}
          onOpenErDiagram={() => handlers.handleOpenErDiagram()}
          onOpenObjects={handlers.handleOpenObjects}
          onOpenPrivileges={handlers.handleOpenPrivileges}
          onBatchExport={handleOpenBatchExportFromToolbar}
          onToggleAiChat={() => setAiChatOpen((v) => !v)}
          onToggleDetail={() => setDetailOpen((p) => !p)}
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
              connectionHomeSlot={kvSlots.connectionHome}
            />
          ) : (
            <PanelContentRenderer
              activePanel={activePanel}
              focusedPaneId={focusedPaneId}
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
              kvSlotState={kvSlots.panelState}
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
          connectionName={connectionName}
          currentDatabase={currentDatabase}
          databaseType={databaseType}
          kvPanelState={kvSlots.panelState}
          onCloseDetail={closeDetail}
          keyPropsSidebarSlot={kvSlots.keyPropsSidebar}
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
          statusBarSlot={kvSlots.statusBar}
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
      {kvActions.dialog}
    </div>
  );
}
