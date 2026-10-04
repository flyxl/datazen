/**
 * [tester] KV slot state-machine gaps — filled during the redis-kvbar-ui test round.
 *
 * The coder's suite (`kvBarSlots.test.tsx`) covers the happy path of both slots;
 * this file adds the branches that were still uncovered and that carry real risk:
 *  - the "stale replies are dropped" invariant `useKeyObjectInfo` claims in its
 *    own docblock (both the resolve and the reject side);
 *  - per-field `null` degradation in the *status bar* (PRD §3.4: an unavailable
 *    measurement must not render as `0`, and must not drag the other rows down);
 *  - the status bar's database label when the host has not resolved an index;
 *  - the sidebar's `null` arms for memory / idle / a real remaining TTL;
 *  - an INFO reply whose section list carries no `maxmemory_policy` at all, and
 *    a policy reply that lands after `dbSessionId` has moved on;
 *  - both `PTTL` sentinel arms at the render side: `-2` (gone inside the pipeline
 *    while `missing` stayed false) vs `-1` (genuinely never expires) — BUG-004.
 *
 * No case here is skipped any more. Three of them were registered as
 * `FIXME(redis-kvbar-ui-BUG-00N)` in round 1 and are un-skipped by the fix
 * commit that closes each one; the measured pre-fix failure is quoted next to
 * the case as the proof it was never vacuous. The BUG-004 pair used to pin
 * today's wrong label in a green case so the skipped twin could not pass by
 * accident — once the fix landed that would have been a second, redundant
 * assertion of the same arm, so it now asserts the *other* arm (`PTTL -1`, a
 * key that genuinely never expires). Gone and never-expires are therefore both
 * pinned and either can regress on its own.
 *
 * Assertion policy (PRD §7-6): `data-*` markers and i18n keys only — no rendered
 * English copy is pinned anywhere in this file.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, waitFor } from '@testing-library/react';
import type { KvSlotState, KvStatusBarProps } from '@datazen/driver-sdk';

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

const commandInvoke = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  redisCommandInvoke: (...args: unknown[]) => commandInvoke(...(args as [])),
}));

import { RedisKeyPropsSidebar, RedisKvStatusBar } from '../kv-bar';
import { invokeMaxmemoryPolicy, type KeyObjectInfo } from '../kv-bar/keyObjectInfo';

function makeRelay(): KvSlotState {
  const listeners = new Set<() => void>();
  let selectedKey: string | null = null;
  let dirty = false;
  // F-1 widened scalars (W3-A §1.1). Kept in step with the frozen contract —
  // the slots read these, so a relay missing them is not a KvSlotState at all.
  let loadedCount = 0;
  let scanCursor = '0';
  let scanning = false;
  let budgetUsed = 0;
  let budgetTotal = 0;
  let selectionCount = 0;
  let lastWriteCommand: string | null = null;
  let lastWriteDurationMs: number | null = null;
  const notify = () => {
    for (const listener of listeners) listener();
  };
  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    getSelectedKey: () => selectedKey,
    selectKey(key) {
      if (key === selectedKey) return;
      selectedKey = key;
      notify();
    },
    getDirty: () => dirty,
    setDirty(next) {
      if (next === dirty) return;
      dirty = next;
      notify();
    },
    // ── W3-A §1.1 widening. Setters are idempotent, like the host atom. ──
    getLoadedCount: () => loadedCount,
    setLoadedCount(next) {
      if (next === loadedCount) return;
      loadedCount = next;
      notify();
    },
    getScanCursor: () => scanCursor,
    setScanCursor(next) {
      if (next === scanCursor) return;
      scanCursor = next;
      notify();
    },
    isScanning: () => scanning,
    setScanning(next) {
      if (next === scanning) return;
      scanning = next;
      notify();
    },
    getScanBudgetUsed: () => budgetUsed,
    getScanBudgetTotal: () => budgetTotal,
    setScanBudget(used, total) {
      if (used === budgetUsed && total === budgetTotal) return;
      budgetUsed = used;
      budgetTotal = total;
      notify();
    },
    getSelectionCount: () => selectionCount,
    setSelectionCount(next) {
      if (next === selectionCount) return;
      selectionCount = next;
      notify();
    },
    getLastWriteCommand: () => lastWriteCommand,
    getLastWriteDurationMs: () => lastWriteDurationMs,
    recordWrite(command, durationMs) {
      if (command === lastWriteCommand && durationMs === lastWriteDurationMs) return;
      lastWriteCommand = command;
      lastWriteDurationMs = durationMs;
      notify();
    },
  };
}

function info(overrides: Partial<KeyObjectInfo> = {}): KeyObjectInfo {
  return {
    missing: false,
    type: 'string',
    memoryBytes: 640,
    encoding: 'embstr',
    idleSeconds: 90,
    freq: null,
    ttlMs: -1,
    ...overrides,
  };
}

function slotProps(
  state: KvSlotState,
  overrides: Partial<KvStatusBarProps> = {},
): KvStatusBarProps {
  return {
    connectionId: 'conn-1',
    dbSessionId: 'sess-1',
    connectionName: 'local',
    databaseType: 'redis' as KvStatusBarProps['databaseType'],
    database: 'db5',
    dbIndex: 5,
    state,
    ...overrides,
  };
}

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Queue one deferred per `key_object_info` call, keyed by the requested key. */
function queueReads(): Map<string, Deferred<KeyObjectInfo>> {
  const pending = new Map<string, Deferred<KeyObjectInfo>>();
  commandInvoke.mockImplementation((_plugin: string, command: string, payload: unknown) => {
    if (command !== 'key_object_info') return Promise.resolve({ sections: [] });
    const key = (payload as { key: string }).key;
    const slot = deferred<KeyObjectInfo>();
    pending.set(key, slot);
    return slot.promise;
  });
  return pending;
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('[tester] KvSlotState read staleness (useKeyObjectInfo docblock claim)', () => {
  // redis-kvbar-ui-BUG-001 (fixed, was `it.skip`): the read used to keep the
  // previous key's payload while the next key's read was in flight, so both slots
  // attributed one key's type/size/TTL to another. Un-skipped by the fix commit;
  // measured failure before the fix was
  // `expected <span data-part="type">…</span> to be null` while
  // `data-status-state` read `loading`.
  it('does not keep painting the previous key while the new read is in flight', async () => {
    const relay = makeRelay();
    const pending = queueReads();
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);

    act(() => relay.selectKey('first'));
    await waitFor(() => expect(pending.has('first')).toBe(true));
    pending.get('first')!.resolve(info({ type: 'string', memoryBytes: 640 }));
    await waitFor(() =>
      expect(container.querySelector('[data-part="type"]')?.textContent).toBe('string'),
    );

    act(() => relay.selectKey('second'));
    await waitFor(() => expect(pending.has('second')).toBe(true));
    // Selection moved to `second`, its read has not landed: the bar must not
    // attribute `first`'s type / size / TTL to it (R3 in the track ledger).
    expect(container.querySelector('[data-status-state]')?.getAttribute('data-status-state')).toBe(
      'loading',
    );
    expect(container.querySelector('[data-part="selected-key"]')?.textContent).toBe('second');
    expect(container.querySelector('[data-part="type"]')).toBeNull();
    expect(container.querySelector('[data-part="size"]')).toBeNull();
  });

  it('drops a late success reply after the selection moved on', async () => {
    const relay = makeRelay();
    const pending = queueReads();
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);

    act(() => relay.selectKey('slow'));
    await waitFor(() => expect(pending.has('slow')).toBe(true));
    act(() => relay.selectKey('fast'));
    await waitFor(() => expect(pending.has('fast')).toBe(true));

    const fast = info({ type: 'hash', memoryBytes: 2048 });
    pending.get('fast')!.resolve(fast);
    await waitFor(() =>
      expect(container.querySelector('[data-part="type"]')?.textContent).toBe('hash'),
    );
    // The superseded reply lands afterwards and must not repaint the bar.
    pending.get('slow')!.resolve(info({ type: 'zset', memoryBytes: 99 }));
    await new Promise((r) => setTimeout(r, 0));
    expect(container.querySelector('[data-part="type"]')?.textContent).toBe('hash');
    expect(container.querySelector('[data-part="size"]')?.textContent).toBe('2.0 KB');
  });

  it('drops a late failure after the selection moved on', async () => {
    const relay = makeRelay();
    const pending = queueReads();
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);

    act(() => relay.selectKey('slow'));
    await waitFor(() => expect(pending.has('slow')).toBe(true));
    act(() => relay.selectKey('fast'));
    pending.get('slow')!.reject(new Error('aborted by db switch'));
    pending.get('fast')!.resolve(info({ type: 'list' }));

    await waitFor(() =>
      expect(
        container.querySelector('[data-status-state]')?.getAttribute('data-status-state'),
      ).toBe('ready'),
    );
    expect(container.querySelector('[data-part="type"]')?.textContent).toBe('list');
  });
});

