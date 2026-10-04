/**
 * redis-kvbar-ui — round-1 bug-fix regressions (BUG-001 … BUG-004).
 *
 * The two suites this complements already pin the happy path
 * (`kvBarSlots.test.tsx`) and the branch gaps the first test round found
 * (`kvBarSlotTesterGaps.test.tsx`, whose three `FIXME` cases the fix commits
 * un-skip). What lives here is the part a *fix* has to defend against its own
 * regressions: rules that hold inside the hook's state and therefore cannot be
 * observed through the rendered DOM, because every test renderer flushes effects
 * before asserting (the frame these rules cover is the one *between* the render
 * that moves a selection and the effect that starts the next read).
 *
 * Mutation policy for this file: each
 * identity field of a read — database session, database index, key name — is
 * pinned by its own case, so dropping any one of them from the ownership token
 * reddens exactly that case instead of leaving a silent survivor.
 *
 * Assertion policy: `data-*` markers, i18n keys and values echoed by
 * Redis only. No rendered English copy is asserted anywhere in this file.
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

import { RedisKeyPropsSidebar, RedisKvStatusBar } from '../kv-bar';
import type { KeyObjectInfo } from '../kv-bar/keyObjectInfo';
import { publishRead, type KeyReadOwner, type OwnedKeyRead } from '../kv-bar/useKeyObjectInfo';

/** Local stand-in for the host's per-panel atom (same reason as the other suites). */
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

/**
 * One deferred per `key_object_info` call, keyed by the **whole** read identity.
 *
 * Keying by session + index + name is what lets a case resolve "the reply for the
 * previous database" separately from the current one; a name-only map cannot
 * express that, which is why the shared helper in the other suites is not reused.
 */
function queueReadsByIdentity(): Map<string, Deferred<KeyObjectInfo>> {
  const pending = new Map<string, Deferred<KeyObjectInfo>>();
  commandInvoke.mockImplementation((_plugin: string, command: string, payload: unknown) => {
    if (command !== 'key_object_info') return Promise.resolve({ sections: [] });
    const { dbSessionId, dbIndex, key } = payload as {
      dbSessionId: string;
      dbIndex: number;
      key: string;
    };
    const slot = deferred<KeyObjectInfo>();
    pending.set(`${dbSessionId}|${dbIndex}|${key}`, slot);
    return slot.promise;
  });
  return pending;
}

const NOTHING = { info: null, loading: false, failed: false };

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('[fix:BUG-001] read ownership rule (publishRead, pure)', () => {
  const owner: KeyReadOwner = { dbSessionId: 'sess-1', dbIndex: 5, key: 'first' };
  const settled: OwnedKeyRead = {
    owner,
    info: info({ type: 'hash', memoryBytes: 2048 }),
    loading: false,
    failed: false,
  };

  it('publishes a read to exactly the key it was read from', () => {
    expect(publishRead(settled, { ...owner })).toEqual({
      info: settled.info,
      loading: false,
      failed: false,
    });
  });

  it('publishes nothing to any other read identity, one field at a time', () => {
    // Each case deletes one mutation target: dropping any single field from the
    // ownership token makes exactly one of these four assertions wrong.
    expect(publishRead(settled, { ...owner, key: 'second' })).toEqual(NOTHING);
    expect(publishRead(settled, { ...owner, dbIndex: 6 })).toEqual(NOTHING);
    expect(publishRead(settled, { ...owner, dbSessionId: 'sess-2' })).toEqual(NOTHING);
    expect(publishRead(settled, null)).toEqual(NOTHING);
  });

  it("does not let another key's in-flight flag reach the renderer", () => {
    // `loading` is published with the same ownership test as `info`: a borrowed
    // loading flag would name the new key `loading` before its own read exists,
    // which is the mirror image of the stale-payload bug.
    const inflight: OwnedKeyRead = { owner, info: null, loading: true, failed: false };
    expect(publishRead(inflight, { ...owner, key: 'second' })).toEqual(NOTHING);
    expect(publishRead(inflight, { ...owner })).toEqual({
      info: null,
      loading: true,
      failed: false,
    });
  });

  it('keeps a failure owned by its key', () => {
    const failed: OwnedKeyRead = { owner, info: null, loading: false, failed: true };
    expect(publishRead(failed, { ...owner })).toEqual({
      info: null,
      loading: false,
      failed: true,
    });
    expect(publishRead(failed, null)).toEqual(NOTHING);
  });

  it('publishes an empty read for no selection without inventing a state', () => {
    expect(publishRead({ owner: null, ...NOTHING }, null)).toEqual(NOTHING);
  });
});

