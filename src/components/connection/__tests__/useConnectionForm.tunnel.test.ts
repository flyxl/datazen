import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConnectionForm } from '../useConnectionForm';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

const { testConnectionMock, saveConnectionMock } = vi.hoisted(() => ({
  testConnectionMock: vi
    .fn()
    .mockResolvedValue({ serverVersion: '16.0', serverType: 'postgresql' }),
  saveConnectionMock: vi.fn(),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    testConnection: testConnectionMock,
    saveConnection: saveConnectionMock,
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    vi.fn((selector: (s: { saveConnection: typeof saveConnectionMock }) => unknown) =>
      selector({ saveConnection: saveConnectionMock }),
    ),
    { getState: () => ({ saveConnection: saveConnectionMock }) },
  ),
}));

describe('useConnectionForm tunnel kinds', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    testConnectionMock.mockResolvedValue({ serverVersion: '16.0', serverType: 'postgresql' });
  });

  it('builds httpProxy tunnel into onTest IPC payload', async () => {
    const { result } = renderHook(() => useConnectionForm());
    act(() => {
      result.current.setName('Proxy PG');
      result.current.setHost('db.internal');
      result.current.setPort('5432');
      result.current.setTunnelKind('httpProxy');
      result.current.setHttpProxyHost('proxy.example');
      result.current.setHttpProxyPort('8080');
      result.current.setHttpProxyScheme('https');
    });

    await act(async () => {
      await result.current.onTest();
    });

    expect(testConnectionMock).toHaveBeenCalledTimes(1);
    const [config] = testConnectionMock.mock.calls[0] as [Record<string, unknown>];
    expect(config.tunnelKind).toBe('httpProxy');
    expect((config.httpProxyTunnel as { host: string; scheme: string }).host).toBe('proxy.example');
    expect((config.httpProxyTunnel as { host: string; scheme: string }).scheme).toBe('https');
  });

  it('builds websocket tunnel into onTest IPC payload', async () => {
    const { result } = renderHook(() => useConnectionForm());
    act(() => {
      result.current.setName('WS PG');
      result.current.setHost('db.internal');
      result.current.setPort('5432');
      result.current.setTunnelKind('websocket');
      result.current.setWsUrl('wss://relay.example/v1');
      result.current.setWsMode('raw_binary');
      result.current.setWsAuthToken('tok');
    });

    await act(async () => {
      await result.current.onTest();
    });

    const [config] = testConnectionMock.mock.calls[0] as [Record<string, unknown>];
    expect(config.tunnelKind).toBe('websocket');
    expect((config.websocketTunnel as { url: string; mode: string }).url).toBe(
      'wss://relay.example/v1',
    );
    expect((config.websocketTunnel as { url: string; mode: string }).mode).toBe('raw_binary');
  });

  it('loads httpProxy tunnel from existing connection', () => {
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-http',
        existingConnections: [
          {
            id: 'c-http',
            name: 'via-proxy',
            databaseType: 'postgresql',
            host: 'db.internal',
            port: 5432,
            sslMode: 'prefer',
            tunnelKind: 'httpProxy',
            httpProxyTunnel: {
              enabled: true,
              host: 'proxy.example',
              port: 3128,
              scheme: 'http',
              username: 'u',
              password: 'p',
              connectTimeoutSecs: 20,
            },
          },
        ],
      }),
    );
    expect(result.current.tunnelKind).toBe('httpProxy');
    expect(result.current.httpProxyHost).toBe('proxy.example');
    expect(result.current.httpProxyPort).toBe('3128');
  });

  it('loads websocket tunnel from existing connection', () => {
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-ws',
        existingConnections: [
          {
            id: 'c-ws',
            name: 'via-ws',
            databaseType: 'postgresql',
            host: 'db.internal',
            port: 5432,
            sslMode: 'prefer',
            tunnelKind: 'websocket',
            websocketTunnel: {
              enabled: true,
              url: 'wss://relay.example/t',
              mode: 'datazen_v1',
              authToken: 'abc',
              connectTimeoutSecs: 15,
            },
          },
        ],
      }),
    );
    expect(result.current.tunnelKind).toBe('websocket');
    expect(result.current.wsUrl).toBe('wss://relay.example/t');
    expect(result.current.wsAuthToken).toBe('abc');
  });

  it('validate requires proxy host/port and websocket url', () => {
    const { result } = renderHook(() => useConnectionForm());
    act(() => {
      result.current.setHost('127.0.0.1');
      result.current.setPort('5432');
      result.current.setTunnelKind('httpProxy');
      result.current.setHttpProxyHost('');
      result.current.setHttpProxyPort('');
    });
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.httpProxyHost).toBe('newConn.required');
    expect(result.current.validationErrors.httpProxyPort).toBe('newConn.required');

    act(() => {
      result.current.setTunnelKind('websocket');
      result.current.setWsUrl('');
    });
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.wsUrl).toBe('newConn.required');
  });

  it('setTunnelKind(ssh) enables ssh', () => {
    const { result } = renderHook(() => useConnectionForm());
    act(() => {
      result.current.setTunnelKind('ssh');
      result.current.setSshHost('bastion');
      result.current.setSshUsername('ops');
      result.current.setSshAuthMethod('agent');
    });
    expect(result.current.sshEnabled).toBe(true);
    expect(result.current.tunnelKind).toBe('ssh');
  });
});