describe('[tester] KvStatusBar per-field degradation (PRD §3.4 "不渲染，不是渲染 0")', () => {
  it('omits type and size when the server answered null for just those', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ type: null, memoryBytes: null, encoding: null }));
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);
    act(() => relay.selectKey('partial'));

    await waitFor(() =>
      expect(
        container.querySelector('[data-status-state]')?.getAttribute('data-status-state'),
      ).toBe('ready'),
    );
    expect(container.querySelector('[data-part="type"]')).toBeNull();
    expect(container.querySelector('[data-part="size"]')).toBeNull();
    expect(container.querySelector('[data-part="selected-key"]')?.textContent).toBe('partial');
    // Nothing may fall back to a zero-like value.
    expect(container.textContent).not.toMatch(/\b0\b/);
  });

  it('keeps the TTL pill away for a remaining TTL that could not be read', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ ttlMs: -1 }));
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);
    act(() => relay.selectKey('forever'));
    await waitFor(() =>
      expect(container.querySelector('[data-part="size"]')?.textContent).toBe('640 B'),
    );
    expect(container.querySelector('[data-part="ttl"]')).toBeNull();
  });

  it('derives the database label from the index when the host has no label yet', () => {
    const relay = makeRelay();
    const { container } = render(
      <RedisKvStatusBar {...slotProps(relay, { database: null, dbIndex: 3 })} />,
    );
    expect(container.querySelector('[data-part="database"]')?.textContent).toBe('db3');
  });

  it('falls back to the neutral dash when neither label nor index resolved', () => {
    const relay = makeRelay();
    const { container } = render(
      <RedisKvStatusBar {...slotProps(relay, { database: null, dbIndex: undefined })} />,
    );
    expect(container.querySelector('[data-part="database"]')?.textContent).toBe('—');
    expect(container.querySelector('[data-status-state]')?.getAttribute('data-status-state')).toBe(
      'no-key',
    );
    expect(commandInvoke).not.toHaveBeenCalled();
  });

  it('reports an unresolvable db index for a selected key as unavailable, not ready', async () => {
    const relay = makeRelay();
    const { container } = render(
      <RedisKvStatusBar {...slotProps(relay, { database: null, dbIndex: undefined })} />,
    );
    act(() => relay.selectKey('orphan'));
    await waitFor(() =>
      expect(
        container.querySelector('[data-status-state]')?.getAttribute('data-status-state'),
      ).toBe('unavailable'),
    );
    expect(commandInvoke).not.toHaveBeenCalled();
  });
});

