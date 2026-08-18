import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  type MouseEvent,
  type MutableRefObject,
  type ReactNode,
} from 'react';
import {
  BookOpen,
  Braces,
  Code2,
  Database,
  Download,
  Eye,
  GitFork,
  Hash,
  KeyRound,
  MessageSquare,
  Plus,
  Shapes,
  Table2,
  TableProperties,
  X,
  Zap,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useResizable } from '../../hooks/useResizable';
import { useI18n } from '../../hooks/useI18n';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useQueryStore } from '../../stores/queryStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import {
  usePanelStore,
  nextPanelId,
  type SubTabId,
  type TablePanel,
  type ViewPanel,
  type QueryPanel as QueryPanelType,
  type CreateTablePanel,
  type ErDiagramPanel,
  type ObjectsPanel,
  type PrivilegesPanel,
  type DatabaseObjectPanel,
  type RedisDbPanel,
  type ConnectionContext,
  type Panel,
} from '../../stores/panelStore';
import { cn } from '../../lib/cn';
import { openDocsWindow } from '../../lib/windowManager';
import { DB_REGISTRY, escapeIdent, getDbLabel } from '../../lib/databaseTypes';
import { canOpenStructureEditor } from '../../lib/structureEditor/canOpenStructureEditor';
import {
  resolveExportScope,
  supportsAnyExport,
  supportsFullTableExport,
} from '../../lib/exportCapability';
import { resolveCreateTableSchema } from '../../lib/structureEditor/resolveCreateTableSchema';
import { getCachedDDL, invalidateSchemaCache } from '../../lib/schemaCache';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { buildConnectionTabContextMenuItems } from '../../lib/connectionTabContextMenu';
import { buildSchemaTreeContextMenuItems } from '../../lib/schemaTreeContextMenu';
import { getSqlDialect } from '../../lib/sqlDialects';
import { queryCommands } from '../../commands/query';
import { getConnectionView } from '../../lib/connectionViews';
import type {
  ConnectionViewActions,
  NodeContextMenuPayload,
} from '../../lib/connectionViews/types';
import type { DatabaseType } from '../../types';
import { SchemaTree, type SchemaTreeNodeContextMenuPayload } from './schema-tree/SchemaTree';
import { StructureView } from './StructureView';
import { TableView } from './TableView';
import { IndexesView } from './IndexesView';
import { ForeignKeysView } from './ForeignKeysView';
import { DDLView } from './DDLView';
import { QueryPanel } from './QueryPanel';
import { ExportDialog } from './ExportDialog';
import { BatchExportDialog } from './BatchExportDialog';
import { ImportDialog } from './ImportDialog';
import { loadBatchExportTableData } from '../../lib/loadBatchExportTable';
import { TableStructureEditor } from './TableStructureEditor';
import type { TranslationKey } from '../../locales';
import { DetailPanel } from '../../components/DataTable/DetailPanel';
import { DetailPanelToggle } from '../../components/DataTable/DetailPanelToggle';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { AiChatPanel } from '../../components/ai/AiChatPanel';
import { rowToRecord } from '../../lib/rowToRecord';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { ErDiagramView } from './ErDiagramView';
import { ObjectBrowser } from './ObjectBrowser';
import { DatabaseObjectView } from './DatabaseObjectView';
import { PrivilegeView } from './PrivilegeView';

function getSubTabs(
  t: (key: TranslationKey) => string,
  readOnly?: boolean,
): { id: SubTabId; label: string }[] {
  if (readOnly) {
    return [
      { id: 'data', label: t('connWin.data') },
      { id: 'structure', label: t('connWin.structure') },
    ];
  }
  return [
    { id: 'data', label: t('connWin.data') },
    { id: 'structure', label: t('connWin.structure') },
    { id: 'indexes', label: t('connWin.indexes') },
    { id: 'foreignKeys', label: t('connWin.foreignKeys') },
    { id: 'ddl', label: 'DDL' },
  ];
}

function getViewSubTabs(t: (key: TranslationKey) => string): { id: SubTabId; label: string }[] {
  return [
    { id: 'data', label: t('connWin.data') },
    { id: 'structure', label: t('connWin.structure') },
    { id: 'ddl', label: 'DDL' },
  ];
}

function getPanelIcon(panel: Panel): ReactNode {
  switch (panel.type) {
    case 'table':
      return <Table2 className="h-3.5 w-3.5 shrink-0 text-blue-400" />;
    case 'view':
      return <Eye className="h-3.5 w-3.5 shrink-0 text-purple-400" />;
    case 'query':
      return <Code2 className="h-3.5 w-3.5 shrink-0" />;
    case 'create-table':
      return <TableProperties className="h-3.5 w-3.5 shrink-0" />;
    case 'er-diagram':
      return <GitFork className="h-3.5 w-3.5 shrink-0" />;
    case 'objects':
      return <Code2 className="h-3.5 w-3.5 shrink-0" />;
    case 'privileges':
      return <KeyRound className="h-3.5 w-3.5 shrink-0" />;
    case 'db-object': {
      const kind = (panel as DatabaseObjectPanel).objectKind;
      if (kind === 'trigger') return <Zap className="h-3.5 w-3.5 shrink-0 text-amber-400" />;
      if (kind === 'procedure') return <Braces className="h-3.5 w-3.5 shrink-0 text-emerald-400" />;
      if (kind === 'sequence') return <Hash className="h-3.5 w-3.5 shrink-0 text-cyan-400" />;
      if (kind === 'type') return <Shapes className="h-3.5 w-3.5 shrink-0 text-pink-400" />;
      return <Braces className="h-3.5 w-3.5 shrink-0 text-orange-400" />;
    }
    case 'redis-db':
      return <Database className="h-3.5 w-3.5 shrink-0 text-teal-400" />;
    default:
      return null;
  }
}

