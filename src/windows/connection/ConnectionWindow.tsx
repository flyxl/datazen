import { useCallback, useEffect, useRef, useState, type MutableRefObject } from 'react';
import { PanelLeftOpen } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { useI18n } from '../../hooks/useI18n';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { useQueryStore } from '../../stores/queryStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { connectionCommands } from '../../commands/connection';
import { emitCrossWindow, listenCrossWindow } from '../../lib/crossWindowBus';
import { DB_REGISTRY, getDbLabel } from '../../lib/databaseTypes';
import { getConnectionView } from '../../lib/connectionViews';
import { openNewConnectionWindow, PENDING_CONNECTION_KEY } from '../../lib/windowManager';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import {
  ConnectionShareDialog,
  type ConnectionShareMode,
} from '../../components/connection/ConnectionShareDialog';
import { ConnectionNavigatorTree } from './ConnectionNavigatorTree';
import type { DatabaseType } from '../../types';

// ── Connection Tab ────────────────────────────────────────────────

interface ConnectionTab {
  configId: string;
  connectionId: string;
  connectionName: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  status: 'connecting' | 'connected' | 'error';
  error?: string;
}

function makeTabFromPayload(data: Record<string, string>): ConnectionTab | null {
  const configId = data.configId ?? '';
  if (!configId) return null;
  const connectionId = data.connectionId ?? '';
  return {
    configId,
    connectionId,
    connectionName: data.connectionName ?? '',
    databaseType: (data.databaseType ?? 'postgresql') as DatabaseType,
    initialDatabase: data.database,
    status: connectionId ? 'connected' : 'connecting',
  };
}

function consumePendingConnection(): ConnectionTab | null {
  try {
    const raw = localStorage.getItem(PENDING_CONNECTION_KEY);
    localStorage.removeItem(PENDING_CONNECTION_KEY);
    if (!raw) return null;
    return makeTabFromPayload(JSON.parse(raw));
  } catch {
    return null;
  }
}

// ── Sync all keyed stores to a specific connection ────────────────

function syncStoresActiveConnection(connectionId: string | null) {
  useSchemaStore.getState().setActiveConnection(connectionId);
  useQueryStore.getState().setActiveConnection(connectionId);
  useTableDataStore.getState().setActiveConnection(connectionId);
}

function removeConnectionFromStores(connectionId: string) {
  useSchemaStore.getState().removeConnection(connectionId);
  useQueryStore.getState().removeConnection(connectionId);
  useTableDataStore.getState().removeConnection(connectionId);
}

// ── Component ─────────────────────────────────────────────────────

