/**
 * F6 RPC bridge — security-focused regression suite (test agent).
 *
 * Scope (PRD §3 / §4.4):
 * - credential whitelist proof for context APIs (constructive mapping)
 * - stack-trace / audit-log non-leakage
 * - permission gate vs malformed envelopes, case variants and
 *   prototype-pollution keys (`__proto__`, `constructor`, …)
 * - cross-iframe source isolation, post-detach silence
 * - rate-limit quota release semantics (completion / timeout / denials)
 */
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
  consoleInfoSpy,
} = vi.hoisted(() => ({
  storageGetMock: vi.fn(),
  storageSetMock: vi.fn(),
  storageRemoveMock: vi.fn(),
  driverExecuteMock: vi.fn(),
  getConnectionsIpcMock: vi.fn(),
  connectionStoreState: {
    current: { connections: [] as unknown[], connectionsLoaded: false, loading: false },
  },
  activeConnectionStoreState: { current: { connections: {} } },
  notificationInvokeMock: vi.fn(),
  consoleInfoSpy: vi.fn(),
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

vi.spyOn(console, 'info').mockImplementation((...args: unknown[]) => consoleInfoSpy(...args));

import { THEME_TOKENS, THEME_SNAPSHOT_VERSION } from '../themeTokens';
import {
  attachBridge,
  BRIDGE_ERROR,
  BRIDGE_CHANNEL,
  type UiPluginBridgeHandle,
} from '../uiPluginBridge';

type SentEnvelope = Record<string, unknown>;

interface FakeFrame {
  iframe: HTMLIFrameElement;
  sent: SentEnvelope[];
}

function makeFakeIframe(): FakeFrame {
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

function receive(data: unknown, source: unknown): void {
  window.dispatchEvent(new MessageEvent('message', { data, source: source as Window | null }));
}

const flush = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 0));

async function waitUntil(cond: () => boolean): Promise<void> {
  for (let i = 0; i < 200 && !cond(); i += 1) {
    await flush();
  }
}

let frame: FakeFrame;
let handle: UiPluginBridgeHandle | null = null;

function request(type: string, reqId?: string, payload?: unknown): PluginRequestEnvelope {
  return { ch: BRIDGE_CHANNEL, type, target: 'host', ...(reqId ? { reqId } : {}), payload };
}

afterEach(() => {
  handle?.detach();
  handle = null;
});

/** ConnectionConfig-shaped fixture carrying every credential-bearing field. */
const LEAKY_CONFIG = {
  id: 'conn_9',
  name: 'Vault PG',
  databaseType: 'postgres',
  host: 'vault.internal',
  port: 5432,
  database: 'secret_db',
  username: 'admin',
  password: 'sup3rs3cret-pw',
  sslMode: 'require',
  sshTunnel: {
    enabled: true,
    host: 'jump.internal',
    port: 22,
    username: 'tunnel-user',
    authMethod: 'password',
    password: 'tunnel-pw',
    privateKeyPath: '/home/u/id_rsa',
    passphrase: 'key-passphrase',
    jump: { enabled: false, host: '', port: 0, username: '', authMethod: 'agent' },
  },
  options: { tlsCa: 'ca-secret', topology: 'internal-only' },
};

const SECRET_MARKERS = [
  'vault.internal',
  'jump.internal',
  '5432',
  'admin',
  'sup3rs3cret-pw',
  'tunnel-pw',
  'id_rsa',
  'key-passphrase',
  'tlsCa',
  'ca-secret',
];

function expectNoCredentialLeak(sentEnvelopes: SentEnvelope[]): void {
  const wire = JSON.stringify(sentEnvelopes);
  for (const marker of SECRET_MARKERS) {
    expect(wire).not.toContain(marker);
  }
}

beforeEach(() => {
  vi.clearAllMocks();
  frame = makeFakeIframe();
  connectionStoreState.current = { connections: [], connectionsLoaded: false, loading: false };
  activeConnectionStoreState.current = { connections: {} };
});

afterEach(() => {
  vi.useRealTimers();
});

describe('F6 security: credential whitelisting', () => {
  it('getConnections (IPC fallback path) emits constructively whitelisted summaries', async () => {
    getConnectionsIpcMock.mockResolvedValue([LEAKY_CONFIG]);
    handle = attachBridge(frame.iframe, {
      pluginId: 'p',
      permissions: ['context:connections'],
    });

    receive(request('context.getConnections', 'c1'), frame.iframe.contentWindow);
    await waitUntil(() => frame.sent.length > 0);

    expect(frame.sent[0].type).toBe('context.getConnections.ok');
    const connections = (frame.sent[0].payload as { connections: Record<string, unknown>[] })
      .connections;
    // Exactly three own keys proves construction, not delete-sanitization.
    expect(connections).toHaveLength(1);
    expect(Object.keys(connections[0]).sort()).toEqual(['dbType', 'id', 'name']);
    expect(connections[0]).toEqual({ id: 'conn_9', name: 'Vault PG', dbType: 'postgres' });
    expectNoCredentialLeak(frame.sent);
  });

  it('getActiveConnection emits the same whitelist while a connection is live', async () => {
    connectionStoreState.current = {
      connections: [LEAKY_CONFIG],
      connectionsLoaded: true,
      loading: false,
    };
    activeConnectionStoreState.current = {
      connections: {
        conn_9: { configId: 'conn_9', status: 'connected' },
      },
    };
    handle = attachBridge(frame.iframe, {
      pluginId: 'p',
      permissions: ['context:connections'],
    });

    receive(request('context.getActiveConnection', 'a1'), frame.iframe.contentWindow);
    await waitUntil(() => frame.sent.length > 0);

    const payload = frame.sent[0].payload as { connection: Record<string, unknown> };
    expect(Object.keys(payload.connection).sort()).toEqual(['dbType', 'id', 'name']);
    expectNoCredentialLeak(frame.sent);
  });

  it('INTERNAL error responses carry messages only — never Rust stacks', async () => {
    const bomb = new Error(`query failed after ${'x'.repeat(800)} chars`);
    (bomb as Error & { stack: string }).stack =
      `Error: query failed\n    at execute_driver_command (rust${'y'.repeat(
        400,
      )}) secret-frame-token`;
    driverExecuteMock.mockRejectedValue(bomb);
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: ['command:invoke'] });

    receive(
      request('command.invoke', 'boom', { configId: 'cfg', command: 'query' }),
      frame.iframe.contentWindow,
    );
    await waitUntil(() => frame.sent.length > 0);

    const payload = frame.sent[0].payload as { code: string; message: string };
    expect(payload.code).toBe(BRIDGE_ERROR.INTERNAL);
    expect(JSON.stringify(frame.sent)).not.toContain('secret-frame-token');
    expect(payload.message.length).toBeLessThanOrEqual(501); // 500 chars + ellipsis
    expect(payload.message.endsWith('…')).toBe(true);
  });

  it('audit log prefixes [ui-plugin:{id}] and never logs argument contents', async () => {
    driverExecuteMock.mockResolvedValue({ data: null });
    handle = attachBridge(frame.iframe, {
      pluginId: 'acme.bill-audit',
      permissions: ['command:invoke'],
    });

    receive(
      request('command.invoke', 'log1', {
        configId: 'cfg',
        command: 'query',
        args: { sql: "select * from users where pw = 'top-secret-value'" },
      }),
      frame.iframe.contentWindow,
    );
    await waitUntil(() => frame.sent.length > 0);

    const logged = consoleInfoSpy.mock.calls.map((c) => c.join(' ')).join('\n');
    expect(logged).toContain('[ui-plugin:acme.bill-audit]');
    expect(logged).toContain('command.invoke');
    expect(logged).not.toContain('top-secret-value');
  });
});

