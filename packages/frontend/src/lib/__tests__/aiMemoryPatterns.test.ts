import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { QueryHistoryEntry, QueryHistoryPage } from '../../types';

const mocks = vi.hoisted(() => ({
  history: vi.fn(),
  extract: vi.fn(),
}));

vi.mock('../../commands/ai', () => ({
  aiCommands: { memoryExtractCandidates: mocks.extract },
}));
vi.mock('../../commands/query', () => ({
  queryCommands: { getQueryHistoryPage: mocks.history },
}));

import {
  MEMORY_PATTERN_MIN_OCCURRENCES,
  MEMORY_PATTERN_WINDOW_DAYS,
  extractMemoryPatternsIfNeeded,
} from '../aiMemoryPatterns';

const NOW = Date.parse('2026-04-20T12:00:00.000Z');

function entry(id: string, sql: string, success = true): QueryHistoryEntry {
  return {
    id,
    connectionId: 'conn-1',
    database: 'sales',
    sql,
    executedAt: '2026-04-19T12:00:00.000Z',
    executionTimeMs: 5,
    success,
  };
}

function page(entries: QueryHistoryEntry[]): QueryHistoryPage {
  return { entries, total: entries.length };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.extract.mockResolvedValue([]);
  mocks.history.mockResolvedValue(
    page([
      entry('h1', 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id'),
      entry('h2', 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id'),
      entry('h3', 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id'),
      entry('failed', 'SELECT broken', false),
    ]),
  );
});

describe('automatic memory pattern extraction', () => {
  it('does nothing when the extraction switch is off', async () => {
    await extractMemoryPatternsIfNeeded({
      sql: 'SELECT 1',
      connectionId: 'conn-1',
      database: 'sales',
      autoLearn: false,
      now: NOW,
    });
    expect(mocks.history).not.toHaveBeenCalled();
    expect(mocks.extract).not.toHaveBeenCalled();
  });

  it('feeds recent successful history and the current SQL to the extractor', async () => {
    await extractMemoryPatternsIfNeeded({
      sql: 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id',
      connectionId: 'conn-1',
      database: 'sales',
      autoLearn: true,
      now: NOW,
    });

    expect(mocks.history).toHaveBeenCalledWith({
      limit: 100,
      connectionId: 'conn-1',
      database: 'sales',
      since: new Date(NOW - MEMORY_PATTERN_WINDOW_DAYS * 24 * 60 * 60 * 1000).toISOString(),
      order: 'recent',
    });
    expect(mocks.extract).toHaveBeenCalledWith({
      sql: 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id',
      connectionId: 'conn-1',
      database: 'sales',
      history: [
        {
          sql: 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id',
          observedAt: '2026-04-19T12:00:00.000Z',
        },
        {
          sql: 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id',
          observedAt: '2026-04-19T12:00:00.000Z',
        },
        {
          sql: 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id',
          observedAt: '2026-04-19T12:00:00.000Z',
        },
      ],
      windowDays: MEMORY_PATTERN_WINDOW_DAYS,
      minOccurrences: MEMORY_PATTERN_MIN_OCCURRENCES,
    });
    // Current SQL + three historical samples is the ≥3 occurrence proof.
    expect(
      (mocks.extract.mock.calls[0][0] as { history: unknown[] }).history.length + 1,
    ).toBeGreaterThanOrEqual(MEMORY_PATTERN_MIN_OCCURRENCES);
  });

  it('swallows history and extraction failures', async () => {
    mocks.history.mockRejectedValueOnce(new Error('history unavailable'));
    await expect(
      extractMemoryPatternsIfNeeded({
        sql: 'SELECT 1',
        connectionId: 'conn-1',
        database: 'sales',
        autoLearn: true,
        now: NOW,
      }),
    ).resolves.toBeUndefined();

    mocks.extract.mockRejectedValueOnce(new Error('extract unavailable'));
    await expect(
      extractMemoryPatternsIfNeeded({
        sql: 'SELECT 2',
        connectionId: 'conn-1',
        database: 'sales',
        autoLearn: true,
        now: NOW,
      }),
    ).resolves.toBeUndefined();
  });

  it('coalesces a burst and reruns once with the newest SQL', async () => {
    let release: ((value: QueryHistoryPage) => void) | undefined;
    mocks.history.mockImplementationOnce(
      () =>
        new Promise<QueryHistoryPage>((resolve) => {
          release = resolve;
        }),
    );
    const first = extractMemoryPatternsIfNeeded({
      sql: 'SELECT first',
      connectionId: 'conn-1',
      database: 'sales',
      autoLearn: true,
      now: NOW,
    });
    const second = extractMemoryPatternsIfNeeded({
      sql: 'SELECT second',
      connectionId: 'conn-1',
      database: 'sales',
      autoLearn: true,
      now: NOW,
    });
    expect(mocks.history).toHaveBeenCalledTimes(1);
    release?.(page([]));
    await Promise.all([first, second]);
    expect(mocks.history).toHaveBeenCalledTimes(2);
    expect(mocks.extract.mock.calls[1][0]).toMatchObject({ sql: 'SELECT second' });
  });
});
