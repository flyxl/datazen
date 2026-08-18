import { useCallback, useEffect, useLayoutEffect, useMemo, useState, type MouseEvent } from 'react';
import {
  BookOpen,
  Code2,
  Database,
  Download,
  KeyRound,
  GitFork,
  MessageSquare,
  Plus,
  TableProperties,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useResizable } from '../../hooks/useResizable';
import { useI18n } from '../../hooks/useI18n';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useQueryStore } from '../../stores/queryStore';
import { useSettingsStore } from '../../stores/settingsStore';
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
  type ConnectionContext,
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
import type { ConnectionViewProps } from '../../lib/connectionViews/types';
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

export function SqlConnectionView({
  connectionId,
  configId,
  connectionName,
  databaseType,
  initialDatabase,
  hideSidebar: externalSidebar,
  isActive = true,
  selectTableRef,
  nodeContextMenuRef,
  actionsRef,
}: ConnectionViewProps) {
  const { t } = useI18n();
  const [confirmAction, confirmActionDialog] = useConfirmDialog();
  const dbMeta = DB_REGISTRY[databaseType];
  const isReadOnly = dbMeta?.readOnly === true;
  const showStructureEditor = canOpenStructureEditor(dbMeta) && !isReadOnly;
  const supportsErDiagram = dbMeta?.supportsErDiagram !== false;
  const exportScope = resolveExportScope(dbMeta);
  const exportDataSupported = supportsAnyExport(exportScope);
  const batchExportSupported = supportsFullTableExport(exportScope);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);

  const allPanels = usePanelStore((s) => s.panels);
  const panels = useMemo(
    () => allPanels.filter((p) => p.configId === configId),
    [allPanels, configId],
  );
  const activePanelId = usePanelStore((s) => s.activePanelId);
  const addPanel = usePanelStore((s) => s.addPanel);
  const removePanel = usePanelStore((s) => s.removePanel);
  const storeUpdatePanel = usePanelStore((s) => s.updatePanel);
  const setActivePanel = usePanelStore((s) => s.setActivePanel);
  const removeAllForConnection = usePanelStore((s) => s.removeAllForConnection);
  const [sidebarOpen] = useState(true);
  const [aiChatOpen, setAiChatOpen] = useState(false);
  // AI chat toggle is always visible; AiChatPanel handles unconfigured state
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [exportTableName, setExportTableName] = useState<string | null>(null);
  const [importTableName, setImportTableName] = useState<string | null>(null);
  const [batchExportOpen, setBatchExportOpen] = useState(false);
  const [batchExportInitialSelected, setBatchExportInitialSelected] = useState<string[]>([]);
  const [lastTableSchema, setLastTableSchema] = useState<string | null>(null);

  const connCtx: ConnectionContext = useMemo(
    () => ({ configId, connectionId, connectionName, databaseType }),
    [configId, connectionId, connectionName, databaseType],
  );

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

  const activePanel = panels.find((p) => p.id === activePanelId) ?? null;

  // The detail panel only makes sense while viewing row data (table "data" sub-tab
  // or query results); hide it on structure/indexes/foreign keys/DDL tabs.
  const detailPanelApplicable =
    activePanel != null &&
    (activePanel.type !== 'table' || activePanel.subTab === 'data') &&
    (activePanel.type !== 'view' || (activePanel as ViewPanel).subTab === 'data');

  const resolveTableSchema = useCallback(
    (tableName: string): string | null => {
      const hit = [...schemaTables, ...schemaViews].find((t) => t.name === tableName);
      return hit?.schema ?? currentDatabase ?? null;
    },
    [schemaTables, schemaViews, currentDatabase],
  );

  const createTableContextSchema = useMemo(() => {
    if (activePanel?.type === 'table') {
      return resolveTableSchema(activePanel.tableName);
    }
    return lastTableSchema;
  }, [activePanel, lastTableSchema, resolveTableSchema]);

  const createTableSchema = useMemo(
    () =>
      resolveCreateTableSchema(databaseType, {
        currentDatabase,
        contextSchema: createTableContextSchema,
      }),
    [databaseType, currentDatabase, createTableContextSchema],
  );

  const { size: sidebarWidth, handleRef } = useResizable({
    direction: 'horizontal',
    initialSize: 280,
    minSize: 200,
    maxSize: 420,
    storageKey: 'connection.sidebar',
  });
  const { size: aiSidebarWidth, handleRef: aiHandleRef } = useResizable({
    direction: 'horizontal',
    initialSize: 320,
    minSize: 240,
    maxSize: 600,
    reverse: true,
    storageKey: 'connection.aiSidebar',
  });

  useEffect(() => {
    setDbType(databaseType);
  }, [databaseType, setDbType]);

  const handleSelectTable = useCallback(
    (table: string, schema?: string) => {
      if (schema) setLastTableSchema(schema);
      console.log('[SqlConnectionView] select table', table);
      const isView = schemaViews.some((v) => v.name === table);
      if (isView) {
        const existing = panels.find((p) => p.type === 'view' && p.viewName === table);
        if (existing) {
          setActivePanel(existing.id);
          return;
        }
        const panel: ViewPanel = {
          ...connCtx,
          type: 'view',
          id: nextPanelId('view'),
          viewName: table,
          subTab: 'data',
        };
        addPanel(panel);
        return;
      }
      const existing = panels.find((p) => p.type === 'table' && p.tableName === table);
      if (existing) {
        setActivePanel(existing.id);
        return;
      }
      const panel: TablePanel = {
        ...connCtx,
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: table,
        subTab: 'data',
      };
      addPanel(panel);
    },
    [schemaViews, panels, connCtx, addPanel, setActivePanel],
  );

  useLayoutEffect(() => {
    if (selectTableRef && isActive) selectTableRef.current = handleSelectTable;
    return () => {
      if (selectTableRef && isActive) selectTableRef.current = undefined;
    };
  }, [selectTableRef, handleSelectTable, isActive]);

  const handleCreateTable = useCallback(() => {
    const existing = panels.find((p) => p.type === 'create-table');
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
  }, [panels, connCtx, addPanel, setActivePanel]);

  /** Enter alter-structure editor inside the table's Structure sub-tab (no new primary tab). */
  const handleEditTableStructure = useCallback(
    (name: string) => {
      const existing = panels.find((p) => p.type === 'table' && p.tableName === name);
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
    [panels, connCtx, addPanel, setActivePanel, storeUpdatePanel],
  );

  /** Open Structure sub-tab (edit mode when structure editor is available). */
  const handleOpenStructure = useCallback(
    (name: string) => {
      if (showStructureEditor) {
        handleEditTableStructure(name);
        return;
      }
      const existing = panels.find((p) => p.type === 'table' && p.tableName === name);
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
      showStructureEditor,
      handleEditTableStructure,
      panels,
      connCtx,
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
      const existing = panels.find((p) => p.type === 'er-diagram');
      if (existing) {
        if (focus) {
          storeUpdatePanel(existing.id, { focusTable: focus });
        }
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
    [panels, connCtx, addPanel, setActivePanel, storeUpdatePanel],
  );

  const handleOpenObjects = useCallback(() => {
    const existing = panels.find((p) => p.type === 'objects');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: ObjectsPanel = { ...connCtx, type: 'objects', id: nextPanelId('obj') };
    addPanel(panel);
  }, [panels, connCtx, addPanel, setActivePanel]);

  const handleOpenDbObject = useCallback(
    (
      kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
      name: string,
      schema?: string,
    ) => {
      const existing = panels.find(
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
    [panels, connCtx, addPanel, setActivePanel],
  );

  const handleOpenPrivileges = useCallback(() => {
    const existing = panels.find((p) => p.type === 'privileges');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: PrivilegesPanel = { ...connCtx, type: 'privileges', id: nextPanelId('priv') };
    addPanel(panel);
  }, [panels, connCtx, addPanel, setActivePanel]);

  const handleNewQuery = useCallback(
    (initialSql?: string) => {
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
        title: db ? `${connectionName}@${db}` : connectionName,
      };
      addPanel(panel);
    },
    [
      createQueryTab,
      updateQuerySql,
      currentDatabase,
      initialDatabase,
      connectionName,
      connCtx,
      addPanel,
    ],
  );

  const handleClosePanel = useCallback(
    (panelId: string) => {
      const closing = panels.find((p) => p.id === panelId);
      if (closing?.type === 'query') {
        closeQueryTab(closing.queryTabId);
      }
      removePanel(panelId);
    },
    [panels, closeQueryTab, removePanel],
  );

  const handleCloseOtherPanels = useCallback(
    (keepPanelId: string) => {
      const toClose = panels.filter((p) => p.id !== keepPanelId);
      for (const panel of toClose) {
        if (panel.type === 'query') {
          closeQueryTab(panel.queryTabId);
        }
        removePanel(panel.id);
      }
      setActivePanel(keepPanelId);
    },
    [panels, closeQueryTab, removePanel, setActivePanel],
  );

  const handleCloseAllPanels = useCallback(() => {
    for (const panel of panels) {
      if (panel.type === 'query') {
        closeQueryTab(panel.queryTabId);
      }
    }
    removeAllForConnection(configId);
  }, [panels, closeQueryTab, removeAllForConnection, configId]);

  const handleClosePanelsToTheRight = useCallback(
    (panelId: string) => {
      const idx = panels.findIndex((p) => p.id === panelId);
      if (idx < 0) return;
      const toClose = panels.slice(idx + 1);
      for (const panel of toClose) {
        if (panel.type === 'query') closeQueryTab(panel.queryTabId);
        removePanel(panel.id);
      }
      setActivePanel(panelId);
    },
    [panels, closeQueryTab, removePanel, setActivePanel],
  );

  const handleClosePanelsToTheLeft = useCallback(
    (panelId: string) => {
      const idx = panels.findIndex((p) => p.id === panelId);
      if (idx < 0) return;
      const toClose = panels.slice(0, idx);
      for (const panel of toClose) {
        if (panel.type === 'query') closeQueryTab(panel.queryTabId);
        removePanel(panel.id);
      }
      setActivePanel(panelId);
    },
    [panels, closeQueryTab, removePanel, setActivePanel],
  );

  const handlePanelTabContextMenu = useCallback(
    (panelId: string, e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const idx = panels.findIndex((p) => p.id === panelId);
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
          onlyOneTab: panels.length <= 1,
          hasTabsToRight: idx >= 0 && idx < panels.length - 1,
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
      panels,
    ],
  );

  const handleSetSubTab = useCallback(
    (panelId: string, subTab: SubTabId) => {
      const p = panels.find((panel) => panel.id === panelId);
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
    [panels, storeUpdatePanel],
  );

  // Kept for ConnectionWindow unified tab bar integration.
  useEffect(() => {}, [handlePanelTabContextMenu]);

  const handleRefresh = useCallback(() => {
    if (!connectionId) return;
    if (currentDatabase) {
      void loadTables(currentDatabase);
    } else {
      void loadForConnection(connectionId, { databaseType });
    }
  }, [connectionId, currentDatabase, databaseType, loadTables, loadForConnection]);

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
    (name: string) =>
      loadBatchExportTableData({
        connectionId,
        tableName: name,
        databaseType,
        includeRows: false,
      }),
    [connectionId, databaseType],
  );

  const handleNodeContextMenu = useCallback(
    (payload: SchemaTreeNodeContextMenuPayload) => {
      const { kind, name, schema } = payload;
      const dbType = databaseType as DatabaseType;
      const copyText = (text: string) => {
        void navigator.clipboard.writeText(text);
      };
      const quoted = escapeIdent(name, dbType);

      const copyDdl = () => {
        const dialect = getSqlDialect(dbType);
        if (!dialect) return;
        const { sql, extractColumnIndex } = dialect.ddl.getTableDdlQuery(name);
        void getCachedDDL(connectionId, name, sql, (rows) => {
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
          await queryCommands.executeQuery(connectionId, sql);
          afterSuccess?.();
        } catch (e) {
          console.warn(e);
        }
      };

      const closePanelsForTable = (tableName: string) => {
        const toClose = panels.filter((p) => p.type === 'table' && p.tableName === tableName);
        for (const p of toClose) {
          removePanel(p.id);
        }
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
              !isReadOnly && (kind === 'table' || kind === 'database' || kind === 'blank')
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
              kind === 'table' && !isReadOnly && !safeMode
                ? () => {
                    const dialect = getSqlDialect(dbType);
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
                          void store.loadTableData({ connectionId, table: name });
                        }
                      },
                    );
                  }
                : undefined,
            onDrop:
              (kind === 'table' || kind === 'view') && !isReadOnly && !safeMode
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
                        invalidateSchemaCache(connectionId, name);
                        removeRelation(name);
                        handleRefresh();
                        closePanelsForTable(name);
                      },
                    );
                  }
                : undefined,
          },
          readOnly: isReadOnly,
          showOpenStructure: true,
          showErFocus: supportsErDiagram,
          showExport: kind === 'table' ? false : exportDataSupported,
          showBatchExport: kind === 'table' ? false : batchExportSupported,
          showNewTable: showStructureEditor,
        }),
        { x: payload.x, y: payload.y },
      );
    },
    [
      t,
      connectionId,
      databaseType,
      handleSelectTable,
      handleOpenStructure,
      handleOpenErDiagram,
      handleRefresh,
      handleNewQuery,
      handleCreateTable,
      removeRelation,
      openBatchExport,
      isReadOnly,
      showStructureEditor,
      supportsErDiagram,
      exportDataSupported,
      batchExportSupported,
      safeMode,
      panels,
      removePanel,
    ],
  );

  useLayoutEffect(() => {
    if (nodeContextMenuRef && isActive) {
      nodeContextMenuRef.current = (payload) =>
        handleNodeContextMenu(payload as SchemaTreeNodeContextMenuPayload);
    }
    return () => {
      if (nodeContextMenuRef && isActive) nodeContextMenuRef.current = undefined;
    };
  }, [nodeContextMenuRef, handleNodeContextMenu, isActive]);

  useLayoutEffect(() => {
    if (actionsRef && isActive) {
      actionsRef.current = {
        newQuery: handleNewQuery,
        openErDiagram: handleOpenErDiagram,
        refresh: handleRefresh,
        openObject: handleOpenDbObject,
      };
    }
    return () => {
      if (actionsRef && isActive) actionsRef.current = undefined;
    };
  }, [
    actionsRef,
    handleNewQuery,
    handleOpenErDiagram,
    handleOpenDbObject,
    handleRefresh,
    isActive,
  ]);

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

  // TablePlus-style Space shortcut: toggle the detail (right sidebar) panel.
  // Skip when focus is in a text field or on an interactive control.
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

  return (
    <>
      <div className="flex h-12 min-h-[48px] shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
        <Button variant="primary" className="h-8" onClick={() => handleNewQuery()}>
          <Plus className="h-4 w-4" />
          {t('connWin.newQuery')}
        </Button>
        {showStructureEditor && (
          <Button variant="secondary" className="h-8" onClick={handleCreateTable}>
            <TableProperties className="h-4 w-4" />
            {t('connWin.newTable')}
          </Button>
        )}
        {supportsErDiagram && (
          <Button variant="secondary" className="h-8" onClick={() => handleOpenErDiagram()}>
            <GitFork className="h-4 w-4" />
            {t('erDiagram.title')}
          </Button>
        )}
        {!isReadOnly && (
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
        {batchExportSupported && (
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

      <div className="flex min-h-0 flex-1">
        {sidebarOpen && !externalSidebar && (
          <>
            <aside
              style={{ width: sidebarWidth }}
              className="flex shrink-0 flex-col overflow-y-auto border-r border-edge bg-surface-alt"
            >
              <SchemaTree
                connectionId={connectionId}
                databaseType={databaseType}
                initialDatabase={initialDatabase}
                selectedTable={
                  activePanel?.type === 'table'
                    ? activePanel.tableName
                    : activePanel?.type === 'view'
                      ? (activePanel as ViewPanel).viewName
                      : null
                }
                searchQuery=""
                onSelectTable={handleSelectTable}
                onNodeContextMenu={handleNodeContextMenu}
              />
            </aside>

            <div
              ref={handleRef}
              className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/30"
            />
          </>
        )}
        {externalSidebar && (
          <div className="hidden">
            <SchemaTree
              connectionId={connectionId}
              databaseType={databaseType}
              initialDatabase={initialDatabase}
              selectedTable={null}
              searchQuery=""
              onSelectTable={handleSelectTable}
            />
          </div>
        )}

        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {activePanel?.type === 'table' && (
            <>
              <div className="flex shrink-0 border-b border-edge bg-surface-alt">
                {getSubTabs(t, isReadOnly).map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    className={cn(
                      'relative px-5 py-2 text-[13px] transition-colors',
                      activePanel.subTab === tab.id
                        ? 'bg-surface text-fg font-medium'
                        : 'text-fg-secondary hover:text-fg',
                    )}
                    onClick={() => handleSetSubTab(activePanel.id, tab.id)}
                  >
                    {tab.label}
                    <span
                      className={cn(
                        'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                        activePanel.subTab === tab.id ? 'opacity-100' : 'opacity-0',
                      )}
                    />
                  </button>
                ))}
              </div>

              <div className="flex min-h-0 flex-1 flex-col">
                {activePanel.subTab === 'data' && (
                  <TableView
                    connectionId={connectionId}
                    database={currentDatabase ?? ''}
                    tableName={activePanel.tableName}
                    databaseType={databaseType}
                    dataExportCapability={exportScope}
                  />
                )}
                {activePanel.subTab === 'structure' &&
                  (activePanel.structureEditing ? (
                    <TableStructureEditor
                      connectionId={connectionId}
                      databaseType={databaseType}
                      schema={resolveTableSchema(activePanel.tableName)}
                      mode="alter"
                      tableName={activePanel.tableName}
                      showBackButton
                      onSuccess={() => {
                        invalidateSchemaCache(connectionId, activePanel.tableName);
                        handleExitStructureEditing(activePanel.id);
                        handleRefresh();
                      }}
                      onCancel={() => handleExitStructureEditing(activePanel.id)}
                    />
                  ) : (
                    <StructureView
                      connectionId={connectionId}
                      tableName={activePanel.tableName}
                      onEditStructure={
                        showStructureEditor
                          ? () => handleEditTableStructure(activePanel.tableName)
                          : undefined
                      }
                    />
                  ))}
                {activePanel.subTab === 'indexes' && (
                  <IndexesView
                    connectionId={connectionId}
                    tableName={activePanel.tableName}
                    databaseType={databaseType}
                    onEditStructure={
                      showStructureEditor
                        ? () => handleSetSubTab(activePanel.id, 'structure')
                        : undefined
                    }
                  />
                )}
                {activePanel.subTab === 'foreignKeys' && (
                  <ForeignKeysView connectionId={connectionId} tableName={activePanel.tableName} />
                )}
                {activePanel.subTab === 'ddl' && (
                  <DDLView
                    connectionId={connectionId}
                    tableName={activePanel.tableName}
                    databaseType={databaseType}
                  />
                )}
              </div>
            </>
          )}

          {activePanel?.type === 'view' && (
            <>
              <div className="flex shrink-0 border-b border-edge bg-surface-alt">
                {getViewSubTabs(t).map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    className={cn(
                      'relative px-5 py-2 text-[13px] transition-colors',
                      activePanel.subTab === tab.id
                        ? 'bg-surface text-fg font-medium'
                        : 'text-fg-secondary hover:text-fg',
                    )}
                    onClick={() => handleSetSubTab(activePanel.id, tab.id)}
                  >
                    {tab.label}
                    <span
                      className={cn(
                        'absolute inset-x-0 bottom-0 h-0.5 bg-accent transition-opacity duration-300',
                        activePanel.subTab === tab.id ? 'opacity-100' : 'opacity-0',
                      )}
                    />
                  </button>
                ))}
              </div>

              <div className="flex min-h-0 flex-1 flex-col">
                {activePanel.subTab === 'data' && (
                  <TableView
                    connectionId={connectionId}
                    database={currentDatabase ?? ''}
                    tableName={activePanel.viewName}
                    databaseType={databaseType}
                    dataExportCapability={exportScope}
                  />
                )}
                {activePanel.subTab === 'structure' && (
                  <StructureView connectionId={connectionId} tableName={activePanel.viewName} />
                )}
                {activePanel.subTab === 'ddl' && (
                  <DDLView
                    connectionId={connectionId}
                    tableName={activePanel.viewName}
                    databaseType={databaseType}
                  />
                )}
              </div>
            </>
          )}

          {activePanel?.type === 'query' && (
            <QueryPanel
              connectionId={connectionId}
              configId={configId}
              queryTabId={activePanel.queryTabId}
              databaseType={databaseType}
            />
          )}

          {activePanel?.type === 'create-table' && (
            <TableStructureEditor
              connectionId={connectionId}
              databaseType={databaseType}
              schema={createTableSchema}
              mode="create"
              onSuccess={() => {
                handleClosePanel(activePanel.id);
                handleRefresh();
              }}
              onCancel={() => handleClosePanel(activePanel.id)}
            />
          )}

          {activePanel?.type === 'er-diagram' && currentDatabase && (
            <div className="flex min-h-0 flex-1 flex-col">
              <ErDiagramView
                connectionId={connectionId}
                database={currentDatabase}
                focusTable={(activePanel as ErDiagramPanel).focusTable}
                onSelectTable={handleSelectTable}
                onFocusTable={(table) => handleOpenErDiagram(table)}
              />
            </div>
          )}

          {activePanel?.type === 'objects' && (
            <ObjectBrowser connectionId={connectionId} databaseType={databaseType} />
          )}

          {activePanel?.type === 'privileges' && <PrivilegeView connectionId={connectionId} />}

          {activePanel?.type === 'db-object' && (
            <DatabaseObjectView
              connectionId={connectionId}
              databaseType={databaseType}
              objectKind={(activePanel as DatabaseObjectPanel).objectKind}
              objectName={(activePanel as DatabaseObjectPanel).objectName}
              objectSchema={(activePanel as DatabaseObjectPanel).objectSchema}
            />
          )}

          {!activePanel && (
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

        {aiChatOpen && (
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
                      (t) => t.id === (activePanel as QueryPanelType).queryTabId,
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
            getDbLabel(databaseType),
            connectionName,
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

      {exportOpen && exportTableName && (
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
          databaseType={databaseType}
          connectionId={connectionId}
          totalRows={totalRows}
          defaultScope="entire_table"
          dataExportCapability={exportScope}
        />
      )}

      <BatchExportDialog
        open={batchExportOpen}
        onClose={() => {
          setBatchExportOpen(false);
          setBatchExportInitialSelected([]);
        }}
        connectionId={connectionId}
        databaseType={databaseType}
        database={currentDatabase ?? undefined}
        tables={exportableTableNames}
        initialSelected={batchExportInitialSelected}
        loadTableExportData={loadTableExportData}
        dataExportCapability={exportScope}
      />

      <ImportDialog
        open={importOpen}
        onClose={() => {
          setImportOpen(false);
          setImportTableName(null);
        }}
        connectionId={connectionId}
        tableName={importTableName}
        onImported={handleRefresh}
        databaseType={databaseType}
      />

      {confirmActionDialog}
    </>
  );
}
