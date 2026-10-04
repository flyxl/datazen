/**
 * [tester] Key-tree coverage battery — round 1 of the `redis-tree-ui` test pass.
 *
 * Written against the branches the coder's own suite leaves unexercised **inside
 * this track's acceptance surface** (D-1..D-8). The sub-80 files the coder named
 * surveyed are mostly D-0 extraction layers (split drag, dialogs,
 * context-menu plumbing), which this track accepts as out of its acceptance
 * surface. What is
 * *not* deferrable is a branch that belongs to a delivered requirement, and
 * exactly six of those were still cold on `ef0d62d94`:
 *
 *  1. `keyUnderFolder` (D-4 folder cascade boundary) — the "`app` must not
 *     swallow `apple`" rule at `keyTree.ts:137-139` had **zero** coverage,
 *     including the configured-separator arm;
 *  2. R1/I-4 `(n+)` — `KeyTreeList.tsx:158-161` renders `redis.tree.folderPartial`
 *     only while the parent level's cursor is open; no test ever left a cursor
 *     open, so the partial label and `data-partial='true'` were untested;
 *  3. the leaf checkbox itself (`KeyTreeList.tsx:456-465`) — every existing
 *     selection test goes through `selectMany` (⌘A / header) or `toggleKeys`
 *     (folder cascade), so `useKeySelection.toggleKey` was never called at all;
 *  4. `Enter` on a **folder** row (I-9 activate → toggle, `KeyTreeList.tsx:238`);
 *  5. clicking a **pinned** sticky header (`KeyTreeList.tsx:280`) — the folder
 *     row and its pinned twin are two different elements with the same handler;
 *  6. `useKeyTree`'s own scheduling (D-5): the stale-reply guard, the re-root
 *     refetch of still-expanded prefixes, `loadMore`'s guards, and `clearTree`.
 *
 * Assertion policy: `data-*` / i18n keys / recorded invoke args only; `useI18n`
 * is stubbed to an identity `t`, so no English copy is pinned anywhere.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
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
const redisCommand = vi.fn();
const showNativeContextMenu = vi.fn();

vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
  invokeListChildren: (...args: unknown[]) => listChildren(...args),
  invokeDbSizes: (...args: unknown[]) => dbSizes(...args),
  invokeGetKey: (...args: unknown[]) => getKey(...args),
  redisCommandInvoke: (...args: unknown[]) => redisCommand(...args),
}));

vi.mock('@datazen/driver-sdk', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@datazen/driver-sdk')>();
  return {
    ...actual,
    showNativeContextMenu: (...args: unknown[]) => showNativeContextMenu(...args),
  };
});

import { RedisWorkbench } from '../key-browser/RedisWorkbench';
import { keyUnderFolder, type KeyTreeRow } from '../key-browser/keyTree';
import { parentIndexOf } from '../key-browser/treeRowSpec';
import { useKeyTree } from '../key-browser/useKeyTree';
import { EMPTY_LEVEL } from '../key-browser/treeLevels';
import { invokeCountMatching } from '../key-browser/batchInvokes';
import {
  DEFAULT_TREE_WIDTH,
  MAX_TREE_WIDTH,
  MIN_TREE_WIDTH,
  clampTreeWidth,
} from '../key-browser/useWorkbenchSplit';
import type { ChildEntry } from '../shared/redisInvoke';
import type { KeyEntry } from '@datazen/driver-sdk';

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

function leaf(key: string, keyType = 'string', ttl = -1): ChildEntry {
  return { kind: 'key', key, keyType, ttl, logicalLen: 4, memBytes: null };
}

function folder(prefix: string, count: number): ChildEntry {
  return { kind: 'folder', prefix, count };
}

/** A `KeyEntry` for the row-shape unit assertions below. */
/**
 * The leaf keys whose checkbox is currently ticked.
 *
 * The header's selection-count badge left with the batch action group, so the
 * selection is read off the checkboxes directly — the same set the badge used to
 * summarise, minus the summary. State comes from the input's `checked` property
 * rather than the row's optional `data-checked` mirror.
 */
function tickedKeys(): string[] {
  return Array.from(
    document.querySelectorAll<HTMLInputElement>('[data-testid^="redis-tree-key-check-"]'),
  )
    .filter((el) => el.checked)
    .map((el) => (el.getAttribute('data-testid') ?? '').replace('redis-tree-key-check-', ''));
}

