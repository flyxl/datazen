import { describe, expect, it } from 'vitest';
import type { KeyEntry } from '@datazen/driver-sdk';
import type { ChildEntry } from '../shared/redisInvoke';
import type { KeyTreeRow } from '../key-browser/keyTree';
import {
  EMPTY_LEVEL,
  anyLevelScanning,
  applyFetch,
  beginFetch,
  childIdentity,
  fetchCursorFor,
  markFetchFailed,
  mergeChildren,
  rootLevelFailed,
  type TreeLevel,
} from '../key-browser/treeLevels';
import {
  INDENT_BASE,
  INDENT_STEP,
  ROW_HEIGHT,
  firstChildIndex,
  firstVisibleIndex,
  nextActiveIndex,
  nextNavigableIndex,
  parentIndexOf,
  rowIndent,
  stickyFolderChain,
  treeNavAction,
} from '../key-browser/treeRowSpec';
import {
  DEFAULT_TREE_PREFS,
  TREE_PREFS_STORAGE_KEY,
  asSeparatorChoice,
  isTreeViewMode,
  readTreePrefs,
  treePrefsBucket,
  writeTreePrefs,
} from '../key-browser/treePreferences';
import {
  TREE_EMPTY_STATE_KEYS,
  hasActiveTreeFilter,
  isGlobalPattern,
  resolveTreeEmptyState,
  type TreeEmptySignal,
} from '../key-browser/treeEmptyState';

/* ── fixtures ─────────────────────────────────────────────────────────────── */

function folder(prefix: string, count: number): ChildEntry {
  return { kind: 'folder', prefix, count };
}

function leaf(key: string, ttl = -1): ChildEntry {
  return { kind: 'key', key, keyType: 'string', ttl, logicalLen: 5, memBytes: null };
}

function entry(key: string): KeyEntry {
  return { key, keyType: 'string', ttl: -1, size: 0, preview: '' };
}

function folderRow(path: string, depth: number, count = 2): KeyTreeRow {
  return { kind: 'folder', path, label: path, depth, count };
}

function keyRow(key: string, depth: number): KeyTreeRow {
  return { kind: 'key', entry: entry(key), depth, label: key };
}

/** A fake Web Storage that never throws, plus an optional pre-seeded payload. */
function fakeStorage(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, value),
    removeItem: (key: string) => void map.delete(key),
    raw: () => map.get(TREE_PREFS_STORAGE_KEY) ?? null,
  };
}

const signal = (over: Partial<TreeEmptySignal> = {}): TreeEmptySignal => ({
  rowCount: 0,
  loading: false,
  rootError: false,
  scanning: false,
  pattern: '*',
  keyType: 'all',
  noTtlOnly: false,
  ...over,
});

/* ── D-5 / I-4: the level fold is a replayable state machine ──────────────── */

describe('treeLevels: mergeChildren (I-4 loaded subset)', () => {
  it('identifies children by prefix / key, not by position', () => {
    expect(childIdentity(folder('app:', 3))).toBe('f:app:');
    expect(childIdentity(leaf('app:1'))).toBe('k:app:1');
  });

  it('appends unseen children and never duplicates a known one', () => {
    const merged = mergeChildren([folder('app:', 1), leaf('root')], [leaf('app:1'), leaf('root')]);
    expect(merged.map(childIdentity)).toEqual(['f:app:', 'k:root', 'k:app:1']);
  });

  it('lets a later reply win for a key but only ever grow a folder count', () => {
    const merged = mergeChildren(
      [folder('app:', 9), leaf('a:1', 60)],
      [folder('app:', 4), leaf('a:1', -1)],
    );
    // `count_matching` under-reports while the folder's own scan is open, so a
    // refresh must not make `(9)` walk backwards to `(4)`.
    expect(merged[0]).toEqual(folder('app:', 9));
    // …but a key's mutable attributes are authoritative from the newest reply.
    expect(merged[1]).toEqual(leaf('a:1', -1));
  });
});

