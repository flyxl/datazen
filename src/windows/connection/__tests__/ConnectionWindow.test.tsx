import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@testing-library/react';
import { ConnectionWindow } from '../ConnectionWindow';

const {
  connectMock,
  releaseConnectionMock,
  pingMock,
  loadSettingsMock,
  loadAiConfigMock,
  setupAiListenersMock,
  emitCrossWindowMock,
  listenCrossWindowMock,
  getActiveConnectionState,
  closeMock,
  fetchConnectionsMock,
  fetchGroupsMock,
  fetchDashboardsMock,
  openWorkflowWindowMock,
  openDashboardWindowMock,
  openBackupWindowMock,
  openDataSyncWindowMock,
  openSchemaDiffWindowMock,
  openSettingsWindowMock,
  openNewConnectionWindowMock,
} = vi.hoisted(() => ({
  connectMock: vi.fn(),
  releaseConnectionMock: vi.fn(),
  pingMock: vi.fn(),
  loadSettingsMock: vi.fn().mockResolvedValue(undefined),
  loadAiConfigMock: vi.fn().mockResolvedValue(undefined),
  setupAiListenersMock: vi.fn().mockResolvedValue(() => {}),
  emitCrossWindowMock: vi.fn().mockResolvedValue(undefined),
  listenCrossWindowMock: vi.fn().mockResolvedValue(() => {}),
  getActiveConnectionState: vi.fn(() => ({
    connections: {} as Record<string, { status: string; connectionId?: string }>,
  })),
  closeMock: vi.fn().mockResolvedValue(undefined),
  fetchConnectionsMock: vi.fn().mockResolvedValue(undefined),
  fetchGroupsMock: vi.fn().mockResolvedValue(undefined),
  fetchDashboardsMock: vi.fn().mockResolvedValue(undefined),
  openWorkflowWindowMock: vi.fn(),
  openDashboardWindowMock: vi.fn(),
  openBackupWindowMock: vi.fn(),
  openDataSyncWindowMock: vi.fn(),
  openSchemaDiffWindowMock: vi.fn(),
  openSettingsWindowMock: vi.fn(),
  openNewConnectionWindowMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => {},
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(false), null],
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (sel: (s: { loadSettings: () => Promise<void> }) => unknown) =>
    sel({ loadSettings: loadSettingsMock, settings: { theme: { mode: 'dark' } } }),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (
    sel: (s: {
      loadConfig: () => Promise<void>;
      setupEventListeners: () => Promise<() => void>;
    }) => unknown,
  ) => sel({ loadConfig: loadAiConfigMock, setupEventListeners: setupAiListenersMock }),
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      connections: [],
      groups: [],
      fetchConnections: fetchConnectionsMock,
      fetchGroups: fetchGroupsMock,
      deleteConnection: vi.fn(),
    }),
  groupConnections: () => [],
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (sel: (s: Record<string, unknown>) => unknown) =>
      sel({ connections: getActiveConnectionState().connections, connect: vi.fn() }),
    {
      getState: () => ({
        ...getActiveConnectionState(),
        markConnecting: vi.fn(),
        markConnected: vi.fn(),
        markError: vi.fn(),
        removeByConnectionId: vi.fn(),
      }),
    },
  ),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: {
    getState: () => ({
      reset: vi.fn(),
      removeConnection: vi.fn(),
      setActiveConnection: vi.fn(),
      databases: [],
      currentDatabase: null,
      tables: [],
    }),
  },
}));

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: {
    getState: () => ({
      reset: vi.fn(),
      setActiveConnection: vi.fn(),
      removeConnection: vi.fn(),
    }),
  },
}));

