import { useCallback, useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { DbTypeBadge } from '../../components/DbTypeBadge';
import { useI18n } from '../../hooks/useI18n';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { useQueryStore } from '../../stores/queryStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { connectionCommands } from '../../commands/connection';
import { emitCrossWindow, listenCrossWindow } from '../../lib/crossWindowBus';
import { DB_REGISTRY, getDbLabel } from '../../lib/databaseTypes';
import { getConnectionView } from '../../lib/connectionViews';
import { PENDING_CONNECTION_KEY } from '../../lib/windowManager';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { cn } from '../../lib/cn';
import type { DatabaseType, TableInfo } from '../../types';
import type { QueryTab } from '../../stores/queryStore';

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

// Snapshot of per-connection store state for save/restore on tab switch.
interface StoreSnapshot {
  queryTabs: QueryTab[];
  queryActiveTabId: string;
  schemaDatabases: string[];
  schemaCurrentDatabase: string | null;
  schemaTables: TableInfo[];
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

// ── Component ─────────────────────────────────────────────────────

export function ConnectionWindow() {
  useSettings();

  const { t } = useI18n();
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const loadAiConfig = useAiStore((s) => s.loadConfig);
  const setupAiListeners = useAiStore((s) => s.setupEventListeners);

  const [tabs, setTabs] = useState<ConnectionTab[]>(() => {
    const pending = consumePendingConnection();
    return pending ? [pending] : [];
  });
  const [activeIdx, setActiveIdx] = useState(0);
  const storeCache = useRef(new Map<string, StoreSnapshot>());

  const activeTab = tabs[activeIdx] ?? null;

  // ── Settings / AI — fire-and-forget, once ──

  useEffect(() => {
    void loadSettings();
    void loadAiConfig();
    const aiCleanupPromise = setupAiListeners();
    return () => {
      void aiCleanupPromise.then((fn) => fn());
    };
  }, [loadSettings, loadAiConfig, setupAiListeners]);

  // ── Connect the initial tab (if not already connected) ──

  const connectTab = useCallback(async (tab: ConnectionTab): Promise<string> => {
    const existing = useActiveConnectionStore.getState().connections[tab.configId];
    if (existing?.status === 'connected' && existing.connectionId) {
      return existing.connectionId;
    }
    return connectionCommands.connect(tab.configId);
  }, []);

  useEffect(() => {
    if (tabs.length === 0) return;
    const pending = tabs.filter((t) => t.status === 'connecting' && !t.connectionId);
    for (const tab of pending) {
      void (async () => {
        try {
          const connId = await connectTab(tab);
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

      // Clear localStorage to prevent duplicate pickup on remount
      try {
        localStorage.removeItem(PENDING_CONNECTION_KEY);
      } catch {}

      const newTab = makeTabFromPayload(data);
      if (!newTab) return;

      setTabs((prev) => {
        const existingIdx = prev.findIndex((t) => t.configId === newTab.configId);
        if (existingIdx >= 0) {
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
      setTabs((prev) => {
        const filtered = prev.filter((t) => t.connectionId !== data.connectionId);
        return filtered;
      });
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

  // ── Tab switching with store snapshot save/restore ──

  const prevActiveIdx = useRef(activeIdx);

  const saveStoreSnapshot = useCallback((configId: string) => {
    const qState = useQueryStore.getState();
    const sState = useSchemaStore.getState();
    storeCache.current.set(configId, {
      queryTabs: qState.tabs,
      queryActiveTabId: qState.activeTabId,
      schemaDatabases: sState.databases,
      schemaCurrentDatabase: sState.currentDatabase,
      schemaTables: sState.tables,
    });
  }, []);

  const restoreStoreSnapshot = useCallback((configId: string) => {
    const cached = storeCache.current.get(configId);
    if (cached) {
      useQueryStore.setState({
        tabs: cached.queryTabs,
        activeTabId: cached.queryActiveTabId,
      });
    }
  }, []);

  useEffect(() => {
    if (prevActiveIdx.current !== activeIdx) {
      const prevTab = tabs[prevActiveIdx.current];
      if (prevTab?.configId) {
        saveStoreSnapshot(prevTab.configId);
      }
      prevActiveIdx.current = activeIdx;
    }
  }, [activeIdx, tabs, saveStoreSnapshot]);

  // Restore after the active view has mounted and set its connectionId.
  // Schedule on next tick to let the view's own effects run first.
  useEffect(() => {
    if (!activeTab?.connectionId) return;
    const timer = setTimeout(() => restoreStoreSnapshot(activeTab.configId), 50);
    return () => clearTimeout(timer);
  }, [activeTab?.configId, activeTab?.connectionId, restoreStoreSnapshot]);

  // ── Close a connection tab ──

  const handleCloseTab = useCallback(
    async (idx: number) => {
      const tab = tabs[idx];
      if (!tab) return;

      // Save snapshot cleanup
      storeCache.current.delete(tab.configId);

      if (tab.connectionId) {
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
        if (next.length === 0 && '__TAURI_INTERNALS__' in globalThis) {
          void import('@tauri-apps/api/window').then(({ getCurrentWindow }) =>
            getCurrentWindow().close(),
          );
        }
        return next;
      });

      setActiveIdx((prev) => {
        if (idx < prev) return prev - 1;
        if (idx === prev) return Math.min(prev, tabs.length - 2);
        return prev;
      });

      // Reset stores when the last tab is closed
      if (tabs.length <= 1) {
        useSchemaStore.getState().reset();
        useQueryStore.getState().reset();
        useTableDataStore.getState().reset();
      }
    },
    [tabs],
  );

  const switchTab = useCallback(
    (idx: number) => {
      if (idx === activeIdx) return;
      setActiveIdx(idx);
    },
    [activeIdx],
  );

  // ── Render ──

  if (tabs.length === 0) {
    return (
      <div className="flex h-screen items-center justify-center bg-surface text-fg">
        <TitleBar title="DataZen" />
        <div className="text-sm text-fg-muted">{t('connWin.noConnections')}</div>
      </div>
    );
  }

  const connectedTab =
    activeTab?.status === 'connected' && activeTab.connectionId ? activeTab : null;
  const viewMode = connectedTab
    ? (DB_REGISTRY[connectedTab.databaseType]?.connectionView ?? 'sql')
    : 'sql';
  const ViewComponent = getConnectionView(viewMode);
  const centerTitle = activeTab
    ? `${activeTab.connectionName} - ${getDbLabel(activeTab.databaseType)} - DataZen`
    : 'DataZen';

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar
        title={centerTitle}
        leftContent={
          activeTab && (
            <div className="flex items-center gap-2">
              <span
                className={cn(
                  'inline-flex h-2 w-2 rounded-full',
                  activeTab.status === 'connected'
                    ? 'bg-green-500'
                    : activeTab.status === 'error'
                      ? 'bg-red-500'
                      : 'animate-pulse bg-yellow-400',
                )}
              />
              <span className="text-xs text-fg-secondary">{activeTab.connectionName}</span>
            </div>
          )
        }
      />

      <div className="flex min-h-0 flex-1">
        {/* ── Left connection sidebar (TablePlus-style) ── */}
        {tabs.length > 1 && (
          <aside className="flex w-44 shrink-0 flex-col border-r border-edge bg-surface-alt">
            <div className="flex-1 overflow-y-auto py-1">
              {tabs.map((tab, idx) => {
                const isActive = idx === activeIdx;
                return (
                  <div
                    key={tab.configId}
                    className={cn(
                      'group relative flex items-center gap-2 px-2.5 py-2 text-xs transition-colors cursor-pointer',
                      isActive
                        ? 'bg-accent/12 text-fg'
                        : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                    )}
                    onClick={() => switchTab(idx)}
                  >
                    <DbTypeBadge databaseType={tab.databaseType} size={20} className="shrink-0" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-medium">
                        {tab.connectionName || tab.configId}
                      </div>
                      <div className="truncate text-[10px] text-fg-muted">
                        {getDbLabel(tab.databaseType)}
                      </div>
                    </div>
                    <span
                      className={cn(
                        'h-1.5 w-1.5 shrink-0 rounded-full',
                        tab.status === 'connected'
                          ? 'bg-green-500'
                          : tab.status === 'error'
                            ? 'bg-red-500'
                            : 'animate-pulse bg-yellow-400',
                      )}
                    />
                    <button
                      type="button"
                      className="shrink-0 rounded p-0.5 text-fg-muted opacity-0 hover:bg-surface hover:text-fg group-hover:opacity-100"
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleCloseTab(idx);
                      }}
                    >
                      <X className="h-3 w-3" />
                    </button>
                    {isActive && <span className="absolute inset-y-0 left-0 w-0.5 bg-accent" />}
                  </div>
                );
              })}
            </div>
          </aside>
        )}

        {/* ── Main content area ── */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
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
                  onClick={() => void handleCloseTab(activeIdx)}
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

          {connectedTab && (
            <ViewComponent
              key={connectedTab.configId}
              connectionId={connectedTab.connectionId}
              configId={connectedTab.configId}
              connectionName={connectedTab.connectionName}
              databaseType={connectedTab.databaseType}
              initialDatabase={connectedTab.initialDatabase}
            />
          )}
        </div>
      </div>
    </div>
  );
}
