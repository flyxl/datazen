/**
 * Full KV status bar (PRD §3.4 底部状态条): `db0 · 52 keys · loaded 52 ·
 * 扫描游标 0 · 选中 3 · 最后写操作 SET app:cache:session:1 (12ms)`.
 *
 * The E-track `kvBarSlots.test.tsx` covers the subset that existed then
 * (`database` / `selected-key` / type / size / TTL / dirty). This file covers
 * what W3-A's contract widening made possible and nothing else did:
 *
 * - the four relay-backed scalars (`loaded`, scan cursor, selection count, last
 *   write) render from `KvSlotState`, each with its own non-rendering arm;
 * - the `52 keys` field, which the relay deliberately does **not** carry, comes
 *   from `db_sizes` and is omitted — never zeroed — when that read fails;
 * - the **paired** scan judgement F-1 insists on: `cursor === '0'` alone must not
 *   be read as "scan finished", so `data-scan-state` is asserted across all
 *   three combinations of cursor × loaded;
 * - `db_sizes` is shared, not duplicated, between the two slots of one panel.
 *
 * Assertion policy (PRD §7-6): locators are `data-testid` / `data-*`, and `t()`
 * echoes the key with its params appended, so no case here can pin English copy.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import type { KvSlotState, KvStatusBarProps } from '@datazen/driver-sdk';
import { bindSettingsStore, type SettingsBridgeState } from '@datazen/driver-sdk';

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) =>
      params
        ? `${key} ${Object.entries(params)
            .map(([name, value]) => `${name}=${String(value)}`)
            .join(' ')}`
        : key,
  }),
}));

const commandInvoke = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  redisCommandInvoke: (...args: unknown[]) => commandInvoke(...(args as [])),
}));

import { RedisKvStatusBar } from '../kv-bar';
import type { KeyObjectInfo } from '../kv-bar/keyObjectInfo';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);

/**
 * A relay wired to every F-1 getter, with `publish` so a case can drive one
 * field at a time. Deliberately idempotent setters, like the host atom.
 */
interface RelayHarness {
  state: KvSlotState;
  publish: {
    loadedCount(n: number): void;
    scanCursor(cursor: string): void;
    scanning(next: boolean): void;
    selectionCount(n: number): void;
    write(command: string | null, durationMs: number | null): void;
    selectKey(key: string | null): void;
  };
}