describe('[fix:BUG-001] superseded replies (rendered through the statusBar slot)', () => {
  /**
   * Move one identity field of the read while the first reply is still in
   * flight, land the *new* reply, then land the old one out of order.
   *
   * The old reply must be discarded where it arrives: painting it would show
   * another database's key, and simply writing it to state would clear the
   * current key's `loading`/`ready` markers without showing anything.
   */
  async function outOfOrderReadLands(vary: 'dbSessionId' | 'dbIndex'): Promise<void> {
    const relay = makeRelay();
    const pending = queueReadsByIdentity();
    const { container, rerender } = render(<RedisKvStatusBar {...slotProps(relay)} />);

    act(() => relay.selectKey('same'));
    await waitFor(() => expect(pending.size).toBe(1));
    const staleId = [...pending.keys()][0]!;

    rerender(
      <RedisKvStatusBar
        {...slotProps(relay, vary === 'dbSessionId' ? { dbSessionId: 'sess-2' } : { dbIndex: 6 })}
      />,
    );
    await waitFor(() => expect(pending.size).toBe(2));
    const currentId = [...pending.keys()].find((id) => id !== staleId)!;

    pending.get(currentId)!.resolve(info({ type: 'hash', memoryBytes: 2048 }));
    await waitFor(() =>
      expect(container.querySelector('[data-part="type"]')?.textContent).toBe('hash'),
    );

    pending.get(staleId)!.resolve(info({ type: 'string', memoryBytes: 640 }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(container.querySelector('[data-part="type"]')?.textContent).toBe('hash');
    expect(container.querySelector('[data-part="size"]')?.textContent).toBe('2.0 KB');
    // The state marker has to stay `ready`: a discarded reply that still reaches
    // the state would leave the bar at `unavailable` for a key it has data for.
    expect(container.querySelector('[data-status-state]')?.getAttribute('data-status-state')).toBe(
      'ready',
    );
  }

  it('discards a reply from a database session that has been replaced', async () => {
    await outOfOrderReadLands('dbSessionId');
  });

  it('discards a reply from a database index that has been replaced', async () => {
    await outOfOrderReadLands('dbIndex');
  });

  it('publishes no known facts between selecting another key and starting its read', async () => {
    // The in-flight window itself, measured from the outside: while a read is
    // open the bar carries `loading` and *no* attribute parts at all.
    const relay = makeRelay();
    const pending = queueReadsByIdentity();
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(pending.size).toBe(1));

    expect(container.querySelector('[data-status-state]')?.getAttribute('data-status-state')).toBe(
      'loading',
    );
    expect(container.querySelector('[data-part="type"]')).toBeNull();
    expect(container.querySelector('[data-part="size"]')).toBeNull();
    expect(container.querySelector('[data-part="ttl"]')).toBeNull();

    pending.get('sess-1|5|user:1')!.resolve(info({ ttlMs: 125_000 }));
    await waitFor(() =>
      expect(
        container.querySelector('[data-status-state]')?.getAttribute('data-status-state'),
      ).toBe('ready'),
    );
    expect(container.querySelector('[data-part="size"]')?.textContent).toBe('640 B');
    expect(container.querySelector('[data-part="ttl"]')?.textContent).toBe('2m 05s');
  });
});

/** How many times a driver command was invoked. */
function readsOf(command: string): number {
  return commandInvoke.mock.calls.filter(([, name]) => name === command).length;
}

/** The keys `key_object_info` was actually asked for, in call order. */
function keysRead(): string[] {
  return commandInvoke.mock.calls
    .filter(([, name]) => name === 'key_object_info')
    .map(([, , payload]) => (payload as { key: string }).key);
}

const statusState = (container: HTMLElement) =>
  container.querySelector('[data-status-state]')?.getAttribute('data-status-state');
const propsState = (container: HTMLElement) =>
  container.querySelector('[data-props-state]')?.getAttribute('data-props-state');

describe('[fix:BUG-002] one round trip per key for the whole panel', () => {
  it('merges the status bar and sidebar read of the same key into one command', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ type: 'hash', memoryBytes: 2048 }));
    const bar = render(<RedisKvStatusBar {...slotProps(relay)} />);
    const side = render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);

    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(statusState(bar.container)).toBe('ready'));
    await waitFor(() => expect(propsState(side.container)).toBe('ready'));

    // The measured regression: this selection used to cost 2 identical
    // `key_object_info` calls (each `SELECT` + pipeline) and 3 commands overall.
    expect(keysRead()).toEqual(['user:1']);
    expect(bar.container.querySelector('[data-part="type"]')?.textContent).toBe('hash');
    expect(side.container.querySelector('[data-attr="type"] dd')?.getAttribute('data-value')).toBe(
      'hash',
    );
    expect(
      side.container.querySelector('[data-attr="memory"] dd')?.getAttribute('data-value'),
    ).toBe('2.0 KB');
  });

  it('re-reads the next key instead of serving it from the merged read', async () => {
    // The merge is an in-flight join, not a value cache: a settled reply must not
    // be reusable, so the next selection and an explicit refresh each cost a call.
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info());
    const { container } = render(
      <>
        <RedisKvStatusBar {...slotProps(relay)} />
        <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />
      </>,
    );
    act(() => relay.selectKey('first'));
    await waitFor(() => expect(statusState(container)).toBe('ready'));
    expect(keysRead()).toEqual(['first']);

    act(() => relay.selectKey('second'));
    await waitFor(() => expect(statusState(container)).toBe('ready'));
    expect(keysRead()).toEqual(['first', 'second']);

    fireEvent.click(container.querySelector('[data-testid="redis-kv-props-refresh"]')!);
    await waitFor(() => expect(readsOf('key_object_info')).toBe(3));
    expect(keysRead()).toEqual(['first', 'second', 'second']);
  });

  it('shares a failure with both slots and forgets the read afterwards', async () => {
    const relay = makeRelay();
    commandInvoke.mockRejectedValue(new Error('connection reset'));
    const bar = render(<RedisKvStatusBar {...slotProps(relay)} />);
    const side = render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);
    act(() => relay.selectKey('user:1'));

    await waitFor(() => expect(statusState(bar.container)).toBe('failed'));
    expect(propsState(side.container)).toBe('failed');
    expect(readsOf('key_object_info')).toBe(1);

    // The rejected flight has to leave with its promise: the next read of the same
    // key must issue a fresh command rather than reuse the cached rejection.
    vi.clearAllMocks();
    commandInvoke.mockResolvedValue(info({ type: 'zset' }));
    fireEvent.click(side.container.querySelector('[data-testid="redis-kv-props-refresh"]')!);
    await waitFor(() => expect(propsState(side.container)).toBe('ready'));
    expect(readsOf('key_object_info')).toBe(1);
    expect(side.container.querySelector('[data-attr="type"] dd')?.getAttribute('data-value')).toBe(
      'zset',
    );
    // `reload` is still each slot's own attempt — the bar recovers on the next
    // selection, not on the drawer's refresh. Naming that split here so a later
    // round that shares the attempt across the panel flips this assertion on
    // purpose instead of discovering it as a regression.
    expect(statusState(bar.container)).toBe('failed');
  });

  it('keeps two panels of the same connection from sharing a read scope', async () => {
    // The merge is scoped by the panel relay, so a second KV panel on the same
    // session and key still reads for itself — a plain module map keyed by the
    // read identity would answer both panels from one promise.
    const first = makeRelay();
    const second = makeRelay();
    commandInvoke.mockResolvedValue(info());
    const a = render(<RedisKvStatusBar {...slotProps(first)} />);
    const b = render(<RedisKvStatusBar {...slotProps(second)} />);

    act(() => {
      first.selectKey('shared-name');
      second.selectKey('shared-name');
    });
    await waitFor(() => expect(statusState(a.container)).toBe('ready'));
    await waitFor(() => expect(statusState(b.container)).toBe('ready'));
    expect(readsOf('key_object_info')).toBe(2);
    expect(keysRead()).toEqual(['shared-name', 'shared-name']);
  });
});