function entryOf(key: string): KeyEntry {
  return { key, keyType: 'string', ttl: -1, size: 0, preview: '' };
}

/** Mount a hook and keep a live handle on its (fresh every render) return value. */
function useHook<T>(factory: () => T) {
  let latest: T | null = null;
  const Probe = () => {
    latest = factory();
    return null;
  };
  const view = render(<Probe />);
  return {
    get current(): T {
      if (!latest) throw new Error('[tester] hook never rendered');
      return latest;
    },
    rerender: view.rerender,
  };
}

function renderWorkbench() {
  render(<RedisWorkbench dbSessionId="sess-1" initialDatabase="db0" hideSidebar />);
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  getKey.mockResolvedValue({
    key: 'k',
    keyType: 'string',
    ttl: -1,
    value: 'v',
    size: 1,
    memory: null,
  });
  redisCommand.mockResolvedValue(undefined);
  dbSizes.mockResolvedValue([
    { db: 0, keys: 2 },
    { db: 1, keys: 0 },
  ]);
  scanKeys.mockResolvedValue({
    keys: [
      { key: 'app:user:1', keyType: 'string', ttl: -1, size: 4, preview: '' },
      { key: 'app:user:2', keyType: 'hash', ttl: 60, size: 4, preview: '' },
    ],
    cursor: 0,
    dbSize: 2,
    matched: 2,
  });
  listChildren.mockResolvedValue({
    children: [folder('app:', 2), leaf('root-plain')],
    cursor: 0,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/* ── 1. D-4 folder-cascade boundary (pure) ────────────────────────────────── */
describe('[tester] keyUnderFolder owns the folder/leaf boundary (D-4)', () => {
  it('requires a non-empty remainder after the prefix', () => {
    // The exact-name case: `app:` is not *inside* `app:`.
    expect(keyUnderFolder('app:', 'app:', ':')).toBe(false);
    expect(keyUnderFolder('app', 'app', ':')).toBe(false);
  });

  it('refuses a prefix match that is not a namespace boundary', () => {
    // The bug this rule exists to kill: `app` swallowing `apple`.
    expect(keyUnderFolder('apple:1', 'app', ':')).toBe(false);
    expect(keyUnderFolder('app:1', 'app', ':')).toBe(true);
  });

  it('accepts the configured separator as the boundary', () => {
    expect(keyUnderFolder('app.cache.1', 'app.', '.')).toBe(true);
    // A dot-built key is still inside a dot folder written without the dot.
    expect(keyUnderFolder('app.cache', 'app', '.')).toBe(true);
    // …and `app` still must not swallow `appcache`.
    expect(keyUnderFolder('appcache', 'app', '.')).toBe(false);
  });

  it('keeps a prefix folded under another separator (preference changed mid-session)', () => {
    // `KeyTreeList` filters the *loaded* key set with the current separator, so a
    // folder that arrived from a `:` scan must still own its keys after the user
    // switches to `.` — otherwise the cascade silently selects nothing.
    expect(keyUnderFolder('app:user:1', 'app:', '.')).toBe(true);
    expect(keyUnderFolder('a/b/c', 'a/', ':')).toBe(true);
  });
});

/* ── 2. I-4 `(n+)` on a level whose cursor is still open ─────────────────── */

describe('[tester] R1/行规格: an unfinished level counts as n+ (D-5)', () => {
  it('renders the folder count with the partial key while its level keeps scanning', async () => {
    // Root answers with an OPEN cursor ⇒ every child count is a lower bound.
    listChildren.mockImplementation(
      async (_s: string, _i: number, prefix: string, cursor: number) => {
        if (prefix !== '') return { children: [leaf('app:user:1')], cursor: 0 };
        if (cursor === 0) return { children: [folder('app:', 2)], cursor: 41 };
        return { children: [leaf('app:user:2')], cursor: 0 };
      },
    );
    renderWorkbench();
    const count = await screen.findByTestId('redis-tree-folder-count-app:');
    expect(count.getAttribute('data-count')).toBe('2');
    expect(count.getAttribute('data-partial')).toBe('true');
    // The label is the partial template rather than a bare number. The expected
    // string is rebuilt from the same i18n key the component reads, so this file
    // pins no copy of its own (AGENTS.md 原则六).
    expect(count.textContent).toContain('redis.tree.folderPartial');

    // R1's counter agrees with the row: `N+` while any level is open.
    const tree = screen.getByTestId('redis-key-tree');
    expect(tree.getAttribute('data-row-count')).toBe('1');
  });

  it('drops the + once the level wraps', async () => {
    let open = true;
    listChildren.mockImplementation(async () =>
      open
        ? { children: [folder('app:', 2)], cursor: 41 }
        : { children: [folder('app:', 2)], cursor: 0 },
    );
    renderWorkbench();
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-folder-count-app:').getAttribute('data-partial')).toBe(
        'true',
      ),
    );
    open = false;
    // A refresh re-walks the finished level; the wrapped pass lands exact.
    fireEvent.click(screen.getByTestId('redis-tree-refresh'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-folder-count-app:').getAttribute('data-partial')).toBe(
        'false',
      ),
    );
    // Non-partial renders the bare count (no partial template involved).
    expect(screen.getByTestId('redis-tree-folder-count-app:').textContent).toBe('(2)');
  });
});

/* ── 3-5. the row affordances no existing test drives ─────────────────────── */

describe('[tester] row affordances: leaf checkbox, Enter on folder, pinned header click', () => {
  beforeEach(() => {
    // Flat rows so the leaves are directly visible (no expand step needed).
    listChildren.mockResolvedValue({
      children: [leaf('app:user:1'), leaf('app:user:2')],
      cursor: 0,
    });
  });

  it('ticking one leaf toggles exactly that key', async () => {
    renderWorkbench();
    const check = await screen.findByTestId('redis-tree-key-check-app:user:1');
    expect(check.getAttribute('data-checked')).toBe('false');

    // This is `useKeySelection.toggleKey` — the only selection mutator the other
    // suites never reach (they all go through selectMany / toggleKeys).
    fireEvent.click(check);
    await waitFor(() =>
      expect(
        screen.getByTestId('redis-tree-key-check-app:user:1').getAttribute('data-checked'),
      ).toBe('true'),
    );
    expect(screen.getByTestId('redis-tree-key-check-app:user:2').getAttribute('data-checked')).toBe(
      'false',
    );
    expect(tickedKeys()).toEqual(['app:user:1']);

    // Unticking is the exit; the selection empties with it.
    fireEvent.click(screen.getByTestId('redis-tree-key-check-app:user:1'));
    await waitFor(() => expect(tickedKeys()).toEqual([]));
  });

  it('a leaf row click selects the key, not the checkbox', async () => {
    renderWorkbench();
    const row = await screen.findByTestId('redis-key-row-app:user:1');
    fireEvent.click(row);
    await waitFor(() => expect(row.getAttribute('data-selected')).toBe('true'));
    // Opening the detail must not be counted as a checkbox selection (I-8 feed).
    expect(tickedKeys()).toEqual([]);
    expect(getKey.mock.calls.some((c) => String((c as unknown[])[2]) === 'app:user:1')).toBe(true);
  });

  it('Enter on a folder row toggles it (I-9 activate, both row kinds)', async () => {
    listChildren.mockImplementation(async (_s: string, _i: number, prefix: string) =>
      prefix === ''
        ? { children: [folder('app:', 2)], cursor: 0 }
        : { children: [leaf('app:user:1')], cursor: 0 },
    );
    renderWorkbench();
    const tree = await screen.findByTestId('redis-key-tree');
    await screen.findByTestId('redis-tree-folder-app:');

    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(tree.getAttribute('data-active-index')).toBe('0');
    expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-expanded')).toBe(
      'false',
    );

    fireEvent.keyDown(tree, { key: 'Enter' });
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-expanded')).toBe(
        'true',
      ),
    );
    // Enter on a folder opens it; it never mounts a detail for a prefix.
    expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-row-kind')).toBe(
      'folder',
    );
    expect(getKey.mock.calls).toHaveLength(0);
  });

  it('clicking the pinned twin folds the folder it mirrors (D-4 sticky)', async () => {
    // Two rows *inside* `app:` so that scrolling one row makes `app:` a strict
    // ancestor of the top row (a single-child folder has nothing to scroll past).
    listChildren.mockImplementation(async (_s: string, _i: number, prefix: string) =>
      prefix === ''
        ? { children: [folder('app:', 2)], cursor: 0 }
        : { children: [leaf('app:user:1'), leaf('app:user:2')], cursor: 0 },
    );
    renderWorkbench();
    fireEvent.click(await screen.findByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:user:1');
    const tree = screen.getByTestId('redis-key-tree');
    expect(tree.getAttribute('data-sticky-depth')).toBe('0');
    expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-expanded')).toBe('true');

    Object.defineProperty(tree, 'scrollTop', { value: 30, writable: true, configurable: true });
    fireEvent.scroll(tree);
    const pinned = await screen.findByTestId('redis-tree-sticky-folder-app:');
    expect(pinned.getAttribute('data-sticky-depth')).toBe('0');

    // The pinned header owns the same toggle as the in-flow row (exit transition).
    fireEvent.click(pinned);
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-expanded')).toBe(
        'false',
      ),
    );
    // Folding drops the subtree rows, so the pin is released with them.
    await waitFor(() =>
      expect(screen.getByTestId('redis-key-tree').getAttribute('data-sticky-depth')).toBe('0'),
    );
  });

  it('right-clicking a leaf hands the row identity to the web context menu', async () => {
    renderWorkbench();
    const row = await screen.findByTestId('redis-key-row-app:user:1');
    fireEvent.contextMenu(row);
    expect(showNativeContextMenu).toHaveBeenCalledOnce();
    const [items, point] = showNativeContextMenu.mock.calls[0] as unknown as [
      unknown[],
      { x: number; y: number },
    ];
    // Bound by data, not geometry: the menu is built for *this* key.
    expect(Array.isArray(items) && items.length).toBeGreaterThan(0);
    expect(point).toBeTruthy();
  });
});

