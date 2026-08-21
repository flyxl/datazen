import { useCallback, useMemo, useRef, type MouseEvent } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { useSchemaStore } from '../../stores/schemaStore';
import {
  usePanelStore,
  nextPanelId,
  type SubTabId,
  type TablePanel,
  type ViewPanel,
  type QueryPanel,
  type CreateTablePanel,
  type ErDiagramPanel,
  type ObjectsPanel,
  type PrivilegesPanel,
  type DatabaseObjectPanel,
  type ConnectionContext,
} from '../../stores/panelStore';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { buildConnectionTabContextMenuItems } from '../../lib/connectionTabContextMenu';

export interface PanelHandlers {
  handleSelectTable: (table: string, schema?: string, database?: string) => void;
  handleCreateTable: () => void;
  handleEditTableStructure: (name: string) => void;
  handleOpenStructure: (name: string) => void;
  handleExitStructureEditing: (panelId: string) => void;
  handleOpenErDiagram: (focus?: string) => void;
  handleOpenObjects: () => void;
  handleOpenDbObject: (
    kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
    name: string,
    schema?: string,
  ) => void;
  handleOpenPrivileges: () => void;
  handleNewQuery: (initialSql?: string) => void;
  handleOpenQueryHistory: () => void;
  handleClosePanel: (panelId: string) => void;
  handleCloseOtherPanels: (keepPanelId: string) => void;
  handleCloseAllPanels: () => void;
  handleClosePanelsToTheRight: (panelId: string) => void;
  handleClosePanelsToTheLeft: (panelId: string) => void;
  handlePanelTabContextMenu: (panelId: string, e: MouseEvent) => void;
  handleSetSubTab: (panelId: string, subTab: SubTabId) => void;
  handleRefresh: () => void;
}

