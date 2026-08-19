import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@testing-library/react';
import { MainWindow } from '../MainWindow';
import { WebContextMenuHost } from '../../../components/ui/WebContextMenu';
import { useContextMenuStore } from '../../../stores/contextMenuStore';
import type { ConnectionConfig } from '../../../types';

// ─── Hoisted mocks ───────────────────────────────────────────────────

const {
  fetchConnectionsMock,
  fetchGroupsMock,
  loadSettingsMock,
  setSearchQueryMock,
  addGroupMock,
  renameGroupMock,
  deleteGroupMock,
  duplicateConnectionMock,
  deleteConnectionMock,
  moveConnectionToGroupMock,
  setMainSidebarWidthMock,
  connectActionMock,
  markConnectingMock,
  disconnectMock,
  removeByConnectionIdMock,
  markConnectedMock,
  markErrorMock,
  fetchDashboardsMock,
  saveDashboardMock,
  deleteDashboardMock,
  listenCrossWindowMock,
  crossWindowHandlers,
  openBackupWindowMock,
  openConnectionWindowMock,
  openDashboardWindowMock,
  openDataSyncWindowMock,
  openNewConnectionWindowMock,
  openSettingsWindowMock,
  openWorkflowWindowMock,
  exportAppDataMock,
  saveEncryptionKeyMock,
  importAppDataMock,
  restartAppMock,
  openLogDirMock,
  invokeMock,
  askMock,
  confirmDialogFn,
  connectionState,
  activeState,
  dashboardState,
  uiState,
  capturedShortcuts,
  stableT,
} = vi.hoisted(() => {
  const connectionState = {
    connections: [] as ConnectionConfig[],
    groups: [] as string[],
    searchQuery: '',
    loading: false,
    error: null as string | null,
    fetchConnections: vi.fn().mockResolvedValue(undefined),
    fetchGroups: vi.fn().mockResolvedValue(undefined),
    setSearchQuery: vi.fn(),
    addGroup: vi.fn().mockResolvedValue(undefined),
    renameGroup: vi.fn().mockResolvedValue(undefined),
    deleteGroup: vi.fn().mockResolvedValue(undefined),
    duplicateConnection: vi.fn().mockResolvedValue(undefined),
    deleteConnection: vi.fn().mockResolvedValue(undefined),
    moveConnectionToGroup: vi.fn().mockResolvedValue(undefined),
  };

  const activeState = {
    connections: {} as Record<string, { status: string; connectionId?: string; error?: string }>,
    connect: vi.fn().mockResolvedValue(undefined),
    markConnecting: vi.fn(),
    disconnect: vi.fn().mockResolvedValue(undefined),
    removeByConnectionId: vi.fn(),
    markConnected: vi.fn(),
    markError: vi.fn(),
  };

  const dashboardState = {
    list: [] as { id: string; name: string }[],
    listLoading: false,
    listError: null as string | null,
    fetchDashboards: vi.fn().mockResolvedValue(undefined),
    saveDashboard: vi.fn().mockResolvedValue(undefined),
    deleteDashboard: vi.fn().mockResolvedValue(undefined),
  };

  const uiState = {
    mainSidebarWidth: 240,
    setMainSidebarWidth: vi.fn(),
  };

  const capturedShortcuts: { key: string; action: () => void }[] = [];
  const crossWindowHandlers = new Map<string, (payload?: unknown) => void>();
  const stableT = (key: string, params?: Record<string, unknown>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;

  return {
    fetchConnectionsMock: connectionState.fetchConnections,
    fetchGroupsMock: connectionState.fetchGroups,
    loadSettingsMock: vi.fn().mockResolvedValue(undefined),
    setSearchQueryMock: connectionState.setSearchQuery,
    addGroupMock: connectionState.addGroup,
    renameGroupMock: connectionState.renameGroup,
    deleteGroupMock: connectionState.deleteGroup,
    duplicateConnectionMock: connectionState.duplicateConnection,
    deleteConnectionMock: connectionState.deleteConnection,
    moveConnectionToGroupMock: connectionState.moveConnectionToGroup,
    setMainSidebarWidthMock: uiState.setMainSidebarWidth,
    connectActionMock: activeState.connect,
    markConnectingMock: activeState.markConnecting,
    disconnectMock: activeState.disconnect,
    removeByConnectionIdMock: activeState.removeByConnectionId,
    markConnectedMock: activeState.markConnected,
    markErrorMock: activeState.markError,
    fetchDashboardsMock: dashboardState.fetchDashboards,
    saveDashboardMock: dashboardState.saveDashboard,
    deleteDashboardMock: dashboardState.deleteDashboard,
    crossWindowHandlers,
    listenCrossWindowMock: vi
      .fn()
      .mockImplementation((event: string, handler: (p?: unknown) => void) => {
        crossWindowHandlers.set(event, handler);
        // No-op unsubscribe: avoid async teardown racing with re-register (mirrors production timing bug in tests only).
        return Promise.resolve(() => {});
      }),
    openBackupWindowMock: vi.fn(),
    openConnectionWindowMock: vi.fn(),
    openDashboardWindowMock: vi.fn(),
    openDataSyncWindowMock: vi.fn(),
    openNewConnectionWindowMock: vi.fn(),
    openSettingsWindowMock: vi.fn(),
    openWorkflowWindowMock: vi.fn(),
    exportAppDataMock: vi.fn().mockResolvedValue(true),
    saveEncryptionKeyMock: vi.fn().mockResolvedValue(true),
    importAppDataMock: vi.fn().mockResolvedValue(true),
    restartAppMock: vi.fn().mockResolvedValue(undefined),
    openLogDirMock: vi.fn().mockResolvedValue(undefined),
    invokeMock: vi.fn().mockResolvedValue(true),
    askMock: vi.fn().mockResolvedValue(false),
    confirmDialogFn: vi.fn().mockResolvedValue(true),
    connectionState,
    activeState,
    dashboardState,
    uiState,
    capturedShortcuts,
    stableT,
  };
});

async function emitCrossWindow(event: string, payload?: unknown) {
  await waitFor(() => expect(crossWindowHandlers.has(event)).toBe(true));
  crossWindowHandlers.get(event)!(payload);
}

function getListArea() {
  return document.querySelector('.flex-1.overflow-auto') as HTMLElement;
}

function mainTree() {
  return (
    <>
      <MainWindow />
      <WebContextMenuHost />
    </>
  );
}

function renderMain() {
  return render(mainTree());
}

async function openBlankContextMenu() {
  fireEvent.contextMenu(getListArea(), { clientX: 40, clientY: 40 });
  await screen.findByTestId('web-context-menu');
}

async function openGroupContextMenu(groupLabel: string) {
  const header = screen.getByText(groupLabel).closest('[data-group-header]') as HTMLElement;
  fireEvent.contextMenu(header, { clientX: 40, clientY: 40 });
  await screen.findByTestId('web-context-menu');
}

async function openConnContextMenu(connId: string) {
  fireEvent.contextMenu(screen.getByTestId(`conn-${connId}`), { clientX: 40, clientY: 40 });
  await screen.findByTestId('web-context-menu');
}

async function clickMenuItem(id: string) {
  fireEvent.click(await screen.findByTestId(`web-context-item-${id}`));
}

async function hoverSubmenu(id: string) {
  fireEvent.mouseEnter(await screen.findByTestId(`web-context-submenu-trigger-${id}`));
  await screen.findByTestId('web-context-submenu');
}

// ─── Module mocks ────────────────────────────────────────────────────

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT }),
}));