describe('[tester] KeyPropsSidebar null arms', () => {
  it('labels a real remaining TTL and keeps the memory/idle rows named when null', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(
      info({ memoryBytes: null, idleSeconds: null, ttlMs: 125_000, encoding: null }),
    );
    render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);
    act(() => relay.selectKey('user:1'));

    await waitFor(() =>
      expect(
        document
          .querySelector('[data-testid="redis-kv-key-props-sidebar"]')
          ?.getAttribute('data-props-state'),
      ).toBe('ready'),
    );
    const aside = document.querySelector('[data-testid="redis-kv-key-props-sidebar"]')!;
    // TTL -1 has its own word; a positive PTTL must print the measured duration.
    expect(aside.querySelector('[data-attr="ttl"] dd')?.getAttribute('data-value')).toBe('2m 05s');
    for (const attr of ['memory', 'idle', 'encoding']) {
      const dd = aside.querySelector(`[data-attr="${attr}"] dd`)!;
      expect(dd.getAttribute('data-value'), attr).toBe('');
      expect(dd.getAttribute('data-fallback-key'), attr).toBe('redis.keyProps.unavailable');
    }
    expect(aside.textContent).not.toMatch(/\b0\b/);
  });

  it('prints an LFU freq when the server really returned one', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ freq: 7 }));
    render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);
    act(() => relay.selectKey('hot'));
    await waitFor(() =>
      expect(document.querySelector('[data-attr="freq"] dd')?.getAttribute('data-value')).toBe('7'),
    );
    expect(document.querySelector('[data-attr="freq"] dd')?.getAttribute('data-fallback-key')).toBe(
      null,
    );
  });
});

