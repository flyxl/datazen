import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  ApiError,
  clearBackendClients,
  createBackendClient,
  getBackendClient,
  getSelectedBackendId,
  isBackendClientBound,
  selectBackend,
  setBackendClient,
  toCounter,
  toId,
  toTimestamp,
  useBackendClient,
  type BackendTransport,
  type ConnectionEvent,
  type Counter,
  type EventEnvelope,
  type Id,
  type MethodMap,
  type Timestamp,
  type OpenSessionRequest,
  type SessionHandle,
  type SubmissionToken,
  type SubscribeEventsRequest,
} from '../src/index';

/**
 * Fixtures go through the narrowing helpers on purpose.
 *
 * The branded types exist so that a raw `string` cannot be passed where an
 * `Id`/`Counter`/`Timestamp` belongs; a test that reached for `as Id` would
 * defeat exactly that and could not catch a swapped handle.
 */
const id = (value: string): Id => mustNarrow(toId(value), 'Id', value);
const counter = (value: number): Counter => mustNarrow(toCounter(value), 'Counter', value);
const timestamp = (value: number): Timestamp => mustNarrow(toTimestamp(value), 'Timestamp', value);

/** Fail loudly at fixture construction rather than leaking `undefined` downstream. */
function mustNarrow<T>(narrowed: T | undefined, kind: string, value: unknown): T {
  if (narrowed === undefined) throw new Error(`fixture ${String(value)} is not a valid ${kind}`);
  return narrowed;
}

const TOKEN: SubmissionToken = {
  idempotencyKey: 'idem-1',
  expiresAt: timestamp(1_700_000_000_000),
};

const OPEN_REQUEST: OpenSessionRequest = {
  initialTarget: {
    connectionId: id('conn-1'),
    namespace: { database: null, catalog: null, schema: 'public', path: [] },
    object: null,
  },
  owner: { kind: 'editor', clientInstanceId: id('client-1'), editorSessionId: id('ed-1') },
  idempotencyKey: 'idem-open-1',
};

const HANDLE: SessionHandle = { dbSessionId: id('sess-1'), runtimeEpoch: id('epoch-1') };

/**
 * Records every `call` so the contract can be asserted on the wire shape, not
 * on the facade's return value. A facade that quietly renames or reshapes an
 * argument would otherwise still receive the fake's canned answer and pass.
 */
function createFakeTransport() {
  const calls: Array<{ method: keyof MethodMap; payload: unknown }> = [];
  const responses = new Map<keyof MethodMap, unknown>();

  const transport: BackendTransport = {
    async call<K extends keyof MethodMap>(method: K, payload: MethodMap[K]['request']) {
      calls.push({ method, payload });
      if (!responses.has(method)) throw new Error(`fake transport has no response for ${method}`);
      return responses.get(method) as MethodMap[K]['response'];
    },
  };

  return { transport, calls, responses };
}