describe('F6 security: permission gate vs malformed routing', () => {
  function attachAll(): UiPluginBridgeHandle {
    return attachBridge(frame.iframe, {
      pluginId: 'p',
      permissions: ['context:connections', 'command:invoke', 'storage:local'],
    });
  }

  it('answers E_NOT_FOUND for case variants of routed types (routing is case-sensitive)', async () => {
    handle = attachAll();
    for (const [i, type] of [
      'CONTEXT.GETCONNECTIONS',
      'context.getconnections',
      'Context.Get_Connections',
      ' context.getConnections',
    ].entries()) {
      receive(request(type, `cv${i}`), frame.iframe.contentWindow);
    }
    await waitUntil(() => frame.sent.length === 4);

    for (const sent of frame.sent) {
      expect((sent.payload as { code: string }).code).toBe(BRIDGE_ERROR.NOT_FOUND);
    }
    expect(getConnectionsIpcMock).not.toHaveBeenCalled();
  });

  it.each([['__proto__'], ['constructor'], ['hasOwnProperty'], ['toString'], ['valueOf']])(
    'denies prototype-chain api type "%s" without reaching any handler',
    async (type) => {
      driverExecuteMock.mockResolvedValue({ data: 'MUST_NOT_APPEAR' });
      storageGetMock.mockResolvedValue('MUST_NOT_APPEAR');
      handle = attachAll();

      receive(
        request(type, `pp-${type}`, { configId: 'c', command: 'query' }),
        frame.iframe.contentWindow,
      );
      await waitUntil(() =>
        frame.sent.some((m) => m.reqId === `pp-${type}` && (m.payload as { code?: string }).code),
      );

      const sent = frame.sent.find((m) => m.reqId === `pp-${type}`);
      // Security property: never ok, never reaches business logic.
      expect(String(sent?.type)).not.toMatch(/\.ok$/);
      expect(JSON.stringify(frame.sent)).not.toContain('MUST_NOT_APPEAR');
      expect(driverExecuteMock).not.toHaveBeenCalled();
      expect(storageGetMock).not.toHaveBeenCalled();
      // Current behavior pins these to E_PERMISSION (prototype members resolve
      // through API_ROUTES' prototype chain); spec-intended code would be
      // E_NOT_FOUND — deviation recorded as BUG-F6-02.
      expect((sent!.payload as { code: string }).code).toBe(BRIDGE_ERROR.PERMISSION);
      // Prototype chain itself remains intact.
      expect((Object.prototype as unknown as Record<string, unknown>).granted).toBeUndefined();
    },
  );

  it('ignores envelopes whose target is not exactly "host"', async () => {
    handle = attachAll();
    receive(
      { ch: BRIDGE_CHANNEL, type: 'storage.get', reqId: 't1', target: 'Host' },
      frame.iframe.contentWindow,
    );
    receive(
      { ch: BRIDGE_CHANNEL, type: 'storage.get', reqId: 't2', target: 'plugin' },
      frame.iframe.contentWindow,
    );
    receive({ ch: BRIDGE_CHANNEL, type: 'storage.get', reqId: 't3' }, frame.iframe.contentWindow);
    receive(
      { ch: 'ui-plugin ', type: 'storage.get', reqId: 't4', target: 'host' },
      frame.iframe.contentWindow,
    );
    await flush();

    expect(frame.sent).toHaveLength(0);
    expect(storageGetMock).not.toHaveBeenCalled();
  });

  it('never answers requests lacking a reqId (nothing to correlate)', async () => {
    storageGetMock.mockResolvedValue('x');
    handle = attachAll();
    receive(request('storage.get'), frame.iframe.contentWindow);
    await flush();

    expect(frame.sent).toHaveLength(0);
    expect(storageGetMock).not.toHaveBeenCalled();
  });
});