vi.mock('../../../hooks/useTauriEvent', () => ({ useTauriEvent: () => {} }));
vi.mock('../../../hooks/useSettings', () => ({ useSettings: () => {} }));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 256, handleRef: { current: null } }),
}));

vi.mock('../../../hooks/useKeyboardShortcuts', () => ({
  useKeyboardShortcuts: (shortcuts: { key: string; action: () => void }[]) => {
    capturedShortcuts.length = 0;
    capturedShortcuts.push(...shortcuts);
  },
}));

vi.mock('../../../stores/connectionStore', async () => {
  const actual = await vi.importActual<typeof import('../../../stores/connectionStore')>(
    '../../../stores/connectionStore',
  );
  return {
    ...actual,
    useConnectionStore: (sel: (s: typeof connectionState) => unknown) => sel(connectionState),
  };
});

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => Promise<void> }) => unknown) =>
    sel({ loadSettings: loadSettingsMock }),
}));

vi.mock('../../../stores/uiStore', () => ({
  useUiStore: (sel: (s: typeof uiState) => unknown) => sel(uiState),
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (sel: (s: typeof activeState) => unknown) => sel(activeState),
    { getState: () => activeState },
  ),
}));

vi.mock('../../../stores/dashboardStore', () => ({
  useDashboardStore: Object.assign(
    (sel: (s: typeof dashboardState) => unknown) => sel(dashboardState),
    { getState: () => dashboardState },
  ),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  listenCrossWindow: (...args: unknown[]) => listenCrossWindowMock(...args),
}));