export function usePanelHandlers({
  connCtx: sidebarConnCtx,
  showStructureEditor,
  currentDatabase,
  initialDatabase,
  lastTableSchema,
  schemaViews,
}: {
  connCtx: ConnectionContext | null;
  showStructureEditor: boolean;
  currentDatabase: string | null;
  initialDatabase: string | undefined;
  lastTableSchema: string | null;
  schemaViews: { name: string; schema?: string }[];
}): PanelHandlers {
  const { t } = useI18n();

  const connCtxRef = useRef(sidebarConnCtx);
  connCtxRef.current = sidebarConnCtx;

  const addPanel = usePanelStore((s) => s.addPanel);
  const removePanel = usePanelStore((s) => s.removePanel);
  const storeUpdatePanel = usePanelStore((s) => s.updatePanel);
  const setActivePanel = usePanelStore((s) => s.setActivePanel);
  const updateSql = usePanelStore((s) => s.updateSql);

  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const loadTables = useSchemaStore((s) => s.loadTables);

  const allPanels = usePanelStore((s) => s.panels);
  const connPanels = useMemo(
    () => (sidebarConnCtx ? allPanels.filter((p) => p.configId === sidebarConnCtx.configId) : []),
    [allPanels, sidebarConnCtx?.configId],
  );

  const handleSelectTable = useCallback(
    (table: string, schema?: string, database?: string) => {
      const ctx = sidebarConnCtx;
      if (!ctx) return;
      const currentPanels = usePanelStore
        .getState()
        .panels.filter((p) => p.configId === ctx.configId);
      const isView = schemaViews.some(
        (v) => v.name === table && (schema == null || v.schema === schema),
      );
      if (isView) {
        const existing = currentPanels.find(
          (p) =>
            p.type === 'view' &&
            p.viewName === table &&
            (database == null || p.database === database),
        );
        if (existing) {
          setActivePanel(existing.id);
          return;
        }
        const panel: ViewPanel = {
          ...ctx,
          type: 'view',
          id: nextPanelId('view'),
          viewName: table,
          database,
          viewSchema: schema,
          subTab: 'data',
        };
        addPanel(panel);
        return;
      }
      const existing = currentPanels.find(
        (p) =>
          p.type === 'table' &&
          p.tableName === table &&
          (database == null || p.database === database),
      );
      if (existing) {
        setActivePanel(existing.id);
        return;
      }
      const panel: TablePanel = {
        ...ctx,
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: table,
        database,
        tableSchema: schema,
        subTab: 'data',
      };
      addPanel(panel);
    },
    [sidebarConnCtx, schemaViews, addPanel, setActivePanel],
  );

  const handleCreateTable = useCallback(() => {
    if (!sidebarConnCtx) return;
    const existing = connPanels.find((p) => p.type === 'create-table');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: CreateTablePanel = {
      ...sidebarConnCtx,
      type: 'create-table',
      id: nextPanelId('new-tbl'),
      database: currentDatabase ?? initialDatabase ?? undefined,
      tableSchema: lastTableSchema ?? undefined,
    };
    addPanel(panel);
  }, [
    sidebarConnCtx,
    connPanels,
    addPanel,
    setActivePanel,
    currentDatabase,
    initialDatabase,
    lastTableSchema,
  ]);

  const handleEditTableStructure = useCallback(
    (name: string) => {
      if (!sidebarConnCtx) return;
      const existing = connPanels.find((p) => p.type === 'table' && p.tableName === name);
      if (existing) {
        setActivePanel(existing.id);
        storeUpdatePanel(existing.id, { subTab: 'structure', structureEditing: true });
        return;
      }
      const panel: TablePanel = {
        ...sidebarConnCtx,
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: name,
        subTab: 'structure',
        structureEditing: true,
      };
      addPanel(panel);
    },
    [sidebarConnCtx, connPanels, addPanel, setActivePanel, storeUpdatePanel],
  );

  const handleOpenStructure = useCallback(
    (name: string) => {
      if (!sidebarConnCtx) return;
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
        ...sidebarConnCtx,
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: name,
        subTab: 'structure',
      };
      addPanel(panel);
    },
    [
      sidebarConnCtx,
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
      if (!sidebarConnCtx) return;
      const existing = connPanels.find((p) => p.type === 'er-diagram');
      if (existing) {
        if (focus) storeUpdatePanel(existing.id, { focusTable: focus });
        setActivePanel(existing.id);
        return;
      }
      const panel: ErDiagramPanel = {
        ...sidebarConnCtx,
        type: 'er-diagram',
        id: nextPanelId('er'),
        focusTable: focus,
      };
      addPanel(panel);
    },
    [sidebarConnCtx, connPanels, addPanel, setActivePanel, storeUpdatePanel],
  );

  const handleOpenObjects = useCallback(() => {
    if (!sidebarConnCtx) return;
    const existing = connPanels.find((p) => p.type === 'objects');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: ObjectsPanel = { ...sidebarConnCtx, type: 'objects', id: nextPanelId('obj') };
    addPanel(panel);
  }, [sidebarConnCtx, connPanels, addPanel, setActivePanel]);

  const handleOpenDbObject = useCallback(
    (
      kind: 'function' | 'procedure' | 'trigger' | 'sequence' | 'type',
      name: string,
      schema?: string,
    ) => {
      if (!sidebarConnCtx) return;
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
        ...sidebarConnCtx,
        type: 'db-object',
        id: nextPanelId('dbobj'),
        objectKind: kind,
        objectName: name,
        objectSchema: schema,
      };
      addPanel(panel);
    },
    [sidebarConnCtx, connPanels, addPanel, setActivePanel],
  );

  const handleOpenPrivileges = useCallback(() => {
    if (!sidebarConnCtx) return;
    const existing = connPanels.find((p) => p.type === 'privileges');
    if (existing) {
      setActivePanel(existing.id);
      return;
    }
    const panel: PrivilegesPanel = {
      ...sidebarConnCtx,
      type: 'privileges',
      id: nextPanelId('priv'),
    };
    addPanel(panel);
  }, [sidebarConnCtx, connPanels, addPanel, setActivePanel]);

  const handleNewQuery = useCallback(
    (initialSql?: string) => {
      if (!sidebarConnCtx) return;
      const panelId = nextPanelId('qry');
      const db = currentDatabase ?? initialDatabase ?? '';
      const panel: QueryPanel = {
        ...sidebarConnCtx,
        type: 'query',
        id: panelId,
        title: db ? `${sidebarConnCtx.connectionName}@${db}` : sidebarConnCtx.connectionName,
      };
      addPanel(panel);
      if (initialSql) updateSql(panelId, initialSql);
    },
    [sidebarConnCtx, currentDatabase, initialDatabase, addPanel, updateSql],
  );

  const handleOpenQueryHistory = useCallback(() => {
    if (!sidebarConnCtx) return;
    const existing = connPanels.find((p) => p.type === 'query');
    if (existing) {
      setActivePanel(existing.id);
    } else {
      handleNewQuery();
    }
    void usePanelStore.getState().openQueryHistory(sidebarConnCtx.configId);
  }, [sidebarConnCtx, connPanels, setActivePanel, handleNewQuery]);

  const handleClosePanel = useCallback(
    (panelId: string) => {
      removePanel(panelId);
    },
    [removePanel],
  );

  const handleCloseOtherPanels = useCallback((keepPanelId: string) => {
    usePanelStore.getState().closeOtherPanels(keepPanelId);
  }, []);

  const handleCloseAllPanels = useCallback(() => {
    usePanelStore.getState().closeAllPanels();
  }, []);

  const handleClosePanelsToTheRight = useCallback((panelId: string) => {
    usePanelStore.getState().closePanelsToTheRight(panelId);
  }, []);

  const handleClosePanelsToTheLeft = useCallback((panelId: string) => {
    usePanelStore.getState().closePanelsToTheLeft(panelId);
  }, []);

  const handlePanelTabContextMenu = useCallback(
    (panelId: string, e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const panels = usePanelStore.getState().panels;
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
    ],
  );

  const handleSetSubTab = useCallback(
    (panelId: string, subTab: SubTabId) => {
      const p = usePanelStore.getState().panels.find((panel) => panel.id === panelId);
      if (!p) return;
      if (p.type === 'table') {
        storeUpdatePanel(panelId, {
          subTab,
          structureEditing: subTab === 'structure' ? (p as TablePanel).structureEditing : false,
        });
      } else if (p.type === 'view') {
        storeUpdatePanel(panelId, { subTab });
      }
    },
    [storeUpdatePanel],
  );

  const handleRefresh = useCallback(() => {
    if (!sidebarConnCtx?.connectionId) return;
    if (currentDatabase) {
      void loadTables(currentDatabase);
    } else {
      void loadForConnection(sidebarConnCtx.connectionId, {
        databaseType: sidebarConnCtx.databaseType,
      });
    }
  }, [sidebarConnCtx, currentDatabase, loadTables, loadForConnection]);

  return {
    handleSelectTable,
    handleCreateTable,
    handleEditTableStructure,
    handleOpenStructure,
    handleExitStructureEditing,
    handleOpenErDiagram,
    handleOpenObjects,
    handleOpenDbObject,
    handleOpenPrivileges,
    handleNewQuery,
    handleOpenQueryHistory,
    handleClosePanel,
    handleCloseOtherPanels,
    handleCloseAllPanels,
    handleClosePanelsToTheRight,
    handleClosePanelsToTheLeft,
    handlePanelTabContextMenu,
    handleSetSubTab,
    handleRefresh,
  };
}