describe('F6 security: malformed command.invoke payloads', () => {
  beforeEach(() => {
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: ['command:invoke'] });
  });

  afterEach(() => {
    handle.detach();
  });

  const invoke = (reqId: string, payload: unknown) => {
    receive(request('command.invoke', reqId, payload), frame.iframe.contentWindow);
  };

  async function expectBadRequest(reqId: string): Promise<void> {
    await waitUntil(() => frame.sent.some((m) => m.reqId === reqId));
    const sent = frame.sent.find((m) => m.reqId === reqId)!;
    expect(sent.type).toBe('command.invoke.err');
    expect((sent.payload as { code: string }).code).toBe(BRIDGE_ERROR.BAD_REQUEST);
    expect(driverExecuteMock).not.toHaveBeenCalled();
  }

  it('rejects a missing payload entirely', async () => {
    invoke('bad0', undefined);
    await expectBadRequest('bad0');
  });

  it('rejects missing / non-string / numeric configId', async () => {
    invoke('bad1', { command: 'query' });
    invoke('bad2', { configId: 123, command: 'query' });
    invoke('bad3', { configId: '', command: 'query' });
    await expectBadRequest('bad1');
    await expectBadRequest('bad2');
    await expectBadRequest('bad3');
  });

  it('rejects primitive (non-object) args', async () => {
    invoke('bad4', { configId: 'c', command: 'query', args: 'drop table users' });
    invoke('bad5', { configId: 'c', command: 'query', args: 42 });
    invoke('bad6', { configId: 'c', command: 'query', args: true });
    await expectBadRequest('bad4');
    await expectBadRequest('bad5');
    await expectBadRequest('bad6');
  });

  it('treats args:null as an empty input object (benign, pinned behavior)', async () => {
    driverExecuteMock.mockResolvedValue({ data: 'ok' });
    invoke('nullargs', { configId: 'c', command: 'query', args: null });
    await waitUntil(() => frame.sent.some((m) => m.reqId === 'nullargs'));

    expect(driverExecuteMock).toHaveBeenCalledWith({
      connectionId: 'c',
      command: 'query',
      input: {},
    });
    expect(frame.sent.find((m) => m.reqId === 'nullargs')?.type).toBe('command.invoke.ok');
  });

  it('forwards array args verbatim without treating them as key/value maps', async () => {
    driverExecuteMock.mockResolvedValue({ data: 'ok' });
    invoke('arrargs', { configId: 'c', command: 'query', args: ['a', 'b'] });
    await waitUntil(() => frame.sent.some((m) => m.reqId === 'arrargs'));
    expect(driverExecuteMock).toHaveBeenCalledWith({
      connectionId: 'c',
      command: 'query',
      input: ['a', 'b'],
    });
  });
});