describe('treeLevels: fetch modes (I-4 refresh / collapse)', () => {
  const scanning = applyFetch(beginFetch(EMPTY_LEVEL, 'reset'), 'reset', {
    children: [folder('app:', 2)],
    cursor: 17,
  });

  it('reset starts a different tree from cursor 0 and keeps nothing', () => {
    expect(fetchCursorFor(scanning, 'reset')).toBe(0);
    expect(beginFetch(scanning, 'reset')).toEqual({ ...EMPTY_LEVEL, loading: true });
  });

  it('continue pages forward from the stored cursor', () => {
    expect(fetchCursorFor(scanning, 'continue')).toBe(17);
    const finished = applyFetch(scanning, 'continue', { children: [leaf('zed')], cursor: 0 });
    expect(finished.done).toBe(true);
    expect(finished.children).toEqual([folder('app:', 2), leaf('zed')]);
  });

  it('rescan on a finished level keeps the visible subset while re-walking', () => {
    const opened = beginFetch(finishedAgain(), 'rescan');
    // The I-4 violation this unit exists to kill: refresh used to blank the tree.
    expect(opened.children).toEqual([folder('app:', 2)]);
    expect(opened.pass).toEqual([]);
    expect(opened.cursor).toBe(0);
    expect(fetchCursorFor(opened, 'rescan')).toBe(0);
  });

  it('an authoritative pass only lands when it wraps, so nothing flickers away', () => {
    const opened = beginFetch(finishedAgain(), 'rescan');
    const mid = applyFetch(opened, 'rescan', { children: [leaf('a')], cursor: 41 });
    // `app:` is still on screen even though this page does not contain it.
    expect(mid.children).toEqual([folder('app:', 2)]);
    expect(mid.pass).toEqual([leaf('a')]);
    expect(mid.done).toBe(false);
    const wrapped = applyFetch(mid, 'rescan', { children: [leaf('b')], cursor: 0 });
    // Wrapped ⇒ the pass replaces the level, which drops the deleted folder.
    expect(wrapped.children).toEqual([leaf('a'), leaf('b')]);
    expect(wrapped).toMatchObject({ pass: null, done: true, cursor: 0, error: false });
  });

  it('rescan on an unfinished level does not restart its scan', () => {
    // "Each folder's cursor survives a refresh" — the pass continues in place.
    expect(fetchCursorFor(scanning, 'rescan')).toBe(17);
    expect(beginFetch(scanning, 'rescan').pass).toBeNull();
    const grown = applyFetch(beginFetch(scanning, 'rescan'), 'rescan', {
      children: [leaf('late')],
      cursor: 88,
    });
    expect(grown).toMatchObject({ cursor: 88, done: false, pass: null });
    expect(grown.children).toEqual([folder('app:', 2), leaf('late')]);
  });

  it('a failed continue keeps the loaded subset and records the failure', () => {
    const failed = markFetchFailed(scanning, 'continue');
    expect(failed).toMatchObject({ loading: false, error: true, done: true });
    expect(failed.children).toEqual([folder('app:', 2)]);
  });

  it('a failed reset has no subset to keep', () => {
    expect(markFetchFailed(EMPTY_LEVEL, 'reset')).toMatchObject({
      error: true,
      done: true,
      loading: false,
    });
  });

  /*
   * [redis-tree-ui-BUG-002] — the `rescan` branch with an **open** authoritative
   * pass. Round 1 had zero coverage for this arm, which is why the whole gate
   * stayed green while a failed refresh pinned one level as "scanning" forever
   * (R1's `N+` and I-11's `interrupted` both read `anyLevelScanning`). Full state
   * machine: entry, behaviour inside the state, and both exit transitions.
   */
  describe('[redis-tree-ui-BUG-002] a failed rescan closes the level', () => {
    /** A finished root level holding one folder, mid-rescan with `pass` open. */
    function midRescan(): TreeLevel {
      return beginFetch(finishedAgain(), 'rescan');
    }

    it('entry + abandon: the failed pass closes and stops claiming a scan', () => {
      const opened = midRescan();
      expect(opened.pass).toEqual([]);
      expect(anyLevelScanning({ '': opened })).toBe(true);

      const failed = markFetchFailed(opened, 'rescan');
      // I-4 half: the loaded subset survives the failure …
      expect(failed.children).toEqual([folder('app:', 2)]);
      // … and the abandoned pass must not stay open (TreeLevel.pass contract).
      expect(failed).toMatchObject({ loading: false, error: true, done: true });
      expect(failed.pass).toBeNull();
      // Exit transition 1: the level no longer claims to be scanning.
      expect(anyLevelScanning({ '': failed })).toBe(false);
    });

    it('exit transition 2: the next successful rescan clears the error (self-heal)', () => {
      const failed = markFetchFailed(midRescan(), 'rescan');
      expect(failed.error).toBe(true);
      const healed = applyFetch(beginFetch(failed, 'rescan'), 'rescan', {
        children: [folder('app:', 3), leaf('fresh')],
        cursor: 0,
      });
      expect(healed).toMatchObject({ error: false, done: true, loading: false });
      expect(healed.pass).toBeNull();
      expect(healed.children).toEqual([folder('app:', 3), leaf('fresh')]);
      expect(anyLevelScanning({ '': healed })).toBe(false);
    });

    it('a failure mid-pass drops the partial pass, not the last good subset', () => {
      // The pass may already hold pages when the failure lands; those partial
      // results are abandoned with it (`children` keep the previous good set).
      const withPass = applyFetch(midRescan(), 'rescan', {
        children: [leaf('partial')],
        cursor: 41,
      });
      expect(withPass).toMatchObject({ done: false, pass: [leaf('partial')] });
      const failed = markFetchFailed(withPass, 'rescan');
      expect(failed.pass).toBeNull();
      expect(failed.done).toBe(true);
      expect(failed.children).toEqual([folder('app:', 2)]);
      expect(anyLevelScanning({ '': failed })).toBe(false);
    });
  });

  function finishedAgain(): TreeLevel {
    return applyFetch(beginFetch(EMPTY_LEVEL, 'reset'), 'reset', {
      children: [folder('app:', 2)],
      cursor: 0,
    });
  }
});

