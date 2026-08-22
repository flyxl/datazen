import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PluginRequestEnvelope } from '../uiPluginBridge';

const {
  storageGetMock,
  storageSetMock,
  storageRemoveMock,
  driverExecuteMock,
  getConnectionsIpcMock,
  connectionStoreState,
  activeConnectionStoreState,
  notificationInvokeMock,
} = vi.hoisted(() => ({
  storageGetMock: vi.fn(),
  storageSetMock: vi.fn(),
  storageRemoveMock: vi.fn(),
  driverExecuteMock: vi.fn(),
  getConnectionsIpcMock: vi.fn(),
  connectionStoreState: {
    current: { connections: [], connectionsLoaded: false, loading: false },
  },
  activeConnectionStoreState: { current: { connections: {} } },
  notificationInvokeMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => notificationInvokeMock(...args),
}));

vi.mock('../../commands/plugins', () => ({
  pluginCommands: {
    pluginStorageGet: (...args: unknown[]) => storageGetMock(...args),
    pluginStorageSet: (...args: unknown[]) => storageSetMock(...args),
    pluginStorageRemove: (...args: unknown[]) => storageRemoveMock(...args),
  },
}));

vi.mock('../../commands/driver', () => ({
  driverCommands: {
    execute: (...args: unknown[]) => driverExecuteMock(...args),
  },
}));

vi.mock('../../commands/connection', () => ({
  connectionCommands: {
    getConnections: () => getConnectionsIpcMock(),
  },
}));

vi.mock('../../stores/connectionStore', () => ({
  useConnectionStore: { getState: () => connectionStoreState.current },
}));

vi.mock('../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: { getState: () => activeConnectionStoreState.current },
}));

import { THEME_TOKENS } from '../themeTokens';
import {
  attachBridge,
  BRIDGE_ERROR,
  BRIDGE_CHANNEL,
  MAX_INFLIGHT_REQUESTS,
} from '../uiPluginBridge';

type SentEnvelope = Record<string, unknown>;

function makeFakeIframe() {
  const sent: SentEnvelope[] = [];
  const fakeWindow = {
    postMessage: vi.fn((data: unknown) => {
      sent.push(data as SentEnvelope);
    }),
  };
  const iframe = document.createElement('iframe');
  Object.defineProperty(iframe, 'contentWindow', {
    configurable: true,
    get: () => fakeWindow,
  });
  return { iframe, sent };
}

/** Simulate a plugin → host message originating from the given source window. */
function receive(data: unknown, source: unknown): void {
  window.dispatchEvent(new MessageEvent('message', { data, source: source as Window | null }));
}

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

async function waitUntil(cond: () => boolean): Promise<void> {
  for (let i = 0; i < 100 && !cond(); i += 1) {
    await flush();
  }
}

let env: ReturnType<typeof makeFakeIframe>;

function request(type: string, reqId?: string, payload?: unknown): PluginRequestEnvelope {
  return { ch: BRIDGE_CHANNEL, type, target: 'host', ...(reqId ? { reqId } : {}), payload };
}

const ALL_PERMISSIONS = [
  'context:connections',
  'command:invoke',
  'storage:local',
  'ui:notify',
] as const;

beforeEach(() => {
  vi.clearAllMocks();
  env = makeFakeIframe();
  connectionStoreState.current = { connections: [], connectionsLoaded: false, loading: false };
  activeConnectionStoreState.current = { connections: {} };
});

afterEach(() => {
  vi.useRealTimers();
});

