/**
 * redis-kvbar-ui — round-3 Tester re-check battery (`redis-kvbar-ui-BUG-005`).
 *
 * Independent reproduction for the fix at `010c6b406`: the eviction-policy row
 * is held as `{ session, value }` and filtered while rendering, so a
 * `dbSessionId` jump invalidates it in the same frame instead of keeping the
 * previous server's answer on screen next to the new session's attributes.
 *
 * What each case adds beyond the round-2 files (which split the same fact into
 * a before-frame case and an after-frame case, and pin the late-reply arm via
 * `kvBarSlotTesterGaps`):
 *
 *  1. **one continuous journey** — old value *visible* → jump → *not visible*
 *     (named empty state, not a blank) → the new session answers → painted —
 *     with the round-trip counts asserted at the end, so neither half of the
 *     pair can pass by accident and the jump cannot smuggle an extra ask.
 *  2. **a path no existing case walks**: the drawer is closed across the jump.
 *     Contract obligation 1 keeps the slot mounted while collapsed ("host owns
 *     the geometry"), so the tagged value *survives* in state through
 *     close → session swap → reopen; only the render-time identity check may
 *     keep the old server's fact off the new screen. A bare string, or a tag
 *     that is never consumed, fails this frame visibly.
 *  3. **coming back to a session that already answered** (`sess-1 → sess-2 →
 *     sess-1`). Every existing battery only moves forward, so it cannot tell a
 *     session-tagged single slot apart from a `Map<session, value>`: both serve
 *     a session they have not seen yet with nothing, while only the cache paints
 *     `sess-1`'s old answer the instant the panel hops back — with no round trip.
 *     This is the case that measures "the tag did not quietly become a cache",
 *     which is the direction the ruling forbids.
 *
 * Assertion policy (PRD §7-6): `data-*` markers, i18n keys and values echoed
 * by Redis (`noeviction`, `allkeys-lfu`) only. No rendered English copy.
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

import { RedisKeyPropsSidebar } from '../kv-bar';
import type { KeyObjectInfo } from '../kv-bar/keyObjectInfo';

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

/** Local stand-in for the host's per-panel atom (boundary guard forbids importing it). */
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

