import type { SortCondition } from '../../types';
import type { TableChangeContext } from '../../lib/tableChanges';

/**
 * Per-table sort persistence (DB-11).
 *
 * The sort list a user builds for a table is part of the table's identity in
 * their workflow: reopening the app and reopening the table must give the
 * same order. TablePlus never persisted it (#2913/#2919/#3154/#3812, five
 * separate unresolved requests), and the multi-column sort itself was
 * fragile (#2843 — adding a column lost the previous one). The persistence
 * here is deliberately **local to the window** (`localStorage`): it is a UI
 * preference, not data, and it travels with the machine the user sorted on.
 *
 * The key is the table's stable identity — connection, database, schema,
 * table. A session without a configured connection has no identity that
 * survives a restart, so it is not persisted at all rather than persisted
 * under a key that can never be found again.
 */
const STORAGE_PREFIX = 'datazen:table-sort:v1';

/** The storage key of a table's persisted sorts, or `null` when the panel's table has no stable identity. */
export function tableSortStorageKey(context: TableChangeContext): string | null {
  if (!context.connectionId) return null;
  const identity = [
    context.connectionId,
    context.database ?? '',
    context.schema ?? '',
    context.table,
  ];
  return `${STORAGE_PREFIX}:${JSON.stringify(identity)}`;
}

/**
 * A stored list is untrusted: it may predate a column being dropped, or have
 * been written by anything at all. Only a well-shaped list of sort
 * conditions is accepted — anything else reads as "no stored sorts".
 */
function isSortCondition(value: unknown): value is SortCondition {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as { column?: unknown; descending?: unknown };
  return typeof candidate.column === 'string' && typeof candidate.descending === 'boolean';
}

/** The sorts a table was left with, or `[]` when nothing usable is stored. */
export function readPersistedTableSorts(context: TableChangeContext): SortCondition[] {
  const key = tableSortStorageKey(context);
  if (!key || typeof window === 'undefined' || !window.localStorage) return [];
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Shape is checked here; whether a column still exists is the host's
    // answer when the page is read — a stale sort column surfaces as a
    // visible query error the user can clear from the header menu.
    const sorts = parsed.filter(isSortCondition);
    return sorts.length > 0 ? sorts : [];
  } catch {
    return [];
  }
}

/** Store (or clear) a table's sorts. Failures are swallowed: persistence is a convenience, never a gate. */
export function persistTableSorts(context: TableChangeContext, sorts: SortCondition[]): void {
  const key = tableSortStorageKey(context);
  if (!key || typeof window === 'undefined' || !window.localStorage) return;
  try {
    if (sorts.length === 0) {
      window.localStorage.removeItem(key);
    } else {
      window.localStorage.setItem(key, JSON.stringify(sorts));
    }
  } catch {
    // Quota exceeded or storage disabled: the sort still works for this
    // session, it just will not survive a restart.
  }
}