describe('uiPluginBridge handshake', () => {
  it('answers plugin.ready with host.ready carrying apiVersion/locale/dark/tokens', async () => {
    document.documentElement.classList.add('dark');
    const handle = attachBridge(env.iframe, {
      pluginId: 'acme.bill-audit',
      permissions: [],
      locale: 'de',
    });

    receive(request('plugin.ready'), env.iframe.contentWindow);
    await flush();

    expect(env.sent).toHaveLength(1);
    const ready = env.sent[0];
    expect(ready.ch).toBe(BRIDGE_CHANNEL);
    expect(ready.type).toBe('host.ready');
    expect(ready.target).toBe('host');
    const payload = ready.payload as Record<string, unknown>;
    expect(payload.apiVersion).toBe(2);
    expect(payload.locale).toBe('de');
    expect(payload.dark).toBe(true);
    for (const token of THEME_TOKENS) {
      expect(Object.keys(payload.tokens as Record<string, string>)).toContain(token);
    }

    handle.detach();
    document.documentElement.classList.remove('dark');
  });

  it('pushThemeSnapshot posts theme.apply with versioned snapshot', () => {
    const handle = attachBridge(env.iframe, { pluginId: 'acme.bill-audit', permissions: [] });
    handle.pushThemeSnapshot();

    expect(env.sent).toHaveLength(1);
    const apply = env.sent[0];
    expect(apply.type).toBe('theme.apply');
    const snapshot = apply.payload as Record<string, unknown>;
    expect(snapshot.v).toBe(2);
    expect(typeof snapshot.dark).toBe('boolean');
    expect(Object.keys(snapshot.tokens as Record<string, string>).sort()).toEqual(
      [...THEME_TOKENS].sort(),
    );
    handle.detach();
  });
});

describe('uiPluginBridge permission gate (deny-by-default)', () => {
  it.each([['context.getConnections'], ['context.getActiveConnection']])(
    'denies %s without context:connections',
    async (type) => {
      const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: [] });
      receive(request(type, 'r1'), env.iframe.contentWindow);
      await waitUntil(() => env.sent.length > 0);

      expect(env.sent[0].type).toBe(`${type}.err`);
      expect((env.sent[0].payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.PERMISSION);
      handle.detach();
    },
  );

  it('denies command.invoke without command:invoke', async () => {
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: [] });
    receive(
      request('command.invoke', 'r1', { configId: 'c', command: 'query' }),
      env.iframe.contentWindow,
    );
    await waitUntil(() => env.sent.length > 0);

    expect((env.sent[0].payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.PERMISSION);
    expect(driverExecuteMock).not.toHaveBeenCalled();
    handle.detach();
  });

  it('denies storage APIs without storage:local', async () => {
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: [] });
    for (const [i, type] of ['storage.get', 'storage.set', 'storage.remove'].entries()) {
      receive(request(type, `r${i}`, { key: 'k' }), env.iframe.contentWindow);
    }
    await waitUntil(() => env.sent.length === 3);

    for (const sent of env.sent) {
      expect((sent.payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.PERMISSION);
    }
    expect(storageGetMock).not.toHaveBeenCalled();
    handle.detach();
  });

  it('allows every API when its permission is declared in the manifest', async () => {
    storageGetMock.mockResolvedValue('stored');
    storageSetMock.mockResolvedValue(undefined);
    storageRemoveMock.mockResolvedValue(undefined);
    driverExecuteMock.mockResolvedValue({ data: { rows: [] } });
    notificationInvokeMock.mockResolvedValue(undefined);

    const handle = attachBridge(env.iframe, {
      pluginId: 'acme.bill-audit',
      permissions: [...ALL_PERMISSIONS],
    });

    receive(request('storage.get', 'a', { key: 'k' }), env.iframe.contentWindow);
    receive(request('storage.set', 'b', { key: 'k', value: { x: 1 } }), env.iframe.contentWindow);
    receive(request('storage.remove', 'c', { key: 'k' }), env.iframe.contentWindow);
    receive(
      request('command.invoke', 'd', {
        configId: 'cfg-1',
        command: 'query',
        args: { sql: 'select 1' },
      }),
      env.iframe.contentWindow,
    );
    receive(request('ui.notify', 'e', { title: 'Hello' }), env.iframe.contentWindow);

    await waitUntil(() => env.sent.length === 5);
    const byReqId = new Map(env.sent.map((m) => [m.reqId, m]));
    expect(byReqId.get('a')?.type).toBe('storage.get.ok');
    expect(byReqId.get('b')?.type).toBe('storage.set.ok');
    expect(byReqId.get('c')?.type).toBe('storage.remove.ok');
    expect(byReqId.get('d')?.type).toBe('command.invoke.ok');
    expect(byReqId.get('e')?.type).toBe('ui.notify.ok');

    // Storage IPC is namespaced by plugin id and passes raw values through.
    expect(storageGetMock).toHaveBeenCalledWith('acme.bill-audit', 'k');
    expect(storageSetMock).toHaveBeenCalledWith('acme.bill-audit', 'k', { x: 1 });
    expect(storageRemoveMock).toHaveBeenCalledWith('acme.bill-audit', 'k');
    // Driver command maps to execute_driver_command's request shape.
    expect(driverExecuteMock).toHaveBeenCalledWith({
      connectionId: 'cfg-1',
      command: 'query',
      input: { sql: 'select 1' },
    });
    expect(notificationInvokeMock).toHaveBeenCalledWith('plugin:notification|notify', {
      options: { title: 'Hello', body: undefined },
    });
    handle.detach();
  });

  it('answers i18n.getString with E_NOT_IMPLEMENTED (locales land in F8/F9)', async () => {
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: [] });
    receive(request('i18n.getString', 'r1', { key: 'greet' }), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length > 0);

    expect(env.sent[0].type).toBe('i18n.getString.err');
    expect((env.sent[0].payload as Record<string, unknown>).code).toBe(
      BRIDGE_ERROR.NOT_IMPLEMENTED,
    );
    handle.detach();
  });
});