vi.mock('../../../lib/windowManager', () => ({
  openBackupWindow: (...a: unknown[]) => openBackupWindowMock(...a),
  openConnectionWindow: (...a: unknown[]) => openConnectionWindowMock(...a),
  openDashboardWindow: (...a: unknown[]) => openDashboardWindowMock(...a),
  openDataSyncWindow: (...a: unknown[]) => openDataSyncWindowMock(...a),
  openNewConnectionWindow: (...a: unknown[]) => openNewConnectionWindowMock(...a),
  openSettingsWindow: (...a: unknown[]) => openSettingsWindowMock(...a),
  openWorkflowWindow: (...a: unknown[]) => openWorkflowWindowMock(...a),
}));

vi.mock('../../../commands/backup', () => ({
  backupCommands: {
    exportAppDataWithDialog: (...a: unknown[]) => exportAppDataMock(...a),
    saveEncryptionKeyWithDialog: (...a: unknown[]) => saveEncryptionKeyMock(...a),
    importAppDataWithDialog: (...a: unknown[]) => importAppDataMock(...a),
    restartApp: (...a: unknown[]) => restartAppMock(...a),
  },
}));

vi.mock('../../../commands/settings', () => ({
  settingsCommands: {
    openLogDir: (...a: unknown[]) => openLogDirMock(...a),
  },
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: (...a: unknown[]) => askMock(...a),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmDialogFn, null],
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../../../components/MenuBar', () => ({
  MenuBar: () => <div data-testid="menu-bar" />,
}));

vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle" />,
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: ({ left, right }: { left: React.ReactNode; right: React.ReactNode }) => (
    <div data-testid="status-bar">
      <span data-testid="status-left">{left}</span>
      <span data-testid="status-right">{right}</span>
    </div>
  ),
}));

vi.mock('../ActionPanel', () => ({
  ActionPanel: ({
    onNewConnection,
    onBackup,
    onRestore,
    onWorkflow,
    onDashboard,
  }: {
    onNewConnection: () => void;
    onBackup: () => void;
    onRestore: () => void;
    onWorkflow: () => void;
    onDashboard: () => void;
  }) => (
    <div data-testid="action-panel">
      <button type="button" data-testid="action-new" onClick={onNewConnection}>
        new
      </button>
      <button type="button" data-testid="action-backup" onClick={onBackup}>
        backup
      </button>
      <button type="button" data-testid="action-restore" onClick={onRestore}>
        restore
      </button>
      <button type="button" data-testid="action-workflow" onClick={onWorkflow}>
        workflow
      </button>
      <button type="button" data-testid="action-dashboard" onClick={onDashboard}>
        dashboard
      </button>
    </div>
  ),
}));

vi.mock('../ConnectionItem', () => ({
  ConnectionItem: ({
    connection,
    selected,
    isDragging,
    onSelect,
    onConnect,
    onContextMenu,
    onPointerDown,
  }: {
    connection: ConnectionConfig;
    selected: boolean;
    isDragging?: boolean;
    onSelect: (id: string) => void;
    onConnect: (cfg: ConnectionConfig) => void;
    onContextMenu: (e: React.MouseEvent, cfg: ConnectionConfig) => void;
    onPointerDown: (e: React.PointerEvent, cfg: ConnectionConfig) => void;
  }) => (
    <div
      data-testid={`conn-${connection.id}`}
      data-conn-item
      data-selected={selected ? 'true' : 'false'}
      data-dragging={isDragging ? 'true' : 'false'}
      onClick={() => onSelect(connection.id)}
      onDoubleClick={() => onConnect(connection)}
      onContextMenu={(e) => {
        void onContextMenu(e, connection);
      }}
      onPointerDown={(e) => onPointerDown(e, connection)}
    >
      {connection.name}
    </div>
  ),
}));

