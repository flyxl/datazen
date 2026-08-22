import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@testing-library/react';
import { ConnectionPage } from '../ConnectionPage';

const {
  connectMock,
  releaseConnectionMock,
  pingMock,
  fetchConnectionsMock,
  fetchGroupsMock,
  listenCrossWindowMock,
  menuOpenSettingsHandler,
} = vi.hoisted(() => ({
  connectMock: vi.fn(),
  releaseConnectionMock: vi.fn(),
  pingMock: vi.fn(),
  fetchConnectionsMock: vi.fn().mockResolvedValue(undefined),
  fetchGroupsMock: vi.fn().mockResolvedValue(undefined),
  listenCrossWindowMock: vi.fn((event: string, handler: (payload?: unknown) => void) => {
    if (event === 'menu:open-settings') {
      menuOpenSettingsHandler.current = handler;
    }
    return Promise.resolve(() => {});
  }),
  menuOpenSettingsHandler: { current: null as ((payload?: unknown) => void) | null },
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
  useSettingsStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({ loadSettings: vi.fn().mockResolvedValue(undefined), settings: {} }),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      loadConfig: vi.fn().mockResolvedValue(undefined),
      setupEventListeners: vi.fn().mockResolvedValue(() => {}),
    }),
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
    (sel: (s: Record<string, unknown>) => unknown) => sel({ connections: {} }),
    {
      getState: () => ({
        connections: {},
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
  openNewConnectionDialog: vi.fn(),
  openBackupWindow: vi.fn(),
  openDataSyncWindow: vi.fn(),
  openSchemaDiffWindow: vi.fn(),
  openWorkflowWindow: vi.fn(),
  openDashboardWindow: vi.fn(),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
  listenCrossWindow: (...args: Parameters<typeof listenCrossWindowMock>) =>
    listenCrossWindowMock(...args),
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
  DashboardPanel: () => <div data-testid="dashboard-panel">dashboard</div>,
}));

vi.mock('../../workflow/WorkflowPage', () => ({
  WorkflowPage: () => <div data-testid="workflow-window">workflow</div>,
}));

vi.mock('../../settings/SettingsPage', () => ({
  SettingsPage: ({ initialSection, onBack }: { initialSection?: string; onBack: () => void }) => (
    <div data-testid="settings-page">
      settings-page section={initialSection ?? 'general'}
      <button type="button" data-testid="settings-back" onClick={onBack}>
        back
      </button>
    </div>
  ),
}));

vi.mock('../../workspace/WorkspaceView', () => ({
  WorkspaceView: ({ onOpenPlugins }: { onOpenPlugins?: () => void }) => (
    <div data-testid="workspace-view">
      workspace-view
      <button type="button" data-testid="workspace-goto-plugins" onClick={onOpenPlugins}>
        goto plugins
      </button>
    </div>
  ),
}));

vi.mock('../../plugins/PluginManagementPage', () => ({
  PluginManagementPage: ({ onOpenInWorkspace }: { onOpenInWorkspace?: () => void }) => (
    <div data-testid="plugins-management-page">
      plugins-management-page
      <button type="button" data-testid="plugins-goto-workspace" onClick={onOpenInWorkspace}>
        goto workspace
      </button>
    </div>
  ),
}));

vi.mock('../../../stores/dashboardStore', () => {
  const state = { list: [], fetchDashboards: vi.fn().mockResolvedValue(undefined) };
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

beforeEach(() => {
  vi.clearAllMocks();
  menuOpenSettingsHandler.current = null;
  localStorage.clear();
});

afterEach(cleanup);

describe('ConnectionPage plugin nav integration (F4)', () => {
  it('renders the two new aside buttons between dashboard and settings', () => {
    render(<ConnectionPage />);

    expect(screen.getByTestId('workspace-nav-workspace-pages')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-nav-plugins')).toBeInTheDocument();

    const order = [
      'workspace-nav-connections',
      'workspace-nav-workflow',
      'workspace-nav-dashboard',
      'workspace-nav-workspace-pages',
      'workspace-nav-plugins',
    ].map((id) => screen.getByTestId(id));
    for (let i = 1; i < order.length; i++) {
      const pos = order[i - 1]!.compareDocumentPosition(order[i]!);
      expect(pos & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  });

  it('switches to the Workspace view with its own title on click', () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workspace-pages'));

    expect(screen.getByTestId('workspace-view')).toBeInTheDocument();
    expect(screen.queryByTestId('mock-content-view')).not.toBeInTheDocument();
    expect(screen.getByTestId('title-bar')).toHaveTextContent('nav.workspacePages');
    expect(screen.queryByTestId('navigator-tree')).not.toBeInTheDocument();
  });

  it('switches to the plugins management page with its own title on click', () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-plugins'));

    expect(screen.getByTestId('plugins-management-page')).toBeInTheDocument();
    expect(screen.getByTestId('title-bar')).toHaveTextContent('nav.plugins');
  });

  it('wires cross-mode shortcuts: workspace empty-state → plugins page → back', () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workspace-pages'));
    fireEvent.click(screen.getByTestId('workspace-goto-plugins'));
    expect(screen.getByTestId('plugins-management-page')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('plugins-goto-workspace'));
    expect(screen.getByTestId('workspace-view')).toBeInTheDocument();
  });

  it('keeps connection tabs intact across workspace/plugins mode round-trips', async () => {
    localStorage.setItem(
      'datazen:pending-connection',
      JSON.stringify({ configId: 'cfg-1', connectionName: 'Local PG' }),
    );
    connectMock.mockResolvedValue('conn-live-1');

    render(<ConnectionPage />);
    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('cfg-1'));

    fireEvent.click(screen.getByTestId('workspace-nav-workspace-pages'));
    expect(screen.queryByTestId('mock-content-view')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-nav-plugins'));
    expect(screen.queryByTestId('mock-content-view')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-nav-connections'));
    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
    // No reconnect: the connection tab survived the mode switches untouched.
    expect(connectMock).toHaveBeenCalledTimes(1);
  });

  it('settings round-trip started from workspace mode restores workspace mode', async () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workspace-pages'));
    expect(screen.getByTestId('workspace-view')).toBeInTheDocument();

    await waitFor(() => expect(menuOpenSettingsHandler.current).not.toBeNull());
    menuOpenSettingsHandler.current?.({ section: 'general' });
    await waitFor(() => expect(screen.getByTestId('settings-page')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('settings-back'));
    await waitFor(() => expect(screen.queryByTestId('settings-page')).not.toBeInTheDocument());
    expect(screen.getByTestId('workspace-view')).toBeInTheDocument();
  });

  it('nav buttons reflect the active mode highlight', () => {
    render(<ConnectionPage />);

    const workspaceBtn = screen.getByTestId('workspace-nav-workspace-pages');
    expect(workspaceBtn.className).not.toMatch(/bg-accent\/20/);
    fireEvent.click(workspaceBtn);
    expect(workspaceBtn.className).toMatch(/bg-accent\/20/);
    expect(screen.getByTestId('workspace-nav-plugins').className).not.toMatch(/bg-accent\/20/);

    fireEvent.click(screen.getByTestId('workspace-nav-plugins'));
    expect(screen.getByTestId('workspace-nav-plugins').className).toMatch(/bg-accent\/20/);
    expect(workspaceBtn.className).not.toMatch(/bg-accent\/20/);
  });
});