describe('BackendClient against a fake transport', () => {
  beforeEach(() => clearBackendClients());
  afterEach(() => clearBackendClients());

  it('sends the documented method name and request for openSession', async () => {
    const fake = createFakeTransport();
    fake.responses.set('openSession', { session: HANDLE });

    const client = createBackendClient('desktop', fake.transport);
    await client.openSession(OPEN_REQUEST, TOKEN);

    expect(fake.calls).toHaveLength(1);
    expect(fake.calls[0]?.method).toBe('openSession');
    // The token is folded into the request rather than carried as a separate
    // transport argument, so the adapter sees exactly one envelope.
    expect(fake.calls[0]?.payload).toEqual({ ...OPEN_REQUEST, submissionToken: TOKEN });
  });

  it('returns the transport response unchanged', async () => {
    const fake = createFakeTransport();
    const receipt = { session: HANDLE };
    fake.responses.set('openSession', receipt);

    const client = createBackendClient('desktop', fake.transport);
    await expect(client.openSession(OPEN_REQUEST, TOKEN)).resolves.toBe(receipt);
  });

  it('routes cancelExecution under its own method name and passes the id through', async () => {
    const fake = createFakeTransport();
    fake.responses.set('cancelExecution', {
      executionId: id('exec-7'),
      disposition: 'requested',
      state: 'cancelRequested',
    });

    const client = createBackendClient('desktop', fake.transport);
    await client.cancelExecution(id('exec-7'), TOKEN);

    expect(fake.calls[0]).toEqual({
      method: 'cancelExecution',
      payload: { executionId: id('exec-7'), submissionToken: TOKEN },
    });
  });

  it('reads a session by handle without adding anything to the payload', async () => {
    const fake = createFakeTransport();
    fake.responses.set('getSession', {});

    const client = createBackendClient('desktop', fake.transport);
    await client.getSession(HANDLE);

    expect(fake.calls[0]).toEqual({ method: 'getSession', payload: { handle: HANDLE } });
  });

  it('turns a transport rejection into an ApiError carrying the requestId', async () => {
    const transport: BackendTransport = {
      call: () => Promise.reject({ code: 'SessionNotFound', message: 'gone', requestId: 'r-1' }),
    };
    const client = createBackendClient('desktop', transport);

    await expect(client.getSession(HANDLE)).rejects.toBeInstanceOf(ApiError);
    await expect(client.getSession(HANDLE)).rejects.toMatchObject({
      code: 'SessionNotFound',
      requestId: 'r-1',
    });
  });

  it('does not swallow an ApiError the transport already produced', async () => {
    const original = new ApiError('ContextConflict', 'context moved on', { requestId: 'r-2' });
    const transport: BackendTransport = { call: () => Promise.reject(original) };
    const client = createBackendClient('desktop', transport);

    await expect(client.getSession(HANDLE)).rejects.toBe(original);
  });

  it('reports CapabilityUnsupported when the backend cannot subscribe', async () => {
    const fake = createFakeTransport();
    const client = createBackendClient('desktop', fake.transport);

    const stream = client.subscribeEvents({ streamId: id('exec-1'), afterSequence: null });

    await expect(stream[Symbol.asyncIterator]().next()).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
  });

  it('hands the subscribe request to the transport untouched', async () => {
    const request: SubscribeEventsRequest = { streamId: id('exec-2'), afterSequence: counter(41) };
    const seen: SubscribeEventsRequest[] = [];
    const transport: BackendTransport = {
      call: () => Promise.reject(new Error('unused')),
      subscribe: (_method, payload) => {
        seen.push(payload);
        const feed: AsyncIterable<EventEnvelope<ConnectionEvent>> = {
          [Symbol.asyncIterator]: async function* () {
            // An empty feed: the assertion is about the request, not the events.
          },
        };
        return feed;
      },
    };
    const client = createBackendClient('desktop', transport);
    const stream = client.subscribeEvents(request);

    await stream[Symbol.asyncIterator]().next();

    expect(seen).toEqual([request]);
  });
});

describe('backend client registry', () => {
  beforeEach(() => clearBackendClients());
  afterEach(() => clearBackendClients());

  it('starts unbound', () => {
    expect(isBackendClientBound()).toBe(false);
    expect(getBackendClient()).toBeNull();
    expect(getSelectedBackendId()).toBeNull();
  });

  it('selects the first backend that binds', () => {
    const a = createBackendClient('desktop', createFakeTransport().transport);

    setBackendClient('desktop', a);

    // The app entry binds once before first paint and then expects lookups to
    // resolve, so the first binding is also the selection.
    expect(getSelectedBackendId()).toBe('desktop');
    expect(getBackendClient()).toBe(a);
  });

  it('does not let a later binding steal the selection', () => {
    const a = createBackendClient('desktop', createFakeTransport().transport);
    const b = createBackendClient('web', createFakeTransport().transport);

    setBackendClient('desktop', a);
    setBackendClient('web', b);

    // Switching is explicit; binding a second facade is not a silent switch.
    expect(getSelectedBackendId()).toBe('desktop');

    selectBackend('web');
    expect(getBackendClient()).toBe(b);

    selectBackend('desktop');
    expect(getBackendClient()).toBe(a);
  });

  it('keeps each backend id mapped to its own facade', () => {
    const a = createBackendClient('desktop', createFakeTransport().transport);
    const b = createBackendClient('web', createFakeTransport().transport);

    setBackendClient('desktop', a);
    setBackendClient('web', b);

    expect(a.backendId).toBe('desktop');
    expect(b.backendId).toBe('web');
  });

  it('exposes the transport the driver-sdk re-exports rely on', () => {
    const fake = createFakeTransport();
    const client = createBackendClient('desktop', fake.transport);
    setBackendClient('desktop', client);

    expect(useBackendClient().transport).toBe(fake.transport);
  });
});

