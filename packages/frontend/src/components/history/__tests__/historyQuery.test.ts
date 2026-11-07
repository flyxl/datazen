import { describe, expect, it } from 'vitest';
import {
  DEFAULT_HISTORY_QUERY,
  applyStatusFilter,
  buildHistoryArgs,
  buildSqlExport,
  exportFileName,
  isTruncated,
  rangeSinceIso,
  truncationNotice,
  type HistoryQueryState,
} from '../historyQuery';
import type { QueryHistoryEntry } from '../../../types';

function entry(over: Partial<QueryHistoryEntry> = {}): QueryHistoryEntry {
  return {
    id: 'h1',
    connectionId: 'c1',
    database: 'app',
    schema: null,
    sql: 'SELECT 1',
    executedAt: '2026-01-01T00:00:00Z',
    executionTimeMs: 12,
    rowsAffected: 3,
    success: true,
    ...over,
  };
}

const NOW = new Date('2026-06-15T12:00:00Z');

describe('buildHistoryArgs', () => {
  it('drops the "all" sentinel and empty strings so the backend applies no filter', () => {
    const args = buildHistoryArgs(DEFAULT_HISTORY_QUERY, 200, NOW);
    expect(args).toMatchObject({
      limit: 200,
      connectionId: null,
      database: null,
      schema: null,
      search: null,
      since: null,
      order: 'recent',
    });
  });

  it('passes a chosen connection, database and schema through', () => {
    const state: HistoryQueryState = {
      ...DEFAULT_HISTORY_QUERY,
      connectionId: 'c9',
      database: 'sales',
      schema: 'public',
    };
    expect(buildHistoryArgs(state, 50, NOW)).toMatchObject({
      connectionId: 'c9',
      database: 'sales',
      schema: 'public',
      limit: 50,
    });
  });

  it('trims the search term and sends null for whitespace only', () => {
    expect(buildHistoryArgs({ ...DEFAULT_HISTORY_QUERY, search: '  ' }, 10, NOW).search).toBeNull();
    expect(
      buildHistoryArgs({ ...DEFAULT_HISTORY_QUERY, search: '  users  ' }, 10, NOW).search,
    ).toBe('users');
  });

  it('forwards the sort order', () => {
    for (const sort of ['recent', 'oldest', 'slowest'] as const) {
      expect(buildHistoryArgs({ ...DEFAULT_HISTORY_QUERY, sort }, 10, NOW).order).toBe(sort);
    }
  });
});

describe('rangeSinceIso', () => {
  it('returns undefined for all-time so the backend applies no bound', () => {
    expect(rangeSinceIso('all', NOW)).toBeUndefined();
  });

  it('steps back the right number of days', () => {
    expect(rangeSinceIso('today', NOW)).toBe('2026-06-14T12:00:00.000Z');
    expect(rangeSinceIso('7d', NOW)).toBe('2026-06-08T12:00:00.000Z');
    expect(rangeSinceIso('30d', NOW)).toBe('2026-05-16T12:00:00.000Z');
    expect(rangeSinceIso('90d', NOW)).toBe('2026-03-17T12:00:00.000Z');
  });
});

describe('status filter', () => {
  const rows = [
    entry({ id: 'a', success: true }),
    entry({ id: 'b', success: false }),
    entry({ id: 'c', success: true }),
  ];

  it('passes everything through for "all"', () => {
    expect(applyStatusFilter(rows, 'all')).toHaveLength(3);
  });

  it('keeps only successes or only failures', () => {
    expect(applyStatusFilter(rows, 'success').map((r) => r.id)).toEqual(['a', 'c']);
    expect(applyStatusFilter(rows, 'failed').map((r) => r.id)).toEqual(['b']);
  });
});

describe('truncation reporting', () => {
  it('is not truncated when everything that matched came back', () => {
    const page = { entries: [entry()], total: 1 };
    expect(isTruncated(page)).toBe(false);
    expect(truncationNotice(page)).toBeNull();
  });

  it('is truncated — and must say so — when the backend had more than it sent', () => {
    const page = { entries: [entry()], total: 57 };
    expect(isTruncated(page)).toBe(true);
    expect(truncationNotice(page)).toEqual({ shown: 1, total: 57 });
  });

  it('is not truncated by an empty page over an empty table', () => {
    const page = { entries: [], total: 0 };
    expect(isTruncated(page)).toBe(false);
    expect(truncationNotice(page)).toBeNull();
  });
});

describe('buildSqlExport', () => {
  it('returns an empty string for no rows', () => {
    expect(buildSqlExport([])).toBe('');
  });

  it('keeps the query order, terminates each statement, and annotates provenance', () => {
    const sql = buildSqlExport(
      [
        entry({ sql: 'SELECT 1', executedAt: '2026-01-02T00:00:00Z', database: 'app' }),
        entry({ sql: 'SELECT 2', executedAt: '2026-01-01T00:00:00Z', database: 'app' }),
      ],
      'local-pg',
    );
    const lines = sql.split('\n');
    expect(lines[0]).toBe('-- local-pg');
    // First emitted query is the first selected one — no silent reordering.
    expect(sql.indexOf('SELECT 1')).toBeLessThan(sql.indexOf('SELECT 2'));
    expect(sql).toContain('SELECT 1;');
    expect(sql).toContain('2026-01-02');
  });

  it('adds a trailing semicolon only when one is missing', () => {
    expect(buildSqlExport([entry({ sql: 'SELECT 1;' })])).toContain('SELECT 1;\n');
    expect(buildSqlExport([entry({ sql: 'SELECT 1' })])).toContain('SELECT 1;\n');
    expect(buildSqlExport([entry({ sql: 'SELECT 1;   ' })])).not.toContain('SELECT 1;;');
  });

  it('marks a failed statement instead of dropping the provenance line', () => {
    const sql = buildSqlExport([entry({ success: false, sql: 'SELECT bad' })]);
    expect(sql).toContain('FAILED');
  });

  it('falls back to a named placeholder when there is no database', () => {
    expect(buildSqlExport([entry({ database: '' })])).toContain('default');
  });
});

describe('exportFileName', () => {
  it('is singular for one row and counted for many', () => {
    expect(exportFileName(1)).toBe('query.sql');
    expect(exportFileName(4)).toBe('queries-4.sql');
  });
});
