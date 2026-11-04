/**
 * Key tree on the shared `VirtualTree` shell.
 *
 * This suite exists because the Redis key browser stopped owning its own
 * virtualization and ARIA container. The shell is generic, so the two trees
 * that sit on it are not required to look alike — but the **key browser's**
 * guarantees must survive the move unchanged, and they are exactly the ones a
 * generic shell cannot infer for a consumer:
 *
 *  - `role="tree"` on the grid, `role="treeitem"` + `aria-level` on painted
 *    rows, `aria-level = depth + 1` and **never** read off the painted indent;
 *  - `aria-expanded` on folders, and deliberately **absent** on leaves;
 *  - a pattern breadcrumb is decoration: no `treeitem`, no level, still
 *    `aria-hidden`;
 *  - row identity from the key/folder itself, so a fold/unfold elsewhere cannot
 *    resurrect a row's React state onto a different row;
 *  - the shell's window paints a window: the grid is sized for the whole list
 *    but only the visible rows are mounted.
 *
 * The virtualizer is stubbed to yield every row: jsdom measures a zero-size
 * scroll element, so the real one would mount nothing. It is stubbed here for
 * the same reason the other fourteen suites stub it (see `driverUiSetup.ts`).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';

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

import { KeyTreeList } from '../key-browser/KeyTreeList';
import { isFolderRow, keyTreeRowKey, ROW_HEIGHT } from '../key-browser/treeRowSpec';
import type { KeyTreeRow } from '../key-browser/keyTree';
import redisEn from '../../locales/en';

/*
 * `key:<name>` is a legal Redis key name, and `folder:<path>` is a legal
 * folder path — so the collision this guards against is reachable with real
 * data, not a contrived string. Folding `x` away and expanding a key literally
 * named `folder:x` used to make React reuse one DOM node for two rows.
 */
const folder = (path: string, depth: number, extra?: { breadcrumb?: boolean }): KeyTreeRow => ({
  kind: 'folder',
  path,
  label: path,
  depth,
  count: 0,
  ...extra,
});

const key = (name: string, depth: number): KeyTreeRow => ({
  kind: 'key',
  entry: { key: name, keyType: 'string', ttl: -1, size: 4, preview: '' },
  label: name,
  depth,
});

/** `app:` > `app:user:` > two keys, plus a sibling root key. */
const TREE: KeyTreeRow[] = [
  folder('app:', 0),
  folder('app:user:', 1),
  key('app:user:1', 2),
  key('app:user:2', 2),
  folder('app:cfg:', 1),
  key('root-plain', 0),
];

function renderList(over: Partial<Parameters<typeof KeyTreeList>[0]> = {}) {
  const onToggleFolder = vi.fn();
  const onSelectKey = vi.fn();
  const result = render(
    <KeyTreeList
      treeRows={TREE}
      allKeys={['app:user:1', 'app:user:2', 'root-plain']}
      expandedFolders={new Set(['app:', 'app:user:'])}
      onToggleFolder={onToggleFolder}
      selectedKey={null}
      selectedKeys={new Set()}
      onSelectKey={onSelectKey}
      onToggleKey={() => {}}
      onToggleKeys={() => {}}
      onKeyContextMenu={() => {}}
      onDeleteRow={() => {}}
      loading={false}
      hasMore={false}
      onLoadMore={() => {}}
      separator={':'}
      emptyState={null}
      pattern=""
      filterActive={false}
      onSelectAllLoaded={() => {}}
      onRefresh={() => {}}
      onClearSelection={() => {}}
      {...over}
    />,
  );
  return { ...result, onToggleFolder, onSelectKey };
}

const treeGrid = (c: HTMLElement) => c.querySelector<HTMLElement>('[role="tree"]')!;
const rowEls = (c: HTMLElement) => Array.from(c.querySelectorAll<HTMLElement>('[data-row-index]'));
/** The shell wrapper, which is where the active row is published. */
const shellEl = (c: HTMLElement) => c.querySelector<HTMLElement>('[data-testid="redis-key-tree"]')!;

beforeEach(() => {
  cleanup();
});
afterEach(() => {
  vi.clearAllMocks();
});

