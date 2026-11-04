/**
 * [redis-tree-ui-BUG-004] Keyboard × breadcrumb journey (I-9 + D-2 round-1).
 *
 * Round 1 claimed "navigation passes breadcrumbs through, and `→` into a subtree
 * does too", but the v8 execution counters proved no test ever reached those
 * statements (`KeyTreeList.tsx:174`, the `:176` clamp arm, and `:274`), i.e. the
 * claim had zero evidence and deleting the guards would not have turned a single
 * gate red. This file is that evidence: every case starts from a *filtered* tree
 * that actually contains a breadcrumb row, and asserts where the active row can
 * and cannot land.
 *
 * Assertion policy: `data-*` only (`data-breadcrumb`, `data-active`,
 * `data-active-index`, `data-row-kind`, `data-testid`), zero English copy, zero
 * geometry. The virtualizer is stubbed to hand back every row, as elsewhere in
 * this track, because jsdom measures a zero-size scroll element.
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
import type { KeyTreeRow } from '../key-browser/keyTree';
import {
  filterKeysByPattern,
  filterTreeRowsByPattern,
  isBreadcrumbRow,
} from '../key-browser/keyTreeFilter';
import type { ChildEntry } from '../shared/redisInvoke';

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

/* ── a fold-faithful two/three-level keyspace ─────────────────────────────── */

function folder(prefix: string, count: number): ChildEntry {
  return { kind: 'folder', prefix, count };
}

function leaf(key: string): ChildEntry {
  return { kind: 'key', key, keyType: 'string', ttl: -1, logicalLen: 4, memBytes: null };
}

/**
 * Point the mocks at `keys`, folding `list_children` the way the Rust does: one
 * level at a time, on the configured separator, leaves keyed by their absolute
 * name. Faithful folding is what makes "a breadcrumb exists at index 0" a
 * reproducible starting state instead of a hand-placed row.
 */
function useKeyspace(keys: readonly string[]): void {
  scanKeys.mockImplementation(async (_s: string, _i: number, pattern: string) => {
    const answer =
      !pattern || pattern === '*'
        ? keys
        : keys.filter((key) => {
            const re = new RegExp(
              `^${pattern
                .replace(/[.+^${}()|[\]\\]/g, '\\$&')
                .replace(/\*/g, '.*')
                .replace(/\?/g, '.')}$`,
            );
            return re.test(key);
          });
    return {
      keys: answer.map((key) => ({ key, keyType: 'string', ttl: -1, size: 4, preview: '' })),
      cursor: 0,
      dbSize: keys.length,
      matched: answer.length,
    };
  });
  listChildren.mockImplementation(async (_s: string, _i: number, prefix: string) => {
    const folders = new Map<string, number>();
    const leaves: string[] = [];
    for (const key of keys) {
      if (!key.startsWith(prefix)) continue;
      const rest = key.slice(prefix.length);
      const at = rest.indexOf(':');
      if (at < 0) leaves.push(key);
      else {
        const fp = `${prefix}${rest.slice(0, at + 1)}`;
        folders.set(fp, (folders.get(fp) ?? 0) + 1);
      }
    }
    return {
      children: [
        ...[...folders.entries()].map(([fp, count]) => folder(fp, count)),
        ...leaves.map((key) => leaf(key)),
      ],
      cursor: 0,
    };
  });
}

function renderWorkbench() {
  render(<RedisWorkbench dbSessionId="sess-1" initialDatabase="db0" hideSidebar />);
}

function tree(): HTMLElement {
  return screen.getByTestId('redis-key-tree');
}

function activeIndex(): string | null {
  return tree().getAttribute('data-active-index');
}

/** `data-active` of the breadcrumb row for `path`, whatever it currently is. */
function breadcrumbActive(path: string): string | null {
  const row = screen.getByTestId(`redis-tree-folder-${path}`);
  expect(row.getAttribute('data-breadcrumb')).toBe('true');
  return row.getAttribute('data-active');
}

async function applyPattern(value: string): Promise<void> {
  const input = await screen.findByTestId('redis-search-input');
  fireEvent.change(input, { target: { value } });
  fireEvent.keyDown(input, { key: 'Enter' });
}

