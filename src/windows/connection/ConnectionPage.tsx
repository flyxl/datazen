import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Database,
  LayoutDashboard,
  LayoutGrid,
  PanelLeftOpen,
  Puzzle,
  Settings,
  Workflow,
  type LucideIcon,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { MenuBar } from '../../components/MenuBar';
import { ThemeToggle } from '../../components/ThemeToggle';
import { ThemedIcon } from '../../components/ThemedIcon';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import { useSettings } from '../../hooks/useSettings';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useSchemaStore } from '../../stores/schemaStore';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { connectionCommands } from '../../commands/connection';
import { backupCommands } from '../../commands/backup';
import { settingsCommands } from '../../commands/settings';
import { emitCrossWindow, listenCrossWindow } from '../../lib/crossWindowBus';
import { getDbLabel } from '../../lib/databaseTypes';
import { openConnectionShareDialog } from '../../lib/connectionShare';
import {
  openBackupWindow,
  openDataSyncWindow,
  openDataTransferWindow,
  openNewConnectionDialog,
  openSchemaDiffWindow,
  PENDING_CONNECTION_KEY,
} from '../../lib/windowManager';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { usePanelStore, nextPanelId, type RedisDbPanel } from '../../stores/panelStore';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import type { ConnectionViewActions } from '../../lib/connectionViews/types';
import {
  ConnectionNavigatorTree,
  type ConnectionNavigatorTreeHandle,
} from './ConnectionNavigatorTree';
import { ContentView } from './ContentView';
import type { UiIconId } from '../../lib/iconIds';
import type { DatabaseType } from '../../types';
import { useDashboardStore } from '../../stores/dashboardStore';
import { DashboardPanel } from '../dashboard/DashboardPanel';
import { WorkflowPage } from '../workflow/WorkflowPage';
import { SettingsPage } from '../settings/SettingsPage';
import { WorkspaceView } from '../workspace/WorkspaceView';
import { PluginManagementPage } from '../plugins/PluginManagementPage';

interface WorkspaceShortcutButtonProps {
  icon: LucideIcon;
  iconId: UiIconId;
  label: string;
  testId: string;
  onClick: () => void;
  active?: boolean;
}

function WorkspaceModeButton({
  icon: Icon,
  iconId,
  label,
  testId,
  onClick,
  active = false,
}: Readonly<WorkspaceShortcutButtonProps>) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      title={label}
      className={`flex h-10 w-full items-center justify-center text-xs transition-colors ${
        active
          ? 'bg-accent/20 text-accent'
          : 'text-fg-secondary hover:bg-surface-raised hover:text-fg'
      }`}
    >
      <ThemedIcon id={iconId} className="h-4 w-4 shrink-0" fallback={Icon} />
    </button>
  );
}

type WorkspaceMode = 'connections' | 'workflow' | 'dashboard' | 'workspace' | 'plugins';
type MainView = 'workspace' | 'settings';

// ── Connection Tab ────────────────────────────────────────────────

interface ConnectionTab {
  /** Persistent connection id (saved connection this tab was opened from). */
  connectionId: string;
  /** Live database session id ('' while connecting). */
  dbSessionId: string;
  connectionName: string;
  databaseType: DatabaseType;
  initialDatabase?: string;
  status: 'connecting' | 'connected' | 'error';
  error?: string;
}

function makeTabFromPayload(data: Record<string, string>): ConnectionTab | null {
  const connectionId = data.connectionId ?? '';
  if (!connectionId) return null;
  const dbSessionId = data.dbSessionId ?? '';
  return {
    connectionId,
    dbSessionId,
    connectionName: data.connectionName ?? '',
    databaseType: (data.databaseType ?? 'postgresql') as DatabaseType,
    initialDatabase: data.database,
    status: dbSessionId ? 'connected' : 'connecting',
  };
}

function consumePendingConnection(): { tab: ConnectionTab; action?: string } | null {
  try {
    const raw = localStorage.getItem(PENDING_CONNECTION_KEY);
    localStorage.removeItem(PENDING_CONNECTION_KEY);
    if (!raw) return null;
    const data = JSON.parse(raw) as Record<string, string>;
    const tab = makeTabFromPayload(data);
    if (!tab) return null;
    return { tab, action: data.action };
  } catch {
    return null;
  }
}

