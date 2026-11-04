import { useCallback, useEffect, useMemo, useState } from 'react';
import type { KeyEntry } from '@datazen/driver-sdk';
import { buildFlatTreeRows, buildServerTreeRows, keyUnderFolder } from './keyTree';
import { countSelectableRows, filterKeysByPattern, filterTreeRowsByPattern } from './keyTreeFilter';
import { isGlobalPattern, resolveTreeEmptyState, type TreeEmptyState } from './treeEmptyState';
import {
  readTreePrefs,
  treePrefsBucket,
  writeTreePrefs,
  type TreePrefs,
  type TreeViewMode,
} from './treePreferences';
import { useKeyTree } from './useKeyTree';

/**
 * R3's view state, in the two halves the composition root needs at different
 * times (task book D-3 + D-8):
 *
 *  - {@link useTreePreferences} is the *durable* half (tree-vs-list + the
 *    grouping separator, persisted per connection). It has to run before
 *    `useKeyTree`, because the separator is a `list_children` argument: the
 *    server folds on the same character the UI shows.
 *  - {@link useKeyTreeView} is the *derivation* half: the row list for whichever
 *    mode is on, and which of the four named empty states (I-11) applies.
 *
 * Carving both out of `RedisWorkbench` is what keeps that file at its D-0 size
 * budget instead of re-growing it ("不许换个地方堆") — the workbench stays one
 * line per half.
 */
export interface UseTreePreferencesOptions {
  /** Stable host connection id when available; falls back to the live session. */
  connectionId?: string;
  dbSessionId: string;
}

export function useTreePreferences({ connectionId, dbSessionId }: UseTreePreferencesOptions) {
  const bucket = treePrefsBucket(connectionId, dbSessionId);
  const [prefs, setPrefs] = useState<TreePrefs>(() => readTreePrefs(bucket));

  // A different connection — or a reconnect, which changes the session bucket —
  // re-reads its own persisted pair instead of inheriting the previous one.
  useEffect(() => {
    setPrefs(readTreePrefs(bucket));
  }, [bucket]);

  const persist = useCallback(
    (next: TreePrefs) => {
      setPrefs(next);
      writeTreePrefs(bucket, next);
    },
    [bucket],
  );

  const setView = useCallback(
    (view: TreeViewMode) => persist({ ...prefs, view }),
    [persist, prefs],
  );

  const setSeparator = useCallback(
    (separator: string) => persist({ ...prefs, separator }),
    [persist, prefs],
  );

  return { view: prefs.view, separator: prefs.separator, setView, setSeparator, bucket };
}

export interface UseKeyTreeViewOptions {
  /* ── forwarded to `useKeyTree` ──────────────────────────────────────────── */
  dbSessionId: string;
  dbIndex: number;
  enabled: boolean;
  noTtlOnly: boolean;
  keyType: string;
  /** Stable host connection id when available; falls back to the live session. */
  connectionId?: string;
  /* ── the flat `scan_keys` list, i.e. what `list` mode shows ─────────────── */
  loadedKeys: KeyEntry[];
  /**
   * What the search row currently holds (typed, maybe unapplied). Only the
   * *filter detection* of I-11 reads it — an unapplied input cannot make rows
   * disappear, so it must not be blamed for them either.
   */
  pattern: string;
  /**
   * The pattern the current scan belongs to (D-2 / redis-tree-ui-BUG-001). This
   * is the one that narrows the tree: it becomes the root `list_children` prefix
   * (via `useKeyTree`) *and* the client-side glob filter over the loaded rows
   * (via `keyTreeFilter`), and it re-keys the tree when it changes.
   */
  appliedPattern: string;
  loading: boolean;
  /** Scan cursor of the flat list still open. */
  scanOpen: boolean;
}

/**
 * Composition of the two halves above: preferences → server-driven levels →
 * rows → named empty state. One call site, because the separator has to reach
 * both the `list_children` request *and* the row fold, and a second one anywhere
 * else is how the UI ends up claiming a `.` tree over a `:` scan. The same goes
 * for the applied pattern: it reaches the request, the row filter and the reset
 * trigger from here, so "what is on screen" has exactly one owner.
 */
