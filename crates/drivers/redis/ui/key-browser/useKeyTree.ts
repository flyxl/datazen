import { useCallback, useEffect, useRef, useState } from 'react';
import { invokeListChildren } from '../shared/redisInvoke';
import { DEFAULT_SEPARATOR } from './keyTree';
import {
  EMPTY_LEVEL,
  anyLevelScanning,
  applyFetch,
  beginFetch,
  fetchCursorFor,
  markFetchFailed,
  rootLevelFailed,
  type LevelFetchMode,
  type TreeLevel,
} from './treeLevels';

const PAGE_SIZE = 500;

/**
 * First metacharacter of a glob: the run before it is purely literal and can be
 * sent to the server as a prefix. `?` and `*` are wildcards, `[` opens a class,
 * `\\` is the escape byte.
 */
const GLOB_METACHARACTER = /[*?[\\]/;

/**
 * The `list_children` prefix the root request should send for an applied R2
 * pattern (redis-tree-ui-BUG-001, the routing half of the fix).
 *
 * `list_children` has no pattern argument (its options are `sep` / `noTtlOnly` /
 * `keyType`, and a Rust-side contract change is outside this track), but it does
 * scan `{prefix}*` — so the *literal head* of the glob can be handed to the
 * server as a prefix and narrows the level the tree walks instead of pretending:
 *
 *  - blank / `*` (no filter, or a pattern that opens with a star) ⇒ `''`, the
 *    whole keyspace;
 *  - `app:*` / `app:user:*` ⇒ the literal head, kept whole because it already
 *    ends at a separator (`app:`);
 *  - a head that stops inside a segment (`app:us*`) ⇒ cut back to the last
 *    separator, since a prefix that is not a namespace boundary would pull in
 *    siblings the pattern excludes;
 *  - no separator at all (`zzz`) ⇒ the literal itself, i.e. scan `zzz*`.
 *
 * This is only a *server-side* narrowing: `keyTreeFilter.ts` still decides what
 * is visible, so an over-broad prefix can never show a key the pattern rejected.
 */
export function patternToTreePrefix(pattern: string, sep: string): string {
  const trimmed = pattern.trim();
  if (!trimmed || trimmed === '*') return '';
  /*
   * The head is the run of *purely literal* bytes before the first metacharacter.
   * That cut matters beyond `*` (BUG-003 consequence, round 2): `list_children`
   * re-globs `{prefix}*` **and** byte-slices keys at `prefix.len` when folding, so
   * a head like `[ac]` — a class, not four literal bytes — would make the server
   * strip four bytes off keys that never started with them. Truncating at the
   * first metacharacter keeps the prefix a true superset of the pattern (the
   * server may hand back more than the pattern admits, and the client filter cuts
   * that down); abandoning routing altogether would throw away a narrowing the
   * server can still do safely, e.g. `app:a?b*` still scans `app:*`.
   */
  const meta = trimmed.search(GLOB_METACHARACTER);
  const head = meta === -1 ? trimmed : trimmed.slice(0, meta);
  if (!head) return '';
  if (sep && head.endsWith(sep)) return head;
  const lastSep = head.lastIndexOf(sep);
  if (sep && lastSep >= 0) return head.slice(0, lastSep + sep.length);
  return head;
}

export interface UseKeyTreeOptions {
  dbSessionId: string;
  dbIndex: number;
  enabled: boolean;
  noTtlOnly?: boolean;
  keyType?: string;
  /**
   * R3 grouping separator (D-3). Sent to `list_children` as `sep` — previously the
   * call omitted it and silently used the server default, so the UI could show a
   * `:` tree while claiming to group on `.` — and it is part of the reset trigger:
   * prefixes belong to one separator, so switching means a new tree.
   */
  separator?: string;
  /**
   * The R2 pattern **as applied** (D-2 / redis-tree-ui-BUG-001). Two effects: its
   * literal head becomes the root `list_children` prefix (see
   * {@link patternToTreePrefix}), and it joins the reset triggers — the levels a
   * `zzz*` scan filled do not belong to the tree an `app:*` scan describes.
   * The key-level filtering itself happens in `keyTreeFilter.ts`.
   */
  appliedPattern?: string;
}

/**
 * Server-driven hierarchical key tree. Each expanded folder prefix owns one
 * `list_children` level (leaf keys + virtual folders) with its own SCAN cursor.
 *
 * The level transitions themselves live in `treeLevels.ts` as pure functions:
 * I-4 ("a refresh or a collapse never throws away what is loaded") is a property
 * of that sequence, and folding replies here is the only way to replay it in a
 * test. What stays in this hook is scheduling: one in-flight request per prefix
 * (`reqSeq` drops stale replies), reset on db/filter/separator change, and
 * `rescan` on refresh.
 */
export function useKeyTree({
  dbSessionId,
  dbIndex,
  enabled,
  noTtlOnly = false,
  keyType = 'all',
  separator,
  appliedPattern = '',
}: UseKeyTreeOptions) {
  const [levels, setLevels] = useState<Record<string, TreeLevel>>({});
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set<string>());
  const [rootLoading, setRootLoading] = useState(false);
  // Single synchronous source of truth for the fold functions: reading `levels`
  // inside an async callback would replay a stale cursor (the bug this replaces).
  const levelsRef = useRef<Record<string, TreeLevel>>({});
  const expandedRef = useRef<Set<string>>(expanded);
  // Guards against out-of-order responses when a prefix refetches quickly.
  const reqSeq = useRef<Record<string, number>>({});

  const sep = separator && separator.length > 0 ? separator : DEFAULT_SEPARATOR;
  /*
   * What the *root* request asks the server for (BUG-001 routing half). The
   * level keeps its state key `''` — that is what the row fold and the expand
   * bookkeeping address — only the request is narrowed: `zzz` ⇒ scan `zzz*`,
   * `app:*` ⇒ scan `app:*`, `*` / blank ⇒ scan `*` as before.
   */
  const rootPrefix = patternToTreePrefix(appliedPattern, sep);

  const writeLevel = useCallback((prefix: string, next: TreeLevel) => {
    levelsRef.current = { ...levelsRef.current, [prefix]: next };
    setLevels(levelsRef.current);
  }, []);

  const fetchLevel = useCallback(
    async (prefix: string, mode: LevelFetchMode) => {
      const seq = (reqSeq.current[prefix] ?? 0) + 1;
      reqSeq.current[prefix] = seq;
      const isRoot = prefix === '';
      // The root level's *state key* stays `''`; only the request narrows.
      const requestPrefix = isRoot ? rootPrefix : prefix;
      if (isRoot) setRootLoading(true);
      const before = levelsRef.current[prefix] ?? EMPTY_LEVEL;
      const started = beginFetch(before, mode);
      writeLevel(prefix, started);
      try {
        const result = await invokeListChildren(
          dbSessionId,
          dbIndex,
          requestPrefix,
          fetchCursorFor(before, mode),
          PAGE_SIZE,
          { sep, noTtlOnly, keyType },
        );
        if (reqSeq.current[prefix] !== seq) return; // stale response
        writeLevel(prefix, applyFetch(started, mode, result));
      } catch (e) {
        console.error('list_children failed:', e);
        if (reqSeq.current[prefix] === seq) writeLevel(prefix, markFetchFailed(started, mode));
      } finally {
        if (isRoot && reqSeq.current[prefix] === seq) setRootLoading(false);
      }
    },
    [dbSessionId, dbIndex, noTtlOnly, keyType, sep, rootPrefix, writeLevel],
  );

  /**
   * Start over: this is a *different* tree (db switch, filter change, separator
   * change, newly applied pattern), not a refresh. Still re-fetches every open
   * prefix, because a folder left expanded has to be filled again — otherwise it
   * renders as expanded and empty (I-4's "collapse and re-expand shows the same
   * rows" needs it too).
   */
  const loadRoot = useCallback(() => {
    reqSeq.current = {};
    levelsRef.current = {};
    setLevels({});
    void fetchLevel('', 'reset');
    for (const prefix of expandedRef.current) {
      if (prefix) void fetchLevel(prefix, 'reset');
    }
  }, [fetchLevel]);

  useEffect(() => {
    if (!enabled) return;
    loadRoot();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- re-run only on db / filter / separator / applied-pattern changes
  }, [enabled, dbSessionId, dbIndex, noTtlOnly, keyType, sep, appliedPattern]);

  /** Expand or collapse a folder. Collapsing keeps the level (I-4). */
  const toggleFolder = useCallback(
    (prefix: string) => {
      const next = new Set(expandedRef.current);
      if (next.has(prefix)) {
        next.delete(prefix);
      } else {
        next.add(prefix);
        if (!levelsRef.current[prefix]) void fetchLevel(prefix, 'reset');
      }
      expandedRef.current = next;
      setExpanded(next);
    },
    [fetchLevel],
  );

  /** Page one level forward from its own stored cursor. */
  const loadMore = useCallback(
    (prefix: string) => {
      const level = levelsRef.current[prefix];
      if (level && !level.done && !level.loading) void fetchLevel(prefix, 'continue');
    },
    [fetchLevel],
  );

  /**
   * ⌘R / post-write refresh (I-4). No `setLevels({})`: every *loaded* level keeps
   * its children and its cursor, a finished level restarts an authoritative pass
   * that only lands when it wraps, and an unfinished one just continues.
   */
  const refresh = useCallback(() => {
    const targets = new Set<string>(['' as string, ...Object.keys(levelsRef.current)]);
    for (const prefix of expandedRef.current) targets.add(prefix);
    for (const prefix of targets) void fetchLevel(prefix, 'rescan');
  }, [fetchLevel]);

  /** Forget the whole tree (db switch driven from the outside). */
  const clearTree = useCallback(() => {
    reqSeq.current = {};
    levelsRef.current = {};
    expandedRef.current = new Set();
    setLevels({});
    setExpanded(expandedRef.current);
  }, []);

  return {
    levels,
    expanded,
    rootLoading,
    /** Root `list_children` rejected ⇒ D-8's named `no-permission` state. */
    rootError: rootLevelFailed(levels),
    /** Any level still scanning ⇒ R1's `N+` and D-8's `interrupted`. */
    scanning: anyLevelScanning(levels),
    toggleFolder,
    loadMore,
    refresh,
    loadRoot,
    clearTree,
  };
}
export type { TreeLevel };