function press(key: string): void {
  fireEvent.keyDown(tree(), { key });
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  dbSizes.mockResolvedValue([
    { db: 0, keys: 3 },
    { db: 1, keys: 0 },
  ]);
  getKey.mockResolvedValue({
    key: 'app:1',
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

describe('[redis-tree-ui-BUG-004] keyboard navigation never stops on a breadcrumb', () => {
  /*
   * Two-level keyspace: under `*:1` only `app:1` survives, so the tree paints
   * `app:` as its ancestor **breadcrumb** at index 0 and the key row at index 1.
   * Index 0 being the breadcrumb is the whole point: `nextActiveIndex(-1, 1, n)`
   * hands back exactly that index, so entering the list is the case that must
   * skip.
   */
  beforeEach(() => useKeyspace(['app:1', 'app:2', 'root-plain']));

  it('setup: the filtered tree really has a breadcrumb at index 0', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:1');
    await applyPattern('*:1');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('1'));
    const crumb = screen.getByTestId('redis-tree-folder-app:');
    expect(crumb.getAttribute('data-breadcrumb')).toBe('true');
    expect(crumb.getAttribute('data-row-index')).toBe('0');
    expect(screen.getByTestId('redis-key-row-app:1').getAttribute('data-row-index')).toBe('1');
  });

  it('↓ enters the list past the leading breadcrumb and lands on the key row', async () => {
    // Covers `KeyTreeList.tsx:174` — the skip loop body, previously hits=0.
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:1');
    await applyPattern('*:1');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('1'));

    expect(activeIndex()).toBe('-1');
    press('ArrowDown');
    expect(activeIndex()).toBe('1');
    expect(breadcrumbActive('app:')).toBe('false');
    expect(screen.getByTestId('redis-key-row-app:1').getAttribute('data-active')).toBe('true');
  });

  it('↑ at the only navigable row stays put instead of landing on the breadcrumb', async () => {
    // Covers the `:176` clamp arm (`index` runs off the top) — round 1 returned
    // `target` there, which is the very breadcrumb being skipped.
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:1');
    await applyPattern('*:1');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('1'));
    press('ArrowDown');
    expect(activeIndex()).toBe('1');

    press('ArrowUp');
    expect(activeIndex()).toBe('1');
    expect(breadcrumbActive('app:')).toBe('false');

    // Repeating it cannot creep onto the breadcrumb either.
    press('ArrowUp');
    press('ArrowUp');
    expect(activeIndex()).toBe('1');
    expect(breadcrumbActive('app:')).toBe('false');
  });

  it('a filtered tree of breadcrumbs only leaves navigation with nothing to enter', async () => {
    // `*:2` keeps `app:2` … which sits behind the same breadcrumb. Wipe the
    // survivor too and no navigable row remains: the guard must bail at
    // `rowCount === 0` rather than land on a decoration row.
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    await applyPattern('*nope');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('0'));
    for (const key of ['ArrowDown', 'ArrowUp', 'ArrowRight', 'ArrowLeft', 'Enter']) {
      press(key);
      expect(activeIndex()).toBe('-1');
    }
  });
});

/* ── why no DOM case asserts the `→` crumb-child path ─────────────────────── */

describe('[redis-tree-ui-BUG-004] the `→` crumb-child arm is unreachable through the filter', () => {
  /**
   * The `→`-into-subtree walk exists to skip a breadcrumb **child** of an open
   * folder. Reaching it through a real pattern needs a glob that matches the
   * parent `F`, matches a survivor `S` under the grandchild `G`, and rejects `G`
   * itself — because precedence makes a folder an ancestor-of-survivor *if and
   * only if* its parent is one too (see `filterTreeRowsByPattern`'s emit
   * order), so `G` cannot be a crumb while `F` stays an ordinary row.
   *
   * This test states that claim executable instead of leaving it as a comment:
   * it enumerates a pattern alphabet over the three-level shape and finds no
   * witness. The guard stays (defence in depth, and it delegates to the same
   * helper the ↑/↓ arm uses, which *is* covered), but nobody should hunt for a
   * DOM test of a path the filter cannot produce.
   */
  const ROWS: KeyTreeRow[] = [
    { kind: 'folder', path: 'a:', label: 'a', depth: 0, count: 2 },
    { kind: 'folder', path: 'a:b:', label: 'b', depth: 1, count: 2 },
    {
      kind: 'key',
      entry: { key: 'a:b:c', keyType: 'string', ttl: -1, size: 1, preview: '' },
      depth: 2,
      label: 'c',
    },
  ];
  const ATOMS = ['a', 'b', ':', '*', '?'];
  const patterns = new Set<string>();
  const grow = (prefix: string, depth: number) => {
    if (depth === 0) {
      if (prefix) patterns.add(prefix);
      return;
    }
    for (const atom of ATOMS) grow(prefix + atom, depth - 1);
  };
  for (let depth = 1; depth <= 4; depth++) grow('', depth);

  it('no pattern yields an ordinary folder followed by a breadcrumb child', () => {
    const witnesses: string[] = [];
    for (const pattern of patterns) {
      const visible = filterKeysByPattern(['a:b:c'], pattern);
      const rows = filterTreeRowsByPattern(ROWS, pattern, (fp) =>
        visible.some((key) => key.startsWith(fp)),
      );
      for (let i = 0; i + 1 < rows.length; i++) {
        const parent = rows[i]!;
        const child = rows[i + 1]!;
        const parentIsFolder = parent.kind === 'folder';
        const childIsFolder = child.kind === 'folder';
        if (
          parentIsFolder &&
          childIsFolder &&
          !isBreadcrumbRow(parent) &&
          isBreadcrumbRow(child) &&
          child.depth === parent.depth + 1
        ) {
          witnesses.push(pattern);
        }
      }
    }
    expect(patterns.size).toBeGreaterThan(300);
    expect(witnesses).toEqual([]);
  });
});
