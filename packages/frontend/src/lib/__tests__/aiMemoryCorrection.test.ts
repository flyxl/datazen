import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  memoryCorrectionCandidate: vi.fn(),
  memoryMarkStale: vi.fn(),
}));

vi.mock('../../commands/ai', () => ({
  aiCommands: {
    memoryCorrectionCandidate: mocks.memoryCorrectionCandidate,
    memoryMarkStale: mocks.memoryMarkStale,
  },
}));

import { markStaleForSchemaDrops, submitMemoryCorrectionIfNeeded } from '../aiMemoryCorrection';

describe('AI memory correction submission', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.memoryCorrectionCandidate.mockResolvedValue({
      id: 'm1',
      status: 'candidate',
    });
    mocks.memoryMarkStale.mockResolvedValue([]);
  });

  it('submits the original and modified SQL after a successful modified AI query', async () => {
    await submitMemoryCorrectionIfNeeded({
      originalSql: 'SELECT * FROM orders',
      currentSql: 'SELECT * FROM orders WHERE status = ?',
      connectionId: 'conn-1',
      database: 'sales',
    });
    expect(mocks.memoryCorrectionCandidate).toHaveBeenCalledWith({
      originalSql: 'SELECT * FROM orders',
      newSql: 'SELECT * FROM orders WHERE status = ?',
      connectionId: 'conn-1',
      database: 'sales',
    });
  });

  it('does not submit an unchanged AI query', async () => {
    expect(
      await submitMemoryCorrectionIfNeeded({
        originalSql: 'SELECT 1',
        currentSql: '  SELECT 1  ',
      }),
    ).toBeNull();
    expect(mocks.memoryCorrectionCandidate).not.toHaveBeenCalled();
  });

  it('does not break the query when candidate submission fails', async () => {
    mocks.memoryCorrectionCandidate.mockRejectedValueOnce(new Error('host offline'));
    await expect(
      submitMemoryCorrectionIfNeeded({
        originalSql: 'SELECT 1',
        currentSql: 'SELECT 2',
      }),
    ).resolves.toBeNull();
  });

  it('marks each dropped schema object stale', async () => {
    await markStaleForSchemaDrops({
      sql: 'DROP TABLE sales.orders;',
      connectionId: 'conn-1',
      database: 'sales',
    });
    expect(mocks.memoryMarkStale).toHaveBeenCalledWith({
      connectionId: 'conn-1',
      database: 'sales',
      schema: 'sales',
      table: 'orders',
      column: undefined,
    });
  });
});
