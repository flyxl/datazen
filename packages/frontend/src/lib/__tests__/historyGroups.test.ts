import { describe, expect, it } from 'vitest';
import { findGroupForDatabase, groupQueryHistory, HISTORY_UNKNOWN_DB_KEY } from '../historyGroups';
import type { QueryHistoryEntry } from '../../types';

function entry(partial: Partial<QueryHistoryEntry> & { id: string }): QueryHistoryEntry {
  return {
    connectionId: 'cfg-1',
    database: 'app_db',
    sql: 'SELECT 1',
    executedAt: '2026-08-21T10:00:00Z',
    executionTimeMs: 5,
    success: true,
    ...partial,
  };
}

describe('groupQueryHistory', () => {
  it('groups entries by recorded database preserving recency order inside groups', () => {
    const groups = groupQueryHistory(
      [
        entry({ id: '1', database: 'app_db' }),
        entry({ id: '2', database: 'analytics', sql: 'SELECT 2' }),
        entry({ id: '3', database: 'app_db', sql: 'SELECT 3' }),
      ],
      '未记录',
    );
    expect(groups.map((g) => g.label)).toEqual(['app_db', 'analytics']);
    expect(groups[0].entries.map((e) => e.id)).toEqual(['1', '3']);
  });

  it('collects legacy rows without database into the unknown group placed last', () => {
    const groups = groupQueryHistory(
      [
        entry({ id: 'old', database: '' }),
        entry({ id: 'new', database: 'app_db', executedAt: '2026-08-22T09:00:00Z' }),
        entry({ id: 'older', database: '  ' }),
      ],
      '未记录',
    );
    expect(groups).toHaveLength(2);
    expect(groups[0].label).toBe('app_db');
    expect(groups[1].key).toBe(HISTORY_UNKNOWN_DB_KEY);
    expect(groups[1].label).toBe('未记录');
    expect(groups[1].entries.map((e) => e.id)).toEqual(['old', 'older']);
  });

  it('orders groups by most recent entry across databases', () => {
    const groups = groupQueryHistory(
      [
        entry({ id: 'a', database: 'app_db', executedAt: '2026-08-20T09:00:00Z' }),
        entry({ id: 'b', database: 'analytics', executedAt: '2026-08-21T09:00:00Z' }),
      ],
      '未记录',
    );
    expect(groups.map((g) => g.label)).toEqual(['analytics', 'app_db']);
  });

  it('returns empty array for empty input', () => {
    expect(groupQueryHistory([], '未记录')).toEqual([]);
  });
});

describe('findGroupForDatabase', () => {
  const groups = groupQueryHistory(
    [entry({ id: '1', database: 'app_db' }), entry({ id: '2', database: 'analytics' })],
    '未记录',
  );

  it('matches the panel database exactly', () => {
    expect(findGroupForDatabase(groups, 'analytics')?.label).toBe('analytics');
  });

  it('trims whitespace before matching', () => {
    expect(findGroupForDatabase(groups, ' app_db ')?.label).toBe('app_db');
  });

  it('returns null when panel has no database or no matching group', () => {
    expect(findGroupForDatabase(groups, null)).toBeNull();
    expect(findGroupForDatabase(groups, undefined)).toBeNull();
    expect(findGroupForDatabase(groups, 'missing')).toBeNull();
  });
});