describe('uiPluginBridge envelope semantics', () => {
  it('echoes reqId on .ok responses even when handlers complete out of order', async () => {
    let releaseFirst!: (v: unknown) => void;
    let releaseSecond!: (v: unknown) => void;
    storageGetMock
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            releaseFirst = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            releaseSecond = resolve;
          }),
      );

    const handle = attachBridge(env.iframe, {
      pluginId: 'p',
      permissions: ['storage:local'],
    });
    receive(request('storage.get', 'first', { key: 'a' }), env.iframe.contentWindow);
    receive(request('storage.get', 'second', { key: 'b' }), env.iframe.contentWindow);
    await flush();

    releaseSecond('b-value');
    await waitUntil(() => env.sent.some((m) => m.reqId === 'second'));
    const second = env.sent.find((m) => m.reqId === 'second');
    expect(second?.type).toBe('storage.get.ok');
    expect((second?.payload as Record<string, unknown>).value).toBe('b-value');
    expect(env.sent).not.toContainEqual(expect.objectContaining({ reqId: 'first' }));

    releaseFirst('a-value');
    await waitUntil(() => env.sent.length === 2);
    const first = env.sent.find((m) => m.reqId === 'first');
    expect(first?.type).toBe('storage.get.ok');
    expect((first?.payload as Record<string, unknown>).value).toBe('a-value');
    handle.detach();
  });

  it('replies E_NOT_FOUND to unknown api types', async () => {
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: [...ALL_PERMISSIONS] });
    receive(request('fs.readFile', 'r1'), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length > 0);

    expect(env.sent[0].type).toBe('fs.readFile.err');
    expect((env.sent[0].payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.NOT_FOUND);
    handle.detach();
  });

  it('ignores messages whose event.source is not this iframe', async () => {
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: [...ALL_PERMISSIONS] });

    receive(request('plugin.ready'), {});
    receive(request('storage.get', 'r1', { key: 'k' }), null);
    receive({ ch: 'other-channel', type: 'x', target: 'host' }, env.iframe.contentWindow);
    receive('garbage', env.iframe.contentWindow);
    await flush();

    expect(env.sent).toHaveLength(0);
    handle.detach();
  });

  it('stops answering after detach', async () => {
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: ['storage:local'] });
    handle.detach();

    receive(request('storage.get', 'r1', { key: 'k' }), env.iframe.contentWindow);
    await flush();
    expect(storageGetMock).not.toHaveBeenCalled();
    expect(env.sent).toHaveLength(0);
  });
});