vi.mock('../../../stores/panelStore', () => {
  const mockState = { panels: [], activePanelId: null };
  const store = (sel: (s: typeof mockState) => unknown) => sel(mockState);
  store.getState = () => ({
    ...mockState,
    addPanel: vi.fn(),
    removePanel: vi.fn(),
    setActivePanel: vi.fn(),
    removeAllForConnection: vi.fn(),
  });
  return {
    usePanelStore: store,
    nextPanelId: (prefix: string) => `panel-${prefix}-test`,
  };
});

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    connect: (...args: unknown[]) => connectMock(...args),
    releaseConnection: (...args: unknown[]) => releaseConnectionMock(...args),
    pingConnection: (...args: unknown[]) => pingMock(...args),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  PENDING_CONNECTION_KEY: 'datazen:pending-connection',
  openNewConnectionWindow: (...args: unknown[]) => openNewConnectionWindowMock(...args),
  openBackupWindow: (...args: unknown[]) => openBackupWindowMock(...args),
  openDataSyncWindow: (...args: unknown[]) => openDataSyncWindowMock(...args),
  openSchemaDiffWindow: (...args: unknown[]) => openSchemaDiffWindowMock(...args),
  openSettingsWindow: (...args: unknown[]) => openSettingsWindowMock(...args),
  openWorkflowWindow: (...args: unknown[]) => openWorkflowWindowMock(...args),
  openDashboardWindow: (...args: unknown[]) => openDashboardWindowMock(...args),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => emitCrossWindowMock(...args),
  listenCrossWindow: (...args: unknown[]) => listenCrossWindowMock(...args),
}));

vi.mock('../ContentView', () => ({
  ContentView: () => <div data-testid="mock-content-view">content-view</div>,
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../../../components/MenuBar', () => ({
  MenuBar: () => <div data-testid="menu-bar">menu</div>,
}));

vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle">theme</div>,
}));

vi.mock('../../../components/ui/Dialog', () => ({
  Dialog: ({ children }: { children?: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock('../ConnectionNavigatorTree', () => ({
  ConnectionNavigatorTree: () => <div data-testid="navigator-tree">tree</div>,
}));

vi.mock('../../dashboard/DashboardPanel', () => ({
  DashboardPanel: ({ onOpenWorkflowEditor }: { onOpenWorkflowEditor?: () => void }) => (
    <div data-testid="dashboard-panel">
      dashboard
      <button type="button" data-testid="dashboard-open-workflow" onClick={onOpenWorkflowEditor}>
        open workflow
      </button>
    </div>
  ),
}));

vi.mock('../../workflow/WorkflowWindow', () => ({
  WorkflowWindow: ({
    onOpenDashboardInShell,
  }: {
    onOpenDashboardInShell?: (dashboardId?: string, dashboardName?: string) => void;
  }) => (
    <div data-testid="workflow-window">
      workflow
      <button
        type="button"
        data-testid="workflow-open-dashboard"
        onClick={() => onOpenDashboardInShell?.('dash-from-workflow', 'Workflow Board')}
      >
        open dashboard
      </button>
    </div>
  ),
}));

vi.mock('../../../stores/dashboardStore', () => {
  const state = {
    list: [] as Array<{ id: string; name: string }>,
    fetchDashboards: fetchDashboardsMock,
  };
  const store = (sel: (s: typeof state) => unknown) => sel(state);
  store.getState = () => state;
  store.setState = (partial: Partial<typeof state>) => Object.assign(state, partial);
  return { useDashboardStore: store };
});

vi.mock('../../../lib/databaseTypes', async () => {
  const actual = await vi.importActual<typeof import('../../../lib/databaseTypes')>(
    '../../../lib/databaseTypes',
  );
  return {
    ...actual,
    getDbLabel: (t: string) => t.toUpperCase(),
  };
});

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: closeMock,
    onCloseRequested: vi.fn().mockResolvedValue(() => {}),
  }),
}));

function setPendingConnection(data: Record<string, string>) {
  localStorage.setItem('datazen:pending-connection', JSON.stringify(data));
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  getActiveConnectionState.mockReturnValue({ connections: {} });
  connectMock.mockResolvedValue('conn-live-1');
  releaseConnectionMock.mockResolvedValue(true);
  pingMock.mockResolvedValue(undefined);
  Object.defineProperty(globalThis, '__TAURI_INTERNALS__', {
    value: {},
    configurable: true,
  });
});

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(globalThis, '__TAURI_INTERNALS__');
});

