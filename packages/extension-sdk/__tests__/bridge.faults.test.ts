/**
 * F8 test-agent additions: malformed host-response tolerance, value
 * serialization fidelity, argument pass-through and concurrent reqId routing.
 *
 * C-03/C-04 lock in the BUG-F8-01 fix: malformed `.err` frames whose
 * `payload` is absent or null must settle the pending request as
 * ExtensionError(E_INTERNAL) with zero uncaught page errors (see
 * docs/prd/ui-plugins-progress.md Bug 跟踪).
 */
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
  dark: false,
  tokens: {},
};

function makeParentWindow() {
  const sent: Sent[] = [];
  const postMessage = vi.fn((data: unknown) => {
    sent.push(data as Sent);
  });
  return { parent: { postMessage } as unknown as Window, sent };
}

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

async function handshake(parent: Window): Promise<{ client: ExtensionClient }> {
  const client = createClient({ parentWindow: parent });
  const readyPromise = client.ready();
  receive(parent, hostReady());
  await readyPromise;
  return { client };
}

/**
 * Capture uncaught page errors during a synchronous dispatch window:
 * both `window.addEventListener('error')` events and `window.onerror`
 * assignments. `restore()` must always run (use try/finally).
 */
function trackUncaught(): { events: unknown[]; restore(): void } {
  const events: unknown[] = [];
  const onError = (event: ErrorEvent): void => {
    events.push(event.error ?? event.message);
  };
  const prevOnerror = window.onerror;
  window.addEventListener('error', onError);
  window.onerror = () => {
    events.push('window.onerror');
  };
  return {
    events,
    restore() {
      window.removeEventListener('error', onError);
      window.onerror = prevOnerror;
    },
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('malformed host responses (F8 tolerance matrix)', () => {
  it('C-01 ok envelope without payload field resolves tolerantly on every typed api', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const got = client.storage.get('k');
    receive(parent, okResponse('storage.get', sent[1].reqId as string));
    await expect(got).resolves.toBeNull();

    const conns = client.context.getConnections();
    receive(parent, okResponse('context.getConnections', sent[2].reqId as string));
    await expect(conns).resolves.toEqual([]);

    const active = client.context.getActiveConnection();
    receive(parent, okResponse('context.getActiveConnection', sent[3].reqId as string));
    await expect(active).resolves.toBeNull();

    const invoked = client.command.invoke({ configId: 'c', command: 'query' });
    receive(parent, okResponse('command.invoke', sent[4].reqId as string));
    await expect(invoked).resolves.toBeUndefined();

    const localized = client.i18n.getString('k');
    receive(parent, okResponse('i18n.getString', sent[5].reqId as string));
    await expect(localized).resolves.toBeNull();
  });

  it('C-02 non-JSON garbage data (string / number / null / array) is ignored without settling', async () => {
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

    for (const garbage of ['not-json', 42, null, [okResponse('storage.get', 'x')], undefined]) {
      receive(parent, garbage);
    }
    expect(settled).toBe(false);

    receive(parent, okResponse('storage.get', sent[1].reqId as string, { value: 'ok' }));
    await pending;
    expect(settled).toBe(true);
  });

  it('C-03 (BUG-F8-01) .err frame with no payload rejects E_INTERNAL and raises zero uncaught errors', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const tracked = trackUncaught();
    const pending = client.storage.get('k').catch((error: unknown) => error);
    try {
      receive(parent, {
        ch: BRIDGE_CHANNEL,
        type: 'storage.get.err',
        target: 'host',
        reqId: sent[1].reqId as string,
        ok: false,
      });
    } finally {
      tracked.restore();
    }

    const failure = (await pending) as ExtensionError;
    expect(tracked.events).toEqual([]);
    expect(failure).toBeInstanceOf(ExtensionError);
    expect(failure.code).toBe(BRIDGE_ERROR.INTERNAL);
    expect(failure.message).toBe('storage.get');
  });

  it('C-04 (BUG-F8-01) .err frame with null payload rejects E_INTERNAL and raises zero uncaught errors', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    for (const badFrame of [
      { payload: null },
      { payload: undefined },
      {}, // payload key absent entirely
    ]) {
      const tracked = trackUncaught();
      const pending = client.notify({ title: 'n' }).catch((error: unknown) => error);
      try {
        receive(parent, {
          ch: BRIDGE_CHANNEL,
          type: 'ui.notify.err',
          target: 'host',
          reqId: sent[sent.length - 1].reqId as string,
          ok: false,
          ...badFrame,
        });
      } finally {
        tracked.restore();
      }

      const failure = (await pending) as ExtensionError;
      expect(tracked.events).toEqual([]);
      expect(failure).toBeInstanceOf(ExtensionError);
      expect(failure.code).toBe(BRIDGE_ERROR.INTERNAL);
      expect(failure.message).toBe('ui.notify');
    }
  });

  it('C-05 err payload carrying non-string primitives still rejects immediately via E_INTERNAL fallback', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    for (const badPayload of [{ code: 7 }, false, 0, 'boom']) {
      const pending = client.storage.remove('k').catch((error: unknown) => error);
      const reqId = (sent[sent.length - 1] as Sent).reqId as string;
      receive(parent, {
        ch: BRIDGE_CHANNEL,
        type: 'storage.remove.err',
        target: 'host',
        reqId,
        ok: false,
        payload: badPayload,
      });
      const failure = (await pending) as ExtensionError;
      expect(failure).toBeInstanceOf(ExtensionError);
      expect(failure.code).toBe(BRIDGE_ERROR.INTERNAL);
      expect(failure.message).toBe('storage.remove');
    }
  });
});