vi.mock('../../../components/connection/ConnectionShareDialog', () => ({
  ConnectionShareDialog: ({
    open,
    mode,
    importSource,
    onClose,
    onExportSuccess,
    onImportSuccess,
    onError,
  }: {
    open: boolean;
    mode: string;
    importSource?: string;
    onClose: () => void;
    onExportSuccess: (count: number) => void;
    onImportSuccess: (result: {
      imported: number;
      overwritten: number;
      groupsAdded: number;
      skipped?: string[];
      sourceFormat?: string;
    }) => void | Promise<void>;
    onError: (message: string) => void;
  }) =>
    open ? (
      <div data-testid="conn-share-dialog" data-mode={mode} data-source={importSource ?? 'file'}>
        <button type="button" data-testid="share-close" onClick={onClose}>
          close
        </button>
        <button type="button" data-testid="share-export-ok" onClick={() => onExportSuccess(2)}>
          export
        </button>
        <button
          type="button"
          data-testid="share-import-ok"
          onClick={() =>
            void onImportSuccess({
              imported: 1,
              overwritten: 0,
              groupsAdded: 1,
              sourceFormat: 'json',
            })
          }
        >
          import
        </button>
        <button
          type="button"
          data-testid="share-import-skipped"
          onClick={() =>
            void onImportSuccess({ imported: 1, overwritten: 0, groupsAdded: 0, skipped: ['x'] })
          }
        >
          import-skipped
        </button>
        <button type="button" data-testid="share-error" onClick={() => onError('share-fail')}>
          error
        </button>
      </div>
    ) : null,
}));

// ─── Fixtures ────────────────────────────────────────────────────────

function makeConn(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: 'c1',
    name: 'Local PG',
    databaseType: 'postgresql',
    host: 'localhost',
    port: 5432,
    database: 'postgres',
    sslMode: 'disable',
    group: 'Dev',
    ...overrides,
  };
}

function resetState() {
  connectionState.connections = [];
  connectionState.groups = [];
  connectionState.searchQuery = '';
  connectionState.loading = false;
  connectionState.error = null;
  activeState.connections = {};
  dashboardState.list = [];
  dashboardState.listLoading = false;
  dashboardState.listError = null;
  crossWindowHandlers.clear();
  vi.clearAllMocks();
  exportAppDataMock.mockResolvedValue(true);
  saveEncryptionKeyMock.mockResolvedValue(true);
  importAppDataMock.mockResolvedValue(true);
  restartAppMock.mockResolvedValue(undefined);
  invokeMock.mockResolvedValue(true);
  askMock.mockResolvedValue(false);
  confirmDialogFn.mockResolvedValue(true);
}

beforeEach(() => {
  resetState();
  Object.defineProperty(globalThis, '__TAURI_INTERNALS__', { value: {}, configurable: true });
});

afterEach(() => {
  useContextMenuStore.getState().hide();
  cleanup();
  Reflect.deleteProperty(globalThis, '__TAURI_INTERNALS__');
});

// ─── Tests ───────────────────────────────────────────────────────────

