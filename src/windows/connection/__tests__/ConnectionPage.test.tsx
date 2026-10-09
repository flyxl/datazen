import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@testing-library/react';
import { ConnectionPage } from '../ConnectionPage';
import { tauriWindowTestState } from '../../../test/mocks/tauriWindow';
import type { Dashboard } from '../../../types/dashboard';

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
  webviewGetAllMock,
  hasOpenChildWindowsMock,
  fetchConnectionsMock,
  fetchGroupsMock,
  fetchDashboardsMock,
  openWorkflowWindowMock,
  openDashboardWindowMock,
  openBackupWindowMock,
  openDataSyncWindowMock,
  openSchemaDiffWindowMock,
  openNewConnectionDialogMock,
  menuOpenSettingsHandler,
  openErDiagramMock,
} = vi.hoisted(() => ({
  connectMock: vi.fn(),
  releaseConnectionMock: vi.fn(),
  pingMock: vi.fn(),
  loadSettingsMock: vi.fn().mockResolvedValue(undefined),
  loadAiConfigMock: vi.fn().mockResolvedValue(undefined),
  setupAiListenersMock: vi.fn().mockResolvedValue(() => {}),
  emitCrossWindowMock: vi.fn().mockResolvedValue(undefined),
  listenCrossWindowMock: vi.fn((...args: unknown[]) => {
    const event = args[0] as string;
    const handler = args[1] as (payload?: unknown) => void;
    if (event === 'menu:open-settings') {
      menuOpenSettingsHandler.current = handler;
    }
    return Promise.resolve(() => {});
  }),
  getActiveConnectionState: vi.fn(() => ({
    connections: {} as Record<
      string,
      { status: string; connectionId?: string; dbSessionId?: string }
    >,
  })),
  webviewGetAllMock: vi.fn().mockResolvedValue([{ label: 'main' }]),
  hasOpenChildWindowsMock: vi.fn().mockResolvedValue(false),
  fetchConnectionsMock: vi.fn().mockResolvedValue(undefined),
  fetchGroupsMock: vi.fn().mockResolvedValue(undefined),
  fetchDashboardsMock: vi.fn().mockResolvedValue(undefined),
  openWorkflowWindowMock: vi.fn(),
  openDashboardWindowMock: vi.fn(),
  openBackupWindowMock: vi.fn(),
  openDataSyncWindowMock: vi.fn(),
  openSchemaDiffWindowMock: vi.fn(),
  openNewConnectionDialogMock: vi.fn(),
  openErDiagramMock: vi.fn(),
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
  useSettingsStore: (
    sel: (s: { loadSettings: () => Promise<void>; settings: unknown }) => unknown,
  ) => sel({ loadSettings: loadSettingsMock, settings: { theme: { mode: 'dark' } } }),
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

vi.mock('../../../stores/schemaStore', async () => {
  // `vi.mock` replaces the whole module, so `ContentView` importing
  // `useConnectionSchemaField` / `useConnectionColumnMaps` at render time
  // needs them here too. They come from the shared double (not a fixed `''`)
  // so a cache hit returns that session's field and a miss the empty snapshot.
  const { schemaStoreMockModule } = await import('../../../test/mocks/schemaStore');
  const perSession = schemaStoreMockModule({ schemas: new Map(), activeDbSessionId: null });
  return {
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
    useConnectionSchemaField: perSession.useConnectionSchemaField,
    useConnectionColumnMaps: perSession.useConnectionColumnMaps,
  };
});

vi.mock('../../../stores/tableDataStore', () => ({
  useTableDataStore: {
    getState: () => ({
      reset: vi.fn(),
      setActiveConnection: vi.fn(),
      removeConnection: vi.fn(),
    }),
  },
}));

vi.mock('../../../stores/panelStore', async () => {
  // `vi.mock` replaces the whole module, so the pane-identity helpers it
  // re-exports must exist here too. They live in a dependency-free leaf
  // module, so take the real implementations rather than faking a shape.
  //
  // `EMPTY_QUERY_EXEC` (also re-exported by `panelStore`) is deliberately NOT
  // pulled in here: loading the real `stores/queryExecActions` graph flips the
  // "minimizes main instead of closing" case — `hasOpenChildWindows` is never
  // called and `close()` runs instead (3/3 runs). Nothing in this suite renders
  // `QueryPanel`/`useQueryExec`, so the gap stays latent; closing it means
  // re-examining that window-close path, not editing the mock.
  const pane = await vi.importActual<typeof import('../../../stores/paneKeys')>(
    '../../../stores/paneKeys',
  );
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
    ...pane,
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
  hasOpenChildWindows: (...args: unknown[]) => hasOpenChildWindowsMock(...args),
  openNewConnectionDialog: (...args: unknown[]) => openNewConnectionDialogMock(...args),
  openBackupWindow: (...args: unknown[]) => openBackupWindowMock(...args),
  openDataSyncWindow: (...args: unknown[]) => openDataSyncWindowMock(...args),
  openSchemaDiffWindow: (...args: unknown[]) => openSchemaDiffWindowMock(...args),
  openWorkflowWindow: (...args: unknown[]) => openWorkflowWindowMock(...args),
  openDashboardWindow: (...args: unknown[]) => openDashboardWindowMock(...args),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => emitCrossWindowMock(...args),
  listenCrossWindow: (...args: unknown[]) => listenCrossWindowMock(...args),
}));

vi.mock('../ContentView', async () => {
  const { useLayoutEffect } = await import('react');
  return {
    ContentView: ({ actionsRef }: { actionsRef?: { current: unknown } }) => {
      useLayoutEffect(() => {
        if (actionsRef) {
          actionsRef.current = {
            openErDiagram: (focusTable?: string, database?: string) =>
              openErDiagramMock(focusTable, database),
          };
        }
        return () => {
          if (actionsRef) actionsRef.current = undefined;
        };
      }, [actionsRef]);
      return <div data-testid="mock-content-view">content-view</div>;
    },
  };
});

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title, leftContent }: { title: string; leftContent?: React.ReactNode }) => (
    <div data-testid="title-bar">
      {leftContent}
      {title}
    </div>
  ),
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

