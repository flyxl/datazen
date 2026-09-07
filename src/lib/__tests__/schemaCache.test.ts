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
  subscribeSchemaInvalidation,
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

  it('keeps database-scoped cache keys separate', async () => {
    const appUsers: TableSchema = {
      tableName: 'users',
      columns: [{ name: 'id', dataType: 'int', nullable: false }],
      primaryKeys: ['id'],
      indexes: [],
      foreignKeys: [],
    };
    const analyticsUsers: TableSchema = {
      tableName: 'users',
      columns: [{ name: 'uid', dataType: 'bigint', nullable: false }],
      primaryKeys: ['uid'],
      indexes: [],
      foreignKeys: [],
    };
    mockGetTableSchema.mockResolvedValueOnce(appUsers).mockResolvedValueOnce(analyticsUsers);

    const fromApp = await getCachedTableSchema('conn-1', 'users', 'app');
    const fromAnalytics = await getCachedTableSchema('conn-1', 'users', 'analytics');
    expect(fromApp.columns[0]?.name).toBe('id');
    expect(fromAnalytics.columns[0]?.name).toBe('uid');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
    expect(mockGetTableSchema).toHaveBeenNthCalledWith(1, 'conn-1', 'users', 'app');
    expect(mockGetTableSchema).toHaveBeenNthCalledWith(2, 'conn-1', 'users', 'analytics');

    mockGetTableSchema.mockClear();
    await getCachedTableSchema('conn-1', 'users', 'app');
    await getCachedTableSchema('conn-1', 'users', 'analytics');
    expect(mockGetTableSchema).not.toHaveBeenCalled();
  });

  it('invalidates database-scoped table keys by table name suffix', async () => {
    mockGetTableSchema.mockResolvedValue(schema);
    await getCachedTableSchema('conn-1', 'users', 'app');
    await getCachedTableSchema('conn-1', 'users', 'analytics');
    await getCachedTableSchema('conn-1', 'orders', 'app');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(3);

    invalidateSchemaCache('conn-1', 'users');
    await getCachedTableSchema('conn-1', 'users', 'app');
    await getCachedTableSchema('conn-1', 'users', 'analytics');
    await getCachedTableSchema('conn-1', 'orders', 'app');
    // Both database-scoped `users` entries refetch; `orders` stays cached.
    expect(mockGetTableSchema).toHaveBeenCalledTimes(5);
  });
});

describe('schemaCache inflight / error / immutable', () => {
  const fullSchema: TableSchema = {
    tableName: 'users',
    columns: [{ name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true }],
    primaryKeys: ['id'],
    indexes: [],
    foreignKeys: [],
  };

  beforeEach(() => {
    vi.clearAllMocks();
    invalidateSchemaCache('conn-1');
    invalidateSchemaCache('conn-2');
  });

  it('dedupes concurrent schema requests to a single IPC call', async () => {
    let resolveFn!: (value: TableSchema) => void;
    mockGetTableSchema.mockReturnValue(new Promise((r) => (resolveFn = r)));
    const p1 = getCachedTableSchema('conn-1', 'users');
    const p2 = getCachedTableSchema('conn-1', 'users');
    resolveFn(fullSchema);
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(mockGetTableSchema).toHaveBeenCalledTimes(1);
    expect(r1).toBe(r2);
    expect(Object.isFrozen(r1)).toBe(true);
  });

  it('returns an immutable (deep-frozen) schema snapshot', async () => {
    mockGetTableSchema.mockResolvedValue(fullSchema);
    const r = await getCachedTableSchema('conn-1', 'users');
    expect(Object.isFrozen(r)).toBe(true);
    expect(Object.isFrozen(r.columns)).toBe(true);
  });

  it('caches a failed schema load for the error TTL and retries after', async () => {
    vi.useFakeTimers();
    mockGetTableSchema.mockRejectedValueOnce(new Error('boom'));
    await expect(getCachedTableSchema('conn-1', 'users')).rejects.toThrow('boom');
    // Within TTL → same error, no IPC re-request.
    await expect(getCachedTableSchema('conn-1', 'users')).rejects.toThrow('boom');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(61_000);
    mockGetTableSchema.mockResolvedValue(fullSchema);
    const r = await getCachedTableSchema('conn-1', 'users');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
    expect(r.tableName).toBe('users');
    vi.useRealTimers();
  });

  it('caches DDL by object kind and full namespace', async () => {
    mockExecuteQuery.mockResolvedValue({ results: [{ rows: [['T']] }] });
    const extractor = (rows: unknown[][]) => String(rows[0]?.[0] ?? '');

    await getCachedDDL('conn-1', 'users', 'sql-a', extractor, {
      objectKind: 'table',
      namespacePath: ['public'],
    });
    await getCachedDDL('conn-1', 'users', 'sql-b', extractor, {
      objectKind: 'view',
      namespacePath: ['public'],
    });
    await getCachedDDL('conn-1', 'users', 'sql-c', extractor, {
      objectKind: 'table',
      namespacePath: ['audit'],
    });
    expect(mockExecuteQuery).toHaveBeenCalledTimes(3);

    // Identical identity reuses the cache cell.
    await getCachedDDL('conn-1', 'users', 'sql-a', extractor, {
      objectKind: 'table',
      namespacePath: ['public'],
    });
    expect(mockExecuteQuery).toHaveBeenCalledTimes(3);
  });

  it('dedupes concurrent DDL requests to a single IPC call', async () => {
    mockExecuteQuery.mockResolvedValue({ results: [{ rows: [['DDL']] }] });
    const extractor = (rows: unknown[][]) => String(rows[0]?.[0] ?? '');
    const p1 = getCachedDDL('conn-1', 'users', 'sql', extractor);
    const p2 = getCachedDDL('conn-1', 'users', 'sql', extractor);
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(mockExecuteQuery).toHaveBeenCalledTimes(1);
    expect(r1).toBe('DDL');
    expect(r2).toBe('DDL');
  });

  it('caches a failed DDL load for the error TTL', async () => {
    mockExecuteQuery.mockRejectedValueOnce(new Error('ddlboom'));
    const extractor = (rows: unknown[][]) => String(rows[0]?.[0] ?? '');
    await expect(getCachedDDL('conn-1', 'users', 'sql', extractor)).rejects.toThrow('ddlboom');
    await expect(getCachedDDL('conn-1', 'users', 'sql', extractor)).rejects.toThrow('ddlboom');
    expect(mockExecuteQuery).toHaveBeenCalledTimes(1);
  });

  it('notifies invalidation subscribers and supports unsubscribe', () => {
    const listener = vi.fn();
    const unsubscribe = subscribeSchemaInvalidation(listener);
    invalidateSchemaCache('conn-1', 'users');
    invalidateSchemaCache('conn-1');
    expect(listener).toHaveBeenCalledWith('conn-1', 'users');
    expect(listener).toHaveBeenCalledWith('conn-1', undefined);
    unsubscribe();
    invalidateSchemaCache('conn-1');
    expect(listener).toHaveBeenCalledTimes(2);
  });
});
