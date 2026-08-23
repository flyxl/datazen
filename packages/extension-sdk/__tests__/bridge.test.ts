import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  BRIDGE_CHANNEL,
  BRIDGE_ERROR,
  REQUEST_TIMEOUT_MS,
  SDK_ERROR,
  EXTENSION_API_VERSION,
  ExtensionError,
  createClient,
} from '../src/bridge';
import type { ExtensionClient } from '../src/bridge';

type Sent = Record<string, unknown>;

const HOST_READY_PAYLOAD = {
  apiVersion: EXTENSION_API_VERSION,
  locale: 'en',
  dark: true,
  tokens: { '--c-accent': '#6366f1', '--dt-number': '#38bdf8' },
};

function makeParentWindow() {
  const sent: Sent[] = [];
  const postMessage = vi.fn((data: unknown) => {
    sent.push(data as Sent);
  });
  const parent = { postMessage } as unknown as Window;
  return { parent, sent };
}

/** Deliver one message as if it came from the given source window. */
function receive(source: unknown, data: unknown): void {
  window.dispatchEvent(new MessageEvent('message', { data, source: source as Window | null }));
}

function hostReady(payload: unknown = HOST_READY_PAYLOAD): Sent {
  return { ch: BRIDGE_CHANNEL, type: 'host.ready', target: 'host', payload };
}

function okResponse(type: string, reqId: string, payload?: unknown): Sent {
  return {
    ch: BRIDGE_CHANNEL,
    type: `${type}.ok`,
    target: 'host',
    reqId,
    ok: true,
    ...(payload === undefined ? {} : { payload }),
  };
}

function errResponse(type: string, reqId: string, code: string, message: string): Sent {
  return {
    ch: BRIDGE_CHANNEL,
    type: `${type}.err`,
    target: 'host',
    reqId,
    ok: false,
    payload: { code, message },
  };
}

/** Create a client and complete the handshake against the fake host window. */
async function handshake(parent: Window): Promise<{ client: ExtensionClient }> {
  const client = createClient({ parentWindow: parent });
  const readyPromise = client.ready();
  receive(parent, hostReady());
  const ctx = await readyPromise;
  expect(ctx).toEqual(HOST_READY_PAYLOAD);
  return { client };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('createClient.ready()', () => {
  it('posts the plugin.ready envelope and resolves the host context', async () => {
    const { parent, sent } = makeParentWindow();
    const client = createClient({ parentWindow: parent });

    const readyPromise = client.ready();
    expect(sent).toEqual([
      {
        ch: BRIDGE_CHANNEL,
        type: 'plugin.ready',
        target: 'host',
        payload: { apiVersion: EXTENSION_API_VERSION },
      },
    ]);

    receive(parent, hostReady());
    await expect(readyPromise).resolves.toEqual(HOST_READY_PAYLOAD);
  });

  it('is idempotent after success and does not re-send plugin.ready', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    await expect(client.ready()).resolves.toEqual(HOST_READY_PAYLOAD);
    expect(sent).toHaveLength(1);
  });

  it('shares one handshake between concurrent ready() callers', async () => {
    const { parent, sent } = makeParentWindow();
    const client = createClient({ parentWindow: parent });

    const [first, second, third] = [client.ready(), client.ready(), client.ready()];
    receive(parent, hostReady());

    await expect(first).resolves.toEqual(HOST_READY_PAYLOAD);
    await expect(second).resolves.toBe(await third);
    expect(sent).toHaveLength(1);
  });

  it('rejects with EXTENSION_VERSION_MISMATCH on an incompatible host and caches the failure', async () => {
    const { parent, sent } = makeParentWindow();
    const client = createClient({ parentWindow: parent });

    const first = client.ready().catch((error: unknown) => error);
    receive(parent, hostReady({ ...HOST_READY_PAYLOAD, apiVersion: 3 }));
    const failure = await first;

    expect(failure).toBeInstanceOf(ExtensionError);
    expect((failure as ExtensionError).code).toBe(SDK_ERROR.VERSION_MISMATCH);

    // Later attempts fail fast instead of re-handshaking with a bad host.
    await expect(client.ready()).rejects.toMatchObject({ code: SDK_ERROR.VERSION_MISMATCH });
    expect(sent).toHaveLength(1);
  });

  it('rejects with E_TIMEOUT when the host never answers the handshake', async () => {
    vi.useFakeTimers();
    const { parent } = makeParentWindow();
    const client = createClient({ parentWindow: parent, timeoutMs: 1_000 });

    const readyPromise = client.ready().catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(999);

    await vi.advanceTimersByTimeAsync(1);
    const failure = await readyPromise;
    expect(failure).toBeInstanceOf(ExtensionError);
    expect((failure as ExtensionError).code).toBe(BRIDGE_ERROR.TIMEOUT);
  });
});

