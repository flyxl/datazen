import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@testing-library/react';
import { ConnectionWindow } from '../ConnectionWindow';

const {
  connectMock,
  disconnectMock,
  pingMock,
  loadSettingsMock,
  loadAiConfigMock,
  setupAiListenersMock,
  getUrlParamMock,
  emitCrossWindowMock,
  listenCrossWindowMock,
  getActiveConnectionState,
  destroyMock,
} = vi.hoisted(() => ({
  connectMock: vi.fn(),
  disconnectMock: vi.fn(),
  pingMock: vi.fn(),
  loadSettingsMock: vi.fn().mockResolvedValue(undefined),
  loadAiConfigMock: vi.fn().mockResolvedValue(undefined),
  setupAiListenersMock: vi.fn().mockResolvedValue(() => {}),
  getUrlParamMock: vi.fn(),
  emitCrossWindowMock: vi.fn().mockResolvedValue(undefined),
  listenCrossWindowMock: vi.fn().mockResolvedValue(() => {}),
  getActiveConnectionState: vi.fn(() => ({
    connections: {} as Record<string, { status: string; connectionId?: string }>,
  })),
  destroyMock: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useSettings', () => ({
  useSettings: () => {},
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

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: {
    getState: () => getActiveConnectionState(),
  },
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    connect: (...args: unknown[]) => connectMock(...args),
    disconnect: (...args: unknown[]) => disconnectMock(...args),
    pingConnection: (...args: unknown[]) => pingMock(...args),
  },
}));

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (key: string) => getUrlParamMock(key),
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
    destroy: destroyMock,
    close: vi.fn(),
    onCloseRequested: vi.fn().mockResolvedValue(() => {}),
  }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  getActiveConnectionState.mockReturnValue({ connections: {} });
  getUrlParamMock.mockImplementation((key: string) => {
    if (key === 'connectionId') return '';
    if (key === 'configId') return '';
    if (key === 'connectionName') return 'Pg';
    if (key === 'databaseType') return 'postgresql';
    if (key === 'database') return 'postgres';
    return null;
  });
  connectMock.mockResolvedValue('conn-live-1');
  pingMock.mockResolvedValue(undefined);
  disconnectMock.mockResolvedValue(undefined);
  destroyMock.mockResolvedValue(undefined);
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
  it('TC-window: shows missing-params when configId and connectionId absent', () => {
    render(<ConnectionWindow />);
    expect(screen.getByText('connWin.missingParams')).toBeInTheDocument();
  });

  it('TC-window: connects via configId and renders view', async () => {
    getUrlParamMock.mockImplementation((key: string) => {
      if (key === 'connectionId') return '';
      if (key === 'configId') return 'cfg-1';
      if (key === 'connectionName') return 'Local PG';
      if (key === 'databaseType') return 'postgresql';
      return null;
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

  it('TC-window: shows connect error UI when connect fails and close destroys window', async () => {
    getUrlParamMock.mockImplementation((key: string) => {
      if (key === 'configId') return 'cfg-bad';
      if (key === 'connectionName') return 'Bad';
      if (key === 'databaseType') return 'postgresql';
      return '';
    });
    connectMock.mockRejectedValue(new Error('boom'));

    render(<ConnectionWindow />);

    await waitFor(() => expect(screen.getByText('boom')).toBeInTheDocument());
    fireEvent.click(screen.getByText('common.close'));
    await waitFor(() => expect(destroyMock).toHaveBeenCalled());
  });

  it('TC-window: uses existing connectionId from URL without reconnect', async () => {
    getUrlParamMock.mockImplementation((key: string) => {
      if (key === 'connectionId') return 'already-open';
      if (key === 'configId') return 'cfg-1';
      if (key === 'connectionName') return 'Local PG';
      if (key === 'databaseType') return 'postgresql';
      return null;
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

    getUrlParamMock.mockImplementation((key: string) => {
      if (key === 'configId') return 'cfg-reuse';
      if (key === 'connectionName') return 'Reuse';
      if (key === 'databaseType') return 'postgresql';
      return '';
    });

    render(<ConnectionWindow />);
    await waitFor(() => expect(screen.getByTestId('mock-view')).toHaveTextContent('view:reuse-1'));
    expect(connectMock).not.toHaveBeenCalled();
  });

  it('TC-window: passes configId to view component for dashboard use', async () => {
    getUrlParamMock.mockImplementation((key: string) => {
      if (key === 'connectionId') return '';
      if (key === 'configId') return 'cfg-dash';
      if (key === 'connectionName') return 'Dashboard PG';
      if (key === 'databaseType') return 'postgresql';
      return null;
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
    getUrlParamMock.mockImplementation((key: string) => {
      if (key === 'configId') return 'cfg-slow';
      if (key === 'connectionName') return 'Slow';
      if (key === 'databaseType') return 'postgresql';
      return '';
    });

    render(<ConnectionWindow />);
    await waitFor(() => expect(screen.getByText('conn.connecting')).toBeInTheDocument(), {
      timeout: 2000,
    });
    resolveConnect('conn-slow');
    await waitFor(() => expect(screen.getByTestId('mock-view')).toBeInTheDocument());
  });
});