describe('uiPluginBridge rate limiting & timeout', () => {
  it('rejects the 21st concurrent request with E_RATE_LIMIT and recovers afterwards', async () => {
    let resolveFirst!: (v: unknown) => void;
    driverExecuteMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          if (!resolveFirst) resolveFirst = resolve;
        }),
    );

    const handle = attachBridge(env.iframe, {
      pluginId: 'p',
      permissions: ['command:invoke'],
      maxInflight: MAX_INFLIGHT_REQUESTS,
    });

    for (let i = 0; i < MAX_INFLIGHT_REQUESTS; i += 1) {
      receive(
        request('command.invoke', `r${i}`, { configId: 'c', command: 'query' }),
        env.iframe.contentWindow,
      );
    }
    await flush();
    expect(env.sent.filter((m) => m.type === 'command.invoke.err')).toHaveLength(0);

    receive(
      request('command.invoke', 'overflow', { configId: 'c', command: 'query' }),
      env.iframe.contentWindow,
    );
    await waitUntil(() => env.sent.some((m) => m.type === 'command.invoke.err'));

    const overflow = env.sent.find((m) => m.reqId === 'overflow');
    expect(overflow?.type).toBe('command.invoke.err');
    expect((overflow?.payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.RATE_LIMIT);
    expect(driverExecuteMock).toHaveBeenCalledTimes(MAX_INFLIGHT_REQUESTS);

    // Capacity frees up once a request settles; new requests are served again.
    resolveFirst({ data: null });
    await flush();
    driverExecuteMock.mockResolvedValue({ data: 42 });
    receive(
      request('command.invoke', 'after', { configId: 'c', command: 'query' }),
      env.iframe.contentWindow,
    );
    await waitUntil(() => env.sent.some((m) => m.reqId === 'after'));
    expect(env.sent.find((m) => m.reqId === 'after')?.type).toBe('command.invoke.ok');
    handle.detach();
  });

  it('limits ui.notify to one shot per 5s cooldown', async () => {
    vi.useFakeTimers();
    notificationInvokeMock.mockResolvedValue(undefined);
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: [] });

    receive(request('ui.notify', 'n1', { title: 'one' }), env.iframe.contentWindow);
    await vi.advanceTimersByTimeAsync(0);
    expect(env.sent[0].type).toBe('ui.notify.ok');

    receive(request('ui.notify', 'n2', { title: 'two' }), env.iframe.contentWindow);
    await vi.advanceTimersByTimeAsync(0);
    const limited = env.sent.find((m) => m.reqId === 'n2');
    expect(limited?.type).toBe('ui.notify.err');
    expect((limited?.payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.RATE_LIMIT);

    await vi.advanceTimersByTimeAsync(5_000);
    receive(request('ui.notify', 'n3', { title: 'three' }), env.iframe.contentWindow);
    await vi.advanceTimersByTimeAsync(0);
    expect(env.sent.find((m) => m.reqId === 'n3')?.type).toBe('ui.notify.ok');
    handle.detach();
  });

  it('answers E_TIMEOUT when the host-internal promise exceeds the deadline', async () => {
    vi.useFakeTimers();
    driverExecuteMock.mockImplementation(() => new Promise(() => undefined));
    const handle = attachBridge(env.iframe, {
      pluginId: 'p',
      permissions: ['command:invoke'],
      timeoutMs: 30_000,
    });

    receive(
      request('command.invoke', 'slow', { configId: 'c', command: 'query' }),
      env.iframe.contentWindow,
    );
    await vi.advanceTimersByTimeAsync(29_999);
    expect(env.sent).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1);

    expect(env.sent[0].reqId).toBe('slow');
    expect(env.sent[0].type).toBe('command.invoke.err');
    expect((env.sent[0].payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.TIMEOUT);
    handle.detach();
  });
});

