import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PluginRequestEnvelope } from '../wappBridge';
import { resetWappAssetResolver, WEB_WAPP_ASSET_RESOLVER, configureWappAssetResolver } from '../wappAssetOrigin';

/**
 * WR-F4 桥契约（web 形态）：与桌面形测试 `wappBridge.test.ts` 同构的最小
 * 契约面，在 **web 绑定**下逐条复验 postMessage 协议不变 ——
 *
 * - 同一封套 `{ ch:'datazen-wapp', type, reqId?, target:'host', payload? }`；
 * - 同一应答规则：`{type}.ok` / `{type}.err` 回显 `reqId`；
 * - 同一拒绝语义：deny-by-default 权限门（先于业务）、未知 API `E_NOT_FOUND`、
 *   命令黑名单、通知冷却；
 * - 同一审计口径：`wappCommands.auditLog`（`wapp_audit_log` 等价）。
 *
 * web 形态的差异只在**注入来源**：平台服务是 web 半边（通知走
 * `PlatformServices.showNotification` 的浏览器语义，而非 Tauri 插件载荷）、
 * legacy 轨解析到 web 绑定的 backend client、资产 URL 走真实 origin 解析器。
 * 封套与路由表逐字不动的部分由本套件钉死（两形态同绿）。
 */

const {
  storageGetMock,
  wappExecuteCommandMock,
  getConnectionsIpcMock,
  notificationMock,
  auditLogMock,
  connectionStoreState,
  activeConnectionStoreState,
} = vi.hoisted(() => ({
  storageGetMock: vi.fn(),
  wappExecuteCommandMock: vi.fn(),
  getConnectionsIpcMock: vi.fn(),
  // 端口契约：PlatformServices.showNotification 返回 Promise<void>——
  // 裸 vi.fn() 的 undefined 返回值会让桥的 runHandler 拿不到 thenable。
  notificationMock: vi.fn((_label: string, _input: { title: string; body?: string }) =>
    Promise.resolve(),
  ),
  auditLogMock: vi.fn(),
  connectionStoreState: {
    current: {
      connections: [] as Array<Record<string, unknown>>,
      connectionsLoaded: false,
      loading: false,
    },
  },
  activeConnectionStoreState: { current: { connections: {} } },
}));

import { setBackendClient, setPlatformServices, clearPlatformServices } from '@datazen/backend-client';

// web 形态的 legacy 轨：backend client 绑定到 web transport 替身（HTTP 面的
// 浏览器半边）；桥内 session/storage 的 invoke 与桌面共用同一封套路径。
setBackendClient('web-contract', {
  transport: {
    call: (c: string, p?: unknown) => (p === undefined ? vi.fn()(c) : vi.fn()(c, p)),
  },
} as never);

beforeEach(() => {
  // web 平台服务半边：通知是浏览器语义的端口调用（桌面形测试里是 Tauri
  // 插件载荷 —— 这是两形态仅有的注入差异之一）。
  setPlatformServices({
    showNotification: (input: { title: string; body?: string }) =>
      notificationMock('web-notification', input),
  } as never);
  // WR-F4：web 形态绑定真实 origin 资产解析器（装配根在生产做同样的事）。
  configureWappAssetResolver(WEB_WAPP_ASSET_RESOLVER);
});

afterEach(() => {
  clearPlatformServices();
  resetWappAssetResolver();
});

vi.mock('../../commands/wapps', () => ({
  wappCommands: {
    wappStorageGet: (...args: unknown[]) => storageGetMock(...args),
    wappStorageSet: vi.fn(),
    wappStorageRemove: vi.fn(),
    readWappFile: vi.fn(),
    auditLog: (...args: unknown[]) => auditLogMock(...args),
    executeDriverCommand: (...args: unknown[]) => wappExecuteCommandMock(...args),
  },
}));

vi.mock('../../commands/driver', () => ({
  driverCommands: { execute: (...args: unknown[]) => wappExecuteCommandMock(...args) },
}));

vi.mock('../../commands/connection', () => ({
  connectionCommands: { getConnections: () => getConnectionsIpcMock() },
}));

vi.mock('../../stores/connectionStore', () => ({
  useConnectionStore: { getState: () => connectionStoreState.current },
}));

vi.mock('../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: { getState: () => activeConnectionStoreState.current },
}));

import { THEME_TOKENS } from '../themeTokens';
import { attachBridge, BRIDGE_ERROR, BRIDGE_CHANNEL } from '../wappBridge';

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

/** Simulate a wapp → host message originating from the given source window. */
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

beforeEach(() => {
  vi.clearAllMocks();
  env = makeFakeIframe();
  connectionStoreState.current = { connections: [], connectionsLoaded: false, loading: false };
  activeConnectionStoreState.current = { connections: {} };
});

