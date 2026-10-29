import { afterEach, describe, expect, it, vi } from 'vitest';
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
import { setBackendClient } from '@datazen/backend-client';
// Form-split R6: wapp_session rides the legacy invoke rail — bind the mock
// transport so the wire assertions keep reading the same invokeMock.
setBackendClient('test', {
  transport: {
    call: (c: string, p?: unknown) => (p === undefined ? invokeMock(c) : invokeMock(c, p)),
  },
} as never);
vi.mock('../../commands/wapps', () => ({ wappCommands: {} }));
vi.mock('../../commands/connection', () => ({ connectionCommands: {} }));
vi.mock('../../stores/connectionStore', () => ({ useConnectionStore: {} }));
vi.mock('../../stores/activeConnectionStore', () => ({ useActiveConnectionStore: {} }));
import type { WappPermission } from '../../types/wapp';
import { attachBridge, BRIDGE_CHANNEL, type WappBridgeHandle } from '../wappBridge';
const handles: WappBridgeHandle[] = [];
afterEach(() => {
  handles.splice(0).forEach((h) => h.detach());
  vi.clearAllMocks();
});
function frame(permissions: WappPermission[] = ['command:invoke', 'session:stateful']) {
  const iframe = document.createElement('iframe');
  const sent: Record<string, unknown>[] = [];
  const source = { postMessage: vi.fn((data: Record<string, unknown>) => sent.push(data)) };
  Object.defineProperty(iframe, 'contentWindow', { get: () => source });
  const handle = attachBridge(iframe, { wappId: 'test.sessions', permissions });
  handles.push(handle);
  function send(type: string, reqId?: string, payload?: unknown, instanceNonce?: string) {
    window.dispatchEvent(
      new MessageEvent('message', {
        source: source as unknown as Window,
        data: { ch: BRIDGE_CHANNEL, target: 'host', type, reqId, payload, instanceNonce },
      }),
    );
  }
  send('wapp.ready');
  const nonce = (sent[0].payload as { instanceNonce: string }).instanceNonce;
  return { iframe, sent, source, handle, send, nonce };
}
async function settle() {
  for (let i = 0; i < 20; i++) await Promise.resolve();
}
describe('Wapp session instance lifecycle', () => {
  it('fails closed without both manifest permissions and ignores another frame source', async () => {
    for (const permissions of [
      [],
      ['session:stateful'],
      ['command:invoke'],
    ] satisfies WappPermission[][]) {
      const f = frame(permissions);
      f.send('session.open', 'denied', { connectionId: 'profile', database: 'app' }, f.nonce);
      await settle();
      expect(f.sent.at(-1)).toMatchObject({ ok: false, payload: { code: 'E_PERMISSION' } });
    }
    expect(invokeMock).not.toHaveBeenCalled();
    const f = frame();
    window.dispatchEvent(
      new MessageEvent('message', {
        source: window,
        data: {
          ch: BRIDGE_CHANNEL,
          type: 'session.open',
          target: 'host',
          reqId: 'foreign',
          instanceNonce: f.nonce,
          payload: { connectionId: 'profile', database: 'app' },
        },
      }),
    );
    await settle();
    expect(invokeMock).not.toHaveBeenCalled();
  });
  it('requires nonce, deduplicates open, rejects changed replay, and ends backend instance on detach', async () => {
    invokeMock.mockImplementation(
      async (_command: string, { request }: { request: { action: string } }) =>
        request.action === 'register' ? { instanceId: 'backend-frame-a' } : { reference: 'opaque' },
    );
    const f = frame();
    const target = { connectionId: 'profile', database: 'app' };
    f.send('session.open', 'bad', target, 'old');
    await settle();
    expect(invokeMock).not.toHaveBeenCalled();
    f.send('session.open', 'open', target, f.nonce);
    f.send('session.open', 'open', target, f.nonce);
    await settle();
    expect(invokeMock.mock.calls.filter(([, arg]) => arg.request.action === 'open')).toHaveLength(
      1,
    );
    f.send('session.open', 'open', { ...target, database: 'other' }, f.nonce);
    await settle();
    expect(f.sent.at(-1)).toMatchObject({ ok: false, payload: { code: 'E_BAD_REQUEST' } });
    f.handle.detach();
    await settle();
    expect(invokeMock).toHaveBeenCalledWith('wapp_session', {
      request: { action: 'end', instanceId: 'backend-frame-a' },
    });
  });
  it('rotates on iframe self-navigation, drops old response, and prevents old nonce opening a new session', async () => {
    let resolveOpen: ((value: unknown) => void) | undefined;
    invokeMock.mockImplementation(
      (_command: string, { request }: { request: { action: string } }) =>
        request.action === 'register'
          ? Promise.resolve({ instanceId: 'backend-old' })
          : request.action === 'open'
            ? new Promise((resolve) => {
                resolveOpen = resolve;
              })
            : Promise.resolve({}),
    );
    const f = frame();
    f.send('session.open', 'old-open', { connectionId: 'profile', database: 'app' }, f.nonce);
    await settle();
    f.iframe.dispatchEvent(new Event('load'));
    resolveOpen?.({ reference: 'old-ref' });
    await settle();
    expect(f.sent.some((m) => m.reqId === 'old-open')).toBe(false);
    f.send(
      'session.use',
      'stale',
      { reference: 'old-ref', command: 'query', input: { sql: 'SELECT 1' } },
      f.nonce,
    );
    await settle();
    expect(f.sent.at(-1)).toMatchObject({ ok: false, payload: { code: 'E_PERMISSION' } });
    expect(invokeMock).toHaveBeenCalledWith('wapp_session', {
      request: { action: 'end', instanceId: 'backend-old' },
    });
  });
});
