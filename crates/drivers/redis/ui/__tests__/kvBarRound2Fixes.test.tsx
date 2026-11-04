/**
 * redis-kvbar-ui — round-2 fix regression battery (`redis-kvbar-ui-BUG-005`).
 *
 * BUG-005 is the session-dimension leftover of BUG-001: the
 * eviction-policy row is read through `invokeMaxmemoryPolicy(dbSessionId)`, so the
 * value belongs to a *session*, but the row's state used to be a bare string that
 * only a new reply could overwrite. After a session swap the sidebar therefore
 * showed the new server's attributes next to the old server's policy.
 *
 * The fix tags the value with the session that produced it and filters while
 * rendering — the same owner-token shape `useKeyObjectInfo` uses for the key
 * dimension, so no cache layer was added and no "remember to clear it" flag exists.
 * That leaves two directions a later change can regress, and each has a case here:
 *
 *  1. **too little invalidation** — a reply from a session that is no longer
 *     current, or a session the panel never asked about, must not fill the row;
 *  2. **too much** — `attempt` (the refresh trigger, redis-kvbar-ui-BUG-003) and
 *     `dbIndex` are *not* invalidation axes: a same-session re-read keeps showing
 *     the fact this session already gave, and a database switch inside one session
 *     is not a new server, so it must neither clear the row nor cost a round trip.
 *
 * The two arms of the swap itself are pinned in `kvBarRound2Tester.test.tsx`
 * (un-skipped BUG-005 case + its partner); this file does not repeat them.
 *
 * Assertion policy (PRD §7-6): `data-*` markers, i18n keys and values echoed by
 * Redis (`noeviction`, `allkeys-lfu`, `hash`) only. No rendered English copy.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
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

type PolicyReply = {
  sections: Array<{ name: string; entries: Array<{ key: string; value: string }> }>;
};

const policyReply = (value: string): PolicyReply => ({
  sections: [{ name: 'Memory', entries: [{ key: 'maxmemory_policy', value }] }],
});

interface Router {
  /** One slot per in-flight read; a repeated identity overwrites, i.e. the newest ask. */
  keys: Map<string, Deferred<KeyObjectInfo>>;
  policies: Map<string, Deferred<PolicyReply>>;
  sessionsAsked(): string[];
  calls(command: string): number;
}

function routeCommands(): Router {
  const keys = new Map<string, Deferred<KeyObjectInfo>>();
  const policies = new Map<string, Deferred<PolicyReply>>();
  commandInvoke.mockImplementation((_plugin: string, command: string, payload: unknown) => {
    const body = payload as { dbSessionId?: string; dbIndex?: number; key?: string };
    if (command === 'key_object_info') {
      const slot = deferred<KeyObjectInfo>();
      keys.set(`${body.dbSessionId}|${body.dbIndex}|${body.key}`, slot);
      return slot.promise;
    }
    if (command === 'info_filtered') {
      const slot = deferred<PolicyReply>();
      policies.set(`${body.dbSessionId}`, slot);
      return slot.promise;
    }
    return Promise.resolve({ sections: [] });
  });
  return {
    keys,
    policies,
    calls: (command) => commandInvoke.mock.calls.filter(([, name]) => name === command).length,
    sessionsAsked: () =>
      commandInvoke.mock.calls
        .filter(([, name]) => name === 'info_filtered')
        .map(([, , payload]) => (payload as { dbSessionId: string }).dbSessionId),
  };
}

const propsState = (el: HTMLElement | null) =>
  el?.querySelector('[data-props-state]')?.getAttribute('data-props-state') ?? null;
/** `null` while the attribute list is not mounted at all, `''` for its empty state. */
const sideValue = (el: HTMLElement | null, attr: string) =>
  el?.querySelector(`[data-attr="${attr}"] dd`)?.getAttribute('data-value') ?? null;