function makeRelay(): RelayHarness {
  const listeners = new Set<() => void>();
  let selectedKey: string | null = null;
  let dirty = false;
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

  const state: KvSlotState = {
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

  return {
    state,
    publish: {
      loadedCount: (n) => state.setLoadedCount(n),
      scanCursor: (cursor) => state.setScanCursor(cursor),
      scanning: (next) => state.setScanning(next),
      selectionCount: (n) => state.setSelectionCount(n),
      write: (command, durationMs) => {
        if (command === null) {
          // No setter clears a write; drive the getters through a fresh relay
          // instead (a real panel's initial value is `null` anyway).
          return;
        }
        state.recordWrite(command, durationMs ?? 0);
      },
      selectKey: (key) => state.selectKey(key),
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

/** `key_object_info` for the key read; `db_sizes` for the count. */
function stubCommands({
  keyInfo = info(),
  dbSizes = [{ db: 5, keys: 52 }],
}: {
  keyInfo?: unknown;
  dbSizes?: unknown;
} = {}) {
  commandInvoke.mockImplementation((_plugin: string, command: string) => {
    if (command === 'key_object_info') return Promise.resolve(keyInfo);
    if (command === 'db_sizes') return Promise.resolve(dbSizes);
    return Promise.resolve({ sections: [] });
  });
}

/** Count calls of one command — the bar issues more than one kind now. */
function callsOf(command: string): number {
  return commandInvoke.mock.calls.filter(([, name]) => name === command).length;
}

async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('RedisKvStatusBar — the six PRD §3.4 segments', () => {
  it('renders db, keys, loaded, cursor, selection and last write together', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    publish.loadedCount(52);
    publish.scanCursor('0');
    publish.selectionCount(3);
    publish.write('SET app:cache:session:1', 12);

    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();

    const part = (name: string) => container.querySelector(`[data-part="${name}"]`);
    expect(part('database')?.textContent).toBe('db5');
    expect(part('keys')?.textContent).toContain('52');
    expect(part('keys')?.getAttribute('data-i18n-key')).toBe('redis.dbSize');
    expect(part('loaded')?.textContent).toContain('52');
    expect(part('loaded')?.getAttribute('data-i18n-key')).toBe('redis.loadedCount');
    expect(part('scan-cursor')?.getAttribute('data-scan-cursor')).toBe('0');
    expect(part('scan-cursor')?.getAttribute('data-scan-state')).toBe('done');
    expect(part('selection')?.textContent).toContain('3');
    expect(part('last-write')?.getAttribute('data-last-write')).toBe('SET app:cache:session:1');
    expect(part('last-write')?.getAttribute('data-last-write-ms')).toBe('12');
    expect(part('last-write')?.textContent).toContain('12');

    // The root mirrors the scalar facts for E2E without re-reading them.
    const bar = screen.getByTestId('redis-kv-status-bar');
    expect(bar.getAttribute('data-loaded')).toBe('52');
    expect(bar.getAttribute('data-selection-count')).toBe('3');
    expect(bar.getAttribute('data-scan-state')).toBe('done');
  });

  it('asks for the db key count through db_sizes with the documented input', async () => {
    stubCommands();
    render(<RedisKvStatusBar {...slotProps(makeRelay().state)} />);
    await settle();

    const sizeCall = commandInvoke.mock.calls.find(([, name]) => name === 'db_sizes');
    expect(sizeCall).toEqual(['redis', 'db_sizes', { dbSessionId: 'sess-1' }]);
  });

  it('picks this db out of the reply rather than summing it', async () => {
    stubCommands({
      dbSizes: [
        { db: 0, keys: 999 },
        { db: 5, keys: 52 },
        { db: 9, keys: 7 },
      ],
    });
    const { container } = render(<RedisKvStatusBar {...slotProps(makeRelay().state)} />);
    await settle();
    expect(container.querySelector('[data-part="keys"]')?.textContent).toContain('52');
  });
});

describe('RedisKvStatusBar — scanStateOf pairs the cursor with the loaded count', () => {
  it('never reads cursor 0 alone as a finished scan (F-1)', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    publish.scanCursor('0');
    // Nothing loaded: the fresh-panel snapshot. This is the exact state F-1
    // warns about — `'0'` here means "untouched", not "wrapped".
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();

    const line = container.querySelector('[data-part="scan-cursor"]');
    expect(line?.getAttribute('data-scan-state')).toBe('idle');
    expect(container.querySelector('[data-status-state]')?.getAttribute('data-status-state')).toBe(
      'no-key',
    );
  });

  it('reports done once something was loaded and the cursor wrapped', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    publish.scanCursor('0');
    publish.loadedCount(52);
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();
    expect(
      container.querySelector('[data-part="scan-cursor"]')?.getAttribute('data-scan-state'),
    ).toBe('done');
  });

  it('reports a non-zero cursor with loaded keys as partial, not idle', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    publish.scanCursor('17');
    publish.loadedCount(200);
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();

    const line = container.querySelector('[data-part="scan-cursor"]');
    expect(line?.getAttribute('data-scan-state')).toBe('stopped');
    expect(line?.getAttribute('data-i18n-key')).toBe('redis.contextBar.status.scanStopped');
    expect(line?.textContent).toContain('17');
  });

  it('is idle for a non-zero cursor when nothing was loaded', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    publish.scanCursor('17');
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();
    expect(
      container.querySelector('[data-part="scan-cursor"]')?.getAttribute('data-scan-state'),
    ).toBe('idle');
  });

  it('follows the relay live across the whole state machine', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();
    const bar = () => screen.getByTestId('redis-kv-status-bar');
    expect(bar().getAttribute('data-scan-state')).toBe('idle');

    act(() => {
      publish.scanning(true);
      publish.scanCursor('9');
      publish.loadedCount(30);
    });
    expect(bar().getAttribute('data-scan-state')).toBe('stopped');
    expect(bar().getAttribute('data-loaded')).toBe('30');

    act(() => {
      publish.scanning(false);
      publish.scanCursor('0');
      publish.loadedCount(52);
    });
    expect(bar().getAttribute('data-scan-state')).toBe('done');
  });
});

