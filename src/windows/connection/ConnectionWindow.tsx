import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  ClipboardCopy,
  Code2,
  Database,
  Download,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Table2,
  TableProperties,
  Upload,
  X,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useResizable } from '../../hooks/useResizable';
import { useI18n } from '../../hooks/useI18n';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useSettingsStore } from '../../stores/settingsStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useQueryStore } from '../../stores/queryStore';
import { connectionCommands } from '../../commands/connection';
import { emitCrossWindow, listenCrossWindow } from '../../lib/crossWindowBus';
import { getUrlParam } from '../../lib/windowKind';
import { cn } from '../../lib/cn';
import { DB_REGISTRY, getDbLabel } from '../../lib/databaseTypes';
import type { DatabaseType } from '../../types';
import { SchemaTree } from './SchemaTree';
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

// ── Panel types ──

type SubTabId = 'data' | 'structure' | 'indexes' | 'foreignKeys' | 'ddl';

function getSubTabs(t: (key: TranslationKey) => string): { id: SubTabId; label: string }[] {
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

type Panel = TablePanel | QueryPanelInfo | CreateTablePanel | AlterTablePanel;

let panelCounter = 0;
function nextPanelId(prefix: string) {
  panelCounter += 1;
  return `${prefix}-${panelCounter}`;
}

// ── Component ──

export function ConnectionWindow() {
  useThemeListener();

  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const connectionId = getUrlParam('connectionId') ?? '';
  const connectionName = getUrlParam('connectionName') ?? t('connWin.connected');
  const databaseType = getUrlParam('databaseType') ?? 'postgresql';
  const initialDatabase = getUrlParam('database') ?? undefined;

  const [panels, setPanels] = useState<Panel[]>([]);
  const [activePanelId, setActivePanelId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [exportTableName, setExportTableName] = useState<string | null>(null);
  const [importTableName, setImportTableName] = useState<string | null>(null);
  const [tableCtx, setTableCtx] = useState<{ tableName: string; x: number; y: number } | null>(null);
  const tableCtxRef = useRef<HTMLDivElement>(null);

  const currentDatabase = useSchemaStore((s) => s.currentDatabase);
  const loadForConnection = useSchemaStore((s) => s.loadForConnection);
  const loadTables = useSchemaStore((s) => s.loadTables);
  const tableColumns = useTableDataStore((s) => s.columns);
  const tableRows = useTableDataStore((s) => s.rows);
  const totalRows = useTableDataStore((s) => s.totalRows);
  const selectedRows = useTableDataStore((s) => s.selectedRows);
  const tableName = useTableDataStore((s) => s.tableName);
  const setDbType = useTableDataStore((s) => s.setDatabaseType);

  const createQueryTab = useQueryStore((s) => s.createTab);
  const closeQueryTab = useQueryStore((s) => s.closeTab);
  const setQueryConnectionId = useQueryStore((s) => s.setConnectionId);

  const activePanel = panels.find((p) => p.id === activePanelId) ?? null;

  const { size: sidebarWidth, handleRef } = useResizable({
    direction: 'horizontal',
    initialSize: 280,
    minSize: 200,
    maxSize: 420,
    storageKey: 'connection.sidebar',
  });

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    if (connectionId) setQueryConnectionId(connectionId);
  }, [connectionId, setQueryConnectionId]);

  useEffect(() => {
    setDbType(databaseType);
  }, [databaseType, setDbType]);

  const handleSelectTable = useCallback((table: string) => {
    console.log('[ConnectionWindow] select table', table);
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
      void loadForConnection(connectionId);
    }
  }, [connectionId, currentDatabase, loadTables, loadForConnection]);

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
          return [{ id: 'edit-structure', label: t('connWin.editStructure'), icon: <Pencil className="h-3.5 w-3.5" /> }, sep, ...common];
        case 'indexes':
          return [{ id: 'create-index', label: t('connWin.newIndex'), icon: <Plus className="h-3.5 w-3.5" /> }, sep, ...common];
        case 'ddl':
          return [{ id: 'copy-ddl', label: t('connWin.copyDDL'), icon: <ClipboardCopy className="h-3.5 w-3.5" /> }, sep, ...common];
        default:
          return common;
      }
    }

    return common;
  }, [activePanel, t]);

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

  const handleTableCtxAction = useCallback((action: 'export' | 'import') => {
    if (!tableCtx) return;
    const name = tableCtx.tableName;
    setTableCtx(null);
    if (action === 'export') {
      setExportTableName(name);
      handleSelectTable(name);
      setExportOpen(true);
    } else {
      setImportTableName(name);
      setImportOpen(true);
    }
  }, [tableCtx, handleSelectTable]);

  // Close table context menu on outside click / Escape
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

  useEffect(() => {
    if (!connectionId) return;
    let unlisten: (() => void) | undefined;
    let isClosing = false;

    (async () => {
      if (!('__TAURI_INTERNALS__' in globalThis)) return;
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      unlisten = await win.onCloseRequested(async (event) => {
        if (isClosing) return;
        isClosing = true;
        event.preventDefault();
        try {
          await connectionCommands.disconnect(connectionId);
        } catch (e) {
          console.error('[ConnectionWindow] disconnect on close failed', e);
        }
        await emitCrossWindow('datazen:connection-closed', { connectionId });
        await win.close();
      });
    })();

    return () => unlisten?.();
  }, [connectionId]);

  useEffect(() => {
    if (!connectionId) return;
    let cleanup: (() => void) | undefined;
    listenCrossWindow('datazen:disconnect-requested', async (payload) => {
      const data = payload as { connectionId?: string } | undefined;
      if (data?.connectionId !== connectionId) return;
      if (!('__TAURI_INTERNALS__' in globalThis)) return;
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().destroy();
    }).then((unlisten) => { cleanup = unlisten; });
    return () => cleanup?.();
  }, [connectionId]);

  useKeyboardShortcuts([
    { key: 'mod+n', scope: 'global', description: t('connWin.newQuery'), action: handleNewQuery },
    { key: 'mod+r', scope: 'global', description: t('connWin.refresh'), action: handleRefresh },
    {
      key: 'mod+w',
      scope: 'global',
      description: t('common.close'),
      action: () => { if (activePanelId) handleClosePanel(activePanelId); },
    },
  ]);

  if (!connectionId) {
    return (
      <div className="flex h-screen items-center justify-center bg-surface text-fg">
        <div className="text-sm text-fg-muted">{t('connWin.missingParams')}</div>
      </div>
    );
  }

  const dbType = databaseType as DatabaseType;
  const isKeyValue = DB_REGISTRY[dbType]?.isKeyValue ?? false;
  const centerTitle = `${connectionName} - ${getDbLabel(dbType)} - DataZen`;

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      {/* Title bar */}
      <TitleBar
        title={centerTitle}
        leftContent={
          <div className="flex items-center gap-2">
            <span className="inline-flex h-2 w-2 rounded-full bg-green-500" />
            <span className="text-xs text-fg-secondary">{connectionName}</span>
          </div>
        }
      />

      {/* Toolbar */}
      <div className="flex h-12 min-h-[48px] shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
        <Button variant="secondary" className="h-8 w-8 !px-0" title={`${t('connWin.refresh')} (⌘R)`} onClick={handleRefresh}>
          <RefreshCw className="h-4 w-4" />
        </Button>
        <Button variant="primary" className="h-8" onClick={handleNewQuery}>
          <Plus className="h-4 w-4" />
          {t('connWin.newQuery')}
        </Button>
        {!isKeyValue && (
          <Button variant="secondary" className="h-8" onClick={handleCreateTable}>
            <TableProperties className="h-4 w-4" />
            {t('connWin.newTable')}
          </Button>
        )}
        <div className="mx-1 h-6 w-px bg-edge" />

        <div className="relative min-w-0 max-w-[280px] flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={isKeyValue ? t('connWin.searchKeys') : t('connWin.searchTables')}
            className="h-8 pl-9 text-xs"
          />
        </div>

        <div className="flex-1" />
      </div>

      {/* Main body */}
      <div className="flex min-h-0 flex-1">
        {/* Left sidebar */}
        <aside
          style={{ width: sidebarWidth }}
          className="flex shrink-0 flex-col overflow-y-auto border-r border-edge bg-surface-alt"
        >
          <SchemaTree
            connectionId={connectionId}
            databaseType={dbType}
            initialDatabase={initialDatabase}
            selectedTable={activePanel?.type === 'table' ? activePanel.tableName : null}
            searchQuery={searchQuery}
            onSelectTable={handleSelectTable}
            onTableContextMenu={isKeyValue ? undefined : handleTableContextMenu}
          />
        </aside>

        <div
          ref={handleRef}
          className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-blue-500/30"
        />

        {/* Right content — wrapped in context menu */}
        <ContextMenu items={contextMenuItems} onAction={handleContextAction}>
          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            {/* Top-level panel tabs */}
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
                    };
                    const labelMap: Record<string, string> = {
                      table: (panel as TablePanel).tableName,
                      query: (panel as QueryPanelInfo).title,
                      'create-table': t('connWin.newTable'),
                      'alter-table': `${t('connWin.editStructure')} · ${(panel as AlterTablePanel).tableName}`,
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
                        {isActive && (
                          <span className="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />
                        )}
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

            {/* Panel content */}
            {activePanel?.type === 'table' && isKeyValue && (
              <div className="flex min-h-0 flex-1 flex-col">
                <TableView connectionId={connectionId} tableName={activePanel.tableName} />
              </div>
            )}

            {activePanel?.type === 'table' && !isKeyValue && (
              <>
                {/* Sub-tab bar */}
                <div className="flex shrink-0 border-b border-edge bg-surface-alt">
                  {getSubTabs(t).map((tab) => (
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
                      {activePanel.subTab === tab.id && (
                        <span className="absolute inset-x-0 bottom-0 h-0.5 bg-blue-500" />
                      )}
                    </button>
                  ))}
                </div>

                {/* Sub-tab content */}
                <div className="flex min-h-0 flex-1 flex-col">
                  {activePanel.subTab === 'data' && (
                    <TableView connectionId={connectionId} tableName={activePanel.tableName} />
                  )}
                  {activePanel.subTab === 'structure' && (
                    <StructureView connectionId={connectionId} tableName={activePanel.tableName} onEditStructure={handleAlterTable} />
                  )}
                  {activePanel.subTab === 'indexes' && (
                    <IndexesView connectionId={connectionId} tableName={activePanel.tableName} createIndexTrigger={createIndexTrigger} databaseType={databaseType} />
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
              <QueryPanel connectionId={connectionId} queryTabId={activePanel.queryTabId} />
            )}

            {activePanel?.type === 'create-table' && (
              <TableStructureEditor
                connectionId={connectionId}
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
                mode="alter"
                tableName={activePanel.tableName}
                onSuccess={() => {
                  handleClosePanel(activePanel.id);
                  handleRefresh();
                }}
                onCancel={() => handleClosePanel(activePanel.id)}
              />
            )}

            {!activePanel && (
              <div className="flex flex-1 items-center justify-center text-fg-muted">
                <div className="text-center">
                  <Database className="mx-auto h-10 w-10 opacity-20" />
                  <div className="mt-3 text-sm">
                    {`${isKeyValue ? t('connWin.selectKey') : t('connWin.selectTable')} (⌘N ${t('connWin.newQuery')})`}
                  </div>
                </div>
              </div>
            )}
          </div>
        </ContextMenu>
      </div>

      {/* Status bar */}
      <footer className="flex h-10 min-h-[40px] shrink-0 items-center justify-between border-t border-edge bg-surface-alt px-4 text-xs text-fg-secondary">
        <div className="flex items-center gap-2">
          <span className="inline-flex h-2 w-2 rounded-full bg-green-500" />
          <span>{t('connWin.connected')}</span>
        </div>
        <div className="truncate text-fg-muted">
          {[
            getDbLabel(dbType),
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
          <kbd className="font-mono">⌘N</kbd> {t('connWin.newQuery')} · <kbd className="font-mono">⌘R</kbd> {t('connWin.refresh')} · <kbd className="font-mono">⌘W</kbd> {t('common.close')}
        </div>
      </footer>

      {/* Table context menu (right-click on table name in SchemaTree) */}
      {tableCtx && createPortal(
        <div
          ref={tableCtxRef}
          className="fixed z-[9999] min-w-[180px] rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
          style={{ left: tableCtx.x, top: tableCtx.y }}
        >
          <button
            type="button"
            className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
            onClick={() => { const name = tableCtx.tableName; setTableCtx(null); handleAlterTable(name); }}
          >
            <Pencil className="h-3.5 w-3.5" />
            {t('connWin.editTableStructure')}
          </button>
          <div className="my-1 h-px bg-edge" />
          <button
            type="button"
            className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
            onClick={() => handleTableCtxAction('export')}
          >
            <Download className="h-3.5 w-3.5" />
            {t('connWin.exportData')}
          </button>
          <button
            type="button"
            className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
            onClick={() => handleTableCtxAction('import')}
          >
            <Upload className="h-3.5 w-3.5" />
            {t('connWin.importData')}
          </button>
        </div>,
        document.body,
      )}

      {/* Dialogs */}
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
    </div>
  );
}