describe('treeLevels: cross-level signals (I-11 inputs)', () => {
  it('any open cursor or in-flight pass means "this is a subset"', () => {
    expect(anyLevelScanning({ '': finishedAgainStatic() })).toBe(false);
    expect(anyLevelScanning({ '': { ...finishedAgainStatic(), cursor: 5, done: false } })).toBe(
      true,
    );
    expect(anyLevelScanning({ '': { ...finishedAgainStatic(), pass: [] } })).toBe(true);
    expect(anyLevelScanning({})).toBe(false);
  });

  it('only a failed root can claim "we know nothing"', () => {
    expect(rootLevelFailed({ '': { ...finishedAgainStatic(), error: true } })).toBe(true);
    expect(
      rootLevelFailed({
        '': finishedAgainStatic(),
        'app:': { ...finishedAgainStatic(), error: true },
      }),
    ).toBe(false);
    expect(rootLevelFailed({})).toBe(false);
  });

  function finishedAgainStatic(): TreeLevel {
    return applyFetch(beginFetch(EMPTY_LEVEL, 'reset'), 'reset', {
      children: [leaf('a')],
      cursor: 0,
    });
  }
});

/* ── D-4 / D-7: row geometry, sticky chain, keyboard map ──────────────────── */

describe('treeRowSpec: row spec (PRD §3.2 行规格)', () => {
  it('pins rows to 30px so the virtualizer and the sticky stack agree', () => {
    expect(ROW_HEIGHT).toBe(30);
    expect(firstVisibleIndex(0, 10)).toBe(0);
    expect(firstVisibleIndex(30, 10)).toBe(1);
    expect(firstVisibleIndex(95, 10)).toBe(3);
    expect(firstVisibleIndex(-10, 10)).toBe(0);
    // Clamped, so a scrolled-past-the-end viewport still pins a real chain.
    expect(firstVisibleIndex(10_000, 10)).toBe(9);
    expect(firstVisibleIndex(100, 0)).toBe(0);
  });

  it('indents 4 + depth×10 px instead of the old 8 + depth×16', () => {
    expect(INDENT_BASE).toBe(4);
    expect(INDENT_STEP).toBe(10);
    expect([rowIndent(0), rowIndent(1), rowIndent(2), rowIndent(5)]).toEqual([4, 14, 24, 54]);
    expect(rowIndent(-2)).toBe(4);
  });
});