describe('wappBridge contract, web form (WR-F4)', () => {
  it('answers wapp.ready with the same host.ready handshake envelope (protocol unchanged)', async () => {
    document.documentElement.classList.add('dark');
    const handle = attachBridge(env.iframe, {
      wappId: 'acme.bill-audit',
      permissions: [],
      locale: 'zh-CN',
    });

    receive(request('wapp.ready'), env.iframe.contentWindow);
    await flush();

    expect(env.sent).toHaveLength(1);
    const ready = env.sent[0]!;
    expect(ready.ch).toBe(BRIDGE_CHANNEL);
    expect(ready.type).toBe('host.ready');
    expect(ready.target).toBe('host');
    const payload = ready.payload as Record<string, unknown>;
    expect(payload.apiVersion).toBe(2);
    expect(payload.locale).toBe('zh-CN');
    expect(payload.dark).toBe(true);
    expect(typeof payload.instanceNonce).toBe('string');
    for (const token of THEME_TOKENS) {
      expect(Object.keys(payload.tokens as Record<string, string>)).toContain(token);
    }
    handle.detach();
    document.documentElement.classList.remove('dark');
  });

  it('echoes reqId with a `.ok` suffix on storage.get through the web-bound rail', async () => {
    storageGetMock.mockResolvedValue('stored-on-web');
    const handle = attachBridge(env.iframe, {
      wappId: 'acme.bill-audit',
      permissions: ['storage:local'],
    });

    receive(request('storage.get', 'req-42', { key: 'k' }), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length > 0);

    expect(env.sent).toHaveLength(1);
    const sent = env.sent[0]!;
    expect(sent.type).toBe('storage.get.ok');
    expect(sent.reqId).toBe('req-42');
    expect(sent.ok).toBe(true);
    expect((sent.payload as Record<string, unknown>).value).toBe('stored-on-web');
    expect(storageGetMock).toHaveBeenCalledWith('acme.bill-audit', 'k');
    handle.detach();
  });

  it('denies unrouted and unpermitted APIs before any business logic', async () => {
    const handle = attachBridge(env.iframe, { wappId: 'p', permissions: [] });

    receive(request('context.getConnections', 'r1'), env.iframe.contentWindow);
    receive(request('made.up.api', 'r2'), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length === 2);

    const denied = env.sent.find((s) => s.type === 'context.getConnections.err');
    const unknown = env.sent.find((s) => s.type === 'made.up.api.err');
    expect((denied!.payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.PERMISSION);
    expect(getConnectionsIpcMock).not.toHaveBeenCalled();
    expect((unknown!.payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.NOT_FOUND);
    handle.detach();
  });

  it('keeps the command denylist and the wapp_audit_log trail equivalent on web', async () => {
    connectionStoreState.current = {
      connections: [{ id: 'c1', name: 'Alpha', databaseType: 'postgresql' }],
      connectionsLoaded: true,
      loading: false,
    };
    wappExecuteCommandMock.mockResolvedValue({ rows: [] });
    const handle = attachBridge(env.iframe, {
      wappId: 'acme.bill-audit',
      permissions: ['command:invoke'],
    });

    // 黑名单命令：拒绝且不触达执行面。
    receive(
      request('command.invoke', 'r1', { connectionId: 'c1', command: 'execute' }),
      env.iframe.contentWindow,
    );
    await waitUntil(() => env.sent.length > 0);
    expect((env.sent[0]!.payload as Record<string, unknown>).code).toBe(
      BRIDGE_ERROR.PERMISSION,
    );
    expect(wappExecuteCommandMock).not.toHaveBeenCalled();

    // 白名单命令：`.ok` 应答 + 审计等价（wapp_audit_log 同一行）。
    receive(
      request('command.invoke', 'r2', { connectionId: 'c1', command: 'query' }),
      env.iframe.contentWindow,
    );
    await waitUntil(() => env.sent.length === 2);
    const ok = env.sent[1]!;
    expect(ok.type).toBe('command.invoke.ok');
    expect(ok.reqId).toBe('r2');
    expect(auditLogMock).toHaveBeenCalledWith(
      'acme.bill-audit',
      'command.invoke',
      'query via c1',
    );
    handle.detach();
  });

  it('routes ui.notify through the web platform-services port with the shared cooldown', async () => {
    const handle = attachBridge(env.iframe, { wappId: 'acme.bill-audit', permissions: [] });

    receive(request('ui.notify', 'n1', { title: 'hello', body: 'world' }), env.iframe.contentWindow);
    receive(request('ui.notify', 'n2', { title: 'again' }), env.iframe.contentWindow);
    await waitUntil(() => env.sent.length === 2);

    expect(notificationMock).toHaveBeenCalledTimes(1);
    expect(notificationMock).toHaveBeenCalledWith('web-notification', {
      title: 'hello',
      body: 'world',
    });
    // 两条回复各自异步投递，到达顺序有竞态（ok 走端口 promise 微任务，
    // 限速走同步冷却判定）——按 reqId 取信封，不对数组下标作序断言。
    const ok = env.sent.find((m) => m.reqId === 'n1')!;
    expect(ok.type).toBe('ui.notify.ok');
    const limited = env.sent.find((m) => m.reqId === 'n2')!;
    expect((limited.payload as Record<string, unknown>).code).toBe(BRIDGE_ERROR.RATE_LIMIT);
    handle.detach();
  });
});
