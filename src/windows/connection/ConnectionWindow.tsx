import { useCallback, useEffect, useRef, useState, type ComponentType } from 'react';
import { Database, LayoutDashboard, PanelLeftOpen, Workflow } from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { MenuBar } from '../../components/MenuBar';
import { ThemeToggle } from '../../components/ThemeToggle';
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
import {
  openBackupWindow,
  openDataSyncWindow,
  openNewConnectionWindow,
  openSchemaDiffWindow,
  openSettingsWindow,
  PENDING_CONNECTION_KEY,
} from '../../lib/windowManager';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { usePanelStore, nextPanelId, type RedisDbPanel } from '../../stores/panelStore';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import {
  ConnectionShareDialog,
  type ConnectionImportSource,
  type ConnectionShareMode,
} from '../../components/connection/ConnectionShareDialog';
import type { ConnectionViewActions } from '../../lib/connectionViews/types';
import { ConnectionNavigatorTree } from './ConnectionNavigatorTree';
import { ContentView } from './ContentView';
import type { DatabaseType } from '../../types';
import { useDashboardStore } from '../../stores/dashboardStore';
import { DashboardPanel } from '../dashboard/DashboardPanel';
import { WorkflowWindow } from '../workflow/WorkflowWindow';

interface WorkspaceShortcutButtonProps {
  icon: ComponentType<{ className?: string }>;
  label: string;
  testId: string;
  onClick: () => void;
  active?: boolean;
}

function WorkspaceModeButton({
  icon: Icon,
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
      className={`flex h-10 w-10 items-center justify-center rounded-lg border text-left text-xs transition-colors ${
        active
          ? 'border-accent/40 bg-accent/10 text-accent'
          : 'border-transparent text-fg-secondary hover:border-edge hover:bg-surface-raised hover:text-fg'
      }`}
    >
      <Icon className="h-4 w-4 shrink-0" />
    </button>
  );
}

type WorkspaceMode = 'connections' | 'workflow' | 'dashboard';

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

function syncStoresActiveConnection(connectionId: string | null) {
  useSchemaStore.getState().setActiveConnection(connectionId);
  useTableDataStore.getState().setActiveConnection(connectionId);
}

