import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen } from '@testing-library/react';
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
    sel({ loadSettings: loadSettingsMock }),
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

vi.mock('../../../stores/queryStore', () => ({
  useQueryStore: {
    getState: () => ({
      reset: vi.fn(),
      setActiveConnection: vi.fn(),
      removeConnection: vi.fn(),
      tabs: [],
      activeTabId: '',
    }),
    setState: vi.fn(),
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

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    connect: (...args: unknown[]) => connectMock(...args),
    releaseConnection: (...args: unknown[]) => releaseConnectionMock(...args),
    pingConnection: (...args: unknown[]) => pingMock(...args),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  PENDING_CONNECTION_KEY: 'datazen:pending-connection',
  openNewConnectionWindow: vi.fn(),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => emitCrossWindowMock(...args),
  listenCrossWindow: (...args: unknown[]) => listenCrossWindowMock(...args),
}));

vi.mock('../../../lib/connectionViews', () => ({
  getConnectionView: () =>
    function MockSqlView({ connectionId, configId }: { connectionId: string; configId: string }) {
      return (
        <div data-testid="mock-view" data-config-id={configId}>
          view:{connectionId}
        </div>
      );
    },
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../ConnectionNavigatorTree', () => ({
  ConnectionNavigatorTree: () => <div data-testid="navigator-tree">tree</div>,
}));

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

  it('TC-window: shows placeholder when no active tab', () => {
    render(<ConnectionWindow />);
    expect(screen.getByText('connWin.noConnections')).toBeInTheDocument();
  });

  it('TC-window: fetches connections and groups on mount', () => {
    render(<ConnectionWindow />);
    expect(fetchConnectionsMock).toHaveBeenCalled();
    expect(fetchGroupsMock).toHaveBeenCalled();
  });

  it('TC-window: connects via localStorage pending connection and renders view', async () => {
    setPendingConnection({
      configId: 'cfg-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionWindow />);

    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('cfg-1'));
    await waitFor(() =>
      expect(screen.getByTestId('mock-view')).toHaveTextContent('view:conn-live-1'),
    );
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

    await waitFor(() =>
      expect(screen.getByTestId('mock-view')).toHaveTextContent('view:already-open'),
    );
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
    await waitFor(() => expect(screen.getByTestId('mock-view')).toHaveTextContent('view:reuse-1'));
    expect(connectMock).not.toHaveBeenCalled();
  });

  it('TC-window: passes configId to view component', async () => {
    setPendingConnection({
      configId: 'cfg-dash',
      connectionName: 'Dashboard PG',
      databaseType: 'postgresql',
    });

    render(<ConnectionWindow />);

    await waitFor(() => expect(screen.getByTestId('mock-view')).toBeInTheDocument());
    expect(screen.getByTestId('mock-view').getAttribute('data-config-id')).toBe('cfg-dash');
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
    await waitFor(() => expect(screen.getByTestId('mock-view')).toBeInTheDocument());
  });
});