describe('treeRowSpec: multi-level sticky header chain', () => {
  const rows: KeyTreeRow[] = [
    folderRow('app:', 0),
    folderRow('app:cache:', 1),
    keyRow('app:cache:1', 2),
    keyRow('app:cache:2', 2),
    folderRow('app:db:', 1),
    keyRow('app:db:1', 2),
    keyRow('root', 0),
  ];

  /**
   * `stickyFolderChain` only ever yields `kind: 'folder'` rows, which are the
   * only variant carrying `path`; the read stays a plain property access so a
   * leaked key row would still surface as `undefined` rather than being masked.
   */
  const chainPaths = (all: KeyTreeRow[], topIndex: number): (string | undefined)[] =>
    stickyFolderChain(all, topIndex).map((r) => (r as { path: string }).path);

  it('is empty while the top row is still inside the viewport', () => {
    expect(stickyFolderChain(rows, 0)).toEqual([]);
  });

  it('pins the strict ancestors of the top row, outermost first', () => {
    // `app:` has scrolled above the edge the moment `app:cache:` reaches it.
    expect(chainPaths(rows, 1)).toEqual(['app:']);
    expect(chainPaths(rows, 2)).toEqual(['app:', 'app:cache:']);
    expect(chainPaths(rows, 4)).toEqual(['app:', 'app:cache:']);
    // Entering the sibling folder swaps exactly one pinned row (enter transition).
    expect(chainPaths(rows, 5)).toEqual(['app:', 'app:db:']);
    // Leaving the whole subtree clears the stack (exit transition).
    expect(chainPaths(rows, 7)).toEqual([]);
    // A scrolled-past-the-end index clamps instead of inventing a chain.
    expect(chainPaths(rows, 99)).toEqual([]);
  });
});

describe('treeRowSpec: keyboard navigation (I-9)', () => {
  const rows: KeyTreeRow[] = [
    folderRow('app:', 0),
    folderRow('app:cache:', 1),
    keyRow('app:cache:1', 2),
    keyRow('root', 0),
  ];

  it('maps the documented chords and nothing else', () => {
    expect(treeNavAction({ key: 'ArrowDown' })).toBe('next');
    expect(treeNavAction({ key: 'ArrowUp' })).toBe('previous');
    expect(treeNavAction({ key: 'ArrowRight' })).toBe('expand');
    expect(treeNavAction({ key: 'ArrowLeft' })).toBe('fold');
    expect(treeNavAction({ key: 'Enter' })).toBe('activate');
    expect(treeNavAction({ key: 'Escape' })).toBe('clear');
    expect(treeNavAction({ key: 'a', metaKey: true })).toBe('select-all');
    expect(treeNavAction({ key: 'r', ctrlKey: true })).toBe('refresh');
    // A text input keeps its own keys; ⌘-something-else is not ours to eat.
    expect(treeNavAction({ key: 'a' })).toBeNull();
    expect(treeNavAction({ key: 'ArrowDown', metaKey: true })).toBeNull();
    expect(treeNavAction({ key: 'k', ctrlKey: true })).toBeNull();
    expect(treeNavAction({ key: ' ' })).toBeNull();
  });

  it('moves the active row and clamps instead of jumping the viewport', () => {
    expect(nextActiveIndex(-1, 1, 4)).toBe(0);
    expect(nextActiveIndex(-1, -1, 4)).toBe(3);
    expect(nextActiveIndex(0, 1, 4)).toBe(1);
    expect(nextActiveIndex(0, -1, 4)).toBe(0);
    expect(nextActiveIndex(3, 1, 4)).toBe(3);
    expect(nextActiveIndex(2, 1, 0)).toBe(-1);
  });

  it('steps into an open folder and back out to its parent', () => {
    expect(firstChildIndex(rows, 0)).toBe(1);
    expect(firstChildIndex(rows, 1)).toBe(2);
    // A collapsed folder / a leaf has no child row to step into.
    expect(firstChildIndex(rows, 2)).toBe(-1);
    expect(firstChildIndex(rows, 3)).toBe(-1);
    expect(parentIndexOf(rows, 2)).toBe(1);
    expect(parentIndexOf(rows, 1)).toBe(0);
    // A root row cannot fold further.
    expect(parentIndexOf(rows, 3)).toBe(3);
    expect(parentIndexOf(rows, 9)).toBe(9);
  });
});

/* ── D-3: per-connection preferences ──────────────────────────────────────── */