function removeConnectionFromStores(connectionId: string) {
  useSchemaStore.getState().removeConnection(connectionId);
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
  const [connShareImportSource, setConnShareImportSource] =
    useState<ConnectionImportSource>('file');
  const [messageDialogOpen, setMessageDialogOpen] = useState(false);
  const [messageDialogText, setMessageDialogText] = useState('');
  const [messageDialogKind, setMessageDialogKind] = useState<'error' | 'success'>('error');
  const fetchDashboards = useDashboardStore((s) => s.fetchDashboards);
  const [workspaceMode, setWorkspaceMode] = useState<WorkspaceMode>('connections');
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
  const selectTableRef = useRef<((table: string, schema?: string) => void) | undefined>();
  const nodeContextMenuRef = useRef<
    | ((payload: { kind: string; name: string; x: number; y: number; schema?: string }) => void)
    | undefined
  >();
  const actionsRef = useRef<ConnectionViewActions | undefined>();

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
    if (!activeTab?.connectionId || activeTab.status !== 'connected') return;
    executePendingAction();
  }, [activeTab?.connectionId, activeTab?.status, executePendingAction]);

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
      const data = payload as { connectionId?: string } | undefined;
      if (!data?.connectionId) return;
      const hasMatchingTab = tabs.some((tab) => tab.connectionId === data.connectionId);
      if (!hasMatchingTab) return;
      actionsRef.current?.refresh?.();
    }).then((fn) => {
      cleanup = fn;
    });
    return () => cleanup?.();
  }, [tabs]);

  // ── Derive active connection from active panel or active tab ──

  const activeConnectionId = activePanel?.connectionId || activeTab?.connectionId || null;

  useEffect(() => {
    syncStoresActiveConnection(activeConnectionId);
  }, [activeConnectionId]);

  useEffect(() => {
    if (!activePanel) return;
    const tabIdx = tabs.findIndex((t) => t.configId === activePanel.configId);
    if (tabIdx >= 0 && tabIdx !== activeIdx) {
      setActiveIdx(tabIdx);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activePanelId]);

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
      setWorkspaceMode('connections');

      try {
        localStorage.removeItem(PENDING_CONNECTION_KEY);
      } catch {}

      const newTab = makeTabFromPayload(data);
      if (!newTab) return;

      if (data.action) pendingActionRef.current = data.action;

      if (newTab.connectionId) syncStoresActiveConnection(newTab.connectionId);

      setTabs((prev) => {
        const existingIdx = prev.findIndex((t) => t.configId === newTab.configId);
        if (existingIdx >= 0) {
          if (prev[existingIdx].connectionId) {
            syncStoresActiveConnection(prev[existingIdx].connectionId);
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

  // ── Close all tabs for a connection (and cleanup store data) ──

  const handleCloseTab = useCallback(
    async (configId: string) => {
      const tab = tabs.find((t) => t.configId === configId);
      if (!tab) return;

      usePanelStore.getState().removeAllForConnection(configId);

      if (tab.connectionId) {
        removeConnectionFromStores(tab.connectionId);
        useActiveConnectionStore.getState().removeByConnectionId(tab.connectionId);
        try {
          const wasDisconnected = await connectionCommands.releaseConnection(tab.connectionId);
          if (wasDisconnected) {
            void emitCrossWindow('datazen:connection-closed', { connectionId: tab.connectionId });
          }
        } catch {
          // best effort
        }
      }

      const idx = tabs.findIndex((t) => t.configId === configId);
      setTabs((prev) => prev.filter((t) => t.configId !== configId));
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
    (configId: string) => {
      setWorkspaceMode('connections');
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

  const handleSelectKvDb = useCallback(
    (configId: string, dbName: string) => {
      setWorkspaceMode('connections');
      handleSelectConnection(configId);

      const panels = usePanelStore.getState().panels;
      const existing = panels.find(
        (p) =>
          p.type === 'redis-db' && p.configId === configId && (p as RedisDbPanel).dbName === dbName,
      );
      if (existing) {
        usePanelStore.getState().setActivePanel(existing.id);
        return;
      }

      const conn = connections.find((c) => c.id === configId);
      if (!conn) return;
      const entry = useActiveConnectionStore.getState().connections[configId];
      const connId = entry?.connectionId ?? '';

      const panel: RedisDbPanel = {
        id: nextPanelId('redis'),
        configId,
        connectionId: connId,
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
    // Defer so that any preceding handleSelectConnection state flush + useLayoutEffect
    // has time to update selectTableRef to the correct connection's handler.
    requestAnimationFrame(() => {
      selectTableRef.current?.(tableName, schema);
    });
  }, []);

  const handleRefresh = useCallback(() => {
    void fetchConnections();
    void fetchGroups();
  }, [fetchConnections, fetchGroups]);

  const openConnShare = useCallback(
    (mode: ConnectionShareMode, source: ConnectionImportSource = 'file') => {
      setConnShareMode(mode);
      setConnShareImportSource(source);
      setConnShareOpen(true);
    },
    [],
  );

  const handleExportConfig = useCallback(async () => {
    let saved: boolean;
    try {
      const date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
      saved = await backupCommands.exportAppDataWithDialog(`datazen-backup-${date}.zip`);
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('appData.exportFailed'), 'error');
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
      const imported = await backupCommands.importAppDataWithDialog(
        t('appData.importConfirmTitle'),
        t('appData.importConfirmMessage'),
      );
      if (!imported) return;
      await backupCommands.restartApp();
    } catch (e) {
      showMessageDialog(e instanceof Error ? e.message : t('appData.importFailed'), 'error');
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

  useEffect(() => {
    const cleanups: Array<() => void> = [];

    void listenCrossWindow('menu:open-settings', () => {
      openSettingsWindow();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:new-connection', () => {
      openNewConnectionWindow();
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:data-sync', () => {
      openDataSyncWindow();
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
    void listenCrossWindow('menu:export-connections', () => {
      openConnShare('export');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections', () => {
      openConnShare('import');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-file', () => {
      openConnShare('import', 'file');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-dbx', () => {
      openConnShare('import', 'dbx');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-navicat', () => {
      openConnShare('import', 'navicat');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-datagrip', () => {
      openConnShare('import', 'datagrip');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-dbeaver', () => {
      openConnShare('import', 'dbeaver');
    }).then((fn) => cleanups.push(fn));
    void listenCrossWindow('menu:import-connections-tableplus', () => {
      openConnShare('import', 'tableplus');
    }).then((fn) => cleanups.push(fn));

    return () => cleanups.forEach((fn) => fn());
  }, [
    handleExportConfig,
    handleImportConfig,
    handleOpenDashboard,
    handleOpenWorkflow,
    openConnShare,
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
                activeConfigId={activeTab?.configId ?? null}
                onSelectConnection={handleSelectConnection}
                onSelectTable={handleSelectTable}
                onSelectKvDb={handleSelectKvDb}
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
                  openSqlFile: () => actionsRef.current?.openSqlFile?.(),
                  createTable: () => actionsRef.current?.createTable?.(),
                  openCreateDatabase: () => actionsRef.current?.openCreateDatabase?.(),
                  openCreateSchema: () => actionsRef.current?.openCreateSchema?.(),
                  openCreateUser: () => actionsRef.current?.openCreateUser?.(),
                  openErDiagram: (...args) => actionsRef.current?.openErDiagram(...args),
                  refresh: () => actionsRef.current?.refresh(),
                  openObject: (...args) => actionsRef.current?.openObject?.(...args),
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
      <TitleBar title={centerTitle} leftContent={<MenuBar />} rightContent={<ThemeToggle />} />

      <div className="flex min-h-0 flex-1">
        <aside className="flex h-full w-14 shrink-0 flex-col items-center gap-2 self-stretch border-r border-edge bg-surface-alt px-2 pb-3">
          <WorkspaceModeButton
            icon={Database}
            label={t('nav.connections')}
            testId="workspace-nav-connections"
            active={workspaceMode === 'connections'}
            onClick={() => setWorkspaceMode('connections')}
          />
          <WorkspaceModeButton
            icon={Workflow}
            label={t('nav.workflow')}
            testId="workspace-nav-workflow"
            active={workspaceMode === 'workflow'}
            onClick={handleOpenWorkflow}
          />
          <WorkspaceModeButton
            icon={LayoutDashboard}
            label={t('nav.dashboard')}
            testId="workspace-nav-dashboard"
            active={workspaceMode === 'dashboard'}
            onClick={() => void handleOpenDashboard()}
          />
        </aside>

        <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col">
          {workspaceMode === 'connections' ? (
            connectionWorkspace
          ) : workspaceMode === 'workflow' ? (
            <WorkflowWindow embedded onOpenDashboardInShell={handleOpenDashboardById} />
          ) : (
            <DashboardPanel
              initialDashboardId={embeddedDashboardId}
              onDashboardChange={(_id, name) => setDashboardTitle(name)}
              onOpenWorkflowEditor={handleOpenWorkflow}
            />
          )}
        </div>
      </div>

      {confirmDeleteDialog}
      <ConnectionShareDialog
        open={connShareOpen}
        mode={connShareMode}
        importSource={connShareImportSource}
        onClose={() => setConnShareOpen(false)}
        onExportSuccess={(count) => {
          setConnShareOpen(false);
          showMessageDialog(t('connShare.exportSuccess', { count }), 'success');
        }}
        onImportSuccess={(result) => {
          setConnShareOpen(false);
          showMessageDialog(
            t('connShare.importSuccess', {
              imported: result.imported,
              skipped: result.skipped?.length ?? 0,
            }),
            'success',
          );
          handleRefresh();
        }}
        onError={(message) => {
          setConnShareOpen(false);
          showMessageDialog(message, 'error');
        }}
      />
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