describe('createClient typed api surface', () => {
  it('context.getConnections sends the route type and unwraps connections', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const connections = [
      { id: 'conn_1', name: 'Prod PG', dbType: 'postgres' },
      { id: 'conn_2', name: 'Localite', dbType: 'sqlite' },
    ];
    const pending = client.context.getConnections();

    expect(sent[1]).toMatchObject({
      ch: BRIDGE_CHANNEL,
      type: 'context.getConnections',
      target: 'host',
      reqId: expect.any(String),
    });
    expect('payload' in sent[1]).toBe(false);

    const reqId = sent[1].reqId as string;
    receive(parent, okResponse('context.getConnections', reqId, { connections }));
    await expect(pending).resolves.toEqual(connections);
  });

  it('context.getActiveConnection unwraps connection or null', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const active = client.context.getActiveConnection();
    receive(
      parent,
      okResponse('context.getActiveConnection', sent[1].reqId as string, {
        connection: { id: 'c1', name: 'n', dbType: 'mysql' },
      }),
    );
    await expect(active).resolves.toEqual({ id: 'c1', name: 'n', dbType: 'mysql' });

    const idle = client.context.getActiveConnection();
    receive(
      parent,
      okResponse('context.getActiveConnection', sent[2].reqId as string, {
        connection: null,
      }),
    );
    await expect(idle).resolves.toBeNull();
  });

  it('command.invoke forwards {configId,command,args} and unwraps the result', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const invokeRequest = { configId: 'cfg-9', command: 'query', args: { sql: 'select 1' } };
    const pending = client.command.invoke(invokeRequest);

    expect(sent[1]).toMatchObject({
      type: 'command.invoke',
      payload: invokeRequest,
    });

    const reqId = sent[1].reqId as string;
    receive(parent, okResponse('command.invoke', reqId, { result: { rows: [[1]] } }));
    await expect(pending).resolves.toEqual({ rows: [[1]] });
  });

  it('storage.get/set/remove correlate reqId and unwrap values', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const got = client.storage.get<{ uid: string }>('lastUid');
    expect(sent[1]).toMatchObject({ type: 'storage.get', payload: { key: 'lastUid' } });
    receive(
      parent,
      okResponse('storage.get', sent[1].reqId as string, { value: { uid: '58043285' } }),
    );
    await expect(got).resolves.toEqual({ uid: '58043285' });

    const stored = client.storage.set('lastUid', '42');
    expect(sent[2]).toMatchObject({
      type: 'storage.set',
      payload: { key: 'lastUid', value: '42' },
    });
    receive(parent, okResponse('storage.set', sent[2].reqId as string, {}));
    await expect(stored).resolves.toBeUndefined();

    const removed = client.storage.remove('lastUid');
    expect(sent[3]).toMatchObject({ type: 'storage.remove', payload: { key: 'lastUid' } });
    receive(parent, okResponse('storage.remove', sent[3].reqId as string));
    await expect(removed).resolves.toBeUndefined();

    const missing = client.storage.get('absent');
    receive(parent, okResponse('storage.get', sent[4].reqId as string, { value: null }));
    await expect(missing).resolves.toBeNull();
  });

  it('notify and i18n.getString emit their routes; getString unwraps strings only', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const notified = client.notify({ title: 'Sync done', body: '12 rows' });
    expect(sent[1]).toMatchObject({
      type: 'ui.notify',
      payload: { title: 'Sync done', body: '12 rows' },
    });
    receive(parent, okResponse('ui.notify', sent[1].reqId as string, {}));
    await expect(notified).resolves.toBeUndefined();

    const localized = client.i18n.getString('greeting');
    expect(sent[2]).toMatchObject({ type: 'i18n.getString', payload: { key: 'greeting' } });
    receive(parent, okResponse('i18n.getString', sent[2].reqId as string, { value: 'Hello' }));
    await expect(localized).resolves.toBe('Hello');

    const untranslated = client.i18n.getString('missing');
    receive(parent, okResponse('i18n.getString', sent[3].reqId as string, { value: 42 }));
    await expect(untranslated).resolves.toBeNull();
  });

  it('assigns monotonically increasing reqIds scoped to the client', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const first = client.storage.get('a');
    receive(parent, okResponse('storage.get', sent[1].reqId as string, {}));
    await first;

    const second = client.storage.get('b');
    receive(parent, okResponse('storage.get', sent[2].reqId as string, {}));
    await second;

    const idA = sent[1].reqId as string;
    const idB = sent[2].reqId as string;
    expect(idB.startsWith(idA.split('-')[0])).toBe(true);
    expect(Number(idB.split('-')[1])).toBeGreaterThan(Number(idA.split('-')[1]));
  });
});