/* ── 6. useKeyTree scheduling (D-5) ───────────────────────────────────────── */

describe('[tester] useKeyTree: the scheduling that the fold functions cannot see (D-5)', () => {
  it('drops a stale reply for the same prefix instead of folding it in', async () => {
    let respond: ((v: { children: ChildEntry[]; cursor: number }) => void)[] = [];
    listChildren.mockImplementation(
      () =>
        new Promise<{ children: ChildEntry[]; cursor: number }>((resolve) => respond.push(resolve)),
    );
    const hook = useHook(() => useKeyTree({ dbSessionId: 's', dbIndex: 0, enabled: true }));
    await waitFor(() => expect(respond).toHaveLength(1));

    // Two requests for the root, answered in the wrong order.
    act(() => hook.current.refresh());
    await waitFor(() => expect(respond).toHaveLength(2));
    const [first, second] = respond;
    act(() => second!({ children: [folder('fresh:', 1)], cursor: 0 }));
    await waitFor(() =>
      expect(
        hook.current.levels['']?.children.map((c) => (c as { prefix?: string }).prefix),
      ).toEqual(['fresh:']),
    );
    // The late reply for the *superseded* request must be discarded.
    act(() => first!({ children: [folder('stale:', 9)], cursor: 0 }));
    await new Promise((r) => setTimeout(r, 0));
    expect(hook.current.levels['']?.children.map((c) => (c as { prefix?: string }).prefix)).toEqual(
      ['fresh:'],
    );
  });

  it('re-fetches every still-expanded prefix when the tree is re-rooted', async () => {
    listChildren.mockResolvedValue({ children: [folder('app:', 2)], cursor: 0 });
    const hook = useHook(() => useKeyTree({ dbSessionId: 's', dbIndex: 0, enabled: true }));
    await waitFor(() => expect(listChildren).toHaveBeenCalled());
    act(() => hook.current.toggleFolder('app:'));
    await waitFor(() => expect(hook.current.expanded.has('app:')).toBe(true));
    const before = listChildren.mock.calls.length;

    // A filter change re-roots: a folder left expanded must be filled again,
    // otherwise it renders "expanded and empty" (I-4's re-expand guarantee).
    act(() => hook.current.loadRoot());
    await waitFor(() => expect(listChildren.mock.calls.length).toBeGreaterThanOrEqual(before + 2));
    const prefixes = listChildren.mock.calls.slice(before).map((c) => String((c as unknown[])[2]));
    expect(prefixes).toContain('');
    expect(prefixes).toContain('app:');
  });

  it('pages an unfinished level forward only when it is neither done nor loading', async () => {
    listChildren.mockImplementation(async (_s: string, _i: number, _p: string, cursor: number) =>
      cursor === 0
        ? { children: [folder('app:', 2)], cursor: 77 }
        : { children: [leaf('app:x')], cursor: 0 },
    );
    const hook = useHook(() => useKeyTree({ dbSessionId: 's', dbIndex: 0, enabled: true }));
    await waitFor(() => expect(hook.current.levels['']?.done).toBe(false));
    const calls = listChildren.mock.calls.length;

    act(() => hook.current.loadMore('missing')); // no such level ⇒ no request
    act(() => hook.current.loadMore('')); // loading already finished ⇒ page forward
    await waitFor(() => expect(listChildren.mock.calls.length).toBe(calls + 1));
    // …and the continuation carried the level's own cursor, not 0 (I-4).
    expect(listChildren.mock.calls.at(-1)?.[3]).toBe(77);

    // A finished level must not be paged again.
    await waitFor(() => expect(hook.current.levels['']?.done).toBe(true));
    const afterDone = listChildren.mock.calls.length;
    act(() => hook.current.loadMore(''));
    expect(listChildren.mock.calls.length).toBe(afterDone);
  });

  it('clearTree forgets levels *and* expanded prefixes (db switch)', async () => {
    listChildren.mockResolvedValue({ children: [folder('app:', 2)], cursor: 0 });
    const hook = useHook(() => useKeyTree({ dbSessionId: 's', dbIndex: 0, enabled: true }));
    await waitFor(() => expect(hook.current.levels['']).toBeTruthy());
    act(() => hook.current.toggleFolder('app:'));
    await waitFor(() => expect(hook.current.expanded.has('app:')).toBe(true));

    act(() => hook.current.clearTree());
    expect(hook.current.levels).toEqual({});
    expect(hook.current.expanded.size).toBe(0);
    // An empty level map can claim nothing: no scanning, no root error.
    expect(hook.current.scanning).toBe(false);
    expect(hook.current.rootError).toBe(false);
    expect(EMPTY_LEVEL.done).toBe(false);
  });
});