describe('treePreferences (R3 / D-3)', () => {
  it('keys the bucket off the stable connection id, falling back to the session', () => {
    expect(treePrefsBucket(' conn-7 ', 'sess-1')).toBe('conn-7');
    expect(treePrefsBucket(undefined, 'sess-1')).toBe('sess-1');
    expect(treePrefsBucket('   ', 'sess-1')).toBe('sess-1');
  });

  it('round-trips through storage and keeps two connections apart', () => {
    const storage = fakeStorage();
    writeTreePrefs('conn-a', { view: 'list', separator: '.' }, storage);
    expect(storage.raw()).toContain('"conn-a"');
    expect(readTreePrefs('conn-a', storage)).toEqual({ view: 'list', separator: '.' });
    expect(readTreePrefs('conn-b', storage)).toEqual(DEFAULT_TREE_PREFS);
  });

  it('rejects a corrupt payload, an old shape and a foreign separator', () => {
    expect(readTreePrefs('c', fakeStorage({ [TREE_PREFS_STORAGE_KEY]: '{not json' }))).toEqual(
      DEFAULT_TREE_PREFS,
    );
    expect(readTreePrefs('c', fakeStorage({ [TREE_PREFS_STORAGE_KEY]: '[1,2]' }))).toEqual(
      DEFAULT_TREE_PREFS,
    );
    expect(readTreePrefs('c', fakeStorage({ [TREE_PREFS_STORAGE_KEY]: 'null' }))).toEqual(
      DEFAULT_TREE_PREFS,
    );
    // Partially valid: the known field survives, the junk falls back.
    expect(
      readTreePrefs('c', fakeStorage({ [TREE_PREFS_STORAGE_KEY]: '{"c":{"view":"list"}}' })),
    ).toEqual({ view: 'list', separator: DEFAULT_TREE_PREFS.separator });
    expect(
      readTreePrefs('c', fakeStorage({ [TREE_PREFS_STORAGE_KEY]: '{"c":{"sep":"|"}}' })),
    ).toEqual(DEFAULT_TREE_PREFS);
    expect(readTreePrefs('', fakeStorage())).toEqual(DEFAULT_TREE_PREFS);
  });

  it('normalises on write, so an unsupported value never reaches disk', () => {
    const storage = fakeStorage();
    const written = writeTreePrefs('c', { view: 'grid' as 'tree', separator: '|' }, storage);
    expect(written).toEqual(DEFAULT_TREE_PREFS);
    expect(readTreePrefs('c', storage)).toEqual(DEFAULT_TREE_PREFS);
  });

  it('survives storage that throws and storage that is absent', () => {
    const hostile = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('quota');
      },
      removeItem: () => {},
    };
    expect(readTreePrefs('c', hostile)).toEqual(DEFAULT_TREE_PREFS);
    expect(writeTreePrefs('c', { view: 'list', separator: '/' }, hostile)).toEqual({
      view: 'list',
      separator: '/',
    });
    expect(readTreePrefs('c', null)).toEqual(DEFAULT_TREE_PREFS);
  });

  it('validates the two value domains', () => {
    expect(['tree', 'list'].every(isTreeViewMode)).toBe(true);
    expect(isTreeViewMode('flat')).toBe(false);
    expect(asSeparatorChoice('/')).toBe('/');
    expect(asSeparatorChoice('::')).toBe(DEFAULT_TREE_PREFS.separator);
    expect(asSeparatorChoice(undefined)).toBe(DEFAULT_TREE_PREFS.separator);
  });
});

/* ── D-8 / I-11: the four named empty states ──────────────────────────────── */

describe('treeEmptyState (I-11)', () => {
  it('says nothing while there are rows or a fetch in flight', () => {
    expect(resolveTreeEmptyState(signal({ rowCount: 3 }))).toBeNull();
    expect(resolveTreeEmptyState(signal({ loading: true }))).toBeNull();
  });

  it('names the four distinct facts', () => {
    expect(resolveTreeEmptyState(signal())).toBe('none');
    expect(resolveTreeEmptyState(signal({ pattern: 'user:*' }))).toBe('no-match');
    expect(resolveTreeEmptyState(signal({ keyType: 'hash' }))).toBe('no-match');
    expect(resolveTreeEmptyState(signal({ noTtlOnly: true }))).toBe('no-match');
    expect(resolveTreeEmptyState(signal({ scanning: true }))).toBe('interrupted');
    expect(resolveTreeEmptyState(signal({ rootError: true }))).toBe('no-permission');
  });

  it('ranks a failed root above an open cursor above a filter', () => {
    // The precedence *is* the contract: a rejected scan may not be reported as
    // "your filter matched nothing", and an unfinished scan may not blame the
    // filter either.
    const all = signal({ rootError: true, scanning: true, pattern: 'user:*' });
    expect(resolveTreeEmptyState(all)).toBe('no-permission');
    expect(resolveTreeEmptyState({ ...all, rootError: false })).toBe('interrupted');
    expect(resolveTreeEmptyState({ ...all, rootError: false, scanning: false })).toBe('no-match');
  });

  it('exposes only i18n keys, never copy', () => {
    expect(Object.values(TREE_EMPTY_STATE_KEYS)).toEqual([
      'redis.tree.empty.none',
      'redis.tree.empty.noMatch',
      'redis.tree.empty.interrupted',
      'redis.tree.empty.noPermission',
    ]);
    for (const value of Object.values(TREE_EMPTY_STATE_KEYS)) {
      expect(value).toMatch(/^redis\.tree\.empty\./);
    }
  });

  it('treats a global pattern as "no filter"', () => {
    expect(isGlobalPattern('*')).toBe(true);
    expect(isGlobalPattern('  ')).toBe(true);
    expect(isGlobalPattern('**')).toBe(false);
    expect(hasActiveTreeFilter(signal({ pattern: ' * ' }))).toBe(false);
  });
});