describe('F6 security: prototype pollution containment', () => {
  afterEach(() => {
    handle?.detach();
    // Hard guarantee the suite leaves the realm unharmed.
    expect((Object.prototype as unknown as Record<string, unknown>).polluted).toBeUndefined();
  });

  it('passes polluted-key args verbatim to IPC without merging them anywhere', async () => {
    driverExecuteMock.mockResolvedValue({ data: null });
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: ['command:invoke'] });

    // JSON.parse gives `__proto__` own-property semantics (postMessage parity).
    const evilArgs = JSON.parse(
      '{"__proto__":{"polluted":"pwned"},"constructor":{"prototype":{"polluted2":"yes"}},"sql":"select 1"}',
    );
    receive(
      request('command.invoke', 'evil1', { configId: 'c', command: 'query', args: evilArgs }),
      frame.iframe.contentWindow,
    );
    await flush();

    // Forwarded untouched (own properties preserved)…
    expect(driverExecuteMock).toHaveBeenCalledTimes(1);
    const forwarded = driverExecuteMock.mock.calls[0][0] as { input: Record<string, unknown> };
    expect(JSON.stringify(forwarded.input)).toContain('select 1');
    // …and Object.prototype was never mutated.
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    expect(({} as Record<string, unknown>).polluted2).toBeUndefined();
  });

  it('passes polluted-key storage values verbatim', async () => {
    storageSetMock.mockResolvedValue(undefined);
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: ['storage:local'] });

    const evilValue = JSON.parse('{"__proto__":{"polluted":"pwned"}}');
    receive(
      request('storage.set', 'evil2', { key: 'k', value: evilValue }),
      frame.iframe.contentWindow,
    );
    await waitUntil(() => frame.sent.length > 0);

    expect(storageSetMock).toHaveBeenCalledWith('p', 'k', evilValue);
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });

  it('does not let a polluted reqId tamper with response envelopes', async () => {
    storageGetMock.mockResolvedValue('v');
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: ['storage:local'] });

    receive(request('storage.get', '__proto__', { key: 'k' }), frame.iframe.contentWindow);
    await waitUntil(() => frame.sent.length > 0);

    expect(frame.sent[0].reqId).toBe('__proto__');
    expect(frame.sent[0].type).toBe('storage.get.ok');
    expect((frame.sent[0] as { ok?: boolean }).ok).toBe(true);
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });
});