describe('[tester] KeyPropsSidebar stale payload — sidebar half of redis-kvbar-ui-BUG-001', () => {
  // redis-kvbar-ui-BUG-001 (fixed, was `it.skip`): same root cause as the status
  // bar case, but the sidebar was worse — it showed the loading hint *and* the
  // previous key's attribute list at the same time. Measured failure before the
  // fix: `expected <div data-attr="type">…</div> to be null` while
  // `data-props-state` was `loading`.
  it('hides the previous key attributes while the next key is being read', async () => {
    const relay = makeRelay();
    const pending = queueReads();
    render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);

    act(() => relay.selectKey('first'));
    await waitFor(() => expect(pending.has('first')).toBe(true));
    pending.get('first')!.resolve(info());
    await waitFor(() =>
      expect(document.querySelector('[data-props-state]')?.getAttribute('data-props-state')).toBe(
        'ready',
      ),
    );

    act(() => relay.selectKey('second'));
    await waitFor(() => expect(pending.has('second')).toBe(true));
    const aside = document.querySelector('[data-testid="redis-kv-key-props-sidebar"]')!;
    expect(aside.getAttribute('data-props-state')).toBe('loading');
    expect(aside.querySelector('[data-attr="type"]')).toBeNull();
    expect(aside.querySelector('[data-i18n-key="redis.keyProps.loading"]')).not.toBeNull();
  });
});