const POLICY_SECTIONS = (policy: string) => ({
  sections: [{ name: 'Memory', entries: [{ key: 'maxmemory_policy', value: policy }] }],
});

function policyValue(container: HTMLElement): string | null {
  return (
    container.querySelector('[data-attr="maxmemory-policy"] dd')?.getAttribute('data-value') ?? null
  );
}

describe('[fix:BUG-003] the refresh action covers the policy row', () => {
  it('re-reads maxmemory_policy together with the key attributes', async () => {
    const relay = makeRelay();
    commandInvoke.mockImplementation((_plugin: string, command: string) =>
      Promise.resolve(command === 'key_object_info' ? info() : POLICY_SECTIONS('noeviction')),
    );
    const { container } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(propsState(container)).toBe('ready'));
    expect(policyValue(container)).toBe('noeviction');
    expect(readsOf('info_filtered')).toBe(1);

    // Measured before the fix: `key_object_info 1->2` while `info_filtered 1->1`,
    // i.e. N clicks never re-read the eviction policy.
    fireEvent.click(container.querySelector('[data-testid="redis-kv-props-refresh"]')!);
    await waitFor(() => expect(readsOf('info_filtered')).toBe(2));
    expect(readsOf('key_object_info')).toBe(2);
    await waitFor(() => expect(propsState(container)).toBe('ready'));
  });

  it('shows the policy the server now reports, not the one from when it opened', async () => {
    const relay = makeRelay();
    let policyReads = 0;
    commandInvoke.mockImplementation((_plugin: string, command: string) => {
      if (command === 'key_object_info') return Promise.resolve(info({ freq: null }));
      policyReads += 1;
      return Promise.resolve(POLICY_SECTIONS(policyReads === 1 ? 'noeviction' : 'allkeys-lfu'));
    });
    const { container } = render(
      <RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />,
    );
    act(() => relay.selectKey('user:1'));
    await waitFor(() => expect(policyValue(container)).toBe('noeviction'));

    // The everyday flow: switch the server to LFU to explain `OBJECT FREQ`, then
    // refresh. Both rows have to move together or the panel reads as lying.
    fireEvent.click(container.querySelector('[data-testid="redis-kv-props-refresh"]')!);
    await waitFor(() => expect(policyValue(container)).toBe('allkeys-lfu'));
    expect(propsState(container)).toBe('ready');
  });
});