function keyReply(overrides: Partial<KeyObjectInfo> = {}): KeyObjectInfo {
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

type PolicyReply = {
  sections: Array<{ name: string; entries: Array<{ key: string; value: string }> }>;
};

const policyReply = (value: string): PolicyReply => ({
  sections: [{ name: 'Memory', entries: [{ key: 'maxmemory_policy', value }] }],
});

interface Router {
  /** In-flight key reads, one slot per `session|dbIndex|key` identity (newest ask). */
  keys: Map<string, Deferred<KeyObjectInfo>>;
  /** In-flight policy reads, one slot per session (the newest ask for it). */
  policies: Map<string, Deferred<PolicyReply>>;
  /** Session ids in the order `info_filtered` was asked for them. */
  asked: string[];
  /** Read identities in the order `key_object_info` was asked for them. */
  keyAsked: string[];
  /** How many times a given identity has been asked (a re-ask overwrites the slot). */
  keyAsks(id: string): number;
  calls(command: string): number;
}

function routeCommands(): Router {
  const keys = new Map<string, Deferred<KeyObjectInfo>>();
  const policies = new Map<string, Deferred<PolicyReply>>();
  const asked: string[] = [];
  const keyAsked: string[] = [];
  commandInvoke.mockImplementation((_plugin: string, command: string, payload: unknown) => {
    const body = payload as { dbSessionId?: string; dbIndex?: number; key?: string };
    if (command === 'key_object_info') {
      const slot = deferred<KeyObjectInfo>();
      const identity = `${body.dbSessionId}|${body.dbIndex}|${body.key}`;
      keyAsked.push(identity);
      keys.set(identity, slot);
      return slot.promise;
    }
    if (command === 'info_filtered') {
      const slot = deferred<PolicyReply>();
      asked.push(body.dbSessionId as string);
      policies.set(`${body.dbSessionId}`, slot);
      return slot.promise;
    }
    return Promise.resolve({ sections: [] });
  });
  return {
    keys,
    policies,
    asked,
    keyAsked,
    keyAsks: (id) => keyAsked.filter((entry) => entry === id).length,
    calls: (command) => commandInvoke.mock.calls.filter(([, name]) => name === command).length,
  };
}

const propsState = (el: HTMLElement | null) =>
  el?.querySelector('[data-props-state]')?.getAttribute('data-props-state') ?? null;
const sideValue = (el: HTMLElement | null, attr: string) =>
  el?.querySelector(`[data-attr="${attr}"] dd`)?.getAttribute('data-value') ?? null;
const sideFallbackKey = (el: HTMLElement | null, attr: string) =>
  el?.querySelector(`[data-attr="${attr}"] dd`)?.getAttribute('data-fallback-key') ?? null;

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

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('[tester r3] BUG-005 re-check: the policy row invalidates with its session', () => {
  it('walks the whole jump: visible for the old session, gone across it, painted by the new one', async () => {
    // The registered defect had two frames; pinning them in ONE continuous
    // journey is what proves the fix did not just move the clearing logic to a
    // different half of the timeline. `noeviction` must really paint before the
    // jump (otherwise the "gone" assertion after it could pass by never having
    // shown anything), then vanish the moment `dbSessionId` is `sess-2` while
    // only its key read has landed, and come back solely from `sess-2`'s INFO.
    const relay = makeRelay();
    const router = routeCommands();
    const { container, rerender } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(router.keys.has('sess-1|5|user:1')).toBe(true));
    await waitFor(() => expect(router.policies.has('sess-1')).toBe(true));
    router.keys.get('sess-1|5|user:1')!.resolve(keyReply({ type: 'string' }));
    router.policies.get('sess-1')!.resolve(policyReply('noeviction'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('noeviction'));
    expect(propsState(container)).toBe('ready');

    rerender(
      <RedisKeyPropsSidebar
        {...slotProps(relay, { dbSessionId: 'sess-2' })}
        open
        onClose={() => {}}
      />,
    );
    await waitFor(() => expect(router.keys.has('sess-2|5|user:1')).toBe(true));
    await waitFor(() => expect(router.policies.has('sess-2')).toBe(true));
    // The new session's attributes land first (one pipeline beats the INFO parse).
    router.keys.get('sess-2|5|user:1')!.resolve(keyReply({ type: 'hash' }));
    await waitFor(() => expect(propsState(container)).toBe('ready'));
    expect(sideValue(container, 'type')).toBe('hash');
    // …and the jump killed the old server's answer: empty, and a *named* empty
    // state, not a blank or a stale value.
    expect(sideValue(container, 'maxmemory-policy')).toBe('');
    expect(sideFallbackKey(container, 'maxmemory-policy')).toBe('redis.keyProps.unavailable');

    router.policies.get('sess-2')!.resolve(policyReply('allkeys-lfu'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('allkeys-lfu'));

    // Exactly one ask per session on this path — the jump is not a cache miss
    // storm and the fix kept fetching (it did not "solve" BUG-005 by stopping).
    expect(router.asked).toEqual(['sess-1', 'sess-2']);
    expect(router.calls('info_filtered')).toBe(2);
    expect(router.calls('key_object_info')).toBe(2);
  });

  it('does not let a value painted before the drawer closed survive a session swap across the close', async () => {
    // Contract obligation 1: collapsing keeps the slot mounted, so the tagged
    // value is still sitting in `useState` when the session swaps underneath it.
    // Only the render-time identity check may keep `noeviction` off `sess-2`'s
    // screen on reopen; clearing-on-trigger or a bare string both fail here —
    // this is the "no bypass flag" ruling measured from the retention side.
    const relay = makeRelay();
    const router = routeCommands();
    const { container, rerender } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(router.keys.has('sess-1|5|user:1')).toBe(true));
    router.keys.get('sess-1|5|user:1')!.resolve(keyReply({ type: 'string' }));
    router.policies.get('sess-1')!.resolve(policyReply('noeviction'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('noeviction'));

    // Close the drawer: no unmount, no new commands (contract obligation 1/2).
    rerender(<RedisKeyPropsSidebar {...slotProps(relay)} open={false} onClose={() => {}} />);
    expect(container.querySelector('[data-props-state]')).toBeNull();
    expect(router.calls('info_filtered')).toBe(1);

    // Session swaps while collapsed, then the drawer reopens on the new server.
    rerender(
      <RedisKeyPropsSidebar
        {...slotProps(relay, { dbSessionId: 'sess-2' })}
        open
        onClose={() => {}}
      />,
    );
    await waitFor(() => expect(router.keys.has('sess-2|5|user:1')).toBe(true));
    await waitFor(() => expect(router.policies.has('sess-2')).toBe(true));
    router.keys.get('sess-2|5|user:1')!.resolve(keyReply({ type: 'zset' }));
    await waitFor(() => expect(propsState(container)).toBe('ready'));
    expect(sideValue(container, 'type')).toBe('zset');
    // The remembered sess-1 value is still in state — but only as a tagged one,
    // so it is invisible here. That tag-while-rendering is what is under test.
    expect(sideValue(container, 'maxmemory-policy')).toBe('');
    expect(sideFallbackKey(container, 'maxmemory-policy')).toBe('redis.keyProps.unavailable');

    router.policies.get('sess-2')!.resolve(policyReply('volatile-lru'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('volatile-lru'));
    // Closed drawer fetched nothing, reopen fetched once for the new session.
    expect(router.asked).toEqual(['sess-1', 'sess-2']);
  });

  it('serves a session it has already answered from nothing, not from memory', async () => {
    // The direction the `{ session, value }` ruling could silently rot into: a
    // `Map<session, value>` keeps every session's last answer, so hopping back to
    // `sess-1` paints `noeviction` in the same frame and needs no round trip at
    // all. A bare string fails the same assertion differently — it still carries
    // `sess-2`'s `allkeys-lfu`, which is the BUG-005 shape pointed the other way.
    // A single tagged slot can neither do this, because its one tag is `sess-2`.
    const relay = makeRelay();
    const router = routeCommands();
    const { container, rerender } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(router.keys.has('sess-1|5|user:1')).toBe(true));
    router.keys.get('sess-1|5|user:1')!.resolve(keyReply({ type: 'string' }));
    router.policies.get('sess-1')!.resolve(policyReply('noeviction'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('noeviction'));

    rerender(
      <RedisKeyPropsSidebar
        {...slotProps(relay, { dbSessionId: 'sess-2' })}
        open
        onClose={() => {}}
      />,
    );
    await waitFor(() => expect(router.policies.has('sess-2')).toBe(true));
    router.keys.get('sess-2|5|user:1')!.resolve(keyReply({ type: 'hash' }));
    router.policies.get('sess-2')!.resolve(policyReply('allkeys-lfu'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('allkeys-lfu'));

    // Back to the first server — the identity the panel is showing has answered
    // before, and that is exactly why it may not be served from before.
    rerender(
      <RedisKeyPropsSidebar
        {...slotProps(relay, { dbSessionId: 'sess-1' })}
        open
        onClose={() => {}}
      />,
    );
    await waitFor(() => expect(router.keyAsks('sess-1|5|user:1')).toBe(2));
    await waitFor(() => expect(router.asked.filter((s) => s === 'sess-1')).toHaveLength(2));
    // Still open: nothing has replied for this hop yet.
    router.keys.get('sess-1|5|user:1')!.resolve(keyReply({ type: 'stream' }));
    await waitFor(() => expect(propsState(container)).toBe('ready'));
    expect(sideValue(container, 'type')).toBe('stream');
    expect(sideValue(container, 'maxmemory-policy')).toBe('');
    expect(sideFallbackKey(container, 'maxmemory-policy')).toBe('redis.keyProps.unavailable');

    router.policies.get('sess-1')!.resolve(policyReply('volatile-ttl'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('volatile-ttl'));

    // Returning is a new read, not a cache hit: three asks for three hops.
    expect(router.asked).toEqual(['sess-1', 'sess-2', 'sess-1']);
    expect(router.calls('info_filtered')).toBe(3);
  });
});
