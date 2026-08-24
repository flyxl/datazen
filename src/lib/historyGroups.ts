import type { QueryHistoryEntry } from '../types';

/** Sentinel group key for entries whose database was not recorded (legacy rows). */
export const HISTORY_UNKNOWN_DB_KEY = '__unknown_db__';

export interface QueryHistoryGroup {
  /** Database name, or the sentinel key for unrecorded rows. */
  key: string;
  /** Display label (database name or localized "unrecorded" text supplied by caller). */
  label: string;
  entries: QueryHistoryEntry[];
}

export type HistoryScope = { kind: 'database'; database: string | null } | { kind: 'all' };

/**
 * Group history entries by their recorded session database, newest first.
 *
 * The query panel binds its sidebar to the panel's own database by default so
 * an applied entry re-runs in the same context instead of failing with
 * "table not exist". Entries without a recorded database (legacy rows) land in
 * a dedicated trailing group.
 */
export function groupQueryHistory(
  entries: QueryHistoryEntry[],
  unknownLabel: string,
): QueryHistoryGroup[] {
  const byKey = new Map<string, QueryHistoryEntry[]>();
  for (const entry of entries) {
    const db = entry.database?.trim();
    const key = db ? db : HISTORY_UNKNOWN_DB_KEY;
    const bucket = byKey.get(key);
    if (bucket) {
      bucket.push(entry);
    } else {
      byKey.set(key, [entry]);
    }
  }
  const groups: QueryHistoryGroup[] = [];
  for (const [key, groupEntries] of byKey) {
    groups.push({
      key,
      label: key === HISTORY_UNKNOWN_DB_KEY ? unknownLabel : key,
      // Entries arrive newest-first from the backend; keep that order within groups.
      entries: groupEntries,
    });
  }
  // Order groups by their most recent entry to mirror overall recency.
  // Compare RFC3339 timestamps numerically: lexicographic order breaks when
  // precision is mixed (`…T00:00:00Z` vs `…T00:00:00.5Z`, '.' < 'Z').
  groups.sort((a, b) => {
    const at = a.entries[0]?.executedAt ?? '';
    const bt = b.entries[0]?.executedAt ?? '';
    const an = Date.parse(at);
    const bn = Date.parse(bt);
    if (Number.isNaN(an) || Number.isNaN(bn)) return bt.localeCompare(at);
    return bn - an;
  });
  return groups;
}

/**
 * Resolve which group matches the panel's current database. Falls back to null
 * when nothing matches (caller then shows all groups).
 */
export function findGroupForDatabase(
  groups: QueryHistoryGroup[],
  currentDatabase: string | null | undefined,
): QueryHistoryGroup | null {
  const db = currentDatabase?.trim();
  if (!db) return null;
  return groups.find((g) => g.key === db) ?? null;
}
