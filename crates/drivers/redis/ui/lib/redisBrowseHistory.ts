/**
 * Connection-scoped "recently browsed keys" history (PRD §3.1 屏 A 最后一区块).
 *
 * Pure localStorage module — no React, no host import, no Redis round trip. The
 * overview reads it; a successful jump into 屏 B writes it (PRD: 点击直达, so an
 * entry only ever represents a key the user actually reached).
 *
 * Shape on disk (one slot per persisted `connectionId`, so two Redis connections
 * never share a history):
 *
 *   { "datazen:redis:browseHistory:v1": { "<connectionId>": [entry, …] } }
 *
 * Every read is defensive: a corrupt payload, a disabled/blocked Web Storage or
 * an old shape degrades to an empty history instead of throwing on the render
 * path (production code must not panic — see docs/development/panic-policy.md).
 */

export const BROWSE_HISTORY_STORAGE_KEY = 'datazen:redis:browseHistory:v1';

/** PRD 只要"最近浏览"，10 条足够一屏且不会把 localStorage 撑大。 */
export const BROWSE_HISTORY_MAX_ENTRIES = 10;

export interface BrowseHistoryEntry {
  key: string;
  dbIndex: number;
  /** Redis TYPE 令牌（`string` / `hash` / …），`null` ⇒ 当时未取到。 */
  keyType: string | null;
  /** `Date.now()` epoch ms. */
  visitedAt: number;
}

/** Minimal Web Storage surface (tests inject a fake one). */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface NewBrowseEntry {
  key: string;
  dbIndex: number;
  keyType?: string | null;
}

function globalStorage(): StorageLike | null {
  try {
    const candidate = (globalThis as { localStorage?: StorageLike }).localStorage;
    return candidate ?? null;
  } catch {
    // Some webviews throw on *accessing* localStorage in a sandboxed frame.
    return null;
  }
}

function resolveStorage(storage: StorageLike | null | undefined): StorageLike | null {
  return storage ?? globalStorage();
}

/** Dedup identity: the same key name in two logical databases is two entries. */
function entryIdentity(entry: BrowseHistoryEntry): string {
  return `${entry.dbIndex}\u0000${entry.key}`;
}

function asEntry(raw: unknown): BrowseHistoryEntry | null {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as {
    key?: unknown;
    dbIndex?: unknown;
    keyType?: unknown;
    visitedAt?: unknown;
  };
  if (typeof value.key !== 'string' || value.key.length === 0) return null;
  const dbIndex = Number(value.dbIndex);
  if (!Number.isFinite(dbIndex) || dbIndex < 0) return null;
  const visitedAt = Number(value.visitedAt);
  if (!Number.isFinite(visitedAt)) return null;
  return {
    key: value.key,
    dbIndex: Math.floor(dbIndex),
    keyType: typeof value.keyType === 'string' && value.keyType.length > 0 ? value.keyType : null,
    visitedAt: Math.floor(visitedAt),
  };
}

function readAllBuckets(storage: StorageLike | null): Record<string, BrowseHistoryEntry[]> {
  if (!storage) return {};
  let parsed: unknown;
  try {
    const raw = storage.getItem(BROWSE_HISTORY_STORAGE_KEY);
    if (!raw) return {};
    parsed = JSON.parse(raw);
  } catch {
    return {};
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};

  const buckets: Record<string, BrowseHistoryEntry[]> = {};
  for (const [connectionId, entries] of Object.entries(parsed as Record<string, unknown>)) {
    if (!Array.isArray(entries)) continue;
    const clean = entries
      .map(asEntry)
      .filter((entry): entry is BrowseHistoryEntry => entry !== null)
      // Newest first, then hard-capped — a hand-edited blob can't grow unbounded.
      .sort((a, b) => b.visitedAt - a.visitedAt)
      .slice(0, BROWSE_HISTORY_MAX_ENTRIES);
    if (clean.length > 0) buckets[connectionId] = clean;
  }
  return buckets;
}

function writeAllBuckets(
  storage: StorageLike | null,
  buckets: Record<string, BrowseHistoryEntry[]>,
): void {
  if (!storage) return;
  try {
    const compacted = Object.fromEntries(
      Object.entries(buckets).filter(([, entries]) => entries.length > 0),
    );
    if (Object.keys(compacted).length === 0) {
      storage.removeItem(BROWSE_HISTORY_STORAGE_KEY);
      return;
    }
    storage.setItem(BROWSE_HISTORY_STORAGE_KEY, JSON.stringify(compacted));
  } catch {
    // Quota exceeded / storage disabled: the block just renders its empty state.
  }
}

/** Recently visited keys for one connection, newest first. */
export function readBrowseHistory(
  connectionId: string,
  storage?: StorageLike | null,
): BrowseHistoryEntry[] {
  if (!connectionId) return [];
  return readAllBuckets(resolveStorage(storage))[connectionId] ?? [];
}

/**
 * Record a visit: the entry moves to the front (dedup by db + key name) and the
 * tail beyond {@link BROWSE_HISTORY_MAX_ENTRIES} is dropped. Returns the new list.
 */
export function pushBrowseEntry(
  connectionId: string,
  entry: NewBrowseEntry,
  storage?: StorageLike | null,
  now: number = Date.now(),
): BrowseHistoryEntry[] {
  const target = resolveStorage(storage);
  if (!connectionId || !entry.key) return readBrowseHistory(connectionId, target);

  const fresh: BrowseHistoryEntry = {
    key: entry.key,
    dbIndex: Math.max(0, Math.floor(Number(entry.dbIndex) || 0)),
    keyType: entry.keyType ?? null,
    visitedAt: Math.floor(Number.isFinite(now) ? now : Date.now()),
  };

  const buckets = readAllBuckets(target);
  const previous = buckets[connectionId] ?? [];
  const identity = entryIdentity(fresh);
  const rest = previous.filter((item) => entryIdentity(item) !== identity);
  const next = [fresh, ...rest].slice(0, BROWSE_HISTORY_MAX_ENTRIES);
  buckets[connectionId] = next;
  writeAllBuckets(target, buckets);
  return next;
}

/** Drop one connection's history (leaves other connections untouched). */
export function clearBrowseHistory(connectionId: string, storage?: StorageLike | null): void {
  const target = resolveStorage(storage);
  if (!connectionId || !target) return;
  const buckets = readAllBuckets(target);
  if (!(connectionId in buckets)) return;
  delete buckets[connectionId];
  writeAllBuckets(target, buckets);
}

export interface RelativeTimeParts {
  value: number;
  /** `redis.overview.timeAgo.<unit>` — the component renders `t(unitKey, { value })`. */
  unitKey: string;
}

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * Locale-neutral relative time: the model returns a magnitude + unit **key**, so
 * no English ("5 min ago") is ever baked into the module or its tests.
 */
export function relativeTimeParts(visitedAt: number, now: number = Date.now()): RelativeTimeParts {
  const delta = Math.max(0, Math.floor(now) - Math.floor(visitedAt));
  if (delta < MINUTE)
    return {
      value: Math.max(1, Math.round(delta / SECOND)),
      unitKey: 'redis.overview.timeAgo.seconds',
    };
  if (delta < HOUR)
    return { value: Math.round(delta / MINUTE), unitKey: 'redis.overview.timeAgo.minutes' };
  if (delta < DAY)
    return { value: Math.round(delta / HOUR), unitKey: 'redis.overview.timeAgo.hours' };
  return { value: Math.round(delta / DAY), unitKey: 'redis.overview.timeAgo.days' };
}