describe('key tree · ARIA container semantics', () => {
  it('owns role="tree" on the grid, which is the row container', () => {
    const { container } = renderList();
    const grid = treeGrid(container);
    expect(grid).toBeTruthy();
    // Every painted row lives inside the grid, not beside it.
    expect(grid.querySelectorAll('[data-row-index]').length).toBe(TREE.length);
  });

  it('carries an accessible name, because the shell gave it a role', () => {
    // Before the shell this was a plain column of buttons: no role, so nothing
    // needed naming. Adding `role="tree"` made the container something a screen
    // reader announces — unnamed, and indistinguishable from any other tree.
    const grid = treeGrid(renderList().container)!;
    const name = grid.getAttribute('aria-label') ?? grid.getAttribute('aria-labelledby');
    expect(name, 'role="tree" must have an accessible name').toBe('redis.keyBrowser.tree');
    // This harness has no i18n runtime, so `t()` hands back the key path itself.
    // That is the gap: it proves the container is wired, not that the key
    // resolves. Assert the real English value, so "added the aria-label but
    // never added the locale key" cannot pass.
    expect(redisEn['redis.keyBrowser.tree']).toBe('Key browser');
  });

  it('gives every painted row role="treeitem" and a level one past its depth', () => {
    const { container } = renderList();
    for (const el of rowEls(container)) {
      const depth = Number(el.dataset.depth);
      expect(el.getAttribute('role')).toBe('treeitem');
      // `aria-level` is 1-based, `depth` is 0-based: the top row is level 1.
      expect(el.getAttribute('aria-level')).toBe(String(depth + 1));
    }
    // Spelled out so a regression cannot be "fixed" by editing both sides.
    expect(rowEls(container).map((el) => el.getAttribute('aria-level'))).toEqual([
      '1',
      '2',
      '3',
      '3',
      '2',
      '1',
    ]);
  });

  it('derives the level from the row, never from the painted indent', () => {
    // Two levels that are deliberately decoupled: the shell's `levelDepth`
    // override. The indent still follows `depth` alone, so the two disagree —
    // which is the point, and the reason they are asserted separately.
    const decoupled: KeyTreeRow[] = [folder('a:', 0), folder('a:b:', 3), key('a:b:c', 3)];
    const { container } = renderList({ treeRows: decoupled });
    const rows = rowEls(container);
    expect(rows.map((r) => r.getAttribute('aria-level'))).toEqual(['1', '4', '4']);
    // The indent followed `depth` only, so it did not move with the level.
    expect(new Set(rows.map((r) => r.style.paddingLeft)).size).toBeGreaterThan(1);
  });

  it('marks folders expandable and leaves the attribute off leaves entirely', () => {
    const { container } = renderList();
    const byPath = (p: string) => container.querySelector<HTMLElement>(`[data-row-path="${p}"]`)!;
    expect(byPath('app:').getAttribute('aria-expanded')).toBe('true');
    expect(byPath('app:cfg:').getAttribute('aria-expanded')).toBe('false');
    // Deliberately absent — not "false". A leaf that reports `false` tells a
    // screen reader it can be opened, and then nothing happens.
    expect(byPath('app:user:1').hasAttribute('aria-expanded')).toBe(false);
    expect(byPath('app:user:2').hasAttribute('aria-expanded')).toBe(false);
    expect(byPath('root-plain').hasAttribute('aria-expanded')).toBe(false);
  });

  it('keeps a pattern breadcrumb out of the tree structure', () => {
    const withCrumb: KeyTreeRow[] = [folder('app:', 0, { breadcrumb: true }), key('app:user:1', 1)];
    const { container } = renderList({ treeRows: withCrumb });
    const crumb = rowEls(container)[0]!;
    expect(crumb.dataset.breadcrumb).toBe('true');
    expect(crumb.getAttribute('aria-hidden')).toBe('true');
    expect(crumb.hasAttribute('role')).toBe(false);
    expect(crumb.hasAttribute('aria-level')).toBe(false);
    // The row it introduces is still fully in the tree.
    expect(rowEls(container)[1]!.getAttribute('role')).toBe('treeitem');
  });
});