describe('RedisKvStatusBar — every optional field disappears instead of showing 0', () => {
  it('shows no key count while db_sizes is still unknown', async () => {
    let release: ((value: unknown) => void) | null = null;
    commandInvoke.mockImplementation((_plugin: string, command: string) => {
      if (command === 'db_sizes') {
        return new Promise((resolve) => {
          release = resolve;
        });
      }
      return Promise.resolve(info());
    });
    const { container } = render(<RedisKvStatusBar {...slotProps(makeRelay().state)} />);
    await settle();

    // In flight ⇒ no count, and crucially no `0 keys` placeholder.
    expect(container.querySelector('[data-part="keys"]')).toBeNull();

    await act(async () => {
      release?.([{ db: 5, keys: 52 }]);
      await Promise.resolve();
    });
    expect(container.querySelector('[data-part="keys"]')?.textContent).toContain('52');
  });

  it('omits the key count when db_sizes fails (an ACL, an offline server)', async () => {
    commandInvoke.mockImplementation((_plugin: string, command: string) =>
      command === 'db_sizes' ? Promise.reject(new Error('NOPERM')) : Promise.resolve(info()),
    );
    const { container } = render(<RedisKvStatusBar {...slotProps(makeRelay().state)} />);
    await settle();

    expect(container.querySelector('[data-part="keys"]')).toBeNull();
    // The rest of the bar is intact — one refused command must not blank it.
    expect(container.querySelector('[data-part="loaded"]')).not.toBeNull();
    expect(container.querySelector('[data-part="scan-cursor"]')).not.toBeNull();
  });

  it('omits the key count when the reply does not cover this db', async () => {
    stubCommands({ dbSizes: [{ db: 0, keys: 4 }] });
    const { container } = render(<RedisKvStatusBar {...slotProps(makeRelay().state)} />);
    await settle();
    expect(container.querySelector('[data-part="keys"]')).toBeNull();
  });

  it('survives a malformed db_sizes reply without crashing or zeroing', async () => {
    stubCommands({ dbSizes: 'not-an-array' });
    const { container } = render(<RedisKvStatusBar {...slotProps(makeRelay().state)} />);
    await settle();
    expect(container.querySelector('[data-part="keys"]')).toBeNull();
    expect(screen.getByTestId('redis-kv-status-bar')).not.toBeNull();
  });

  it('renders loaded 0 — a real measurement — but no selection and no write', async () => {
    stubCommands();
    const { container } = render(<RedisKvStatusBar {...slotProps(makeRelay().state)} />);
    await settle();

    // Zero loaded keys on an empty db is a fact the user asked for.
    expect(container.querySelector('[data-part="loaded"]')?.textContent).toContain('0');
    // Zero multi-selection is not: no pill, no `0 selected`.
    expect(container.querySelector('[data-part="selection"]')).toBeNull();
    // No write has happened yet, so there is no write segment at all.
    expect(container.querySelector('[data-part="last-write"]')).toBeNull();
    expect(container.textContent ?? '').not.toContain('nullms');
  });

  it('drops the write segment for a fresh panel whose write is null', async () => {
    stubCommands();
    const { state } = makeRelay();
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();
    expect(container.querySelector('[data-part="last-write"]')).toBeNull();
  });

  it('shows the command without a duration when the round trip is unknown', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    // A driver that knows the command but not its timing writes ms 0; the bar
    // must still name the command rather than print a bogus `(0ms)`.
    publish.write('DEL user:1', 0);
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();

    const write = container.querySelector('[data-part="last-write"]');
    expect(write?.getAttribute('data-last-write')).toBe('DEL user:1');
    expect(write?.textContent).toContain('DEL user:1');
  });

  it('keeps loaded / cursor / selection when no db index is resolvable', async () => {
    stubCommands();
    const { state, publish } = makeRelay();
    publish.loadedCount(4);
    publish.selectionCount(2);
    const { container } = render(
      <RedisKvStatusBar {...slotProps(state, { database: null, dbIndex: undefined })} />,
    );
    await settle();

    // No db index ⇒ no db_sizes read, hence no count.
    expect(callsOf('db_sizes')).toBe(0);
    expect(container.querySelector('[data-part="keys"]')).toBeNull();
    expect(container.querySelector('[data-part="database"]')?.textContent).toBe('—');
    // The relay facts do not depend on the db index, so they still render.
    expect(container.querySelector('[data-part="loaded"]')?.textContent).toContain('4');
    expect(container.querySelector('[data-part="selection"]')?.textContent).toContain('2');
  });

  it('does not read db_sizes without a session', async () => {
    stubCommands();
    render(<RedisKvStatusBar {...slotProps(makeRelay().state, { dbSessionId: '' })} />);
    await settle();
    expect(callsOf('db_sizes')).toBe(0);
    expect(screen.getByTestId('redis-kv-status-bar')).not.toBeNull();
  });
});

