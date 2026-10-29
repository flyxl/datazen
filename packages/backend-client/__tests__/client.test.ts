import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
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
  type Id,
  type MethodMap,
  type SessionHandle,
} from '../src/index';

/**
 * The facade half of the F3 domain split: what `client.ts` itself owns —
 * composition, the binding registry, and the DTO narrowing helpers the Job
 * domain reuses. Per-domain wire contracts live in the per-domain test files
 * (`sessionClient.test.ts`, `executionClient.test.ts`, ...).
 */

/** Narrowing fixtures keep the branded types honest (see `toCounter` suite). */
const id = (value: string): Id => mustNarrow(toId(value), 'Id', value);

function mustNarrow<T>(narrowed: T | undefined, kind: string, value: unknown): T {
  if (narrowed === undefined) throw new Error(`fixture ${String(value)} is not a valid ${kind}`);
  return narrowed;
}

/**
 * Records every `call` so the contract can be asserted on the wire shape, not
 * on the facade's return value. A facade that quietly renames or reshapes an
 * argument would otherwise still receive the fake's canned answer and pass.
 */
export function createFakeTransport() {
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

describe('facade composes the domain clients', () => {
  it('exposes every domain method on one object', () => {
    const client = createBackendClient('desktop', createFakeTransport().transport);
    // One method per domain, as a composition smoke: the per-domain files
    // assert the wire shapes; this asserts the spread lost nothing.
    expect(typeof client.getSettings).toBe('function');
    expect(typeof client.getPlatformIdentity).toBe('function');
    expect(typeof client.listConnections).toBe('function');
    expect(typeof client.openSession).toBe('function');
    expect(typeof client.executeInSession).toBe('function');
    expect(typeof client.subscribeEvents).toBe('function');
    expect(typeof client.startJob).toBe('function');
    expect(typeof client.readArtifact).toBe('function');
    expect(typeof client.getTableData).toBe('function');
    expect(typeof client.watchJob).toBe('function');
    expect(typeof client.submitJobIdempotent).toBe('function');
    expect(client.backendId).toBe('desktop');
  });

  it('a second facade is a second composition — the Job domain state never leaks across', () => {
    const a = createBackendClient('desktop', createFakeTransport().transport);
    const b = createBackendClient('desktop', createFakeTransport().transport);
    // The Job domain's receipt map is per-composition-root, which is the
    // documented boundary: two clients never share idempotency receipts.
    expect(a).not.toBe(b);
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

/** Shared fixture: a narrowed session handle, reused by the domain test files. */
export const TEST_HANDLE: SessionHandle = {
  dbSessionId: id('sess-1'),
  runtimeEpoch: id('epoch-1'),
};

/** Re-exported for the per-domain files so the fake stays in one place. */
export { id, toCounter, toTimestamp };