export function ConnectionWindow() {
  useSettings();

  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const loadAiConfig = useAiStore((s) => s.loadConfig);
  const setupAiListeners = useAiStore((s) => s.setupEventListeners);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);
  const fetchGroups = useConnectionStore((s) => s.fetchGroups);
  const deleteConnection = useConnectionStore((s) => s.deleteConnection);
  const connections = useConnectionStore((s) => s.connections);
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const [connShareOpen, setConnShareOpen] = useState(false);
  const [connShareMode, setConnShareMode] = useState<ConnectionShareMode>('export');

  const [tabs, setTabs] = useState<ConnectionTab[]>(() => {
    const pending = consumePendingConnection();
    return pending ? [pending] : [];
  });
  const [activeIdx, setActiveIdx] = useState(0);
  const [sidebarWidth, setSidebarWidth] = useState(280);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const isResizingRef = useRef(false);
  const resizeHandleRef = useRef<HTMLDivElement>(null);
  const selectTableRef = useRef<((table: string, schema?: string) => void) | undefined>();
  const nodeContextMenuRef = useRef<
    | ((payload: { kind: string; name: string; x: number; y: number; schema?: string }) => void)
    | undefined
  >();
  const actionsRef = useRef<
    | {
        newQuery: (initialSql?: string) => void;
        openErDiagram: (focusTable?: string) => void;
        refresh: () => void;
      }
    | undefined
  >();

  const activeTab = tabs[activeIdx] ?? null;

  // ── Load connections + settings — fire-and-forget, once ──

  useEffect(() => {
    void loadSettings();
    void loadAiConfig();
    void fetchConnections();
    void fetchGroups();
    const aiCleanupPromise = setupAiListeners();
    return () => {
      void aiCleanupPromise.then((fn) => fn());
    };
  }, [loadSettings, loadAiConfig, setupAiListeners, fetchConnections, fetchGroups]);

  // Refresh connections when other windows update them
  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenCrossWindow('datazen:connections-changed', () => {
      void fetchConnections();
      void fetchGroups();
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [fetchConnections, fetchGroups]);

  // ── Sync keyed stores whenever active connection changes ──

  useEffect(() => {
    const connId = activeTab?.connectionId || null;
    syncStoresActiveConnection(connId);
  }, [activeTab?.connectionId]);

  // ── Connect the initial tab (if not already connected) ──

  const connectTab = useCallback(async (tab: ConnectionTab): Promise<string> => {
    const store = useActiveConnectionStore.getState();
    const existing = store.connections[tab.configId];
    if (existing?.status === 'connected' && existing.connectionId) {
      return existing.connectionId;
    }
    store.markConnecting(tab.configId, tab.initialDatabase ?? null);
    return connectionCommands.connect(tab.configId);
  }, []);

  useEffect(() => {
    if (tabs.length === 0) return;
    const pending = tabs.filter((t) => t.status === 'connecting' && !t.connectionId);
    for (const tab of pending) {
      void (async () => {
        try {
          const connId = await connectTab(tab);
          useActiveConnectionStore.getState().markConnected(tab.configId, connId);
          setTabs((prev) =>
            prev.map((t) =>
              t.configId === tab.configId
                ? { ...t, connectionId: connId, status: 'connected', error: undefined }
                : t,
            ),
          );
          void emitCrossWindow('datazen:connection-ready', {
            configId: tab.configId,
            connectionId: connId,
          });
        } catch (e) {
          const msg =
            typeof e === 'string' ? e : e instanceof Error ? e.message : t('backend.unknownError');
          useActiveConnectionStore.getState().markError(tab.configId, msg);
          setTabs((prev) =>
            prev.map((tb) =>
              tb.configId === tab.configId ? { ...tb, status: 'error', error: msg } : tb,
            ),
          );
        }
      })();
    }
  }, [tabs, connectTab, t]);

  // ── Listen for new connection requests from MainWindow ──

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenCrossWindow('datazen:open-connection', (payload) => {
      const data = payload as Record<string, string> | undefined;
      if (!data?.configId) return;

      try {
        localStorage.removeItem(PENDING_CONNECTION_KEY);
      } catch {}

      const newTab = makeTabFromPayload(data);
      if (!newTab) return;

      if (newTab.connectionId) syncStoresActiveConnection(newTab.connectionId);

      setTabs((prev) => {
        const existingIdx = prev.findIndex((t) => t.configId === newTab.configId);
        if (existingIdx >= 0) {
          if (prev[existingIdx].connectionId) {
            syncStoresActiveConnection(prev[existingIdx].connectionId);
          }
          setActiveIdx(existingIdx);
          return prev;
        }
        const next = [...prev, newTab];
        setActiveIdx(next.length - 1);
        return next;
      });
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, []);

  // ── Listen for cross-window connection lifecycle events ──

  useEffect(() => {
    const cleanups: (() => void)[] = [];

    void listenCrossWindow('datazen:connection-ready', (payload) => {
      const data = payload as { configId?: string; connectionId?: string } | undefined;
      if (!data?.configId || !data?.connectionId) return;
      setTabs((prev) =>
        prev.map((tab) =>
          tab.configId === data.configId
            ? { ...tab, connectionId: data.connectionId!, status: 'connected', error: undefined }
            : tab,
        ),
      );
    }).then((fn) => cleanups.push(fn));

    void listenCrossWindow('datazen:connection-failed', (payload) => {
      const data = payload as { configId?: string; error?: string } | undefined;
      if (!data?.configId) return;
      setTabs((prev) =>
        prev.map((tab) =>
          tab.configId === data.configId
            ? { ...tab, status: 'error', error: data.error ?? 'Unknown error' }
            : tab,
        ),
      );
    }).then((fn) => cleanups.push(fn));

    void listenCrossWindow('datazen:disconnect-requested', (payload) => {
      const data = payload as { connectionId?: string } | undefined;
      if (!data?.connectionId) return;
      setTabs((prev) => prev.filter((t) => t.connectionId !== data.connectionId));
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  }, []);

  // ── Heartbeat for all connected tabs ──

  useEffect(() => {
    const HEARTBEAT_MS = 5 * 60 * 1000;
    const timer = setInterval(() => {
      for (const tab of tabs) {
        if (tab.connectionId && tab.status === 'connected') {
          connectionCommands.pingConnection(tab.connectionId).catch(() => {});
        }
      }
    }, HEARTBEAT_MS);
    return () => clearInterval(timer);
  }, [tabs]);

  // ── Window close → release all connections ──

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in globalThis)) return;
    let unlisten: (() => void) | undefined;
    let isClosing = false;

    void (async () => {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      unlisten = await win.onCloseRequested(async (event) => {
        if (isClosing) return;
        isClosing = true;
        event.preventDefault();
        for (const tab of tabs) {
          if (tab.connectionId) {
            try {
              const wasDisconnected = await connectionCommands.releaseConnection(tab.connectionId);
              if (wasDisconnected) {
                await emitCrossWindow('datazen:connection-closed', {
                  connectionId: tab.connectionId,
                });
              }
            } catch {
              // best effort
            }
          }
        }
        await win.close();
      });
    })();

    return () => unlisten?.();
  }, [tabs]);

  // ── Close a connection tab (and cleanup all store data) ──

  const handleCloseTab = useCallback(
    async (configId: string) => {
      const idx = tabs.findIndex((t) => t.configId === configId);
      const tab = tabs[idx];
      if (!tab) return;

      if (tab.connectionId) {
        removeConnectionFromStores(tab.connectionId);
        useActiveConnectionStore.getState().removeByConnectionId(tab.connectionId);
        try {
          const wasDisconnected = await connectionCommands.releaseConnection(tab.connectionId);
          if (wasDisconnected) {
            void emitCrossWindow('datazen:connection-closed', {
              connectionId: tab.connectionId,
            });
          }
        } catch {
          // best effort
        }
      }

      setTabs((prev) => {
        const next = prev.filter((_, i) => i !== idx);
        return next;
      });

      setActiveIdx((prev) => {
        if (idx < prev) return prev - 1;
        if (idx === prev) return Math.min(prev, tabs.length - 2);
        return prev;
      });
    },
    [tabs],
  );

  // ── Tree callbacks ──

  const handleSelectConnection = useCallback(
    (configId: string) => {
      const existingIdx = tabs.findIndex((t) => t.configId === configId);
      if (existingIdx >= 0) {
        const existingTab = tabs[existingIdx];
        if (existingTab.connectionId) {
          syncStoresActiveConnection(existingTab.connectionId);
        }
        setActiveIdx(existingIdx);
        return;
      }
      const conn = connections.find((c) => c.id === configId);
      if (!conn) return;

      const entry = useActiveConnectionStore.getState().connections[configId];
      const connId = entry?.connectionId ?? '';
      const newTab: ConnectionTab = {
        configId,
        connectionId: connId,
        connectionName: conn.name,
        databaseType: conn.databaseType,
        initialDatabase: conn.database,
        status: connId ? 'connected' : 'connecting',
      };
      if (connId) syncStoresActiveConnection(connId);
      setTabs((prev) => {
        const next = [...prev, newTab];
        setActiveIdx(next.length - 1);
        return next;
      });
    },
    [tabs, connections],
  );

  const handleDeleteConnection = useCallback(
    (configId: string) => {
      const conn = connections.find((c) => c.id === configId);
      if (!conn) return;
      void confirmDelete({
        title: t('main.ctx.deleteConnection'),
        message: t('main.ctx.confirmDeleteConnection', { name: conn.name }),
        kind: 'warning',
      }).then((ok) => {
        if (!ok) return;
        void handleCloseTab(configId);
        void deleteConnection(configId);
      });
    },
    [connections, confirmDelete, handleCloseTab, deleteConnection, t],
  );

  const handleDisconnect = useCallback(
    (configId: string) => {
      void handleCloseTab(configId);
    },
    [handleCloseTab],
  );

  const handleSelectTable = useCallback((tableName: string, schema?: string) => {
    selectTableRef.current?.(tableName, schema);
  }, []);

  const handleRefresh = useCallback(() => {
    void fetchConnections();
    void fetchGroups();
  }, [fetchConnections, fetchGroups]);

  useEffect(() => {
    const handle = resizeHandleRef.current;
    if (!handle) return;

    const clampWidth = (width: number) => Math.max(200, Math.min(500, width));

    const onMouseDown = (e: MouseEvent) => {
      e.preventDefault();
      isResizingRef.current = true;
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
    };

    const onMouseMove = (e: MouseEvent) => {
      if (!isResizingRef.current) return;
      setSidebarWidth(clampWidth(e.clientX));
    };

    const onMouseUp = () => {
      if (!isResizingRef.current) return;
      isResizingRef.current = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    handle.addEventListener('mousedown', onMouseDown);
    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);

    return () => {
      handle.removeEventListener('mousedown', onMouseDown);
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, []);

  // ── Render ──

  const connectedTabs = tabs.filter(
    (t): t is ConnectionTab & { connectionId: string } =>
      t.status === 'connected' && !!t.connectionId,
  );
  const centerTitle = activeTab
    ? `${activeTab.connectionName} - ${getDbLabel(activeTab.databaseType)} - DataZen`
    : 'DataZen';

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar title={centerTitle} />

      <div className="flex min-h-0 flex-1">
        {/* ── Left navigator tree ── */}
        {sidebarCollapsed ? (
          <div className="flex shrink-0 flex-col items-center border-r border-edge bg-surface-alt py-2">
            <button
              type="button"
              className="flex h-7 w-7 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg"
              onClick={() => setSidebarCollapsed(false)}
              title={t('connWin.expandSidebar')}
            >
              <PanelLeftOpen className="h-4 w-4" />
            </button>
          </div>
        ) : (
          <>
            <aside
              style={{ width: sidebarWidth }}
              className="flex shrink-0 flex-col border-r border-edge bg-surface-alt"
            >
              <ConnectionNavigatorTree
                activeConfigId={activeTab?.configId ?? null}
                onSelectConnection={handleSelectConnection}
                onSelectTable={handleSelectTable}
                onNewConnection={() => openNewConnectionWindow()}
                onRefresh={handleRefresh}
                onExportConnections={() => {
                  setConnShareMode('export');
                  setConnShareOpen(true);
                }}
                onImportConnections={() => {
                  setConnShareMode('import');
                  setConnShareOpen(true);
                }}
                onEditConnection={(id) => openNewConnectionWindow(id)}
                onDeleteConnection={handleDeleteConnection}
                onDisconnect={handleDisconnect}
                onCollapseSidebar={() => setSidebarCollapsed(true)}
                onNodeContextMenu={(payload) => nodeContextMenuRef.current?.(payload)}
                viewActions={{
                  newQuery: (...args) => actionsRef.current?.newQuery(...args),
                  openErDiagram: (...args) => actionsRef.current?.openErDiagram(...args),
                  refresh: () => actionsRef.current?.refresh(),
                }}
              />
            </aside>
            <div
              ref={resizeHandleRef}
              className="w-px shrink-0 cursor-col-resize bg-edge hover:bg-accent/30"
              title={t('main.sidebar.resize')}
            />
          </>
        )}

        {/* ── Main content area ── */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {!activeTab && (
            <div className="flex flex-1 flex-col items-center justify-center gap-3">
              <p className="text-sm text-fg-muted">{t('connWin.noConnections')}</p>
            </div>
          )}

          {activeTab?.status === 'error' && (
            <div className="flex flex-1 flex-col items-center justify-center gap-4">
              <div className="text-sm text-red-400">{activeTab.error}</div>
              <div className="flex gap-2">
                <button
                  className="rounded-md bg-blue-500 px-4 py-1.5 text-sm text-white hover:bg-blue-600"
                  type="button"
                  onClick={() => {
                    setTabs((prev) =>
                      prev.map((tab, i) =>
                        i === activeIdx
                          ? { ...tab, status: 'connecting', connectionId: '', error: undefined }
                          : tab,
                      ),
                    );
                  }}
                >
                  {t('common.retry')}
                </button>
                <button
                  className="rounded-md bg-surface-raised px-4 py-1.5 text-sm text-fg-secondary hover:text-fg"
                  type="button"
                  onClick={() => void handleCloseTab(activeTab.configId)}
                >
                  {t('common.close')}
                </button>
              </div>
            </div>
          )}

          {activeTab?.status === 'connecting' && (
            <div className="flex flex-1 flex-col items-center justify-center gap-4">
              <div className="h-6 w-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
              <div className="text-sm text-fg-muted">{t('conn.connecting')}</div>
            </div>
          )}

          {connectedTabs.map((tab) => {
            const isTabActive = tab.configId === activeTab?.configId;
            const viewMode = DB_REGISTRY[tab.databaseType]?.connectionView ?? 'sql';
            const View = getConnectionView(viewMode);
            return (
              <div
                key={tab.configId}
                className="min-h-0 min-w-0 flex-1 flex-col"
                style={{ display: isTabActive ? 'flex' : 'none' }}
              >
                <View
                  connectionId={tab.connectionId}
                  configId={tab.configId}
                  connectionName={tab.connectionName}
                  databaseType={tab.databaseType}
                  initialDatabase={tab.initialDatabase}
                  hideSidebar
                  isActive={isTabActive}
                  selectTableRef={
                    selectTableRef as MutableRefObject<
                      ((table: string, schema?: string) => void) | undefined
                    >
                  }
                  nodeContextMenuRef={
                    nodeContextMenuRef as MutableRefObject<
                      | ((payload: {
                          kind: string;
                          name: string;
                          x: number;
                          y: number;
                          schema?: string;
                        }) => void)
                      | undefined
                    >
                  }
                  actionsRef={
                    actionsRef as MutableRefObject<
                      | {
                          newQuery: (initialSql?: string) => void;
                          openErDiagram: (focusTable?: string) => void;
                          refresh: () => void;
                        }
                      | undefined
                    >
                  }
                />
              </div>
            );
          })}
        </div>
      </div>

      {confirmDeleteDialog}
      <ConnectionShareDialog
        open={connShareOpen}
        mode={connShareMode}
        importSource="file"
        onClose={() => setConnShareOpen(false)}
        onExportSuccess={() => setConnShareOpen(false)}
        onImportSuccess={() => {
          setConnShareOpen(false);
          handleRefresh();
        }}
        onError={() => setConnShareOpen(false)}
      />
    </div>
  );
}