const refreshButton = (el: HTMLElement | null) =>
  el?.querySelector<HTMLButtonElement>('[data-testid="redis-kv-props-refresh"]') ?? null;

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('[fix:BUG-005] the eviction-policy row is scoped to its session', () => {
  it('keeps the current session policy on screen while a refresh re-read is open', async () => {
    // Direction (2), the over-correction side. `attempt` must not touch the tag:
    // blanking the row on every trigger would look like a fix for BUG-005 while
    // throwing away BUG-003's ruling that a re-read keeps the fact this very
    // session already gave. The key read answers first (one pipeline), so the row
    // really is painted while `info_filtered` is still open — that is the frame.
    const relay = makeRelay();
    const router = routeCommands();
    const { container } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(router.keys.has('sess-1|5|user:1')).toBe(true));
    router.keys.get('sess-1|5|user:1')!.resolve(info({ type: 'string' }));
    router.policies.get('sess-1')!.resolve(policyReply('noeviction'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('noeviction'));

    fireEvent.click(refreshButton(container)!);
    await waitFor(() => expect(router.calls('info_filtered')).toBe(2));
    // The new key attributes have landed; the new policy read has not.
    router.keys.get('sess-1|5|user:1')!.resolve(info({ type: 'hash', memoryBytes: 2048 }));
    await waitFor(() => expect(propsState(container)).toBe('ready'));
    expect(sideValue(container, 'type')).toBe('hash');
    expect(sideValue(container, 'maxmemory-policy')).toBe('noeviction');

    // …and the re-read is not a no-op wearing a retention flag.
    router.policies.get('sess-1')!.resolve(policyReply('allkeys-lfu'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('allkeys-lfu'));
  });

  it('asks each session it lands on and never answers from a remembered value', async () => {
    // Direction (1) seen from the "did the fix become a cache?" angle. A
    // `Map<session, value>` would paint `noeviction` on the third hop at once,
    // since some earlier session answered that way; a session-tagged value cannot,
    // because the tag it carries is `sess-2`'s. Round trips are counted too, so a
    // fix that stopped asking after the first session is caught as well.
    const relay = makeRelay();
    const router = routeCommands();
    const { container, rerender } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    const swapTo = async (sessionId: string, policy: string | null) => {
      rerender(
        <RedisKeyPropsSidebar
          {...slotProps(relay, { dbSessionId: sessionId })}
          open
          onClose={() => {}}
        />,
      );
      await waitFor(() => expect(router.keys.has(`${sessionId}|5|user:1`)).toBe(true));
      await waitFor(() => expect(router.policies.has(sessionId)).toBe(true));
      router.keys.get(`${sessionId}|5|user:1`)!.resolve(info({ type: 'hash' }));
      await waitFor(() => expect(propsState(container)).toBe('ready'));
      // No reply for *this* session yet ⇒ the row is empty, whatever was known before.
      expect(sideValue(container, 'maxmemory-policy')).toBe('');
      if (policy !== null) {
        router.policies.get(sessionId)!.resolve(policyReply(policy));
        await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe(policy));
      }
    };

    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(router.keys.has('sess-1|5|user:1')).toBe(true));
    router.keys.get('sess-1|5|user:1')!.resolve(info({ type: 'string' }));
    router.policies.get('sess-1')!.resolve(policyReply('noeviction'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('noeviction'));

    await swapTo('sess-2', 'allkeys-lfu');
    await swapTo('sess-3', null);

    expect(router.sessionsAsked()).toEqual(['sess-1', 'sess-2', 'sess-3']);
    expect(router.calls('info_filtered')).toBe(3);
  });

  it('is not invalidated by a database switch inside the same session', async () => {
    // Direction (2) again, on the axis BUG-001 *does* key on: `dbIndex`. One server
    // answers `INFO memory` for all of its databases, so a `SELECT`-level switch
    // must leave this row alone — clearing or re-reading here would trade a
    // round trip per key-tree click for nothing, and is the shape a
    // "key the tag like the key read" over-fix would produce.
    const relay = makeRelay();
    const router = routeCommands();
    const { container, rerender } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(router.keys.has('sess-1|5|user:1')).toBe(true));
    router.keys.get('sess-1|5|user:1')!.resolve(info({ type: 'string' }));
    router.policies.get('sess-1')!.resolve(policyReply('volatile-lru'));
    await waitFor(() => expect(sideValue(container, 'maxmemory-policy')).toBe('volatile-lru'));
    relay.setDirty(true);

    rerender(
      <RedisKeyPropsSidebar {...slotProps(relay, { dbIndex: 7 })} open onClose={() => {}} />,
    );
    await waitFor(() => expect(router.keys.has('sess-1|7|user:1')).toBe(true));
    router.keys.get('sess-1|7|user:1')!.resolve(info({ type: 'zset' }));
    await waitFor(() => expect(propsState(container)).toBe('ready'));

    expect(sideValue(container, 'type')).toBe('zset');
    expect(sideValue(container, 'maxmemory-policy')).toBe('volatile-lru');
    expect(router.sessionsAsked()).toEqual(['sess-1']);
    expect(router.calls('info_filtered')).toBe(1);
  });
});
