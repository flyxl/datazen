import { DEFAULT_SEPARATOR, SEPARATOR_CHOICES } from './keyTree';
import type { StorageLike } from '../lib/redisBrowseHistory';

/**
 * Per-connection key-tree preferences (PRD §3.2 R3 / task book D-3).
 *
 * Two settings only: whether the left column folds namespaces into a tree or
 * lists keys flat, and which single character it folds on. Both are persisted
 * **per connection**, because a cache-buster instance (`a.b.c` keys) and a
 * business instance (`user:1:2` keys) legitimately want different separators on
 * the same screen.
 *
 * Shape on disk (one bucket per persisted key, so two connections never share a
 * preference):
 *
 *   { "datazen:redis:treePrefs:v1": { "<bucket>": { "view": "tree", "sep": ":" } } }
 *
 * Same defensive rules as `redisBrowseHistory`: a corrupt payload, a blocked Web
 * Storage or an old shape degrades to the defaults instead of throwing on the
 * render path.
 */
export const TREE_PREFS_STORAGE_KEY = 'datazen:redis:treePrefs:v1';

export type TreeViewMode = 'tree' | 'list';

export interface TreePrefs {
  view: TreeViewMode;
  /** Exactly one character out of {@link SEPARATOR_CHOICES}. */
  separator: string;
}

export const DEFAULT_TREE_PREFS: TreePrefs = { view: 'tree', separator: DEFAULT_SEPARATOR };

/**
 * Preference bucket: the stable connection id when the host passes one, else the
 * live session id (a reconnect then legitimately starts from the defaults).
 */
export function treePrefsBucket(connectionId: string | undefined, dbSessionId: string): string {
  const stable = connectionId?.trim();
  return stable && stable.length > 0 ? stable : dbSessionId;
}

export function isTreeViewMode(value: unknown): value is TreeViewMode {
  return value === 'tree' || value === 'list';
}

/** Only the three documented separators are accepted; anything else ⇒ default. */
export function asSeparatorChoice(value: unknown): string {
  return typeof value === 'string' && (SEPARATOR_CHOICES as readonly string[]).includes(value)
    ? value
    : DEFAULT_SEPARATOR;
}

function globalStorage(): StorageLike | null {
  try {
    return (globalThis as { localStorage?: StorageLike }).localStorage ?? null;
  } catch {
    return null;
  }
}

function readAllBuckets(storage: StorageLike | null): Record<string, unknown> {
  if (!storage) return {};
  try {
    const raw = storage.getItem(TREE_PREFS_STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    return parsed as Record<string, unknown>;
  } catch {
    return {};
  }
}

export function readTreePrefs(bucket: string, storage?: StorageLike | null): TreePrefs {
  if (!bucket) return { ...DEFAULT_TREE_PREFS };
  const stored = readAllBuckets(storage ?? globalStorage())[bucket];
  if (!stored || typeof stored !== 'object') return { ...DEFAULT_TREE_PREFS };
  const value = stored as { view?: unknown; sep?: unknown };
  return {
    view: isTreeViewMode(value.view) ? value.view : DEFAULT_TREE_PREFS.view,
    separator: asSeparatorChoice(value.sep),
  };
}

/** Persist one connection's prefs and return what was written. */
export function writeTreePrefs(
  bucket: string,
  prefs: TreePrefs,
  storage?: StorageLike | null,
): TreePrefs {
  const target = storage ?? globalStorage();
  const next: TreePrefs = {
    view: isTreeViewMode(prefs.view) ? prefs.view : DEFAULT_TREE_PREFS.view,
    separator: asSeparatorChoice(prefs.separator),
  };
  if (!bucket || !target) return next;
  try {
    const buckets = readAllBuckets(target);
    buckets[bucket] = { view: next.view, sep: next.separator };
    target.setItem(TREE_PREFS_STORAGE_KEY, JSON.stringify(buckets));
  } catch {
    // Quota exceeded / storage disabled: the preference just stops being durable.
  }
  return next;
}
