import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  BookOpen,
  ClipboardCopy,
  Code2,
  Database,
  Download,
  GitFork,
  MessageSquare,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Table2,
  TableProperties,
  Upload,
  X,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useResizable } from '../../hooks/useResizable';
import { useI18n } from '../../hooks/useI18n';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useQueryStore } from '../../stores/queryStore';
import { cn } from '../../lib/cn';
import { openDocsWindow } from '../../lib/windowManager';
import { DB_REGISTRY, getDbLabel } from '../../lib/databaseTypes';
import { canOpenStructureEditor } from '../../lib/structureEditor/canOpenStructureEditor';
import { resolveCreateTableSchema } from '../../lib/structureEditor/resolveCreateTableSchema';
import type { ConnectionViewProps } from '../../lib/connectionViews/types';
import { SchemaTree } from './schema-tree/SchemaTree';
import { StructureView } from './StructureView';
import { TableView } from './TableView';
import { IndexesView } from './IndexesView';
import { ForeignKeysView } from './ForeignKeysView';
import { DDLView } from './DDLView';
import { QueryPanel } from './QueryPanel';
import { ExportDialog } from './ExportDialog';
import { ImportDialog } from './ImportDialog';
import { TableStructureEditor } from './TableStructureEditor';
import { ContextMenu } from '../../components/ui/ContextMenu';
import type { ContextMenuEntry } from '../../components/ui/ContextMenu';
import type { TranslationKey } from '../../locales';
import { DetailPanel } from '../../components/DataTable/DetailPanel';
import { DetailPanelToggle } from '../../components/DataTable/DetailPanelToggle';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { AiChatPanel } from '../../components/ai/AiChatPanel';
import { rowToRecord } from '../../lib/rowToRecord';
import { ErDiagramView } from './ErDiagramView';

type SubTabId = 'data' | 'structure' | 'indexes' | 'foreignKeys' | 'ddl';