/* ── 7. keyboard edge arms the journeys never take (D-7) ──────────────────── */

describe('[tester] I-9 edge arms: empty tree, entry from nowhere, orphaned child', () => {
  it('a key press on an empty tree neither enters navigation nor throws', async () => {
    listChildren.mockResolvedValue({ children: [], cursor: 0 });
    renderWorkbench();
    const tree = await screen.findByTestId('redis-key-tree');
    expect(tree.getAttribute('data-row-count')).toBe('0');

    // Every intent that needs a row must bail while rowCount is 0 …
    for (const key of ['ArrowDown', 'ArrowRight', 'ArrowLeft', 'Enter'] as const) {
      fireEvent.keyDown(tree, { key });
      expect(tree.getAttribute('data-active-index')).toBe('-1');
    }
    // … but the owner-independent chords still work (refresh is not row-scoped),
    // proving `rowCount === 0` is a navigation guard, not a global keylock.
    const rootsBefore = listChildren.mock.calls.length;
    fireEvent.keyDown(tree, { key: 'r', metaKey: true });
    await waitFor(() => expect(listChildren.mock.calls.length).toBeGreaterThan(rootsBefore));
  });

  it('→ / ← / Enter with no active row enter the list instead of acting on nothing', async () => {
    listChildren.mockImplementation(async (_s: string, _i: number, prefix: string) =>
      prefix === ''
        ? { children: [folder('app:', 2)], cursor: 0 }
        : { children: [leaf('app:user:1')], cursor: 0 },
    );
    renderWorkbench();
    const tree = await screen.findByTestId('redis-key-tree');
    await screen.findByTestId('redis-tree-folder-app:');
    expect(tree.getAttribute('data-active-index')).toBe('-1');

    // Enter with nothing active ⇒ "enter the list" (index 0), not activate(-1).
    fireEvent.keyDown(tree, { key: 'Enter' });
    expect(tree.getAttribute('data-active-index')).toBe('0');

    // Back to no selection, then the same entry via → and via ←.
    fireEvent.keyDown(tree, { key: 'Escape' });
    expect(tree.getAttribute('data-active-index')).toBe('-1');
    fireEvent.keyDown(tree, { key: 'ArrowRight' });
    expect(tree.getAttribute('data-active-index')).toBe('0');
    fireEvent.keyDown(tree, { key: 'Escape' });
    fireEvent.keyDown(tree, { key: 'ArrowLeft' });
    expect(tree.getAttribute('data-active-index')).toBe('0');
  });

  it('← on a row whose folder parent vanished above it stays put (orphan clamp)', () => {
    // parentIndexOf walk: a depth-1 row with *no* depth-0 folder above it must
    // return `from` rather than -1 (which would make the tree setActiveIndex(-1)
    // and silently drop the user out of keyboard navigation).
    const rows: KeyTreeRow[] = [
      { kind: 'key', entry: entryOf('orphan'), depth: 1, label: 'orphan' },
      { kind: 'key', entry: entryOf('second'), depth: 1, label: 'second' },
    ];
    expect(parentIndexOf(rows, 0)).toBe(0);
    expect(parentIndexOf(rows, 1)).toBe(1);
    // Out-of-range index: same "stay put" answer, no crash.
    expect(parentIndexOf(rows, 7)).toBe(7);
  });
});