describe('createClient error handling', () => {
  it('turns .err responses into ExtensionError with the wire code', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const denied = client.context.getConnections();
    receive(
      parent,
      errResponse(
        'context.getConnections',
        sent[1].reqId as string,
        BRIDGE_ERROR.PERMISSION,
        '"context.getConnections" needs permission "context:connections"',
      ),
    );
    await expect(denied).rejects.toMatchObject({
      code: BRIDGE_ERROR.PERMISSION,
      name: 'ExtensionError',
    });
  });

  it('falls back to E_INTERNAL when an err payload carries no usable code', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const pending = client.storage.get('k').catch((error: unknown) => error);
    receive(parent, {
      ch: BRIDGE_CHANNEL,
      type: 'storage.get.err',
      target: 'host',
      reqId: sent[1].reqId,
      ok: false,
      payload: {},
    });
    const failure = await pending;
    expect((failure as ExtensionError).code).toBe(BRIDGE_ERROR.INTERNAL);
  });

  it('ignores responses from sources other than the parent window', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    let settled = false;
    const pending = client.storage.get('k').then(
      () => {
        settled = true;
      },
      () => {
        settled = true;
      },
    );

    const reqId = sent[1].reqId as string;
    receive({}, okResponse('storage.get', reqId, { value: 'spoofed' }));
    receive(null, okResponse('storage.get', reqId, { value: 'spoofed' }));
    receive(parent, { ch: 'other', type: 'storage.get.ok', target: 'host', reqId, ok: true });
    receive(parent, 'garbage');

    expect(settled).toBe(false);

    receive(parent, okResponse('storage.get', reqId, { value: 'real' }));
    await pending;
    expect(settled).toBe(true);
  });
});

describe('createClient timeouts', () => {
  it('rejects a request after the default 30s deadline and ignores late responses', async () => {
    vi.useFakeTimers();
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    let caught: unknown;
    const pending = client.storage.get('slow').then(
      () => {
        caught = 'resolved';
      },
      (error: unknown) => {
        caught = error;
      },
    );

    await vi.advanceTimersByTimeAsync(REQUEST_TIMEOUT_MS - 1);
    expect(caught).toBeUndefined();

    await vi.advanceTimersByTimeAsync(1);
    expect(caught).toBeInstanceOf(ExtensionError);
    expect((caught as ExtensionError).code).toBe(BRIDGE_ERROR.TIMEOUT);
    expect((caught as ExtensionError).message).toContain('"storage.get"');
    await pending;

    // The entry is gone: a late host answer must not double-settle or throw.
    receive(parent, okResponse('storage.get', sent[1].reqId as string, { value: 'late' }));
  });

  it('honors a custom timeoutMs', async () => {
    vi.useFakeTimers();
    const { parent } = makeParentWindow();
    const client = createClient({ parentWindow: parent, timeoutMs: 50 });
    const readyPromise = client.ready();
    receive(parent, hostReady());
    await readyPromise;

    let caught: unknown;
    const pending = client.notify({ title: 'x' }).catch((error: unknown) => {
      caught = error;
    });
    await vi.advanceTimersByTimeAsync(50);
    await pending;
    expect((caught as ExtensionError).code).toBe(BRIDGE_ERROR.TIMEOUT);
  });
});

describe('createClient.detach()', () => {
  it('aborts in-flight requests and rejects later calls without posting', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    let abort: unknown;
    const inflight = client.notify({ title: 'bye' }).catch((error: unknown) => {
      abort = error;
    });
    expect(sent).toHaveLength(2); // plugin.ready + ui.notify

    client.detach();
    await inflight;
    expect(abort).toBeInstanceOf(ExtensionError);
    expect((abort as ExtensionError).code).toBe(SDK_ERROR.DETACHED);

    await expect(client.context.getConnections()).rejects.toMatchObject({
      code: SDK_ERROR.DETACHED,
    });
    await expect(client.ready()).rejects.toMatchObject({ code: SDK_ERROR.DETACHED });
    expect(sent).toHaveLength(2);
  });

  it('removes the message listener: later host messages have no effect', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);
    client.detach();

    expect(() =>
      receive(parent, okResponse('storage.get', `${sent[0].reqId}-x`, { value: 'zombie' })),
    ).not.toThrow();
    expect(() => receive(parent, hostReady())).not.toThrow();
  });
});