function getPanelLabel(panel: Panel): string {
  switch (panel.type) {
    case 'table':
      return (panel as TablePanel).tableName;
    case 'view':
      return (panel as ViewPanel).viewName;
    case 'query':
      return (panel as QueryPanelType).title;
    case 'create-table':
      return 'New Table';
    case 'er-diagram':
      return 'ER Diagram';
    case 'objects':
      return 'Objects';
    case 'privileges':
      return 'Privileges';
    case 'db-object':
      return (panel as DatabaseObjectPanel).objectName;
    case 'redis-db':
      return `${panel.connectionName}@${(panel as RedisDbPanel).dbName}`;
    default:
      return '';
  }
}

function resolveConnectionContext(
  connectionId: string,
  activeConnections: ReturnType<typeof useActiveConnectionStore.getState>['connections'],
  savedConnections: ReturnType<typeof useConnectionStore.getState>['connections'],
): ConnectionContext | null {
  const entry = Object.values(activeConnections).find((e) => e.connectionId === connectionId);
  if (!entry?.connectionId) return null;
  const saved = savedConnections.find((c) => c.id === entry.configId);
  if (!saved) return null;
  return {
    configId: entry.configId,
    connectionId: entry.connectionId,
    connectionName: saved.name,
    databaseType: saved.databaseType,
  };
}

export interface ContentViewProps {
  selectTableRef?: MutableRefObject<((table: string, schema?: string) => void) | undefined>;
  nodeContextMenuRef?: MutableRefObject<((payload: NodeContextMenuPayload) => void) | undefined>;
  actionsRef?: MutableRefObject<ConnectionViewActions | undefined>;
}