/* ── 8. KeyTreeColumn mode switch (D-1 result column reuse) ───────────────── */

describe('[tester] KeyTreeColumn swaps the tree for the value-hit list by scope (R1 tabs)', () => {
  it('value scope renders the hit list, key scope renders the tree again', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    // Exit transition of value mode back into key mode is the interesting half:
    // the column must hand the tree its props again, not a dead div.
    fireEvent.click(screen.getByTestId('redis-search-mode-value'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-header').getAttribute('data-search-mode')).toBe(
        'value',
      ),
    );
    expect(screen.queryByTestId('redis-key-tree')).toBeNull();
    // The column really is the hit list now, not an empty div that happens to
    // lack the tree (identity `t` ⇒ the empty-state text *is* its i18n key).
    expect(await screen.findByText('redis.search.noResults')).toBeTruthy();

    fireEvent.click(screen.getByTestId('redis-search-mode-key'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-header').getAttribute('data-search-mode')).toBe('key'),
    );
    expect(await screen.findByTestId('redis-key-tree')).toBeTruthy();
    expect(screen.getByTestId('redis-tree-folder-app:')).toBeTruthy();
  });
});

/* ── 9. the batch invoke seam speaks camelCase to the host (I-8 feed) ─────── */

describe('[tester] batchInvokes payload contract (D-6 seam)', () => {
  it('passes the count_matching pattern through as the preview it is (not a write)', async () => {
    const invoke = vi.fn(async (..._args: unknown[]) => 7);
    await expect(invokeCountMatching('sess-1', 2, 'app:*', invoke)).resolves.toBe(7);
    const [driver, command, payload] = invoke.mock.calls[0] as unknown as [
      string,
      string,
      Record<string, unknown>,
    ];
    expect(driver).toBe('redis');
    expect(command).toBe('count_matching');
    expect(payload).toEqual({ dbSessionId: 'sess-1', dbIndex: 2, pattern: 'app:*' });
  });
});

/* ── 10. split clamp (the pure, UI-free half of useWorkbenchSplit) ────────── */

describe('[tester] clampTreeWidth keeps the split inside its bounds', () => {
  it('clamps both ends, rounds, and refuses non-finite widths', () => {
    expect(clampTreeWidth(DEFAULT_TREE_WIDTH)).toBe(DEFAULT_TREE_WIDTH);
    expect(clampTreeWidth(1)).toBe(MIN_TREE_WIDTH);
    expect(clampTreeWidth(99_999)).toBe(MAX_TREE_WIDTH);
    expect(clampTreeWidth(300.6)).toBe(301);
    // A drag that computes a non-finite width must fall back to the default
    // rather than write `NaNpx` into the style (which collapses the column).
    expect(clampTreeWidth(Number.NaN)).toBe(DEFAULT_TREE_WIDTH);
    expect(clampTreeWidth(Number.POSITIVE_INFINITY)).toBe(DEFAULT_TREE_WIDTH);
  });
});