describe('value serialization fidelity (storage)', () => {
  it('C-06 set serializes nested objects/arrays verbatim onto the wire', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const value = { nested: { arr: [1, 'a', null, { b: true }] }, flag: false, empty: '' };
    const stored = client.storage.set('cfg', value);
    expect(sent[1]).toMatchObject({ type: 'storage.set', payload: { key: 'cfg', value } });
    receive(parent, okResponse('storage.set', sent[1].reqId as string, {}));
    await stored;
  });

  it('C-07 set posts primitive values as-is including explicit null and undefined', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const num = client.storage.set('n', 0);
    expect(sent[1]).toMatchObject({ type: 'storage.set', payload: { key: 'n', value: 0 } });
    receive(parent, okResponse('storage.set', sent[1].reqId as string, {}));
    await num;

    const bool = client.storage.set('f', false);
    expect(sent[2]).toMatchObject({ payload: { key: 'f', value: false } });
    receive(parent, okResponse('storage.set', sent[2].reqId as string, {}));
    await bool;

    const nil = client.storage.set('z', null);
    expect(sent[3]).toMatchObject({ payload: { key: 'z', value: null } });
    receive(parent, okResponse('storage.set', sent[3].reqId as string, {}));
    await nil;

    const undef = client.storage.set('u', undefined);
    const undefPayload = sent[4].payload as Record<string, unknown>;
    expect(undefPayload.key).toBe('u');
    expect('value' in undefPayload && undefPayload.value === undefined).toBe(true);
    receive(parent, okResponse('storage.set', sent[4].reqId as string, {}));
    await undef;
  });

  it('C-08 get returns falsy values transparently instead of collapsing them to null', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const falsy = client.storage.get<boolean>('flag');
    receive(parent, okResponse('storage.get', sent[1].reqId as string, { value: false }));
    await expect(falsy).resolves.toBe(false);

    const zero = client.storage.get<number>('zero');
    receive(parent, okResponse('storage.get', sent[2].reqId as string, { value: 0 }));
    await expect(zero).resolves.toBe(0);

    const blank = client.storage.get<string>('blank');
    receive(parent, okResponse('storage.get', sent[3].reqId as string, { value: '' }));
    await expect(blank).resolves.toBe('');

    const obj = client.storage.get<Record<string, unknown>>('obj');
    const roundTrip = { deep: { x: [1, 2] }, s: 'text' };
    receive(parent, okResponse('storage.get', sent[4].reqId as string, { value: roundTrip }));
    await expect(obj).resolves.toEqual(roundTrip);

    const absent = client.storage.get('absent');
    receive(parent, okResponse('storage.get', sent[5].reqId as string, {}));
    await expect(absent).resolves.toBeNull();
  });
});

describe('argument pass-through (command.invoke)', () => {
  it('C-09 forwards the request object verbatim: identity, key set, order and extra fields', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const args: Record<string, unknown> = { sql: 'select 1', limit: 10, opts: { timeoutMs: 5 } };
    const invokeRequest = {
      configId: 'cfg-9',
      command: 'query',
      args,
      tracingTag: 'plugin-side-extra',
    };

    const pending = client.command.invoke(invokeRequest);

    // Zero restructuring: the exact object handed in is what reaches the wire.
    expect(sent[1].type).toBe('command.invoke');
    expect(sent[1].payload).toBe(invokeRequest);
    const payload = sent[1].payload as Record<string, unknown>;
    expect(Object.keys(payload)).toEqual(Object.keys(invokeRequest));
    expect(payload.args).toBe(args);
    expect(payload.tracingTag).toBe('plugin-side-extra');

    receive(
      parent,
      okResponse('command.invoke', sent[1].reqId as string, { result: { rows: [], columns: [] } }),
    );
    await expect(pending).resolves.toEqual({ rows: [], columns: [] });
  });
});