describe('[tester] PTTL sentinels reach the renderer (BUG-004)', () => {
  // redis-kvbar-ui-BUG-004 (fixed, was `it.skip`): `key_object_info` can answer
  // `missing:false` together with `ttlMs:-2` — `TYPE` succeeds and `PTTL` reports
  // the key gone inside the same pipeline (`ops/workbench/shapes.rs`'s
  // `parse_key_info` takes `ttl_ms` straight from the reply while
  // `unreadable_key_state` hard-codes `missing:false`).
  // `describeTtl` separates that as `kind:'missing'`, but no renderer consumed
  // the distinction, so the ttl row printed `redis.noExpiry` — the one thing
  // certainly not true of a key the server just reported as absent. Measured
  // failure before the fix:
  // `expected 'redis.noExpiry' to be 'redis.keyProps.missing'`.
  it('labels a gone-by-PTTL key as gone, never as "no expiry"', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ missing: false, ttlMs: -2 }));
    render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);
    act(() => relay.selectKey('racing'));

    await waitFor(() =>
      expect(document.querySelector('[data-props-state]')?.getAttribute('data-props-state')).toBe(
        'ready',
      ),
    );
    const dd = document.querySelector('[data-attr="ttl"] dd')!;
    expect(dd.getAttribute('data-fallback-key')).toBe('redis.keyProps.missing');
    // Gone is an answer, not a measurement: no duration may be printed either.
    expect(dd.getAttribute('data-value')).toBe('');
    // The rest of the pipeline still stands, so the *row list* is not in the
    // command-wide `missing` state — only the ttl line carries the news.
    expect(document.querySelector('[data-props-state]')?.getAttribute('data-props-state')).toBe(
      'ready',
    );
    expect(document.querySelector('[data-attr="type"]')).not.toBeNull();
  });

  it('keeps a genuinely never-expires key on the no-expiry word', async () => {
    // The other arm of the same branch, asserted on purpose: `-1` is the server
    // answering "this key has no TTL", so folding the two sentinels together (or
    // labelling everything `missing`) reddens here instead of quietly passing.
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ missing: false, ttlMs: -1 }));
    render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);
    act(() => relay.selectKey('eternal'));

    await waitFor(() =>
      expect(document.querySelector('[data-props-state]')?.getAttribute('data-props-state')).toBe(
        'ready',
      ),
    );
    const dd = document.querySelector('[data-attr="ttl"] dd')!;
    expect(dd.getAttribute('data-fallback-key')).toBe('redis.noExpiry');
    expect(dd.getAttribute('data-value')).toBe('');
  });
});

describe('[tester] maxmemory_policy read shape', () => {
  it('degrades to null when INFO answered sections but named no policy', async () => {
    commandInvoke.mockResolvedValue({
      sections: [{ name: 'Memory', entries: [{ key: 'used_memory', value: '123' }] }],
    });
    await expect(invokeMaxmemoryPolicy('sess-1')).resolves.toBeNull();
  });

  it('takes the policy out of a later section when the first one lacks it', async () => {
    commandInvoke.mockResolvedValue({
      sections: [
        { name: 'Server', entries: [{ key: 'redis_version', value: '7.2.0' }] },
        { name: 'Memory', entries: [{ key: 'maxmemory_policy', value: 'volatile-lru' }] },
      ],
    });
    await expect(invokeMaxmemoryPolicy('sess-1')).resolves.toBe('volatile-lru');
  });

  it('drops a policy reply that lands after the session switched', async () => {
    // `maxmemory_policy` is per-server, so a reply from the previous session
    // must never paint the new session's row while its own INFO is in flight.
    const relay = makeRelay();
    const policies = new Map<string, Deferred<string>>();
    commandInvoke.mockImplementation((_plugin: string, command: string, payload: unknown) => {
      if (command === 'key_object_info') return Promise.resolve(info());
      const sessionId = (payload as { dbSessionId: string }).dbSessionId;
      const slot = deferred<string>();
      policies.set(sessionId, slot);
      return slot.promise.then((value) => ({
        sections: [{ name: 'Memory', entries: [{ key: 'maxmemory_policy', value }] }],
      }));
    });

    const { rerender } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(policies.has('sess-1')).toBe(true));

    rerender(
      <RedisKeyPropsSidebar
        {...slotProps(relay, { dbSessionId: 'sess-2' })}
        open
        onClose={() => {}}
      />,
    );
    await waitFor(() => expect(policies.has('sess-2')).toBe(true));

    policies.get('sess-2')!.resolve('noeviction');
    await waitFor(() =>
      expect(
        document.querySelector('[data-attr="maxmemory-policy"] dd')?.getAttribute('data-value'),
      ).toBe('noeviction'),
    );

    policies.get('sess-1')!.resolve('allkeys-lfu');
    await new Promise((r) => setTimeout(r, 0));
    expect(
      document.querySelector('[data-attr="maxmemory-policy"] dd')?.getAttribute('data-value'),
    ).toBe('noeviction');
  });
});
