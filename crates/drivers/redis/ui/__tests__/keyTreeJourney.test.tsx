/**
 * 键树 track journey tests (PRD §3.2 R1/R2, §4 I-9 in part).
 *
 * Assertion policy (PRD §7-6 / ruling 8-4, same as the kv-slot tests):
 *   - `useI18n` is stubbed with an identity `t`, so a rendered string equals its
 *     i18n key; **no test reads copy** — locators are `data-testid`, state is read
 *     from `data-*` attributes or from the recorded command calls;
 *   - the virtualizer is stubbed to yield every row, because jsdom measures a
 *     zero-size scroll element and would mount none ("click a row" untestable).
 *
 * The journeys are keystroke-by-keystroke on purpose (AGENTS.md 连续旅程测试):
 * intermediate states (empty input, half-typed pattern, glob-vs-literal, chip on
 * then off) and the exit transitions are asserted, not just the final legal one.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSchemaStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SchemaStoreState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (opts: { count: number; estimateSize: () => number }) => {
    const size = opts.estimateSize();
    const items = Array.from({ length: opts.count }, (_, index) => ({
      index,
      key: index,
      start: index * size,
      size,
      lane: 0,
    }));
    return {
      getVirtualItems: () => items,
      getTotalSize: () => items.length * size,
      measureElement: () => undefined,
      scrollToOffset: () => undefined,
      scrollToIndex: () => undefined,
    };
  },
}));

class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
// eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
globalThis.ResizeObserver ??= MockResizeObserver as unknown as typeof ResizeObserver;

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

const scanKeys = vi.fn();
const listChildren = vi.fn();
const dbSizes = vi.fn();
const getKey = vi.fn();

vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
  invokeListChildren: (...args: unknown[]) => listChildren(...args),
  invokeDbSizes: (...args: unknown[]) => dbSizes(...args),
  invokeGetKey: (...args: unknown[]) => getKey(...args),
}));

import { RedisWorkbench } from '../key-browser/RedisWorkbench';
import { toScanPattern } from '../key-browser/useWorkbenchSearch';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);
bindSchemaStore(
  create<SchemaStoreState>(() => ({
    schemas: new Map([['sess-1', { pathItems: {}, databases: ['db0', 'db1'], loading: false }]]),
    loadForConnection: async () => {},
    setLoadedTables: () => {},
    mergeNamespace: () => {},
    registerPathAliases: () => {},
    cachePathItems: () => {},
  })),
);

function leaf(key: string, keyType = 'string', ttl = -1) {
  return { kind: 'key', key, keyType, ttl, logicalLen: 4, memBytes: null };
}

function folder(prefix: string, count: number) {
  return { kind: 'folder', prefix, count };
}

function renderWorkbench(overrides: { hideSidebar?: boolean } = {}) {
  render(
    <RedisWorkbench
      dbSessionId="sess-1"
      initialDatabase="db0"
      hideSidebar={overrides.hideSidebar ?? true}
    />,
  );
}

/** Last `invokeScanKeys` call, as the pattern/cursor tuple it was made with. */
function lastScan(): { pattern: string; cursor: number; opts: Record<string, unknown> } {
  const call = scanKeys.mock.calls.at(-1) as unknown[];
  return {
    pattern: String(call[2]),
    cursor: Number(call[3]),
    opts: (call[5] ?? {}) as Record<string, unknown>,
  };
}