describe('uiPluginBridge context whitelist', () => {
  const leakyConfig = {
    id: 'conn_1',
    name: 'Prod PG',
    databaseType: 'postgres',
    host: 'db.internal',
    port: 5432,
    username: 'admin',
    password: 'sup3rs3cret',
    sshTunnel: {
      enabled: true,
      host: 'jump',
      port: 22,
      username: 'u',
      authMethod: 'password',
      password: 'pw',
    },
    options: { tlsCa: 'secret' },
  };

  it('getConnections exposes only id/name/dbType even when configs hold credentials', async () => {
    connectionStoreState.current = {
      connections: [leakyConfig],
      connectionsLoaded: true,
      loading: false,
    };

    const handle = attachBridge(env.iframe, {
      pluginId: 'p',
      permissions: ['context:connections'],
    });
    receive(request('context.getConnections', 'r1'), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length > 0);

    expect(env.sent[0].type).toBe('context.getConnections.ok');
    const payload = env.sent[0].payload as { connections: Record<string, unknown>[] };
    expect(payload.connections).toEqual([{ id: 'conn_1', name: 'Prod PG', dbType: 'postgres' }]);
    expect(JSON.stringify(payload)).not.toContain('sup3rs3cret');
    expect(JSON.stringify(payload)).not.toContain('db.internal');
    expect(getConnectionsIpcMock).not.toHaveBeenCalled();
    handle.detach();
  });

  it('falls back to fresh IPC fetch while the store has not loaded yet', async () => {
    getConnectionsIpcMock.mockResolvedValue([leakyConfig]);

    const handle = attachBridge(env.iframe, {
      pluginId: 'p',
      permissions: ['context:connections'],
    });
    receive(request('context.getConnections', 'r1'), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length > 0);

    expect(getConnectionsIpcMock).toHaveBeenCalledTimes(1);
    const payload = env.sent[0].payload as { connections: Record<string, unknown>[] };
    expect(payload.connections).toEqual([{ id: 'conn_1', name: 'Prod PG', dbType: 'postgres' }]);
    handle.detach();
  });

  it('getActiveConnection returns the whitelisted connected config, or null', async () => {
    connectionStoreState.current = {
      connections: [leakyConfig],
      connectionsLoaded: true,
      loading: false,
    };

    const handle = attachBridge(env.iframe, {
      pluginId: 'p',
      permissions: ['context:connections'],
    });

    receive(request('context.getActiveConnection', 'idle'), env.iframe.contentWindow);
    await waitUntil(() => env.sent.some((m) => m.reqId === 'idle'));
    expect(env.sent.find((m) => m.reqId === 'idle')?.payload).toEqual({ connection: null });

    activeConnectionStoreState.current = {
      connections: {
        conn_1: {
          configId: 'conn_1',
          status: 'connected',
          connectionId: 'live_1',
          serverInfo: null,
          currentDatabase: null,
          error: null,
        },
      },
    };
    receive(request('context.getActiveConnection', 'active'), env.iframe.contentWindow);
    await waitUntil(() => env.sent.some((m) => m.reqId === 'active'));
    expect(env.sent.find((m) => m.reqId === 'active')?.payload).toEqual({
      connection: { id: 'conn_1', name: 'Prod PG', dbType: 'postgres' },
    });
    handle.detach();
  });
});

describe('uiPluginBridge command.invoke error mapping', () => {
  it('maps backend "not found" rejections to E_NOT_FOUND', async () => {
    driverExecuteMock.mockRejectedValue('connection not found: cfg-x');
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: ['command:invoke'] });

    receive(
      request('command.invoke', 'miss', { configId: 'cfg-x', command: 'query' }),
      env.iframe.contentWindow,
    );
    await waitUntil(() => env.sent.length > 0);

    expect(env.sent[0].type).toBe('command.invoke.err');
    expect((env.sent[0].payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.NOT_FOUND);
    handle.detach();
  });

  it('rejects malformed payloads with E_BAD_REQUEST', async () => {
    const handle = attachBridge(env.iframe, { pluginId: 'p', permissions: ['command:invoke'] });
    receive(request('command.invoke', 'bad', { command: '' }), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length > 0);

    expect((env.sent[0].payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.BAD_REQUEST);
    handle.detach();
  });
});