describe('[redis-tree-ui-BUG-004] nextNavigableIndex walks past non-navigable rows', () => {
  /*
   * Round 1 claimed "navigation passes breadcrumbs through" with zero execution
   * evidence, so the walk is now a pure helper and this is its state machine:
   * enter (start on a blocked candidate), state (keep walking in the same
   * direction), exit (run off either end ⇒ -1 — callers stay put).
   * `first` is inclusive: callers advance before asking.
   */
  const rowsOf =
    (navigable: boolean[]): ((i: number) => boolean) =>
    (i) =>
      i >= 0 && i < navigable.length && navigable[i];

  it('entry: starting on a blocked candidate walks forward to the next free row', () => {
    const free = rowsOf([false, false, true, false, true]);
    expect(nextNavigableIndex(0, 1, 5, free)).toBe(2);
    expect(nextNavigableIndex(1, 1, 5, free)).toBe(2);
    expect(nextNavigableIndex(3, 1, 5, free)).toBe(4);
    // Inclusive: a free candidate is returned as-is, no accidental skip.
    expect(nextNavigableIndex(2, 1, 5, free)).toBe(2);
  });

  it('the same walk backwards mirrors it exactly', () => {
    const free = rowsOf([true, false, true, false, false]);
    expect(nextNavigableIndex(4, -1, 5, free)).toBe(2);
    expect(nextNavigableIndex(3, -1, 5, free)).toBe(2);
    expect(nextNavigableIndex(1, -1, 5, free)).toBe(0);
    expect(nextNavigableIndex(0, -1, 5, free)).toBe(0);
  });

  it('exit: nothing free in that direction ⇒ -1, never a blocked index', () => {
    const none = rowsOf([false, false, false]);
    expect(nextNavigableIndex(0, 1, 3, none)).toBe(-1);
    expect(nextNavigableIndex(2, -1, 3, none)).toBe(-1);
    // Trailing blocked rows: the forward walk runs off the end …
    expect(nextNavigableIndex(2, 1, 4, rowsOf([true, true, false, false]))).toBe(-1);
    // … and leading blocked rows make the backward walk run off the top.
    expect(nextNavigableIndex(1, -1, 4, rowsOf([false, false, true, true]))).toBe(-1);
  });

  it('an empty list has nowhere to go', () => {
    expect(nextNavigableIndex(0, 1, 0, rowsOf([]))).toBe(-1);
    expect(nextNavigableIndex(-1, 1, 0, rowsOf([]))).toBe(-1);
  });

  it('every row being a breadcrumb yields -1 for the whole sweep (leading-crumb case)', () => {
    // `data-row-count = 0` with rows still painted is exactly this shape: the
    // tree shows crumbs only, so no key press may land anywhere.
    const onlyCrumbs = rowsOf([false, false]);
    let index = -1;
    for (let step = 0; step < 4; step++) {
      const next = nextNavigableIndex(index + 1, 1, 2, onlyCrumbs);
      expect(next).toBe(-1);
      index = next < 0 ? index : next;
    }
  });

  it('caller-side contract: -1 means stay put, not "land on the skipped crumb"', () => {
    /*
     * `stepActiveIndex` used to return its *target* when the walk found nothing —
     * and the target is precisely the breadcrumb row it was skipping, so ↑ from
     * the row below a breadcrumb landed on it. The helper returns -1 and the
     * caller resolves: stay on the current row if it is still navigable, else
     * leave selection.
     */
    const free = rowsOf([false, true]); // index 0 = breadcrumb, 1 = real row
    const resolve = (walked: number, from: number) =>
      walked < 0 ? (free(from) ? from : -1) : walked;
    expect(nextNavigableIndex(0, -1, 2, free)).toBe(-1);
    expect(resolve(-1, 1)).toBe(1); // stayed on the real row
    expect(resolve(-1, 0)).toBe(-1); // current row itself is a crumb ⇒ deselect
  });
});
