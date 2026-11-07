/**
 * KV workspace slot tests — `statusBar` and `keyPropsSidebar` (PRD §3.4, 8-3 = M).
 *
 * Both slots are read-only consumers of the host's per-panel `KvSlotState`
 * relay (contract F-2): the workbench publishes, the slots render. A driver test
 * cannot import the host atom factory (boundary guard R1), so a local copy of
 * the relay is used — the contract under test is the `KvSlotState` *type*, not
 * the host implementation of it.
 *
 * Assertion policy (PRD §7-6): nothing here quotes translated copy. Locators are
 * `data-testid` / `data-*`, state is asserted through `data-props-state`,
 * `data-status-state`, `data-value` and `data-fallback-key` (which carry i18n
 * *keys*, not their English values). Values echoed from Redis — a type name, a
 * byte count — are server data and are asserted directly.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { KvSlotState, KvStatusBarProps } from '@datazen/driver-sdk';

// Identity `t()`: rendered text equals the i18n key, so no assertion can ever
// pin a translation by accident.
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
import {
  attributeViewState,
  describeTtl,
  formatDurationMs,
  formatIdle,
  invokeKeyObjectInfo,
  invokeMaxmemoryPolicy,
  type KeyObjectInfo,
} from '../kv-bar/keyObjectInfo';

/** Local stand-in for the host's per-panel atom (see header note). */
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

/** A full, LFU-less reply — the shape a stock `noeviction` server returns. */
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

const LFU_POLICY = {
  sections: [{ name: 'Memory', entries: [{ key: 'maxmemory_policy', value: 'allkeys-lfu' }] }],
};

/**
 * The panel-slot base props the host freezes (contract F-2). Typed through
 * `KvStatusBarProps` so a drift in the frozen shape fails type-checking here
 * rather than at runtime; `databaseType` is the host's own union, which a driver
 * must not import, hence the cast through the prop type itself.
 */