// ── Sync all keyed stores to a specific connection ────────────────

function syncStoresActiveConnection(dbSessionId: string | null) {
  useSchemaStore.getState().setActiveConnection(dbSessionId);
  useTableDataStore.getState().setActiveConnection(dbSessionId);
}

function removeConnectionFromStores(dbSessionId: string) {
  useSchemaStore.getState().removeConnection(dbSessionId);
  useTableDataStore.getState().removeConnection(dbSessionId);
}

// ── Component ─────────────────────────────────────────────────────

export function ConnectionPage() {
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
  const [messageDialogOpen, setMessageDialogOpen] = useState(false);
  const [messageDialogText, setMessageDialogText] = useState('');
  const [messageDialogKind, setMessageDialogKind] = useState<'error' | 'success'>('error');
  const fetchDashboards = useDashboardStore((s) => s.fetchDashboards);
  const [workspaceMode, setWorkspaceMode] = useState<WorkspaceMode>('connections');
  const [mainView, setMainView] = useState<MainView>('workspace');
  const [settingsSection, setSettingsSection] = useState<string | undefined>(undefined);
  /** Preserved for F3 sidebar Settings entry — restore workspace mode on back. */
  const settingsReturnModeRef = useRef<WorkspaceMode>('connections');
  const [embeddedDashboardId, setEmbeddedDashboardId] = useState<string | undefined>(undefined);
  const [dashboardTitle, setDashboardTitle] = useState('');

  const initialPendingRef = useRef(consumePendingConnection());
  const [tabs, setTabs] = useState<ConnectionTab[]>(() => {
    return initialPendingRef.current ? [initialPendingRef.current.tab] : [];
  });
  const [activeIdx, setActiveIdx] = useState(0);
  const [sidebarWidth, setSidebarWidth] = useState(280);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const pendingActionRef = useRef<string | null>(null);
  const isResizingRef = useRef(false);
  const resizeHandleRef = useRef<HTMLDivElement>(null);
  const selectTableRef = useRef<
    ((table: string, schema?: string, database?: string) => void) | undefined
  >();
  const nodeContextMenuRef = useRef<
    | ((payload: { kind: string; name: string; x: number; y: number; schema?: string }) => void)
    | undefined
  >();
  const actionsRef = useRef<ConnectionViewActions | undefined>();
  const navigatorRef = useRef<ConnectionNavigatorTreeHandle>(null);

  const activeTab = tabs[activeIdx] ?? null;

  const showMessageDialog = useCallback((text: string, kind: 'error' | 'success') => {
    setMessageDialogText(text);
    setMessageDialogKind(kind);
    setMessageDialogOpen(true);
  }, []);

  const executePendingAction = useCallback(() => {
    const action = pendingActionRef.current;
    if (!action) return;
    pendingActionRef.current = null;
    setWorkspaceMode('connections');
    if (action === 'openSqlFile') {
      actionsRef.current?.openSqlFile?.();
    }
  }, []);

  useEffect(() => {
    if (initialPendingRef.current?.action) {
      pendingActionRef.current = initialPendingRef.current.action;
      initialPendingRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!activeTab?.dbSessionId || activeTab.status !== 'connected') return;
    executePendingAction();
  }, [activeTab?.dbSessionId, activeTab?.status, executePendingAction]);

  const allPanels = usePanelStore((s) => s.panels);
  const activePanelId = usePanelStore((s) => s.activePanelId);
  const activePanel = allPanels.find((p) => p.id === activePanelId) ?? null;

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

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenCrossWindow('datazen:refresh-connection', (payload) => {
      const data = payload as { dbSessionId?: string } | undefined;
      if (!data?.dbSessionId) return;
      const tab = tabs.find((t) => t.dbSessionId === data.dbSessionId);
      if (!tab) return;
      void navigatorRef.current?.refreshConnection(tab.connectionId);
      actionsRef.current?.refresh?.();
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [tabs]);

  // ── Derive active connection from active panel or active tab ──

  const activeDbSessionId = activePanel?.dbSessionId || activeTab?.dbSessionId || null;

  useEffect(() => {
    syncStoresActiveConnection(activeDbSessionId);
  }, [activeDbSessionId]);

  useEffect(() => {
    if (!activePanel) return;
    const tabIdx = tabs.findIndex((t) => t.connectionId === activePanel.connectionId);
    if (tabIdx >= 0 && tabIdx !== activeIdx) {
      setActiveIdx(tabIdx);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activePanelId]);

  // ── Connect the initial tab (if not already connected) ──

  const connectTab = useCallback(async (tab: ConnectionTab): Promise<string> => {
    const store = useActiveConnectionStore.getState();
    const existing = store.connections[tab.connectionId];
    if (existing?.status === 'connected' && existing.dbSessionId) {
      return existing.dbSessionId;
    }
    store.markConnecting(tab.connectionId, tab.initialDatabase ?? null);
    return connectionCommands.connect(tab.connectionId);
  }, []);

  useEffect(() => {
    if (tabs.length === 0) return;
    const pending = tabs.filter((t) => t.status === 'connecting' && !t.dbSessionId);
    for (const tab of pending) {
      void (async () => {
        try {
          const sessionId = await connectTab(tab);
          useActiveConnectionStore.getState().markConnected(tab.connectionId, sessionId);
          setTabs((prev) =>
            prev.map((t) =>
              t.connectionId === tab.connectionId
                ? { ...t, dbSessionId: sessionId, status: 'connected', error: undefined }
                : t,
            ),
          );
          void emitCrossWindow('datazen:connection-ready', {
            connectionId: tab.connectionId,
            dbSessionId: sessionId,
          });
        } catch (e) {
          const msg =
            typeof e === 'string' ? e : e instanceof Error ? e.message : t('backend.unknownError');
          useActiveConnectionStore.getState().markError(tab.connectionId, msg);
          setTabs((prev) =>
            prev.map((tb) =>
              tb.connectionId === tab.connectionId ? { ...tb, status: 'error', error: msg } : tb,
            ),
          );
        }
      })();
    }
  }, [tabs, connectTab, t]);

  // ── Listen for new connection requests from MainPage ──

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void listenCrossWindow('datazen:open-connection', (payload) => {
      const data = payload as Record<string, string> | undefined;
      if (!data?.connectionId) return;
      setWorkspaceMode('connections');

      try {
        localStorage.removeItem(PENDING_CONNECTION_KEY);
      } catch {}

      const newTab = makeTabFromPayload(data);
      if (!newTab) return;

      if (data.action) pendingActionRef.current = data.action;

      if (newTab.dbSessionId) syncStoresActiveConnection(newTab.dbSessionId);

      setTabs((prev) => {
        const existingIdx = prev.findIndex((t) => t.connectionId === newTab.connectionId);
        if (existingIdx >= 0) {
          if (prev[existingIdx].dbSessionId) {
            syncStoresActiveConnection(prev[existingIdx].dbSessionId);
          }
          setActiveIdx(existingIdx);
          if (prev[existingIdx].status === 'connected' && data.action) {
            setTimeout(() => executePendingAction(), 500);
          }
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
      const data = payload as { connectionId?: string; dbSessionId?: string } | undefined;
      if (!data?.connectionId || !data?.dbSessionId) return;
      setTabs((prev) =>
        prev.map((tab) =>
          tab.connectionId === data.connectionId
            ? { ...tab, dbSessionId: data.dbSessionId!, status: 'connected', error: undefined }
            : tab,
        ),
      );
    }).then((fn) => cleanups.push(fn));

    void listenCrossWindow('datazen:connection-failed', (payload) => {
      const data = payload as { connectionId?: string; error?: string } | undefined;
      if (!data?.connectionId) return;
      setTabs((prev) =>
        prev.map((tab) =>
          tab.connectionId === data.connectionId
            ? { ...tab, status: 'error', error: data.error ?? 'Unknown error' }
            : tab,
        ),
      );
    }).then((fn) => cleanups.push(fn));

    void listenCrossWindow('datazen:disconnect-requested', (payload) => {
      const data = payload as { dbSessionId?: string } | undefined;
      if (!data?.dbSessionId) return;
      setTabs((prev) => prev.filter((t) => t.dbSessionId !== data.dbSessionId));
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  }, []);

  // ── Heartbeat for all connected tabs ──

  useEffect(() => {
    const HEARTBEAT_MS = 5 * 60 * 1000;
    const timer = setInterval(() => {
      for (const tab of tabs) {
        if (tab.dbSessionId && tab.status === 'connected') {
          connectionCommands.pingConnection(tab.dbSessionId).catch(() => {});
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
          if (tab.dbSessionId) {
            try {
              const wasDisconnected = await connectionCommands.releaseConnection(tab.dbSessionId);
              if (wasDisconnected) {
                await emitCrossWindow('datazen:connection-closed', {
                  dbSessionId: tab.dbSessionId,
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

  // ── Close all tabs for a connection (and cleanup store data) ──

  const handleCloseTab = useCallback(
    async (connectionId: string) => {
      const tab = tabs.find((t) => t.connectionId === connectionId);
      if (!tab) return;

      usePanelStore.getState().removeAllForConnection(connectionId);

      if (tab.dbSessionId) {
        removeConnectionFromStores(tab.dbSessionId);
        useActiveConnectionStore.getState().removeByDbSessionId(tab.dbSessionId);
        try {
          const wasDisconnected = await connectionCommands.releaseConnection(tab.dbSessionId);
          if (wasDisconnected) {
            void emitCrossWindow('datazen:connection-closed', { dbSessionId: tab.dbSessionId });
          }
        } catch {
          // best effort
        }
      }

      const idx = tabs.findIndex((t) => t.connectionId === connectionId);
      setTabs((prev) => prev.filter((t) => t.connectionId !== connectionId));
      setActiveIdx((prev) => {
        if (idx < 0) return prev;
        if (prev < idx) return prev;
        if (prev === idx) return Math.min(prev, tabs.length - 2);
        return prev - 1;
      });
    },
    [tabs],
  );

  // ── Tree callbacks ──

  const handleSelectConnection = useCallback(
    (connectionId: string) => {
      setWorkspaceMode('connections');
      const existingIdx = tabs.findIndex((t) => t.connectionId === connectionId);
      if (existingIdx >= 0) {
        const existingTab = tabs[existingIdx];
        if (existingTab.dbSessionId) {
          syncStoresActiveConnection(existingTab.dbSessionId);
        }
        setActiveIdx(existingIdx);
        return;
      }
      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) return;

      const entry = useActiveConnectionStore.getState().connections[connectionId];
      const sessionId = entry?.dbSessionId ?? '';
      const newTab: ConnectionTab = {
        connectionId,
        dbSessionId: sessionId,
        connectionName: conn.name,
        databaseType: conn.databaseType,
        initialDatabase: conn.database,
        status: sessionId ? 'connected' : 'connecting',
      };
      if (sessionId) syncStoresActiveConnection(sessionId);
      setTabs((prev) => {
        const next = [...prev, newTab];
        setActiveIdx(next.length - 1);
        return next;
      });
    },
    [tabs, connections],
  );

  const handleSelectKvDb = useCallback(
    (connectionId: string, dbName: string) => {
      setWorkspaceMode('connections');
      handleSelectConnection(connectionId);

      const panels = usePanelStore.getState().panels;
      const existing = panels.find(
        (p) =>
          p.type === 'redis-db' &&
          p.connectionId === connectionId &&
          (p as RedisDbPanel).dbName === dbName,
      );
      if (existing) {
        usePanelStore.getState().setActivePanel(existing.id);
        return;
      }

      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) return;
      const entry = useActiveConnectionStore.getState().connections[connectionId];
      const sessionId = entry?.dbSessionId ?? '';

      const panel: RedisDbPanel = {
        id: nextPanelId('redis'),
        connectionId,
        dbSessionId: sessionId,
        connectionName: conn.name,
        databaseType: conn.databaseType,
        type: 'redis-db',
        dbName,
      };
      usePanelStore.getState().addPanel(panel);
    },
    [handleSelectConnection, connections],
  );

  const handleDeleteConnection = useCallback(
    (connectionId: string) => {
      const conn = connections.find((c) => c.id === connectionId);
      if (!conn) return;
      void confirmDelete({
        title: t('common.deleteConnection'),
        message: t('main.ctx.confirmDeleteConnection', { name: conn.name }),
        kind: 'warning',
      }).then((ok) => {
        if (!ok) return;
        void handleCloseTab(connectionId);
        void deleteConnection(connectionId);
      });
    },
    [connections, confirmDelete, handleCloseTab, deleteConnection, t],
  );

  const handleDisconnect = useCallback(
    (connectionId: string) => {
      void handleCloseTab(connectionId);
    },
    [handleCloseTab],
  );

  const handleSelectTable = useCallback((tableName: string, schema?: string, database?: string) => {
    // Defer so that any preceding handleSelectConnection state flush + useLayoutEffect
    // has time to update selectTableRef to the correct connection's handler.
    requestAnimationFrame(() => {
      selectTableRef.current?.(tableName, schema, database);
    });
  }, []);

  const handleRefresh = useCallback(() => {
    void fetchConnections();
    void fetchGroups();
    void navigatorRef.current?.refreshAllConnections();
  }, [fetchConnections, fetchGroups]);

  const handleExportConfig = useCallback(async () => {
    let saved: boolean;
    try {
      const date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
      saved = await backupCommands.exportAppData(`datazen-backup-${date}.zip`);
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('common.exportFailed'), 'error');
      return;
    }
    if (!saved) return;
    showMessageDialog(t('appData.exportSuccess'), 'success');

    try {
      const { ask } = await import('@tauri-apps/plugin-dialog');
      const wantKey = await ask(t('appData.backupKeyMessage'), {
        title: t('appData.backupKeyTitle'),
        kind: 'info',
      });
      if (wantKey) {
        const keySaved = await backupCommands.saveEncryptionKeyWithDialog('datazen.key');
        if (keySaved) {
          showMessageDialog(t('appData.backupKeySaved'), 'success');
        }
      }
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('appData.backupKeyFailed'), 'error');
    }
  }, [showMessageDialog, t]);

  const handleImportConfig = useCallback(async () => {
    try {
      const imported = await backupCommands.importAppData(
        t('common.importAppData'),
        t('appData.importConfirmMessage'),
      );
      if (!imported) return;
      await backupCommands.restartApp();
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('common.importFailed'), 'error');
    }
  }, [showMessageDialog, t]);

  const handleOpenDashboard = useCallback(async () => {
    await fetchDashboards();
    const list = useDashboardStore.getState().list;
    if (list.length > 0) {
      setEmbeddedDashboardId(list[0]!.id);
      setDashboardTitle(list[0]!.name);
      setWorkspaceMode('dashboard');
      return;
    }
    setEmbeddedDashboardId(undefined);
    setDashboardTitle(t('dashboard.title'));
    setWorkspaceMode('dashboard');
  }, [fetchDashboards, t]);

  const handleOpenDashboardById = useCallback(
    (dashboardId?: string, dashboardName?: string) => {
      setEmbeddedDashboardId(dashboardId);
      setDashboardTitle(dashboardName ?? t('dashboard.title'));
      setWorkspaceMode('dashboard');
    },
    [t],
  );

  const handleOpenWorkflow = useCallback(() => {
    setWorkspaceMode('workflow');
  }, []);

  const openSettingsInShell = useCallback(
    (section?: string) => {
      settingsReturnModeRef.current = workspaceMode;
      setSettingsSection(section);
      setMainView('settings');
    },
    [workspaceMode],
  );

  const handleSettingsBack = useCallback(() => {
    setMainView('workspace');
    setSettingsSection(undefined);
    setWorkspaceMode(settingsReturnModeRef.current);
  }, []);

  useEffect(() => {
    const cleanups: Array<() => void> = [];

    void listenCrossWindow('menu:open-settings', (payload) => {
      const data = payload as { section?: string } | undefined;
      openSettingsInShell(data?.section);
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:data-sync', () => {
      openDataSyncWindow();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:data-transfer', () => {
      openDataTransferWindow();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:schema-diff', () => {
      openSchemaDiffWindow();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:workflow', () => {
      handleOpenWorkflow();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:dashboard', () => {
      void handleOpenDashboard();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:open-dashboard', (payload) => {
      const data = payload as { dashboardId?: string; dashboardName?: string } | undefined;
      handleOpenDashboardById(data?.dashboardId, data?.dashboardName);
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:backup', () => {
      openBackupWindow('backup');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:restore', () => {
      openBackupWindow('restore');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:view-logs', () => {
      void settingsCommands.openLogDir();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:export-config', () => {
      void handleExportConfig();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-config', () => {
      void handleImportConfig();
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  }, [
    handleExportConfig,
    handleImportConfig,
    handleOpenDashboard,
    handleOpenWorkflow,
    openSettingsInShell,
  ]);

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

  const centerTitle = (() => {
    if (workspaceMode === 'workflow') return t('win.workflow');
    if (workspaceMode === 'dashboard') {
      return dashboardTitle ? `${dashboardTitle} - DataZen` : t('win.dashboard');
    }
    if (workspaceMode === 'workspace') return t('nav.workspacePages');
    if (workspaceMode === 'plugins') return t('nav.plugins');
    if (activePanel) {
      return `${activePanel.connectionName} - ${getDbLabel(activePanel.databaseType)} - DataZen`;
    }
    if (activeTab) {
      return `${activeTab.connectionName} - ${getDbLabel(activeTab.databaseType)} - DataZen`;
    }
    return t('win.connections');
  })();

  const connectionWorkspace = (
    <div className="flex h-full min-h-0 flex-1">
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
            className="flex h-full min-h-0 shrink-0 flex-col border-r border-edge bg-surface-alt"
          >
            <div className="flex min-h-0 flex-1 flex-col">
              <ConnectionNavigatorTree
                ref={navigatorRef}
                activeConnectionId={activeTab?.connectionId ?? null}
                onSelectConnection={handleSelectConnection}
                onSelectTable={handleSelectTable}
                onSelectKvDb={handleSelectKvDb}
                onNewConnection={() => openNewConnectionDialog()}
                onRefresh={handleRefresh}
                onExportConnections={() => openConnectionShareDialog('export')}
                onImportConnections={() => openConnectionShareDialog('import')}
                onEditConnection={(id) => openNewConnectionDialog(id)}
                onDeleteConnection={handleDeleteConnection}
                onDisconnect={handleDisconnect}
                onCollapseSidebar={() => setSidebarCollapsed(true)}
                onNodeContextMenu={(payload) => nodeContextMenuRef.current?.(payload)}
                onShowMessage={showMessageDialog}
                viewActions={{
                  newQuery: (...args) => actionsRef.current?.newQuery(...args),
                  openSqlFile: () => actionsRef.current?.openSqlFile?.(),
                  createTable: () => actionsRef.current?.createTable?.(),
                  openCreateDatabase: () => actionsRef.current?.openCreateDatabase?.(),
                  openCreateSchema: () => actionsRef.current?.openCreateSchema?.(),
                  openCreateUser: () => actionsRef.current?.openCreateUser?.(),
                  openErDiagram: (...args) => actionsRef.current?.openErDiagram(...args),
                  refresh: () => actionsRef.current?.refresh(),
                  openObject: (...args) => actionsRef.current?.openObject?.(...args),
                  openQueryHistory: () => actionsRef.current?.openQueryHistory?.(),
                  openServerStatus: (...args) => actionsRef.current?.openServerStatus?.(...args),
                  openProcessList: (...args) => actionsRef.current?.openProcessList?.(...args),
                }}
              />
            </div>
          </aside>
          <div
            ref={resizeHandleRef}
            className="w-px shrink-0 cursor-col-resize bg-edge hover:bg-accent/30"
            title={t('main.sidebar.resize')}
          />
        </>
      )}

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {activeTab?.status === 'error' && !activePanel && (
          <div className="flex flex-1 flex-col items-center justify-center gap-4">
            <div className="copyable text-sm text-red-400">{activeTab.error}</div>
            <div className="flex gap-2">
              <button
                className="rounded-md bg-blue-500 px-4 py-1.5 text-sm text-white hover:bg-blue-600"
                type="button"
                onClick={() => {
                  setTabs((prev) =>
                    prev.map((tab, i) =>
                      i === activeIdx
                        ? { ...tab, status: 'connecting', dbSessionId: '', error: undefined }
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
                onClick={() => void handleCloseTab(activeTab.connectionId)}
              >
                {t('common.close')}
              </button>
            </div>
          </div>
        )}

        {activeTab?.status === 'connecting' && !activePanel && (
          <div className="flex flex-1 flex-col items-center justify-center gap-4">
            <div className="h-6 w-6 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
            <div className="text-sm text-fg-muted">{t('conn.connecting')}</div>
          </div>
        )}

        <ContentView
          selectTableRef={selectTableRef}
          nodeContextMenuRef={nodeContextMenuRef}
          actionsRef={actionsRef}
        />
      </div>
    </div>
  );

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface text-fg">
      {mainView === 'settings' ? (
        <SettingsPage initialSection={settingsSection} onBack={handleSettingsBack} />
      ) : (
        <>
          <TitleBar title={centerTitle} leftContent={<MenuBar />} rightContent={<ThemeToggle />} />

          <div className="flex min-h-0 flex-1">
            <aside className="flex h-full w-10 shrink-0 flex-col self-stretch border-r border-edge bg-surface-alt">
              <div className="flex flex-col">
                <WorkspaceModeButton
                  icon={Database}
                  iconId="nav.connections"
                  label={t('nav.connections')}
                  testId="workspace-nav-connections"
                  active={workspaceMode === 'connections'}
                  onClick={() => setWorkspaceMode('connections')}
                />
                <WorkspaceModeButton
                  icon={Workflow}
                  iconId="action.workflow"
                  label={t('nav.workflow')}
                  testId="workspace-nav-workflow"
                  active={workspaceMode === 'workflow'}
                  onClick={handleOpenWorkflow}
                />
                <WorkspaceModeButton
                  icon={LayoutDashboard}
                  iconId="action.dashboard"
                  label={t('nav.dashboard')}
                  testId="workspace-nav-dashboard"
                  active={workspaceMode === 'dashboard'}
                  onClick={() => void handleOpenDashboard()}
                />
                <WorkspaceModeButton
                  icon={LayoutGrid}
                  iconId="nav.workspacePages"
                  label={t('nav.workspacePages')}
                  testId="workspace-nav-workspace-pages"
                  active={workspaceMode === 'workspace'}
                  onClick={() => setWorkspaceMode('workspace')}
                />
                <WorkspaceModeButton
                  icon={Puzzle}
                  iconId="nav.plugins"
                  label={t('nav.plugins')}
                  testId="workspace-nav-plugins"
                  active={workspaceMode === 'plugins'}
                  onClick={() => setWorkspaceMode('plugins')}
                />
              </div>
              <div className="mt-auto">
                <WorkspaceModeButton
                  icon={Settings}
                  iconId="nav.settings"
                  label={t('nav.settings')}
                  testId="workspace-nav-settings"
                  onClick={() => openSettingsInShell()}
                />
              </div>
            </aside>

            <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
              {workspaceMode === 'connections' ? (
                connectionWorkspace
              ) : workspaceMode === 'workflow' ? (
                <WorkflowPage embedded onOpenDashboardInShell={handleOpenDashboardById} />
              ) : workspaceMode === 'workspace' ? (
                <WorkspaceView onOpenPlugins={() => setWorkspaceMode('plugins')} />
              ) : workspaceMode === 'plugins' ? (
                <PluginManagementPage onOpenInWorkspace={() => setWorkspaceMode('workspace')} />
              ) : (
                <DashboardPanel
                  initialDashboardId={embeddedDashboardId}
                  onDashboardChange={(_id, name) => setDashboardTitle(name)}
                  onOpenWorkflowEditor={handleOpenWorkflow}
                />
              )}
            </div>
          </div>
        </>
      )}

      {confirmDeleteDialog}
      <Dialog
        open={messageDialogOpen}
        title={messageDialogKind === 'error' ? t('common.error') : t('common.success')}
        onClose={() => setMessageDialogOpen(false)}
        footer={null}
      >
        <div
          className={`copyable whitespace-pre-wrap break-words text-sm ${
            messageDialogKind === 'error' ? 'text-red-400' : 'text-green-400'
          }`}
        >
          {messageDialogText}
        </div>
      </Dialog>
    </div>
  );
}
