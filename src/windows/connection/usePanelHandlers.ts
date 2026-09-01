import { useCallback, useEffect, useMemo, useRef, type MouseEvent } from 'react';
import { useI18n } from '../../hooks/useI18n';
import type { ConnectionOpenTarget } from '../../lib/connectionViews/types';
import { useSchemaStore } from '../../stores/schemaStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
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
import {
  buildQueryOpenContext,
  type TableContextInput,
  type TableSqlActionKind,
} from '../../lib/tableSqlActions';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { splitPathHierarchyDatabasePin } from '../../lib/queryContextPath';

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
  handleOpenServerStatus: (ctx?: ConnectionOpenTarget) => void;
  handleOpenProcessList: (ctx?: ConnectionOpenTarget) => void;
  handleNewQuery: (
    initialSql?: string,
    context?: Pick<TableContextInput, 'database' | 'schema'>,
  ) => void;
  handleOpenTableAction: (context: TableContextInput, action: TableSqlActionKind) => void;
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

/**
 * 右键菜单打开「进程列表 / 服务器仪表盘」时，调用方会显式传入被点击的连接 target，
 * 面板据此绑定（connectionId + dbSessionId），不依赖「全局活动连接」，避免 MySQL/PG 串数据。
 * 未传入时（如从已绑定视图内部调用）回退到当前侧栏上下文。
 */
function resolveOpenTarget(
  target: ConnectionOpenTarget | undefined,
  sidebar: ConnectionContext | null,
): ConnectionContext | null {
  // 有显式目标时，以其 connectionId 为准，并从活动连接表中解析当前实时 dbSessionId，
  // 绝不对齐到传入的 target 可能带有的旧 id、也不回落到其它连接的上下文。
  if (target && target.connectionId) {
    const live = useActiveConnectionStore.getState().connections[target.connectionId]?.dbSessionId;
    return {
      connectionId: target.connectionId,
      dbSessionId: live || target.dbSessionId,
      connectionName: target.connectionName,
      databaseType: target.databaseType,
    };
  }
  return sidebar;
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
    () =>
      sidebarConnCtx ? allPanels.filter((p) => p.connectionId === sidebarConnCtx.connectionId) : [],
    [allPanels, sidebarConnCtx?.connectionId],
  );

  const handleSelectTable = useCallback(
    (table: string, schema?: string, database?: string) => {
      const ctx = sidebarConnCtx;
      if (!ctx) return;
      const currentPanels = usePanelStore
        .getState()
        .panels.filter((p) => p.connectionId === ctx.connectionId);
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

  const handleOpenServerStatus = useCallback(
    (target?: ConnectionOpenTarget) => {
      const ctx = resolveOpenTarget(target, sidebarConnCtx);
      if (!ctx) return;
      const all = usePanelStore.getState().panels;
      // 每个连接的服务器仪表盘面板唯一，按 connectionId 绑定，绝不复用其它连接的面板。
      const existing = all.find(
        (p) => p.type === 'server-status' && p.connectionId === ctx.connectionId,
      );
      if (existing) {
        setActivePanel(existing.id);
        return;
      }
      addPanel({ ...ctx, type: 'server-status', id: nextPanelId('status') });
    },
    [sidebarConnCtx, addPanel, setActivePanel],
  );

  const handleOpenProcessList = useCallback(
    (target?: ConnectionOpenTarget) => {
      const ctx = resolveOpenTarget(target, sidebarConnCtx);
      if (!ctx) return;
      const all = usePanelStore.getState().panels;
      const existing = all.find(
        (p) => p.type === 'processes' && p.connectionId === ctx.connectionId,
      );
      if (existing) {
        setActivePanel(existing.id);
        return;
      }
      addPanel({ ...ctx, type: 'processes', id: nextPanelId('proc') });
    },
    [sidebarConnCtx, addPanel, setActivePanel],
  );

  const handleNewQuery = useCallback(
    (initialSql?: string, target?: Pick<TableContextInput, 'database' | 'schema'>) => {
      if (!sidebarConnCtx) return;
      const panelId = nextPanelId('qry');
      let panelDatabase = target?.database?.trim() || undefined;
      let namespacePath: string[] | undefined;
      const meta = DB_REGISTRY[sidebarConnCtx.databaseType];
      if (meta?.namespaceEnsure === 'path-hierarchy' && panelDatabase?.includes('/')) {
        const split = splitPathHierarchyDatabasePin(panelDatabase);
        panelDatabase = split.root || undefined;
        namespacePath = split.namespacePath.length > 0 ? split.namespacePath : undefined;
      }
      const db = panelDatabase ?? currentDatabase ?? initialDatabase ?? '';
      const panel: QueryPanel = {
        ...sidebarConnCtx,
        type: 'query',
        id: panelId,
        title: db ? `${sidebarConnCtx.connectionName}@${db}` : sidebarConnCtx.connectionName,
        database: panelDatabase || undefined,
        schema: target?.schema?.trim() || undefined,
        namespacePath,
      };
      addPanel(panel);
      if (initialSql) updateSql(panelId, initialSql);
    },
    [sidebarConnCtx, currentDatabase, initialDatabase, addPanel, updateSql],
  );

  const handleOpenTableAction = useCallback(
    (input: TableContextInput, action: TableSqlActionKind) => {
      if (!sidebarConnCtx || input.connectionId !== sidebarConnCtx.connectionId) return;
      const context = buildQueryOpenContext(input, { kind: action, source: 'table-action' });
      if (action === 'openData') {
        handleSelectTable(context.tableName, context.schema, context.database);
        return;
      }
      handleNewQuery(context.initialSql, context);
    },
    [handleNewQuery, handleSelectTable, sidebarConnCtx],
  );

  const handleOpenQueryHistory = useCallback(() => {
    if (!sidebarConnCtx) return;
    const existing = connPanels.find((p) => p.type === 'query');
    if (existing) {
      setActivePanel(existing.id);
    } else {
      handleNewQuery();
    }
    void usePanelStore.getState().openQueryHistory(sidebarConnCtx.connectionId);
  }, [sidebarConnCtx, connPanels, setActivePanel, handleNewQuery]);

  // Consume pending query-history intent set by the navigator context menu
  // when the connection was not yet open at click time.
  useEffect(() => {
    const pendingId = usePanelStore.getState().pendingQueryHistoryConnectionId;
    if (!pendingId || !sidebarConnCtx || sidebarConnCtx.connectionId !== pendingId) return;
    usePanelStore.getState().setPendingQueryHistory(null);
    handleOpenQueryHistory();
  }, [sidebarConnCtx, handleOpenQueryHistory]);

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
    if (!sidebarConnCtx?.dbSessionId) return;
    if (currentDatabase) {
      void loadTables(currentDatabase);
    } else {
      void loadForConnection(sidebarConnCtx.dbSessionId, {
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
    handleOpenServerStatus,
    handleOpenProcessList,
    handleNewQuery,
    handleOpenTableAction,
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