describe('RedisKvStatusBar — the db_sizes read is shared per panel (BUG-002 rule)', () => {
  it('costs one command for two reads of the same panel', async () => {
    stubCommands();
    const { state } = makeRelay();
    // Mounting the bar twice on one relay models the sidebar + bar pair, which
    // sit in different subtrees and would otherwise each pay for `db_sizes`.
    render(
      <>
        <RedisKvStatusBar {...slotProps(state)} />
        <RedisKvStatusBar {...slotProps(state, { connectionId: 'conn-2' })} />
      </>,
    );
    await settle();
    expect(callsOf('db_sizes')).toBe(1);
  });

  it('does not reuse a settled read for a panel that was reopened', async () => {
    stubCommands();
    const first = makeRelay();
    const view = render(<RedisKvStatusBar {...slotProps(first.state)} />);
    await settle();
    expect(callsOf('db_sizes')).toBe(1);

    view.unmount();
    const second = makeRelay();
    render(<RedisKvStatusBar {...slotProps(second.state)} />);
    await settle();

    // A new panel is a new relay ⇒ a fresh read; serving the old promise would
    // show a count the server may have changed (F-2 rejected slot-side caches).
    expect(callsOf('db_sizes')).toBe(2);
  });

  it('re-reads after the session is swapped', async () => {
    stubCommands();
    const { state } = makeRelay();
    const { rerender } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();
    expect(callsOf('db_sizes')).toBe(1);

    rerender(<RedisKvStatusBar {...slotProps(state, { dbSessionId: 'sess-2' })} />);
    await settle();
    expect(callsOf('db_sizes')).toBe(2);
  });

  it('never paints the previous session\u2019s count after a swap', async () => {
    commandInvoke.mockImplementation((_plugin: string, command: string, args: unknown) => {
      if (command !== 'db_sizes') return Promise.resolve(info());
      const { dbSessionId } = args as { dbSessionId: string };
      return Promise.resolve([{ db: 5, keys: dbSessionId === 'sess-1' ? 52 : 7 }]);
    });
    const { state } = makeRelay();
    const { container, rerender } = render(<RedisKvStatusBar {...slotProps(state)} />);
    await settle();
    expect(container.querySelector('[data-part="keys"]')?.textContent).toContain('52');

    rerender(<RedisKvStatusBar {...slotProps(state, { dbSessionId: 'sess-2' })} />);
    await waitFor(() =>
      expect(container.querySelector('[data-part="keys"]')?.textContent).toContain('7'),
    );
    expect(container.querySelector('[data-part="keys"]')?.textContent).not.toContain('52');
  });
});

describe('RedisKvStatusBar — the E-track fields are untouched by the widening', () => {
  it('still merges the key read with the sidebar and keeps per-field degradation', async () => {
    stubCommands({ keyInfo: info({ type: null, memoryBytes: null, ttlMs: -1 }) });
    const { state, publish } = makeRelay();
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    act(() => publish.selectKey('user:1'));
    await settle();

    expect(container.querySelector('[data-part="selected-key"]')?.textContent).toBe('user:1');
    // Null attributes stay unrendered, exactly as before.
    expect(container.querySelector('[data-part="type"]')).toBeNull();
    expect(container.querySelector('[data-part="size"]')).toBeNull();
    expect(container.querySelector('[data-part="ttl"]')).toBeNull();
    expect(callsOf('key_object_info')).toBe(1);
  });

  it('renders type / size / ttl for a normal key alongside the new segments', async () => {
    stubCommands({ keyInfo: info({ type: 'hash', memoryBytes: 2048, ttlMs: 125_000 }) });
    const { state, publish } = makeRelay();
    publish.loadedCount(9);
    const { container } = render(<RedisKvStatusBar {...slotProps(state)} />);
    act(() => publish.selectKey('user:1'));
    await settle();

    expect(container.querySelector('[data-part="type"]')?.textContent).toBe('hash');
    expect(container.querySelector('[data-part="size"]')?.textContent).toBe('2.0 KB');
    expect(container.querySelector('[data-part="ttl"]')?.textContent).toBe('2m 05s');
    // And the new fields coexist rather than replacing them.
    expect(container.querySelector('[data-part="loaded"]')?.textContent).toContain('9');
    expect(container.querySelector('[data-part="keys"]')?.textContent).toContain('52');
  });
});
