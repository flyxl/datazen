import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableSchema } from '../../types';

const mockGetTableSchema = vi.fn();
const mockExecuteQuery = vi.fn();

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getTableSchema: (...args: unknown[]) => mockGetTableSchema(...args),
  },
}));

vi.mock('../../commands/query', () => ({
  queryCommands: {
    executeQuery: (...args: unknown[]) => mockExecuteQuery(...args),
  },
}));

import {
  getCachedTableSchema,
  getCachedDDL,
  invalidateSchemaCache,
} from '../schemaCache';

const schema: TableSchema = {
  name: 'users',
  columns: [],
  indexes: [],
  foreignKeys: [],
};

describe('schemaCache', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invalidateSchemaCache('conn-1');
    invalidateSchemaCache('conn-2');
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('fetches and caches table schema', async () => {
    mockGetTableSchema.mockResolvedValue(schema);
    const r1 = await getCachedTableSchema('conn-1', 'users');
    const r2 = await getCachedTableSchema('conn-1', 'users');
    expect(r1).toBe(schema);
    expect(r2).toBe(schema);
    expect(mockGetTableSchema).toHaveBeenCalledTimes(1);
  });

  it('refetches after TTL expires', async () => {
    vi.useFakeTimers();
    mockGetTableSchema.mockResolvedValue(schema);
    await getCachedTableSchema('conn-1', 'users');
    vi.advanceTimersByTime(61_000);
    await getCachedTableSchema('conn-1', 'users');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
  });

  it('fetches and caches DDL via query + extractor', async () => {
    mockExecuteQuery.mockResolvedValue({
      results: [{ rows: [['CREATE TABLE users (...);']] }],
    });
    const extractor = vi.fn((rows: unknown[][]) => String(rows[0]?.[0] ?? ''));
    const ddl = await getCachedDDL('conn-1', 'users', 'SHOW CREATE TABLE users', extractor);
    expect(ddl).toBe('CREATE TABLE users (...);');
    expect(mockExecuteQuery).toHaveBeenCalledWith('conn-1', 'SHOW CREATE TABLE users');
    expect(extractor).toHaveBeenCalledWith([['CREATE TABLE users (...);']]);

    mockExecuteQuery.mockClear();
    await getCachedDDL('conn-1', 'users', 'SHOW CREATE TABLE users', extractor);
    expect(mockExecuteQuery).not.toHaveBeenCalled();
  });

  it('handles empty query result in DDL extractor', async () => {
    mockExecuteQuery.mockResolvedValue({ results: [{ rows: [] }] });
    const extractor = vi.fn(() => '(empty)');
    const ddl = await getCachedDDL('conn-1', 'users', 'SELECT 1', extractor);
    expect(ddl).toBe('(empty)');
    expect(extractor).toHaveBeenCalledWith([]);
  });

  it('invalidates single table or whole connection', async () => {
    mockGetTableSchema.mockResolvedValue(schema);
    await getCachedTableSchema('conn-1', 'users');
    await getCachedTableSchema('conn-1', 'orders');
    await getCachedTableSchema('conn-2', 'users');

    invalidateSchemaCache('conn-1', 'users');
    await getCachedTableSchema('conn-1', 'users');
    await getCachedTableSchema('conn-1', 'orders');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(4); // 3 initial + 1 refetch users

    mockGetTableSchema.mockClear();
    invalidateSchemaCache('conn-1');
    await getCachedTableSchema('conn-1', 'orders');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(1);
  });
});