describe('ConnectionWindow', () => {
  it('TC-window: always renders navigator tree sidebar', () => {
    render(<ConnectionWindow />);
    expect(screen.getByTestId('navigator-tree')).toBeInTheDocument();
  });

  it('TC-window: renders content view even with no active tab', () => {
    render(<ConnectionWindow />);
    expect(screen.getByTestId('mock-content-view')).toBeInTheDocument();
  });

  it('TC-window: fetches connections and groups on mount', () => {
    render(<ConnectionWindow />);
    expect(fetchConnectionsMock).toHaveBeenCalled();
    expect(fetchGroupsMock).toHaveBeenCalled();
  });

  it('TC-window: connects via localStorage pending connection and renders content view', async () => {
    setPendingConnection({
      configId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionWindow />);

    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('cfg-1'));
    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
    expect(emitCrossWindowMock).toHaveBeenCalledWith(
      'datazen:connection-ready',
      expect.objectContaining({ configId: 'cfg-1', connectionId: 'conn-live-1' }),
    );
  });

  it('TC-window: shows connect error UI with retry and close buttons', async () => {
    setPendingConnection({
      configId: 'cfg-bad',
      connectionName: 'Bad',
      databaseType: 'postgresql',
    });
    connectMock.mockRejectedValue(new Error('boom'));

    render(<ConnectionWindow />);

    await waitFor(() => expect(screen.getByText('boom')).toBeInTheDocument());
    expect(screen.getByText('common.retry')).toBeInTheDocument();
    expect(screen.getByText('common.close')).toBeInTheDocument();
  });

  it('TC-window: uses existing connectionId from pending without reconnect', async () => {
    setPendingConnection({
      connectionId: 'already-open',
      configId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionWindow />);

    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
    expect(connectMock).not.toHaveBeenCalled();
  });

  it('TC-window: reuses activeConnectionStore session when present', async () => {
    getActiveConnectionState.mockReturnValue({
      connections: {
        'cfg-reuse': { status: 'connected', connectionId: 'reuse-1' },
      },
    });

    setPendingConnection({
      configId: 'cfg-reuse',
      connectionName: 'Reuse',
      databaseType: 'postgresql',
    });

    render(<ConnectionWindow />);
    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
    expect(connectMock).not.toHaveBeenCalled();
  });

  it('TC-window: renders content view after successful connection', async () => {
    setPendingConnection({
      configId: 'cfg-dash',
      connectionName: 'Dashboard PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionWindow />);

    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
  });

  it('TC-window: shows loading spinner while connect is pending', async () => {
    let resolveConnect: (v: string) => void = () => {};
    connectMock.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveConnect = resolve;
        }),
    );
    setPendingConnection({
      configId: 'cfg-slow',
      connectionName: 'Slow',
      databaseType: 'postgresql',
    });

    render(<ConnectionWindow />);
    await waitFor(() => expect(screen.getByText('conn.connecting')).toBeInTheDocument(), {
      timeout: 2000,
    });
    resolveConnect('conn-slow');
    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
  });

  it('TC-window: switches workspace via icon rail', async () => {
    const { useDashboardStore } = await import('../../../stores/dashboardStore');
    useDashboardStore.setState({
      list: [{ id: 'dash-1', name: 'Ops Board' }],
    });

    render(<ConnectionWindow />);

    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-nav-dashboard'));
    await waitFor(() => expect(fetchDashboardsMock).toHaveBeenCalledOnce());
    expect(screen.getByTestId('dashboard-panel')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-nav-connections'));
    expect(screen.getByTestId('navigator-tree')).toBeInTheDocument();
  });

  it('TC-window: allows embedded workflow and dashboard to switch each other', async () => {
    render(<ConnectionWindow />);

    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    fireEvent.click(screen.getByTestId('workflow-open-dashboard'));
    await waitFor(() => expect(screen.getByTestId('dashboard-panel')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-open-workflow'));
    await waitFor(() => expect(screen.getByTestId('workflow-window')).toBeInTheDocument());
  });
});