describe('F6 security: storage & notify payload validation', () => {
  it('rejects missing / empty / non-string storage keys with E_BAD_REQUEST', async () => {
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: ['storage:local'] });
    const types = ['storage.get', 'storage.set', 'storage.remove'];
    let n = 0;
    for (const type of types) {
      receive(request(type, `sk${n++}`, {}), frame.iframe.contentWindow); // missing key
      receive(request(type, `sk${n++}`, { key: '' }), frame.iframe.contentWindow); // empty
      receive(request(type, `sk${n++}`, { key: 42 }), frame.iframe.contentWindow); // number
    }
    await waitUntil(() => frame.sent.length === 9);

    for (const sent of frame.sent) {
      expect(sent.type).toMatch(/\.err$/);
      expect((sent.payload as { code: string }).code).toBe(BRIDGE_ERROR.BAD_REQUEST);
    }
    expect(storageGetMock).not.toHaveBeenCalled();
    expect(storageSetMock).not.toHaveBeenCalled();
    expect(storageRemoveMock).not.toHaveBeenCalled();
    handle.detach();
  });

  it('rejects ui.notify without a title or with non-string body before invoking', async () => {
    notificationInvokeMock.mockResolvedValue(undefined);
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: [] });

    receive(request('ui.notify', 'n1', {}), frame.iframe.contentWindow);
    receive(request('ui.notify', 'n2', { title: '' }), frame.iframe.contentWindow);
    receive(request('ui.notify', 'n3', { title: 'Hi', body: 42 }), frame.iframe.contentWindow);
    await waitUntil(() => frame.sent.length === 3);

    for (const sent of frame.sent) {
      expect((sent.payload as { code: string }).code).toBe(BRIDGE_ERROR.BAD_REQUEST);
    }
    expect(notificationInvokeMock).not.toHaveBeenCalled();
    handle.detach();
  });
});

describe('F6 security: cross-iframe source isolation', () => {
  it('routes messages strictly by event.source — sibling frames cannot cross-talk', async () => {
    const a = makeFakeIframe();
    const b = makeFakeIframe();
    storageGetMock.mockImplementation((_pluginId: string, key: string) =>
      Promise.resolve(key === 'from-a' ? 'A-STORE' : 'B-STORE'),
    );
    const handleA = attachBridge(a.iframe, {
      pluginId: 'plugin.a',
      permissions: ['storage:local'],
    });
    const handleB = attachBridge(b.iframe, {
      pluginId: 'plugin.b',
      permissions: ['storage:local'],
    });

    receive(request('storage.get', 'q-b', { key: 'from-b' }), b.iframe.contentWindow);
    await waitUntil(() => b.sent.length > 0);

    // Only B's bridge answered; A stayed silent.
    expect(a.sent).toHaveLength(0);
    expect(b.sent[0].type).toBe('storage.get.ok');
    expect((b.sent[0].payload as { value: string }).value).toBe('B-STORE');
    expect(storageGetMock).toHaveBeenLastCalledWith('plugin.b', 'from-b');

    receive(request('storage.get', 'q-a', { key: 'from-a' }), a.iframe.contentWindow);
    await waitUntil(() => a.sent.length > 0);
    expect(storageGetMock).toHaveBeenLastCalledWith('plugin.a', 'from-a');
    expect(a.sent.filter((m) => m.reqId === 'q-a')).toHaveLength(1);
    expect(b.sent.filter((m) => m.reqId === 'q-a')).toHaveLength(0);

    handleA.detach();
    handleB.detach();
  });

  it('stays fully silent after detach — requests and handshake alike', async () => {
    storageGetMock.mockResolvedValue('late');
    const handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: ['storage:local'] });
    handle.detach();

    receive(request('plugin.ready'), frame.iframe.contentWindow);
    receive(request('storage.get', 'late1', { key: 'k' }), frame.iframe.contentWindow);
    await flush();

    expect(frame.sent).toHaveLength(0); // no host.ready, no late response
    expect(storageGetMock).not.toHaveBeenCalled();
  });
});