describe('concurrent request routing', () => {
  it('C-10 routes 50 in-flight requests by unique reqId even when answered out of order', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const COUNT = 50;
    const markers = Array.from({ length: COUNT }, (_, i) => `payload-${i}`);
    const pendings = markers.map((marker) =>
      client.i18n.getString(marker).then((value) => ({ marker, value })),
    );

    const ids = sent.slice(1).map((envelope) => envelope.reqId as string);
    expect(ids).toHaveLength(COUNT);
    expect(new Set(ids).size).toBe(COUNT);

    // Shuffle deterministically so completion order != request order.
    const order = [...ids.keys()];
    for (let i = order.length - 1; i > 0; i -= 1) {
      const j = (i * 7 + 13) % (i + 1);
      [order[i], order[j]] = [order[j]!, order[i]!];
    }
    for (const idx of order) {
      receive(parent, okResponse('i18n.getString', ids[idx]!, { value: markers[idx] }));
    }

    const results = await Promise.all(pendings);
    for (const [i, result] of results.entries()) {
      expect(result).toEqual({ marker: markers[i], value: markers[i] });
    }
  });

  it('C-11 ignores duplicate and late responses after a request already settled', async () => {
    const { parent, sent } = makeParentWindow();
    const { client } = await handshake(parent);

    const first = client.i18n.getString('dup');
    const reqId = sent[1].reqId as string;
    receive(parent, okResponse('i18n.getString', reqId, { value: 'first' }));
    receive(parent, okResponse('i18n.getString', reqId, { value: 'duplicate' }));
    await expect(first).resolves.toBe('first');

    // Unknown/foreign reqIds never touch live requests.
    const second = client.i18n.getString('other');
    receive(parent, okResponse('i18n.getString', 'bogus-id', { value: 'nope' }));
    receive(parent, okResponse('i18n.getString', sent[2].reqId as string, { value: 'real' }));
    await expect(second).resolves.toBe('real');
  });

  it('C-12 detach aborts all 50 in-flight requests with EXTENSION_DETACHED', async () => {
    const { parent } = makeParentWindow();
    const { client } = await handshake(parent);

    const COUNT = 50;
    const pendings = Array.from({ length: COUNT }, (_, i) =>
      client.storage.get(`key-${i}`).catch((error: unknown) => error),
    );
    client.detach();

    const outcomes = await Promise.all(pendings);
    for (const outcome of outcomes) {
      expect(outcome).toBeInstanceOf(ExtensionError);
      expect((outcome as ExtensionError).code).toBe(SDK_ERROR.DETACHED);
    }
  });

  it('C-13 two clients on one page keep nonce-scoped reqIds disjoint and route answers correctly', async () => {
    const { parent, sent } = makeParentWindow();
    const clientA = createClient({ parentWindow: parent });
    const clientB = createClient({ parentWindow: parent });

    const readyA = clientA.ready();
    const readyB = clientB.ready();
    receive(parent, hostReady()); // one broadcast satisfies both handshakes
    await Promise.all([readyA, readyB]);

    const pa = clientA.context.getActiveConnection().then((value) => ({ from: 'A', value }));
    const pb = clientB.context.getActiveConnection().then((value) => ({ from: 'B', value }));

    // sent[0]/sent[1] are the two plugin.ready envelopes; pick out the typed
    // requests by route type (A posted first, B second).
    const reqIds = sent
      .filter((envelope) => envelope.type === 'context.getActiveConnection')
      .map((envelope) => envelope.reqId as string);
    expect(reqIds).toHaveLength(2);
    const [idA, idB] = reqIds;
    expect(idA!.split('-')[0]).not.toBe(idB!.split('-')[0]);

    // Answer B first: routing must be by reqId, not arrival order.
    receive(
      parent,
      okResponse('context.getActiveConnection', idB, {
        connection: { id: 'b1', name: 'from-b', dbType: 'pg' },
      }),
    );
    receive(
      parent,
      okResponse('context.getActiveConnection', idA, {
        connection: { id: 'a1', name: 'from-a', dbType: 'sqlite' },
      }),
    );

    await expect(pb).resolves.toEqual({
      from: 'B',
      value: { id: 'b1', name: 'from-b', dbType: 'pg' },
    });
    await expect(pa).resolves.toEqual({
      from: 'A',
      value: { id: 'a1', name: 'from-a', dbType: 'sqlite' },
    });
  });
});

describe('environmental guards', () => {
  it('C-14 typed calls reject E_INTERNAL when no parent window exists', async () => {
    const client = createClient({ parentWindow: null });
    await expect(client.storage.get('k')).rejects.toMatchObject({
      code: BRIDGE_ERROR.INTERNAL,
      message: expect.stringContaining('storage.get'),
    });
    await expect(client.notify({ title: 'x' })).rejects.toMatchObject({
      code: BRIDGE_ERROR.INTERNAL,
    });
  });

  it('C-15 SDK deadline constant mirrors the host router deadline', () => {
    expect(REQUEST_TIMEOUT_MS).toBe(30_000);
  });
});