vi.mock('../ConnectionNavigatorTree', async () => {
  const { forwardRef } = await import('react');
  return {
    ConnectionNavigatorTree: forwardRef<
      HTMLDivElement,
      {
        viewActions?: {
          openErDiagram?: (focusTable?: string, database?: string) => void;
        };
      }
    >(({ viewActions }, _ref) => (
      <div data-testid="navigator-tree">
        tree
        <button
          type="button"
          data-testid="open-target-er-diagram"
          onClick={() => viewActions?.openErDiagram?.(undefined, 'analytics')}
        >
          open target ER diagram
        </button>
      </div>
    )),
  };
});

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

vi.mock('../../workflow/WorkflowPage', () => ({
  WorkflowPage: ({
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

vi.mock('../../settings/SettingsContent', () => ({
  SettingsContent: ({ initialSection }: { initialSection?: string }) => (
    <div data-testid="settings-content-mock">section={initialSection ?? 'general'}</div>
  ),
}));

vi.mock('../../../stores/dashboardStore', () => {
  const state = {
    list: [] as Dashboard[],
    fetchDashboards: fetchDashboardsMock,
  };
  const store = (sel: (s: typeof state) => unknown) => sel(state);
  store.getState = () => state;
  store.setState = (partial: Partial<typeof state>) => Object.assign(state, partial);
  return { useDashboardStore: store };
});

/** Only `id`/`name` are read here; the rest keeps the object a real `Dashboard`. */
function makeDashboard(id: string, name: string): Dashboard {
  return {
    id,
    name,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    layout: { cols: 12, rowHeight: 1 },
    widgets: [],
    enabled: true,
  };
}

vi.mock('../../../lib/databaseTypes', async () => {
  const actual = await vi.importActual<typeof import('../../../lib/databaseTypes')>(
    '../../../lib/databaseTypes',
  );
  return {
    ...actual,
    getDbLabel: (t: string) => t.toUpperCase(),
  };
});

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: {
    getAll: (...args: unknown[]) => webviewGetAllMock(...args),
  },
}));

function setPendingConnection(data: Record<string, string>) {
  localStorage.setItem('datazen:pending-connection', JSON.stringify(data));
}

beforeEach(() => {
  vi.clearAllMocks();
  menuOpenSettingsHandler.current = null;
  tauriWindowTestState.closeRequestedHandler.current = null;
  tauriWindowTestState.closeHandlerRegistrationCount.current = 0;
  tauriWindowTestState.closeMock.mockReset().mockResolvedValue(undefined);
  tauriWindowTestState.minimizeMock.mockReset().mockResolvedValue(undefined);
  hasOpenChildWindowsMock.mockResolvedValue(false);
  webviewGetAllMock.mockResolvedValue([{ label: 'main' }]);
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

describe('ConnectionPage', () => {
  it('TC-window: always renders navigator tree sidebar', () => {
    render(<ConnectionPage />);
    expect(screen.getByTestId('navigator-tree')).toBeInTheDocument();
  });

  it('forwards the navigator ER database target to the connection-view handler', () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('open-target-er-diagram'));

    expect(openErDiagramMock).toHaveBeenCalledWith(undefined, 'analytics');
  });

  it('TC-window: renders content view even with no active tab', () => {
    render(<ConnectionPage />);
    expect(screen.getByTestId('mock-content-view')).toBeInTheDocument();
  });

  it('TC-window: fetches connections and groups on mount', () => {
    render(<ConnectionPage />);
    expect(fetchConnectionsMock).toHaveBeenCalled();
    expect(fetchGroupsMock).toHaveBeenCalled();
  });

  it('TC-window: connects via localStorage pending connection and renders content view', async () => {
    setPendingConnection({
      connectionId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionPage />);

    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('cfg-1'));
    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
    expect(emitCrossWindowMock).toHaveBeenCalledWith(
      'datazen:connection-ready',
      expect.objectContaining({ connectionId: 'cfg-1', dbSessionId: 'conn-live-1' }),
    );
  });

  it('TC-window: shows connect error UI with retry and close buttons', async () => {
    setPendingConnection({
      connectionId: 'cfg-bad',
      connectionName: 'Bad',
      databaseType: 'postgresql',
    });
    connectMock.mockRejectedValue(new Error('boom'));

    render(<ConnectionPage />);

    await waitFor(() => expect(screen.getByText('boom')).toBeInTheDocument());
    expect(screen.getByText('common.retry')).toBeInTheDocument();
    expect(screen.getByText('common.close')).toBeInTheDocument();
  });

  it('TC-window: uses existing connectionId from pending without reconnect', async () => {
    setPendingConnection({
      dbSessionId: 'already-open',
      connectionId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionPage />);

    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
    expect(connectMock).not.toHaveBeenCalled();
  });

  it('TC-window: reuses activeConnectionStore session when present', async () => {
    getActiveConnectionState.mockReturnValue({
      connections: {
        'cfg-reuse': { status: 'connected', dbSessionId: 'reuse-1' },
      },
    });

    setPendingConnection({
      connectionId: 'cfg-reuse',
      connectionName: 'Reuse',
      databaseType: 'postgresql',
    });

    render(<ConnectionPage />);
    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
    expect(connectMock).not.toHaveBeenCalled();
  });

  it('TC-window: renders content view after successful connection', async () => {
    setPendingConnection({
      connectionId: 'cfg-dash',
      connectionName: 'Dashboard PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionPage />);

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
      connectionId: 'cfg-slow',
      connectionName: 'Slow',
      databaseType: 'postgresql',
    });

    render(<ConnectionPage />);
    await waitFor(() => expect(screen.getByText('conn.connecting')).toBeInTheDocument(), {
      timeout: 2000,
    });
    // ContentView is hidden while connecting to avoid duplicate loading indicators
    expect(screen.queryByTestId('mock-content-view')).not.toBeInTheDocument();
    resolveConnect('conn-slow');
    await waitFor(() => expect(screen.getByTestId('mock-content-view')).toBeInTheDocument());
  });

  it('TC-window: switches workspace via icon rail', async () => {
    const { useDashboardStore } = await import('../../../stores/dashboardStore');
    useDashboardStore.setState({
      list: [makeDashboard('dash-1', 'Ops Board')],
    });

    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-nav-dashboard'));
    // Sidebar icon is a plain mode switch; the real DashboardPanel fetches on
    // its own mount (here mocked, so just assert the panel appears).
    await waitFor(() => expect(screen.getByTestId('dashboard-panel')).toBeInTheDocument());
    expect(screen.queryByTestId('workflow-window')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-nav-databases'));
    expect(screen.getByTestId('navigator-tree')).toBeInTheDocument();
  });

  it('TC-window: allows embedded workflow and dashboard to switch each other', async () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    fireEvent.click(screen.getByTestId('workflow-open-dashboard'));
    await waitFor(() => expect(screen.getByTestId('dashboard-panel')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-open-workflow'));
    await waitFor(() => expect(screen.getByTestId('workflow-window')).toBeInTheDocument());
  });

  it('unmounts inactive mode panels (conditional render) and remounts on return', async () => {
    const { useDashboardStore } = await import('../../../stores/dashboardStore');
    useDashboardStore.setState({
      list: [makeDashboard('dash-1', 'Ops Board')],
    });
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();

    // Switching away unmounts the inactive panel: only one mode's DOM exists.
    fireEvent.click(screen.getByTestId('workspace-nav-dashboard'));
    await waitFor(() => expect(screen.getByTestId('dashboard-panel')).toBeInTheDocument());
    expect(screen.queryByTestId('workflow-window')).not.toBeInTheDocument();

    // Switching back remounts the panel (view state restores from snapshot).
    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();
    expect(screen.queryByTestId('dashboard-panel')).not.toBeInTheDocument();
  });

  it('TC-window: menu:open-settings shows SettingsPage with section', async () => {
    render(<ConnectionPage />);

    await waitFor(() => expect(menuOpenSettingsHandler.current).not.toBeNull());
    menuOpenSettingsHandler.current?.({ section: 'ai' });

    await waitFor(() => expect(screen.getByTestId('settings-page')).toBeInTheDocument());
    expect(screen.getByTestId('settings-content-mock')).toHaveTextContent('section=ai');
    expect(screen.getByTestId('menu-bar')).toBeInTheDocument();
    expect(screen.queryByTestId('navigator-tree')).not.toBeInTheDocument();
  });

  it('[tester] settings view keeps TitleBar with settings title and back control', async () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-settings'));
    await waitFor(() => expect(screen.getByTestId('settings-page')).toBeInTheDocument());

    const titleBar = screen.getByTestId('title-bar');
    expect(titleBar).toHaveTextContent('win.settings');
    expect(screen.getByTestId('menu-bar')).toBeInTheDocument();
    expect(screen.getByTestId('settings-back')).toBeInTheDocument();
  });

  it('TC-window: SettingsPage back returns to workspace and restores mode', async () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();

    await waitFor(() => expect(menuOpenSettingsHandler.current).not.toBeNull());
    menuOpenSettingsHandler.current?.({ section: 'general' });
    await waitFor(() => expect(screen.getByTestId('settings-page')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('settings-back'));
    await waitFor(() => expect(screen.queryByTestId('settings-page')).not.toBeInTheDocument());
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();
  });

  it('TC-window: sidebar Settings opens SettingsPage', async () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-settings'));
    await waitFor(() => expect(screen.getByTestId('settings-page')).toBeInTheDocument());
    expect(screen.getByTestId('menu-bar')).toBeInTheDocument();
    expect(screen.queryByTestId('navigator-tree')).not.toBeInTheDocument();
    expect(screen.queryByTestId('workspace-nav-databases')).not.toBeInTheDocument();
  });

  it('TC-window: sidebar Settings from workflow returns to workflow on back', async () => {
    render(<ConnectionPage />);

    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('workspace-nav-settings'));
    await waitFor(() => expect(screen.getByTestId('settings-page')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('settings-back'));
    await waitFor(() => expect(screen.queryByTestId('settings-page')).not.toBeInTheDocument());
    expect(screen.getByTestId('workflow-window')).toBeInTheDocument();
  });

  it('TC-window: sidebar Settings button has no unreachable active highlight', () => {
    render(<ConnectionPage />);

    const connectionsNav = screen.getByTestId('workspace-nav-databases');
    const settingsNav = screen.getByTestId('workspace-nav-settings');

    expect(connectionsNav.className).toMatch(/bg-accent\/20/);
    expect(settingsNav.className).not.toMatch(/bg-accent\/20/);
  });

  it('TC-window: minimizes main instead of closing when sub-windows are open', async () => {
    hasOpenChildWindowsMock.mockResolvedValueOnce(true);
    setPendingConnection({
      connectionId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionPage />);
    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('cfg-1'));
    await waitFor(() => expect(tauriWindowTestState.closeRequestedHandler.current).not.toBeNull());

    const preventDefault = vi.fn();
    await tauriWindowTestState.closeRequestedHandler.current?.({ preventDefault });

    expect(preventDefault).toHaveBeenCalled();
    expect(tauriWindowTestState.minimizeMock).toHaveBeenCalled();
    expect(releaseConnectionMock).not.toHaveBeenCalled();
    expect(tauriWindowTestState.closeMock).not.toHaveBeenCalled();
  });

  it('TC-window: releases connections and closes when no sub-windows remain', async () => {
    hasOpenChildWindowsMock.mockResolvedValueOnce(false);
    setPendingConnection({
      connectionId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionPage />);
    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('cfg-1'));
    await waitFor(() =>
      expect(emitCrossWindowMock).toHaveBeenCalledWith(
        'datazen:connection-ready',
        expect.objectContaining({ dbSessionId: 'conn-live-1' }),
      ),
    );
    await waitFor(() =>
      expect(tauriWindowTestState.closeHandlerRegistrationCount.current).toBeGreaterThanOrEqual(2),
    );
    await waitFor(() => expect(tauriWindowTestState.closeRequestedHandler.current).not.toBeNull());

    const preventDefault = vi.fn();
    await tauriWindowTestState.closeRequestedHandler.current?.({ preventDefault });

    await waitFor(() => expect(releaseConnectionMock).toHaveBeenCalledWith('conn-live-1'));
    expect(preventDefault).toHaveBeenCalled();
    expect(tauriWindowTestState.minimizeMock).not.toHaveBeenCalled();
    expect(tauriWindowTestState.closeMock).toHaveBeenCalled();
  });

  it('TC-sidebar: keeps sidebar resizable when switching to workflow and back', async () => {
    render(<ConnectionPage />);
    const aside = screen.getByTestId('connection-navigator-aside');
    expect(aside).toBeInTheDocument();
    expect(aside).toHaveStyle({ width: '280px' });

    // Switch to workflow mode
    fireEvent.click(screen.getByTestId('workspace-nav-workflow'));
    expect(screen.queryByTestId('connection-navigator-aside')).not.toBeInTheDocument();

    // Switch back to connections mode
    fireEvent.click(screen.getByTestId('workspace-nav-databases'));
    const asideAfter = screen.getByTestId('connection-navigator-aside');
    expect(asideAfter).toBeInTheDocument();
    expect(asideAfter).toHaveStyle({ width: '280px' });
  });
});