describe('MainWindow', () => {
  it('TC-main: init fetches connections, groups, and settings', async () => {
    renderMain();
    await waitFor(() => {
      expect(fetchConnectionsMock).toHaveBeenCalled();
      expect(fetchGroupsMock).toHaveBeenCalled();
      expect(loadSettingsMock).toHaveBeenCalled();
    });
    expect(screen.getByTestId('title-bar')).toHaveTextContent('DataZen');
    expect(setMainSidebarWidthMock).toHaveBeenCalledWith(256);
  });

  it('TC-main: status bar shows loading, error, active, and ready states', () => {
    connectionState.loading = true;
    const { rerender } = renderMain();
    expect(screen.getByTestId('status-left')).toHaveTextContent('common.loading');

    connectionState.loading = false;
    connectionState.error = 'load failed';
    rerender(mainTree());
    expect(screen.getByTestId('status-left')).toHaveTextContent('load failed');

    connectionState.error = null;
    activeState.connections = {
      c1: { status: 'connected', connectionId: 'live-1' },
    };
    rerender(mainTree());
    expect(screen.getByTestId('status-left')).toHaveTextContent('main.activeConnections');

    activeState.connections = {};
    rerender(mainTree());
    expect(screen.getByTestId('status-left')).toHaveTextContent('main.ready');
  });

  it('TC-main: empty state and new-connection actions', () => {
    renderMain();
    expect(screen.getByText('main.noConnections')).toBeInTheDocument();
    fireEvent.click(screen.getByText('main.createFirst'));
    expect(openNewConnectionWindowMock).toHaveBeenCalled();

    fireEvent.click(screen.getByTitle('main.newConnection'));
    expect(openNewConnectionWindowMock).toHaveBeenCalledTimes(2);
  });

  it('TC-main: renders grouped connections and toggles expand', () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [
      makeConn(),
      makeConn({ id: 'c2', name: 'Redis', databaseType: 'redis', group: 'Dev' }),
    ];

    renderMain();
    expect(screen.getByText('Dev')).toBeInTheDocument();
    expect(screen.getByTestId('conn-c1')).toBeInTheDocument();
    expect(screen.getByTestId('conn-c2')).toBeInTheDocument();

    fireEvent.click(screen.getByText('Dev'));
    expect(screen.queryByTestId('conn-c1')).not.toBeInTheDocument();

    fireEvent.click(screen.getByText('Dev'));
    expect(screen.getByTestId('conn-c1')).toBeInTheDocument();
  });

  it('TC-main: search updates query via store', () => {
    renderMain();
    const input = screen.getByPlaceholderText('main.searchPlaceholder');
    fireEvent.change(input, { target: { value: 'pg' } });
    expect(setSearchQueryMock).toHaveBeenCalledWith('pg');
  });

  it('TC-main: action panel opens windows', () => {
    renderMain();
    fireEvent.click(screen.getByTestId('action-new'));
    fireEvent.click(screen.getByTestId('action-backup'));
    fireEvent.click(screen.getByTestId('action-workflow'));
    expect(openNewConnectionWindowMock).toHaveBeenCalled();
    expect(openBackupWindowMock).toHaveBeenCalled();
    expect(openWorkflowWindowMock).toHaveBeenCalled();
  });

  it('TC-main: handleConnect opens window for idle and connected sessions', () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [makeConn()];

    renderMain();
    fireEvent.doubleClick(screen.getByTestId('conn-c1'));
    expect(connectActionMock).toHaveBeenCalled();
    expect(openConnectionWindowMock).toHaveBeenCalledWith(
      { configId: 'c1' },
      'Local PG',
      'postgres',
      'postgresql',
      undefined,
    );

    activeState.connections = { c1: { status: 'connected', connectionId: 'live-1' } };
    fireEvent.doubleClick(screen.getByTestId('conn-c1'));
    expect(openConnectionWindowMock).toHaveBeenLastCalledWith(
      { configId: 'c1', connectionId: 'live-1' },
      'Local PG',
      'postgres',
      'postgresql',
      undefined,
    );
  });

  it('TC-main: cross-window connection lifecycle events update active store', async () => {
    renderMain();
    await waitFor(() => expect(listenCrossWindowMock).toHaveBeenCalled());

    await emitCrossWindow('datazen:connection-closed', { connectionId: 'live-1' });
    expect(removeByConnectionIdMock).toHaveBeenCalledWith('live-1');

    await emitCrossWindow('datazen:connection-ready', { configId: 'c1', connectionId: 'live-1' });
    expect(markConnectedMock).toHaveBeenCalledWith('c1', 'live-1');

    await emitCrossWindow('datazen:connection-failed', { configId: 'c1', error: 'auth fail' });
    expect(markErrorMock).toHaveBeenCalledWith('c1', 'auth fail');

    await emitCrossWindow('datazen:connection-failed', { configId: 'c2' });
    expect(markErrorMock).toHaveBeenCalledWith('c2', 'backend.unknownError');
  });

  it('TC-main: menu cross-window events open windows and log dir', async () => {
    renderMain();
    await waitFor(() => expect(listenCrossWindowMock).toHaveBeenCalled());

    await emitCrossWindow('menu:open-settings');
    await emitCrossWindow('menu:new-connection');
    await emitCrossWindow('menu:data-sync');
    await emitCrossWindow('menu:backup');
    await emitCrossWindow('menu:view-logs');

    expect(openSettingsWindowMock).toHaveBeenCalled();
    expect(openNewConnectionWindowMock).toHaveBeenCalled();
    expect(openDataSyncWindowMock).toHaveBeenCalled();
    expect(openBackupWindowMock).toHaveBeenCalled();
    expect(openLogDirMock).toHaveBeenCalled();
  });

  it('TC-main: keyboard shortcut opens new connection', () => {
    renderMain();
    const shortcut = capturedShortcuts.find((s) => s.key === 'mod+n');
    expect(shortcut).toBeDefined();
    shortcut!.action();
    expect(openNewConnectionWindowMock).toHaveBeenCalled();
  });

  it('TC-main: new group dialog adds group', async () => {
    renderMain();
    await openBlankContextMenu();

    await clickMenuItem('new-group');
    await waitFor(() => expect(screen.getByText('main.newGroupTitle')).toBeInTheDocument());

    const nameInput = screen.getByPlaceholderText('main.groupNamePlaceholder');
    fireEvent.change(nameInput, { target: { value: 'Staging' } });
    fireEvent.click(screen.getByText('common.ok'));
    expect(addGroupMock).toHaveBeenCalledWith('Staging');
  });

  it('TC-main: group context menu rename and delete', async () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [makeConn()];

    renderMain();
    await openGroupContextMenu('Dev');

    await clickMenuItem('rename-group');
    await waitFor(() => expect(screen.getByDisplayValue('Dev')).toBeInTheDocument());
    const renameInput = screen.getByDisplayValue('Dev');
    fireEvent.change(renameInput, { target: { value: 'Production' } });
    fireEvent.keyDown(renameInput, { key: 'Enter' });
    expect(renameGroupMock).toHaveBeenCalledWith('Dev', 'Production');

    await openGroupContextMenu('Dev');
    await clickMenuItem('delete-group');
    expect(deleteGroupMock).toHaveBeenCalledWith('Dev');
  });

  it('TC-main: connection context menu open, edit, duplicate, delete', async () => {
    connectionState.groups = ['Dev', 'Prod'];
    connectionState.connections = [makeConn()];

    renderMain();
    await openConnContextMenu('c1');

    await clickMenuItem('open-connection');
    expect(openConnectionWindowMock).toHaveBeenCalled();

    await openConnContextMenu('c1');
    await clickMenuItem('edit-connection');
    expect(openNewConnectionWindowMock).toHaveBeenCalledWith('c1');

    await openConnContextMenu('c1');
    await clickMenuItem('duplicate-connection');
    expect(duplicateConnectionMock).toHaveBeenCalledWith('c1');

    confirmDialogFn.mockResolvedValueOnce(true);
    await openConnContextMenu('c1');
    await clickMenuItem('delete-connection');
    await waitFor(() => expect(deleteConnectionMock).toHaveBeenCalledWith('c1'));
  });

  it('TC-main: connection context menu disconnect when connected', async () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [makeConn()];
    activeState.connections = { c1: { status: 'connected', connectionId: 'live-1' } };

    renderMain();
    await openConnContextMenu('c1');
    await clickMenuItem('disconnect');
    expect(disconnectMock).toHaveBeenCalledWith('c1');
  });

  it('TC-main: connection context menu move to group', async () => {
    connectionState.groups = ['Dev', 'Prod'];
    connectionState.connections = [makeConn()];

    renderMain();
    await openConnContextMenu('c1');
    await hoverSubmenu('move-to-group');
    await clickMenuItem('move-group-Prod');
    expect(moveConnectionToGroupMock).toHaveBeenCalledWith('c1', 'Prod');

    await openConnContextMenu('c1');
    await hoverSubmenu('move-to-group');
    await clickMenuItem('remove-from-group');
    expect(moveConnectionToGroupMock).toHaveBeenCalledWith('c1', undefined);
  });

  it('TC-main: drag-and-drop moves connection to another group', async () => {
    connectionState.groups = ['Dev', 'Prod'];
    connectionState.connections = [makeConn()];

    renderMain();

    const groupEl = screen.getByText('Prod').closest('[data-group-name]') as HTMLElement;
    groupEl.getBoundingClientRect = () =>
      ({ left: 0, right: 200, top: 0, bottom: 100, width: 200, height: 100 }) as DOMRect;

    const conn = screen.getByTestId('conn-c1');
    fireEvent.pointerDown(conn, { button: 0, clientX: 10, clientY: 10 });

    fireEvent(window, new PointerEvent('pointermove', { clientX: 50, clientY: 50, bubbles: true }));
    fireEvent(window, new PointerEvent('pointerup', { clientX: 50, clientY: 50, bubbles: true }));

    await waitFor(() => {
      expect(moveConnectionToGroupMock).toHaveBeenCalledWith('c1', 'Prod');
    });
  });

  it('TC-main: export config success and optional key save', async () => {
    renderMain();
    askMock.mockResolvedValueOnce(true);
    await emitCrossWindow('menu:export-config');

    await waitFor(() => expect(screen.getByText('appData.exportSuccess')).toBeInTheDocument());
    expect(exportAppDataMock).toHaveBeenCalled();
    await waitFor(() => expect(saveEncryptionKeyMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('appData.backupKeySaved')).toBeInTheDocument());
  });

  it('TC-main: export config handles errors', async () => {
    exportAppDataMock.mockRejectedValueOnce(new Error('export boom'));
    renderMain();
    await emitCrossWindow('menu:export-config');
    await waitFor(() => expect(screen.getByText('export boom')).toBeInTheDocument());
  });

  it('TC-main: import config restarts app', async () => {
    renderMain();
    await emitCrossWindow('menu:import-config');
    await waitFor(() => expect(importAppDataMock).toHaveBeenCalled());
    await waitFor(() => expect(restartAppMock).toHaveBeenCalled());
  });

  it('TC-main: import config shows error dialog', async () => {
    importAppDataMock.mockRejectedValueOnce(new Error('import fail'));
    renderMain();
    await emitCrossWindow('menu:import-config');
    await waitFor(() => expect(screen.getByText('import fail')).toBeInTheDocument());
  });

  it('TC-main: connection share export/import dialogs', async () => {
    renderMain();
    await emitCrossWindow('menu:export-connections');
    await waitFor(() =>
      expect(screen.getByTestId('conn-share-dialog')).toHaveAttribute('data-mode', 'export'),
    );

    fireEvent.click(screen.getByTestId('share-export-ok'));
    await waitFor(() => expect(screen.getByText(/connShare.exportSuccess/)).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('share-close'));

    await emitCrossWindow('menu:import-connections');
    await waitFor(() =>
      expect(screen.getByTestId('conn-share-dialog')).toHaveAttribute('data-mode', 'import'),
    );
    fireEvent.click(screen.getByTestId('share-import-ok'));
    await waitFor(() => expect(fetchConnectionsMock).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByText(/connShare.importSuccess/)).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('share-import-skipped'));
    await waitFor(() =>
      expect(screen.getByText(/connShare.importSuccessWithSkipped/)).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId('share-error'));
    await waitFor(() => expect(screen.getByText('share-fail')).toBeInTheDocument());
  });

  it('TC-main: import-from-app menu opens dialog with source', async () => {
    renderMain();
    await emitCrossWindow('menu:import-connections-dbx');
    await waitFor(() => {
      const dialog = screen.getByTestId('conn-share-dialog');
      expect(dialog).toHaveAttribute('data-mode', 'import');
      expect(dialog).toHaveAttribute('data-source', 'dbx');
    });
  });

  it('TC-main: dashboard opens window directly without dialog', async () => {
    dashboardState.list = [{ id: 'd1', name: 'Ops' }];
    renderMain();

    fireEvent.click(screen.getByTestId('action-dashboard'));
    await waitFor(() => expect(openDashboardWindowMock).toHaveBeenCalledWith('d1', 'Ops'));
    expect(screen.queryByTestId('dashboard-dialog')).not.toBeInTheDocument();
  });

  it('TC-main: dashboard opens shell when no boards exist', async () => {
    dashboardState.list = [];
    renderMain();
    fireEvent.click(screen.getByTestId('action-dashboard'));
    await waitFor(() => expect(openDashboardWindowMock).toHaveBeenCalledWith());
    expect(screen.queryByTestId('dashboard-dialog')).not.toBeInTheDocument();
  });

  it('TC-main: restore opens backup window in restore mode without requiring a selection', async () => {
    renderMain();
    fireEvent.click(screen.getByTestId('action-restore'));
    expect(openBackupWindowMock).toHaveBeenCalledWith('restore');
    expect(screen.queryByText('backup.selectConnectionFirst')).not.toBeInTheDocument();
    expect(screen.queryByText('main.restoreFailed')).not.toBeInTheDocument();
  });

  it('TC-main: restore menu event opens restore window', async () => {
    renderMain();
    await emitCrossWindow('menu:restore');
    await waitFor(() => expect(openBackupWindowMock).toHaveBeenCalledWith('restore'));
  });

  it('TC-main: ungrouped label and new groups auto-expand', () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [makeConn({ group: undefined })];
    const { rerender } = renderMain();
    expect(screen.getByText('main.ungrouped')).toBeInTheDocument();

    connectionState.groups = ['Dev', 'NewGroup'];
    rerender(mainTree());
    expect(screen.getByText('NewGroup')).toBeInTheDocument();
  });

  it('TC-main: blank context menu on list area', async () => {
    renderMain();
    await openBlankContextMenu();
    await clickMenuItem('new-connection');
    expect(openNewConnectionWindowMock).toHaveBeenCalled();
  });

  it('TC-main: rename escape cancels inline edit', async () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [makeConn()];
    renderMain();

    await openGroupContextMenu('Dev');
    await clickMenuItem('rename-group');
    await waitFor(() => expect(screen.getByDisplayValue('Dev')).toBeInTheDocument());
    const renameInput = screen.getByDisplayValue('Dev');
    fireEvent.keyDown(renameInput, { key: 'Escape' });
    expect(renameGroupMock).not.toHaveBeenCalled();
  });

  it('TC-main: export cancelled when dialog returns false', async () => {
    exportAppDataMock.mockResolvedValueOnce(false);
    renderMain();
    await emitCrossWindow('menu:export-config');
    await waitFor(() => expect(exportAppDataMock).toHaveBeenCalled());
    expect(screen.queryByText('appData.exportSuccess')).not.toBeInTheDocument();
  });

  it('TC-main: import skipped when user cancels', async () => {
    importAppDataMock.mockResolvedValueOnce(false);
    renderMain();
    await emitCrossWindow('menu:import-config');
    await waitFor(() => expect(importAppDataMock).toHaveBeenCalled());
    expect(restartAppMock).not.toHaveBeenCalled();
  });

  it('TC-main: restore from action panel does not invoke restore IPC on the main window', async () => {
    renderMain();
    fireEvent.click(screen.getByTestId('action-restore'));
    expect(invokeMock).not.toHaveBeenCalledWith('restore_database_with_dialog', expect.anything());
  });

  it('TC-main: connection count in status bar', () => {
    connectionState.connections = [makeConn(), makeConn({ id: 'c2', name: 'B' })];
    renderMain();
    expect(screen.getByTestId('status-left')).toHaveTextContent('main.connectionCount');
  });

  it('TC-main: drag ghost renders while dragging', () => {
    connectionState.groups = ['Dev', 'Prod'];
    connectionState.connections = [makeConn()];

    renderMain();
    const groupEl = screen.getByText('Prod').closest('[data-group-name]') as HTMLElement;
    groupEl.getBoundingClientRect = () =>
      ({ left: 0, right: 200, top: 0, bottom: 100, width: 200, height: 100 }) as DOMRect;

    fireEvent.pointerDown(screen.getByTestId('conn-c1'), { button: 0, clientX: 0, clientY: 0 });
    fireEvent(window, new PointerEvent('pointermove', { clientX: 20, clientY: 20, bubbles: true }));
    const ghost = document.querySelector('.pointer-events-none.fixed');
    expect(ghost).toHaveTextContent('Local PG');
  });

  it('TC-main: ungrouped context menu has no rename/delete', async () => {
    connectionState.groups = [];
    connectionState.connections = [makeConn({ group: undefined })];
    renderMain();

    const header = screen.getByText('main.ungrouped').closest('[data-group-header]')!;
    fireEvent.contextMenu(header, { clientX: 40, clientY: 40 });
    await screen.findByTestId('web-context-item-new-group');
    expect(screen.queryByTestId('web-context-item-rename-group')).not.toBeInTheDocument();
    expect(screen.queryByTestId('web-context-item-delete-group')).not.toBeInTheDocument();
  });

  it('TC-main: new group Enter key submits', async () => {
    renderMain();
    await openBlankContextMenu();
    await clickMenuItem('new-group');
    await waitFor(() =>
      expect(screen.getByPlaceholderText('main.groupNamePlaceholder')).toBeInTheDocument(),
    );
    const nameInput = screen.getByPlaceholderText('main.groupNamePlaceholder');
    fireEvent.change(nameInput, { target: { value: 'QA' } });
    fireEvent.keyDown(nameInput, { key: 'Enter' });
    expect(addGroupMock).toHaveBeenCalledWith('QA');
  });

  it('TC-main: export backup key failure shows error', async () => {
    exportAppDataMock.mockResolvedValueOnce(true);
    askMock.mockRejectedValueOnce(new Error('key err'));
    renderMain();
    await emitCrossWindow('menu:export-config');
    await waitFor(() => expect(screen.getByText('key err')).toBeInTheDocument());
  });

  it('TC-main: blank context menu skips conn item target', async () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [makeConn()];
    renderMain();

    await openConnContextMenu('c1');
    expect(screen.getByTestId('web-context-item-edit-connection')).toBeInTheDocument();
    expect(screen.queryByTestId('web-context-item-new-connection')).not.toBeInTheDocument();
  });

  it('TC-main: connecting status skips re-connect', () => {
    connectionState.groups = ['Dev'];
    connectionState.connections = [makeConn()];
    activeState.connections = { c1: { status: 'connecting' } };

    renderMain();
    fireEvent.doubleClick(screen.getByTestId('conn-c1'));
    expect(connectActionMock).not.toHaveBeenCalled();
    expect(openConnectionWindowMock).toHaveBeenCalled();
  });
});