function getSubTabs(t: (key: TranslationKey) => string, readOnly?: boolean): { id: SubTabId; label: string }[] {
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

interface TablePanel {
  type: 'table';
  id: string;
  tableName: string;
  subTab: SubTabId;
}

interface QueryPanelInfo {
  type: 'query';
  id: string;
  queryTabId: string;
  title: string;
}

interface CreateTablePanel {
  type: 'create-table';
  id: string;
}

interface AlterTablePanel {
  type: 'alter-table';
  id: string;
  tableName: string;
}

interface ErDiagramPanel {
  type: 'er-diagram';
  id: string;
  focusTable?: string;
}

type Panel = TablePanel | QueryPanelInfo | CreateTablePanel | AlterTablePanel | ErDiagramPanel;

let panelCounter = 0;
function nextPanelId(prefix: string) {
  panelCounter += 1;
  return `${prefix}-${panelCounter}`;
}

export function SqlConnectionView({
  connectionId,
  connectionName,
  databaseType,
  initialDatabase,
}: ConnectionViewProps) {
  const { t } = useI18n();
  const dbMeta = DB_REGISTRY[databaseType];
  const isReadOnly = dbMeta?.readOnly === true;
  const showStructureEditor = canOpenStructureEditor(dbMeta) && !isReadOnly;
  const supportsErDiagram = dbMeta?.supportsErDiagram !== false;

  const [panels, setPanels] = useState<Panel[]>([]);
  const [activePanelId, setActivePanelId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [aiChatOpen, setAiChatOpen] = useState(false);
  // AI chat toggle is always visible; AiChatPanel handles unconfigured state
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [exportTableName, setExportTableName] = useState<string | null>(null);
  const [importTableName, setImportTableName] = useState<string | null>(null);
  const [tableCtx, setTableCtx] = useState<{ tableName: string; x: number; y: number } | null>(null);
  const [lastTableSchema, setLastTableSchema] = useState<string | null>(null);
  const tableCtxRef = useRef<HTMLDivElement>(null);

  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const schemaTables = useSchemaStore((s) => s.tables);
  const schemaViews = useSchemaStore((s) => s.views);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const loadTables = useSchemaStore((s) => s.loadTables);
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
  const setQueryConnectionId = useQueryStore((s) => s.setConnectionId);
  const queryTabs = useQueryStore((s) => s.tabs);
  const updateQuerySql = useQueryStore((s) => s.updateSql);
  const resultDetailRowIndex = useQueryStore((s) => s.resultDetailRowIndex);
  const updateResultCell = useQueryStore((s) => s.updateResultCell);

  const activePanel = panels.find((p) => p.id === activePanelId) ?? null;

  // The detail panel only makes sense while viewing row data (table "data" sub-tab
  // or query results); hide it on structure/indexes/foreign keys/DDL tabs.
  const detailPanelApplicable =
    activePanel != null && (activePanel.type !== 'table' || activePanel.subTab === 'data');

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
    if (connectionId) setQueryConnectionId(connectionId);
  }, [connectionId, setQueryConnectionId]);

  useEffect(() => {
    setDbType(databaseType);
  }, [databaseType, setDbType]);

  const handleSelectTable = useCallback((table: string, schema?: string) => {
    if (schema) setLastTableSchema(schema);
    console.log('[SqlConnectionView] select table', table);
    setPanels((prev) => {
      const existing = prev.find((p) => p.type === 'table' && p.tableName === table);
      if (existing) {
        setActivePanelId(existing.id);
        return prev;
      }
      const panel: TablePanel = {
        type: 'table',
        id: nextPanelId('tbl'),
        tableName: table,
        subTab: 'data',
      };
      setActivePanelId(panel.id);
      return [...prev, panel];
    });
  }, []);

  const handleCreateTable = useCallback(() => {
    const existing = panels.find((p) => p.type === 'create-table');
    if (existing) {
      setActivePanelId(existing.id);
      return;
    }
    const panel: CreateTablePanel = { type: 'create-table', id: nextPanelId('new-tbl') };
    setPanels((prev) => [...prev, panel]);
    setActivePanelId(panel.id);
  }, [panels]);

  const handleAlterTable = useCallback((name: string) => {
    const existing = panels.find((p) => p.type === 'alter-table' && p.tableName === name);
    if (existing) {
      setActivePanelId(existing.id);
      return;
    }
    const panel: AlterTablePanel = { type: 'alter-table', id: nextPanelId('alt-tbl'), tableName: name };
    setPanels((prev) => [...prev, panel]);
    setActivePanelId(panel.id);
  }, [panels]);

  const handleOpenErDiagram = useCallback((focus?: string) => {
    const existing = panels.find((p) => p.type === 'er-diagram');
    if (existing) {
      if (focus) {
        setPanels((prev) =>
          prev.map((p) =>
            p.id === existing.id ? { ...p, focusTable: focus } as ErDiagramPanel : p,
          ),
        );
      }
      setActivePanelId(existing.id);
      return;
    }
    const panel: ErDiagramPanel = {
      type: 'er-diagram',
      id: nextPanelId('er'),
      focusTable: focus,
    };
    setPanels((prev) => [...prev, panel]);
    setActivePanelId(panel.id);
  }, [panels]);

  const handleNewQuery = useCallback(() => {
    createQueryTab();
    const latestTab = useQueryStore.getState().tabs.at(-1);
    if (!latestTab) return;
    const panel: QueryPanelInfo = {
      type: 'query',
      id: nextPanelId('qry'),
      queryTabId: latestTab.id,
      title: latestTab.title,
    };
    setPanels((prev) => [...prev, panel]);
    setActivePanelId(panel.id);
  }, [createQueryTab]);

  const handleClosePanel = useCallback((panelId: string) => {
    setPanels((prev) => {
      const idx = prev.findIndex((p) => p.id === panelId);
      const closing = prev[idx];
      const next = prev.filter((p) => p.id !== panelId);

      if (closing?.type === 'query') {
        closeQueryTab(closing.queryTabId);
      }

      setActivePanelId((current) => {
        if (current !== panelId) return current;
        if (next.length === 0) return null;
        const newIdx = Math.min(idx, next.length - 1);
        return next[newIdx].id;
      });

      return next;
    });
  }, [closeQueryTab]);

  const handleSetSubTab = useCallback((panelId: string, subTab: SubTabId) => {
    setPanels((prev) =>
      prev.map((p) => (p.id === panelId && p.type === 'table' ? { ...p, subTab } : p)),
    );
  }, []);

  const handleRefresh = useCallback(() => {
    if (!connectionId) return;
    if (currentDatabase) {
      void loadTables(currentDatabase);
    } else {
      void loadForConnection(connectionId, { databaseType });
    }
  }, [connectionId, currentDatabase, databaseType, loadTables, loadForConnection]);

  const [createIndexTrigger, setCreateIndexTrigger] = useState(0);

  const contextMenuItems: ContextMenuEntry[] = useMemo(() => {
    if (!activePanel) {
      return [{ id: 'new-query', label: t('connWin.newQuery'), icon: <Plus className="h-3.5 w-3.5" /> }];
    }

    const common: ContextMenuEntry[] = [
      { id: 'refresh', label: t('connWin.refresh'), icon: <RefreshCw className="h-3.5 w-3.5" /> },
      { id: 'new-query', label: t('connWin.newQuery'), icon: <Plus className="h-3.5 w-3.5" /> },
    ];
    const sep: ContextMenuEntry = { id: 'sep1', separator: true };

    if (activePanel.type === 'table') {
      switch (activePanel.subTab) {
        case 'data':
          return [{ id: 'copy-cell', label: t('connWin.copyCell'), icon: <ClipboardCopy className="h-3.5 w-3.5" /> }, sep, ...common];
        case 'structure':
          if (!showStructureEditor) return common;
          return [{ id: 'edit-structure', label: t('connWin.editStructure'), icon: <Pencil className="h-3.5 w-3.5" /> }, sep, ...common];
        case 'indexes': {
          const entries: ContextMenuEntry[] = [];
          if (!isReadOnly) {
            entries.push({ id: 'create-index', label: t('connWin.newIndex'), icon: <Plus className="h-3.5 w-3.5" /> });
          }
          if (showStructureEditor) {
            entries.push({ id: 'edit-structure', label: t('connWin.editStructure'), icon: <Pencil className="h-3.5 w-3.5" /> });
          }
          if (entries.length === 0) return common;
          return [...entries, sep, ...common];
        }
        case 'ddl':
          return [{ id: 'copy-ddl', label: t('connWin.copyDDL'), icon: <ClipboardCopy className="h-3.5 w-3.5" /> }, sep, ...common];
        default:
          return common;
      }
    }

    return common;
  }, [activePanel, showStructureEditor, isReadOnly, t]);

  const handleContextAction = useCallback((id: string) => {
    switch (id) {
      case 'refresh': handleRefresh(); break;
      case 'new-query': handleNewQuery(); break;
      case 'copy-cell': {
        const sel = globalThis.getSelection()?.toString();
        if (sel) void navigator.clipboard.writeText(sel);
        break;
      }
      case 'copy-ddl': {
        const pre = document.querySelector('pre');
        if (pre?.textContent) void navigator.clipboard.writeText(pre.textContent);
        break;
      }
      case 'edit-structure': {
        if (activePanel?.type === 'table') handleAlterTable(activePanel.tableName);
        break;
      }
      case 'create-index': {
        setCreateIndexTrigger((v) => v + 1);
        break;
      }
    }
  }, [handleRefresh, handleNewQuery, activePanel, handleAlterTable]);

  const handleTableContextMenu = useCallback((name: string, x: number, y: number) => {
    setTableCtx({ tableName: name, x, y });
  }, []);

  const handleTableCtxAction = useCallback((action: 'export' | 'import' | 'er-focus') => {
    if (!tableCtx) return;
    const name = tableCtx.tableName;
    setTableCtx(null);
    if (action === 'export') {
      setExportTableName(name);
      handleSelectTable(name);
      setExportOpen(true);
    } else if (action === 'import') {
      setImportTableName(name);
      setImportOpen(true);
    } else if (action === 'er-focus') {
      handleOpenErDiagram(name);
    }
  }, [tableCtx, handleSelectTable, handleOpenErDiagram]);

  useEffect(() => {
    if (!tableCtx) return;
    const onMouseDown = (e: MouseEvent) => {
      if (tableCtxRef.current && !tableCtxRef.current.contains(e.target as Node)) {
        setTableCtx(null);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setTableCtx(null);
    };
    globalThis.addEventListener('mousedown', onMouseDown);
    globalThis.addEventListener('keydown', onKey);
    return () => {
      globalThis.removeEventListener('mousedown', onMouseDown);
      globalThis.removeEventListener('keydown', onKey);
    };
  }, [tableCtx]);

  useKeyboardShortcuts([
    { key: 'mod+b', scope: 'global', description: t('connWin.toggleSidebar') ?? 'Toggle Sidebar', action: () => setSidebarOpen((v) => !v) },
    { key: 'mod+n', scope: 'global', description: t('connWin.newQuery'), action: handleNewQuery },
    { key: 'mod+r', scope: 'global', description: t('connWin.refresh'), action: handleRefresh },
    {
      key: 'mod+w',
      scope: 'global',
      description: t('common.close'),
      action: () => { if (activePanelId) handleClosePanel(activePanelId); },
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

  const activeQueryTab = activePanel?.type === 'query'
    ? queryTabs.find((tab) => tab.id === (activePanel as QueryPanelInfo).queryTabId)
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
    if (activeQueryResult && resultDetailRowIndex !== null && resultDetailRowIndex < activeQueryResult.rows.length) {
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
        <Button variant="secondary" className="h-8 w-8 !px-0" title={`${t('connWin.refresh')} (⌘R)`} onClick={handleRefresh}>
          <RefreshCw className="h-4 w-4" />
        </Button>
        <Button variant="primary" className="h-8" onClick={handleNewQuery}>
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
        <div className="mx-1 h-6 w-px bg-edge" />

        <div className="relative min-w-0 max-w-[280px] flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t('connWin.searchTables')}
            className="h-8 pl-9 text-xs"
          />
        </div>

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
        {sidebarOpen && (
          <>
            <aside
              style={{ width: sidebarWidth }}
              className="flex shrink-0 flex-col overflow-y-auto border-r border-edge bg-surface-alt"
            >
              <SchemaTree
                connectionId={connectionId}
                databaseType={databaseType}
                initialDatabase={initialDatabase}
                selectedTable={activePanel?.type === 'table' ? activePanel.tableName : null}
                searchQuery={searchQuery}
                onSelectTable={handleSelectTable}
                onTableContextMenu={handleTableContextMenu}
              />
            </aside>

            <div
              ref={handleRef}
              className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/30"
            />
          </>
        )}

        <ContextMenu items={contextMenuItems} onAction={handleContextAction}>
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            {panels.length > 0 && (
              <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt">
                <div className="flex min-w-0 flex-1 overflow-x-auto">
                  {panels.map((panel) => {
                    const isActive = panel.id === activePanelId;
                    const iconMap = {
                      table: <Table2 className="h-3.5 w-3.5 shrink-0" />,
                      query: <Code2 className="h-3.5 w-3.5 shrink-0" />,
                      'create-table': <TableProperties className="h-3.5 w-3.5 shrink-0" />,
                      'alter-table': <Pencil className="h-3.5 w-3.5 shrink-0" />,
                      'er-diagram': <GitFork className="h-3.5 w-3.5 shrink-0" />,
                    };
                    const labelMap: Record<string, string> = {
                      table: (panel as TablePanel).tableName,
                      query: (panel as QueryPanelInfo).title,
                      'create-table': t('connWin.newTable'),
                      'alter-table': `${t('connWin.editStructure')} · ${(panel as AlterTablePanel).tableName}`,
                      'er-diagram': t('erDiagram.title'),
                    };
                    const icon = iconMap[panel.type];
                    const label = labelMap[panel.type];

                    return (
                      <div
                        key={panel.id}
                        className={cn(
                          'group relative flex items-center gap-1.5 border-r border-edge px-3 py-2 text-xs',
                          isActive
                            ? 'bg-surface text-fg'
                            : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                        )}
                      >
                        <button
                          type="button"
                          className="flex items-center gap-1.5"
                          onClick={() => setActivePanelId(panel.id)}
                        >
                          {icon}
                          <span className="max-w-[120px] truncate">{label}</span>
                        </button>
                        <button
                          type="button"
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
                <button
                  type="button"
                  className="shrink-0 px-2 py-2 text-fg-muted hover:text-fg"
                  title={`${t('connWin.newQuery')} (⌘N)`}
                  onClick={handleNewQuery}
                >
                  <Plus className="h-3.5 w-3.5" />
                </button>
              </div>
            )}

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
                    <TableView connectionId={connectionId} database={currentDatabase ?? ''} tableName={activePanel.tableName} />
                  )}
                  {activePanel.subTab === 'structure' && (
                    <StructureView
                      connectionId={connectionId}
                      tableName={activePanel.tableName}
                      onEditStructure={showStructureEditor ? handleAlterTable : undefined}
                    />
                  )}
                  {activePanel.subTab === 'indexes' && (
                    <IndexesView
                      connectionId={connectionId}
                      tableName={activePanel.tableName}
                      createIndexTrigger={createIndexTrigger}
                      databaseType={databaseType}
                      onEditStructure={showStructureEditor ? handleAlterTable : undefined}
                    />
                  )}
                  {activePanel.subTab === 'foreignKeys' && (
                    <ForeignKeysView connectionId={connectionId} tableName={activePanel.tableName} />
                  )}
                  {activePanel.subTab === 'ddl' && (
                    <DDLView connectionId={connectionId} tableName={activePanel.tableName} databaseType={databaseType} />
                  )}
                </div>
              </>
            )}

            {activePanel?.type === 'query' && (
              <QueryPanel connectionId={connectionId} queryTabId={activePanel.queryTabId} databaseType={databaseType} />
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

            {activePanel?.type === 'alter-table' && (
              <TableStructureEditor
                connectionId={connectionId}
                databaseType={databaseType}
                schema={resolveTableSchema(activePanel.tableName)}
                mode="alter"
                tableName={activePanel.tableName}
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
                />
              </div>
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
        </ContextMenu>

        {detailPanelApplicable && (
          <DetailPanel
            open={detailOpen}
            columns={detailColumnDefs}
            row={detailRow}
            rowIndex={detailRowIdx}
            selectedRows={activePanel?.type === 'table' ? selectedRows : undefined}
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
            <aside style={{ width: aiSidebarWidth }} className="shrink-0 border-l border-edge bg-surface">
              <AiChatPanel
              connectionId={connectionId}
              database={currentDatabase ?? undefined}
              onInsertSql={(sql) => {
                if (activePanel?.type === 'query') {
                  const tab = queryTabs.find((t) => t.id === (activePanel as QueryPanelInfo).queryTabId);
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
          <kbd className="font-mono">⌘N</kbd> {t('connWin.newQuery')} · <kbd className="font-mono">⌘R</kbd> {t('connWin.refresh')} · <kbd className="font-mono">⌘W</kbd> {t('common.close')} · <kbd className="font-mono">Space</kbd> {t('detail.title')}
        </div>
      </footer>

      {tableCtx && createPortal(
        <div
          ref={tableCtxRef}
          className="fixed z-[9999] min-w-[180px] rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
          style={{ left: tableCtx.x, top: tableCtx.y }}
        >
          {showStructureEditor && (
            <>
              <button
                type="button"
                className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
                onClick={() => { const name = tableCtx.tableName; setTableCtx(null); handleAlterTable(name); }}
              >
                <Pencil className="h-3.5 w-3.5" />
                {t('connWin.editTableStructure')}
              </button>
              <div className="my-1 h-px bg-edge" />
            </>
          )}
          {supportsErDiagram && (
            <button
              type="button"
              className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
              onClick={() => handleTableCtxAction('er-focus')}
            >
              <GitFork className="h-3.5 w-3.5" />
              {t('erDiagram.focusTable')}
            </button>
          )}
          <button
            type="button"
            className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
            onClick={() => handleTableCtxAction('export')}
          >
            <Download className="h-3.5 w-3.5" />
            {t('connWin.exportData')}
          </button>
          {!isReadOnly && (
            <button
              type="button"
              className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
              onClick={() => handleTableCtxAction('import')}
            >
              <Upload className="h-3.5 w-3.5" />
              {t('connWin.importData')}
            </button>
          )}
        </div>,
        document.body,
      )}

      {exportOpen && exportTableName && (
        <ExportDialog
          open={exportOpen}
          onClose={() => { setExportOpen(false); setExportTableName(null); }}
          tableName={exportTableName}
          columns={tableColumns}
          rows={tableRows}
          selectedRows={selectedRows}
          databaseType={databaseType}
        />
      )}

      <ImportDialog
        open={importOpen}
        onClose={() => { setImportOpen(false); setImportTableName(null); }}
        connectionId={connectionId}
        tableName={importTableName}
        onImported={handleRefresh}
        databaseType={databaseType}
      />
    </>
  );
}
