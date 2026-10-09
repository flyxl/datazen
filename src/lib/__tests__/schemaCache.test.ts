import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableSchema } from '../../types';

const mockGetTableSchema = vi.fn();
const mockExecuteQuery = vi.fn();

vi.mock('@datazen/driver-sdk', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/driver-sdk')>()),
  schemaClient: {
    readSchema: async (session: string, ref: import('@datazen/driver-sdk').RelationRef) => ({
      value: {
        ref,
        definition: await mockGetTableSchema(session, ref.name, ref.database, ref.schema),
      },
    }),
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
  tableName: 'users',
  columns: [],
  primaryKeys: [],
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
    const r1 = await getCachedTableSchema('conn-1', 'users', 'app');
    const r2 = await getCachedTableSchema('conn-1', 'users', 'app');
    expect(r1).toBe(schema);
    expect(r2).toBe(schema);
    expect(mockGetTableSchema).toHaveBeenCalledTimes(1);
    expect(mockGetTableSchema).toHaveBeenCalledWith('conn-1', 'users', 'app', null);
  });

  it('refetches after TTL expires', async () => {
    vi.useFakeTimers();
    mockGetTableSchema.mockResolvedValue(schema);
    await getCachedTableSchema('conn-1', 'users', 'app');
    vi.advanceTimersByTime(61_000);
    await getCachedTableSchema('conn-1', 'users', 'app');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
  });

  it('fetches and caches DDL via query + extractor', async () => {
    mockExecuteQuery.mockResolvedValue({
      results: [{ rows: [['CREATE TABLE users (...);']] }],
    });
    const extractor = vi.fn((rows: unknown[][]) => String(rows[0]?.[0] ?? ''));
    const ddl = await getCachedDDL('conn-1', 'users', 'SHOW CREATE TABLE users', extractor, 'app');
    expect(ddl).toBe('CREATE TABLE users (...);');
    expect(mockExecuteQuery).toHaveBeenCalledWith(
      'conn-1',
      'SHOW CREATE TABLE users',
      undefined,
      'app',
      null,
    );
    expect(extractor).toHaveBeenCalledWith([['CREATE TABLE users (...);']]);

    mockExecuteQuery.mockClear();
    await getCachedDDL('conn-1', 'users', 'SHOW CREATE TABLE users', extractor, 'app');
    expect(mockExecuteQuery).not.toHaveBeenCalled();
  });

  it('handles empty query result in DDL extractor', async () => {
    mockExecuteQuery.mockResolvedValue({ results: [{ rows: [] }] });
    const extractor = vi.fn(() => '(empty)');
    const ddl = await getCachedDDL('conn-1', 'users', 'SELECT 1', extractor, 'app');
    expect(ddl).toBe('(empty)');
    expect(extractor).toHaveBeenCalledWith([]);
  });

  it('invalidates single table or whole connection', async () => {
    mockGetTableSchema.mockResolvedValue(schema);
    await getCachedTableSchema('conn-1', 'users', 'app');
    await getCachedTableSchema('conn-1', 'orders', 'app');
    await getCachedTableSchema('conn-2', 'users', 'app');

    invalidateSchemaCache('conn-1', 'users');
    await getCachedTableSchema('conn-1', 'users', 'app');
    await getCachedTableSchema('conn-1', 'orders', 'app');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(4); // 3 initial + 1 refetch users

    mockGetTableSchema.mockClear();
    invalidateSchemaCache('conn-1');
    await getCachedTableSchema('conn-1', 'orders', 'app');
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
    expect(mockGetTableSchema).toHaveBeenNthCalledWith(1, 'conn-1', 'users', 'app', null);
    expect(mockGetTableSchema).toHaveBeenNthCalledWith(2, 'conn-1', 'users', 'analytics', null);

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
    const p1 = getCachedTableSchema('conn-1', 'users', 'app');
    const p2 = getCachedTableSchema('conn-1', 'users', 'app');
    resolveFn(fullSchema);
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(mockGetTableSchema).toHaveBeenCalledTimes(1);
    expect(r1).toBe(r2);
    expect(Object.isFrozen(r1)).toBe(true);
  });

  it('returns an immutable (deep-frozen) schema snapshot', async () => {
    mockGetTableSchema.mockResolvedValue(fullSchema);
    const r = await getCachedTableSchema('conn-1', 'users', 'app');
    expect(Object.isFrozen(r)).toBe(true);
    expect(Object.isFrozen(r.columns)).toBe(true);
  });

  it('caches a failed schema load for the error TTL and retries after', async () => {
    vi.useFakeTimers();
    mockGetTableSchema.mockRejectedValueOnce(new Error('boom'));
    await expect(getCachedTableSchema('conn-1', 'users', 'app')).rejects.toThrow('boom');
    // Within TTL → same error, no IPC re-request.
    await expect(getCachedTableSchema('conn-1', 'users', 'app')).rejects.toThrow('boom');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(61_000);
    mockGetTableSchema.mockResolvedValue(fullSchema);
    const r = await getCachedTableSchema('conn-1', 'users', 'app');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
    expect(r.tableName).toBe('users');
    vi.useRealTimers();
  });

  it('caches DDL by object kind and full namespace', async () => {
    mockExecuteQuery.mockResolvedValue({ results: [{ rows: [['T']] }] });
    const extractor = (rows: unknown[][]) => String(rows[0]?.[0] ?? '');

    await getCachedDDL('conn-1', 'users', 'sql-a', extractor, 'app', {
      objectKind: 'table',
      namespacePath: ['public'],
    });
    await getCachedDDL('conn-1', 'users', 'sql-b', extractor, 'app', {
      objectKind: 'view',
      namespacePath: ['public'],
    });
    await getCachedDDL('conn-1', 'users', 'sql-c', extractor, 'app', {
      objectKind: 'table',
      namespacePath: ['audit'],
    });
    expect(mockExecuteQuery).toHaveBeenCalledTimes(3);

    // Identical identity reuses the cache cell.
    await getCachedDDL('conn-1', 'users', 'sql-a', extractor, 'app', {
      objectKind: 'table',
      namespacePath: ['public'],
    });
    expect(mockExecuteQuery).toHaveBeenCalledTimes(3);
  });

  it('keeps DDL cache cells separate per database', async () => {
    mockExecuteQuery.mockResolvedValue({ results: [{ rows: [['T']] }] });
    const extractor = (rows: unknown[][]) => String(rows[0]?.[0] ?? '');
    await getCachedDDL('conn-1', 'users', 'sql', extractor, 'app');
    await getCachedDDL('conn-1', 'users', 'sql', extractor, 'analytics');
    expect(mockExecuteQuery).toHaveBeenCalledTimes(2);
    expect(mockExecuteQuery).toHaveBeenNthCalledWith(1, 'conn-1', 'sql', undefined, 'app', null);
    expect(mockExecuteQuery).toHaveBeenNthCalledWith(
      2,
      'conn-1',
      'sql',
      undefined,
      'analytics',
      null,
    );
  });

  it('dedupes concurrent DDL requests to a single IPC call', async () => {
    mockExecuteQuery.mockResolvedValue({ results: [{ rows: [['DDL']] }] });
    const extractor = (rows: unknown[][]) => String(rows[0]?.[0] ?? '');
    const p1 = getCachedDDL('conn-1', 'users', 'sql', extractor, 'app');
    const p2 = getCachedDDL('conn-1', 'users', 'sql', extractor, 'app');
    const [r1, r2] = await Promise.all([p1, p2]);
    expect(mockExecuteQuery).toHaveBeenCalledTimes(1);
    expect(r1).toBe('DDL');
    expect(r2).toBe('DDL');
  });

  it('caches a failed DDL load for the error TTL', async () => {
    mockExecuteQuery.mockRejectedValueOnce(new Error('ddlboom'));
    const extractor = (rows: unknown[][]) => String(rows[0]?.[0] ?? '');
    await expect(getCachedDDL('conn-1', 'users', 'sql', extractor, 'app')).rejects.toThrow(
      'ddlboom',
    );
    await expect(getCachedDDL('conn-1', 'users', 'sql', extractor, 'app')).rejects.toThrow(
      'ddlboom',
    );
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
  it('refresh detaches an old request without deleting a newer in-flight read', async () => {
    let resolveOld!: (value: TableSchema) => void;
    let resolveNew!: (value: TableSchema) => void;
    mockGetTableSchema
      .mockReturnValueOnce(
        new Promise<TableSchema>((r) => {
          resolveOld = r;
        }),
      )
      .mockReturnValueOnce(
        new Promise<TableSchema>((r) => {
          resolveNew = r;
        }),
      );
    const old = getCachedTableSchema('conn-1', 'users', 'app', 'public');
    invalidateSchemaCache('conn-1', 'users');
    const fresh = getCachedTableSchema('conn-1', 'users', 'app', 'public');
    resolveOld({ ...fullSchema, columns: [{ name: 'old', dataType: 'int', nullable: false }] });
    await old;
    const sameFresh = getCachedTableSchema('conn-1', 'users', 'app', 'public');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
    resolveNew(fullSchema);
    await Promise.all([fresh, sameFresh]);
    expect(await getCachedTableSchema('conn-1', 'users', 'app', 'public')).toBe(fullSchema);
  });

  it('an invalidated failure cannot suppress a later successful read', async () => {
    let rejectOld!: (error: Error) => void;
    mockGetTableSchema
      .mockReturnValueOnce(
        new Promise<TableSchema>((_, reject) => {
          rejectOld = reject;
        }),
      )
      .mockResolvedValueOnce(fullSchema);
    const old = getCachedTableSchema('conn-1', 'users', 'app');
    const failed = expect(old).rejects.toThrow('stale failure');
    invalidateSchemaCache('conn-1');
    rejectOld(new Error('stale failure'));
    await failed;
    expect(await getCachedTableSchema('conn-1', 'users', 'app')).toBe(fullSchema);
  });

  it('database-scoped invalidation preserves the same relation in other databases', async () => {
    mockGetTableSchema.mockResolvedValue(fullSchema);
    await getCachedTableSchema('conn-1', 'users', 'app', 'public');
    await getCachedTableSchema('conn-1', 'users', 'other', 'public');
    invalidateSchemaCache('conn-1', 'users', undefined, 'app');
    await getCachedTableSchema('conn-1', 'users', 'other', 'public');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
    await getCachedTableSchema('conn-1', 'users', 'app', 'public');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(3);
  });

  it('tuple keys distinguish delimiter-containing catalog names', async () => {
    mockGetTableSchema.mockResolvedValue(fullSchema);
    await getCachedTableSchema('conn-1', 'users', 'a::b', 'c');
    await getCachedTableSchema('conn-1', 'users', 'a', 'b::c');
    expect(mockGetTableSchema).toHaveBeenCalledTimes(2);
  });
});