describe('unbound BackendClient reports an explicit error', () => {
  beforeEach(() => clearBackendClients());
  afterEach(() => clearBackendClients());

  it('useBackendClient() throws instead of lazily constructing a client', () => {
    expect(isBackendClientBound()).toBe(false);
    expect(() => useBackendClient()).toThrow(
      'BackendClient has not been bound; check that the platform adapter ran.',
    );
  });

  it('names the missing adapter so the failure points at the wiring step', () => {
    let message = '';
    try {
      useBackendClient();
    } catch (error) {
      message = error instanceof Error ? error.message : '';
    }
    expect(message).toContain('has not been bound');
    expect(message).toContain('platform adapter');
  });

  it('goes back to throwing after the bindings are cleared', () => {
    setBackendClient('desktop', createBackendClient('desktop', createFakeTransport().transport));
    expect(() => useBackendClient()).not.toThrow();

    clearBackendClients();
    expect(() => useBackendClient()).toThrow('has not been bound');
  });

  it('does not reach the transport at all while unbound', () => {
    const fake = createFakeTransport();
    const client = createBackendClient('desktop', fake.transport);
    // The facade works standalone; only the registry accessor is gated.
    expect(client.backendId).toBe('desktop');
    expect(fake.calls).toHaveLength(0);
    expect(isBackendClientBound()).toBe(false);
  });
});

/**
 * `toCounter` is the whole frontend half of the CM-01 counter contract.
 *
 * The kernel counts in `u64` and serializes a `Counter` as a decimal string, so
 * a narrowing helper that only accepts numbers does not reject the payload — it
 * returns `undefined` for every counter, and every caller that defaults turns
 * that into 0. A job that moved ten million rows renders as a job that moved
 * none, with no error anywhere. These tests pin the accepted wire forms, and
 * pin the refusal to round, because rounding would reintroduce the exact
 * precision loss CM-01 exists to prevent.
 */
describe('toCounter', () => {
  it('accepts the decimal string the kernel serializes', () => {
    expect(toCounter('0')).toBe(0);
    expect(toCounter('42')).toBe(42);
    expect(toCounter('10000000')).toBe(10_000_000);
  });

  it('accepts a leading-zero decimal without treating it as octal', () => {
    expect(toCounter('007')).toBe(7);
  });

  it('still accepts a bare number', () => {
    expect(toCounter(0)).toBe(0);
    expect(toCounter(41)).toBe(41);
  });

  it('refuses to round a count past what a JS number holds exactly', () => {
    // 2^53 + 1: a valid decimal integer that `Number` cannot hold exactly, so
    // accepting it would hand back a count that is simply wrong. Refuse rather
    // than round — and accept the largest exact value right up to the edge.
    expect(toCounter('9007199254740993')).toBeUndefined();
    expect(toCounter(String(Number.MAX_SAFE_INTEGER))).toBe(9007199254740991);
    expect(toCounter(String(Number.MAX_SAFE_INTEGER + 1))).toBeUndefined();
  });

  it('rejects anything that is not a plain non-negative integer', () => {
    for (const value of [
      '',
      ' 1',
      '1 ',
      '-1',
      '+1',
      '1.0',
      '1e3',
      '0x10',
      'NaN',
      '1,000',
      null,
      undefined,
      true,
      {},
      [],
      Number.NaN,
      Number.POSITIVE_INFINITY,
      -1,
      1.5,
    ]) {
      expect(toCounter(value), `toCounter(${String(value)})`).toBeUndefined();
    }
  });
});
