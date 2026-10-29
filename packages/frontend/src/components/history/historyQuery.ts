/**
 * Pure helpers behind the global query-history dialog.
 *
 * Kept free of React and of `invoke` so the decision the UI actually gets wrong —
 * "is what I'm showing the whole truth, or a slice of it?" — is a plain function
 * a test can pin without a DOM.
 */

import type { HistorySort, QueryHistoryEntry, QueryHistoryPage } from '../../types';

export type { HistorySort, QueryHistoryPage };

/** How far back the list shows. `null` = no bound. */
export type HistoryRange = 'all' | 'today' | '7d' | '30d' | '90d';

export interface HistoryQueryState {
  search: string;
  connectionId: string;
  database: string;
  schema: string;
  status: 'all' | 'success' | 'failed';
  range: HistoryRange;
  sort: HistorySort;
}

export const DEFAULT_HISTORY_QUERY: HistoryQueryState = {
  search: '',
  connectionId: 'all',
  database: '',
  schema: '',
  status: 'all',
  range: 'all',
  sort: 'recent',
};

/** `'all'` is a sentinel the backend must not receive. */
const ALL = 'all';

const RANGE_DAYS: Record<Exclude<HistoryRange, 'all'>, number> = {
  today: 1,
  '7d': 7,
  '30d': 30,
  '90d': 90,
};

/**
 * Lower bound for a range, as RFC3339, or `undefined` for unbounded.
 *
 * `today` means the last 24h rather than the local calendar day: the column is
 * stamped in UTC and the user compares against a list that is not sorted by
 * their clock, so a calendar-midnight cut would silently drop this morning's
 * queries for anyone east or west of UTC.
 */
export function rangeSinceIso(range: HistoryRange, now: Date): string | undefined {
  if (range === ALL) return undefined;
  const days = RANGE_DAYS[range];
  return new Date(now.getTime() - days * 86_400_000).toISOString();
}

/** Arguments for `get_query_history_page`, with sentinels dropped. */
export function buildHistoryArgs(
  state: HistoryQueryState,
  limit: number,
  now: Date = new Date(),
): Record<string, unknown> {
  const search = state.search.trim();
  const since = rangeSinceIso(state.range, now);
  return {
    limit,
    connectionId: state.connectionId === ALL ? null : state.connectionId,
    database: state.database.trim() ? state.database.trim() : null,
    schema: state.schema.trim() ? state.schema.trim() : null,
    search: search ? search : null,
    since: since ?? null,
    order: state.sort,
  };
}

/**
 * Whether the server applied a filter that this component cannot re-apply.
 *
 * The status filter is the one predicate the backend has no column for, so it
 * stays client-side — and it can shrink the page below the backend's own
 * `limit`, which is a different thing from the server having truncated.
 */
export function applyStatusFilter(
  entries: QueryHistoryEntry[],
  status: HistoryQueryState['status'],
): QueryHistoryEntry[] {
  if (status === 'success') return entries.filter((e) => e.success);
  if (status === 'failed') return entries.filter((e) => !e.success);
  return entries;
}

/**
 * True when the backend had more matches than it returned.
 *
 * The dialog must not present a truncated page as a complete list: a user who
 * searched for a query that fell outside the window would otherwise see "no
 * matches" and conclude the query was never run.
 */
export function isTruncated(page: QueryHistoryPage): boolean {
  return page.total > page.entries.length;
}

/** The honest count line, or `null` when nothing is being hidden. */
export function truncationNotice(page: QueryHistoryPage): { shown: number; total: number } | null {
  if (!isTruncated(page)) return null;
  return { shown: page.entries.length, total: page.total };
}

/**
 * Render selected history rows as one `.sql` file.
 *
 * Rows are emitted newest-first with a header each, so re-importing the file
 * preserves the order a human reads them in rather than reversing it.
 */
export function buildSqlExport(entries: QueryHistoryEntry[], connectionName?: string): string {
  if (entries.length === 0) return '';
  const parts: string[] = [];
  if (connectionName) parts.push(`-- ${connectionName}`);
  for (const e of entries) {
    const status = e.success ? 'ok' : 'FAILED';
    const rows = e.rowsAffected == null ? '' : `, ${e.rowsAffected} rows`;
    parts.push(
      `-- ${e.executedAt} · ${e.database || 'default'} · ${e.executionTimeMs}ms · ${status}${rows}`,
    );
    parts.push(e.sql.trimEnd().replace(/;?\s*$/, ';'));
    parts.push('');
  }
  return parts.join('\n');
}

/** Default file name for the export dialog. */
export function exportFileName(count: number): string {
  return count === 1 ? 'query.sql' : `queries-${count}.sql`;
}