export function useKeyTreeView({
  connectionId,
  dbSessionId,
  dbIndex,
  enabled,
  noTtlOnly,
  keyType,
  loadedKeys,
  pattern,
  appliedPattern,
  loading,
  scanOpen,
}: UseKeyTreeViewOptions) {
  const prefs = useTreePreferences({ connectionId, dbSessionId });
  const tree = useKeyTree({
    dbSessionId,
    dbIndex,
    enabled,
    noTtlOnly,
    keyType,
    separator: prefs.separator,
    appliedPattern,
  });

  /*
   * The separator is not a client-side cosmetic: switching it re-fetches the
   * server levels (see `useKeyTree`), so `treeRows` keys off it here rather than
   * post-processing a `:`-built tree. `list` turns namespace folding off entirely,
   * which is the escape hatch for keys whose names contain the separator.
   *
   * The applied pattern then narrows whatever the fold produced (BUG-001): a
   * folder survives on its own glob match, a surviving deep row pulls its
   * non-matching ancestors back as breadcrumbs, and nothing matched ⇒ zero rows
   * — which is exactly what makes I-11's `no-match` reachable in the default
   * view. `list` mode shows the flat `scan_keys` output, which the *server*
   * already cut with the same pattern, so it is never filtered a second time.
   */
  const treeRows = useMemo(() => {
    if (prefs.view === 'list') return buildFlatTreeRows(loadedKeys);
    /*
     * A *collapsed* folder's subtree is the one thing the folded rows cannot
     * speak about, so the filter is handed a probe over the filtered key set:
     * the folder survives — clickable, expandable — when the pattern admits any
     * key under its prefix. That key set is the flat `scan_keys` result cut by
     * the same glob, and it is also what 「全选已加载」 and the folder checkbox
     * cascade read, so a folder and the counter cannot drift apart again.
     * `*user*` therefore keeps `app:`; `zzz*` drops it.
     */
    const visible = filterKeysByPattern(
      loadedKeys.map((entry) => entry.key),
      appliedPattern,
    );
    return filterTreeRowsByPattern(
      buildServerTreeRows(tree.levels, tree.expanded, prefs.separator),
      appliedPattern,
      (folderPath) => visible.some((key) => keyUnderFolder(key, folderPath, prefs.separator)),
    );
  }, [prefs.view, prefs.separator, tree.levels, tree.expanded, loadedKeys, appliedPattern]);

  const scanning = tree.scanning || scanOpen;

  /*
   * Single source of truth for "which keys are on screen" (BUG-001). R1's
   * counter, 「全选已加载」, the folder-checkbox cascade and I-11's `{loaded}`
   * all read {@link visibleKeys}, so they can no longer disagree with the column
   * about what the applied pattern kept — that contradiction pair is the bug.
   *
   *  - tree mode: the loaded key names cut by the same client-side glob that cut
   *    the rows, so a *collapsed* matching folder still cascades over its whole
   *    (matching) subtree, exactly as it did unfiltered;
   *  - list mode: `scan_keys` already applied the pattern server-side, so the
   *    names are used as they are — filtering twice would let two glob
   *    implementations vote on one list.
   */
  const visibleKeys = useMemo(() => {
    const loaded = loadedKeys.map((entry) => entry.key);
    return prefs.view === 'list' ? loaded : filterKeysByPattern(loaded, appliedPattern);
  }, [prefs.view, loadedKeys, appliedPattern]);

  /*
   * Which pattern the named empty state is allowed to blame. An *applied*
   * pattern owns the verdict; the typed-but-unapplied input only counts while
   * nothing narrower is in force (it is what the search row shows the user, and
   * `resolveTreeEmptyState` must keep saying `no-match` for it).
   */
  const filterPattern = isGlobalPattern(appliedPattern) ? pattern : appliedPattern;

  const emptyState: TreeEmptyState | null = useMemo(
    () =>
      resolveTreeEmptyState({
        // Path breadcrumbs are decoration, not rows.
        rowCount: countSelectableRows(treeRows),
        loading,
        rootError: tree.rootError,
        scanning,
        pattern: filterPattern,
        keyType,
        noTtlOnly,
      }),
    [treeRows, loading, tree.rootError, scanning, filterPattern, keyType, noTtlOnly],
  );

  return {
    tree,
    mode: prefs.view,
    separator: prefs.separator,
    setMode: prefs.setView,
    setSeparator: prefs.setSeparator,
    treeRows,
    visibleKeys,
    appliedPattern,
    filterPattern,
    /** A filter is in force ⇒ an unfiltered `(n+)` remainder must say so. */
    filterActive: !isGlobalPattern(filterPattern),
    emptyState,
    scanning,
  };
}

/** What {@link KeyTreePane} consumes: the view, its rows and the owned tree state. */
export type KeyTreeView = ReturnType<typeof useKeyTreeView>;