beforeEach(() => {
  scanKeys.mockImplementation(async (_s: string, _i: number, pattern: string, cursor: number) => ({
    keys: [
      { key: 'app:user:1', keyType: 'string', ttl: -1, size: 4, preview: '' },
      { key: 'app:user:2', keyType: 'hash', ttl: 60, size: 4, preview: '' },
    ],
    // Two pages for the first request so `scanning` is observable.
    cursor: cursor === 0 ? 7 : 0,
    dbSize: 2,
    matched: pattern === '*' ? 2 : 1,
  }));
  listChildren.mockResolvedValue({
    children: [folder('app:', 2), leaf('root-plain')],
    cursor: 0,
  });
  dbSizes.mockResolvedValue([
    { db: 0, keys: 2 },
    { db: 1, keys: 0 },
  ]);
  getKey.mockResolvedValue({
    key: 'app:user:1',
    keyType: 'string',
    ttl: -1,
    value: 'v',
    size: 1,
    memory: null,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('R1 column header (D-1)', () => {
  it('is resident even when the database sidebar is hidden', async () => {
    renderWorkbench({ hideSidebar: true });
    // The segment control used to live only inside <DbSidebar>, so `hideSidebar`
    // (the host's normal layout) removed the user's only way to pick a scope.
    expect(await screen.findByTestId('redis-tree-header')).toBeTruthy();
    expect(screen.getByTestId('redis-search-mode-value')).toBeTruthy();
    expect(screen.queryByTestId('redis-db-sidebar')).toBeNull();
  });

  it('marks the counter partial while the scan cursor is still open', async () => {
    renderWorkbench();
    const counter = await screen.findByTestId('redis-tree-count');
    await waitFor(() => expect(counter.getAttribute('data-loaded')).toBe('2'));
    // First page answered with cursor 7 ⇒ the loaded set is not the total.
    expect(counter.getAttribute('data-partial')).toBe('true');
    expect(counter.getAttribute('data-total')).toBe('2');
  });
});

describe('R2 search row (D-2)', () => {
  it('toScanPattern resolves literal, glob, blank and fuzzy states', () => {
    expect(toScanPattern('', false)).toBe('*');
    expect(toScanPattern('   ', true)).toBe('*');
    // A literal is a PREFIX, not an exact key: `app` has to reach `app:cache`.
    expect(toScanPattern('app', false)).toBe('app*');
    expect(toScanPattern('user', false)).toBe('user*');
    expect(toScanPattern('user', true)).toBe('*user*');
    // an explicit glob is never widened, in either chip state
    expect(toScanPattern('user:*', true)).toBe('user:*');
    expect(toScanPattern('user:*', false)).toBe('user:*');
    expect(toScanPattern('[a-z]bc', true)).toBe('[a-z]bc');
    // whitespace around a literal is not part of the key, and trimming happens
    // *before* wrapping so the star hugs the pattern
    expect(toScanPattern('  user:1  ', false)).toBe('user:1*');
    expect(toScanPattern('  user  ', true)).toBe('*user*');
    // fuzzy still widens a literal that has no glob char
    expect(toScanPattern('user:1', true)).toBe('*user:1*');
    // …and a hand-written trailing star is not doubled
    expect(toScanPattern('app*', false)).toBe('app*');
    expect(toScanPattern('app*', true)).toBe('app*');
  });

  it('walks the whole typing journey without ever losing the applied pattern', async () => {
    renderWorkbench();
    const input = await screen.findByTestId('redis-search-input');

    // Intermediate keystrokes must not fire a scan (Enter applies).
    fireEvent.change(input, { target: { value: 'u' } });
    fireEvent.change(input, { target: { value: 'us' } });
    fireEvent.change(input, { target: { value: 'user' } });
    expect(input).toHaveValue('user');
    expect(scanKeys.mock.calls.filter((c) => c[2] === 'user*')).toHaveLength(0);

    // Fuzzy chip on → Enter: literal becomes a substring glob.
    fireEvent.click(screen.getByTestId('redis-tree-chip-fuzzy'));
    expect(screen.getByTestId('redis-tree-chip-fuzzy').getAttribute('data-active')).toBe('on');
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(lastScan().pattern).toBe('*user*'));

    // Chip back off → the same input narrows to a key prefix (not a bare `user`,
    // which would admit only the one key spelled `user`).
    fireEvent.click(screen.getByTestId('redis-tree-chip-fuzzy'));
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(lastScan().pattern).toBe('user*'));

    // Esc clears the input (exit transition of the row, no scan left running).
    fireEvent.keyDown(input, { key: 'Escape' });
    await waitFor(() => expect(input).toHaveValue(''));
  });

  it('continues an unfinished cursor with the pattern it belongs to', async () => {
    renderWorkbench();
    const input = await screen.findByTestId('redis-search-input');
    fireEvent.change(input, { target: { value: 'user:*' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(lastScan().pattern).toBe('user:*'));
    expect(lastScan().cursor).toBe(0);

    // The user keeps typing *after* the search — the load-more page must still
    // use the applied pattern, not the half-typed input.
    fireEvent.change(input, { target: { value: 'user:1' } });
    fireEvent.click(screen.getByTestId('redis-tree-load-more'));
    await waitFor(() => expect(lastScan().pattern).toBe('user:*'));
    expect(lastScan().cursor).toBe(7);
  });

  it('toggles the no-expiry filter into the scan request', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-search-row');
    fireEvent.click(screen.getByTestId('redis-tree-chip-no-ttl'));
    await waitFor(() => expect(lastScan().opts.noTtlOnly).toBe(true));
    expect(screen.getByTestId('redis-tree-chip-no-ttl').getAttribute('data-active')).toBe('on');
    fireEvent.click(screen.getByTestId('redis-tree-chip-no-ttl'));
    await waitFor(() => expect(lastScan().opts.noTtlOnly).toBe(false));
  });
});
