import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, screen, fireEvent, waitFor } from '@testing-library/react';
import { ConnectionPage } from '../ConnectionPage';
import { SAMPLE_GETTING_STARTED_SQL } from '../gettingStartedQuery';
import { useUiStore } from '../../../stores/uiStore';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { PENDING_CONNECTION_KEY } from '../../../lib/windowManager';
import type { ConnectionViewActions } from '../../../lib/connectionViews/types';

const { fetchConnectionsMock, fetchGroupsMock, listenCrossWindowMock, newQueryMock } = vi.hoisted(
  () => ({
    fetchConnectionsMock: vi.fn().mockResolvedValue(undefined),
    fetchGroupsMock: vi.fn().mockResolvedValue(undefined),
    listenCrossWindowMock: vi.fn(() => Promise.resolve(() => {})),
    newQueryMock: vi.fn(() => true),
  }),
);

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
        removeByDbSessionId: vi.fn(),
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
    executeQuery: vi.fn().mockResolvedValue(undefined),
  });
  return {
    usePanelStore: store,
    nextPanelId: (prefix: string) => `panel-${prefix}-test`,
  };
});

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    connect: vi.fn(),
    releaseConnection: vi.fn(),
    pingConnection: vi.fn(),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  PENDING_CONNECTION_KEY: 'datazen:pending-connection',
  hasOpenChildWindows: vi.fn().mockResolvedValue(false),
  openNewConnectionDialog: vi.fn(),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
  listenCrossWindow: (...args: unknown[]) => listenCrossWindowMock(...args),
}));

vi.mock('../ContentView', async () => {
  const { useLayoutEffect } = await import('react');
  return {
    ContentView: ({
      actionsRef,
    }: {
      actionsRef?: { current: ConnectionViewActions | undefined };
    }) => {
      useLayoutEffect(() => {
        if (actionsRef) {
          actionsRef.current = {
            newQuery: newQueryMock,
            openErDiagram: vi.fn(),
            refresh: vi.fn(),
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
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../../../components/MenuBar', () => ({
  MenuBar: () => <div data-testid="menu-bar">menu</div>,
}));

vi.mock('../../../components/ThemeToggle', () => ({
  ThemeToggle: () => <div data-testid="theme-toggle">theme</div>,
}));

vi.mock('../ConnectionNavigatorTree', async () => {
  const { forwardRef } = await import('react');
  return {
    ConnectionNavigatorTree: forwardRef((_props: unknown, _ref: unknown) => (
      <div data-testid="navigator-tree">tree</div>
    )),
  };
});

vi.mock('../../dashboard/DashboardPanel', () => ({
  DashboardPanel: () => <div data-testid="dashboard-panel">dashboard</div>,
}));

vi.mock('../../workflow/WorkflowPage', () => ({
  WorkflowPage: () => <div data-testid="workflow-window">workflow</div>,
}));

vi.mock('../../settings/SettingsContent', () => ({
  SettingsContent: () => <div data-testid="settings-content-mock">settings</div>,
}));

vi.mock('../../workspace/WorkspaceView', () => ({
  WorkspaceView: () => <div data-testid="workspace-view">workspace</div>,
}));

vi.mock('../../wapps/WappManagementPage', () => ({
  WappManagementPage: () => <div data-testid="wapp-management">plugins</div>,
}));

vi.mock('../../../stores/dashboardStore', () => {
  const state = { list: [] as Array<{ id: string; name: string }>, fetchDashboards: vi.fn() };
  const store = (sel: (s: typeof state) => unknown) => sel(state);
  store.getState = () => state;
  return { useDashboardStore: store };
});

describe('ConnectionPage Onboarding & Dual Mode Sidebar', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
    useUiStore.setState({ workspaceSidebarMode: 'icons' });
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it('allows toggling workspace sidebar mode between icons and expanded', () => {
    expect(useUiStore.getState().workspaceSidebarMode).toBe('icons');
    useUiStore.getState().toggleWorkspaceSidebarMode();
    expect(useUiStore.getState().workspaceSidebarMode).toBe('expanded');
    useUiStore.getState().toggleWorkspaceSidebarMode();
    expect(useUiStore.getState().workspaceSidebarMode).toBe('icons');
  });

  it('renders OnboardingGuideBar when onboarding is active', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    render(<ConnectionPage />);
    expect(screen.getByTestId('onboarding-guide-bar')).toBeInTheDocument();
  });

  it('renders workspace sidebar toggle button', () => {
    render(<ConnectionPage />);
    expect(screen.getByTestId('workspace-sidebar-toggle')).toBeInTheDocument();
  });

  it('toggles sidebar mode when clicking workspace sidebar toggle', () => {
    render(<ConnectionPage />);
    expect(useUiStore.getState().workspaceSidebarMode).toBe('icons');
    fireEvent.click(screen.getByTestId('workspace-sidebar-toggle'));
    expect(useUiStore.getState().workspaceSidebarMode).toBe('expanded');
  });

  it('persists workspace sidebar mode to localStorage', () => {
    render(<ConnectionPage />);
    expect(localStorage.getItem('datazen:workspace-sidebar-mode')).toBeNull();
    fireEvent.click(screen.getByTestId('workspace-sidebar-toggle'));
    expect(localStorage.getItem('datazen:workspace-sidebar-mode')).toBe('expanded');
    fireEvent.click(screen.getByTestId('workspace-sidebar-toggle'));
    expect(localStorage.getItem('datazen:workspace-sidebar-mode')).toBe('icons');
  });

  it('auto-opens getting_started.sql with preset SQL when onboarding sample connection connects', async () => {
    const sampleConnectionId = 'sample_sqlite';
    useOnboardingStore.getState().startOnboarding(sampleConnectionId);
    localStorage.setItem(
      PENDING_CONNECTION_KEY,
      JSON.stringify({
        connectionId: sampleConnectionId,
        dbSessionId: 'session-onboarding-1',
        connectionName: 'Sample SQLite',
        databaseType: 'sqlite',
      }),
    );

    render(<ConnectionPage />);

    await waitFor(() => {
      expect(newQueryMock).toHaveBeenCalledWith(
        SAMPLE_GETTING_STARTED_SQL,
        undefined,
        'getting_started.sql',
      );
    });
  });
});