function slotProps(state: KvSlotState): KvStatusBarProps {
  return {
    connectionId: 'conn-1',
    dbSessionId: 'sess-1',
    connectionName: 'local',
    databaseType: 'redis' as KvStatusBarProps['databaseType'],
    database: 'db5',
    dbIndex: 5,
    state,
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('key_object_info client (pure)', () => {
  it('separates the three PTTL meanings', () => {
    expect(describeTtl(-2)).toEqual({ kind: 'missing' });
    expect(describeTtl(-1)).toEqual({ kind: 'no-expiry' });
    expect(describeTtl(-7)).toEqual({ kind: 'no-expiry' });
    expect(describeTtl(0)).toEqual({ kind: 'remaining', ms: 0 });
    expect(describeTtl(60_000)).toEqual({ kind: 'remaining', ms: 60_000 });
  });

  it('formats durations with two significant units', () => {
    expect(formatDurationMs(500)).toBe('500ms');
    expect(formatDurationMs(12_000)).toBe('12s');
    expect(formatDurationMs(184_000)).toBe('3m 04s');
    expect(formatDurationMs(3_720_000)).toBe('1h 2m');
    expect(formatDurationMs(90_060_000)).toBe('1d 1h');
    expect(formatDurationMs(-1)).toBe('—');
    expect(formatIdle(null)).toBe('—');
    expect(formatIdle(90)).toBe('1m 30s');
  });

  it('names every render branch of the attribute read', () => {
    expect(attributeViewState(null, false, false, null)).toBe('no-key');
    expect(attributeViewState('k', true, false, null)).toBe('loading');
    expect(attributeViewState('k', false, true, null)).toBe('failed');
    expect(attributeViewState('k', false, false, info({ missing: true }))).toBe('missing');
    expect(attributeViewState('k', false, false, info())).toBe('ready');
    // A key is selected but nothing could be read at all (e.g. no db index).
    expect(attributeViewState('k', false, false, null)).toBe('unavailable');
  });

  it('reads one key through the documented command and input', async () => {
    commandInvoke.mockResolvedValue(info());
    const result = await invokeKeyObjectInfo('sess-1', 5, 'user:1');
    expect(commandInvoke).toHaveBeenCalledWith('redis', 'key_object_info', {
      dbSessionId: 'sess-1',
      dbIndex: 5,
      key: 'user:1',
    });
    expect(result.type).toBe('string');
  });

  it('takes the eviction policy from INFO and degrades it to null', async () => {
    commandInvoke.mockResolvedValue(LFU_POLICY);
    await expect(invokeMaxmemoryPolicy('sess-1')).resolves.toBe('allkeys-lfu');
    commandInvoke.mockResolvedValue({ sections: 'not-an-array' });
    await expect(invokeMaxmemoryPolicy('sess-1')).resolves.toBeNull();
    commandInvoke.mockResolvedValue('a bare string');
    await expect(invokeMaxmemoryPolicy('sess-1')).resolves.toBeNull();
    commandInvoke.mockRejectedValue(new Error('no permission'));
    await expect(invokeMaxmemoryPolicy('sess-1')).resolves.toBeNull();
  });
});

describe('RedisKvStatusBar (statusBar slot)', () => {
  it('renders the no-key state and issues no key read without a selection', () => {
    const relay = makeRelay();
    render(<RedisKvStatusBar {...slotProps(relay)} />);
    const bar = screen.getByTestId('redis-kv-status-bar');
    expect(bar.getAttribute('data-status-state')).toBe('no-key');
    expect(bar.getAttribute('data-selected-key')).toBe('');
    expect(bar.querySelector('[data-part="selected-key"]')).not.toBeNull();
    // Narrowed by the Wave-4 full status bar. What this case has always been
    // about — selection drives the *key* read, so no selection means no
    // `key_object_info` traffic — is asserted directly now, because the bar
    // additionally reads `db_sizes` for the PRD §3.4 `52 keys` field, which by
    // design does not depend on any selection. The old blanket
    // `not.toHaveBeenCalled()` pinned "the bar issues exactly zero commands",
    // an E-track statement of the pre-widening contract that the widened
    // contract deliberately replaces.
    expect(commandInvoke.mock.calls.filter(([, name]) => name === 'key_object_info')).toEqual([]);
    expect(commandInvoke.mock.calls.map(([, name]) => name)).toEqual(['db_sizes']);
  });

  it('follows the relay: selection alone drives the read', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ type: 'hash', memoryBytes: 2048 }));
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);

    act(() => relay.selectKey('user:1'));
    // One wait, one render root (redis-kvbar-ui-BUG-006). `selected-key` is printed
    // straight from the relay, so it is already true on the click frame; the `type`
    // part only exists once the reply has landed (≥3 extra microtask hops in
    // useKeyObjectInfo.ts). Waiting for the first and then bare-asserting the second
    // left a legal empty window — that is the race this gate used to be exposed to.
    // The status-bar root carries `data-status-state` *and* the parts, both computed
    // from the same `info` in one render pass, so no frame can pass this check by
    // accident; `ready` is pinned too, which the old form never asserted.
    await waitFor(() => {
      const bar = container.querySelector('[data-status-state]');
      expect(bar?.getAttribute('data-status-state')).toBe('ready');
      expect(bar?.querySelector('[data-part="selected-key"]')?.textContent).toBe('user:1');
      expect(bar?.querySelector('[data-part="type"]')?.textContent).toBe('hash');
    });
    expect(commandInvoke).toHaveBeenCalledWith('redis', 'key_object_info', {
      dbSessionId: 'sess-1',
      dbIndex: 5,
      key: 'user:1',
    });
  });

  it('keeps the database label from the host props', () => {
    const relay = makeRelay();
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);
    expect(container.querySelector('[data-part="database"]')?.textContent).toBe('db5');
  });

  it('shows a TTL only when the key actually expires', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ ttlMs: 125_000, memoryBytes: 1536 }));
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);
    act(() => relay.selectKey('session:1'));
    await waitFor(() =>
      expect(container.querySelector('[data-part="size"]')?.textContent).toBe('1.5 KB'),
    );
    expect(container.querySelector('[data-part="ttl"]')?.textContent).toBe('2m 05s');

    vi.clearAllMocks();
    const other = makeRelay();
    commandInvoke.mockResolvedValue(info({ ttlMs: -1 }));
    const second = render(<RedisKvStatusBar {...slotProps(other)} />);
    act(() => other.selectKey('forever'));
    await waitFor(() =>
      expect(second.container.querySelector('[data-part="size"]')).not.toBeNull(),
    );
    expect(second.container.querySelector('[data-part="ttl"]')).toBeNull();
  });

  it('renders an expired key as a state, not as an error', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(
      info({ missing: true, type: null, memoryBytes: null, ttlMs: -2 }),
    );
    const bar = render(<RedisKvStatusBar {...slotProps(relay)} />);
    act(() => relay.selectKey('gone'));
    await waitFor(() =>
      expect(
        bar.container.querySelector('[data-status-state]')?.getAttribute('data-status-state'),
      ).toBe('missing'),
    );
    expect(bar.container.querySelector('[data-part="type"]')).toBeNull();
    expect(bar.container.querySelector('[data-part="size"]')).toBeNull();
  });

  it('mirrors the relay dirty flag without a re-render from the driver', async () => {
    const relay = makeRelay();
    const { container } = render(<RedisKvStatusBar {...slotProps(relay)} />);
    expect(container.querySelector('[data-dirty="true"]')).toBeNull();
    act(() => relay.setDirty(true));
    expect(screen.getByTestId('redis-kv-status-bar').getAttribute('data-dirty')).toBe('true');
    const badge = container.querySelector('[data-part="dirty"]');
    expect(badge?.getAttribute('data-i18n-key')).toBe('redis.contextBar.status.unsaved');
    act(() => relay.setDirty(false));
    expect(container.querySelector('[data-part="dirty"]')).toBeNull();
  });
});