describe('[fix:BUG-004] the two PTTL sentinels, told apart on screen', () => {
  // `describeTtl` already split `-2` (gone) from `-1` (never expires); the fix is
  // the renderers consuming it. `kvBarSlotTesterGaps.test.tsx` pins each word for
  // one slot; what is pinned here is the part a single-slot test cannot see — that
  // the two surfaces of the *same* selection never tell opposite stories about the
  // same reply (the fix: "需与侧栏口径一致").
  function goneReads() {
    commandInvoke.mockImplementation((_plugin: string, command: string) =>
      Promise.resolve(
        command === 'key_object_info'
          ? info({ missing: false, ttlMs: -2 })
          : POLICY_SECTIONS('noeviction'),
      ),
    );
  }

  it('says "gone" in the sidebar while the status bar claims nothing about a deadline', async () => {
    const relay = makeRelay();
    goneReads();
    const bar = render(<RedisKvStatusBar {...slotProps(relay)} />);
    const side = render(<RedisKeyPropsSidebar {...slotProps(relay)} open onClose={() => {}} />);
    act(() => relay.selectKey('racing'));

    await waitFor(() => expect(propsState(side.container)).toBe('ready'));
    const dd = side.container.querySelector('[data-attr="ttl"] dd')!;
    expect(dd.getAttribute('data-fallback-key')).toBe('redis.keyProps.missing');
    expect(dd.getAttribute('data-value')).toBe('');

    await waitFor(() => expect(statusState(bar.container)).toBe('ready'));
    // The bar's wording for a gone key is silence, not a deadline it was not
    // given — and not the "no expiry" claim either, which is why no marker for
    // either state may appear here.
    expect(bar.container.querySelector('[data-part="ttl"]')).toBeNull();
    // What the pipeline *did* answer stays visible: the `-2` invalidates the TTL
    // row only, not the type/size rows read before it in the same pipeline.
    expect(bar.container.querySelector('[data-part="type"]')?.textContent).toBe('string');
  });

  it('keeps the status bar TTL part available for a key that really does expire', async () => {
    // The control for the negative assertion above: if the ttl part simply never
    // rendered, "gone ⇒ no ttl part" would pass for the wrong reason.
    const relay = makeRelay();
    commandInvoke.mockImplementation((_plugin: string, command: string) =>
      Promise.resolve(
        command === 'key_object_info'
          ? info({ missing: false, ttlMs: 125_000 })
          : POLICY_SECTIONS('noeviction'),
      ),
    );
    const bar = render(<RedisKvStatusBar {...slotProps(relay)} />);
    act(() => relay.selectKey('expiring'));

    await waitFor(() => expect(statusState(bar.container)).toBe('ready'));
    expect(bar.container.querySelector('[data-part="ttl"]')?.textContent).toBe('2m 05s');
  });
});