describe('key tree · key contract', () => {
  it('keys a folder by its path and a key by its name, and the two cannot collide', () => {
    const theFolder = folder('x', 0);
    // A Redis key may legitimately be named `folder:x`, so this pair is real
    // data, not a contrived string: unprefixed, they were the same key.
    const theKey = key('folder:x', 0);
    expect(keyTreeRowKey(theFolder)).toBe('folder:x');
    expect(keyTreeRowKey(theKey)).toBe('key:folder:x');
    expect(keyTreeRowKey(theFolder)).not.toBe(keyTreeRowKey(theKey));
  });

  it('never puts a position in a row key', () => {
    // The same row rendered at two different indices must produce one key: a
    // key that embeds the index makes every insert above it recycle the React
    // state of the row that used to sit in that slot.
    const row: KeyTreeRow = key('app:user', 2);
    // Names that carry no digit at all, so a stray index cannot hide in the
    // assertion: `depth` below is the only place an index could come from.
    expect(keyTreeRowKey(row)).toBe('key:app:user');
    expect(keyTreeRowKey({ ...row, depth: 7 })).toBe('key:app:user');
    expect(keyTreeRowKey(folder('app:user', 9))).toBe('folder:app:user');
  });

  it('separates two rows that share a label but not an identity', () => {
    // Two connections, one namespace: the label and depth are identical, so a
    // key built from either would merge two different rows into one.
    const here = key('tbl', 0);
    const there = key('tbl', 0);
    expect(keyTreeRowKey(here)).toBe(keyTreeRowKey(there));
    // ...and the parts that *are* the identity are all in the key.
    expect(keyTreeRowKey(here)).toContain('tbl');
  });

  it("keeps a row's DOM node attached to it across a fold that shifts its index", () => {
    const { container, rerender } = renderList();
    const target = container.querySelector<HTMLElement>('[data-row-path="app:cfg:"]')!;
    const targetIndexBefore = target.dataset.rowIndex;
    const sameNode = target;

    // Fold the first folder: the row above disappears, every row below it
    // shifts down one index, and a tree that keys on the index would move this
    // node's state onto whichever row slid into its old slot.
    rerender(
      <KeyTreeList
        treeRows={[
          folder('app:', 0),
          folder('app:user:', 1),
          key('app:user:1', 2),
          key('app:user:2', 2),
          folder('app:cfg:', 1),
          key('root-plain', 0),
        ]}
        allKeys={[]}
        expandedFolders={new Set(['app:', 'app:user:'])}
        onToggleFolder={() => {}}
        selectedKey={null}
        selectedKeys={new Set()}
        onSelectKey={() => {}}
        onToggleKey={() => {}}
        onToggleKeys={() => {}}
        onKeyContextMenu={() => {}}
        onDeleteRow={() => {}}
        loading={false}
        hasMore={false}
        onLoadMore={() => {}}
        separator={':'}
        emptyState={null}
        pattern=""
        filterActive={false}
        onSelectAllLoaded={() => {}}
        onRefresh={() => {}}
        onClearSelection={() => {}}
      />,
    );
    expect(targetIndexBefore).toBe('4');
    // The subtree collapsed; the row survived with its identity and its index.
    const after = container.querySelector<HTMLElement>('[data-row-path="app:cfg:"]')!;
    expect(after).toBe(sameNode);
    expect(after.dataset.rowIndex).toBe('4');
    expect(sameNode.dataset.rowPath).toBe('app:cfg:');
  });

  it('tells folders from keys for the shell, by the row not by its index', () => {
    // The shell asks "is this row a branch" with no index to hand, which is
    // only answerable because the model carries the distinction.
    expect(isFolderRow(folder('a:', 0))).toBe(true);
    // A breadcrumb is still a folder — it is drawn as one, it just is not a
    // destination, and the shell is not allowed to guess otherwise.
    expect(isFolderRow(folder('a:', 0, { breadcrumb: true }))).toBe(true);
    expect(isFolderRow(key('a:1', 1))).toBe(false);
  });
});

describe('key tree · virtualization', () => {
  it('sizes the grid for the whole list, not for the window', () => {
    const { container } = renderList();
    expect(treeGrid(container).style.height).toBe(`${TREE.length * ROW_HEIGHT}px`);
  });

  it('mounts one row per list entry and gives each a full row height', () => {
    const { container } = renderList();
    const rows = rowEls(container);
    expect(rows.map((r) => r.style.height)).toEqual(Array(TREE.length).fill(`${ROW_HEIGHT}px`));
    expect(rows.map((r) => r.dataset.rowIndex)).toEqual(['0', '1', '2', '3', '4', '5']);
  });

  it('paints rows outside its window when the virtualizer reports fewer', () => {
    // 200 rows, window of 2: the shell must mount what it is given and still
    // reserve the full scroll height.
    const many: KeyTreeRow[] = Array.from({ length: 200 }, (_, i) => key(`k:${i}`, 0));
    const { container } = renderList({ treeRows: many });
    expect(treeGrid(container).style.height).toBe(`${200 * ROW_HEIGHT}px`);
    expect(rowEls(container).length).toBe(200);
  });
});

describe('key tree · keyboard on the shared shell', () => {
  it('moves down the list and marks the landed row active', () => {
    const { container } = renderList();
    const grid = treeGrid(container);
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(shellEl(container).dataset.activeIndex).toBe('0');
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    expect(shellEl(container).dataset.activeIndex).toBe('1');
    expect(rowEls(container)[1]!.dataset.active).toBe('true');
  });

  it('folds and unfolds a folder without moving the cursor off it', () => {
    const { container, onToggleFolder } = renderList();
    const grid = treeGrid(container);
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowLeft' });
    expect(onToggleFolder).toHaveBeenCalledWith('app:');
    expect(shellEl(container).dataset.activeIndex).toBe('0');
  });

  it('activates a key row on Enter', () => {
    const { container, onSelectKey } = renderList();
    const grid = treeGrid(container);
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onSelectKey).toHaveBeenCalledWith('app:user:1');
  });

  it('skips a breadcrumb instead of landing on it', () => {
    const withCrumb: KeyTreeRow[] = [
      folder('app:', 0, { breadcrumb: true }),
      folder('app:user:', 1),
      key('app:user:1', 2),
    ];
    const { container } = renderList({ treeRows: withCrumb });
    const grid = treeGrid(container);
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    const active = shellEl(container).dataset.activeIndex;
    expect(active).toBe('1');
    expect(rowEls(container)[0]!.dataset.breadcrumb).toBe('true');
  });
});