describe('RedisKeyPropsSidebar (keyPropsSidebar slot)', () => {
  function sidebar(state: KvSlotState, open: boolean, onClose = vi.fn()) {
    return render(<RedisKeyPropsSidebar {...slotProps(state)} open={open} onClose={onClose} />);
  }

  it('renders nothing while collapsed, and fetches nothing (host keeps it mounted)', () => {
    const relay = makeRelay();
    relay.selectKey('user:1');
    const { container } = sidebar(relay, false);
    expect(container.firstChild).toBeNull();
    expect(commandInvoke).not.toHaveBeenCalled();
  });

  it('names the no-key state while open', () => {
    const relay = makeRelay();
    sidebar(relay, true);
    const aside = screen.getByTestId('redis-kv-key-props-sidebar');
    expect(aside.getAttribute('data-props-state')).toBe('no-key');
    expect(aside.querySelector('[data-i18n-key="redis.keyProps.noKeyHint"]')).not.toBeNull();
    expect(aside.querySelectorAll('dt')).toHaveLength(0);
    expect(aside.querySelector('[data-attr="type"]')).toBeNull();
  });

  it('lists every attribute of the selected key, with per-field empty states', async () => {
    const relay = makeRelay();
    commandInvoke.mockImplementation((_plugin: string, command: string) =>
      Promise.resolve(command === 'key_object_info' ? info() : LFU_POLICY),
    );
    sidebar(relay, true);
    act(() => relay.selectKey('user:1'));

    await waitFor(() =>
      expect(
        screen.getByTestId('redis-kv-key-props-sidebar').getAttribute('data-props-state'),
      ).toBe('ready'),
    );
    const aside = screen.getByTestId('redis-kv-key-props-sidebar');
    expect(aside.getAttribute('data-selected-key')).toBe('user:1');
    expect(aside.querySelector('[data-attr="type"] dd')?.getAttribute('data-value')).toBe('string');
    // A sub-KB key keeps byte precision instead of rounding down to 0 KB.
    expect(aside.querySelector('[data-attr="memory"] dd')?.getAttribute('data-value')).toBe(
      '640 B',
    );
    expect(aside.querySelector('[data-attr="encoding"] dd')?.getAttribute('data-value')).toBe(
      'embstr',
    );
    expect(aside.querySelector('[data-attr="idle"] dd')?.getAttribute('data-value')).toBe('1m 30s');
    // PTTL -1 is an answer with its own word, not a missing measurement.
    expect(aside.querySelector('[data-attr="ttl"] dd')?.getAttribute('data-fallback-key')).toBe(
      'redis.noExpiry',
    );
    // `OBJECT FREQ` rejected on a non-LFU server: named empty state, never 0.
    expect(aside.querySelector('[data-attr="freq"] dd')?.getAttribute('data-fallback-key')).toBe(
      'redis.keyProps.freqUnavailable',
    );
    expect(
      aside.querySelector('[data-attr="maxmemory-policy"] dd')?.getAttribute('data-value'),
    ).toBe('allkeys-lfu');
    // Labels are addressed by i18n key so copy can change without touching tests.
    expect(aside.querySelector('[data-attr="encoding"] dt')?.getAttribute('data-i18n-key')).toBe(
      'redis.keyProps.encoding',
    );
  });

  it('shows the expired-key empty state instead of an error', async () => {
    const relay = makeRelay();
    commandInvoke.mockResolvedValue(info({ missing: true, ttlMs: -2 }));
    sidebar(relay, true);
    act(() => relay.selectKey('gone'));
    await waitFor(() =>
      expect(
        screen.getByTestId('redis-kv-key-props-sidebar').getAttribute('data-props-state'),
      ).toBe('missing'),
    );
    const aside = screen.getByTestId('redis-kv-key-props-sidebar');
    expect(aside.querySelector('[data-i18n-key="redis.keyProps.missing"]')).not.toBeNull();
    expect(aside.querySelector('[data-attr="type"]')).toBeNull();
  });

  it('names a command failure as its own state', async () => {
    const relay = makeRelay();
    commandInvoke.mockImplementation((_plugin: string, command: string) =>
      command === 'key_object_info'
        ? Promise.reject(new Error('connection reset'))
        : Promise.resolve(LFU_POLICY),
    );
    sidebar(relay, true);
    act(() => relay.selectKey('user:1'));
    await waitFor(() =>
      expect(
        screen.getByTestId('redis-kv-key-props-sidebar').getAttribute('data-props-state'),
      ).toBe('failed'),
    );
    expect(
      screen.getByTestId('redis-kv-key-props-sidebar').querySelector('[data-attr="memory"]'),
    ).toBeNull();
  });

  it('re-reads on refresh and closes through the host toggle', async () => {
    const relay = makeRelay();
    const onClose = vi.fn();
    commandInvoke.mockResolvedValue(info());
    sidebar(relay, true, onClose);
    act(() => relay.selectKey('user:1'));
    await waitFor(() =>
      expect(
        screen.getByTestId('redis-kv-key-props-sidebar').getAttribute('data-props-state'),
      ).toBe('ready'),
    );
    const readsBefore = commandInvoke.mock.calls.filter(
      ([, command]) => command === 'key_object_info',
    ).length;

    fireEvent.click(screen.getByTestId('redis-kv-props-refresh'));
    await waitFor(() =>
      expect(
        commandInvoke.mock.calls.filter(([, command]) => command === 'key_object_info').length,
      ).toBeGreaterThan(readsBefore),
    );
    fireEvent.click(screen.getByTestId('redis-kv-props-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
