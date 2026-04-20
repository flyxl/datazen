import { useCallback, useEffect, useMemo, useState } from 'react';
import { LayoutGrid, List, Plus, Search } from 'lucide-react';
import { Toolbar } from '../../components/Toolbar';
import { StatusBar } from '../../components/StatusBar';
import { Dialog } from '../../components/ui/Dialog';
import { Input } from '../../components/ui/Input';
import { Button } from '../../components/ui/Button';
import { TrafficLights } from '../../components/TrafficLights';
import { useKeyboardShortcuts } from '../../hooks/useKeyboardShortcuts';
import { useResizable } from '../../hooks/useResizable';
import { useTauriEvent } from '../../hooks/useTauriEvent';
import { useThemeListener } from '../../hooks/useThemeListener';
import { filterConnections, useConnectionStore } from '../../stores/connectionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useUiStore } from '../../stores/uiStore';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { cn } from '../../lib/cn';
import { listenCrossWindow } from '../../lib/crossWindowBus';
import { openConnectionWindow, openNewConnectionWindow, openSettingsWindow } from '../../lib/windowManager';
import { ThemeToggle } from '../../components/ThemeToggle';
import { GroupPanel } from './GroupPanel';
import { ConnectionCard } from './ConnectionCard';

export function MainWindow() {
  useTauriEvent();
  useThemeListener();

  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);
  const connections = useConnectionStore((s) => s.connections);
  const groups = useConnectionStore((s) => s.groups);
  const selectedGroup = useConnectionStore((s) => s.selectedGroup);
  const searchQuery = useConnectionStore((s) => s.searchQuery);
  const setSelectedGroup = useConnectionStore((s) => s.setSelectedGroup);
  const setSearchQuery = useConnectionStore((s) => s.setSearchQuery);
  const addGroup = useConnectionStore((s) => s.addGroup);
  const duplicateConnection = useConnectionStore((s) => s.duplicateConnection);
  const deleteConnection = useConnectionStore((s) => s.deleteConnection);
  const loading = useConnectionStore((s) => s.loading);
  const error = useConnectionStore((s) => s.error);

  const loadSettings = useSettingsStore((s) => s.loadSettings);

  const mainSidebarWidth = useUiStore((s) => s.mainSidebarWidth);
  const setMainSidebarWidth = useUiStore((s) => s.setMainSidebarWidth);
  const connectionsViewMode = useUiStore((s) => s.connectionsViewMode);
  const setConnectionsViewMode = useUiStore((s) => s.setConnectionsViewMode);

  const connectAction = useActiveConnectionStore((s) => s.connect);
  const disconnectAction = useActiveConnectionStore((s) => s.disconnect);
  const activeConnections = useActiveConnectionStore((s) => s.connections);

  const [errorDialogOpen, setErrorDialogOpen] = useState(false);
  const [errorMessage, setErrorMessage] = useState('');

  const lastError = useMemo(() => {
    for (const entry of Object.values(activeConnections)) {
      if (entry.status === 'error' && entry.error) return entry.error;
    }
    return null;
  }, [activeConnections]);

  useEffect(() => {
    if (lastError) {
      setErrorMessage(lastError);
      setErrorDialogOpen(true);
    }
  }, [lastError]);

  const handleConnect = useCallback(async (cfg: import('../../types').ConnectionConfig) => {
    await connectAction(cfg);
    const entry = useActiveConnectionStore.getState().connections[cfg.id];
    if (entry?.status === 'connected' && entry.connectionId) {
      console.log('[MainWindow] opening connection window', entry.connectionId, cfg.name);
      openConnectionWindow(entry.connectionId, cfg.name, cfg.database, cfg.databaseType);
    }
  }, [connectAction]);

  const { size: sidebarWidth, handleRef } = useResizable({
    direction: 'horizontal',
    initialSize: mainSidebarWidth,
    minSize: 180,
    maxSize: 520,
    storageKey: 'main.sidebar',
  });

  useEffect(() => {
    setMainSidebarWidth(sidebarWidth);
  }, [setMainSidebarWidth, sidebarWidth]);

  useEffect(() => {
    void fetchConnections();
    void fetchGroups();
    void loadSettings();
  }, [fetchConnections, fetchGroups, loadSettings]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listenCrossWindow('datazen:connection-closed', (payload) => {
      const data = payload as { connectionId?: string } | undefined;
      if (data?.connectionId) {
        useActiveConnectionStore.getState().removeByConnectionId(data.connectionId);
      }
    }).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void listenCrossWindow('menu:open-settings', () => {
      if (!cancelled) openSettingsWindow();
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else cleanup = unlisten;
    });
    return () => { cancelled = true; cleanup?.(); };
  }, []);

  const filtered = useMemo(
    () => filterConnections(connections, selectedGroup, searchQuery),
    [connections, searchQuery, selectedGroup],
  );

  useKeyboardShortcuts([
    {
      key: 'mod+n',
      scope: 'global',
      description: '新建连接',
      action: () => openNewConnectionWindow(),
    },
  ]);

  const activeCount = useMemo(
    () => Object.values(activeConnections).filter((e) => e.status === 'connected').length,
    [activeConnections],
  );

  const statusLeft = (() => {
    if (loading) return '加载中…';
    if (error) return <span className="text-red-400">{error}</span>;
    if (activeCount > 0) {
      return <span className="text-green-400">{activeCount} 个活跃连接</span>;
    }
    return '就绪';
  })();

  return (
    <div className="flex h-screen min-h-0 min-w-[720px] flex-col bg-surface text-fg">
      <header className="relative flex h-10 min-h-[40px] shrink-0 items-center bg-titlebar">
        <div className="absolute inset-0" data-tauri-drag-region />
        <div className="relative z-10 px-3">
          <TrafficLights />
        </div>
        <div className="pointer-events-none flex min-w-0 flex-1 justify-center">
          <div className="truncate text-xs font-medium text-fg-secondary">DataZen</div>
        </div>
        <div className="w-[72px] shrink-0" />
      </header>

      <Toolbar
        left={
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <div className="relative min-w-0 max-w-xl flex-1">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
              <Input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="搜索连接…"
                className="pl-9"
              />
            </div>
          </div>
        }
        right={
          <>
            <div className="hidden items-center gap-1 rounded-md border border-edge bg-surface p-1 sm:flex">
              <button
                type="button"
                className={cn(
                  'inline-flex h-8 w-8 items-center justify-center rounded-md text-fg-muted hover:bg-surface-alt hover:text-fg',
                  connectionsViewMode === 'grid' && 'bg-surface-alt text-fg',
                )}
                title="卡片视图"
                onClick={() => setConnectionsViewMode('grid')}
              >
                <LayoutGrid className="h-4 w-4" />
              </button>
              <button
                type="button"
                className={cn(
                  'inline-flex h-8 w-8 items-center justify-center rounded-md text-fg-muted hover:bg-surface-alt hover:text-fg',
                  connectionsViewMode === 'list' && 'bg-surface-alt text-fg',
                )}
                title="列表视图"
                onClick={() => setConnectionsViewMode('list')}
              >
                <List className="h-4 w-4" />
              </button>
            </div>
            <Button variant="primary" onClick={() => openNewConnectionWindow()}>
              <Plus className="h-4 w-4" />
              新建连接
            </Button>
            <ThemeToggle />
          </>
        }
      />

      <div className="flex min-h-0 flex-1">
        <aside
          style={{ width: sidebarWidth }}
          className="shrink-0 border-r border-edge bg-surface-alt"
        >
          <GroupPanel
            groups={groups}
            selectedGroup={selectedGroup}
            onSelectGroup={setSelectedGroup}
            onAddGroup={(name) => void addGroup(name)}
          />
        </aside>
        <div
          ref={handleRef}
          className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-blue-500/30"
          title="拖拽调整侧边栏宽度"
        />

        <main className="min-w-0 flex-1 overflow-auto p-6">
          {connectionsViewMode === 'grid' ? (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-4">
              {filtered.map((c) => (
                <ConnectionCard
                  key={c.id}
                  connection={c}
                  viewMode="grid"
                  status={activeConnections[c.id]?.status ?? 'idle'}
                  onConnect={(cfg) => void handleConnect(cfg)}
                  onDisconnect={() => void disconnectAction(c.id)}
                  onEdit={(id) => openNewConnectionWindow(id)}
                  onDuplicate={(id) => void duplicateConnection(id)}
                  onDelete={(id) => void deleteConnection(id)}
                />
              ))}
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              {filtered.map((c) => (
                <ConnectionCard
                  key={c.id}
                  connection={c}
                  viewMode="list"
                  status={activeConnections[c.id]?.status ?? 'idle'}
                  onConnect={(cfg) => void handleConnect(cfg)}
                  onDisconnect={() => void disconnectAction(c.id)}
                  onEdit={(id) => openNewConnectionWindow(id)}
                  onDuplicate={(id) => void duplicateConnection(id)}
                  onDelete={(id) => void deleteConnection(id)}
                />
              ))}
            </div>
          )}

          {filtered.length === 0 ? (
            <div className="mt-10 text-center text-sm text-fg-muted">没有匹配的连接</div>
          ) : null}
        </main>
      </div>

      <Dialog
        open={errorDialogOpen}
        title="连接失败"
        onClose={() => setErrorDialogOpen(false)}
        footer={
          <Button variant="primary" onClick={() => setErrorDialogOpen(false)}>
            确定
          </Button>
        }
      >
        <p className="whitespace-pre-wrap break-all text-sm text-red-400">{errorMessage}</p>
      </Dialog>

      <StatusBar
        left={
          <span className="truncate">
            {statusLeft}
            <span className="mx-2 text-edge">|</span>
            <span title="连接数量">连接：{filtered.length}</span>
          </span>
        }
        right={<span className="tabular-nums">DataZen v1.0.0</span>}
      />
    </div>
  );
}