describe('F6 security: rate-limit quota lifecycle', () => {
  it('frees the slot as soon as a request settles (completion releases quota)', async () => {
    let releaseA!: (v: unknown) => void;
    let releaseD!: (v: unknown) => void;
    storageGetMock
      .mockImplementationOnce(() => new Promise((resolve) => (releaseA = resolve)))
      .mockImplementationOnce(() => new Promise(() => undefined)) // h2 hangs on purpose
      .mockImplementationOnce(() => new Promise((resolve) => (releaseD = resolve)))
      .mockImplementation(() => Promise.resolve('unexpected-extra-call'));

    handle = attachBridge(frame.iframe, {
      pluginId: 'p',
      permissions: ['storage:local'],
      maxInflight: 2,
    });

    receive(request('storage.get', 'h1', { key: 'a' }), frame.iframe.contentWindow);
    receive(request('storage.get', 'h2', { key: 'b' }), frame.iframe.contentWindow);
    await flush();
    expect(frame.sent).toHaveLength(0); // both occupy the cap

    receive(request('storage.get', 'h3', { key: 'c' }), frame.iframe.contentWindow);
    await waitUntil(() => frame.sent.some((m) => m.reqId === 'h3'));
    expect((frame.sent.find((m) => m.reqId === 'h3')!.payload as { code: string }).code).toBe(
      BRIDGE_ERROR.RATE_LIMIT,
    );

    // Settling ONE request restores room for exactly one more.
    releaseA('a-done');
    await flush();
    receive(request('storage.get', 'h4', { key: 'd' }), frame.iframe.contentWindow);
    await waitUntil(() =>
      frame.sent.some((m) => m.reqId === 'h4' || (m.payload as { code?: string }).code),
    );
    expect(storageGetMock).toHaveBeenCalledTimes(3); // h4 was accepted, not limited
    expect(frame.sent.find((m) => m.reqId === 'h3' || m.reqId === 'h4')).toBeDefined();

    // Cap saturated again (h2 + h4 hang) → the next request is still limited.
    receive(request('storage.get', 'h5', { key: 'e' }), frame.iframe.contentWindow);
    await waitUntil(() => frame.sent.length >= 3);
    expect(frame.sent[2].reqId).toBe('h5');
    expect((frame.sent[2].payload as { code: string }).code).toBe(BRIDGE_ERROR.RATE_LIMIT);

    releaseD('d-done');
  });

  it('timed-out requests release capacity too', async () => {
    vi.useFakeTimers();
    storageGetMock.mockImplementation(() => new Promise(() => undefined)); // hangs forever
    const handle = attachBridge(frame.iframe, {
      pluginId: 'p',
      permissions: ['storage:local'],
      timeoutMs: 1_000,
      maxInflight: 1,
    });

    receive(request('storage.get', 'slow', { key: 'a' }), frame.iframe.contentWindow);
    receive(request('storage.get', 'blocked', { key: 'b' }), frame.iframe.contentWindow);
    await vi.advanceTimersByTimeAsync(1_000);

    expect((frame.sent.find((m) => m.reqId === 'slow')!.payload as { code: string }).code).toBe(
      BRIDGE_ERROR.TIMEOUT,
    );
    expect(frame.sent.find((m) => m.reqId === 'blocked')).toBeDefined(); // rate-limited meanwhile

    // After the timeout the hanging handler no longer holds the slot.
    storageGetMock.mockResolvedValue('recovered');
    receive(request('storage.get', 'fresh', { key: 'c' }), frame.iframe.contentWindow);
    await vi.advanceTimersByTimeAsync(0);
    expect(frame.sent.find((m) => m.reqId === 'fresh')?.type).toBe('storage.get.ok');

    handle.detach();
  });

  it('permission-denied floods never consume concurrency quota', async () => {
    handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: [] });
    for (let i = 0; i < 30; i += 1) {
      receive(request('context.getConnections', `deny${i}`), frame.iframe.contentWindow);
    }
    await waitUntil(() => frame.sent.length === 30);

    for (const sent of frame.sent) {
      expect((sent.payload as { code: string }).code).toBe(BRIDGE_ERROR.PERMISSION);
    }
    expect(
      frame.sent.some((m) => (m.payload as { code?: string }).code === BRIDGE_ERROR.RATE_LIMIT),
    ).toBe(false);
  });
});

describe('F6 security: manual theme snapshot reflects live theme state', () => {
  it('pushThemeSnapshot mirrors the current dark flag and token contract on every call', () => {
    document.documentElement.classList.remove('dark');
    const handle = attachBridge(frame.iframe, { pluginId: 'p', permissions: [] });

    handle.pushThemeSnapshot();
    expect(frame.sent[0]).toMatchObject({
      ch: BRIDGE_CHANNEL,
      type: 'theme.apply',
      target: 'host',
    });
    let snapshot = frame.sent[0].payload as {
      v: number;
      dark: boolean;
      tokens: Record<string, string>;
    };
    expect(snapshot.v).toBe(THEME_SNAPSHOT_VERSION);
    expect(snapshot.dark).toBe(false);

    document.documentElement.classList.add('dark');
    handle.pushThemeSnapshot();
    snapshot = frame.sent[1].payload as typeof snapshot;
    expect(snapshot.dark).toBe(true);
    expect(Object.keys(snapshot.tokens).sort()).toEqual([...THEME_TOKENS].sort());

    handle.detach();
    document.documentElement.classList.remove('dark');
  });
});