export function ContentView({ selectTableRef, nodeContextMenuRef, actionsRef }: ContentViewProps) {
  const { t } = useI18n();
  const [confirmAction, confirmActionDialog] = useConfirmDialog();
  const safeMode = useSettingsStore((s) => s.settings.safeMode);

  const allPanels = usePanelStore((s) => s.panels);
  const activePanelId = usePanelStore((s) => s.activePanelId);
  const addPanel = usePanelStore((s) => s.addPanel);
  const removePanel = usePanelStore((s) => s.removePanel);
  const storeUpdatePanel = usePanelStore((s) => s.updatePanel);
  const setActivePanel = usePanelStore((s) => s.setActivePanel);

  const activePanel = allPanels.find((p) => p.id === activePanelId) ?? null;

  const connectionId = activePanel?.connectionId ?? '';
  const configId = activePanel?.configId ?? '';
  const connectionName = activePanel?.connectionName ?? '';
  const databaseType = activePanel?.databaseType as DatabaseType | undefined;

  const connCtx: ConnectionContext | null = useMemo(() => {
    if (!activePanel) return null;
    return {
      configId: activePanel.configId,
      connectionId: activePanel.connectionId,
      connectionName: activePanel.connectionName,
      databaseType: activePanel.databaseType,
    };
  }, [
    activePanel?.configId,
    activePanel?.connectionId,
    activePanel?.connectionName,
    activePanel?.databaseType,
  ]);

  const connPanels = useMemo(
    () => (connCtx ? allPanels.filter((p) => p.configId === connCtx.configId) : []),
    [allPanels, connCtx?.configId],
  );

  const savedConnections = useConnectionStore((s) => s.connections);
  const activeConnections = useActiveConnectionStore((s) => s.connections);
  const storeActiveConnectionId = useSchemaStore((s) => s.activeConnectionId);

  const sidebarConnCtx = useMemo(() => {
    if (!storeActiveConnectionId) return connCtx;
    return (
      resolveConnectionContext(storeActiveConnectionId, activeConnections, savedConnections) ??
      connCtx
    );
  }, [storeActiveConnectionId, activeConnections, savedConnections, connCtx]);

  const initialDatabase = useMemo(() => {
    const ctxConfigId = sidebarConnCtx?.configId ?? configId;
    if (!ctxConfigId) return undefined;
    return savedConnections.find((c) => c.id === ctxConfigId)?.database;
  }, [savedConnections, sidebarConnCtx?.configId, configId]);

  const dbMeta = databaseType ? DB_REGISTRY[databaseType] : undefined;
  const showStructureEditor = canOpenStructureEditor(dbMeta) && dbMeta?.readOnly !== true;
  const exportScope = resolveExportScope(dbMeta);
  const batchExportSupported = supportsFullTableExport(exportScope);

  const isRedisPanel = activePanel?.type === 'redis-db';
  const showNewQuery = !isRedisPanel && dbMeta?.supportsSQL !== false && !!databaseType;
  const showNewTable =
    !isRedisPanel && canOpenStructureEditor(dbMeta) && dbMeta?.readOnly !== true && !!databaseType;
  const showErDiagramToolbar =
    !isRedisPanel && dbMeta?.supportsErDiagram !== false && !!databaseType;
  const showObjectsToolbar = !isRedisPanel && dbMeta?.readOnly !== true && !!databaseType;

  const [aiChatOpen, setAiChatOpen] = useState(false);
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [exportTableName, setExportTableName] = useState<string | null>(null);
  const [importTableName, setImportTableName] = useState<string | null>(null);
  const [batchExportOpen, setBatchExportOpen] = useState(false);
  const [batchExportInitialSelected, setBatchExportInitialSelected] = useState<string[]>([]);
  const [lastTableSchema, setLastTableSchema] = useState<string | null>(null);

  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const schemaTables = useSchemaStore((s) => s.tables);
  const schemaViews = useSchemaStore((s) => s.views);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const loadTables = useSchemaStore((s) => s.loadTables);
  const removeRelation = useSchemaStore((s) => s.removeRelation);
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

  const createQueryTab = useQueryStore((s) => s.createTab);
  const closeQueryTab = useQueryStore((s) => s.closeTab);
  const queryTabs = useQueryStore((s) => s.tabs);
  const updateQuerySql = useQueryStore((s) => s.updateSql);
  const resultDetailRowIndex = useQueryStore((s) => s.resultDetailRowIndex);
  const updateResultCell = useQueryStore((s) => s.updateResultCell);

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

  const handleSelectTable = useCallback(
    (table: string, schema?: string) => {
      const ctx = sidebarConnCtx;
      if (!ctx) return;
      if (schema) setLastTableSchema(schema);
      const scopedPanels = allPanels.filter((p) => p.configId === ctx.configId);
      const isView = schemaViews.some((v) => v.name === table);
      if (isView) {
        const existing = scopedPanels.find((p) => p.type === 'view' && p.viewName === table);
        if (existing) {
          setActivePanel(existing.id);
          return;
        }
        const panel: ViewPanel = {
          ...ctx,
          type: 'view',
          id: nextPanelId('view'),
          viewName: table,
          subTab: 'data',
        };
        addPanel(panel);
        return;
      }
      const existing = scopedPanels.find((p) => p.type === 'table' && p.tableName === table);
      if (existing) {
        setActivePanel(existing.id);
        return;
      }
      const panel: TablePanel = {
        ...ctx,
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: table,
        subTab: 'data',
      };
      addPanel(panel);
    },
    [sidebarConnCtx, allPanels, schemaViews, addPanel, setActivePanel],
  );

  useLayoutEffect(() => {
    if (selectTableRef) selectTableRef.current = handleSelectTable;
    return () => {
      if (selectTableRef) selectTableRef.current = undefined;
    };
  }, [selectTableRef, handleSelectTable]);

  const handleCreateTable = useCallback(() => {
    if (!connCtx) return;
    const existing = connPanels.find((p) => p.type === 'create-table');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: CreateTablePanel = {
      ...connCtx,
      type: 'create-table',
      id: nextPanelId('new-tbl'),
    };
    addPanel(panel);
  }, [connCtx, connPanels, addPanel, setActivePanel]);

  const handleEditTableStructure = useCallback(
    (name: string) => {
      if (!connCtx) return;
      const existing = connPanels.find((p) => p.type === 'table' && p.tableName === name);
      if (existing) {
        setActivePanel(existing.id);
        storeUpdatePanel(existing.id, { subTab: 'structure', structureEditing: true });
        return;
      }
      const panel: TablePanel = {
        ...connCtx,
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: name,
        subTab: 'structure',
        structureEditing: true,
      };
      addPanel(panel);
    },
    [connCtx, connPanels, addPanel, setActivePanel, storeUpdatePanel],
  );

  const handleOpenStructure = useCallback(
    (name: string) => {
      if (!connCtx) return;
      if (showStructureEditor) {
        handleEditTableStructure(name);
        return;
      }
      const existing = connPanels.find((p) => p.type === 'table' && p.tableName === name);
      if (existing) {
        setActivePanel(existing.id);
        storeUpdatePanel(existing.id, { subTab: 'structure' });
        return;
      }
      const panel: TablePanel = {
        ...connCtx,
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: name,
        subTab: 'structure',
      };
      addPanel(panel);
    },
    [
      connCtx,
      showStructureEditor,
      handleEditTableStructure,
      connPanels,
      addPanel,
      setActivePanel,
      storeUpdatePanel,
    ],
  );

  const handleExitStructureEditing = useCallback(
    (panelId: string) => {
      storeUpdatePanel(panelId, { structureEditing: false });
    },
    [storeUpdatePanel],
  );

  const handleOpenErDiagram = useCallback(
    (focus?: string) => {
      if (!connCtx) return;
      const existing = connPanels.find((p) => p.type === 'er-diagram');
      if (existing) {
        if (focus) storeUpdatePanel(existing.id, { focusTable: focus });
        setActivePanel(existing.id);
        return;
      }
      const panel: ErDiagramPanel = {
        ...connCtx,
        type: 'er-diagram',
        id: nextPanelId('er'),
        focusTable: focus,
      };
      addPanel(panel);
    },
    [connCtx, connPanels, addPanel, setActivePanel, storeUpdatePanel],
  );

  const handleOpenObjects = useCallback(() => {
    if (!connCtx) return;
    const existing = connPanels.find((p) => p.type === 'objects');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: ObjectsPanel = { ...connCtx, type: 'objects', id: nextPanelId('obj') };
    addPanel(panel);
  }, [connCtx, connPanels, addPanel, setActivePanel]);

  const handleOpenDbObject = useCallback(
    (
      kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
      name: string,
      schema?: string,
    ) => {
      if (!connCtx) return;
      const existing = connPanels.find(
        (p) =>
          p.type === 'db-object' &&
          (p as DatabaseObjectPanel).objectName === name &&
          (p as DatabaseObjectPanel).objectKind === kind,
      );
      if (existing) {
        setActivePanel(existing.id);
        return;
      }
      const panel: DatabaseObjectPanel = {
        ...connCtx,
        type: 'db-object',
        id: nextPanelId('dbobj'),
        objectKind: kind,
        objectName: name,
        objectSchema: schema,
      };
      addPanel(panel);
    },
    [connCtx, connPanels, addPanel, setActivePanel],
  );

  const handleOpenPrivileges = useCallback(() => {
    if (!connCtx) return;
    const existing = connPanels.find((p) => p.type === 'privileges');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: PrivilegesPanel = { ...connCtx, type: 'privileges', id: nextPanelId('priv') };
    addPanel(panel);
  }, [connCtx, connPanels, addPanel, setActivePanel]);

  const handleNewQuery = useCallback(
    (initialSql?: string) => {
      if (!connCtx) return;
      createQueryTab();
      const latestTab = useQueryStore.getState().tabs.at(-1);
      if (!latestTab) return;
      if (initialSql) updateQuerySql(latestTab.id, initialSql);
      const db = currentDatabase ?? initialDatabase ?? '';
      const panel: QueryPanelType = {
        ...connCtx,
        type: 'query',
        id: nextPanelId('qry'),
        queryTabId: latestTab.id,
        title: db ? `${connCtx.connectionName}@${db}` : connCtx.connectionName,
      };
      addPanel(panel);
    },
    [connCtx, createQueryTab, updateQuerySql, currentDatabase, initialDatabase, addPanel],
  );

  const handleClosePanel = useCallback(
    (panelId: string) => {
      const closing = allPanels.find((p) => p.id === panelId);
      if (closing?.type === 'query') {
        closeQueryTab(closing.queryTabId);
      }
      removePanel(panelId);
    },
    [allPanels, closeQueryTab, removePanel],
  );

  const handleCloseOtherPanels = useCallback(
    (keepPanelId: string) => {
      const toClose = allPanels.filter((p) => p.id !== keepPanelId);
      for (const panel of toClose) {
        if (panel.type === 'query') closeQueryTab(panel.queryTabId);
        removePanel(panel.id);
      }
      setActivePanel(keepPanelId);
    },
    [allPanels, closeQueryTab, removePanel, setActivePanel],
  );

  const handleCloseAllPanels = useCallback(() => {
    for (const panel of allPanels) {
      if (panel.type === 'query') closeQueryTab(panel.queryTabId);
    }
    usePanelStore.getState().closeAllPanels();
  }, [allPanels, closeQueryTab]);

  const handleClosePanelsToTheRight = useCallback(
    (panelId: string) => {
      const idx = allPanels.findIndex((p) => p.id === panelId);
      if (idx < 0) return;
      const toClose = allPanels.slice(idx + 1);
      for (const panel of toClose) {
        if (panel.type === 'query') closeQueryTab(panel.queryTabId);
        removePanel(panel.id);
      }
      setActivePanel(panelId);
    },
    [allPanels, closeQueryTab, removePanel, setActivePanel],
  );

  const handleClosePanelsToTheLeft = useCallback(
    (panelId: string) => {
      const idx = allPanels.findIndex((p) => p.id === panelId);
      if (idx < 0) return;
      const toClose = allPanels.slice(0, idx);
      for (const panel of toClose) {
        if (panel.type === 'query') closeQueryTab(panel.queryTabId);
        removePanel(panel.id);
      }
      setActivePanel(panelId);
    },
    [allPanels, closeQueryTab, removePanel, setActivePanel],
  );

  const handlePanelTabContextMenu = useCallback(
    (panelId: string, e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const idx = allPanels.findIndex((p) => p.id === panelId);
      void showNativeContextMenu(
        buildConnectionTabContextMenuItems({
          labels: {
            close: t('connWin.closeTab'),
            closeOthers: t('connWin.closeOtherTabs'),
            closeAll: t('connWin.closeAllTabs'),
            closeToTheRight: t('connWin.closeTabsToRight'),
            closeToTheLeft: t('connWin.closeTabsToLeft'),
          },
          handlers: {
            onClose: () => handleClosePanel(panelId),
            onCloseOthers: () => handleCloseOtherPanels(panelId),
            onCloseAll: handleCloseAllPanels,
            onCloseToTheRight: () => handleClosePanelsToTheRight(panelId),
            onCloseToTheLeft: () => handleClosePanelsToTheLeft(panelId),
          },
          onlyOneTab: allPanels.length <= 1,
          hasTabsToRight: idx >= 0 && idx < allPanels.length - 1,
          hasTabsToLeft: idx > 0,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      t,
      handleClosePanel,
      handleCloseOtherPanels,
      handleCloseAllPanels,
      handleClosePanelsToTheRight,
      handleClosePanelsToTheLeft,
      allPanels,
    ],
  );

  const handleSetSubTab = useCallback(
    (panelId: string, subTab: SubTabId) => {
      const p = allPanels.find((panel) => panel.id === panelId);
      if (!p) return;
      if (p.type === 'table') {
        storeUpdatePanel(panelId, {
          subTab,
          structureEditing: subTab === 'structure' ? p.structureEditing : false,
        });
      } else if (p.type === 'view') {
        storeUpdatePanel(panelId, { subTab });
      }
    },
    [allPanels, storeUpdatePanel],
  );

  const handleRefresh = useCallback(() => {
    const ctx = sidebarConnCtx ?? connCtx;
    if (!ctx?.connectionId) return;
    if (currentDatabase) {
      void loadTables(currentDatabase);
    } else {
      void loadForConnection(ctx.connectionId, { databaseType: ctx.databaseType });
    }
  }, [sidebarConnCtx, connCtx, currentDatabase, loadTables, loadForConnection]);

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
      const ctx = connCtx ?? sidebarConnCtx;
      if (!ctx) return Promise.reject(new Error('No active connection'));
      return loadBatchExportTableData({
        connectionId: ctx.connectionId,
        tableName: name,
        databaseType: ctx.databaseType,
        includeRows: false,
      });
    },
    [connCtx, sidebarConnCtx],
  );

  const handleNodeContextMenu = useCallback(
    (payload: SchemaTreeNodeContextMenuPayload) => {
      const ctx = sidebarConnCtx ?? connCtx;
      if (!ctx) return;
      const { kind, name, schema } = payload;
      const ctxDbType = ctx.databaseType as DatabaseType;
      const ctxDbMeta = DB_REGISTRY[ctxDbType];
      const ctxIsReadOnly = ctxDbMeta?.readOnly === true;
      const ctxShowStructureEditor = canOpenStructureEditor(ctxDbMeta) && !ctxIsReadOnly;
      const ctxSupportsErDiagram = ctxDbMeta?.supportsErDiagram !== false;
      const ctxExportScope = resolveExportScope(ctxDbMeta);
      const ctxExportDataSupported = supportsAnyExport(ctxExportScope);
      const ctxBatchExportSupported = supportsFullTableExport(ctxExportScope);
      const scopedPanels = allPanels.filter((p) => p.configId === ctx.configId);

      const copyText = (text: string) => {
        void navigator.clipboard.writeText(text);
      };
      const quoted = escapeIdent(name, ctxDbType);

      const copyDdl = () => {
        const dialect = getSqlDialect(ctxDbType);
        if (!dialect) return;
        const { sql, extractColumnIndex } = dialect.ddl.getTableDdlQuery(name);
        void getCachedDDL(ctx.connectionId, name, sql, (rows) => {
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
        try {
          await queryCommands.executeQuery(ctx.connectionId, sql);
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
            copyName: t('schemaTree.copyName'),
            copyDdl: t('connWin.copyDDL'),
            focusEr: t('erDiagram.focusTable'),
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
            viewErDiagram: t('schemaTree.viewErDiagram'),
            newSchema: t('schemaTree.newSchema'),
            dataTransfer: t('schemaTree.dataTransfer'),
            compareSchema: t('schemaTree.compareSchema'),
            compareData: t('schemaTree.compareData'),
          },
          handlers: {
            onOpen:
              kind === 'table' || kind === 'view'
                ? () => handleSelectTable(name, schema)
                : undefined,
            onOpenStructure: kind === 'table' ? () => handleOpenStructure(name) : undefined,
            onCopyName: kind === 'table' || kind === 'view' ? () => copyText(name) : undefined,
            onCopyDdl: kind === 'table' || kind === 'view' ? () => copyDdl() : undefined,
            onFocusEr: kind === 'table' ? () => handleOpenErDiagram(name) : undefined,
            onExport:
              kind === 'table' || kind === 'view'
                ? () => {
                    setExportTableName(name);
                    handleSelectTable(name, schema);
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
            onRefresh: handleRefresh,
            onNewQuery: () => {
              if (kind === 'table') {
                handleNewQuery(`SELECT * FROM ${quoted} LIMIT 100`);
              } else {
                handleNewQuery();
              }
            },
            onCopyDatabaseName: kind === 'database' ? () => copyText(name) : undefined,
            onNewTable: handleCreateTable,
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
                            connectionId: ctx.connectionId,
                            table: name,
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
                        invalidateSchemaCache(ctx.connectionId, name);
                        removeRelation(name);
                        handleRefresh();
                        closePanelsForTable(name);
                      },
                    );
                  }
                : undefined,
          },
          readOnly: ctxIsReadOnly,
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
      connCtx,
      t,
      handleSelectTable,
      handleOpenStructure,
      handleOpenErDiagram,
      handleRefresh,
      handleNewQuery,
      handleCreateTable,
      removeRelation,
      openBatchExport,
      safeMode,
      allPanels,
      removePanel,
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
        newQuery: handleNewQuery,
        openErDiagram: handleOpenErDiagram,
        refresh: handleRefresh,
        openObject: handleOpenDbObject,
      };
    }
    return () => {
      if (actionsRef) actionsRef.current = undefined;
    };
  }, [actionsRef, handleNewQuery, handleOpenErDiagram, handleOpenDbObject, handleRefresh]);

  useKeyboardShortcuts([
    {
      key: 'mod+n',
      scope: 'global',
      description: t('connWin.newQuery'),
      action: () => handleNewQuery(),
    },
    {
      key: 'mod+w',
      scope: 'global',
      description: t('common.close'),
      action: () => {
        if (activePanelId) handleClosePanel(activePanelId);
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

  const activeQueryTab =
    activePanel?.type === 'query'
      ? queryTabs.find((tab) => tab.id === (activePanel as QueryPanelType).queryTabId)
      : null;
  const activeQueryResult = activeQueryTab?.results[activeQueryTab.activeResultIdx] ?? null;

  const detailColumnDefs: ColumnDef[] = useMemo(() => {
    if (activePanel?.type === 'table') {
      return tableColumns.map((c) => ({ id: c.name, name: c.name, type: c.dataType }));
    }
    if (activeQueryResult) {
      return activeQueryResult.columns.map((c) => ({ id: c.name, name: c.name, type: c.dataType }));
    }
    return [];
  }, [activePanel?.type, tableColumns, activeQueryResult]);

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
      } else if (activePanel?.type === 'query' && activeQueryTab) {
        updateResultCell(activeQueryTab.id, activeQueryTab.activeResultIdx, row, col, value);
      }
    },
    [activePanel, activeQueryTab, updateCell, updateResultCell, applyColumnToRows, selectedRows],
  );

  const renderSqlPanelContent = (panel: Panel) => {
    const panelDbMeta = DB_REGISTRY[panel.databaseType];
    const panelIsReadOnly = panelDbMeta?.readOnly === true;
    const panelShowStructureEditor = canOpenStructureEditor(panelDbMeta) && !panelIsReadOnly;
    const panelExportScope = resolveExportScope(panelDbMeta);

    if (panel.type === 'table') {
      return (
        <>
          <div className="flex shrink-0 border-b border-edge bg-surface-alt">
            {getSubTabs(t, panelIsReadOnly).map((tab) => (
              <button
                key={tab.id}
                type="button"
                className={cn(
                  'relative px-5 py-2 text-[13px] transition-colors',
                  panel.subTab === tab.id
                    ? 'bg-surface text-fg font-medium'
                    : 'text-fg-secondary hover:text-fg',
                )}
                onClick={() => handleSetSubTab(panel.id, tab.id)}
              >
                {tab.label}
                <span
                  className={cn(
                    'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                    panel.subTab === tab.id ? 'opacity-100' : 'opacity-0',
                  )}
                />
              </button>
            ))}
          </div>
          <div className="flex min-h-0 flex-1 flex-col">
            {panel.subTab === 'data' && (
              <TableView
                connectionId={panel.connectionId}
                database={currentDatabase ?? ''}
                tableName={panel.tableName}
                databaseType={panel.databaseType}
                dataExportCapability={panelExportScope}
              />
            )}
            {panel.subTab === 'structure' &&
              (panel.structureEditing ? (
                <TableStructureEditor
                  connectionId={panel.connectionId}
                  databaseType={panel.databaseType}
                  schema={resolveTableSchema(panel.tableName)}
                  mode="alter"
                  tableName={panel.tableName}
                  showBackButton
                  onSuccess={() => {
                    invalidateSchemaCache(panel.connectionId, panel.tableName);
                    handleExitStructureEditing(panel.id);
                    handleRefresh();
                  }}
                  onCancel={() => handleExitStructureEditing(panel.id)}
                />
              ) : (
                <StructureView
                  connectionId={panel.connectionId}
                  tableName={panel.tableName}
                  onEditStructure={
                    panelShowStructureEditor
                      ? () => handleEditTableStructure(panel.tableName)
                      : undefined
                  }
                />
              ))}
            {panel.subTab === 'indexes' && (
              <IndexesView
                connectionId={panel.connectionId}
                tableName={panel.tableName}
                databaseType={panel.databaseType}
                onEditStructure={
                  panelShowStructureEditor
                    ? () => handleSetSubTab(panel.id, 'structure')
                    : undefined
                }
              />
            )}
            {panel.subTab === 'foreignKeys' && (
              <ForeignKeysView connectionId={panel.connectionId} tableName={panel.tableName} />
            )}
            {panel.subTab === 'ddl' && (
              <DDLView
                connectionId={panel.connectionId}
                tableName={panel.tableName}
                databaseType={panel.databaseType}
              />
            )}
          </div>
        </>
      );
    }

    if (panel.type === 'view') {
      return (
        <>
          <div className="flex shrink-0 border-b border-edge bg-surface-alt">
            {getViewSubTabs(t).map((tab) => (
              <button
                key={tab.id}
                type="button"
                className={cn(
                  'relative px-5 py-2 text-[13px] transition-colors',
                  panel.subTab === tab.id
                    ? 'bg-surface text-fg font-medium'
                    : 'text-fg-secondary hover:text-fg',
                )}
                onClick={() => handleSetSubTab(panel.id, tab.id)}
              >
                {tab.label}
                <span
                  className={cn(
                    'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                    panel.subTab === tab.id ? 'opacity-100' : 'opacity-0',
                  )}
                />
              </button>
            ))}
          </div>
          <div className="flex min-h-0 flex-1 flex-col">
            {panel.subTab === 'data' && (
              <TableView
                connectionId={panel.connectionId}
                database={currentDatabase ?? ''}
                tableName={panel.viewName}
                databaseType={panel.databaseType}
                dataExportCapability={panelExportScope}
              />
            )}
            {panel.subTab === 'structure' && (
              <StructureView connectionId={panel.connectionId} tableName={panel.viewName} />
            )}
            {panel.subTab === 'ddl' && (
              <DDLView
                connectionId={panel.connectionId}
                tableName={panel.viewName}
                databaseType={panel.databaseType}
              />
            )}
          </div>
        </>
      );
    }

    if (panel.type === 'query') {
      return (
        <QueryPanel
          connectionId={panel.connectionId}
          configId={panel.configId}
          queryTabId={panel.queryTabId}
          databaseType={panel.databaseType}
        />
      );
    }

    if (panel.type === 'create-table') {
      const schema = resolveCreateTableSchema(panel.databaseType, {
        currentDatabase,
        contextSchema: lastTableSchema,
      });
      return (
        <TableStructureEditor
          connectionId={panel.connectionId}
          databaseType={panel.databaseType}
          schema={schema}
          mode="create"
          onSuccess={() => {
            handleClosePanel(panel.id);
            handleRefresh();
          }}
          onCancel={() => handleClosePanel(panel.id)}
        />
      );
    }

    if (panel.type === 'er-diagram' && currentDatabase) {
      return (
        <div className="flex min-h-0 flex-1 flex-col">
          <ErDiagramView
            connectionId={panel.connectionId}
            database={currentDatabase}
            focusTable={(panel as ErDiagramPanel).focusTable}
            onSelectTable={handleSelectTable}
            onFocusTable={(table) => handleOpenErDiagram(table)}
          />
        </div>
      );
    }

    if (panel.type === 'objects') {
      return <ObjectBrowser connectionId={panel.connectionId} databaseType={panel.databaseType} />;
    }

    if (panel.type === 'privileges') {
      return <PrivilegeView connectionId={panel.connectionId} />;
    }

    if (panel.type === 'db-object') {
      return (
        <DatabaseObjectView
          connectionId={panel.connectionId}
          databaseType={panel.databaseType}
          objectKind={panel.objectKind}
          objectName={panel.objectName}
          objectSchema={panel.objectSchema}
        />
      );
    }

    return null;
  };

  const schemaTreeConnectionId = sidebarConnCtx?.connectionId ?? connectionId;
  const schemaTreeDatabaseType = sidebarConnCtx?.databaseType ?? databaseType;

  return (
    <>
      <div className="flex h-12 min-h-[48px] shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
        {showNewQuery && (
          <Button variant="primary" className="h-8" onClick={() => handleNewQuery()}>
            <Plus className="h-4 w-4" />
            {t('connWin.newQuery')}
          </Button>
        )}
        {showNewTable && (
          <Button variant="secondary" className="h-8" onClick={handleCreateTable}>
            <TableProperties className="h-4 w-4" />
            {t('connWin.newTable')}
          </Button>
        )}
        {showErDiagramToolbar && (
          <Button variant="secondary" className="h-8" onClick={() => handleOpenErDiagram()}>
            <GitFork className="h-4 w-4" />
            {t('erDiagram.title')}
          </Button>
        )}
        {showObjectsToolbar && (
          <>
            <Button variant="secondary" className="h-8" onClick={handleOpenObjects}>
              <Code2 className="h-4 w-4" />
              {t('objects.title')}
            </Button>
            <Button variant="secondary" className="h-8" onClick={handleOpenPrivileges}>
              <KeyRound className="h-4 w-4" />
              {t('privileges.title')}
            </Button>
          </>
        )}
        {batchExportSupported && showNewQuery && (
          <Button
            variant="secondary"
            className="h-8"
            data-testid="conn-toolbar-export"
            title={t('batchExport.title')}
            onClick={handleOpenBatchExportFromToolbar}
          >
            <Download className="h-4 w-4" />
            {t('batchExport.title')}
          </Button>
        )}

        <div className="flex-1" />

        <Button
          variant="ghost"
          className="h-7 gap-1 px-2 text-xs"
          title={t('docs.openAiHelp')}
          onClick={() => openDocsWindow('ai')}
        >
          <BookOpen className="h-3.5 w-3.5" />
        </Button>

        <Button
          variant={aiChatOpen ? 'secondary' : 'ghost'}
          className="h-7 gap-1 px-2 text-xs"
          onClick={() => setAiChatOpen((v) => !v)}
        >
          <MessageSquare className="h-3.5 w-3.5" />
        </Button>

        {detailPanelApplicable && (
          <DetailPanelToggle open={detailOpen} onToggle={() => setDetailOpen((p) => !p)} />
        )}
      </div>

      {allPanels.length > 0 && (
        <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt">
          <div
            className="scrollbar-hide flex min-w-0 flex-1 overflow-x-auto"
            onWheel={(e) => {
              if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
              e.currentTarget.scrollLeft += e.deltaY;
            }}
          >
            {allPanels.map((panel) => {
              const isActive = panel.id === activePanelId;
              return (
                <div
                  key={panel.id}
                  data-testid="panel-tab"
                  className={cn(
                    'group relative flex items-center gap-1.5 border-r border-edge px-3 py-2 text-xs transition-colors',
                    isActive
                      ? 'bg-surface text-fg'
                      : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                  )}
                  title={`${panel.connectionName} · ${getPanelLabel(panel)}`}
                  onContextMenu={(e) => handlePanelTabContextMenu(panel.id, e)}
                >
                  <button
                    type="button"
                    className="flex items-center gap-1.5"
                    onClick={() => setActivePanel(panel.id)}
                  >
                    {getPanelIcon(panel)}
                    <span className="max-w-[160px] truncate">{getPanelLabel(panel)}</span>
                  </button>
                  <button
                    type="button"
                    data-testid="panel-tab-close"
                    className="rounded p-0.5 text-fg-muted opacity-0 hover:bg-surface-raised hover:text-fg group-hover:opacity-100"
                    onClick={() => handleClosePanel(panel.id)}
                  >
                    <X className="h-3 w-3" />
                  </button>
                  <span
                    className={cn(
                      'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                      isActive ? 'opacity-100' : 'opacity-0',
                    )}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {schemaTreeConnectionId && schemaTreeDatabaseType && (
          <div className="hidden">
            <SchemaTree
              connectionId={schemaTreeConnectionId}
              databaseType={schemaTreeDatabaseType}
              initialDatabase={initialDatabase}
              selectedTable={null}
              searchQuery=""
              onSelectTable={handleSelectTable}
            />
          </div>
        )}

        <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
          {allPanels.map((panel) => {
            const isActive = panel.id === activePanelId;
            if (panel.type === 'redis-db') {
              const viewMode = DB_REGISTRY[panel.databaseType]?.connectionView ?? 'sql';
              const RedisView = getConnectionView(viewMode);
              return (
                <div
                  key={panel.id}
                  className="absolute inset-0 flex flex-col"
                  style={{ display: isActive ? 'flex' : 'none' }}
                >
                  <RedisView
                    connectionId={panel.connectionId}
                    configId={panel.configId}
                    connectionName={panel.connectionName}
                    databaseType={panel.databaseType}
                    initialDatabase={panel.dbName}
                    hideSidebar
                    isActive={isActive}
                  />
                </div>
              );
            }
            return (
              <div
                key={panel.id}
                className="absolute inset-0 flex flex-col"
                style={{ display: isActive ? 'flex' : 'none' }}
              >
                {renderSqlPanelContent(panel)}
              </div>
            );
          })}

          {!activePanel && allPanels.length === 0 && (
            <div className="flex flex-1 items-center justify-center text-fg-muted">
              <div className="text-center">
                <Database className="mx-auto h-10 w-10 opacity-20" />
                <div className="mt-3 text-sm">
                  {`${t('connWin.selectTable')} (⌘N ${t('connWin.newQuery')})`}
                </div>
              </div>
            </div>
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

        {aiChatOpen && connectionId && (
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
                connectionId={connectionId}
                database={currentDatabase ?? undefined}
                onInsertSql={(sql) => {
                  if (activePanel?.type === 'query') {
                    const tab = queryTabs.find(
                      (qt) => qt.id === (activePanel as QueryPanelType).queryTabId,
                    );
                    if (tab) updateQuerySql(tab.id, sql);
                  }
                }}
              />
            </aside>
          </>
        )}
      </div>

      <footer className="flex h-10 min-h-[40px] shrink-0 items-center justify-between border-t border-edge bg-surface-alt px-4 text-xs text-fg-secondary">
        <div className="flex items-center gap-2">
          <span className="inline-flex h-2 w-2 rounded-full bg-green-500" />
          <span>{t('connWin.connected')}</span>
        </div>
        <div className="truncate text-fg-muted">
          {[
            databaseType ? getDbLabel(databaseType) : null,
            connectionName || null,
            currentDatabase,
            tableName,
            tableColumns.length > 0 && `${tableColumns.length} ${t('connWin.fields')}`,
            totalRows > 0 && `${totalRows} ${t('connWin.rowCount')}`,
          ]
            .filter(Boolean)
            .join(' · ')}
        </div>
        <div className="shrink-0 text-fg-muted">
          <kbd className="font-mono">⌘N</kbd> {t('connWin.newQuery')} ·{' '}
          <kbd className="font-mono">⌘W</kbd> {t('common.close')} ·{' '}
          <kbd className="font-mono">Space</kbd> {t('detail.title')}
        </div>
      </footer>

      {exportOpen && exportTableName && connCtx && (
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
          databaseType={connCtx.databaseType}
          connectionId={connCtx.connectionId}
          totalRows={totalRows}
          defaultScope="entire_table"
          dataExportCapability={exportScope}
        />
      )}

      {connCtx && (
        <BatchExportDialog
          open={batchExportOpen}
          onClose={() => {
            setBatchExportOpen(false);
            setBatchExportInitialSelected([]);
          }}
          connectionId={connCtx.connectionId}
          databaseType={connCtx.databaseType}
          database={currentDatabase ?? undefined}
          tables={exportableTableNames}
          initialSelected={batchExportInitialSelected}
          loadTableExportData={loadTableExportData}
          dataExportCapability={exportScope}
        />
      )}

      {connCtx && (
        <ImportDialog
          open={importOpen}
          onClose={() => {
            setImportOpen(false);
            setImportTableName(null);
          }}
          connectionId={connCtx.connectionId}
          tableName={importTableName}
          onImported={handleRefresh}
          databaseType={connCtx.databaseType}
        />
      )}

      {confirmActionDialog}
    </>
  );
}
