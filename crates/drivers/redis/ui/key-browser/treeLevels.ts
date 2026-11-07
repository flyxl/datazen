import type { ChildEntry } from '../shared/redisInvoke';

/**
 * Per-prefix `list_children` level state and its fold functions (PRD §4 I-4 /
 * task book D-5).
 *
 * Kept as pure data functions — no React, no invoke — because I-4 is a
 * *sequence* property ("the loaded subset and each folder's cursor survive a
 * refresh / a collapse"), which is only testable if the transitions can be
 * replayed without mounting a tree.
 *
 * The three fetch modes are the whole design:
 * - `reset`    — this is a *different* tree (db switch, filter change, separator
 *                change): drop everything and start at cursor 0.
 * - `continue` — page the scan forward from the level's stored cursor.
 * - `rescan`   — ⌘R / post-write refresh. I-4 forbids throwing the loaded subset
 *                away, so a finished level starts an authoritative pass
 *                (`pass`) from cursor 0 **while its current children stay
 *                visible**, and the pass replaces the level only when it wraps.
 *                An unfinished level is not restarted at all: the pass simply
 *                continues from its cursor, which is what "cursors survive a
 *                refresh" means.
 */
export type LevelFetchMode = 'reset' | 'continue' | 'rescan';

export interface TreeLevel {
  children: ChildEntry[];
  /** Last SCAN cursor the server handed back (`0` ⇒ this level is complete). */
  cursor: number;
  done: boolean;
  loading: boolean;
  /** Last fetch threw — D-8 renders this as the named `no-permission` state. */
  error: boolean;
  /**
   * Children collected by an in-flight authoritative pass (`rescan` on a
   * finished level). `null` ⇒ no pass open.
   */
  pass: ChildEntry[] | null;
}

export const EMPTY_LEVEL: TreeLevel = {
  children: [],
  cursor: 0,
  done: false,
  loading: false,
  error: false,
  pass: null,
};

/** Identity used to fold pages together: a folder prefix or a key name. */
export function childIdentity(child: ChildEntry): string {
  return child.kind === 'folder' ? `f:${child.prefix}` : `k:${child.key}`;
}

/**
 * Fold freshly scanned children into the ones already on screen.
 *
 * Later wins for a key's mutable attributes (ttl/type), and a folder only ever
 * grows: `count_matching` under-reports while its own scan is open, so taking the
 * max keeps `(n)` from flickering backwards on a refresh.
 */
export function mergeChildren(existing: ChildEntry[], fresh: ChildEntry[]): ChildEntry[] {
  if (existing.length === 0) return [...fresh];
  const at = new Map<string, number>();
  existing.forEach((child, index) => at.set(childIdentity(child), index));
  const out = [...existing];
  for (const child of fresh) {
    const identity = childIdentity(child);
    const known = at.get(identity);
    if (known === undefined) {
      at.set(identity, out.length);
      out.push(child);
      continue;
    }
    const previous = out[known]!;
    out[known] =
      previous.kind === 'folder' && child.kind === 'folder' && previous.count > child.count
        ? previous
        : child;
  }
  return out;
}

/** Cursor the next request must send for `mode` (the I-4 "cursor survives" bit). */
export function fetchCursorFor(level: TreeLevel, mode: LevelFetchMode): number {
  if (mode === 'reset') return 0;
  if (mode === 'rescan') return level.done ? 0 : level.cursor;
  return level.cursor;
}

/** Mark the level as fetching, keeping whatever is already on screen. */
export function beginFetch(level: TreeLevel, mode: LevelFetchMode): TreeLevel {
  if (mode === 'reset') return { ...EMPTY_LEVEL, loading: true };
  if (mode === 'rescan' && level.done && level.pass === null) {
    // Authoritative pass: `children` stay visible, `pass` accumulates from zero.
    return { ...level, loading: true, cursor: 0, pass: [] };
  }
  return { ...level, loading: true };
}

/** Fold one `list_children` reply into the level. */
export function applyFetch(
  level: TreeLevel,
  mode: LevelFetchMode,
  result: { children: ChildEntry[]; cursor: number },
): TreeLevel {
  const done = result.cursor === 0;
  if (level.pass !== null) {
    const pass = mergeChildren(level.pass, result.children);
    if (done)
      return {
        ...level,
        children: pass,
        pass: null,
        cursor: 0,
        done: true,
        loading: false,
        error: false,
      };
    return { ...level, pass, cursor: result.cursor, done: false, loading: false, error: false };
  }
  const base = mode === 'reset' ? EMPTY_LEVEL : level;
  const children =
    mode === 'reset'
      ? mergeChildren([], result.children)
      : mergeChildren(base.children, result.children);
  return { ...base, children, cursor: result.cursor, done, loading: false, error: false };
}

/**
 * The request failed: keep the loaded subset, remember why nothing new arrived,
 * and **end** the level's scan claim (redis-tree-ui-BUG-002).
 *
 * An abandoned authoritative pass must be dropped (`pass: null`) and the level
 * closed (`done: true`): `anyLevelScanning` reads either an open pass or an open
 * cursor as "still scanning", so a failed refresh used to pin R1's `N+` and I-11's
 * `interrupted` forever — the level declared itself scanning while no request was
 * in flight and nothing could ever finish it. Dropping the pass keeps `children`
 * at the last good subset, which is exactly the fallback I-4 asks for, and the
 * next successful `rescan` opens a fresh pass and clears `error` (see
 * {@link applyFetch}), so a failure self-heals instead of poisoning the tree.
 */
export function markFetchFailed(level: TreeLevel, mode: LevelFetchMode): TreeLevel {
  if (mode === 'reset') return { ...EMPTY_LEVEL, loading: false, done: true, error: true };
  return { ...level, loading: false, done: true, error: true, pass: null };
}

/**
 * True while any loaded level is still being scanned — D-8's `interrupted` empty
 * state and R1's `N+` counter share this so "the list is not the whole db" is
 * decided in exactly one place.
 */
export function anyLevelScanning(levels: Record<string, TreeLevel>): boolean {
  return Object.values(levels).some((level) => !level.done || level.pass !== null);
}

/** Root level failed (ACL / NOAUTH) — the tree cannot say anything about contents. */
export function rootLevelFailed(levels: Record<string, TreeLevel>): boolean {
  return levels['']?.error === true;
}
